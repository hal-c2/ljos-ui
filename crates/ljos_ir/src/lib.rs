//! The compiled form of one QML file (ARCHITECTURE.md §5.3).
//!
//! `ljos_compiler` writes it and the runtime reads it; nothing here knows how
//! a file was parsed, so a shipped app links this crate and never a parser
//! (§4). Everything is owned and arena-free, and the whole of it serialises
//! with serde, so a build can cache it with postcard and a dev host can send
//! it over a wire.
//!
//! The shape, top down:
//!
//! - [`CompiledFile`]: one per `.qml` file. Its components, the expressions
//!   of all of them, and one JavaScript module with a function per expression.
//! - [`ComponentDef`]: the file's own component, each inline `component`, and
//!   each template written inline (a `delegate`, the content of a
//!   `Component { }`). Its objects are a flat list, the root first.
//! - [`ObjectDef`]: one object as written: its type, what it declares and
//!   what it sets. A value is a [`Literal`] when it is the same every time it
//!   is read, and an [`ExprId`] into [`CompiledFile::exprs`] otherwise.
//! - [`Binding`]: one binding, handler or function: the function in the module
//!   that evaluates it, the names it uses and what each name is
//!   ([`Resolved`]), and how the engine fills the function's parameters
//!   ([`Arg`]).

use serde::{Deserialize, Serialize};

pub mod color;

/// Bumped whenever the serialised shape changes, or what the compiled
/// functions ask of the engine, so a cache written by an older compiler is
/// rebuilt instead of misread.
pub const IR_VERSION: u32 = 10;

macro_rules! index {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub u32);

        impl $name {
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

index!(
    /// A component of the file: an index into [`CompiledFile::components`].
    CompIdx
);
index!(
    /// An object of a component: an index into [`ComponentDef::objects`].
    ObjIdx
);
index!(
    /// An expression of the file: an index into [`CompiledFile::exprs`].
    ExprId
);
index!(
    /// A function of the file's module: an index into [`JsModule::functions`].
    FnIndex
);

/// One QML file, compiled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledFile {
    pub version: u32,
    /// The file's key in its project: its path from the project root without
    /// `.qml`, with forward slashes.
    pub file: String,
    /// blake3 of the source text: what a cache is keyed by.
    pub source_hash: [u8; 32],
    pub imports: Vec<Import>,
    pub pragmas: Vec<Pragma>,
    /// `[0]` is the file's own component.
    pub components: Vec<ComponentDef>,
    pub exprs: Vec<Binding>,
    /// Every string the file marks for translation, for the catalogue
    /// (ARCHITECTURE.md §15), whether or not it is a static value.
    pub translations: Vec<Translatable>,
    pub js: JsModule,
    /// What the file's programs take of other files to be so.
    pub invocable: Vec<Invocable>,
    /// The script files the file imports by path (`import "calc.js" as Calc`).
    pub scripts: Vec<Script>,
}

/// A JavaScript file a QML file imports by path, ready to run: the engine
/// has no parser, so what it needs of the file is worked out here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    /// As the import writes it, which is what an [`Arg::Namespace`] names;
    /// empty for a script only other scripts import.
    pub path: String,
    /// Where the file is from the project's root: two imports of one file
    /// name the same script.
    pub key: String,
    /// `.pragma library`: one copy for every importer. Without it each
    /// instance of the importing component has its own.
    pub library: bool,
    /// An expression that makes the script's object when evaluated: its
    /// top-level names as members, read and written through to the
    /// script's own variables. It is a function of the script's address
    /// and of the objects of [`Script::imports`], in that order.
    pub source: String,
    /// The scripts this one imports itself (`.import "other.js" as Other`),
    /// each among the file's [`CompiledFile::scripts`].
    pub imports: Vec<ScriptImport>,
}

/// One `.import "path" as Name` line of a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptImport {
    /// As the line writes it: from the importing script's directory.
    pub path: String,
    /// The imported script's [`Script::key`].
    pub key: String,
}

/// The functions of a singleton another file is, each with a tier-1 program
/// of its own that writes nothing when this file was compiled: what a
/// program of this file may call in place ([`Op::Invoke`]). The engine looks
/// again when it runs one, as the other file may have changed since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocable {
    pub ty: TypeRef,
    pub functions: Vec<String>,
}

impl CompiledFile {
    pub fn component(&self, index: CompIdx) -> &ComponentDef {
        &self.components[index.index()]
    }

    pub fn expr(&self, id: ExprId) -> &Binding {
        &self.exprs[id.index()]
    }

    pub fn object(&self, at: ObjRef) -> &ObjectDef {
        &self.components[at.component.index()].objects[at.object.index()]
    }

    /// The stable key of an object (ARCHITECTURE.md §12): the path of
    /// segments from its component's root, the root's being empty.
    pub fn object_key(&self, at: ObjRef) -> ObjectKey {
        let component = self.component(at.component);
        let mut segments = Vec::new();
        let mut object = Some(at.object);
        while let Some(index) = object {
            let def = &component.objects[index.index()];
            if def.parent.is_some() {
                segments.push(def.key.clone());
            }
            object = def.parent;
        }
        segments.reverse();
        ObjectKey {
            component: component.id.clone(),
            segments,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(postcard::to_allocvec(self)?)
    }

    /// Reads what [`CompiledFile::to_bytes`] wrote, refusing another IR
    /// version rather than misreading it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let file: Self = postcard::from_bytes(bytes)?;
        if file.version != IR_VERSION {
            return Err(Error::Version {
                found: file.version,
                expected: IR_VERSION,
            });
        }
        Ok(file)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the IR does not serialise: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("the IR is version {found}, this build reads {expected}")]
    Version { found: u32, expected: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub source: ImportSource,
    pub version: Option<String>,
    pub alias: Option<String>,
    /// The module is the project's own: its types are files compiled with
    /// this one, and the runtime has no module of the name to look for.
    pub in_project: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImportSource {
    /// `import QtQuick.Controls`.
    Module(String),
    /// `import "content"`, `import "logic.js" as Logic`.
    Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pragma {
    pub name: String,
    pub value: Option<String>,
}

/// One component: something QML makes a context for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    /// The file's stem, the inline component's name, or for a template the
    /// type of its root.
    pub name: String,
    /// Stable across edits that do not move the component: the file key,
    /// `file#Inline`, or for a template the key of the object it is written
    /// in and the property it is the value of. [`ObjectKey`]s start with it.
    pub id: String,
    pub kind: ComponentKind,
    /// `[0]` is the root. An object comes before the objects inside it.
    pub objects: Vec<ObjectDef>,
    pub signature: ComponentSignature,
}

impl ComponentDef {
    pub fn root(&self) -> &ObjectDef {
        &self.objects[0]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    /// The file's own component. It sees nothing around it.
    File,
    /// `component Name: Type { }`. It sees nothing around it either.
    Inline,
    /// Written where a template goes: a `delegate`, the content of a
    /// `Component { }`. It sees the names of `outer`, and when `is_delegate`
    /// what a view gives each of its delegates (`index`, `modelData`, roles).
    Template { outer: CompIdx, is_delegate: bool },
}

/// What a change to a component must keep live instances from surviving
/// (ARCHITECTURE.md §12): when it changes, an instance is remade; when it
/// does not, the instance keeps its state and takes the new bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentSignature(pub [u8; 32]);

impl ComponentSignature {
    /// Hashes the root's declared properties (names and types), its signals,
    /// and the names of the file's inline components. The order they are
    /// written in does not matter.
    pub fn of(root: &ObjectDef, inline: &[String]) -> Self {
        let mut properties: Vec<String> = root
            .properties
            .iter()
            .map(|p| {
                let list = if p.is_list { "list " } else { "" };
                format!("{}:{list}{}", p.name, p.ty)
            })
            .chain(root.required.iter().map(|name| format!("{name}:required")))
            .collect();
        properties.sort();
        let mut signals: Vec<String> = root
            .signals
            .iter()
            .map(|s| {
                let params: Vec<String> = s
                    .params
                    .iter()
                    .map(|p| format!("{}:{}", p.name, p.ty.as_deref().unwrap_or("var")))
                    .collect();
                format!("{}({})", s.name, params.join(","))
            })
            .collect();
        signals.sort();
        let mut inline = inline.to_vec();
        inline.sort();
        let mut hasher = blake3::Hasher::new();
        // Each part is length-prefixed, so names cannot run into each other.
        for (tag, items) in [("p", &properties), ("s", &signals), ("i", &inline)] {
            hasher.update(tag.as_bytes());
            hasher.update(&(items.len() as u64).to_le_bytes());
            for item in items.iter() {
                hasher.update(&(item.len() as u64).to_le_bytes());
                hasher.update(item.as_bytes());
            }
        }
        Self(*hasher.finalize().as_bytes())
    }
}

/// An object of a component, by the component and its index there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjRef {
    pub component: CompIdx,
    pub object: ObjIdx,
}

/// Where an object is, stable across edits that do not move it
/// (ARCHITECTURE.md §12).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectKey {
    pub component: String,
    pub segments: Vec<KeySeg>,
}

/// One step of an [`ObjectKey`], the most stable the object allows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeySeg {
    /// The object's `id`.
    Id(String),
    /// Its literal `objectName`.
    Named(String),
    /// The row of the model a delegate instance is made for. Only the
    /// running program makes these.
    ModelRow(u64),
    /// The `nth` object of type `ty`, among its siblings with neither an id
    /// nor a name.
    Typed { ty: String, nth: u32 },
}

/// A type, as the runtime is to find it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeRef {
    /// A type the runtime provides, by the module that has it.
    Native { module: String, name: String },
    /// A QML file of the project, or an inline component of one.
    Component {
        file: String,
        inline: Option<String>,
    },
    /// A name the compiler could not find, as written. Reported.
    Unresolved(String),
}

impl Default for TypeRef {
    fn default() -> Self {
        TypeRef::Unresolved(String::new())
    }
}

/// Where in the file something is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Byte offsets.
    pub start: u32,
    pub end: u32,
    /// 1-based line and column (in characters) of `start`.
    pub line: u32,
    pub column: u32,
}

/// One object as written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectDef {
    pub ty: TypeRef,
    /// The object's step of its [`ObjectKey`].
    pub key: KeySeg,
    /// The object it is written in, in the same component. None for a root.
    pub parent: Option<ObjIdx>,
    pub id: Option<String>,
    /// `Behavior on x`, `NumberAnimation on opacity`: the property path the
    /// object is a value source or interceptor of.
    pub on: Option<Vec<String>>,
    pub span: SourceSpan,
    pub properties: Vec<PropertyDecl>,
    /// `required name`: a property it inherits that whoever makes it must set.
    pub required: Vec<String>,
    pub signals: Vec<SignalDecl>,
    pub functions: Vec<FunctionDecl>,
    pub enums: Vec<EnumDecl>,
    /// What it sets on itself.
    pub members: Members,
    /// What it sets on its groups: `anchors.left`, `font { pixelSize: 12 }`,
    /// and in a PropertyChanges `rect.width`.
    pub grouped: Vec<Group>,
    /// What it sets on what a type attaches to it: `Keys.onPressed`,
    /// `Layout.fillWidth`, `Component.onCompleted`.
    pub attached: Vec<Attached>,
    /// The objects written in it that go to its default property, and the
    /// value sources and interceptors written on it.
    pub children: Vec<ObjectValue>,
}

impl Default for KeySeg {
    fn default() -> Self {
        KeySeg::Typed {
            ty: String::new(),
            nth: 0,
        }
    }
}

/// `property <type> name`, with whatever modifiers. Its initial value, if
/// any, is in [`ObjectDef::members`] under the same name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyDecl {
    pub name: String,
    /// As written: `int`, `color`, `Item`, `alias`. For `list<Item>`, `Item`.
    pub ty: String,
    pub is_list: bool,
    pub is_default: bool,
    pub is_readonly: bool,
    pub is_required: bool,
    pub alias: Option<AliasTarget>,
    pub span: SourceSpan,
}

/// `property alias name: id.path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasTarget {
    pub id: String,
    /// The object the id names, when it names one in sight.
    pub object: Option<ObjRef>,
    /// Empty for `property alias name: id`.
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDecl {
    pub name: String,
    pub params: Vec<SignalParam>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalParam {
    pub name: String,
    pub ty: Option<String>,
}

/// `function name(...) { }` on an object: a method, `this` being the object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub name: String,
    pub expr: ExprId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDecl {
    pub name: String,
    pub keys: Vec<(String, i64)>,
}

/// What an object, one of its groups, or what is attached to it is given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Members {
    pub assignments: Vec<Assignment>,
    pub bindings: Vec<PropertyBinding>,
    pub handlers: Vec<Handler>,
    pub objects: Vec<ObjectAssignment>,
}

impl Members {
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
            && self.bindings.is_empty()
            && self.handlers.is_empty()
            && self.objects.is_empty()
    }
}

/// A property given a value that never changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub name: String,
    pub value: Literal,
    pub span: SourceSpan,
}

/// A property given an expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyBinding {
    pub name: String,
    pub expr: ExprId,
    /// Where the name is written: `border` of `border.width: ...`, `width`
    /// inside `border { width: ... }`, and the whole declaration for a
    /// value given with `property int p: ...`. Qt's binding-loop message
    /// names this place, not the expression.
    pub name_span: SourceSpan,
}

/// `onClicked: ...`: run when the object emits `signal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handler {
    /// `clicked` for `onClicked`.
    pub signal: String,
    pub expr: ExprId,
}

/// A property given objects: `delegate: Rectangle { }`, `states: [ ... ]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectAssignment {
    pub name: String,
    pub values: Vec<ObjectValue>,
    /// Written as `[ ... ]`.
    pub is_list: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectValue {
    /// An object of the same component.
    Object(ObjIdx),
    /// A template: a component of its own, made later.
    Component(CompIdx),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// `["anchors"]`, `["font"]`; in a PropertyChanges `["rect"]`.
    pub path: Vec<String>,
    pub members: Members,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attached {
    /// The type that attaches: `Keys`, `Layout`, `Component`.
    pub ty: TypeRef,
    /// The type as written, `QQC.SplitView`.
    pub written: String,
    /// Names are the attached object's, dotted for its groups.
    pub members: Members,
}

/// A value that is the same every time it is read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// A string given to a property of type `color`, as written.
    Color(String),
    /// `Text.AlignHCenter`, `Image.Fill.Stretch`, `Qt.AlignLeft`.
    Enum(EnumRef),
    /// `qsTr("Open")`: translated when read, again when the language changes.
    Translatable(Translatable),
    List(Vec<Literal>),
    /// An object literal whose every value is static.
    Object(Vec<(String, Literal)>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnumRef {
    pub ty: TypeRef,
    /// The enum, when written: `Fill` of `Image.Fill.Stretch`.
    pub scope: Option<String>,
    pub key: String,
    /// None when the compiler does not know the number: Qt's type table has
    /// none, so the runtime's registry decides.
    pub value: Option<i64>,
}

/// One translatable string: its catalogue key (ARCHITECTURE.md §15).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Translatable {
    pub kind: TranslationKind,
    /// `qsTr`: the file's stem, or `pragma Translator`. `qsTranslate`: its
    /// first argument. Empty for `qsTrId`.
    pub context: String,
    /// The source text, or for `qsTrId` the id.
    pub source: String,
    /// The disambiguation.
    pub comment: Option<String>,
    /// `n` of a plural form, when it is a literal.
    pub plural: Option<i64>,
    /// Whether the call has an `n` at all (it may be an expression).
    pub is_plural: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TranslationKind {
    /// `qsTr`, `qsTranslate`: translated.
    Text,
    /// `qsTrId`: translated by id.
    Id,
    /// `QT_TR_NOOP` and friends: only marked for the catalogue; the value is
    /// the source text.
    NoOp,
}

/// The file's JavaScript: one ES module with a function per expression
/// (ARCHITECTURE.md §5.4, tier 2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsModule {
    pub source: String,
    pub functions: Vec<JsFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsFunction {
    /// The name the module exports it by.
    pub export: String,
    pub expr: ExprId,
}

/// One expression: a binding, a handler or a method.
///
/// Its function is called with `this` the scope object, then [`Binding::params`]
/// in order, then [`Binding::args`] (a signal's or a method's arguments).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub kind: BindingKind,
    /// The object the expression's names are looked up from first.
    pub scope: ObjRef,
    pub js_fn: FnIndex,
    pub params: Vec<Param>,
    /// The names the function takes after its params. Empty when the
    /// signal's signature is unknown: then the function takes the arguments
    /// as they come and the body sees them as `arguments`.
    pub args: Vec<String>,
    /// Every free name the expression uses, once, with what it is.
    pub refs: Vec<Ref>,
    pub span: SourceSpan,
    /// Of the expression's text: two expressions with the same hash and the
    /// same refs are the same.
    pub hash: u64,
    /// The register bytecode of tier 1, when the whole expression is in
    /// the subset tier 1 runs; the JS function means the same.
    pub tier1: Option<Tier1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    /// A property's value: read, never assigned.
    Property,
    /// Run when a signal is emitted.
    Handler,
    /// A method of the object.
    Function,
}

mod tier1;
pub use tier1::{BinaryOp, Builtin, Const, InlineArg, Op, Reg, Tier1, UnaryOp};

/// A parameter of an expression's function, and what the engine passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// As it appears in the function: the name itself for an id or a type,
    /// `$scope`, `$root` or `$c<component>o<object>` for an object whose
    /// properties the body reads through it.
    pub name: String,
    pub arg: Arg,
}

/// What the engine passes for a parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arg {
    /// The instance of that object in the scope chain of the expression's
    /// own instance.
    Object(ObjRef),
    /// The type, as an object: `Text.AlignLeft` reads its enums, and
    /// `ListView.isCurrentItem` what it attaches to `this`.
    Type(TypeRef),
    /// The one object a singleton type is.
    Singleton(TypeRef),
    /// What the type attaches to `object`, made if it was not there:
    /// `Layout` of `Layout.row` written in that object.
    Attached { ty: TypeRef, object: ObjRef },
    /// What the file imports as `alias`.
    Namespace { alias: String, source: ImportSource },
    /// What the context of `component` is given (`index`, `modelData`, a
    /// role), or with no component, what the context the instance is made in
    /// has at run time. When it has nothing of the name, `fallback`, and
    /// without one the global of the name or `undefined`.
    Context {
        component: Option<CompIdx>,
        name: String,
        fallback: Option<Box<Arg>>,
    },
    /// The object that has `name`, for a script that assigns to it: the
    /// first context object from the context of `component` outwards (from
    /// the instance's own without one) that has a property so named, else
    /// `fallback`. The script reads and writes the name as a member of
    /// what it is given, so an assignment reaches the object. When nothing
    /// has the name the engine gives an object with only that member, set
    /// to what [`Arg::Context`] would have been.
    Holder {
        component: Option<CompIdx>,
        name: String,
        fallback: Option<ObjRef>,
    },
    /// The value of a property of an object: only ever a fallback of
    /// [`Arg::Context`], since a property the body uses directly is read
    /// through its object.
    Property { object: ObjRef, name: String },
    /// A name the compiler could not resolve: the engine looks it up at run
    /// time. Where nothing has the name it passes what `__ljos_named` knows
    /// for nothing, and the function throws a ReferenceError where it reads
    /// the name.
    Dynamic(String),
}

/// A free name an expression uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    pub name: String,
    pub resolved: Resolved,
}

/// What a free name is (tier 0, ARCHITECTURE.md §5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolved {
    /// A parameter of the handler's signal or of the method.
    Local,
    /// An object of the same component, by its id.
    Id(ObjRef),
    /// An object of a component around this one, by its id.
    OuterId(ObjRef),
    /// A member of the scope object.
    ScopeProp,
    /// A member of the component's root.
    RootProp,
    /// A member of another object: the root of a component around, or the
    /// target a PropertyChanges names.
    OuterProp(ObjRef),
    /// `ListView.isCurrentItem`: a member of what a type attaches. With an
    /// empty member, the name alone is what it attaches (`target: Component`).
    Attached {
        ty: TypeRef,
        member: String,
    },
    /// `Text.AlignHCenter`.
    Enum(EnumRef),
    Singleton(TypeRef),
    /// A type named for anything else.
    Type(TypeRef),
    /// What the file imports `as` the name.
    ImportNs(ImportSource),
    /// What QML or JavaScript puts in every scope: `Math`, `qsTr`, `Qt`.
    Global {
        is_qml: bool,
    },
    /// What a context is given at run time: with a component, what a view
    /// gives its delegate; without one, a name only the context an instance
    /// is made in has. `fallback` is what the name is when the context has
    /// nothing of it.
    ContextProp {
        component: Option<CompIdx>,
        fallback: Option<Box<Resolved>>,
    },
    /// Nothing in sight. Reported, and looked up at run time.
    Unknown,
}

impl Resolved {
    /// Whether the compiler knows what the name is.
    pub fn is_known(&self) -> bool {
        !matches!(self, Resolved::Unknown)
    }
}

mod reload;
pub use reload::{FilePatch, ReloadOp, ReloadPatch};
