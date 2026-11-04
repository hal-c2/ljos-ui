//! The checker of a file written in Ljós's own API (docs/DIALECT.md §5).
//!
//! A file that imports `Ljos.Ui` has its scripts held to the strict
//! TypeScript subset, unless it says they are still JavaScript (`pragma
//! Scripts: JavaScript`, which the compat pass and `ljos migrate` write).
//! Each rule broken is an error, said in the file's terms, with the rule's
//! name as its [`Code`].
//!
//! The checker gives every value a script makes a type ([`ty`]): literals,
//! names (a script's own locals first, then what QML's scope has), members
//! of QML objects through their prototypes, of TypeScript records, arrays
//! and the built-ins [`builtins`] lists, calls, operators, functions and
//! arrows, object, array and template literals, narrowed by `typeof`, a
//! comparison with `undefined` or a literal, and `?.`. It walks each script
//! once. What it cannot type is an error type that goes anywhere, so one
//! mistake is said once and what the checker does not know is let by.
//!
//! DECISION: a finding at the place the compiler already reported
//! something (a literal of the wrong type for its property, say) is not
//! said again.

mod builtins;
mod expr;
mod ty;

use std::collections::HashMap;

use qml_front::{
    Special, TypeOracle,
    project::Key,
    scope::{CHANGES, handled, is_group},
    types::{Kind as QKind, Types},
};
use qml_syntax::{
    GetSpan, Span,
    ast::{QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject},
};

use super::Compiler;
use crate::{Code, Diagnostic};
use ty::{Arena, Kind, Param, Sig, Ty, article};

/// The module whose import makes a file one of Ljós's API.
const MODULE: &str = "Ljos.Ui";

/// Whether `document` is written in Ljós's API: it imports it itself.
pub(crate) fn is_dialect(document: &QmlDocument<'_>) -> bool {
    document.imports.iter().any(|import| {
        matches!(&import.source, QmlImportSource::Module(name) if name.to_string() == MODULE)
    })
}

/// Whether `document` says its scripts are Qt's JavaScript, not yet the
/// subset: `pragma Scripts: JavaScript`.
pub(crate) fn scripts_are_javascript(document: &QmlDocument<'_>) -> bool {
    document
        .pragmas
        .iter()
        .any(|pragma| pragma.name == "Scripts" && pragma.value == Some("JavaScript"))
}

/// Holds the scripts of a dialect file to the checker's rules.
pub(super) fn document<'a, O: TypeOracle>(
    compiler: &mut Compiler<'_, 'a, '_, O>,
    document: &QmlDocument<'a>,
) {
    let mut found = {
        let mut checker = Checker::new(compiler);
        checker.declare(&document.root, None);
        checker.object(&document.root);
        checker.found
    };
    found.sort_by_key(|(_, _, span)| span.start);
    found.dedup_by_key(|(_, _, span)| span.start);
    for (code, message, span) in found {
        if compiler
            .diagnostics
            .iter()
            .any(|d| d.span.start == span.start)
        {
            continue;
        }
        let span = compiler.span(span);
        compiler
            .diagnostics
            .push(Diagnostic::error(code, message, span));
    }
}

/// An object the checker's types name: by its place in [`Checker::objs`].
#[derive(Clone, PartialEq)]
enum Obj<T> {
    /// Of a type the oracle has.
    Native(T),
    /// Of a component a file declares.
    Component(Key),
    /// The object of this file's tree, by its index.
    Instance(usize),
}

/// What a name a script declares is bound to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Binding {
    Const,
    Let,
    Param,
    Function,
    /// A `let` or `const` before its declaration runs.
    Dead,
    /// A TypeScript type, `type T = …` or `interface T {}`.
    Type,
}

#[derive(Clone, Copy, Debug)]
struct Local<'a> {
    name: &'a str,
    ty: Ty,
    binding: Binding,
    id: u32,
}

/// What a condition, or an assignment, says a path is from here on.
#[derive(Clone, Copy, Debug)]
struct Fact<'a> {
    /// The path as written: `next`, `root.level`.
    key: &'a str,
    /// The local the path starts at, or None for a name of QML's scope.
    local: Option<u32>,
    ty: Ty,
    /// What the path is declared as, for when the fact no longer holds.
    base: Ty,
}

/// The function whose body is being walked.
struct FnCtx {
    /// What it is declared, or expected, to return.
    ret: Option<Ty>,
    returns: Vec<Ty>,
    /// The words of a return of the wrong type: a binding's block says it
    /// of its property.
    target: Option<(String, String)>,
}

/// A member a QML object declares, with its type and, for a property, the
/// type as written.
#[derive(Clone)]
struct Own {
    ty: Ty,
    written: Option<String>,
}

pub(super) struct Checker<'c, 't, 'a, 'p, O: TypeOracle> {
    c: &'c Compiler<'t, 'a, 'p, O>,
    arena: Arena,
    objs: Vec<Obj<O::Type>>,
    found: Vec<(Code, String, Span)>,
    /// Above 0, nothing is reported: a path is read again for a fact.
    quiet: u32,
    locals: Vec<Local<'a>>,
    next_local: u32,
    facts: Vec<Fact<'a>>,
    /// The facts below it were found outside the function being walked:
    /// only those of a `const` hold inside it.
    floor: usize,
    /// The facts assignments made, for where branches meet.
    assigned: Vec<Fact<'a>>,
    fns: Vec<FnCtx>,
    own: HashMap<(usize, &'a str), Own>,
}

impl<'c, 't, 'a, 'p, O: TypeOracle> Checker<'c, 't, 'a, 'p, O> {
    fn new(c: &'c Compiler<'t, 'a, 'p, O>) -> Self {
        Self {
            c,
            arena: Arena::new(),
            objs: Vec::new(),
            found: Vec::new(),
            quiet: 0,
            locals: Vec::new(),
            next_local: 0,
            facts: Vec::new(),
            floor: 0,
            assigned: Vec::new(),
            fns: Vec::new(),
            own: HashMap::new(),
        }
    }

    fn report(&mut self, code: Code, message: String, span: Span) {
        if self.quiet == 0 {
            self.found.push((code, message, span));
        }
    }

    fn text(&self, span: Span) -> &'a str {
        &self.c.source[span.start as usize..span.end as usize]
    }

    // Objects.

    fn obj(&mut self, obj: Obj<O::Type>) -> u32 {
        if let Some(at) = self.objs.iter().position(|known| *known == obj) {
            return at as u32;
        }
        self.objs.push(obj);
        (self.objs.len() - 1) as u32
    }

    fn of_kind(&mut self, kind: &QKind<O::Type>) -> u32 {
        match kind {
            QKind::Builtin(ty) => self.obj(Obj::Native(*ty)),
            QKind::Component(key) => self.obj(Obj::Component(key.clone())),
        }
    }

    fn object_ty(&mut self, kind: &QKind<O::Type>) -> Ty {
        let o = self.of_kind(kind);
        self.arena.intern(Kind::Object(o))
    }

    fn obj_name(&self, o: u32) -> String {
        match &self.objs[o as usize] {
            Obj::Native(ty) => self.c.types.oracle.name(*ty).to_string(),
            Obj::Component(key) => key
                .inline
                .clone()
                .unwrap_or_else(|| super::stem(&key.file).to_string()),
            Obj::Instance(index) => self.c.tree.objects[*index].name.clone(),
        }
    }

    fn word(&self, ty: Ty) -> String {
        self.arena.word(ty, &|o| self.obj_name(o))
    }

    /// The type of the object `o` is, as the front end names it.
    fn kind_of(&self, o: u32) -> Option<QKind<O::Type>> {
        match &self.objs[o as usize] {
            Obj::Native(ty) => Some(QKind::Builtin(*ty)),
            Obj::Component(key) => Some(QKind::Component(key.clone())),
            Obj::Instance(index) => self.c.tree.objects[*index].kind.clone(),
        }
    }

    /// The types of the front end the object is one of: itself, what it is
    /// built on, and so on to the end of its prototypes.
    fn chain(&self, o: u32) -> Vec<Obj<O::Type>> {
        let mut chain = vec![self.objs[o as usize].clone()];
        let mut kind = self.kind_of(o);
        for _ in 0..64 {
            match kind.take() {
                Some(QKind::Component(key)) => {
                    chain.push(Obj::Component(key.clone()));
                    let Some(shape) = self.c.project.front.shape(&key) else {
                        break;
                    };
                    let types =
                        Types::new(&self.c.project.front, &self.c.project.oracle, &key.file);
                    let parts: Vec<&str> = shape.root.iter().map(String::as_str).collect();
                    kind = types.find(&parts).map(|found| found.kind);
                }
                Some(QKind::Builtin(mut ty)) => {
                    chain.push(Obj::Native(ty));
                    for _ in 0..64 {
                        let Some(next) = self.c.types.oracle.prototype(ty) else {
                            break;
                        };
                        chain.push(Obj::Native(next));
                        ty = next;
                    }
                    break;
                }
                None => break,
            }
        }
        chain
    }

    // Members.

    /// The type of a QML type written as text (`real`, `list<Item>`,
    /// `QtQml.real`, `Button`), as the file `types` reads names.
    fn written(&mut self, written: &str, is_list: bool, types: &Types<'_, O>) -> Ty {
        if let Some(inner) = written
            .strip_prefix("list<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            let elem = self.written(inner, false, types);
            return self.arena.array(elem, true);
        }
        let elem = self.written_one(written, types);
        if is_list {
            self.arena.array(elem, true)
        } else {
            elem
        }
    }

    fn written_one(&mut self, written: &str, types: &Types<'_, O>) -> Ty {
        if let Some(ty) = primitive(plain(written)) {
            return ty;
        }
        let parts: Vec<&str> = written.split('.').collect();
        if let Some(found) = types.find(&parts) {
            return self.object_ty(&found.kind);
        }
        // A value type the file's imports do not name: `font`, `point`.
        let oracle = self.c.types.oracle;
        let name = plain(written);
        for module in ["QtQuick", "QtQml"] {
            if let Some(ty) = oracle.module_type(module, name) {
                return self.object_ty(&QKind::Builtin(ty));
            }
        }
        Ty::ERROR
    }

    /// What `name` is on the object `index` of the file's tree.
    fn instance_member(&mut self, index: usize, name: &str) -> Option<Own> {
        if let Some(own) = self.own.get(&(index, name)) {
            return Some(own.clone());
        }
        if let Some(property) = name.strip_suffix("Changed")
            && self
                .own
                .get(&(index, property))
                .is_some_and(|own| own.written.is_some())
        {
            let ty = self.signal_ty(Vec::new());
            return Some(Own { ty, written: None });
        }
        let kind = self.c.tree.objects[index].kind.clone()?;
        self.kind_member(&kind, name)
    }

    fn signal_ty(&mut self, params: Vec<Param>) -> Ty {
        self.arena.function(Sig {
            params: params.into_boxed_slice(),
            rest: None,
            ret: Ty::VOID,
        })
    }

    /// A function the checker knows nothing of: it takes anything.
    fn loose(&mut self) -> Ty {
        self.arena.function(Sig {
            params: Box::new([]),
            rest: Some(Ty::ERROR),
            ret: Ty::ERROR,
        })
    }

    /// What `name` is on an object of the type `kind`.
    fn kind_member(&mut self, kind: &QKind<O::Type>, name: &str) -> Option<Own> {
        let mut kind = kind.clone();
        for _ in 0..64 {
            match kind {
                QKind::Builtin(ty) => return self.native_member(ty, name),
                QKind::Component(key) => {
                    let front = &self.c.project.front;
                    let shape = front.shape(&key)?;
                    let types = Types::new(front, &self.c.project.oracle, &key.file);
                    if let Some(declaration) = shape.properties.get(name) {
                        if declaration.alias.is_some() || declaration.type_name == "alias" {
                            return Some(Own {
                                ty: Ty::ERROR,
                                written: None,
                            });
                        }
                        let ty = self.written(&declaration.type_name, declaration.is_list, &types);
                        let written =
                            super::declared_name(&declaration.type_name, declaration.is_list);
                        return Some(Own {
                            ty,
                            written: Some(written),
                        });
                    }
                    if let Some(params) = shape.signals.get(name) {
                        let params = params.iter().map(|name| param(name, Ty::ERROR)).collect();
                        let ty = self.signal_ty(params);
                        return Some(Own { ty, written: None });
                    }
                    if shape.functions.contains(name) {
                        let ty = self.loose();
                        return Some(Own { ty, written: None });
                    }
                    if let Some(property) = name.strip_suffix("Changed")
                        && shape.properties.contains_key(property)
                    {
                        let ty = self.signal_ty(Vec::new());
                        return Some(Own { ty, written: None });
                    }
                    let parts: Vec<&str> = shape.root.iter().map(String::as_str).collect();
                    kind = types.find(&parts)?.kind;
                }
            }
        }
        None
    }

    /// What `name` is on an object of the type the oracle has.
    fn native_member(&mut self, ty: O::Type, name: &str) -> Option<Own> {
        let oracle = self.c.types.oracle;
        if let Some(property) = oracle.property(ty, name) {
            let written = oracle.property_type(ty, name);
            let text = written.as_deref().unwrap_or_default();
            let one = if let Some(primitive) = primitive(plain(text)) {
                primitive
            } else if let Some(value) = property.value {
                self.object_ty(&QKind::Builtin(value))
            } else if let Some(enum_ty) = self.enum_of(ty, text) {
                enum_ty
            } else {
                Ty::ERROR
            };
            let ty = if property.is_list {
                self.arena.array(one, true)
            } else {
                one
            };
            return Some(Own { ty, written });
        }
        if let Some(names) = oracle.signal(ty, name) {
            let types = oracle.signal_types(ty, name);
            let scope = self.c.types;
            let mut params = Vec::new();
            for (at, param_name) in names.iter().enumerate() {
                let ty = match types.as_ref().and_then(|types| types.get(at)) {
                    Some(written) => self.written(written, false, &scope),
                    None => Ty::ERROR,
                };
                params.push(param(param_name, ty));
            }
            let ty = self.signal_ty(params);
            return Some(Own { ty, written: None });
        }
        if oracle.has_method(ty, name) {
            // DECISION: the oracle has no types for a native method's
            // arguments and result: it takes and gives what the checker
            // lets by.
            let ty = self.loose();
            return Some(Own { ty, written: None });
        }
        None
    }

    /// The enum a property of `owner` declared as `written` holds.
    fn enum_of(&mut self, owner: O::Type, written: &str) -> Option<Ty> {
        let oracle = self.c.types.oracle;
        if let Some(name) = written.split_once("::").map(|(_, name)| name) {
            let qt = self.c.qt?;
            let object = self.obj(Obj::Native(qt));
            return Some(self.arena.intern(Kind::Enum {
                object,
                name: name.into(),
            }));
        }
        if written.is_empty() || !oracle.has_enum(owner, written) {
            return None;
        }
        // The enum is named by the type that declares it: `Text.VAlignment`,
        // not that of a type built on it.
        let mut home = owner;
        let mut ty = owner;
        while let Some(next) = oracle.prototype(ty) {
            if oracle.has_enum(next, written) {
                home = next;
            }
            ty = next;
        }
        let object = self.obj(Obj::Native(home));
        Some(self.arena.intern(Kind::Enum {
            object,
            name: written.into(),
        }))
    }

    /// Whether `key` is a key of the enum `name` of the object `o`.
    fn has_key(&self, o: u32, name: &str, key: &str) -> bool {
        match self.kind_of(o) {
            Some(kind) => self.c.types.is_key(&kind, Some(name), key),
            None => true,
        }
    }

    /// What `name` is on the type `o` named as a value: `Text.AlignLeft`,
    /// `Image.FillMode`, an attached property, a singleton's member.
    fn type_member(&mut self, o: u32, name: &str) -> Option<Ty> {
        let kind = self.kind_of(o)?;
        let types = self.c.types;
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return match types.declared_enum(&kind, name) {
                Some(false) => Some(self.arena.intern(Kind::Key {
                    object: o,
                    key: name.into(),
                })),
                Some(true) => Some(self.arena.intern(Kind::EnumNs {
                    object: o,
                    name: name.into(),
                })),
                None => None,
            };
        }
        if types.is_singleton(&kind) {
            return self.kind_member(&kind, name).map(|own| own.ty);
        }
        let attached = types
            .base(&kind)
            .and_then(|base| types.oracle.attached(base))?;
        self.native_member(attached, name).map(|own| own.ty)
    }

    // Declarations.

    /// Gives each member a QML object of the file declares its type, before
    /// any script reads it.
    fn declare(&mut self, object: &QmlObject<'a>, owner: Option<usize>) {
        let index = if is_group(object) {
            owner
        } else {
            self.c.tree.index_of(object)
        };
        for member in &object.members {
            match member {
                QmlMember::Object(child) => self.declare(child, index),
                QmlMember::Binding(binding) => self.declare_value(&binding.value, index),
                QmlMember::Property(declaration) => {
                    if let (Some(index), Some(type_name)) = (index, &declaration.type_name) {
                        let written = type_name.name.to_string();
                        let ty = if written == "alias" {
                            Ty::ERROR
                        } else {
                            let types = self.c.types;
                            self.written(&written, type_name.is_list, &types)
                        };
                        let written = super::declared_name(&written, type_name.is_list);
                        self.own.insert(
                            (index, declaration.name.name.as_str()),
                            Own {
                                ty,
                                written: Some(written),
                            },
                        );
                    }
                    if let Some(value) = &declaration.value {
                        self.declare_value(value, index);
                    }
                }
                QmlMember::Signal(signal) => {
                    let Some(index) = index else { continue };
                    let types = self.c.types;
                    let params = signal
                        .params
                        .iter()
                        .map(|p| {
                            let ty = match &p.type_name {
                                Some(t) => self.written(&t.name.to_string(), t.is_list, &types),
                                None => Ty::ERROR,
                            };
                            param(p.name.name.as_str(), ty)
                        })
                        .collect();
                    let ty = self.signal_ty(params);
                    self.own.insert(
                        (index, signal.name.name.as_str()),
                        Own { ty, written: None },
                    );
                }
                QmlMember::Function(function) => {
                    let (Some(index), Some(id)) = (index, &function.id) else {
                        continue;
                    };
                    let ty = self.signature(&function.params, function.return_type.as_deref());
                    self.own
                        .insert((index, id.name.as_str()), Own { ty, written: None });
                }
                QmlMember::InlineComponent(inline) => self.declare(&inline.object, None),
                QmlMember::Enum(_) => {}
            }
        }
    }

    fn declare_value(&mut self, value: &QmlBindingValue<'a>, owner: Option<usize>) {
        match value {
            QmlBindingValue::Object(object) => self.declare(object, owner),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.declare(object, owner);
                }
            }
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
        }
    }

    // The QML walk.

    fn object(&mut self, object: &QmlObject<'a>) {
        let Some(index) = self.c.tree.index_of(object) else {
            return;
        };
        self.members(index, object, &[]);
    }

    fn members(&mut self, owner: usize, object: &QmlObject<'a>, group: &[&'a str]) {
        for member in &object.members {
            match member {
                QmlMember::Object(child) if is_group(child) => {
                    let mut path = group.to_vec();
                    path.extend(child.type_name.parts.iter().copied());
                    self.members(owner, child, &path);
                }
                QmlMember::Object(child) => self.object(child),
                QmlMember::Binding(binding) => {
                    let mut path = group.to_vec();
                    path.extend(binding.name.parts.iter().copied());
                    if path == ["id"] {
                        continue;
                    }
                    self.binding(owner, &path, &binding.value);
                }
                QmlMember::Property(declaration) => {
                    let Some(value) = &declaration.value else {
                        continue;
                    };
                    let name = declaration.name.name.as_str();
                    let target = match self.own.get(&(owner, name)) {
                        Some(own) if own.ty != Ty::ERROR => Some((own.ty, self.qml_word(own))),
                        _ => None,
                    };
                    self.value(name, target, value);
                }
                QmlMember::Function(function) => {
                    let declared = function
                        .id
                        .as_ref()
                        .and_then(|id| self.own.get(&(owner, id.name.as_str())))
                        .map(|own| own.ty);
                    self.function(function, declared);
                }
                QmlMember::InlineComponent(inline) => self.object(&inline.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    /// The type a property is, in the words QML declares it with: "a real",
    /// "a Text.VAlignment".
    fn qml_word(&self, own: &Own) -> String {
        if matches!(self.arena.kind(own.ty), Kind::Enum { .. }) {
            return self.word(own.ty);
        }
        match own.written.as_deref() {
            Some(written) if !written.is_empty() && !written.contains("::") => {
                let plain = plain(written);
                let plain = match plain {
                    "QString" => "string",
                    "QUrl" => "url",
                    "QColor" => "color",
                    "double" | "qreal" | "float" => "real",
                    "QVariant" | "variant" => "var",
                    other => other,
                };
                article(plain)
            }
            _ => self.word(own.ty),
        }
    }

    /// A member written on the object `owner`, by its path.
    fn binding(&mut self, owner: usize, path: &[&'a str], value: &QmlBindingValue<'a>) {
        let last = path[path.len() - 1];
        if let Some(signal) = handled(last) {
            let sig = self.handler_signal(owner, &path[..path.len() - 1], &signal);
            self.handler(sig, value);
            return;
        }
        let target = self.member_of_path(owner, path);
        let target = target
            .filter(|own| own.ty != Ty::ERROR)
            .map(|own| (own.ty, self.qml_word(&own)));
        self.value(last, target, value);
    }

    /// The property a binding's path names.
    fn member_of_path(&mut self, owner: usize, path: &[&str]) -> Option<Own> {
        let tree = self.c.tree;
        let changes =
            self.special(owner) == Some(Special::PropertyChanges) && !CHANGES.contains(&path[0]);
        let object = if changes { tree.target(owner)? } else { owner };
        if path[0].starts_with(|c: char| c.is_ascii_uppercase()) {
            let types = self.c.types;
            let qualified = path.len() >= 3
                && path[1].starts_with(|c: char| c.is_ascii_uppercase())
                && types.namespace(path[0]).is_some();
            let n = if qualified { 2 } else { 1 };
            let found = types.find(&path[..n])?;
            let attached = types
                .base(&found.kind)
                .and_then(|base| types.oracle.attached(base))?;
            let [name] = &path[n..] else {
                return None;
            };
            return self.native_member(attached, name);
        }
        let mut own = self.instance_member(object, path[0])?;
        for name in &path[1..] {
            let mut next = None;
            for part in self.arena.members(own.ty) {
                if let Kind::Object(o) = *self.arena.kind(part) {
                    next = self.object_member(o, name);
                }
            }
            own = next?;
        }
        Some(own)
    }

    fn special(&self, index: usize) -> Option<Special> {
        let kind = self.c.tree.objects[index].kind.as_ref()?;
        self.c.types.oracle.special(self.c.types.base(kind)?)
    }

    /// What `name` is on the object `o`.
    fn object_member(&mut self, o: u32, name: &str) -> Option<Own> {
        match self.objs[o as usize].clone() {
            Obj::Instance(index) => self.instance_member(index, name),
            Obj::Native(ty) => self.native_member(ty, name),
            Obj::Component(key) => self.kind_member(&QKind::Component(key), name),
        }
    }

    /// The type of the signal a handler written at `group` of `owner`
    /// handles.
    fn handler_signal(&mut self, owner: usize, group: &[&str], signal: &str) -> Option<Ty> {
        let types = self.c.types;
        let own = match group {
            [] => {
                let object = if self.special(owner) == Some(Special::Connections) {
                    self.c.tree.target(owner)?
                } else {
                    owner
                };
                self.instance_member(object, signal)?
            }
            ["Component"] if matches!(signal, "completed" | "destruction" | "reloaded") => {
                return Some(self.signal_ty(Vec::new()));
            }
            [first, ..] if first.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let found = types.find(group)?;
                let attached = types
                    .base(&found.kind)
                    .and_then(|base| types.oracle.attached(base))?;
                self.native_member(attached, signal)?
            }
            _ => {
                let mut path = group.to_vec();
                path.push(signal);
                self.member_of_path(owner, &path)?
            }
        };
        matches!(self.arena.kind(own.ty), Kind::Function(_)).then_some(own.ty)
    }

    /// A value given to a property, `target` its type and word where the
    /// checker knows it.
    fn value(&mut self, name: &str, target: Option<(Ty, String)>, value: &QmlBindingValue<'a>) {
        match value {
            QmlBindingValue::Expression(expression) => {
                let want = target.as_ref().map(|(ty, _)| *ty);
                let given = self.expr(expression, want);
                if let Some((ty, word)) = &target {
                    self.check_assign(name, *ty, word, given, expression.span());
                }
            }
            QmlBindingValue::Statement(statement) => {
                // A block is a function that gives the property its value.
                let target = target.map(|(ty, word)| (ty, (name.to_string(), word)));
                self.body(|checker| {
                    checker.fns.push(FnCtx {
                        ret: target.as_ref().map(|(ty, _)| *ty),
                        returns: Vec::new(),
                        target: target.map(|(_, words)| words),
                    });
                    checker.statement(statement);
                    checker.fns.pop();
                });
            }
            QmlBindingValue::Object(object) => self.object(object),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object);
                }
            }
        }
    }

    /// A handler, of the signal of type `sig` where it is known.
    fn handler(&mut self, sig: Option<Ty>, value: &QmlBindingValue<'a>) {
        let params: Vec<(Box<str>, Ty)> = match sig.map(|sig| self.arena.kind(sig)) {
            Some(Kind::Function(sig)) => {
                sig.params.iter().map(|p| (p.name.clone(), p.ty)).collect()
            }
            _ => Vec::new(),
        };
        match value {
            QmlBindingValue::Expression(
                expression @ (qml_syntax::js::Expression::ArrowFunctionExpression(_)
                | qml_syntax::js::Expression::FunctionExpression(_)),
            ) => {
                self.expr(expression, sig);
            }
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {
                self.body(|checker| {
                    // The arguments are in scope by their names.
                    for (name, ty) in &params {
                        let name: &'a str = checker.intern_name(name);
                        checker.bind(name, *ty, Binding::Param);
                    }
                    checker.fns.push(FnCtx {
                        ret: None,
                        returns: Vec::new(),
                        target: None,
                    });
                    match value {
                        QmlBindingValue::Expression(expression) => {
                            checker.expr(expression, None);
                        }
                        QmlBindingValue::Statement(statement) => {
                            checker.statement(statement);
                        }
                        _ => {}
                    }
                    checker.fns.pop();
                });
            }
            QmlBindingValue::Object(_) | QmlBindingValue::Objects(_) => {}
        }
    }

    /// A signal's argument names live as long as the oracle's tables, not
    /// the file: the file's text has each one written where a handler
    /// uses it, and a name it does not use need not be in scope.
    fn intern_name(&self, name: &str) -> &'a str {
        let source = self.c.source;
        let mut from = 0;
        while let Some(at) = source[from..].find(name) {
            let start = from + at;
            let end = start + name.len();
            let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
            let before = source[..start].chars().next_back().is_some_and(word);
            let after = source[end..].chars().next().is_some_and(word);
            if !before && !after {
                return &source[start..end];
            }
            from = end;
        }
        ""
    }

    /// Walks a function's body in its own scope: names it declares and
    /// facts it finds end with it, and facts from outside hold in it only
    /// for a `const`.
    fn body(&mut self, walk: impl FnOnce(&mut Self)) {
        let locals = self.locals.len();
        let facts = self.facts.len();
        let floor = self.floor;
        let assigned = self.assigned.len();
        self.floor = facts;
        walk(self);
        self.locals.truncate(locals);
        self.facts.truncate(facts);
        self.assigned.truncate(assigned);
        self.floor = floor;
    }

    // Names.

    fn bind(&mut self, name: &'a str, ty: Ty, binding: Binding) -> u32 {
        let id = self.next_local;
        self.next_local += 1;
        if !name.is_empty() {
            self.locals.push(Local {
                name,
                ty,
                binding,
                id,
            });
        }
        id
    }

    fn local(&self, name: &str) -> Option<Local<'a>> {
        self.locals
            .iter()
            .rev()
            .find(|local| local.name == name && local.binding != Binding::Type)
            .copied()
    }

    fn type_local(&self, name: &str) -> Option<Ty> {
        self.locals
            .iter()
            .rev()
            .find(|local| local.name == name && local.binding == Binding::Type)
            .map(|local| local.ty)
    }

    /// What a fact says the path `key` is, starting at `local`.
    fn fact(&self, key: &str, local: Option<u32>) -> Option<Ty> {
        for (at, fact) in self.facts.iter().enumerate().rev() {
            if fact.key != key || fact.local != local {
                continue;
            }
            if at < self.floor {
                // Outside the function, only a `const` stays what it was.
                let constant = local.is_some_and(|id| {
                    self.locals
                        .iter()
                        .any(|l| l.id == id && l.binding == Binding::Const)
                });
                if !constant {
                    return None;
                }
            }
            return Some(fact.ty);
        }
        None
    }

    // Assignability.

    /// Whether a value of type `source` may go where `target` is wanted.
    fn takes(&self, target: Ty, source: Ty) -> bool {
        if target == source
            || source == Ty::ERROR
            || target == Ty::ERROR
            || target == Ty::UNKNOWN
            || source == Ty::NEVER
        {
            return true;
        }
        let arena = &self.arena;
        if let Kind::Union(parts) = arena.kind(source) {
            return parts.iter().all(|part| self.takes(target, *part));
        }
        if let Kind::Union(parts) = arena.kind(target) {
            return parts.iter().any(|part| self.takes(*part, source));
        }
        match (arena.kind(target), arena.kind(source)) {
            (Kind::Number, Kind::Num(_) | Kind::Enum { .. } | Kind::Key { .. }) => true,
            (Kind::String | Kind::Color, Kind::String | Kind::Str(_) | Kind::Color) => true,
            (Kind::Boolean, Kind::Bool(_)) => true,
            (Kind::Undefined | Kind::Void, Kind::Undefined | Kind::Void) => true,
            (Kind::Enum { object, name }, Kind::Key { key, .. }) => {
                self.has_key(*object, name, key)
            }
            // DECISION: a number a script works out goes into an enum, as
            // Qt's flags are combined with `|`; a number written as a
            // literal is not a key.
            (Kind::Enum { .. }, Kind::Number | Kind::Enum { .. }) => true,
            (Kind::Object(t), Kind::Object(s)) => self.object_takes(*t, *s),
            (Kind::Record(tf), Kind::Record(sf)) => {
                tf.iter()
                    .all(|field| match sf.iter().find(|s| s.name == field.name) {
                        Some(given) => {
                            (field.optional || !given.optional) && self.takes(field.ty, given.ty)
                        }
                        None => field.optional,
                    })
            }
            (
                Kind::Array {
                    elem: te,
                    readonly: tr,
                },
                Kind::Array {
                    elem: se,
                    readonly: sr,
                },
            ) => (*tr || !*sr) && self.takes(*te, *se),
            (Kind::Map { key: tk, value: tv }, Kind::Map { key: sk, value: sv }) => {
                tk == sk && tv == sv
            }
            (Kind::Function(t), Kind::Function(s)) => {
                let params_ok = s.params.iter().enumerate().all(|(at, given)| {
                    let wanted = t.params.get(at).map(|p| p.ty).or(t.rest);
                    match wanted {
                        Some(wanted) => self.takes(given.ty, wanted),
                        None => given.optional,
                    }
                });
                params_ok && (t.ret == Ty::VOID || self.takes(t.ret, s.ret))
            }
            (Kind::Function(_), Kind::Method(..)) => true,
            _ => false,
        }
    }

    fn object_takes(&self, target: u32, source: u32) -> bool {
        let wanted = &self.objs[target as usize];
        if let Obj::Native(ty) = wanted
            && self.c.types.oracle.name(*ty) == "QtObject"
        {
            return true;
        }
        self.chain(source).contains(wanted)
    }

    /// Reports a value of type `given` going where `name`, of type
    /// `target`, said as `word`, is.
    fn check_assign(&mut self, name: &str, target: Ty, word: &str, given: Ty, span: Span) {
        if !self.takes(target, given) {
            let given = self.word(given);
            self.report(
                Code::TsAssignType,
                format!("`{name}` is {word}; this gives it {given}"),
                span,
            );
        }
    }
}

/// A QML type name without its module: `QtQml.real` is `real`.
fn plain(written: &str) -> &str {
    match written.rsplit_once('.') {
        Some((module, name))
            if module.starts_with(|c: char| c.is_ascii_uppercase())
                && name.starts_with(|c: char| c.is_ascii_lowercase()) =>
        {
            name
        }
        _ => written,
    }
}

/// The type of a QML value type that is a TypeScript primitive.
fn primitive(written: &str) -> Option<Ty> {
    Some(match written {
        "bool" | "boolean" => Ty::BOOLEAN,
        "int" | "real" | "double" | "number" | "qreal" | "float" => Ty::NUMBER,
        "string" | "QString" | "url" | "QUrl" => Ty::STRING,
        "color" | "QColor" => Ty::COLOR,
        "var" | "variant" | "unknown" | "QVariant" | "QJSValue" => Ty::UNKNOWN,
        // DECISION: TYPES.md has no `date`: what holds one is let by.
        "date" | "QDateTime" => Ty::ERROR,
        _ => return None,
    })
}

fn param(name: &str, ty: Ty) -> Param {
    Param {
        name: name.into(),
        ty,
        optional: false,
        written: None,
    }
}
