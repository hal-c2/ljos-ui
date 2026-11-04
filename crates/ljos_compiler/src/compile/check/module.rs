//! The TypeScript modules a file imports (docs/DIALECT.md §4.4), read by
//! the checker: what each exports, with its types, for the file that
//! reaches it through its import's name, and the module itself checked by
//! the same rules as any script of Ljós's API, its findings at its own
//! lines.
//!
//! A module is read once, the first time it is reached, in the checker of
//! the file that reaches it, so its types are of the file's own arena.

use qml_front::{
    TypeOracle,
    names::Target,
    project::{self, Source},
};
use qml_syntax::{
    Allocator, GetSpan, Span,
    ast::{QmlDocument, QmlImportSource},
    js::{
        Declaration, ImportDeclarationSpecifier, ImportOrExportKind, Program, Statement,
        VariableDeclarationKind,
    },
};

use super::ty::{Kind, Ty};
use super::{Binding, Checker};
use crate::{
    Code, Project,
    script::module::{self as erase, Fault},
};

/// A module a file reaches, parsed.
pub(in crate::compile) struct Module<'a> {
    pub(super) key: String,
    pub(super) source: &'a str,
    program: Program<'a>,
    /// What its erasure refuses: what is not only types and does not
    /// erase. A module that does not parse has its syntax error here, alone.
    faults: Vec<Fault>,
    broken: bool,
}

/// What a module gives its importers: the names it exports, as values and
/// as types, and the names it keeps, each with the word that declares it,
/// for a message that names the fix.
#[derive(Default)]
pub(super) struct Exports<'a> {
    values: Vec<(&'a str, Ty)>,
    types: Vec<(&'a str, Ty)>,
    own: Vec<(&'a str, &'static str)>,
}

/// How far a module is read.
pub(super) enum Read<'a> {
    Not,
    /// Being read: an import back to it is a cycle, whose names are let by.
    Reading,
    Done(Exports<'a>),
}

/// Where a name is looked for among what a module exports.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Space {
    Value,
    Type,
    /// An import names either.
    Either,
}

fn relative(path: &str) -> bool {
    path.starts_with("./") || path.starts_with("../")
}

/// The modules `document`, the file at `key`, reaches: those it imports,
/// then those they import, each once.
pub(in crate::compile) fn modules<'a, O: TypeOracle>(
    allocator: &'a Allocator,
    project: &'a Project<O>,
    key: &str,
    document: &QmlDocument<'a>,
) -> Vec<Module<'a>> {
    let mut keys: Vec<String> = Vec::new();
    for import in &document.imports {
        if let QmlImportSource::Path(path) = &import.source
            && erase::is_module(path)
        {
            let key = project.front().import_dir(key, path);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    let mut modules: Vec<Module<'a>> = Vec::new();
    let mut next = 0;
    while next < keys.len() {
        let key = keys[next].clone();
        next += 1;
        let Some(source) = project.script(&key) else {
            continue;
        };
        let parsed = oxc_parser::Parser::new(allocator, source, erase::source_type()).parse();
        let broken = !parsed.diagnostics.is_empty();
        let faults = erase::erase(&key, source).faults;
        let directory = project::directory(&key);
        for statement in &parsed.program.body {
            let path = match statement {
                Statement::ImportDeclaration(import) => import.source.value.as_str(),
                Statement::ExportFromDeclaration(export) => export.source.value.as_str(),
                _ => continue,
            };
            if relative(path) && erase::is_module(path) {
                let key = project::join(directory, path);
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        modules.push(Module {
            key,
            source,
            program: parsed.program,
            faults,
            broken,
        });
    }
    modules
}

/// The words for a list: "`a`", "`a` and `b`", "`a`, `b` and `c`".
pub(super) fn listed(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl<'c, 't, 'a, 'p, O: TypeOracle> Checker<'c, 't, 'a, 'p, O> {
    /// The module at `key`, read: None when the file reaches no module
    /// there, or it is being read.
    pub(super) fn module_at(&mut self, key: &str) -> Option<usize> {
        let index = self.modules.iter().position(|m| m.key == key)?;
        if matches!(self.read[index], Read::Not) {
            self.read_module(index);
        }
        matches!(self.read[index], Read::Done(_)).then_some(index)
    }

    /// The type of the module at `index` as a value: what `import * as`
    /// and a file's `import "x.ts" as` name.
    pub(super) fn module_value(&mut self, index: usize) -> Ty {
        let key = self.modules[index].key.as_str();
        self.arena.intern(Kind::Module(key.into()))
    }

    /// The module whose value `ty` is.
    pub(super) fn module_of(&self, ty: Ty) -> Option<usize> {
        match self.arena.kind(ty) {
            Kind::Module(key) => self.modules.iter().position(|m| *m.key == **key),
            _ => None,
        }
    }

    /// The module a QML import of the file names `as` `name`, at `at`.
    pub(super) fn imported_module(&mut self, name: &str, at: u32) -> Option<usize> {
        if self.module.is_some() {
            return None;
        }
        let Some(Target::Namespace(Source::Path(path))) = self.c.lookup(name, at)?.target else {
            return None;
        };
        if !erase::is_module(&path) {
            return None;
        }
        let key = self.c.project.front().import_dir(self.c.key, &path);
        self.module_at(&key)
    }

    /// The module a name of the script names: a module's `import * as`,
    /// or a file's `import "x.ts" as`.
    pub(super) fn named_module(&mut self, name: &str, at: u32) -> Option<usize> {
        if let Some(local) = self.local(name) {
            return self.module_of(local.ty);
        }
        self.imported_module(name, at)
    }

    /// What the module at `index` exports as `name`: its value and its
    /// type.
    pub(super) fn exported(&self, index: usize, name: &str) -> (Option<Ty>, Option<Ty>) {
        let Read::Done(exports) = &self.read[index] else {
            return (None, None);
        };
        let find = |list: &[(&str, Ty)]| list.iter().find(|(n, _)| *n == name).map(|(_, ty)| *ty);
        (find(&exports.values), find(&exports.types))
    }

    /// Reports `name`, written at `span`, as one the module at `index`
    /// does not export in `space`.
    pub(super) fn not_exported(&mut self, index: usize, name: &str, space: Space, span: Span) {
        let Read::Done(exports) = &self.read[index] else {
            return;
        };
        let key = &self.modules[index].key;
        let own = exports.own.iter().find(|(own, word)| {
            *own == name && (space == Space::Either || (space == Space::Type) == is_type_word(word))
        });
        let fix = match own {
            Some((_, word)) => format!("export it there (`export {word} {name}`)"),
            None => {
                let mut names: Vec<String> = Vec::new();
                if space != Space::Type {
                    names.extend(exports.values.iter().map(|(n, _)| format!("`{n}`")));
                }
                if space != Space::Value {
                    names.extend(exports.types.iter().map(|(n, _)| format!("`{n}`")));
                }
                names.dedup();
                match (space, names.is_empty()) {
                    (_, true) => "it exports nothing of the kind".to_string(),
                    (Space::Type, false) => format!("it exports the types {}", listed(&names)),
                    _ => format!("it exports {}", listed(&names)),
                }
            }
        };
        let (what, code) = match space {
            Space::Type => ("a type", Code::UnknownType),
            _ => ("", Code::UnknownMember),
        };
        let message = if what.is_empty() {
            format!("`{name}` is not exported by {key}: {fix}")
        } else {
            format!("`{name}` is not {what} {key} exports: {fix}")
        };
        self.report(code, message, span);
    }

    /// Reads the module at `index`: checks it, with its findings at its
    /// own lines, and keeps what it exports.
    fn read_module(&mut self, index: usize) {
        let modules = self.modules;
        let module = &modules[index];
        self.read[index] = Read::Reading;
        // The module is walked as a script of its own: nothing of the place
        // it was reached from holds in it.
        let source = std::mem::replace(&mut self.source, module.source);
        let this = self.this.take();
        let reading = self.module.replace(index);
        let quiet = std::mem::take(&mut self.quiet);
        let locals = std::mem::take(&mut self.locals);
        let facts = std::mem::take(&mut self.facts);
        let floor = std::mem::take(&mut self.floor);
        let assigned = std::mem::take(&mut self.assigned);
        let fns = std::mem::take(&mut self.fns);
        let nested = std::mem::take(&mut self.nested);
        let indexed = self.indexed.take();
        let code = if module.broken {
            Code::Syntax
        } else {
            Code::TsForbidden
        };
        for fault in &module.faults {
            self.report(code, fault.message.clone(), fault.span);
        }
        let exports = if module.broken {
            Exports::default()
        } else {
            self.module_body(module)
        };
        self.source = source;
        self.this = this;
        self.module = reading;
        self.quiet = quiet;
        self.locals = locals;
        self.facts = facts;
        self.floor = floor;
        self.assigned = assigned;
        self.fns = fns;
        self.nested = nested;
        self.indexed = indexed;
        self.read[index] = Read::Done(exports);
    }

    fn module_body(&mut self, module: &'c Module<'a>) -> Exports<'a> {
        let directory = project::directory(&module.key);
        let body = &module.program.body;
        for statement in body {
            if let Statement::ImportDeclaration(import) = statement {
                let path = import.source.value.as_str();
                let from = (relative(path) && erase::is_module(path))
                    .then(|| project::join(directory, path))
                    .and_then(|key| self.module_at(&key));
                let types_only = import.import_kind == ImportOrExportKind::Type;
                for specifier in import.specifiers.iter().flatten() {
                    self.import_specifier(specifier, from, types_only);
                }
            }
        }
        for statement in body {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => Some(&export.declaration),
                other => other.as_declaration(),
            };
            if let Some(declaration) = declaration {
                self.bind_ahead(declaration);
            }
        }
        for statement in body {
            match statement {
                Statement::ExportDeclaration(export) => self.walk_declaration(&export.declaration),
                Statement::ImportDeclaration(_)
                | Statement::ExportNamedDeclaration(_)
                | Statement::ExportFromDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
                | Statement::TSExportAssignment(_)
                | Statement::TSNamespaceExportDeclaration(_) => {}
                other => {
                    self.statement(other);
                }
            }
        }
        self.exports(module)
    }

    fn import_specifier(
        &mut self,
        specifier: &ImportDeclarationSpecifier<'a>,
        from: Option<usize>,
        types_only: bool,
    ) {
        match specifier {
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(it) => {
                let ty = from.map_or(Ty::ERROR, |index| self.module_value(index));
                self.bind(it.local.name.as_str(), ty, Binding::Const);
            }
            ImportDeclarationSpecifier::ImportDefaultSpecifier(it) => {
                self.bind(it.local.name.as_str(), Ty::ERROR, Binding::Const);
            }
            ImportDeclarationSpecifier::ImportSpecifier(it) => {
                let local = it.local.name.as_str();
                let Some(from) = from else {
                    self.bind(local, Ty::ERROR, Binding::Const);
                    return;
                };
                let imported = it.imported.name();
                let (value, ty) = self.exported(from, imported.as_str());
                if value.is_none() && ty.is_none() {
                    self.not_exported(from, imported.as_str(), Space::Either, it.imported.span());
                }
                if let Some(ty) = ty {
                    self.bind(local, ty, Binding::Type);
                }
                if !types_only && it.import_kind != ImportOrExportKind::Type {
                    self.bind(local, value.unwrap_or(Ty::ERROR), Binding::Const);
                }
            }
        }
    }

    /// Binds what `declaration` declares before it runs, as a block does:
    /// a function by its signature, a type by what it names, a `let` or
    /// `const` as not yet a value.
    pub(super) fn bind_ahead(&mut self, declaration: &Declaration<'a>) {
        match declaration {
            Declaration::VariableDeclaration(declaration)
                if declaration.kind != VariableDeclarationKind::Var =>
            {
                for declarator in &declaration.declarations {
                    self.bind_dead(&declarator.id);
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    let ty = self.signature(&function.params, function.return_type.as_deref());
                    self.bind(id.name.as_str(), ty, Binding::Function);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                let ty = self.annot(&alias.type_annotation);
                self.bind(alias.id.name.as_str(), ty, Binding::Type);
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                let ty = self.record(&interface.body.body);
                self.bind(interface.id.name.as_str(), ty, Binding::Type);
            }
            _ => {}
        }
    }

    /// Walks what an `export` declares. What does not erase is the
    /// erasure's fault, said already.
    fn walk_declaration(&mut self, declaration: &Declaration<'a>) {
        match declaration {
            Declaration::VariableDeclaration(declaration) => self.declaration(declaration),
            Declaration::FunctionDeclaration(function) => {
                self.nested += 1;
                self.function(function, None);
                self.nested -= 1;
            }
            Declaration::ClassDeclaration(class) => {
                let start = class.span.start;
                self.forbidden(
                    "class",
                    "write a function, or a component",
                    Span::new(start, start + 5),
                );
            }
            _ => {}
        }
    }

    /// What the module, its body walked, exports.
    fn exports(&mut self, module: &'c Module<'a>) -> Exports<'a> {
        let directory = project::directory(&module.key);
        let mut exports = Exports::default();
        for statement in &module.program.body {
            match statement {
                Statement::ExportDeclaration(export) => {
                    for (name, word) in declared(&export.declaration) {
                        self.export_local(&mut exports, name, name, !is_type_word(word));
                    }
                }
                Statement::ExportNamedDeclaration(export) => {
                    for specifier in &export.specifiers {
                        let values = export.export_kind != ImportOrExportKind::Type
                            && specifier.export_kind != ImportOrExportKind::Type;
                        let local = specifier.local.name().as_str();
                        let exported = specifier.exported.name().as_str();
                        self.export_local(&mut exports, exported, local, values);
                    }
                }
                Statement::ExportFromDeclaration(export) => {
                    let path = export.source.value.as_str();
                    let from = (relative(path) && erase::is_module(path))
                        .then(|| project::join(directory, path))
                        .and_then(|key| self.module_at(&key));
                    let Some(from) = from else { continue };
                    for specifier in &export.specifiers {
                        let local = specifier.local.name();
                        let exported = specifier.exported.name().as_str();
                        let (value, ty) = self.exported(from, local.as_str());
                        if value.is_none() && ty.is_none() {
                            self.not_exported(
                                from,
                                local.as_str(),
                                Space::Either,
                                specifier.local.span(),
                            );
                        }
                        let values = export.export_kind != ImportOrExportKind::Type
                            && specifier.export_kind != ImportOrExportKind::Type;
                        if let Some(value) = value.filter(|_| values) {
                            exports.values.push((exported, value));
                        }
                        if let Some(ty) = ty {
                            exports.types.push((exported, ty));
                        }
                    }
                }
                other => {
                    if let Some(declaration) = other.as_declaration() {
                        exports.own.extend(declared(declaration));
                    }
                }
            }
        }
        exports
    }

    /// Exports the module's own `local` as `name`: its type if it is one,
    /// and its value if `values` and it is one.
    fn export_local(&self, exports: &mut Exports<'a>, name: &'a str, local: &str, values: bool) {
        if let Some(ty) = self.type_local(local) {
            exports.types.push((name, ty));
        }
        if values && let Some(found) = self.local(local) {
            exports.values.push((name, found.ty));
        }
    }
}

/// Whether the word that declares a name declares a type.
fn is_type_word(word: &str) -> bool {
    matches!(word, "type" | "interface")
}

/// The names `declaration` declares at a module's top, each with the word
/// that declares it.
fn declared<'a>(declaration: &Declaration<'a>) -> Vec<(&'a str, &'static str)> {
    match declaration {
        Declaration::VariableDeclaration(it) => {
            let word = match it.kind {
                VariableDeclarationKind::Const => "const",
                VariableDeclarationKind::Var => "var",
                _ => "let",
            };
            it.declarations
                .iter()
                .flat_map(|d| d.id.get_binding_identifiers())
                .map(|id| (id.name.as_str(), word))
                .collect()
        }
        Declaration::FunctionDeclaration(it) => it
            .id
            .iter()
            .map(|id| (id.name.as_str(), "function"))
            .collect(),
        Declaration::TSTypeAliasDeclaration(it) => vec![(it.id.name.as_str(), "type")],
        Declaration::TSInterfaceDeclaration(it) => vec![(it.id.name.as_str(), "interface")],
        _ => Vec::new(),
    }
}
