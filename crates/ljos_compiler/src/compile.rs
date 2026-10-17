//! One file, from its QML tree to [`CompiledFile`].
//!
//! The walk follows the front end's [`Tree`]: every object of the tree is an
//! [`ObjectDef`] of the component (context) the tree puts it in, in the same
//! order. A value that is the same every time it is read becomes a
//! [`Literal`]; anything else becomes a function of the file's module, whose
//! parameters are what its free names resolve to (tier 0, then tier 2 of
//! ARCHITECTURE.md §5.4).

use std::collections::{HashMap, HashSet};

use ljos_ir::*;
use qml_front::{
    Special, TypeOracle,
    names::{self, Global, Names, Resolution, Target},
    project::Source,
    scope::{Tree, alias_target, handled, is_group},
    types::{Found, Kind, Lookup, Member, Origin, Types},
};
use qml_syntax::{
    Allocator, GetSpan, Span,
    ast::{
        QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject, QmlPropertyDeclaration,
    },
    js::{Expression, Function, ObjectPropertyKind, PropertyKind, UnaryOperator},
};

use crate::{
    Code, Compiled, Diagnostic, Project, Severity,
    js::{self, ArgValue, Form},
    oracle::{Held, Written, is_color},
};

/// Compiles the file `key` of the project.
///
/// A file with errors still compiles, as far as it can: what did not parse is
/// left out and what is unknown is marked so, and the errors come with the
/// result. Only a file the parser gave up on is an `Err`.
pub fn compile_file<O: TypeOracle>(
    project: &Project<O>,
    key: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let Some(source) = project.source(key) else {
        return Err(vec![Diagnostic::error(
            Code::NotInProject,
            format!("`{key}` is not a file of the project"),
            SourceSpan::default(),
        )]);
    };
    let lines = Lines::new(source);
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let mut diagnostics: Vec<Diagnostic> = parsed
        .diagnostics
        .iter()
        .map(|d| Diagnostic::error(Code::Syntax, d.message.clone(), lines.span(source, d.span)))
        .collect();
    if parsed.panicked {
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::error(
                Code::Syntax,
                "the file could not be parsed",
                SourceSpan::default(),
            ));
        }
        return Err(diagnostics);
    }
    let types = Types::new(&project.front, &project.oracle, key);
    let mut errors = Vec::new();
    let tree = Tree::analyze(&parsed.document, types, &mut errors);
    diagnostics.extend(
        errors
            .into_iter()
            .map(|e| Diagnostic::error(e.code.into(), e.message, lines.span(source, e.span))),
    );
    let mut compiler = Compiler::new(project, key, source, lines, types, &tree);
    compiler.diagnostics = diagnostics;
    if let Some(context) = translator(&parsed.document) {
        compiler.context = context.to_string();
    }
    compiler.collect_enums(&parsed.document.root, None);
    compiler.object(&parsed.document.root, None);
    Ok(compiler.finish(&parsed.document))
}

/// Byte offsets to lines and columns.
struct Lines {
    starts: Vec<u32>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter(|(_, b)| *b == b'\n')
                    .map(|(i, _)| i as u32 + 1),
            )
            .collect();
        Self { starts }
    }

    fn span(&self, source: &str, span: Span) -> SourceSpan {
        let start = (span.start as usize).min(source.len()) as u32;
        let line = self.starts.partition_point(|s| *s <= start);
        let line_start = self.starts[line - 1] as usize;
        let mut end = start as usize;
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        let column = source[line_start..end].chars().count() + 1;
        SourceSpan {
            start: span.start,
            end: span.end,
            line: line as u32,
            column: column as u32,
        }
    }
}

/// An enum a component of the file declares: its inline component (None
/// for the file's own), its name and its keys.
type FileEnum = (Option<String>, String, Vec<(String, i64)>);

/// Where a member's name puts it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Own,
    /// `anchors.left`: the group's path.
    Group(Vec<String>),
    /// `Keys.onPressed`: the type's parts, `["Keys"]` or `["C", "Layout"]`.
    Attached(Vec<String>),
}

/// What a free name becomes in a function.
#[derive(Debug, Clone)]
struct Plan {
    resolved: Resolved,
    param: Option<Param>,
    /// The name is read through this parameter: `width` is `$scope.width`.
    rewrite: Option<String>,
    /// The name, and this many of the members read off it, are this
    /// parameter: `Layout` of `Layout.row` and `L.Layout` of `L.Layout.row`
    /// are what the type attached to the scope object.
    replace: Option<(String, usize)>,
}

struct Compiler<'t, 'a, 'p, O: TypeOracle> {
    project: &'p Project<O>,
    key: &'p str,
    source: &'a str,
    lines: Lines,
    types: Types<'p, O>,
    tree: &'t Tree<'a, O::Type>,
    names: Names<'t, 'a, 'p, O>,
    /// Each object of the tree, as an object of a component.
    place: Vec<ObjRef>,
    components: Vec<ComponentDef>,
    /// For a template, the object it is written in and the property.
    templates: Vec<Option<(ObjRef, String)>>,
    /// How many objects of a type a parent has with neither id nor name.
    typed: HashMap<(CompIdx, Option<ObjIdx>, String), u32>,
    exprs: Vec<Binding>,
    functions: Vec<(JsFunction, String)>,
    translations: Vec<Translatable>,
    diagnostics: Vec<Diagnostic>,
    /// `qsTr`'s context: the file's stem, or `pragma Translator`.
    context: String,
    /// Whether a script of the file calls `qsTr`, which then needs the
    /// context at run time.
    translates: bool,
    /// The type a property is declared with, by object and name.
    declared: HashMap<(usize, String), String>,
    /// The enums of the file's components: inline name, enum, keys.
    enums: Vec<FileEnum>,
    /// The `Qt` global, for its enums.
    qt: Option<O::Type>,
    warned: HashSet<String>,
}

impl<'t, 'a, 'p, O: TypeOracle> Compiler<'t, 'a, 'p, O> {
    fn new(
        project: &'p Project<O>,
        key: &'p str,
        source: &'a str,
        lines: Lines,
        types: Types<'p, O>,
        tree: &'t Tree<'a, O::Type>,
    ) -> Self {
        let mut counts = vec![0u32; tree.contexts.len()];
        let place = tree
            .objects
            .iter()
            .map(|object| {
                let context = object.context;
                let at = ObjRef {
                    component: CompIdx(context as u32),
                    object: ObjIdx(counts[context]),
                };
                counts[context] += 1;
                at
            })
            .collect();
        let components = counts
            .iter()
            .map(|count| ComponentDef {
                name: String::new(),
                id: String::new(),
                kind: ComponentKind::File,
                objects: vec![ObjectDef::default(); *count as usize],
                signature: ComponentSignature([0; 32]),
            })
            .collect();
        let oracle = &project.oracle;
        let qt = oracle
            .module_type("QtQml", "Qt")
            .or_else(|| oracle.module_type("QML", "Qt"));
        Self {
            project,
            key,
            source,
            lines,
            types,
            tree,
            names: Names { tree, types },
            place,
            templates: vec![None; tree.contexts.len()],
            components,
            typed: HashMap::new(),
            exprs: Vec::new(),
            functions: Vec::new(),
            translations: Vec::new(),
            diagnostics: Vec::new(),
            context: stem(key).to_string(),
            translates: false,
            declared: HashMap::new(),
            enums: Vec::new(),
            qt,
            warned: HashSet::new(),
        }
    }

    fn span(&self, span: Span) -> SourceSpan {
        self.lines.span(self.source, span)
    }

    fn def_mut(&mut self, at: ObjRef) -> &mut ObjectDef {
        &mut self.components[at.component.index()].objects[at.object.index()]
    }

    /// The enums the file declares, with their numbers: the front end's
    /// summary has their keys only.
    fn collect_enums(&mut self, object: &QmlObject<'a>, inline: Option<&str>) {
        for member in &object.members {
            match member {
                QmlMember::Enum(declaration) => self.enums.push((
                    inline.map(str::to_string),
                    declaration.name.name.to_string(),
                    declaration
                        .members
                        .iter()
                        .map(|m| (m.name.name.to_string(), m.value))
                        .collect(),
                )),
                QmlMember::InlineComponent(component) => {
                    self.collect_enums(&component.object, Some(component.name.name.as_str()));
                }
                QmlMember::Object(child) => self.collect_enums(child, inline),
                _ => {}
            }
        }
    }

    // ---- objects ----------------------------------------------------------

    /// Lowers an object and everything in it; its index in the tree.
    fn object(&mut self, object: &QmlObject<'a>, parent: Option<usize>) -> Option<usize> {
        let index = self.tree.index_of(object)?;
        let at = self.place[index];
        let ty = self.object_type(index);
        let parent = parent
            .map(|p| self.place[p])
            .filter(|p| p.component == at.component)
            .map(|p| p.object);
        let span = self.span(object.span);
        // The one object a singleton file is cannot be made again. A type
        // the runtime provides is left to it: the type table says what Qt
        // calls a singleton, not what this runtime can make.
        if let Some(kind @ Kind::Component(_)) = &self.tree.objects[index].kind
            && self.types.is_singleton(kind)
        {
            self.diagnostics.push(Diagnostic::error(
                Code::NotCreatable,
                format!(
                    "Composite Singleton Type {} is not creatable.",
                    type_name(&ty)
                ),
                span,
            ));
        }
        let id = self.tree.objects[index].id.map(str::to_string);
        let def = self.def_mut(at);
        def.ty = ty;
        def.id = id;
        def.on = object
            .on
            .as_ref()
            .map(|on| on.parts.iter().map(|p| p.to_string()).collect());
        def.span = span;
        def.parent = parent;
        for member in &object.members {
            if let QmlMember::Property(declaration) = member
                && let Some(type_name) = &declaration.type_name
            {
                self.declared.insert(
                    (index, declaration.name.name.to_string()),
                    declared_name(&type_name.name.to_string(), type_name.is_list),
                );
            }
        }
        self.members(index, object, &[]);
        self.key_of(index, at, parent);
        Some(index)
    }

    /// The object's step of its key, now that its `objectName` is known.
    fn key_of(&mut self, index: usize, at: ObjRef, parent: Option<ObjIdx>) {
        let def = &self.components[at.component.index()].objects[at.object.index()];
        let named = def.members.assignments.iter().find_map(|a| match &a.value {
            Literal::String(name) if a.name == "objectName" => Some(name.clone()),
            _ => None,
        });
        let key = match (&def.id, named) {
            (Some(id), _) => KeySeg::Id(id.clone()),
            (None, Some(name)) => KeySeg::Named(name),
            (None, None) => {
                let ty = self.tree.objects[index].name.clone();
                let nth = self
                    .typed
                    .entry((at.component, parent, ty.clone()))
                    .or_insert(0);
                let key = KeySeg::Typed { ty, nth: *nth };
                *nth += 1;
                key
            }
        };
        self.def_mut(at).key = key;
    }

    fn members(&mut self, owner: usize, object: &QmlObject<'a>, group: &[&'a str]) {
        let at = self.place[owner];
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(owner, child, &path);
                }
                QmlMember::Object(child) => {
                    if let Some(value) = self.object_value(owner, child, "default") {
                        self.def_mut(at).children.push(value);
                    }
                }
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    if path == ["id"] {
                        continue;
                    }
                    self.check_member(owner, &path, binding.name.span);
                    let (slot, name) = self.slot(&path, binding.name.span);
                    self.value(owner, slot, name, binding.name.span, &binding.value, true);
                }
                QmlMember::Property(declaration) => self.property(owner, declaration),
                QmlMember::Signal(signal) => {
                    self.check_name(
                        Declared::Signal,
                        signal.name.name.as_ref(),
                        signal.name.span,
                    );
                    let declaration = SignalDecl {
                        name: signal.name.name.to_string(),
                        params: signal
                            .params
                            .iter()
                            .map(|param| SignalParam {
                                name: param.name.name.to_string(),
                                ty: param.type_name.as_ref().map(|t| t.name.to_string()),
                            })
                            .collect(),
                    };
                    self.def_mut(at).signals.push(declaration);
                }
                QmlMember::Function(function) => self.method(owner, function),
                QmlMember::InlineComponent(inline) => {
                    if let Some(index) = self.object(&inline.object, None) {
                        let component = &mut self.components[self.place[index].component.index()];
                        component.name = inline.name.name.to_string();
                        component.id = format!("{}#{}", self.key, component.name);
                    }
                }
                QmlMember::Enum(declaration) => {
                    let declaration = EnumDecl {
                        name: declaration.name.name.to_string(),
                        keys: declaration
                            .members
                            .iter()
                            .map(|m| (m.name.name.to_string(), m.value))
                            .collect(),
                    };
                    self.def_mut(at).enums.push(declaration);
                }
            }
        }
    }

    /// A binding of a member the object's type does not have, where the
    /// type says that is an error ([`TypeOracle::members_checked`]) and
    /// not a name for the running program to look for.
    fn check_member(&mut self, owner: usize, path: &[&str], span: Span) {
        let [name] = path else {
            self.check_path_member(owner, path, span);
            return;
        };
        let object = &self.tree.objects[owner];
        let Some(kind) = &object.kind else { return };
        let Some(ty) = self.types.base(kind) else {
            return;
        };
        let Some(severity) = self.missing_member(ty) else {
            return;
        };
        if !matches!(self.tree.lookup(self.types, owner, name), Lookup::Missing)
            || self.types.oracle.adds_member(ty, name)
        {
            return;
        }
        let message = match handled(name) {
            Some(signal) => {
                // `onContainsMouseChanged` is the change of `containsMouse`,
                // whatever its notifying signal is called.
                let changed = signal.strip_suffix("Changed").is_some_and(|property| {
                    matches!(
                        self.tree.lookup(self.types, owner, property),
                        Lookup::Found(Member::Property(_))
                    )
                });
                if changed
                    || self.types.oracle.adds_member(ty, &signal)
                    || !matches!(
                        self.tree.lookup_signal(self.types, owner, &signal),
                        Lookup::Missing
                    )
                {
                    return;
                }
                let message = format!(
                    "`{name}` handles the signal `{signal}`, which `{}` does not have",
                    object.name
                );
                (Code::UnknownSignal, message)
            }
            None => (
                Code::UnknownProperty,
                format!("`{name}` is not a property of `{}`", object.name),
            ),
        };
        self.report_missing(severity, message, span);
    }

    /// How a name `ty` lacks is reported where it is written, if it is
    /// ([`TypeOracle::members_checked`], [`TypeOracle::members_warned`]).
    fn missing_member(&self, ty: O::Type) -> Option<Severity> {
        if self.types.oracle.members_checked(ty) {
            Some(Severity::Error)
        } else if self.types.oracle.members_warned(ty) {
            Some(Severity::Warning)
        } else {
            None
        }
    }

    fn report_missing(&mut self, severity: Severity, (code, message): (Code, String), span: Span) {
        let span = self.span(span);
        self.diagnostics.push(match severity {
            Severity::Error => Diagnostic::error(code, message, span),
            // Only Ljós's own description says so, and Qt may have it.
            Severity::Warning => {
                Diagnostic::warning(code, format!("{message} in Ljós's table of types"), span)
            }
        });
    }

    /// [`check_member`](Self::check_member) for a member of a group
    /// (`info.label`) or of what a type attaches (`Native.flag`, or
    /// `Module.Native.flag`): each step is looked up on the type the step
    /// before it gives.
    fn check_path_member(&mut self, owner: usize, path: &[&str], span: Span) {
        let Some(first) = path.first() else { return };
        let (mut ty, mut at) = if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            let of = if path.len() > 2 && self.types.find(&path[..2]).is_some() {
                2
            } else {
                1
            };
            let Some(attached) = self
                .types
                .find(&path[..of])
                .and_then(|found| self.types.base(&found.kind))
                .and_then(|ty| self.types.oracle.attached(ty))
            else {
                return;
            };
            (attached, of)
        } else {
            match self.tree.lookup(self.types, owner, first) {
                Lookup::Found(Member::Property(property)) => match property.value {
                    Some(ty) => (ty, 1),
                    None => return,
                },
                _ => return,
            }
        };
        while let Some(name) = path.get(at) {
            let last = at + 1 == path.len();
            let found = self.types.lookup(&Kind::Builtin(ty), name);
            match found {
                Lookup::Found(Member::Property(property)) if !last => match property.value {
                    Some(next) => ty = next,
                    None => return,
                },
                Lookup::Found(_) if last => return,
                Lookup::Missing => {
                    let Some(severity) = self.missing_member(ty) else {
                        return;
                    };
                    if self.types.oracle.adds_member(ty, name) {
                        return;
                    }
                    let signal = handled(name).filter(|_| last);
                    if let Some(signal) = &signal {
                        let kind = Kind::Builtin(ty);
                        let changed = signal.strip_suffix("Changed").is_some_and(|property| {
                            matches!(
                                self.types.lookup(&kind, property),
                                Lookup::Found(Member::Property(_))
                            )
                        });
                        if changed
                            || self.types.oracle.adds_member(ty, signal)
                            || !matches!(self.types.lookup_signal(&kind, signal), Lookup::Missing)
                        {
                            return;
                        }
                    }
                    let written = path.join(".");
                    let of = path[..at].join(".");
                    let message = match signal {
                        Some(signal) => (
                            Code::UnknownSignal,
                            format!("`{written}`: `{of}` has no signal `{signal}`"),
                        ),
                        None => (
                            Code::UnknownProperty,
                            format!("`{written}`: `{of}` has no property `{name}`"),
                        ),
                    };
                    self.report_missing(severity, message, span);
                    return;
                }
                _ => return,
            }
            at += 1;
        }
    }

    /// `Type.Key` or `Singleton.member` naming what the type does not
    /// have, where the type says that is an error
    /// ([`TypeOracle::reads_checked`]).
    fn check_type_member(
        &mut self,
        name: &str,
        found: &Found<O::Type>,
        chain: &[String],
        offset: u32,
    ) {
        let Kind::Builtin(ty) = found.kind else {
            return;
        };
        let Some(first) = chain.first() else { return };
        if !self.types.oracle.reads_checked(ty) {
            return;
        }
        let missing = if first.starts_with(|c: char| c.is_ascii_uppercase()) {
            match self.types.declared_enum(&found.kind, first) {
                None => Some(first.clone()),
                Some(true) => chain
                    .get(1)
                    .filter(|key| !self.types.is_key(&found.kind, Some(first), key))
                    .map(|key| format!("{first}.{key}")),
                Some(false) => None,
            }
        } else if self.types.is_singleton(&found.kind)
            && matches!(self.types.lookup(&found.kind, first), Lookup::Missing)
        {
            Some(first.clone())
        } else {
            None
        };
        let Some(member) = missing else { return };
        if !self.warned.insert(format!("{offset}:{name}.{member}")) {
            return;
        }
        let span = Span::new(offset, offset + name.len() as u32);
        self.diagnostics.push(Diagnostic::error(
            Code::UnknownMember,
            format!("`{name}.{member}`: `{name}` has no member `{member}`"),
            self.span(span),
        ));
    }

    /// An object written where a value goes: one of the component, or the
    /// root of a template.
    fn object_value(
        &mut self,
        owner: usize,
        child: &QmlObject<'a>,
        property: &str,
    ) -> Option<ObjectValue> {
        let index = self.object(child, Some(owner))?;
        let at = self.place[index];
        if at.component == self.place[owner].component {
            return Some(ObjectValue::Object(at.object));
        }
        self.templates[at.component.index()] = Some((self.place[owner], property.to_string()));
        Some(ObjectValue::Component(at.component))
    }

    fn property(&mut self, owner: usize, declaration: &QmlPropertyDeclaration<'a>) {
        let at = self.place[owner];
        let name = declaration.name.name.to_string();
        let Some(type_name) = &declaration.type_name else {
            self.def_mut(at).required.push(name);
            return;
        };
        self.check_name(Declared::Property, &name, declaration.name.span);
        let is_alias = type_name.name.as_simple() == Some("alias");
        let alias = is_alias
            .then(|| alias_target(declaration))
            .flatten()
            .map(|(id, path)| AliasTarget {
                id: id.to_string(),
                object: self.id_in_sight(owner, id),
                path: path
                    .split('.')
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect(),
            });
        let span = self.span(declaration.span);
        self.def_mut(at).properties.push(PropertyDecl {
            name: name.clone(),
            ty: type_name.name.to_string(),
            is_list: type_name.is_list,
            is_default: declaration.is_default,
            is_readonly: declaration.is_readonly,
            is_required: declaration.is_required,
            alias,
            span,
        });
        if !is_alias && let Some(value) = &declaration.value {
            // Qt places such a binding at the declaration.
            self.value(owner, Slot::Own, name, declaration.span, value, false);
        }
    }

    /// The object `id` names from the object `owner`, nearest context first.
    fn id_in_sight(&self, owner: usize, id: &str) -> Option<ObjRef> {
        let mut context = Some(self.tree.objects[owner].context);
        while let Some(at) = context {
            if let Some(object) = self.tree.id_in(at, id) {
                return Some(self.place[object]);
            }
            context = self.tree.contexts[at].outer;
        }
        None
    }

    /// Where a member written `path` goes, and its name there.
    fn slot(&mut self, path: &[&str], span: Span) -> (Slot, String) {
        let owned = |parts: &[&str]| parts.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        match path {
            [name] => (Slot::Own, name.to_string()),
            [first, ..] if first.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let qualified = path.len() >= 3
                    && path[1].starts_with(|c: char| c.is_ascii_uppercase())
                    && self.types.namespace(first).is_some();
                let n = if qualified { 2 } else { 1 };
                if self.types.find(&path[..n]).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        Code::UnknownType,
                        qml_front::unknown_type(self.types.oracle, &path[..n].join(".")),
                        self.lines.span(self.source, span),
                    ));
                }
                (Slot::Attached(owned(&path[..n])), path[n..].join("."))
            }
            _ => (
                Slot::Group(owned(&path[..path.len() - 1])),
                path[path.len() - 1].to_string(),
            ),
        }
    }

    fn members_mut(&mut self, at: ObjRef, slot: &Slot) -> &mut Members {
        let ty = match slot {
            Slot::Attached(parts) => Some(self.type_of_parts(parts)),
            _ => None,
        };
        let def = self.def_mut(at);
        match slot {
            Slot::Own => &mut def.members,
            Slot::Group(path) => {
                let index = match def.grouped.iter().position(|g| &g.path == path) {
                    Some(index) => index,
                    None => {
                        def.grouped.push(Group {
                            path: path.clone(),
                            members: Members::default(),
                        });
                        def.grouped.len() - 1
                    }
                };
                &mut def.grouped[index].members
            }
            Slot::Attached(parts) => {
                let written = parts.join(".");
                let index = match def.attached.iter().position(|a| a.written == written) {
                    Some(index) => index,
                    None => {
                        def.attached.push(Attached {
                            ty: ty.unwrap_or_default(),
                            written,
                            members: Members::default(),
                        });
                        def.attached.len() - 1
                    }
                };
                &mut def.attached[index].members
            }
        }
    }

    /// What a member is given: a literal, an expression, a handler or
    /// objects.
    fn value(
        &mut self,
        owner: usize,
        slot: Slot,
        name: String,
        name_span: Span,
        value: &QmlBindingValue<'a>,
        may_handle: bool,
    ) {
        let at = self.place[owner];
        let name_span = self.span(name_span);
        let signal = handled(&name).filter(|_| may_handle);
        let target = self.describe(owner, &slot, &name);
        match value {
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) if signal.is_some() => {
                let signal = signal.unwrap_or_default();
                let (form, span) = match value {
                    QmlBindingValue::Expression(e) => (Form::Expression, e.span()),
                    QmlBindingValue::Statement(s) => (Form::Statement, s.span()),
                    _ => unreachable!("matched above"),
                };
                let args = self.signal_args(owner, &slot, &signal);
                let expr = self.expr(BindingKind::Handler, form, span, args, &target);
                self.members_mut(at, &slot)
                    .handlers
                    .push(Handler { signal, expr });
            }
            QmlBindingValue::Expression(expression) => {
                let color = self.is_color(owner, &slot, &name);
                let key = match self.held_enum(owner, &slot, &name) {
                    Some(held) => match self.plain_key(expression, &held) {
                        Ok(key) => key,
                        Err(()) => {
                            let span = self.span(expression.span());
                            self.diagnostics.push(Diagnostic::error(
                                Code::InvalidAssignment,
                                "Invalid property assignment: unknown enumeration",
                                span,
                            ));
                            return;
                        }
                    },
                    None => None,
                };
                if key.is_none()
                    && let Some(written) = Written::of(expression)
                    && let Some(said) = self
                        .held_kind(owner, &slot, &name)
                        .and_then(|held| held.refuses(&written))
                {
                    // qml6 does not load the file; a binding that gives
                    // the same is left to the run.
                    let span = self.span(expression.span());
                    self.diagnostics
                        .push(Diagnostic::error(Code::InvalidAssignment, said, span));
                    return;
                }
                let literal = match key {
                    Some(key) => Some(key),
                    None => self.literal(expression, color),
                };
                if let Some(Literal::Color(text)) = &literal
                    && ljos_ir::color::parse(text).is_none()
                {
                    // qml6 refuses the file; text that only a binding gives
                    // makes an invalid colour instead.
                    let span = self.span(expression.span());
                    self.diagnostics.push(Diagnostic::error(
                        Code::InvalidAssignment,
                        "Invalid property assignment: color expected",
                        span,
                    ));
                    return;
                }
                if let Some(literal) = literal {
                    let span = self.span(expression.span());
                    self.members_mut(at, &slot).assignments.push(Assignment {
                        name,
                        value: literal,
                        span,
                    });
                } else {
                    let expr = self.expr(
                        BindingKind::Property,
                        Form::Expression,
                        expression.span(),
                        Some(Vec::new()),
                        &target,
                    );
                    self.members_mut(at, &slot).bindings.push(PropertyBinding {
                        name,
                        expr,
                        name_span,
                    });
                }
            }
            QmlBindingValue::Statement(statement) => {
                let expr = self.expr(
                    BindingKind::Property,
                    Form::Statement,
                    statement.span(),
                    Some(Vec::new()),
                    &target,
                );
                self.members_mut(at, &slot).bindings.push(PropertyBinding {
                    name,
                    expr,
                    name_span,
                });
            }
            QmlBindingValue::Object(object) => {
                let values = self
                    .object_value(owner, object, &name)
                    .into_iter()
                    .collect();
                self.members_mut(at, &slot).objects.push(ObjectAssignment {
                    name,
                    values,
                    is_list: false,
                });
            }
            QmlBindingValue::Objects(objects) => {
                let values = objects
                    .iter()
                    .filter_map(|object| self.object_value(owner, object, &name))
                    .collect();
                self.members_mut(at, &slot).objects.push(ObjectAssignment {
                    name,
                    values,
                    is_list: true,
                });
            }
        }
    }

    fn method(&mut self, owner: usize, function: &Function<'a>) {
        let Some(id) = &function.id else {
            return;
        };
        let name = id.name.to_string();
        // `function onPicked(index) {}` in a Connections handles its
        // target's signal: it is a handler, not a method anyone calls.
        let signal = handled(&name).filter(|_| self.special(owner) == Some(Special::Connections));
        if let Some(signal) = signal {
            let target = format!("{}.{name}", self.tree.objects[owner].name);
            let expr = self.expr(
                BindingKind::Handler,
                Form::Function,
                function.span,
                Some(Vec::new()),
                &target,
            );
            self.def_mut(self.place[owner])
                .members
                .handlers
                .push(Handler { signal, expr });
            return;
        }
        self.check_name(Declared::Method, &name, id.span);
        let target = format!("{}.{name}()", self.tree.objects[owner].name);
        let expr = self.expr(
            BindingKind::Function,
            Form::Function,
            function.span,
            Some(Vec::new()),
            &target,
        );
        self.def_mut(self.place[owner])
            .functions
            .push(FunctionDecl { name, expr });
    }

    /// A property, signal or method declared with a name Qt's engine
    /// refuses: one with a capital first, or one of the script
    /// environment's own, which the member would hide from the file's
    /// scripts.
    fn check_name(&mut self, declared: Declared, name: &str, span: Span) {
        let capital = name.starts_with(|c: char| c.is_uppercase());
        let global = SCRIPT_GLOBALS.contains(&name);
        let (kind, said) = match declared {
            Declared::Property => ("property", "Property"),
            Declared::Signal => ("signal", "Signal"),
            Declared::Method => ("method", "Method"),
        };
        // A method or a signal is told of its capital first, a property of
        // its name (Qt 6.11.2).
        let message = match (capital, global, declared) {
            (true, _, Declared::Signal | Declared::Method) | (true, false, Declared::Property) => {
                format!("{said} names cannot begin with an upper case letter")
            }
            (_, true, _) => format!("Illegal {kind} name"),
            _ => return,
        };
        self.diagnostics
            .push(Diagnostic::error(Code::Semantic, message, self.span(span)));
    }

    /// `Rectangle.anchors.left`, for the comment above a function.
    fn describe(&self, owner: usize, slot: &Slot, name: &str) -> String {
        let ty = &self.tree.objects[owner].name;
        match slot {
            Slot::Own => format!("{ty}.{name}"),
            Slot::Group(path) => format!("{ty}.{}.{name}", path.join(".")),
            Slot::Attached(parts) => format!("{ty}.{}.{name}", parts.join(".")),
        }
    }

    // ---- types ------------------------------------------------------------

    fn type_ref(&self, found: &Found<O::Type>, qualifier: Option<&str>) -> TypeRef {
        match &found.kind {
            Kind::Builtin(ty) => {
                let oracle = self.types.oracle;
                let imported = match (&found.origin, qualifier) {
                    (Origin::Module(uri), _) => Some(uri.clone()),
                    (_, Some(alias)) => match self.types.namespace(alias) {
                        Some(Source::Module(uri)) => Some(uri.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                TypeRef::Native {
                    module: oracle.module(*ty).or(imported).unwrap_or_default(),
                    name: oracle.name(*ty).to_string(),
                }
            }
            Kind::Component(key) => TypeRef::Component {
                file: key.file.clone(),
                inline: key.inline.clone(),
            },
        }
    }

    fn type_of_parts(&self, parts: &[String]) -> TypeRef {
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        match self.types.find(&refs) {
            Some(found) => {
                let qualifier = (refs.len() == 2).then(|| refs[0]);
                self.type_ref(&found, qualifier)
            }
            None => TypeRef::Unresolved(parts.join(".")),
        }
    }

    fn object_type(&self, index: usize) -> TypeRef {
        let object = &self.tree.objects[index];
        match (&object.kind, &object.origin) {
            (Some(kind), Some(origin)) => {
                let found = Found {
                    kind: kind.clone(),
                    origin: origin.clone(),
                };
                let qualifier = object.name.split_once('.').map(|(q, _)| q);
                self.type_ref(&found, qualifier)
            }
            _ => TypeRef::Unresolved(object.name.clone()),
        }
    }

    fn special(&self, index: usize) -> Option<Special> {
        let kind = self.tree.objects[index].kind.as_ref()?;
        self.types.oracle.special(self.types.base(kind)?)
    }

    /// The parameters of the signal a handler handles; None when the signal
    /// is not known.
    fn signal_args(&self, owner: usize, slot: &Slot, signal: &str) -> Option<Vec<String>> {
        let oracle = self.types.oracle;
        match slot {
            Slot::Own => {
                let object = if self.special(owner) == Some(Special::Connections) {
                    self.tree.target(owner)?
                } else {
                    owner
                };
                self.tree.signal(self.types, object, signal)
            }
            Slot::Attached(parts) => {
                if parts.as_slice() == ["Component"]
                    && matches!(signal, "completed" | "destruction" | "reloaded")
                {
                    return Some(Vec::new());
                }
                let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
                let found = self.types.find(&refs)?;
                let attached = oracle.attached(self.types.base(&found.kind)?)?;
                oracle.signal(attached, signal)
            }
            Slot::Group(path) => {
                let refs: Vec<&str> = path.iter().map(String::as_str).collect();
                if let Some(args) = self
                    .group_kind(owner, &refs)
                    .and_then(|kind| self.types.signal(&kind, signal))
                {
                    return Some(args);
                }
                let property = self.tree.property(self.types, owner, &refs)?;
                oracle.signal(property.value?, signal)
            }
        }
    }

    /// The type of the object a group written on `owner` leads to through
    /// properties that hold objects: `form.button` of `form.button.onClicked`,
    /// where `form`'s component has `button` as an alias.
    fn group_kind(&self, owner: usize, path: &[&str]) -> Option<Kind<O::Type>> {
        let (first, rest) = path.split_first()?;
        let object = &self.tree.objects[owner];
        if object.declared.contains_key(first) {
            return None;
        }
        let mut kind = self.types.property_kind(object.kind.as_ref()?, first)?;
        for name in rest {
            kind = self.types.property_kind(&kind, name)?;
        }
        Some(kind)
    }

    /// Whether the property a member names holds a colour.
    fn is_color(&self, owner: usize, slot: &Slot, name: &str) -> bool {
        self.member_type(owner, slot, name)
            .is_some_and(|(_, ty)| is_color(&ty))
    }

    /// The type of the property a member names, and the built-in type whose
    /// property it is (None for a property a QML file declares).
    fn member_type(
        &self,
        owner: usize,
        slot: &Slot,
        name: &str,
    ) -> Option<(Option<O::Type>, String)> {
        let oracle = self.types.oracle;
        let of = |ty: O::Type| Some((Some(ty), oracle.property_type(ty, name)?));
        match slot {
            Slot::Own => {
                // What a PropertyChanges changes is its target's.
                let object = self.tree.target(owner).unwrap_or(owner);
                self.declared_type(object, name)
            }
            Slot::Group(path) => {
                let refs: Vec<&str> = path.iter().map(String::as_str).collect();
                match refs.as_slice() {
                    [id] if self.id_in_sight(owner, id).is_some() => {
                        let context = self.tree.objects[owner].context;
                        let object = self.tree.id_in(context, id);
                        object.and_then(|object| self.declared_type(object, name))
                    }
                    _ => self
                        .tree
                        .property(self.types, owner, &refs)
                        .and_then(|property| of(property.value?)),
                }
            }
            Slot::Attached(parts) => {
                let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
                self.types
                    .find(&refs)
                    .and_then(|found| self.types.base(&found.kind))
                    .and_then(|base| oracle.attached(base))
                    .and_then(of)
            }
        }
    }

    /// What the property a member names holds, where a value written for
    /// it is checked when the file is read.
    fn held_kind(&self, owner: usize, slot: &Slot, name: &str) -> Option<Held> {
        // What a PropertyChanges is given is checked when its state is
        // entered.
        if matches!(slot, Slot::Own) && self.tree.target(owner).is_some() {
            return None;
        }
        let oracle = self.types.oracle;
        let (ty, type_name) = self.member_type(owner, slot, name)?;
        let Some(ty) = ty else {
            // Declared in a file: `list<T>`, a value type, or an object
            // type, which starts with a capital.
            if type_name.starts_with("list<") {
                return Some(Held::List);
            }
            if let Some(held) = Held::of_value(&type_name) {
                return Some(held);
            }
            let parts: Vec<&str> = type_name.split('.').collect();
            if !parts.last()?.starts_with(char::is_uppercase) {
                return None;
            }
            // A type of a file Qt names by a counter it makes up: it is
            // named as it is written.
            let class = match self.types.find(&parts)?.kind {
                Kind::Builtin(ty) => oracle.class_name(ty).unwrap_or(oracle.name(ty)),
                Kind::Component(_) => &type_name,
            };
            return Some(Held::Object(class.to_string()));
        };
        let property = oracle.property(ty, name)?;
        if property.is_list {
            return Some(Held::List);
        }
        if oracle.holds_object(ty, name) {
            let value = property.value?;
            let class = oracle.class_name(value).unwrap_or(oracle.name(value));
            return Some(Held::Object(class.to_string()));
        }
        Held::of_value(&type_name)
    }

    /// The enum a member holds a key of, where the type table says it holds
    /// one: the type that has the enum, and the enum's name.
    fn held_enum(&self, owner: usize, slot: &Slot, name: &str) -> Option<(O::Type, String)> {
        // What a PropertyChanges is given for its target is for the state
        // to check when it is entered: Qt loads such a file.
        if matches!(slot, Slot::Own) && self.tree.target(owner).is_some() {
            return None;
        }
        let (ty, type_name) = self.member_type(owner, slot, name)?;
        let ty = ty?;
        if self.types.oracle.has_enum(ty, &type_name) {
            return Some((ty, type_name));
        }
        // `Qt::Alignment`: the `Qt` namespace has nothing but enums, and
        // the oracle finds its keys though not, by name, the enum itself.
        let of_qt = type_name.strip_prefix("Qt::")?;
        Some((self.qt?, of_qt.to_string()))
    }

    /// A plain value written for a member that holds a key of `held`: the
    /// key a string names, with its number where the type table has one.
    /// `Err` for a string that names no key, and for a number, a bool or
    /// `null`: Qt takes none of them written in the source ("unknown
    /// enumeration"), though a script may give the same number.
    fn plain_key(
        &self,
        expression: &Expression<'a>,
        held: &(O::Type, String),
    ) -> Result<Option<Literal>, ()> {
        let oracle = self.types.oracle;
        let (ty, name) = held;
        match expression {
            Expression::StringLiteral(string) => {
                let written = string.value.as_str();
                // Flags take several keys, "LeftButton | RightButton"; an
                // enum that holds one key takes one.
                let keys: Vec<&str> = if written.contains('|') {
                    if !oracle.enum_is_flags(*ty, name) {
                        return Err(());
                    }
                    written.split('|').map(str::trim).collect()
                } else {
                    vec![written]
                };
                if keys
                    .iter()
                    .any(|key| !oracle.has_enum_key(*ty, Some(name), key))
                {
                    return Err(());
                }
                // Without a number the runtime, which has its own table,
                // finds the key in the type by its name, as it does for
                // `Text.AlignRight`.
                let value = keys
                    .iter()
                    .map(|key| oracle.enum_value(*ty, Some(name), key))
                    .try_fold(0, |all, one| Some(all | one?));
                Ok(Some(Literal::Enum(EnumRef {
                    ty: TypeRef::Native {
                        module: oracle.module(*ty).unwrap_or_default(),
                        name: oracle.name(*ty).to_string(),
                    },
                    scope: Some(name.clone()),
                    key: keys.join("|"),
                    value,
                })))
            }
            Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_) => Err(()),
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                Err(())
            }
            _ => Ok(None),
        }
    }

    /// The type of a property of an object: one it declares, or one of its
    /// type's, then with the built-in type that has it.
    fn declared_type(&self, object: usize, name: &str) -> Option<(Option<O::Type>, String)> {
        if let Some(ty) = self.declared.get(&(object, name.to_string())) {
            return Some((None, ty.clone()));
        }
        let mut kind = self.tree.objects[object].kind.clone()?;
        for _ in 0..64 {
            match kind {
                Kind::Builtin(ty) => {
                    return Some((Some(ty), self.types.oracle.property_type(ty, name)?));
                }
                Kind::Component(key) => {
                    let shape = self.project.front.shape(&key)?;
                    if let Some(declaration) = shape.properties.get(name) {
                        let written = &declaration.type_name;
                        return Some((None, declared_name(written, declaration.is_list)));
                    }
                    let types = Types::new(&self.project.front, &self.project.oracle, &key.file);
                    let parts: Vec<&str> = shape.root.iter().map(String::as_str).collect();
                    kind = types.find(&parts)?.kind;
                }
            }
        }
        None
    }

    fn enum_value(&self, kind: &Kind<O::Type>, scope: Option<&str>, key: &str) -> Option<i64> {
        match kind {
            Kind::Builtin(ty) => self.types.oracle.enum_value(*ty, scope, key),
            // The enums of the file being compiled are read off its own tree;
            // those of the components it is built on, in other files or in
            // this one, come from what the front end summarised of them.
            Kind::Component(component) => (component.file == self.key)
                .then(|| {
                    self.enums
                        .iter()
                        .filter(|(inline, name, _)| {
                            *inline == component.inline && scope.is_none_or(|scope| scope == name)
                        })
                        .find_map(|(_, _, keys)| {
                            keys.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
                        })
                })
                .flatten()
                .or_else(|| self.types.enum_value(kind, scope, key)),
        }
    }

    fn qt_enum(&self, key: &str) -> Option<EnumRef> {
        let oracle = self.types.oracle;
        let ty = match self.qt {
            Some(qt) => {
                // With a table, only what it lists; without one, any
                // capitalised member of `Qt` is an enum key.
                if !oracle.has_enum_key(qt, None, key) {
                    return None;
                }
                TypeRef::Native {
                    module: oracle.module(qt).unwrap_or_else(|| "QtQml".into()),
                    name: oracle.name(qt).to_string(),
                }
            }
            None => TypeRef::Native {
                module: "QtQml".into(),
                name: "Qt".into(),
            },
        };
        Some(EnumRef {
            ty,
            scope: None,
            key: key.to_string(),
            value: self.qt.and_then(|qt| oracle.enum_value(qt, None, key)),
        })
    }

    // ---- literals ---------------------------------------------------------

    /// The value, when it is the same every time it is read.
    fn literal(&mut self, expression: &Expression<'a>, color: bool) -> Option<Literal> {
        Some(match expression {
            Expression::NumericLiteral(number) => Literal::Number(number.value),
            Expression::StringLiteral(string) if color => Literal::Color(string.value.to_string()),
            Expression::StringLiteral(string) => Literal::String(string.value.to_string()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let text = template.quasis.first()?.value.cooked.as_ref()?.to_string();
                if color {
                    Literal::Color(text)
                } else {
                    Literal::String(text)
                }
            }
            Expression::BooleanLiteral(boolean) => Literal::Bool(boolean.value),
            Expression::NullLiteral(_) => Literal::Null,
            Expression::Identifier(id)
                if id.name == "undefined" && self.is_global(id.name.as_str(), id.span.start) =>
            {
                Literal::Undefined
            }
            Expression::UnaryExpression(unary) => {
                let Expression::NumericLiteral(number) = &unary.argument else {
                    return None;
                };
                match unary.operator {
                    UnaryOperator::UnaryNegation => Literal::Number(-number.value),
                    UnaryOperator::UnaryPlus => Literal::Number(number.value),
                    _ => return None,
                }
            }
            // `("name")` is an expression to qml6, not a colour written in
            // the file: a name that is no colour is not refused.
            Expression::ParenthesizedExpression(inner) => {
                return self.literal(&inner.expression, false);
            }
            Expression::ArrayExpression(array) => {
                let mut items = Vec::new();
                for element in &array.elements {
                    let item = element.as_expression()?;
                    items.push(self.literal(item, false)?);
                }
                Literal::List(items)
            }
            Expression::ObjectExpression(object) => {
                let mut fields = Vec::new();
                for property in &object.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        return None;
                    };
                    if property.computed
                        || property.shorthand
                        || property.method
                        || property.kind != PropertyKind::Init
                    {
                        return None;
                    }
                    let key = property.key.static_name()?.to_string();
                    fields.push((key, self.literal(&property.value, false)?));
                }
                Literal::Object(fields)
            }
            Expression::StaticMemberExpression(_) => Literal::Enum(self.enum_literal(expression)?),
            Expression::CallExpression(call) => {
                let (callee, args) = js::call_args(call)?;
                let Expression::Identifier(id) = &call.callee else {
                    return None;
                };
                if !self.is_global(&callee, id.span.start) {
                    return None;
                }
                let (translatable, is_static) = self.translatable(&callee, &args)?;
                self.translations.push(translatable.clone());
                if !is_static {
                    return None;
                }
                match translatable.kind {
                    TranslationKind::NoOp => Literal::String(translatable.source),
                    _ => Literal::Translatable(translatable),
                }
            }
            _ => return None,
        })
    }

    fn is_global(&self, name: &str, offset: u32) -> bool {
        self.lookup(name, offset)
            .is_some_and(|r| r.roles.is_empty() && matches!(r.target, Some(Target::Global(_))))
    }

    /// What a free name is. In a delegate any name nothing declares may be a
    /// role of the model, except the functions that translate: a text marked
    /// for translation has to be the same text to the tool that lists them
    /// and to the running program, and `qsTr` has to be the file's own (it
    /// carries the context), not the global one a look-up at run time finds.
    fn lookup(&self, name: &str, offset: u32) -> Option<Resolution<O::Type>> {
        let mut resolution = self.names.lookup(name, offset)?;
        if marks_translation(name)
            && resolution.aimed.is_none()
            && matches!(resolution.target, Some(Target::Context { .. }))
            && let Some(global) = names::global(name)
        {
            resolution.roles.clear();
            resolution.target = Some(Target::Global(global));
        }
        Some(resolution)
    }

    /// `Text.AlignLeft`, `Image.Fill.Stretch`, `C.Text.AlignLeft`,
    /// `Qt.AlignLeft`.
    fn enum_literal(&self, expression: &Expression<'a>) -> Option<EnumRef> {
        let mut members = Vec::new();
        let mut object = expression;
        while let Expression::StaticMemberExpression(member) = object {
            members.push(member.property.name.as_str());
            object = &member.object;
        }
        let Expression::Identifier(base) = object else {
            return None;
        };
        members.reverse();
        let name = base.name.as_str();
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) || members.len() > 3 {
            return None;
        }
        let resolution = self.names.lookup(name, base.span.start)?;
        let (found, qualifier, rest) = match resolution.target? {
            Target::Type(found) => (found, None, &members[..]),
            Target::Namespace(_) => {
                let (ty, rest) = members.split_first()?;
                (self.types.find(&[name, ty])?, Some(name), rest)
            }
            Target::Global(_) if name == "Qt" => {
                return match members.as_slice() {
                    [key] if is_key_name(key) => self.qt_enum(key),
                    _ => None,
                };
            }
            _ => return None,
        };
        let (scope, key) = match rest {
            [key] => (None, *key),
            [scope, key] => (Some(*scope), *key),
            _ => return None,
        };
        if !is_key_name(key) || !self.types.is_key(&found.kind, scope, key) {
            return None;
        }
        Some(EnumRef {
            ty: self.type_ref(&found, qualifier),
            scope: scope.map(str::to_string),
            key: key.to_string(),
            value: self.enum_value(&found.kind, scope, key),
        })
    }

    /// What a call of a translation function marks, and whether every
    /// argument is a literal.
    fn translatable(&self, callee: &str, args: &[ArgValue]) -> Option<(Translatable, bool)> {
        let mut is_static = args.iter().all(|a| !matches!(a, ArgValue::Other));
        let text = |index: usize| match args.get(index) {
            Some(ArgValue::Str(text)) => Some(text.clone()),
            _ => None,
        };
        let (kind, context, source, comment, n) = match callee {
            "qsTr" => (
                TranslationKind::Text,
                Some(self.context.clone()),
                0,
                Some(1),
                Some(2),
            ),
            "qsTranslate" => (TranslationKind::Text, None, 1, Some(2), Some(3)),
            "qsTrId" => (TranslationKind::Id, Some(String::new()), 0, None, Some(1)),
            "qsTrNoOp" | "QT_TR_NOOP" => (
                TranslationKind::NoOp,
                Some(self.context.clone()),
                0,
                Some(1),
                None,
            ),
            "qsTranslateNoOp" | "QT_TRANSLATE_NOOP" => {
                (TranslationKind::NoOp, None, 1, Some(2), None)
            }
            "qsTrIdNoOp" | "QT_TRID_NOOP" => {
                (TranslationKind::NoOp, Some(String::new()), 0, None, None)
            }
            _ => return None,
        };
        let context = match context {
            Some(context) => context,
            None => text(0)?,
        };
        let source = text(source)?;
        let comment = comment.and_then(text);
        let (plural, is_plural) = match n.and_then(|n| args.get(n)) {
            Some(ArgValue::Num(n)) => (Some(*n as i64), true),
            Some(_) => {
                is_static = false;
                (None, true)
            }
            None => (None, false),
        };
        Some((
            Translatable {
                kind,
                context,
                source,
                comment,
                plural,
                is_plural,
            },
            is_static,
        ))
    }

    // ---- expressions ------------------------------------------------------

    /// Compiles a script to a function of the module. `args` are the
    /// signal's parameters; None when the signal is not known.
    fn expr(
        &mut self,
        kind: BindingKind,
        form: Form,
        span: Span,
        args: Option<Vec<String>>,
        target: &str,
    ) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        let js_fn = FnIndex(self.functions.len() as u32);
        let export = format!("f{}", js_fn.0);
        let text = qml_syntax::text(self.source, span);
        let scope_index = self.tree.scope_at(span.start);
        let scope = self.place[scope_index];
        let source_span = self.span(span);
        let hash = u64::from_le_bytes(
            blake3::hash(text.as_bytes()).as_bytes()[..8]
                .try_into()
                .unwrap_or_default(),
        );
        let known = args.clone().unwrap_or_default();
        let header = format!(
            "// {}.qml:{}:{} {target}\n",
            self.key, source_span.line, source_span.column
        );
        let (params, refs, body_args, code) = match js::analyze(text, form, &known) {
            Ok(analysis) => self.function(
                &export,
                text,
                span,
                form,
                kind,
                scope_index,
                &args,
                analysis,
            ),
            Err(message) => {
                self.diagnostics.push(Diagnostic::error(
                    Code::ScriptSyntax,
                    format!("the script does not parse as JavaScript: {message}"),
                    source_span,
                ));
                let quoted = format!("{message:?}");
                let code = format!(
                    "export function {export}() {{\n  throw new SyntaxError({quoted});\n}}\n"
                );
                (Vec::new(), Vec::new(), Vec::new(), code)
            }
        };
        self.functions
            .push((JsFunction { export, expr: id }, format!("{header}{code}")));
        self.exprs.push(Binding {
            kind,
            scope,
            js_fn,
            params,
            args: body_args,
            refs,
            span: source_span,
            hash,
            tier1: None,
        });
        id
    }

    /// Resolves the script's free names and writes its function.
    #[allow(clippy::too_many_arguments)]
    fn function(
        &mut self,
        export: &str,
        text: &str,
        span: Span,
        form: Form,
        kind: BindingKind,
        scope: usize,
        args: &Option<Vec<String>>,
        analysis: js::Analysis,
    ) -> (Vec<Param>, Vec<Ref>, Vec<String>, String) {
        let mut params: Vec<Param> = Vec::new();
        let mut refs: Vec<Ref> = Vec::new();
        let mut edits = analysis.edits.clone();
        if form == Form::Statement && kind == BindingKind::Property {
            edits.extend(js::completion_returns(text));
        }
        let mut plans: HashMap<(String, Vec<String>), Plan> = HashMap::new();
        let written: HashSet<&str> = analysis
            .free
            .iter()
            .filter(|free| free.written)
            .map(|free| free.name.as_str())
            .collect();
        // Names whose members the script writes: a value type read through
        // one has to be written back through it.
        let value_bases: HashSet<u32> = analysis
            .member_writes
            .iter()
            .filter_map(|write| match write {
                js::ValueSource::Free(start) => Some(*start),
                _ => None,
            })
            .collect();
        for free in &analysis.free {
            let offset = span.start + free.span.start;
            let plan = plans
                .entry((free.name.clone(), free.chain.clone()))
                .or_insert_with(|| {
                    let plan = self.resolve(&free.name, offset, &free.chain, scope);
                    if written.contains(free.name.as_str()) {
                        held(plan)
                    } else {
                        plan
                    }
                })
                .clone();
            if let Some(param) = &plan.param
                && !params.iter().any(|p| p.name == param.name)
            {
                params.push(param.clone());
            }
            let r = Ref {
                name: free.name.clone(),
                resolved: plan.resolved.clone(),
            };
            if free.name == "qsTr" && matches!(r.resolved, Resolved::Global { .. }) {
                self.translates = true;
            }
            if !refs.contains(&r) {
                refs.push(r);
            }
            let value_base = value_bases.contains(&free.span.start);
            if let Some(through) = &plan.rewrite {
                let text = if free.shorthand {
                    format!("{0}: {through}.{0}", free.name)
                } else if value_base {
                    format!("{VALUE_AT}({through}, {:?})", free.name)
                } else {
                    format!("{through}.{}", free.name)
                };
                edits.push(js::Edit {
                    span: free.span,
                    text,
                });
            } else if value_base
                && plan.replace.is_none()
                && plan.param.as_ref().is_some_and(|param| {
                    param.name == free.name
                        && matches!(param.arg, Arg::Context { .. } | Arg::Dynamic(_))
                })
            {
                // The name reached the function by value: it is a variable
                // of the function's like any other.
                let unknown = plan
                    .param
                    .as_ref()
                    .is_some_and(|param| matches!(param.arg, Arg::Dynamic(_)));
                let text = if unknown {
                    let name = &free.name;
                    format!("{VALUE_IN}({}, ($v) => {name} = $v)", read_or_throw(name))
                } else {
                    value_in(&free.name)
                };
                edits.push(js::Edit {
                    span: free.span,
                    text,
                });
            } else if plan.replace.is_none()
                && plan.param.as_ref().is_some_and(|param| {
                    param.name == free.name && matches!(param.arg, Arg::Dynamic(_))
                })
            {
                // Nothing in sight has the name. Where the running program
                // has none either, reading it throws.
                edits.push(js::Edit {
                    span: free.span,
                    text: or_throw(free),
                });
            }
            if let Some((text, members)) = &plan.replace {
                let end = match members.checked_sub(1) {
                    Some(last) => free.chain_ends.get(last).copied(),
                    None => Some(free.span.end),
                };
                if let Some(end) = end {
                    // `{ Theme }` is `{ Theme: Theme }`.
                    let text = if free.shorthand {
                        format!("{}: {text}", free.name)
                    } else {
                        text.clone()
                    };
                    edits.push(js::Edit {
                        span: Span::new(free.span.start, end),
                        text,
                    });
                }
            }
        }
        // `const p = a.b` of a `p` whose members are written: the variable
        // keeps where its value was read from beside it, unless an edit
        // already made runs across where the place is cut.
        let mut placed: HashSet<u32> = HashSet::new();
        for place in &analysis.places {
            let crosses = |at: u32| {
                edits
                    .iter()
                    .any(|edit| edit.span.start < at && edit.span.end > at)
            };
            let held = format!("{}{HELD}", place.name);
            let rest = format!(", {} = {held}.v", place.name);
            let made = match &place.source {
                js::ValueSource::Member { object, name, end } if !crosses(object.end) => vec![
                    js::Edit {
                        span: Span::new(place.binding.start, object.start),
                        text: format!("{held} = {PLACE}("),
                    },
                    js::Edit {
                        span: Span::new(object.end, *end),
                        text: format!(", {name:?}){rest}"),
                    },
                ],
                js::ValueSource::Element { object, key, end }
                    if !crosses(object.end) && !crosses(key.start) && !crosses(key.end) =>
                {
                    vec![
                        js::Edit {
                            span: Span::new(place.binding.start, object.start),
                            text: format!("{held} = {PLACE}("),
                        },
                        js::Edit {
                            span: Span::new(object.end, key.start),
                            text: ", ".into(),
                        },
                        js::Edit {
                            span: Span::new(key.end, *end),
                            text: format!("){rest}"),
                        },
                    ]
                }
                js::ValueSource::Free(start) => {
                    // A name read through the object that has it; one the
                    // engine passes in by value has no place to go back to.
                    let found = analysis.free.iter().find(|free| free.span.start == *start);
                    let through = found.and_then(|free| {
                        let plan = plans.get(&(free.name.clone(), free.chain.clone()))?;
                        plan.replace.is_none().then_some(())?;
                        Some((free, plan.rewrite.clone()?))
                    });
                    match through {
                        // Covers the edit made for the name itself.
                        Some((free, through)) => vec![js::Edit {
                            span: Span::new(place.binding.start, free.span.end),
                            text: format!("{held} = {PLACE}({through}, {:?}){rest}", free.name),
                        }],
                        None => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };
            if !made.is_empty() {
                edits.extend(made);
                placed.insert(place.symbol);
            }
        }
        // Before the edits for attached objects: where both start at the same
        // place, this one is the outer call.
        for write in &analysis.member_writes {
            match write {
                js::ValueSource::Member { object, name, end } => {
                    edits.push(js::Edit {
                        span: Span::new(object.start, object.start),
                        text: format!("{VALUE_AT}("),
                    });
                    edits.push(js::Edit {
                        span: Span::new(object.end, *end),
                        text: format!(", {name:?})"),
                    });
                }
                js::ValueSource::Element { object, key, end } => {
                    edits.push(js::Edit {
                        span: Span::new(object.start, object.start),
                        text: format!("{VALUE_AT}("),
                    });
                    edits.push(js::Edit {
                        span: Span::new(object.end, key.start),
                        text: ", ".into(),
                    });
                    edits.push(js::Edit {
                        span: Span::new(key.end, *end),
                        text: ")".into(),
                    });
                }
                js::ValueSource::Local { span, name, symbol } => edits.push(js::Edit {
                    span: *span,
                    text: if placed.contains(symbol) {
                        format!("{VALUE_HELD}({name}, {name}{HELD}, ($v) => {name} = $v)")
                    } else {
                        value_in(name)
                    },
                }),
                js::ValueSource::Free(_) => {}
            }
        }
        for member in &analysis.type_members {
            // `Text.AlignLeft`, `Controls.Button`: a name that is a type or
            // a module has no object a type could have attached to.
            let named = member.base.is_some_and(|start| {
                analysis
                    .free
                    .iter()
                    .filter(|free| free.span.start == start)
                    .filter_map(|free| plans.get(&(free.name.clone(), free.chain.clone())))
                    .any(|plan| {
                        matches!(
                            plan.resolved,
                            Resolved::ImportNs(_)
                                | Resolved::Type(_)
                                | Resolved::Enum(_)
                                | Resolved::Attached { .. }
                                | Resolved::Global { .. }
                        )
                    })
            });
            if named {
                continue;
            }
            let Some((module, name)) = self.attaching(&member.name) else {
                continue;
            };
            edits.push(js::Edit {
                span: Span::new(member.object.start, member.object.start),
                text: format!("{ATTACHED}("),
            });
            edits.push(js::Edit {
                span: Span::new(member.object.end, member.end),
                text: format!(", {module:?}, {name:?})"),
            });
        }
        for local in &analysis.locals {
            refs.push(Ref {
                name: local.clone(),
                resolved: Resolved::Local,
            });
        }
        for call in &analysis.calls {
            if let Some((translatable, _)) = self.translatable(&call.callee, &call.args) {
                self.translations.push(translatable);
            }
        }
        let body = js::apply(text, &edits);
        let close = if body.contains("//") { "\n  " } else { "" };
        let mut signature: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        let wrapped = args.is_none() || analysis.uses_arguments;
        let known = args.clone().unwrap_or_default();
        let (body_args, code) = match form {
            Form::Function => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (analysis.function_params.clone(), code)
            }
            Form::Expression if kind == BindingKind::Handler && analysis.is_function_value => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            _ if wrapped => {
                signature.push("...$args".into());
                let inner = match form {
                    Form::Expression => format!(
                        "function ({}) {{ return ({body}{close}); }}",
                        known.join(", ")
                    ),
                    _ => format!("function ({}) {}", known.join(", "), block(&body)),
                };
                let code = format!(
                    "export function {export}({}) {{\n  return ({inner}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Expression => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close});\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Statement => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {}\n",
                    signature.join(", "),
                    block(&body)
                );
                (known, code)
            }
        };
        (params, refs, body_args, code)
    }

    /// What a free name at `offset` is, and how the function gets it.
    fn resolve(&mut self, name: &str, offset: u32, chain: &[String], scope: usize) -> Plan {
        let context = self.tree.objects[self.tree.object_at(offset)].context;
        let Some(resolution) = self.lookup(name, offset) else {
            if self.project.dynamic_names().contains(name) {
                return Plan {
                    resolved: Resolved::ContextProp {
                        component: None,
                        fallback: None,
                    },
                    param: Some(Param {
                        name: name.to_string(),
                        arg: Arg::Context {
                            component: None,
                            name: name.to_string(),
                            fallback: None,
                        },
                    }),
                    rewrite: None,
                    replace: None,
                };
            }
            if self.warned.insert(name.to_string()) {
                let span = self
                    .lines
                    .span(self.source, Span::new(offset, offset + name.len() as u32));
                self.diagnostics.push(Diagnostic::warning(
                    Code::UnresolvedName,
                    format!(
                        "`{name}` is not a name in sight: it is looked up when the program runs"
                    ),
                    span,
                ));
            }
            return Plan {
                resolved: Resolved::Unknown,
                param: Some(Param {
                    name: name.to_string(),
                    arg: Arg::Dynamic(name.to_string()),
                }),
                rewrite: None,
                replace: None,
            };
        };
        if let Some(Target::Type(found)) = &resolution.target {
            self.check_type_member(name, found, chain, offset);
        }
        let base = resolution
            .target
            .as_ref()
            .map(|target| self.target(name, target, chain, scope, context));
        let role = resolution.roles.first().copied();
        if role.is_none()
            && resolution.aimed.is_none()
            && let Some(base) = base
        {
            return base;
        }
        // The running program may have the name first: a role of the model,
        // or what the target of a PropertyChanges has.
        let component = role.map(|c| CompIdx(c as u32));
        let fallback_arg = base.as_ref().and_then(|base| {
            let param = base.param.as_ref()?;
            Some(Box::new(match (&base.rewrite, &param.arg) {
                (Some(_), Arg::Object(object)) => Arg::Property {
                    object: *object,
                    name: name.to_string(),
                },
                (_, arg) => arg.clone(),
            }))
        });
        Plan {
            resolved: Resolved::ContextProp {
                component,
                fallback: base.map(|base| Box::new(base.resolved)),
            },
            param: Some(Param {
                name: name.to_string(),
                arg: Arg::Context {
                    component,
                    name: name.to_string(),
                    fallback: fallback_arg,
                },
            }),
            rewrite: None,
            replace: None,
        }
    }

    fn target(
        &self,
        name: &str,
        target: &Target<O::Type>,
        chain: &[String],
        scope: usize,
        context: usize,
    ) -> Plan {
        let as_is = |arg: Arg| {
            Some(Param {
                name: name.to_string(),
                arg,
            })
        };
        match target {
            Target::Id(object) => {
                let at = self.place[*object];
                let resolved = if self.tree.objects[*object].context == context {
                    Resolved::Id(at)
                } else {
                    Resolved::OuterId(at)
                };
                Plan {
                    resolved,
                    param: as_is(Arg::Object(at)),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Property { object, .. } => {
                let at = self.place[*object];
                let root = self.tree.contexts[context].root;
                let (resolved, through) = if *object == scope {
                    (Resolved::ScopeProp, "$scope".to_string())
                } else if *object == root {
                    (Resolved::RootProp, "$root".to_string())
                } else {
                    (
                        Resolved::OuterProp(at),
                        format!("$c{}o{}", at.component.0, at.object.0),
                    )
                };
                Plan {
                    resolved,
                    param: Some(Param {
                        name: through.clone(),
                        arg: Arg::Object(at),
                    }),
                    rewrite: Some(through),
                    replace: None,
                }
            }
            Target::Context { context, .. } => {
                let component = Some(CompIdx(*context as u32));
                Plan {
                    resolved: Resolved::ContextProp {
                        component,
                        fallback: None,
                    },
                    param: as_is(Arg::Context {
                        component,
                        name: name.to_string(),
                        fallback: None,
                    }),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Type(found) => self.type_plan(name, found, None, chain, scope),
            Target::Namespace(source) => {
                let source = import_source(source);
                // `S.Theme.accent`: the script reads a type of the module,
                // which is what the reference is, through the namespace.
                let through = chain
                    .first()
                    .filter(|ty| is_key_name(ty))
                    .and_then(|ty| Some((self.types.find(&[name, ty.as_str()])?, &chain[1..])))
                    .map(|(found, rest)| self.type_plan(name, &found, Some(name), rest, scope));
                // `L.Layout.row`: what the type attached is all the script
                // needs, where the namespace alone is nothing to it.
                if let Some(plan) = &through
                    && plan.replace.is_some()
                {
                    return plan.clone();
                }
                Plan {
                    resolved: through
                        .map_or_else(|| Resolved::ImportNs(source.clone()), |plan| plan.resolved),
                    param: as_is(Arg::Namespace {
                        alias: name.to_string(),
                        source,
                    }),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Global(global) => {
                if name == "Qt"
                    && let Some(key) = chain.first().filter(|key| is_key_name(key))
                    && let Some(value) = self.qt_enum(key)
                {
                    return Plan {
                        resolved: Resolved::Enum(value),
                        param: None,
                        rewrite: None,
                        replace: None,
                    };
                }
                Plan {
                    resolved: Resolved::Global {
                        is_qml: *global == Global::Qml,
                    },
                    param: None,
                    rewrite: None,
                    replace: None,
                }
            }
        }
    }

    /// The module and name of the type `name` is in this file, when it is
    /// one that attaches an object to others.
    fn attaching(&self, name: &str) -> Option<(String, String)> {
        let found = self.types.find(&[name])?;
        let base = self.types.base(&found.kind)?;
        self.types.oracle.attached(base)?;
        match self.type_ref(&found, None) {
            TypeRef::Native { module, name } => Some((module, name)),
            _ => None,
        }
    }

    /// A type named in a script: what of it the script reads. `scope` is
    /// the object the script is written in.
    fn type_plan(
        &self,
        name: &str,
        found: &Found<O::Type>,
        qualifier: Option<&str>,
        chain: &[String],
        scope: usize,
    ) -> Plan {
        let ty = self.type_ref(found, qualifier);
        let kind = &found.kind;
        // Through a namespace the name the script has is the namespace's,
        // and every type of it is read by that one name: each gets a
        // parameter of its own, written where `Alias.Type` was.
        let through = |prefix: &str| match qualifier {
            Some(qualifier) => format!("${prefix}_{qualifier}_{}", type_name(&ty)),
            None => format!("${prefix}_{name}"),
        };
        let qualified = qualifier.map(|_| through("t"));
        let param = |arg: Arg| {
            Some(Param {
                name: qualified.clone().unwrap_or_else(|| name.to_string()),
                arg,
            })
        };
        let replace = || qualified.clone().map(|through| (through, 1));
        let enumeration = |scope: Option<&str>, key: &str| Plan {
            resolved: Resolved::Enum(EnumRef {
                ty: ty.clone(),
                scope: scope.map(str::to_string),
                key: key.to_string(),
                value: self.enum_value(kind, scope, key),
            }),
            param: param(Arg::Type(ty.clone())),
            rewrite: None,
            replace: replace(),
        };
        if let Some(first) = chain.first() {
            match self.types.declared_enum(kind, first) {
                Some(false) => return enumeration(None, first),
                Some(true) => {
                    if let Some(key) = chain.get(1)
                        && self.types.is_key(kind, Some(first), key)
                    {
                        return enumeration(Some(first), key);
                    }
                }
                None => {}
            }
        }
        if self.types.is_singleton(kind) {
            // The object is made when the script first comes to the name,
            // not when the function is called: a branch not taken leaves it
            // unmade. So the function is given what stands for it, and asks
            // for the object where the name was.
            let through = through("s");
            return Plan {
                resolved: Resolved::Singleton(ty.clone()),
                param: Some(Param {
                    name: through.clone(),
                    arg: Arg::Singleton(ty),
                }),
                rewrite: None,
                replace: Some((
                    format!("{SINGLETON}({through})"),
                    usize::from(qualifier.is_some()),
                )),
            };
        }
        let attaches = self
            .types
            .base(kind)
            .and_then(|base| self.types.oracle.attached(base))
            .is_some();
        // The name alone, as a value (`target: Component`), is what the type
        // attached to the scope object too, as in Qt; its member is "".
        let member = match chain.first() {
            Some(member) if member.starts_with(|c: char| c.is_ascii_lowercase()) => {
                Some(member.as_str())
            }
            None => Some(""),
            Some(_) => None,
        };
        match member {
            Some(member) if attaches => {
                // Its own parameter: the type's name may also be read for an
                // enum in the same script (`Accessible.Button`), and that is
                // the type, not what it attached.
                let through = through("a");
                Plan {
                    resolved: Resolved::Attached {
                        ty: ty.clone(),
                        member: member.to_string(),
                    },
                    param: Some(Param {
                        name: through.clone(),
                        arg: Arg::Attached {
                            ty,
                            object: self.place[scope],
                        },
                    }),
                    rewrite: None,
                    replace: Some((through, usize::from(qualifier.is_some()))),
                }
            }
            _ => Plan {
                resolved: Resolved::Type(ty.clone()),
                param: param(Arg::Type(ty)),
                rewrite: None,
                replace: replace(),
            },
        }
    }

    // ---- the file ---------------------------------------------------------

    /// The scripts the file imports by path, then those they import, each
    /// once. A script that leads to one that is not there, or back to
    /// itself, does not load, nor does the file: Qt says the script is
    /// unavailable.
    fn scripts(&mut self, document: &QmlDocument<'a>) -> Vec<Script> {
        let mut scripts: Vec<Script> = Vec::new();
        for import in &document.imports {
            let QmlImportSource::Path(path) = &import.source else {
                continue;
            };
            if !(path.ends_with(".js") || path.ends_with(".mjs")) {
                continue;
            }
            let key = qml_front::project::join(qml_front::project::directory(self.key), path);
            let Some(text) = self.project.script(&key) else {
                continue;
            };
            let script = crate::script::lower(path, &key, text);
            if let Some(trouble) = self.unavailable(&script, &mut vec![key.clone()]) {
                self.diagnostics.push(Diagnostic::error(
                    Code::ScriptUnavailable,
                    format!("the script \"{path}\" is unavailable: {trouble}"),
                    self.span(import.span),
                ));
            }
            scripts.push(script);
        }
        // Those only scripts import, after the file's own, so a lookup by
        // the path the file writes finds its own first.
        let mut next = 0;
        while next < scripts.len() {
            let wanted: Vec<String> = scripts[next]
                .imports
                .iter()
                .map(|i| i.key.clone())
                .collect();
            next += 1;
            for key in wanted {
                if scripts.iter().any(|s| s.key == key) {
                    continue;
                }
                if let Some(text) = self.project.script(&key) {
                    scripts.push(crate::script::lower("", &key, text));
                }
            }
        }
        scripts
    }

    /// Why `script` cannot be made, if it cannot: a script it leads to is
    /// not in the project, or leads back to one of `chain`.
    fn unavailable(&self, script: &Script, chain: &mut Vec<String>) -> Option<String> {
        for import in &script.imports {
            if let Some(at) = chain.iter().position(|key| *key == import.key) {
                let mut cycle = chain[at..].to_vec();
                cycle.push(import.key.clone());
                return Some(format!("its imports go round: {}", cycle.join(" -> ")));
            }
            let Some(text) = self.project.script(&import.key) else {
                return Some(format!(
                    "\"{}\" imports \"{}\", which is not there",
                    script.key, import.path
                ));
            };
            let inner = crate::script::lower("", &import.key, text);
            chain.push(import.key.clone());
            let trouble = self.unavailable(&inner, chain);
            chain.pop();
            if trouble.is_some() {
                return trouble;
            }
        }
        None
    }

    fn finish(mut self, document: &QmlDocument<'a>) -> Compiled {
        let scripts = self.scripts(document);
        let tree = self.tree;
        let mut inline = Vec::new();
        for (index, context) in tree.contexts.iter().enumerate() {
            let component = &mut self.components[index];
            component.kind = match context.outer {
                None if index == 0 => {
                    component.name = stem(self.key).to_string();
                    component.id = self.key.to_string();
                    ComponentKind::File
                }
                None => {
                    inline.push(component.name.clone());
                    ComponentKind::Inline
                }
                Some(outer) => {
                    component.name = tree.objects[context.root].name.clone();
                    ComponentKind::Template {
                        outer: CompIdx(outer as u32),
                        is_delegate: context.is_delegate,
                    }
                }
            };
        }
        for (index, component) in self.components.iter_mut().enumerate() {
            let names: &[String] = if index == 0 { &inline } else { &[] };
            component.signature = ComponentSignature::of(component.root(), names);
        }
        let mut translations = Vec::new();
        for translatable in std::mem::take(&mut self.translations) {
            if !translations.contains(&translatable) {
                translations.push(translatable);
            }
        }
        let mut source = format!(
            "// {}.qml, compiled by ljos_compiler: a function per binding, handler and method.\n",
            self.key
        );
        if self.translates {
            // `qsTr` looks a text up in the file's context, which a script
            // cannot know: the module binds its own `qsTr` to it, over the
            // global one. `qsTranslate` and `qsTrId` carry theirs.
            source.push_str(&format!(
                "const qsTr = {TR_IN}({});\n",
                js_string(&self.context)
            ));
        }
        let mut functions = Vec::new();
        for (function, code) in self.functions {
            source.push('\n');
            source.push_str(&code);
            functions.push(function);
        }
        if MAKERS.iter().any(|maker| source.contains(maker)) {
            // What a script makes by address is addressed from its own
            // file, which only the running program can place: the module
            // has a `Qt` of its own, and the runtime says where the file
            // is before it runs anything. At the end, so no line moves.
            source.push_str(&format!(
                "\nlet Qt = globalThis.Qt;\nexport function {AT}(url) {{\n  Qt = globalThis.{QT_AT}(url);\n}}\n"
            ));
        }
        // What the singletons the file names can be asked without a script.
        let mut invocable = Vec::new();
        let mut asked: HashSet<&str> = HashSet::new();
        for param in self.exprs.iter().flat_map(|binding| &binding.params) {
            if let Arg::Singleton(ty) = &param.arg
                && let TypeRef::Component { file, inline: None } = ty
                && file != self.key
                && asked.insert(file)
            {
                let functions = self.project.pure_functions(file);
                if !functions.is_empty() {
                    invocable.push(Invocable {
                        ty: ty.clone(),
                        functions: functions.to_vec(),
                    });
                }
            }
        }
        let mut file = CompiledFile {
            version: IR_VERSION,
            file: self.key.to_string(),
            source_hash: *blake3::hash(self.source.as_bytes()).as_bytes(),
            imports: document
                .imports
                .iter()
                .map(|import| Import {
                    source: match &import.source {
                        QmlImportSource::Module(name) => ImportSource::Module(name.to_string()),
                        QmlImportSource::Path(path) => ImportSource::Path(path.to_string()),
                    },
                    version: import.version.map(str::to_string),
                    alias: import.alias.as_ref().map(|alias| alias.name.to_string()),
                    in_project: matches!(
                        &import.source,
                        QmlImportSource::Module(name) if self.project.front.has_module(&name.to_string())
                    ),
                })
                .collect(),
            pragmas: document
                .pragmas
                .iter()
                .map(|pragma| Pragma {
                    name: pragma.name.to_string(),
                    value: pragma.value.map(str::to_string),
                })
                .collect(),
            components: self.components,
            exprs: self.exprs,
            translations,
            js: JsModule { source, functions },
            invocable,
            scripts,
        };
        // A template is known by where it is written, which is only known
        // once every key is: templates come after what they are written in.
        let mut nth: HashMap<String, u32> = HashMap::new();
        for (index, template) in self.templates.iter().enumerate() {
            let Some((owner, property)) = template else {
                continue;
            };
            let key = file.object_key(*owner);
            let mut id = key.component.clone();
            for segment in &key.segments {
                id.push('/');
                id.push_str(&segment_text(segment));
            }
            id.push('.');
            id.push_str(property);
            let count = nth.entry(id.clone()).or_insert(0);
            if *count > 0 {
                id.push_str(&format!("[{count}]"));
            }
            *count += 1;
            file.components[index].id = id;
        }
        crate::tier1::compile(&mut file);
        Compiled {
            file,
            diagnostics: self.diagnostics,
        }
    }
}

/// A statement as a function body.
fn block(body: &str) -> String {
    if body.trim_start().starts_with('{') {
        body.to_string()
    } else {
        format!("{{\n  {body}\n}}")
    }
}

/// The plan for a name the script assigns to. A name the running program
/// supplies reaches a script by value, and assigning to a value changes
/// nothing but the script's copy: such a name is read and written through
/// the object that has it instead.
fn held(plan: Plan) -> Plan {
    let Some(Param { name, arg }) = &plan.param else {
        return plan;
    };
    let (component, fallback) = match arg {
        Arg::Context {
            component,
            fallback,
            ..
        } => match fallback.as_deref() {
            Some(Arg::Property { object, .. }) => (*component, Some(*object)),
            // An outer context is on the way out from the inner one.
            None | Some(Arg::Context { .. }) => (*component, None),
            // An id or a type: nothing an assignment can change.
            Some(_) => return plan,
        },
        Arg::Dynamic(_) => (None, None),
        _ => return plan,
    };
    let through = format!("$w_{name}");
    Plan {
        resolved: plan.resolved,
        param: Some(Param {
            name: through.clone(),
            arg: Arg::Holder {
                component,
                name: name.clone(),
                fallback,
            },
        }),
        rewrite: Some(through),
        replace: None,
    }
}

/// The global that gives what `object[name]` is for a write to one of its
/// members: the member itself, or a stand-in for a value type that writes
/// the changed value back. The engine defines it, and [`VALUE_IN`].
const VALUE_AT: &str = "__ljos_vt";

/// The same for a variable: `(value, put)`, where `put` assigns to it.
const VALUE_IN: &str = "__ljos_vl";

/// The same for a variable that keeps where its value was read from:
/// `(value, place, put)`. The changed value is written to the place too.
const VALUE_HELD: &str = "__ljos_vp";

/// `(object, name)`: the place a variable's value is read from, with the
/// value it read (`.v`).
const PLACE: &str = "__ljos_at";

/// What the variable that keeps a variable's place is named after it.
const HELD: &str = "$ljos";

/// A variable, for a write to one of its members. It has to start with a
/// name: QML is mostly written without semicolons, and a line that starts
/// with a parenthesis would be read as a call of the line before.
fn value_in(name: &str) -> String {
    format!("{VALUE_IN}({name}, ($v) => {name} = $v)")
}

/// The global that gives back what a name is, or throws a ReferenceError
/// for it when the engine found nothing of that name
/// ([`ljos_ir::Arg::Dynamic`]). The engine defines it, and [`KNOWN`].
const NAMED: &str = "__ljos_named";

/// The same for `typeof name`, which is `"undefined"` and no error where
/// nothing has the name.
const KNOWN: &str = "__ljos_known";

/// A name nothing in sight declares, where its value is read: what the
/// engine found for it, or a ReferenceError. Like [`value_in`] it starts
/// with a name.
fn read_or_throw(name: &str) -> String {
    format!("{NAMED}({name}, {name:?})")
}

/// [`read_or_throw`] as the name is written.
fn or_throw(free: &js::Free) -> String {
    let name = &free.name;
    let read = if free.asked {
        format!("{KNOWN}({name})")
    } else {
        read_or_throw(name)
    };
    match (free.shorthand, free.constructed) {
        (true, _) => format!("{name}: {read}"),
        (false, true) => format!("({read})"),
        (false, false) => read,
    }
}

/// The global that gives what a type attached to an object. The engine
/// defines it.
const ATTACHED: &str = "__ljos_att";

/// `__ljos_single($s_Theme)`: the one object a singleton type is, made on
/// the first call. What the function is given for the type only stands for
/// it.
const SINGLETON: &str = "__ljos_single";

/// The members of `Qt` that read an address from the file that asks.
const MAKERS: [&str; 3] = ["createComponent", "createQmlObject", "resolvedUrl"];

/// The export the runtime calls with the address of the module's file.
const AT: &str = "__ljos_at";

/// The runtime's maker of a module's `Qt`: `(url) => Qt`.
const QT_AT: &str = "__ljos_qt";

/// The runtime's maker of a file's `qsTr`: `(context) => qsTr`.
const TR_IN: &str = "__ljos_qsTr";

/// A string as a JavaScript literal.
fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // The two line ends JavaScript has besides those, and what
            // cannot be seen.
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The name of a type, without its module.
fn type_name(ty: &TypeRef) -> &str {
    match ty {
        TypeRef::Native { name, .. } | TypeRef::Unresolved(name) => name,
        TypeRef::Component { file, inline } => inline.as_deref().unwrap_or(stem(file)),
    }
}

fn is_key_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// `Main` of `app/Main`: `qsTr`'s context and the file component's name.
/// The global functions that translate a text or mark one for translation.
fn marks_translation(name: &str) -> bool {
    matches!(
        name,
        "qsTr"
            | "qsTranslate"
            | "qsTrId"
            | "qsTrNoOp"
            | "qsTranslateNoOp"
            | "qsTrIdNoOp"
            | "QT_TR_NOOP"
            | "QT_TRANSLATE_NOOP"
            | "QT_TRID_NOOP"
    )
}

fn stem(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// `pragma Translator: Name`: the context of the file's `qsTr` calls,
/// instead of its name. Written as a name or as a string.
pub(crate) fn translator<'a>(document: &QmlDocument<'a>) -> Option<&'a str> {
    let value = document
        .pragmas
        .iter()
        .find(|pragma| pragma.name == "Translator")?
        .value?;
    let value = value.trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    (!unquoted.is_empty()).then_some(unquoted)
}

fn segment_text(segment: &KeySeg) -> String {
    match segment {
        KeySeg::Id(id) => id.clone(),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
    }
}

fn import_source(source: &Source) -> ImportSource {
    match source {
        Source::Module(uri) => ImportSource::Module(uri.clone()),
        Source::Path(path) => ImportSource::Path(path.clone()),
    }
}

#[derive(Clone, Copy)]
enum Declared {
    Property,
    Signal,
    Method,
}

/// The names of Qt's script environment that no member may have: its
/// globals (Qt 6.11.2, by declaring each with qml6).
const SCRIPT_GLOBALS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "Atomics",
    "Boolean",
    "DOMException",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "Float32Array",
    "Float64Array",
    "Function",
    "Infinity",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "JSON",
    "Map",
    "Math",
    "NaN",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "QT_TRANSLATE_NOOP",
    "QT_TRID_NOOP",
    "QT_TR_NOOP",
    "Qt",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "URL",
    "URLSearchParams",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakSet",
    "XMLHttpRequest",
    "console",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "eval",
    "gc",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "print",
    "qsTr",
    "qsTrId",
    "qsTranslate",
    "undefined",
    "unescape",
];

/// A declared property's type as the compiler names it: `list<T>` for a
/// list, so that it is not taken for what it holds a list of.
fn declared_name(type_name: &str, is_list: bool) -> String {
    if is_list {
        format!("list<{type_name}>")
    } else {
        type_name.to_string()
    }
}
