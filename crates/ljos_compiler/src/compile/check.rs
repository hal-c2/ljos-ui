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
mod controller;
mod expr;
mod module;
mod shadow;
mod task;
mod ty;
mod web;

pub(super) use module::{Module, modules};

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
    js::Expression,
};

use super::Compiler;
use crate::{Code, Diagnostic};
use ty::{Arena, Kind, Param, Sig, Ty, article};

/// The module whose import makes a file one of Ljós's API.
pub(crate) const MODULE: &str = "Ljos.Ui";

/// The module of Ljós's API that arranges children.
pub(crate) const LAYOUT: &str = "Ljos.Layout";

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

/// Holds the scripts of a dialect file, and the modules it reaches, to
/// the checker's rules.
pub(super) fn document<'a, O: TypeOracle>(
    compiler: &mut Compiler<'_, 'a, '_, O>,
    document: &QmlDocument<'a>,
    modules: &[Module<'a>],
) {
    let (mut found, in_modules) = {
        let mut checker = Checker::new(compiler, modules);
        checker.declare(&document.root, None);
        checker.tasks(&document.root, None);
        checker.functions(&document.root, None);
        checker.object(&document.root);
        checker.read_all();
        (checker.found, checker.module_found)
    };
    compiler.modules = checked_modules(modules, in_modules);
    let map = &compiler.lines.map;
    // A rule of style is about what was written: the text a `match` was
    // lowered to is the compiler's, and a subject it repeats in each arm
    // is one place of the file.
    found.retain(|(code, _, span)| !(is_style(*code) && map.is_written(span.start)));
    found.sort_by_key(|(_, _, span)| map.start(span.start));
    found.dedup_by_key(|(_, _, span)| map.start(span.start));
    for (code, message, span) in found {
        let at = compiler.lines.map.start(span.start);
        // A name not in sight is the compiler's warning in Qt's QML; in
        // Ljós's API it has no type, or is left out of the subset, which is
        // the checker's error.
        if matches!(code, Code::TsNoAny | Code::TsForbidden) {
            compiler
                .diagnostics
                .retain(|d| !(d.code == Code::UnresolvedName && d.span.start == at));
        }
        if compiler.diagnostics.iter().any(|d| d.span.start == at) {
            continue;
        }
        let span = compiler.span(span);
        compiler
            .diagnostics
            .push(Diagnostic::error(code, message, span));
    }
}

/// The rules about how a script is written rather than what it does.
fn is_style(code: Code) -> bool {
    matches!(
        code,
        Code::TsNoNull | Code::TsForbidden | Code::TsNoVar | Code::TsNoCast
    )
}

/// Holds the modules a file in Qt's QML reaches to the checker's rules:
/// they are Ljós code, whichever file imports them.
pub(super) fn modules_only<'a, O: TypeOracle>(
    compiler: &mut Compiler<'_, 'a, '_, O>,
    modules: &[Module<'a>],
) {
    let found = module_findings(compiler, modules);
    compiler.modules = checked_modules(modules, found);
}

/// What the checker finds in `modules`: each finding as it is made, at the
/// offsets of the text it read, before [`checked_modules`] says one a place.
pub(super) fn module_findings<'a, O: TypeOracle>(
    compiler: &mut Compiler<'_, 'a, '_, O>,
    modules: &[Module<'a>],
) -> Vec<(usize, Code, String, Span)> {
    let mut checker = Checker::new(compiler, modules);
    checker.read_all();
    checker.module_found
}

/// Each module, with what was found in it at its own lines, once each.
fn checked_modules(
    modules: &[Module<'_>],
    found: Vec<(usize, Code, String, Span)>,
) -> Vec<crate::CheckedModule> {
    modules
        .iter()
        .enumerate()
        .map(|(index, module)| {
            // Found in what was read, put back where the module has it, as
            // a file's are (`document`), then what was said of it as
            // written.
            let mut here: Vec<(Code, String, Span)> = found
                .iter()
                .filter(|(at, code, _, span)| {
                    *at == index && !(is_style(*code) && module.map.is_written(span.start))
                })
                .map(|(_, code, message, span)| (*code, message.clone(), module.map.span(*span)))
                .chain(module.said.iter().cloned())
                .collect();
            here.sort_by_key(|(_, _, span)| span.start);
            here.dedup_by_key(|(_, _, span)| span.start);
            let lines = super::Lines::new(module.original);
            let diagnostics = here
                .into_iter()
                .map(|(code, message, span)| {
                    Diagnostic::error(code, message, lines.at(module.original, span))
                })
                .collect();
            crate::CheckedModule {
                key: module.key.clone(),
                diagnostics,
            }
        })
        .collect()
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

/// The string `object` binds `name` to: `Some(None)` when it is bound to
/// something other than a string written out.
fn string_binding<'a>(object: &QmlObject<'a>, name: &str) -> Option<Option<(&'a str, Span)>> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.as_simple() == Some(name) => {
            Some(match &binding.value {
                QmlBindingValue::Expression(Expression::StringLiteral(text)) => {
                    Some((text.value.as_str(), text.span))
                }
                _ => None,
            })
        }
        _ => None,
    })
}

/// What a name a script declares is bound to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Binding {
    Const,
    /// `using` or `await using`: a `const` whose value is disposed of when
    /// its block ends.
    Using,
    /// A name an `import` binds: the module's, which the importer reads
    /// and does not assign.
    Import,
    Let,
    Param,
    Function,
    /// A `let` or `const` before its declaration runs.
    Dead,
    /// A TypeScript type, `type T = …` or `interface T {}`.
    Type,
    /// A name an `is` binds (`x is { let y }`): the desugar declares it
    /// with `var` before the statement, and it holds a value only where
    /// the `is` was true. Its type is what the `is` gives it.
    IsBound,
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
    /// Whether it is `async`: what it returns is awaited.
    asynchronous: bool,
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
    /// The text of what is walked: the file's, or a module's.
    source: &'a str,
    /// `source`'s offsets to the text as written, which `match` and `is`
    /// were lowered from.
    map: &'c qml_syntax::SpanMap,
    /// The text as written.
    original: &'a str,
    /// The modules the file reaches, how far each is read, and the one
    /// being walked.
    modules: &'c [Module<'a>],
    read: Vec<module::Read<'a>>,
    module: Option<usize>,
    arena: Arena,
    objs: Vec<Obj<O::Type>>,
    found: Vec<(Code, String, Span)>,
    /// What was found in the modules, by module.
    module_found: Vec<(usize, Code, String, Span)>,
    /// Above 0, nothing is reported: a path is read again for a fact.
    quiet: u32,
    locals: Vec<Local<'a>>,
    next_local: u32,
    /// The `let`s and `const`s declared with no written type, by id:
    /// TypeScript narrows by an assertion only through names that have one
    /// (TS2775).
    inferred: Vec<u32>,
    facts: Vec<Fact<'a>>,
    /// The facts below it were found outside the function being walked:
    /// only those of a `const` hold inside it.
    floor: usize,
    /// Where the parameters of the function being walked start in
    /// `locals`, until its body's statements are walked: what an `is` in
    /// that block declares may not be one of them.
    params: Option<usize>,
    /// The facts assignments made, for where branches meet.
    assigned: Vec<Fact<'a>>,
    fns: Vec<FnCtx>,
    own: HashMap<(usize, &'a str), Own>,
    /// The types `declare_states` gave the file's `state`s.
    states: Vec<Ty>,
    /// The element read by index last: what it gives may be undefined.
    indexed: Option<Span>,
    /// The name a `match` or `is` pattern is, being typed: one not in
    /// scope there was likely meant to bind the value.
    pattern_name: Option<Span>,
    /// The QML object whose scope a script runs in: `this`.
    this: Option<usize>,
    /// Above 0, a script is in a `function` of its own, where `this` is
    /// whatever calls it.
    nested: u32,
    /// The `const`s without a written type whose value has literals
    /// written there, with those literals' types: a variable given one
    /// widens them, as given the literals.
    fresh: Vec<(u32, Ty)>,
    /// The file's Tasks whose `run` is typed, by object.
    tasks_typed: HashMap<usize, task::TaskSig>,
}

impl<'c, 't, 'a, 'p, O: TypeOracle> Checker<'c, 't, 'a, 'p, O> {
    fn new(c: &'c Compiler<'t, 'a, 'p, O>, modules: &'c [Module<'a>]) -> Self {
        Self {
            c,
            source: c.source,
            map: &c.lines.map,
            original: c.original,
            modules,
            read: modules.iter().map(|_| module::Read::Not).collect(),
            module: None,
            arena: Arena::new(),
            objs: Vec::new(),
            found: Vec::new(),
            module_found: Vec::new(),
            quiet: 0,
            locals: Vec::new(),
            next_local: 0,
            inferred: Vec::new(),
            facts: Vec::new(),
            floor: 0,
            params: None,
            assigned: Vec::new(),
            fns: Vec::new(),
            own: HashMap::new(),
            states: Vec::new(),
            indexed: None,
            pattern_name: None,
            this: None,
            nested: 0,
            fresh: Vec::new(),
            tasks_typed: HashMap::new(),
        }
    }

    fn report(&mut self, code: Code, message: String, span: Span) {
        if self.quiet > 0 {
            return;
        }
        match self.module {
            Some(index) => self.module_found.push((index, code, message, span)),
            None => self.found.push((code, message, span)),
        }
    }

    /// Reads each module the file reaches that its scripts did not.
    fn read_all(&mut self) {
        let modules = self.modules;
        for module in modules {
            self.module_at(&module.key);
        }
    }

    fn text(&self, span: Span) -> &'a str {
        &self.source[span.start as usize..span.end as usize]
    }

    /// Whether the desugar wrote what is at `at`, rather than copied it
    /// from the text as written.
    fn desugared(&self, at: u32) -> bool {
        self.map.is_written(at)
    }

    /// What `span` stands for in the text as written.
    fn as_written(&self, span: Span) -> &'a str {
        let span = self.map.span(span);
        &self.original[span.start as usize..span.end as usize]
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

    /// Reports the type the file writes at `span`, unless something it
    /// imports names it or it is a value type (`font`, `point`) that every
    /// file may write. A type the file is only handed, a signal's argument,
    /// is not the file's to name, and goes through [`written`](Self::written)
    /// alone.
    fn check_written(&mut self, written: &str, types: &Types<'_, O>, span: Span) {
        let one = written
            .strip_prefix("list<")
            .and_then(|rest| rest.strip_suffix('>'))
            .unwrap_or(written);
        // A dotted name is a namespace's or an enum's (`Text.HAlignment`),
        // which this does not judge.
        let name = plain(one);
        if one.contains('.')
            || primitive(name).is_some()
            || matches!(name, "var" | "variant" | "date" | "alias")
        {
            return;
        }
        if types.find(&[one]).is_some() {
            return;
        }
        let oracle = self.c.types.oracle;
        let is_value = ["QtQuick", "QtQml"]
            .iter()
            .filter_map(|module| oracle.module_type(module, name))
            .any(|ty| oracle.is_value(ty));
        if !is_value {
            self.report(
                Code::UnknownType,
                qml_front::unknown_type(oracle, one),
                span,
            );
        }
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
            guard: None,
        })
    }

    /// A function the checker knows nothing of: it takes anything.
    /// A method the type table names without types: TYPES.md has it take
    /// and give `unknown`.
    fn loose(&mut self) -> Ty {
        self.arena.function(Sig {
            params: Box::new([]),
            rest: Some(Ty::UNKNOWN),
            ret: Ty::UNKNOWN,
            guard: None,
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
    /// The type table gives an Item's `parent` as a QtObject, the type of
    /// the object it is owned by; what a script reads there is the visual
    /// parent, an Item.
    ///
    /// DECISION: an Item's `parent` is an Item, as Qt documents it.
    fn item_parent(&self, ty: O::Type, name: &str) -> Option<O::Type> {
        if name != "parent" {
            return None;
        }
        let oracle = self.c.types.oracle;
        let item = oracle.module_type("QtQuick", "Item")?;
        let mut at = Some(ty);
        for _ in 0..64 {
            let here = at?;
            if here == item {
                return Some(item);
            }
            at = oracle.prototype(here);
        }
        None
    }

    fn native_member(&mut self, ty: O::Type, name: &str) -> Option<Own> {
        let oracle = self.c.types.oracle;
        if let Some(property) = oracle.property(ty, name) {
            let written = oracle.property_type(ty, name);
            let text = written.as_deref().unwrap_or_default();
            let one = if let Some(primitive) = primitive(plain(text)) {
                primitive
            } else if let Some(value) = property.value {
                let value = self.item_parent(ty, name).unwrap_or(value);
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
            // arguments: it takes what the checker lets by. It gives what
            // the table says it does (`Screen.points` a number), else
            // `unknown`.
            let ret = oracle
                .method_returns(ty, name)
                .and_then(|written| primitive(&written))
                .unwrap_or(Ty::UNKNOWN);
            let ty = self.arena.function(Sig {
                params: Box::new([]),
                rest: Some(Ty::UNKNOWN),
                ret,
                guard: None,
            });
            return Some(Own { ty, written: None });
        }
        // Beside Qt's table, what the runtime adds to one of Qt's types
        // (`font.size`, `Screen.points`) is in Ljós's own description of
        // it, and holds there what it holds without Qt's table.
        oracle
            .additions(ty)
            .into_iter()
            .find_map(|added| self.native_member(added, name))
    }

    /// The enum a property of `owner` declared as `written` holds.
    fn enum_of(&mut self, owner: O::Type, written: &str) -> Option<Ty> {
        let oracle = self.c.types.oracle;
        if let Some((scope, name)) = written.split_once("::") {
            // A class other than `Qt` that names the enum may be one the
            // owner is extended with: Qt's table says `Accessible.role` is
            // a `QAccessible::Role`, whose keys are `Accessible.Button`.
            if scope != "Qt"
                && let Some(home) = oracle.enum_home(owner, name)
            {
                let object = self.obj(Obj::Native(home));
                return Some(self.arena.intern(Kind::Enum {
                    object,
                    name: name.into(),
                }));
            }
            let qt = self.c.qt?;
            let object = self.obj(Obj::Native(qt));
            return Some(self.arena.intern(Kind::Enum {
                object,
                name: name.into(),
            }));
        }
        if written.is_empty() {
            return None;
        }
        // The enum is named by the type that declares it: `Text.VAlignment`,
        // not that of a type built on it, but `TextEdit.HAlignment`, which
        // `TextEdit` declares again, for the `horizontalAlignment` it has
        // from the base it shares with `TextInput`.
        let home = oracle.enum_home(owner, written)?;
        let object = self.obj(Obj::Native(home));
        Some(self.arena.intern(Kind::Enum {
            object,
            name: written.into(),
        }))
    }

    /// Whether `key` of the type `from` is the Ljós name of a key of Qt's
    /// enum `qt`: `MouseButton.Left` goes where Qt's `MouseButtons` does
    /// (docs/DIALECT.md §4.3.1).
    fn is_ljos_key(&self, qt: &str, from: u32, key: &str) -> bool {
        let Some(ljos) = qt_types::ljos_enums::of_qt_enum(qt) else {
            return false;
        };
        let Obj::Native(ty) = &self.objs[from as usize] else {
            return false;
        };
        self.c.types.oracle.name(*ty) == ljos.type_name()
            && ljos.keys.iter().any(|(_, own, _)| *own == key)
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
        if let Some(ty) = self.type_static(&kind, name) {
            return Some(ty);
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
                        let name = declaration.name.name.as_str();
                        let ty = if written == "alias" {
                            // DECISION: an alias is let by: its type is its
                            // target's, which the compiler resolves.
                            Ty::ERROR
                        } else if matches!(plain(&written), "var" | "variant") {
                            self.untyped(name, "write `property unknown` or a type", type_name.span)
                        } else if matches!(plain(&written), "date") {
                            self.untyped(
                                name,
                                "declare it as a number of milliseconds or a string",
                                type_name.span,
                            )
                        } else if let Some(ty) =
                            self.module_type(&type_name.name.parts, type_name.name.span)
                        {
                            if type_name.is_list {
                                self.arena.array(ty, true)
                            } else {
                                ty
                            }
                        } else {
                            let types = self.c.types;
                            self.check_written(&written, &types, type_name.name.span);
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
                            let name = p.name.name.as_str();
                            let ty = match &p.type_name {
                                Some(t) => match self.module_type(&t.name.parts, t.name.span) {
                                    Some(ty) if t.is_list => self.arena.array(ty, true),
                                    Some(ty) => ty,
                                    None => {
                                        let written = t.name.to_string();
                                        self.check_written(&written, &types, t.name.span);
                                        self.written(&written, t.is_list, &types)
                                    }
                                },
                                None => self.untyped(
                                    name,
                                    &format!("write its type (`{name}: real`, say)"),
                                    p.name.span,
                                ),
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
        if let Some(index) = index
            && !is_group(object)
        {
            self.declare_states(object, index);
        }
    }

    /// Gives an object whose `states` the file writes a `state` of the
    /// names they are given, and the empty one: the only values it takes
    /// (docs/DIALECT.md §4.9). An `extend` that names another is reported.
    ///
    /// DECISION: a state named by an expression leaves `state` a string,
    /// and so does an object whose states are declared in another file:
    /// what they are is known only as it runs, or there.
    fn declare_states(&mut self, object: &QmlObject<'a>, index: usize) {
        let Some(value) = object.members.iter().find_map(|member| match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("states") => {
                Some(&binding.value)
            }
            _ => None,
        }) else {
            return;
        };
        let states: &[QmlObject<'a>] = match value {
            QmlBindingValue::Object(state) => std::slice::from_ref(state),
            QmlBindingValue::Objects(states) => states,
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => return,
        };
        let mut names = vec![""];
        for state in states {
            if state.type_name.as_simple() != Some("State") {
                return;
            }
            match string_binding(state, "name") {
                Some(Some((name, _))) => names.push(name),
                Some(None) => return,
                // A state without a name applies by its `when`.
                None => {}
            }
        }
        let parts: Vec<Ty> = names.iter().map(|name| self.arena.str(name)).collect();
        let ty = self.arena.union(parts);
        for state in states {
            if let Some(Some((extend, span))) = string_binding(state, "extend")
                && !names.contains(&extend)
            {
                let given = self.arena.str(extend);
                let display = |ty| self.arena.display(ty, &|o| self.obj_name(o));
                let (given, all) = (display(given), display(ty));
                self.report(
                    Code::TsAssignType,
                    format!("`extend` names `{given}`, and the states are `{all}`"),
                    span,
                );
            }
        }
        self.states.push(ty);
        self.own.insert(
            (index, "state"),
            Own {
                ty,
                written: Some("string".into()),
            },
        );
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

    /// Walks the functions of the file's objects before their bindings, so
    /// that a call has the return type a function's body gives.
    ///
    /// DECISION: a function with no return type written that calls another
    /// such function declared after it gets the other's return type as
    /// let by; TypeScript reads them in any order.
    fn functions(&mut self, object: &QmlObject<'a>, owner: Option<usize>) {
        let index = if is_group(object) {
            owner
        } else {
            self.c.tree.index_of(object)
        };
        for member in &object.members {
            match member {
                QmlMember::Object(child) => self.functions(child, index),
                QmlMember::Binding(binding) => self.functions_value(&binding.value, index),
                QmlMember::Property(declaration) => {
                    if let Some(value) = &declaration.value {
                        self.functions_value(value, index);
                    }
                }
                QmlMember::Function(function) => {
                    let Some(index) = index else { continue };
                    let name = function.id.as_ref().map(|id| id.name.as_str());
                    let outer = self.this.replace(index);
                    let ty = self.function(function, None);
                    self.this = outer;
                    if let Some(name) = name {
                        self.own.insert((index, name), Own { ty, written: None });
                    }
                }
                QmlMember::InlineComponent(inline) => self.functions(&inline.object, None),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn functions_value(&mut self, value: &QmlBindingValue<'a>, owner: Option<usize>) {
        match value {
            QmlBindingValue::Object(object) => self.functions(object, owner),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.functions(object, owner);
                }
            }
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
        }
    }

    fn members(&mut self, owner: usize, object: &QmlObject<'a>, group: &[&'a str]) {
        let outer = self.this.replace(owner);
        self.members_of(owner, object, group);
        self.this = outer;
    }

    fn members_of(&mut self, owner: usize, object: &QmlObject<'a>, group: &[&'a str]) {
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
                // Walked first, by `functions`.
                QmlMember::Function(_) => {}
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
        if path == ["args"] && self.task_args(owner, value) {
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
                        asynchronous: false,
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
                    // The arguments are in scope by their names. One the
                    // table leaves unnamed (`Tap.tapped`'s button) is in
                    // scope by none: the handler reaches it only by
                    // declaring it, so a name of its own is free.
                    for (name, ty) in params.iter().filter(|(name, _)| !name.is_empty()) {
                        let name: &'a str = checker.intern_name(name);
                        checker.bind(name, *ty, Binding::Param);
                    }
                    checker.fns.push(FnCtx {
                        ret: None,
                        returns: Vec::new(),
                        target: None,
                        asynchronous: false,
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
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
        // Each match is visited once, so the search ends on any name: an
        // empty one, matched at every character, once looped for ever
        // (#394).
        source
            .match_indices(name)
            .map(|(start, _)| (start, start + name.len()))
            .find(|&(start, end)| {
                end > start
                    && !source[..start].chars().next_back().is_some_and(word)
                    && !source[end..].chars().next().is_some_and(word)
            })
            .map_or("", |(start, end)| &source[start..end])
    }

    /// Walks a function's body in its own scope: names it declares and
    /// facts it finds end with it, and facts from outside hold in it only
    /// for a `const`.
    fn body(&mut self, walk: impl FnOnce(&mut Self)) {
        let locals = self.locals.len();
        let facts = self.facts.len();
        let floor = self.floor;
        let assigned = self.assigned.len();
        let params = self.params.replace(locals);
        self.floor = facts;
        walk(self);
        self.locals.truncate(locals);
        self.facts.truncate(facts);
        self.assigned.truncate(assigned);
        self.floor = floor;
        self.params = params;
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

    /// What an `is` gives the name it binds, `id`: each `is` that binds
    /// it adds to its type.
    fn bound(&mut self, id: u32, ty: Ty) {
        if let Some(index) = self.locals.iter().rposition(|local| local.id == id) {
            let now = self.locals[index].ty;
            self.locals[index].ty = self.arena.or(now, ty);
        }
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
                    self.locals.iter().any(|l| {
                        l.id == id
                            && matches!(
                                l.binding,
                                Binding::Const | Binding::Using | Binding::Import
                            )
                    })
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

    /// Whether `a` and `b` are strings written out that share none: a
    /// `state` of the names declared compared with one that is not.
    ///
    /// DECISION: only literal strings are said never to be equal, as
    /// TypeScript says of types with no overlap; the rest is let by.
    fn never_equal(&self, a: Ty, b: Ty) -> bool {
        let strings = |ty: Ty| {
            let parts = self.arena.parts(ty);
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|part| matches!(self.arena.kind(*part), Kind::Str(_)))
        };
        strings(a)
            && strings(b)
            && !self
                .arena
                .parts(a)
                .iter()
                .any(|part| self.arena.parts(b).contains(part))
    }

    /// Whether `given` goes into a `state` typed `target`: a name the
    /// file declares, or a string a script holds, which names one only as
    /// it runs.
    ///
    /// DECISION: a `string` is let into `state`, as `never_equal` lets a
    /// comparison by: Qt's apps keep the next state in a string property
    /// (coffee's `previousState`, lightningviewer's `close_state`). Only a
    /// name written out that the file does not declare is refused.
    fn names_a_state(&self, target: Ty, given: Ty) -> bool {
        self.states.contains(&target)
            && self.arena.parts(given).iter().all(|part| {
                matches!(self.arena.kind(*part), Kind::String)
                    || self.arena.parts(target).contains(part)
            })
    }

    /// Whether a value of type `source` may go where `target` is wanted.
    fn takes(&self, target: Ty, source: Ty) -> bool {
        self.takes_as(target, source, true)
    }

    /// `takes`, where `strict` holds a mutable array's elements and a
    /// function's parameters to their variance; without it, they are taken
    /// as TypeScript takes an array's and a method's, so that a message can
    /// say what variance alone refuses.
    fn takes_as(&self, target: Ty, source: Ty, strict: bool) -> bool {
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
            return parts
                .iter()
                .all(|part| self.takes_as(target, *part, strict));
        }
        if let Kind::Union(parts) = arena.kind(target) {
            return parts
                .iter()
                .any(|part| self.takes_as(*part, source, strict));
        }
        match (arena.kind(target), arena.kind(source)) {
            (Kind::Number, Kind::Num(_) | Kind::Enum { .. } | Kind::Key { .. }) => true,
            (Kind::String | Kind::Color, Kind::String | Kind::Str(_) | Kind::Color) => true,
            (Kind::Boolean, Kind::Bool(_)) => true,
            (
                Kind::NonPrimitive,
                Kind::Object(_)
                | Kind::Record(_)
                | Kind::Array { .. }
                | Kind::Map { .. }
                | Kind::Function(_)
                | Kind::Method(..)
                | Kind::Promise(_)
                | Kind::Web(_),
            ) => true,
            (Kind::Promise(t), Kind::Promise(s)) => self.takes_as(*t, *s, strict),
            (Kind::Undefined | Kind::Void, Kind::Undefined | Kind::Void) => true,
            (Kind::NonNullish, source) => !matches!(
                source,
                Kind::Unknown | Kind::Null | Kind::Undefined | Kind::Void
            ),
            (Kind::Enum { object, name }, Kind::Key { object: from, key }) => {
                self.has_key(*object, name, key) || self.is_ljos_key(name, *from, key)
            }
            // DECISION: a number a script works out goes into an enum, as
            // Qt's flags are combined with `|`; a number written as a
            // literal is not a key.
            (Kind::Enum { .. }, Kind::Number | Kind::Enum { .. }) => true,
            (Kind::Object(t), Kind::Object(s)) => self.object_takes(*t, *s),
            (Kind::Record(tf), Kind::Record(sf)) => {
                tf.iter()
                    .all(|field| match sf.iter().find(|s| s.name == field.name) {
                        // A readonly member does not go where it may be
                        // written: TypeScript lets it, and the write lands
                        // in what was readonly.
                        Some(given) => {
                            (field.optional || !given.optional)
                                && (field.readonly || !given.readonly || !strict)
                                && self.takes_as(field.ty, given.ty, strict)
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
            ) => {
                (*tr || !*sr)
                    && self.takes_as(*te, *se, strict)
                    && (*tr || !strict || self.takes_as(*se, *te, strict))
            }
            (Kind::Map { key: tk, value: tv }, Kind::Map { key: sk, value: sv }) => {
                tk == sk && tv == sv
            }
            (Kind::Function(t), Kind::Function(s)) => {
                let params_ok = s.params.iter().enumerate().all(|(at, given)| {
                    let wanted = t.params.get(at).map(|p| p.ty).or(t.rest);
                    match wanted {
                        Some(wanted) => {
                            self.takes_as(given.ty, wanted, strict)
                                || (!strict && self.takes_as(wanted, given.ty, strict))
                        }
                        None => given.optional,
                    }
                });
                params_ok && (t.ret == Ty::VOID || self.takes_as(t.ret, s.ret, strict))
            }
            (Kind::Function(_), Kind::Method(..)) => true,
            _ => false,
        }
    }

    /// The readonly member of `given` that `target` would let be written.
    fn readonly_given(&self, target: Ty, given: Ty) -> Option<String> {
        let (Kind::Record(wanted), Kind::Record(fields)) =
            (self.arena.kind(target), self.arena.kind(given))
        else {
            return None;
        };
        wanted.iter().find_map(|field| {
            let f = fields.iter().find(|f| f.name == field.name)?;
            if f.readonly && !field.readonly {
                Some(f.name.to_string())
            } else {
                self.readonly_given(field.ty, f.ty)
            }
        })
    }

    /// Whether what variance alone refuses of `given`, where `target` is
    /// wanted, is a function's parameter rather than an array's element.
    fn variance_in_function(&self, target: Ty, given: Ty) -> bool {
        match (self.arena.kind(target), self.arena.kind(given)) {
            (Kind::Function(_), _) => true,
            (Kind::Record(wanted), Kind::Record(fields)) => wanted.iter().any(|field| {
                fields.iter().any(|f| {
                    f.name == field.name
                        && !self.takes(field.ty, f.ty)
                        && self.variance_in_function(field.ty, f.ty)
                })
            }),
            (Kind::Array { elem: te, .. }, Kind::Array { elem: se, .. }) => {
                self.takes(*te, *se) && self.variance_in_function(*te, *se)
            }
            _ => false,
        }
    }

    fn object_takes(&self, target: u32, source: u32) -> bool {
        let wanted = &self.objs[target as usize];
        if let Obj::Native(ty) = wanted
            && matches!(self.c.types.oracle.name(*ty), "QtObject" | "Object")
        {
            return true;
        }
        self.chain(source).contains(wanted)
    }

    /// Reports a value of type `given` going where `name`, of type
    /// `target`, said as `word`, is.
    /// Reports what has no type, and how to give it one.
    pub(super) fn untyped(&mut self, what: &str, fix: &str, span: Span) -> Ty {
        self.report(Code::TsNoAny, format!("`{what}` has no type: {fix}"), span);
        Ty::ERROR
    }

    /// Reports a value of type `given`, written at `span`, going where
    /// `name`, of type `target` said as `word`, is: by the rule that
    /// refuses it.
    fn check_assign(&mut self, name: &str, target: Ty, word: &str, given: Ty, span: Span) {
        if self.takes(target, given) || self.names_a_state(target, given) {
            return;
        }
        if self.indexed == Some(span) {
            let defined = self.arena.without_undefined(given);
            if self.takes(target, defined) {
                let text = self.text(span);
                self.report(
                    Code::TsIndexUndefined,
                    format!(
                        "`{text}` may be undefined, as an index may be past the end: check it first"
                    ),
                    span,
                );
                return;
            }
        }
        let mut given_word = self.word(given);
        // Literal types say nothing in words: "a string" for `"todo"` and
        // `"todo" | "doing"` alike. They are written out instead, as
        // TypeScript writes them, and so is what a place of literals is
        // given.
        let written;
        let word = if given_word == word || self.has_literal(target) {
            let display = |ty| format!("`{}`", self.arena.display(ty, &|o| self.obj_name(o)));
            given_word = display(given);
            written = display(target);
            written.as_str()
        } else {
            word
        };
        if self.takes_as(target, given, false) {
            let functions = matches!(
                (self.arena.kind(target), self.arena.kind(given)),
                (Kind::Function(_), Kind::Function(_))
            );
            let (word, given_word) = if functions {
                let display = |ty| format!("`{}`", self.arena.display(ty, &|o| self.obj_name(o)));
                (display(target), display(given))
            } else {
                (word.to_string(), given_word)
            };
            if let Some(member) = self.readonly_given(target, given) {
                self.report(
                    Code::TsReadonly,
                    format!(
                        "`{name}` may have its `{member}` written; this gives it one whose `{member}` is readonly: build a new object, or make the place readonly"
                    ),
                    span,
                );
                return;
            }
            let why = if self.variance_in_function(target, given) {
                "and a function must take everything its place may be given"
            } else {
                "and a mutable array holds only its own type: copy it, or make the place readonly"
            };
            self.report(
                Code::TsVariance,
                format!("`{name}` is {word}; this gives it {given_word}, {why}"),
                span,
            );
            return;
        }
        if let Some((field, want, got)) = self.field_refused(target, given) {
            let want = self.word(want);
            let message = match got {
                Some(got) => format!(
                    "`{name}.{field}` is {want}; this gives it {}",
                    self.word(got)
                ),
                None => format!("`{name}.{field}` is {want}, and this leaves it out"),
            };
            self.report(Code::TsAssignType, message, span);
            return;
        }
        self.report(
            Code::TsAssignType,
            format!("`{name}` is {word}; this gives it {given_word}"),
            span,
        );
    }

    /// For an object given where one of another object type goes, the
    /// first field of the place it does not fill: its name, its type, and
    /// what is given for it, if anything.
    fn field_refused(&self, target: Ty, given: Ty) -> Option<(Box<str>, Ty, Option<Ty>)> {
        let (Kind::Record(want), Kind::Record(got)) =
            (self.arena.kind(target), self.arena.kind(given))
        else {
            return None;
        };
        for field in want.iter() {
            match got.iter().find(|g| g.name == field.name) {
                Some(g) if !self.takes(field.ty, g.ty) => {
                    return Some((field.name.clone(), field.ty, Some(g.ty)));
                }
                None if !field.optional => return Some((field.name.clone(), field.ty, None)),
                _ => {}
            }
        }
        None
    }

    /// Whether `ty` has a string or number literal among its parts.
    fn has_literal(&self, ty: Ty) -> bool {
        self.arena
            .members(ty)
            .into_iter()
            .any(|part| matches!(self.arena.kind(part), Kind::Str(_) | Kind::Num(_)))
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

#[cfg(test)]
mod tests {
    use qml_front::QtTypeOracle;
    use qml_front::scope::Tree;
    use qml_syntax::Allocator;

    use super::super::Lines;
    use super::*;
    use crate::Project;

    /// The `ts-is-binding` findings in the module `source`, read as
    /// [`crate::check_modules`] reads one: through the desugar, so the map of what
    /// it wrote is the compiler's own. Each is as the checker made it,
    /// before the report says one a place, as (line, column, the text at
    /// its span, message), in the order of the file.
    fn is_bindings(source: &str) -> Vec<(usize, usize, String, String)> {
        let mut project = Project::new(QtTypeOracle);
        project.add_script("pick.lts", source);
        let allocator = Allocator::default();
        let root = allocator.alloc_str("import \"pick.lts\" as M0\nQtObject {}\n");
        let qml_syntax::Lowered {
            text, parsed, map, ..
        } = qml_syntax::parse_lowered(&allocator, root);
        let types = Types::new(&project.front, &project.oracle, "");
        let tree = Tree::analyze(&parsed.document, types, &mut Vec::new());
        let lines = Lines::lowered(root, map);
        let mut compiler = Compiler::new(&project, "", root, text, lines, types, &tree);
        let modules = modules(&allocator, &project, "", &parsed.document, false);
        assert_eq!(modules.len(), 1, "the root reaches the module");
        let mut found: Vec<_> = module_findings(&mut compiler, &modules)
            .into_iter()
            .filter(|(_, code, _, _)| *code == Code::TsIsBinding)
            .map(|(at, _, message, span)| {
                let module = &modules[at];
                let span = module.map.span(span);
                let (start, end) = (span.start as usize, span.end as usize);
                let before = &module.original[..start];
                let line = before.matches('\n').count() + 1;
                let column = start - before.rfind('\n').map_or(0, |at| at + 1) + 1;
                (
                    line,
                    column,
                    module.original[start..end].to_string(),
                    message,
                )
            })
            .collect();
        // In the order of the file, not of the checker's walk.
        found.sort();
        found
    }

    /// The walk that finds an `is` a scope between takes tells the blocks
    /// and functions the desugar wrote from those a person wrote by the
    /// desugar's own map. A block a person wrote declares what an `is` in
    /// it binds, so the walk leaves it to that block: a clash there is said
    /// once, by the block, and a block that shadows the name is fine. A
    /// block the desugar wrote for a `match` arm is between the arm's `is`
    /// and the `let` before the `match`, so a name the arm binds is said.
    #[test]
    fn the_walk_tells_the_blocks_the_desugar_wrote_from_those_written_by_hand() {
        let source = "\
const v: number = 7
if (v is let h) { }
{
    const h = 1
    if (v is let h) { }
}
{
    if (v is let h) { }
}
export const r: number = match (v) { when let h and if (v is let h): h; default: 0; }
export const s: number = match (v) { when let k and if (v is let m): k + m; default: 0; }
";
        let block = "`h` is a name of this block already, and an `is` declares what it binds in the block around it: bind another name";
        let arm = "`h` is a name this `match` arm binds already, and an `is` in the arm declares what it binds before the `match`: bind another name";
        assert_eq!(
            is_bindings(source),
            [
                (5, 18, "h".to_string(), block.to_string()),
                (10, 66, "h".to_string(), arm.to_string()),
            ]
        );
    }
}
