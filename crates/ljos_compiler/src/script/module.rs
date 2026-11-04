//! A TypeScript module imported by path (`import "filter.ts" as Logic`), as
//! the engine runs it: its types erased, its imports and exports made into
//! what a script is given and gives.
//!
//! Erasure works on the parsed module, never on its text by pattern: each
//! piece that is only a type is blanked where it is, a space for each
//! character and the line breaks kept, so a line and column the engine
//! reports are the module's own. What TypeScript writes that is not only a
//! type (an `enum`, a `namespace`, a parameter property) has no blank to
//! leave and is refused, as `erasableSyntaxOnly` refuses it.

use std::collections::HashSet;

use ljos_ir::ScriptImport;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, FormalParameter, ImportDeclarationSpecifier,
    ImportOrExportKind, MethodDefinitionType, PropertyDefinitionType, PropertyKey, Statement,
    TSAsExpression, TSNonNullExpression, TSSatisfiesExpression, TSThisParameter, TSTypeAnnotation,
    TSTypeAssertion, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};

/// Whether `path` names a TypeScript module, which is imported as a script
/// is; a declaration file (`.d.ts`) is types alone and runs nothing.
pub(crate) fn is_module(path: &str) -> bool {
    path.ends_with(".ts") && !path.ends_with(".d.ts")
}

/// What a module is that it cannot run as: a byte span of its text and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fault {
    pub span: Span,
    pub message: String,
}

/// A module with its types erased.
#[derive(Debug, Default)]
pub(crate) struct Erased {
    /// The module's statements as JavaScript, line for line the module's.
    pub text: String,
    /// What it exports to its importers: each name and the expression that
    /// reads it.
    pub exports: Vec<(String, String)>,
    /// What it imports, in the order the runtime hands it over.
    pub imports: Vec<ScriptImport>,
    pub faults: Vec<Fault>,
}

/// The parsing a module is read with: TypeScript, as an ECMAScript module.
pub(crate) fn source_type() -> SourceType {
    SourceType::ts().with_module(true)
}

/// `source`, the text of the module at `key`, erased.
pub(crate) fn erase(key: &str, source: &str) -> Erased {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        let at = error.labels.first().map_or(0, |label| label.offset());
        return Erased {
            text: source.to_string(),
            faults: vec![Fault {
                span: Span::new(at, at),
                message: error.message.to_string(),
            }],
            ..Erased::default()
        };
    }
    let mut eraser = Eraser {
        source,
        directory: qml_front::project::directory(key),
        edits: Vec::new(),
        out: Erased::default(),
        types: HashSet::new(),
    };
    // Names that are only types are not read at run time: an export of one
    // is the checker's, and gives an importer nothing.
    for statement in &parsed.program.body {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => &export.declaration,
            other => match other.as_declaration() {
                Some(declaration) => declaration,
                None => continue,
            },
        };
        match declaration {
            Declaration::TSTypeAliasDeclaration(alias) => {
                eraser.types.insert(alias.id.name.as_str().to_string());
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                eraser.types.insert(interface.id.name.as_str().to_string());
            }
            _ => {}
        }
    }
    for statement in &parsed.program.body {
        eraser.statement(statement);
    }
    let Eraser { edits, mut out, .. } = eraser;
    out.text = apply(source, edits);
    out
}

/// Replace `span` with `text`, or with blanks when `text` is None.
struct Edit {
    span: Span,
    text: Option<String>,
}

struct Eraser<'s> {
    source: &'s str,
    directory: &'s str,
    edits: Vec<Edit>,
    out: Erased,
    types: HashSet<String>,
}

impl<'s> Eraser<'s> {
    fn blank(&mut self, start: u32, end: u32) {
        if start < end {
            self.edits.push(Edit {
                span: Span::new(start, end),
                text: None,
            });
        }
    }

    /// Blank a whole statement or class member, leaving a `;` where it
    /// began: without it, the line before could run on into the line
    /// after (`f` then `(1)` is `f(1)`), where TypeScript read two.
    fn blank_statement(&mut self, start: u32, end: u32) {
        if start < end {
            self.replace(Span::new(start, start + 1), ";".to_string());
            self.blank(start + 1, end);
        }
    }

    fn replace(&mut self, span: Span, text: String) {
        self.edits.push(Edit {
            span,
            text: Some(text),
        });
    }

    fn fault(&mut self, span: Span, message: impl Into<String>) {
        self.out.faults.push(Fault {
            span,
            message: message.into(),
        });
    }

    /// The import at `path`, where the runtime hands it to the module.
    fn import(&mut self, path: &str) -> usize {
        let key = qml_front::project::join(self.directory, path);
        if let Some(at) = self.out.imports.iter().position(|i| i.key == key) {
            return at;
        }
        self.out.imports.push(ScriptImport {
            path: path.to_string(),
            key,
        });
        self.out.imports.len() - 1
    }

    /// Whether `path`, which a module imports from, is one it can reach: a
    /// file beside it, as a QML file reaches a script.
    fn reachable(&mut self, span: Span, path: &str) -> bool {
        let relative = path.starts_with("./") || path.starts_with("../");
        let script = is_module(path) || path.ends_with(".js") || path.ends_with(".mjs");
        if relative && script {
            return true;
        }
        self.fault(
            span,
            format!(
                "`{path}` is not a file beside the module: import a `.ts` or `.js` file by its path from here (`./{path}`)"
            ),
        );
        false
    }

    fn statement(&mut self, statement: &Statement<'_>) {
        match statement {
            Statement::ImportDeclaration(import) => {
                if import.import_kind == ImportOrExportKind::Type {
                    return self.blank_statement(import.span.start, import.span.end);
                }
                let path = import.source.value.as_str();
                if !self.reachable(import.source.span, path) {
                    return;
                }
                let mut names = Vec::new();
                let mut whole = None;
                let mut typed = 0;
                let specifiers: &[ImportDeclarationSpecifier<'_>] =
                    import.specifiers.as_ref().map_or(&[], |s| s.as_slice());
                for specifier in specifiers {
                    match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(it) => {
                            if it.import_kind == ImportOrExportKind::Type {
                                typed += 1;
                                continue;
                            }
                            let imported = it.imported.name();
                            let local = it.local.name.as_str();
                            names.push(if imported.as_str() == local {
                                local.to_string()
                            } else {
                                format!("{}: {local}", quoted_key(imported.as_str()))
                            });
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(it) => {
                            whole = Some(it.local.name.as_str().to_string());
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(it) => {
                            self.fault(
                                it.span,
                                "a module gives its names, not a default: import them by name (`import { name } from`)",
                            );
                            return;
                        }
                    }
                }
                if typed > 0 && typed == specifiers.len() {
                    return self.blank_statement(import.span.start, import.span.end);
                }
                let index = self.import(path);
                let from = format!("__ljos_imports[{index}]");
                let text = match (whole, names.is_empty()) {
                    (Some(whole), _) => format!("const {whole} = {from};"),
                    (None, false) => format!("const {{ {} }} = {from};", names.join(", ")),
                    (None, true) => format!("{from};"),
                };
                self.replace_keeping_lines(import.span, text);
            }
            Statement::ExportDeclaration(export) => {
                let declaration = &export.declaration;
                if self.type_only(declaration) {
                    return self.blank_statement(export.span.start, export.span.end);
                }
                self.blank(export.span.start, declaration.span().start);
                for name in declared_names(declaration) {
                    self.out.exports.push((name.clone(), name));
                }
                self.visit_declaration(declaration);
            }
            Statement::ExportNamedDeclaration(export) => {
                self.blank_statement(export.span.start, export.span.end);
                if export.export_kind == ImportOrExportKind::Type {
                    return;
                }
                for specifier in &export.specifiers {
                    let local = specifier.local.name();
                    if specifier.export_kind == ImportOrExportKind::Type
                        || self.types.contains(local.as_str())
                    {
                        continue;
                    }
                    self.out.exports.push((
                        specifier.exported.name().as_str().to_string(),
                        local.as_str().to_string(),
                    ));
                }
            }
            Statement::ExportFromDeclaration(export) => {
                self.blank_statement(export.span.start, export.span.end);
                let path = export.source.value.as_str();
                if export.export_kind == ImportOrExportKind::Type
                    || !self.reachable(export.source.span, path)
                {
                    return;
                }
                let specifiers: Vec<(String, String)> = export
                    .specifiers
                    .iter()
                    .filter(|s| s.export_kind != ImportOrExportKind::Type)
                    .map(|s| {
                        (
                            s.exported.name().as_str().to_string(),
                            s.local.name().as_str().to_string(),
                        )
                    })
                    .collect();
                if specifiers.is_empty() {
                    return;
                }
                let index = self.import(path);
                for (exported, local) in specifiers {
                    let read = format!("__ljos_imports[{index}][{}]", quoted(&local));
                    self.out.exports.push((exported, read));
                }
            }
            Statement::ExportAllDeclaration(export) => {
                if export.export_kind == ImportOrExportKind::Type {
                    return self.blank_statement(export.span.start, export.span.end);
                }
                self.fault(
                    export.span,
                    "`export *` gives names the importer cannot see here: export each by name (`export { name } from`)",
                );
            }
            Statement::ExportDefaultDeclaration(export) => {
                self.fault(
                    export.span,
                    "a module gives its names, not a default: export it by name (`export function name`)",
                );
            }
            Statement::TSExportAssignment(it) => {
                self.fault(
                    it.span,
                    "`export =` is CommonJS: export each name (`export const name`)",
                );
            }
            Statement::TSNamespaceExportDeclaration(it) => {
                self.blank_statement(it.span.start, it.span.end);
            }
            other => self.visit_statement(other),
        }
    }

    /// A replacement on the first line of `span`, with its other lines kept
    /// as line breaks.
    fn replace_keeping_lines(&mut self, span: Span, text: String) {
        let breaks = self.source[span.start as usize..span.end as usize]
            .matches('\n')
            .count();
        self.replace(span, format!("{text}{}", "\n".repeat(breaks)));
    }

    /// A declaration that is only types, or only says what is elsewhere
    /// (`declare`), and leaves nothing to run.
    fn type_only(&self, declaration: &Declaration<'_>) -> bool {
        match declaration {
            Declaration::TSTypeAliasDeclaration(_) | Declaration::TSInterfaceDeclaration(_) => true,
            Declaration::VariableDeclaration(it) => it.declare,
            Declaration::FunctionDeclaration(it) => it.declare || it.body.is_none(),
            Declaration::ClassDeclaration(it) => it.declare,
            Declaration::TSEnumDeclaration(it) => it.declare,
            Declaration::TSNamespaceDeclaration(it) => it.declare,
            Declaration::TSExternalModuleDeclaration(it) => it.declare,
            Declaration::TSGlobalDeclaration(it) => it.declare,
            Declaration::TSImportEqualsDeclaration(_) => false,
        }
    }

    /// Blank each TypeScript modifier word between `start` and `end`, the
    /// text before a class member's name.
    fn blank_modifiers(&mut self, start: u32, end: u32) {
        const MODIFIERS: [&str; 7] = [
            "public",
            "private",
            "protected",
            "readonly",
            "override",
            "declare",
            "abstract",
        ];
        let text = &self.source[start as usize..end as usize];
        let mut at = 0usize;
        for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')) {
            if MODIFIERS.contains(&word) {
                let from = start + at as u32;
                self.blank(from, from + word.len() as u32);
            }
            at += word.len() + 1;
        }
    }

    /// Blank the first `marks` character found between `start` and `end`:
    /// the `?` of an optional name, the `!` of a definite one.
    fn blank_mark(&mut self, start: u32, end: u32, marks: &[char]) {
        let text = &self.source[start as usize..end as usize];
        if let Some(at) = text.find(|c: char| marks.contains(&c)) {
            let at = start + at as u32;
            self.blank(at, at + 1);
        }
    }
}

impl<'a> Visit<'a> for Eraser<'_> {
    fn visit_declaration(&mut self, it: &Declaration<'a>) {
        if self.type_only(it) {
            let span = it.span();
            return self.blank_statement(span.start, span.end);
        }
        match it {
            Declaration::TSEnumDeclaration(it) => self.fault(
                it.span,
                "an `enum` is not only a type, and does not erase: write a union of strings (`type Mode = \"a\" | \"b\"`)",
            ),
            Declaration::TSNamespaceDeclaration(it) => self.fault(
                it.span,
                "a `namespace` is not only a type, and does not erase: write a module of its own",
            ),
            Declaration::TSExternalModuleDeclaration(it) => self.fault(
                it.span,
                "a `module` declaration is not only a type, and does not erase: write a module of its own",
            ),
            Declaration::TSGlobalDeclaration(it) => self.fault(
                it.span,
                "`global` is not only a type, and does not erase",
            ),
            Declaration::TSImportEqualsDeclaration(it) => self.fault(
                it.span,
                "`import =` is CommonJS: import by name (`import { name } from`)",
            ),
            other => walk::walk_declaration(self, other),
        }
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if it.accessibility.is_some() || it.readonly || it.r#override {
            self.fault(
                it.span,
                "a parameter property is not only a type, and does not erase: assign the field in the constructor",
            );
        }
        if it.optional {
            let end = it
                .type_annotation
                .as_ref()
                .map_or(it.span.end, |annotation| annotation.span.start);
            self.blank_mark(it.pattern.span().end, end, &['?']);
        }
        walk::walk_formal_parameter(self, it);
    }

    fn visit_ts_this_parameter(&mut self, it: &TSThisParameter<'a>) {
        // `this: T, x` loses `this: T,`.
        let rest = &self.source[it.span.end as usize..];
        let comma = rest
            .find(|c: char| !c.is_whitespace())
            .filter(|at| rest[*at..].starts_with(','))
            .map_or(0, |at| at + 1);
        self.blank(it.span.start, it.span.end + comma as u32);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if it.r#abstract {
            let head = &self.source[it.span.start as usize..];
            if let Some(at) = head.find("abstract") {
                let at = it.span.start + at as u32;
                self.blank(at, at + "abstract".len() as u32);
            }
        }
        if let (Some(first), Some(last)) = (it.implements.first(), it.implements.last()) {
            let before = &self.source[..first.span.start as usize];
            let start = before
                .rfind("implements")
                .map_or(first.span.start, |at| at as u32);
            self.blank(start, last.span.end);
        }
        if let Some(parameters) = &it.type_parameters {
            self.visit_ts_type_parameter_declaration(parameters);
        }
        if let Some(heritage) = &it.heritage {
            self.visit_class_heritage(heritage);
        }
        self.visit_class_body(&it.body);
    }

    fn visit_class_element(&mut self, it: &ClassElement<'a>) {
        match it {
            ClassElement::MethodDefinition(method) => {
                if method.value.body.is_none()
                    || method.r#type == MethodDefinitionType::TSAbstractMethodDefinition
                {
                    return self.blank_statement(method.span.start, method.span.end);
                }
                self.blank_modifiers(method.span.start, key_start(&method.key));
                if method.optional {
                    self.blank_mark(
                        method.key.span().end,
                        method.value.params.span.start,
                        &['?'],
                    );
                }
            }
            ClassElement::PropertyDefinition(property) => {
                if property.declare
                    || property.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition
                {
                    return self.blank_statement(property.span.start, property.span.end);
                }
                self.blank_modifiers(property.span.start, key_start(&property.key));
                if property.optional || property.definite {
                    let end = property
                        .type_annotation
                        .as_ref()
                        .map(|annotation| annotation.span.start)
                        .or_else(|| property.value.as_ref().map(|value| value.span().start))
                        .unwrap_or(property.span.end);
                    self.blank_mark(property.key.span().end, end, &['?', '!']);
                }
            }
            ClassElement::AccessorProperty(property) => {
                self.blank_modifiers(property.span.start, key_start(&property.key));
                if property.definite {
                    let end = property
                        .type_annotation
                        .as_ref()
                        .map_or(property.span.end, |annotation| annotation.span.start);
                    self.blank_mark(property.key.span().end, end, &['!']);
                }
            }
            ClassElement::TSIndexSignature(signature) => {
                return self.blank_statement(signature.span.start, signature.span.end);
            }
            ClassElement::StaticBlock(_) => {}
        }
        walk::walk_class_element(self, it);
    }

    fn visit_ts_type_annotation(&mut self, it: &TSTypeAnnotation<'a>) {
        self.blank(it.span.start, it.span.end);
    }

    fn visit_ts_type_parameter_declaration(&mut self, it: &TSTypeParameterDeclaration<'a>) {
        self.blank(it.span.start, it.span.end);
    }

    fn visit_ts_type_parameter_instantiation(&mut self, it: &TSTypeParameterInstantiation<'a>) {
        self.blank(it.span.start, it.span.end);
    }

    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.blank(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_satisfies_expression(&mut self, it: &TSSatisfiesExpression<'a>) {
        self.blank(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_non_null_expression(&mut self, it: &TSNonNullExpression<'a>) {
        self.blank(it.expression.span().end, it.span.end);
        self.visit_expression(&it.expression);
    }

    fn visit_ts_type_assertion(&mut self, it: &TSTypeAssertion<'a>) {
        self.blank(it.span.start, it.expression.span().start);
        self.visit_expression(&it.expression);
    }
}

/// Where a class member's name starts, with the `[` of a computed one.
fn key_start(key: &PropertyKey<'_>) -> u32 {
    key.span().start
}

/// The names a declaration that runs binds at the module's top level.
fn declared_names(declaration: &Declaration<'_>) -> Vec<String> {
    match declaration {
        Declaration::VariableDeclaration(it) => it
            .declarations
            .iter()
            .flat_map(|d| d.id.get_binding_identifiers())
            .map(|id| id.name.as_str().to_string())
            .collect(),
        Declaration::FunctionDeclaration(it) => it
            .id
            .iter()
            .map(|id| id.name.as_str().to_string())
            .collect(),
        Declaration::ClassDeclaration(it) => it
            .id
            .iter()
            .map(|id| id.name.as_str().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn quoted(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for c in name.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A name as the key of an object pattern: bare when it is an identifier.
fn quoted_key(name: &str) -> String {
    let identifier = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    if identifier {
        name.to_string()
    } else {
        quoted(name)
    }
}

/// `source` with the edits made, a blank keeping each line break and
/// putting a space for every other character. An edit inside one already
/// made is part of it.
fn apply(source: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|edit| (edit.span.start, std::cmp::Reverse(edit.span.end)));
    let mut out = String::with_capacity(source.len());
    let mut at = 0usize;
    for edit in edits {
        let (start, end) = (edit.span.start as usize, edit.span.end as usize);
        if end <= at || end > source.len() {
            continue;
        }
        let start = start.max(at);
        out.push_str(&source[at..start]);
        match edit.text {
            Some(text) => out.push_str(&text),
            None => out.extend(
                source[start..end]
                    .chars()
                    .map(|c| if c == '\n' || c == '\r' { c } else { ' ' }),
            ),
        }
        at = end;
    }
    out.push_str(&source[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erased(source: &str) -> Erased {
        erase("logic/filter.ts", source)
    }

    #[test]
    fn types_erase_to_blanks_and_lines_stay_put() {
        let out = erased(
            "export interface Row { tag: string }\n\
             export type Tag = \"a\" | \"b\"\n\
             export function only<T>(tag: Tag, rows?: readonly Row[]): number {\n\
             \x20 return (rows ?? []).filter((r: Row) => r.tag === tag).length as number\n\
             }\n\
             export const label = \"x\" satisfies string\n\
             const n = label!\n",
        );
        assert!(out.faults.is_empty(), "{:?}", out.faults);
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0].trim(), ";");
        assert_eq!(lines[1].trim(), ";");
        assert_eq!(
            lines[2].split_whitespace().collect::<Vec<_>>().join(" "),
            "function only (tag , rows ) {"
        );
        assert_eq!(lines[2].find("tag"), Some("export function only<T>(".len()));
        assert_eq!(
            lines[3].split_whitespace().collect::<Vec<_>>().join(" "),
            "return (rows ?? []).filter((r ) => r.tag === tag).length"
        );
        assert_eq!(lines[5].trim(), "const label = \"x\"");
        assert_eq!(lines[6].trim(), "const n = label");
        let exports: Vec<&str> = out.exports.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(exports, ["only", "label"]);
    }

    #[test]
    fn imports_become_what_the_runtime_hands_over() {
        let out = erased(
            "import { only, type Row, label as name } from \"./other.ts\"\n\
             import * as Other from \"./other.ts\"\n\
             import type { Tag } from \"../types.ts\"\n\
             import \"./side.js\"\n\
             export { only as just }\n\
             export type { Row }\n\
             export { label } from \"./other.ts\"\n",
        );
        assert!(out.faults.is_empty(), "{:?}", out.faults);
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(lines[0], "const { only, label: name } = __ljos_imports[0];");
        assert_eq!(lines[1], "const Other = __ljos_imports[0];");
        assert_eq!(lines[2].trim(), ";");
        assert_eq!(lines[3], "__ljos_imports[1];");
        let keys: Vec<&str> = out.imports.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["logic/other.ts", "logic/side.js"]);
        assert_eq!(
            out.exports,
            [
                ("just".to_string(), "only".to_string()),
                (
                    "label".to_string(),
                    "__ljos_imports[0][\"label\"]".to_string()
                ),
            ]
        );
    }

    #[test]
    fn classes_lose_their_modifiers_and_signatures() {
        let out = erased(
            "export abstract class Box<T> implements Thing {\n\
             \x20 private readonly size?: number\n\
             \x20 count!: number\n\
             \x20 abstract area(): number\n\
             \x20 grow(this: Box<T>, by: number): void { this.count = by }\n\
             }\n",
        );
        assert!(out.faults.is_empty(), "{:?}", out.faults);
        let words = |line: &str| line.split_whitespace().collect::<Vec<_>>().join(" ");
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(words(lines[0]), "class Box {");
        assert_eq!(words(lines[1]), "size");
        assert_eq!(words(lines[2]), "count");
        assert_eq!(words(lines[3]), ";");
        assert_eq!(words(lines[4]), "grow( by ) { this.count = by }");
    }

    #[test]
    fn a_statement_erased_does_not_join_its_neighbours() {
        // Without a semicolon, the line before a type would run on into the
        // line after it, `f(1)`, where TypeScript reads two statements.
        let out = erased(
            "const f = (n: number) => n\n\
             export const a = f\n\
             type T = number\n\
             (1)\n\
             interface I { n: number }\n\
             [a].length\n",
        );
        assert!(out.faults.is_empty(), "{:?}", out.faults);
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &out.text, SourceType::mjs()).parse();
        assert!(parsed.diagnostics.is_empty(), "{}", out.text);
        // `const f`, `const a`, `(1)` and `[a].length`, with the empty
        // statements the types left.
        let real: Vec<_> = parsed
            .program
            .body
            .iter()
            .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
            .collect();
        assert_eq!(real.len(), 4, "{}", out.text);
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(lines[2].len(), "type T = number".len());
        assert_eq!(lines[2].trim(), ";");
    }

    #[test]
    fn what_does_not_erase_is_refused() {
        let out = erased(
            "export enum Mode { A }\n\
             namespace N { export const a = 1 }\n\
             export default function f() {}\n\
             import x from \"./x.ts\"\n\
             import { y } from \"lodash\"\n\
             class C { constructor(private n: number) {} }\n\
             declare enum Quiet { B }\n",
        );
        let messages: Vec<&str> = out.faults.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(messages.len(), 6, "{messages:?}");
        assert!(messages[0].contains("`enum`"));
        assert!(messages[1].contains("`namespace`"));
        assert!(messages[2].contains("not a default"));
        assert!(messages[3].contains("not a default"));
        assert!(messages[4].contains("`lodash`"));
        assert!(messages[5].contains("parameter property"));
    }
}
