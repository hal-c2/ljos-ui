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
        BindingPattern, Declaration, ImportDeclarationSpecifier, ImportOrExportKind, Program,
        Statement, VariableDeclarationKind,
    },
};

use oxc_span::SourceType;

use super::ty::{Kind, Ty};
use super::{Binding, Checker};
use crate::{
    Code, Project,
    script::module::{self as erase, Fault},
};

/// A module a file reaches, parsed.
pub(in crate::compile) struct Module<'a> {
    pub(super) key: String,
    /// The text the checker reads: an `.lts` module's with `match` and
    /// `is` lowered.
    pub(super) source: &'a str,
    /// `source`'s offsets to the module's own.
    pub(super) map: qml_syntax::SpanMap,
    /// The module as written.
    pub(super) original: &'a str,
    program: Program<'a>,
    /// What its erasure refuses: what is not only types and does not
    /// erase. A module that does not parse has its syntax error here, alone.
    faults: Vec<Fault>,
    broken: bool,
    /// The declarations of a JavaScript file (`calc.d.ts`): types alone,
    /// with nothing that runs.
    declarations: bool,
    /// What is found in it before it is read, at `original`'s offsets: a
    /// `.ts` module's `match`, which is the syntax of an `.lts` one.
    pub(super) said: Vec<(Code, String, Span)>,
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

/// The key of the module an import at `path`, relative to `directory`,
/// reaches: a TypeScript module, or the declarations of a JavaScript file,
/// whether or not the project has them.
fn reached(directory: &str, path: &str) -> Option<String> {
    if !relative(path) {
        return None;
    }
    let key = project::join(directory, path);
    if erase::is_module(path) {
        Some(key)
    } else {
        erase::declarations_of(&key)
    }
}

/// The modules `document`, the file at `key`, reaches: those it imports,
/// then those they import, each once. A file in Ljós's API, `dialect`,
/// reaches the JavaScript files it imports through their declarations too.
pub(in crate::compile) fn modules<'a, O: TypeOracle>(
    allocator: &'a Allocator,
    project: &'a Project<O>,
    key: &str,
    document: &QmlDocument<'a>,
    dialect: bool,
) -> Vec<Module<'a>> {
    let mut keys: Vec<String> = Vec::new();
    // The scripts the file imports, and those they import: a module one
    // of them imports runs as the file does.
    let mut scripts: Vec<String> = Vec::new();
    for import in &document.imports {
        if let QmlImportSource::Path(path) = &import.source
            && project::is_script(path)
        {
            let key = project.front().import_dir(key, path);
            if dialect
                && let Some(declarations) = erase::declarations_of(&key)
                && project.declaration(&declarations).is_some()
                && !keys.contains(&declarations)
            {
                keys.push(declarations);
            }
            let list = if erase::is_module(path) {
                &mut keys
            } else {
                &mut scripts
            };
            if !list.contains(&key) {
                list.push(key);
            }
        }
    }
    let mut next = 0;
    while next < scripts.len() {
        let script = scripts[next].clone();
        next += 1;
        let Some(source) = project.script(&script) else {
            continue;
        };
        let directory = project::directory(&script);
        for path in crate::script::script_imports(source) {
            let key = project::join(directory, path);
            let list = if erase::is_module(path) {
                &mut keys
            } else {
                &mut scripts
            };
            if !list.contains(&key) {
                list.push(key);
            }
        }
    }
    let mut modules: Vec<Module<'a>> = Vec::new();
    let mut next = 0;
    while next < keys.len() {
        let key = keys[next].clone();
        next += 1;
        let declarations = !erase::is_module(&key);
        let source = if declarations {
            project.declaration(&key)
        } else {
            project.script(&key)
        };
        let Some(source) = source else {
            continue;
        };
        let source_type = if declarations {
            SourceType::d_ts()
        } else {
            erase::source_type()
        };
        let mut parsed = oxc_parser::Parser::new(allocator, source, source_type).parse();
        let mut broken = !parsed.diagnostics.is_empty();
        let mut text = source;
        let mut map = qml_syntax::SpanMap::identity(source.len() as u32);
        let mut said = Vec::new();
        // An `.lts` module is read as the TypeScript its `match` and `is`
        // stand for, and so is a `.ts` module that has them, which is
        // the one finding said of it, so the rest of what it says is
        // found too.
        let desugared = if erase::is_lts(&key) || (broken && !declarations) {
            Some(qml_syntax::desugar(source))
        } else {
            None
        };
        if let Some(desugared) = desugared
            && desugared.diagnostics.is_empty()
            && (erase::is_lts(&key) || !desugared.uses.is_empty())
        {
            if let Some(&keyword) = desugared.uses.first()
                && !erase::is_lts(&key)
            {
                said.push((
                    Code::TsLtsSyntax,
                    lts_syntax(
                        project,
                        &key,
                        &source[keyword.start as usize..keyword.end as usize],
                    ),
                    keyword,
                ));
            }
            text = allocator.alloc_str(&desugared.text);
            map = desugared.map;
            parsed = oxc_parser::Parser::new(allocator, text, source_type).parse();
            broken = !parsed.diagnostics.is_empty();
        }
        // Stock TypeScript reads an `.lts` module through its declaration,
        // which needs each export's type written (docs/DIALECT.md §4.21).
        if erase::is_lts(&key) && !broken {
            for (span, message) in crate::lts::untyped_exports(source) {
                said.push((Code::LtsExportType, message, span));
            }
        }
        // Declarations run nothing, so there is nothing to erase. The
        // faults of what was read are where it has them, as the
        // checker's own findings are.
        let faults = if declarations {
            Vec::new()
        } else if text != source {
            erase::erase_text(&key, text).faults
        } else {
            erase::erase(&key, source).faults
        };
        let directory = project::directory(&key);
        for statement in &parsed.program.body {
            let path = match statement {
                Statement::ImportDeclaration(import) => import.source.value.as_str(),
                Statement::ExportFromDeclaration(export) => export.source.value.as_str(),
                _ => continue,
            };
            if let Some(key) = reached(directory, path)
                && (erase::is_module(&key) || project.declaration(&key).is_some())
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        modules.push(Module {
            key,
            map,
            original: source,
            source: text,
            program: parsed.program,
            faults,
            broken,
            declarations,
            said,
        });
    }
    modules
}

/// What is said of the `.ts` module at `key` that has `keyword`, the
/// syntax of an `.lts` one: why it is an error, and the rename that ends
/// it, unless a file is where the rename would put it (`module-clash`).
fn lts_syntax<O: TypeOracle>(project: &Project<O>, key: &str, keyword: &str) -> String {
    let name = key.rsplit('/').next().unwrap_or(key);
    let renamed = erase::lts_name(name);
    let why = format!(
        "`{keyword}` is Ljós syntax, which stock TypeScript tools cannot read in a `.ts` file"
    );
    if project.script(&erase::lts_name(key)).is_some() {
        format!("{why}; it would be renamed to `{renamed}`, but `{renamed}` is in the way")
    } else {
        format!("{why}; rename `{name}` to `{renamed}`")
    }
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
        let key = self.c.project.front().import_dir(self.c.key, &path);
        if erase::is_module(&path) {
            self.module_at(&key)
        } else {
            self.module_at(&erase::declarations_of(&key)?)
        }
    }

    /// The JavaScript file at `key` as a value: the module its
    /// declarations are, or, with none beside it, a value whose members
    /// are each `ts-boundary`.
    pub(super) fn javascript_value(&mut self, key: &str) -> Ty {
        let Some(declarations) = erase::declarations_of(key) else {
            return Ty::ERROR;
        };
        if !self.reaches(&declarations) {
            return self.arena.intern(Kind::Script(key.into()));
        }
        match self.module_at(&declarations) {
            Some(index) => self.module_value(index),
            // DECISION: declarations being read, reached again through a
            // cycle, are let by, as a module is.
            None => Ty::ERROR,
        }
    }

    /// Whether the file reaches a module at `key`.
    fn reaches(&self, key: &str) -> bool {
        self.modules.iter().any(|m| m.key == key)
    }

    /// Reports `name`, read off the JavaScript file at `key`, which has no
    /// declarations, at `span`.
    pub(super) fn boundary(&mut self, key: &str, name: &str, span: Span) {
        let declarations = erase::declarations_of(key).unwrap_or_default();
        let declarations = declarations.rsplit('/').next().unwrap_or_default();
        self.report(
            Code::TsBoundary,
            format!(
                "`{name}` of {key} has no type: {key} has no declarations; write {declarations} \
                 beside it, declaring `{name}` (and what has no fixed shape as `unknown`, for a \
                 decoder to narrow)"
            ),
            span,
        );
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

    /// The type the module at `index` exports as `name`, written at
    /// `span`: an error, reported, when it exports none.
    pub(super) fn exported_type(&mut self, index: usize, name: &str, span: Span) -> Ty {
        if let Some(ty) = self.exported(index, name).1 {
            return ty;
        }
        self.not_exported(index, name, Space::Type, span);
        Ty::ERROR
    }

    /// The type a declaration of the file writes as `parts`, at `span`,
    /// when its first part names a module the file imports
    /// (`property Logic.Row picked`).
    pub(super) fn module_type(&mut self, parts: &[&str], span: Span) -> Option<Ty> {
        let [namespace, name] = parts else {
            return None;
        };
        let index = self.imported_module(namespace, span.start)?;
        let at = Span::new(span.end - name.len() as u32, span.end);
        Some(self.exported_type(index, name, at))
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
        let map = std::mem::replace(&mut self.map, &module.map);
        let original = std::mem::replace(&mut self.original, module.original);
        let this = self.this.take();
        let reading = self.module.replace(index);
        let quiet = std::mem::take(&mut self.quiet);
        let locals = std::mem::take(&mut self.locals);
        let facts = std::mem::take(&mut self.facts);
        let floor = std::mem::take(&mut self.floor);
        let assigned = std::mem::take(&mut self.assigned);
        let fns = std::mem::take(&mut self.fns);
        let fresh = std::mem::take(&mut self.fresh);
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
        self.map = map;
        self.original = original;
        self.this = this;
        self.module = reading;
        self.quiet = quiet;
        self.locals = locals;
        self.facts = facts;
        self.floor = floor;
        self.assigned = assigned;
        self.fns = fns;
        self.fresh = fresh;
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
                let reached = reached(directory, path);
                if let Some(script) = self.bare_javascript(directory, path, reached.as_deref()) {
                    for specifier in import.specifiers.iter().flatten() {
                        self.import_bare(specifier, &script);
                    }
                    continue;
                }
                let from = reached.and_then(|key| self.module_at(&key));
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
            if let Some(declaration) = declaration
                && !matches!(statement, Statement::VariableDeclaration(d) if self.declared_by_is(d))
            {
                self.bind_ahead(declaration);
                if module.declarations {
                    self.bind_declared(declaration);
                }
            }
        }
        // What an `is` at the top declares is the module's, beside what it
        // imports and declares.
        self.is_declared_twice(body, 0, None);
        // Declarations run nothing: what they say is bound above.
        if module.declarations {
            return self.exports(module);
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
                self.bind(it.local.name.as_str(), ty, Binding::Import);
            }
            ImportDeclarationSpecifier::ImportDefaultSpecifier(it) => {
                self.bind(it.local.name.as_str(), Ty::ERROR, Binding::Import);
            }
            ImportDeclarationSpecifier::ImportSpecifier(it) => {
                let local = it.local.name.as_str();
                let Some(from) = from else {
                    self.bind(local, Ty::ERROR, Binding::Import);
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
                    self.bind(local, value.unwrap_or(Ty::ERROR), Binding::Import);
                }
            }
        }
    }

    /// The key of the JavaScript file a module's import at `path` names,
    /// when the file reaches no declarations of it: `reached` is where
    /// they would be.
    fn bare_javascript(
        &self,
        directory: &str,
        path: &str,
        reached: Option<&str>,
    ) -> Option<String> {
        if !relative(path)
            || !erase::is_javascript(path)
            || reached.is_some_and(|k| self.reaches(k))
        {
            return None;
        }
        Some(project::join(directory, path))
    }

    /// Binds what a module imports from the JavaScript file `script`, which
    /// has no declarations: a namespace whose members are each
    /// `ts-boundary`, and a name it picks is `ts-boundary` where it picks
    /// it.
    fn import_bare(&mut self, specifier: &ImportDeclarationSpecifier<'a>, script: &str) {
        match specifier {
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(it) => {
                let ty = self.arena.intern(Kind::Script(script.into()));
                self.bind(it.local.name.as_str(), ty, Binding::Import);
            }
            ImportDeclarationSpecifier::ImportDefaultSpecifier(it) => {
                self.boundary(script, "default", it.local.span);
                self.bind(it.local.name.as_str(), Ty::ERROR, Binding::Import);
            }
            ImportDeclarationSpecifier::ImportSpecifier(it) => {
                self.boundary(script, it.imported.name().as_str(), it.imported.span());
                self.bind(it.local.name.as_str(), Ty::ERROR, Binding::Import);
            }
        }
    }

    /// Binds what a declaration of a JavaScript file's declarations says
    /// is there: a `const` or `let` of the type written. What it leaves
    /// untyped is `any` to TypeScript, so it is `ts-no-any` here.
    fn bind_declared(&mut self, declaration: &Declaration<'a>) {
        match declaration {
            Declaration::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let ty = match &declarator.type_annotation {
                        Some(annotation) => self.annot(&annotation.type_annotation),
                        None => {
                            self.untyped(id.name.as_str(), "write its type", id.span);
                            Ty::ERROR
                        }
                    };
                    self.bind(id.name.as_str(), ty, Binding::Const);
                }
            }
            Declaration::FunctionDeclaration(function) => {
                for param in &function.params.items {
                    if param.type_annotation.is_none() {
                        let name = param.pattern.span();
                        let text = self.text(name).to_string();
                        self.untyped(&text, "write its type", name);
                    }
                }
                if function.return_type.is_none()
                    && let Some(id) = &function.id
                {
                    self.untyped(id.name.as_str(), "write the type it returns", id.span);
                }
            }
            _ => {}
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
                    let reached = reached(directory, path);
                    if let Some(script) = self.bare_javascript(directory, path, reached.as_deref())
                    {
                        for specifier in &export.specifiers {
                            let name = specifier.local.name();
                            self.boundary(&script, name.as_str(), specifier.local.span());
                        }
                        continue;
                    }
                    let from = reached.and_then(|key| self.module_at(&key));
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
