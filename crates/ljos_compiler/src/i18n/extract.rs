//! Finds Ljós's own messages in a file: every `t`, `msg`, `plural`,
//! `select` and `selectOrdinal` of `Ljos.Core`, read by
//! [`message_of`](super::message::message_of), so a catalogue holds each
//! message under the key the running program asks for (docs/DIALECT.md
//! §4.13).
//!
//! The reading is of the syntax alone, as [`extract_qml`](super::extract_qml)
//! is: a QML file's macros are those of a file that imports `Ljos.Core`, a
//! module's those it imports from `@ljos/core` under their own names. It
//! needs no types, so a file the compiler cannot compile yet is still read.
//!
//! A macro, or a choice (`plural`, `select`, `selectOrdinal`) inside a
//! message, is one only where the name is the macro, as the compiler reads
//! it: in a module, where the name is the import; in a QML file, where
//! neither the script, an id, the object the script is on nor the root of a
//! component around it binds the name. A QML file's scripts are read by the
//! compiler's own reading of a script ([`crate::js::analyze_with`]), so a
//! `t` a script declares, or a parameter of that name, is the script's here
//! as it is there. What a type gives its objects is not known without
//! types: a choice named as a member of the object's type (`TextInput`'s
//! `select`) is read as a form here and as a value by the compiler.

use oxc_allocator::Allocator;
use std::collections::HashSet;

use oxc_ast::ast::{
    CallExpression, Expression, IdentifierReference, ImportDeclarationSpecifier,
    ImportOrExportKind, ModuleDeclaration, TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;
use qml_syntax::ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject};

use crate::js::{self, Form};

use super::message::{MessageCall, MessageError, call_message_in, tagged_message_in};
use super::{Lines, Note};

/// One message as it stands in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMessage {
    /// What the macro says. Its `key` is never `None`: a `t` of a message
    /// made elsewhere has nothing for a catalogue and is not found.
    pub call: MessageCall,
    /// The line of the macro, from 1.
    pub line: u32,
}

/// What one file holds of Ljós's messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Messages {
    /// In source order.
    pub messages: Vec<FoundMessage>,
    /// A macro that says no message: what the checker says of it.
    pub notes: Vec<Note>,
    /// How many places did not parse. What did parse is still searched.
    pub syntax_errors: usize,
}

/// The messages of a QML file. Only a file that imports `Ljos.Core`
/// itself, without `as`, has the macros (docs/DIALECT.md §4.3).
pub fn messages_qml(source: &str) -> Messages {
    let allocator = Allocator::default();
    // `match` and `is` are read as the scripts they stand for, as the
    // compiler reads them, and each message put back where the file has it.
    let qml_syntax::Lowered {
        text, parsed, map, ..
    } = qml_syntax::parse_lowered(&allocator, source);
    let core = parsed.document.imports.iter().any(|import| {
        import.alias.is_none()
            && matches!(&import.source,
                QmlImportSource::Module(name) if name.to_string() == qml_front::names::CORE)
    });
    let mut scripts = Scripts {
        source: text,
        ids: HashSet::new(),
        found: Vec::new(),
    };
    if core && !parsed.panicked {
        ids_of(&parsed.document.root, &mut scripts.ids);
        let root = declared(&parsed.document.root);
        scripts.object(&parsed.document.root, &[root]);
    }
    let mut messages = finish(scripts.found, source, |span| map.span(span));
    messages.syntax_errors = parsed.diagnostics.len();
    messages
}

/// The messages of a `.ts` or `.lts` module, whose key (its path) says
/// which: an `.lts` module is desugared first, and its lines are its own.
pub fn messages_module(key: &str, source: &str) -> Messages {
    if !crate::script::module::is_lts(key) {
        return module_text(source, source, |span| span);
    }
    let desugared = qml_syntax::desugar(source);
    if !desugared.diagnostics.is_empty() {
        return Messages {
            syntax_errors: desugared.diagnostics.len(),
            ..Messages::default()
        };
    }
    module_text(source, &desugared.text, |span| desugared.map.span(span))
}

fn module_text(source: &str, text: &str, map: impl Fn(Span) -> Span) -> Messages {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, crate::script::module::source_type()).parse();
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let mut symbols = HashSet::new();
    for statement in &parsed.program.body {
        let Some(ModuleDeclaration::ImportDeclaration(import)) = statement.as_module_declaration()
        else {
            continue;
        };
        // A type-only import brings no value, so no macro.
        if import.import_kind == ImportOrExportKind::Type
            || import.source.value != crate::script::module::CORE_MODULE
        {
            continue;
        }
        for specifier in import.specifiers.iter().flatten() {
            if let ImportDeclarationSpecifier::ImportSpecifier(it) = specifier
                && it.import_kind != ImportOrExportKind::Type
                && it.imported.name() == it.local.name
                && MACROS.contains(&it.local.name.as_str())
            {
                symbols.insert(it.local.symbol_id());
            }
        }
    }
    let any = !symbols.is_empty();
    let mut finder = Finder {
        scoping: semantic.scoping(),
        symbols,
        found: Vec::new(),
    };
    if any {
        finder.visit_program(&parsed.program);
    }
    let mut messages = finish(finder.found, source, map);
    messages.syntax_errors = parsed.diagnostics.len();
    messages
}

const MACROS: &[&str] = super::message::MACROS;

/// The names an object declares as its members.
fn declared(object: &QmlObject<'_>) -> Vec<String> {
    object
        .members
        .iter()
        .filter_map(|member| match member {
            QmlMember::Property(property) => Some(property.name.name.to_string()),
            QmlMember::Signal(signal) => Some(signal.name.name.to_string()),
            QmlMember::Function(function) => function.id.as_ref().map(|id| id.name.to_string()),
            _ => None,
        })
        .collect()
}

fn ids_of(object: &QmlObject<'_>, ids: &mut HashSet<String>) {
    ids.extend(qml_front::scope::object_id(object).map(str::to_owned));
    for member in &object.members {
        match member {
            QmlMember::Object(child) => ids_of(child, ids),
            QmlMember::InlineComponent(component) => ids_of(&component.object, ids),
            QmlMember::Binding(binding) => values_of(&binding.value, ids),
            QmlMember::Property(property) => {
                if let Some(value) = &property.value {
                    values_of(value, ids);
                }
            }
            QmlMember::Function(_) | QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn values_of(value: &QmlBindingValue<'_>, ids: &mut HashSet<String>) {
    match value {
        QmlBindingValue::Object(object) => ids_of(object, ids),
        QmlBindingValue::Objects(objects) => {
            for object in objects {
                ids_of(object, ids);
            }
        }
        QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
    }
}

/// Walks a QML file's objects and reads each script as the compiler does.
struct Scripts<'s> {
    source: &'s str,
    /// Every id of the file. A component's ids are in sight of the
    /// components inside it, so this is more than the compiler sees only
    /// where an inner component's id names a macro.
    ids: HashSet<String>,
    found: Vec<Result<MessageCall, MessageError>>,
}

impl Scripts<'_> {
    /// `roots` are the declared names of the roots of the components
    /// the object is in, the outermost first.
    fn object(&mut self, object: &QmlObject<'_>, roots: &[Vec<String>]) {
        let own = declared(object);
        // A `Component` starts a component, whose root is its object.
        let component = object.type_name.parts.last() == Some(&"Component");
        for member in &object.members {
            match member {
                QmlMember::Object(child) if component => {
                    let mut inner = roots.to_vec();
                    inner.push(declared(child));
                    self.object(child, &inner);
                }
                QmlMember::Object(child) => self.object(child, roots),
                QmlMember::Binding(binding) => {
                    // `delegate: Item { … }`: the object is a component's
                    // root, as the compiler's scope reads it without types.
                    let starts = binding
                        .name
                        .as_simple()
                        .is_some_and(|name| qml_front::scope::takes_component(&[name]));
                    self.value(&binding.value, &own, roots, starts);
                }
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        // `property Component row: Item { … }`.
                        let starts = property.type_name.as_ref().is_some_and(|type_name| {
                            type_name.name.as_simple() == Some("Component") && !type_name.is_list
                        });
                        self.value(value, &own, roots, starts);
                    }
                }
                QmlMember::Function(function) => {
                    self.script(function.span, Form::Function, &own, roots);
                }
                QmlMember::InlineComponent(component) => {
                    let mut inner = roots.to_vec();
                    inner.push(declared(&component.object));
                    self.object(&component.object, &inner);
                }
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    /// `starts` when an object given here is the root of a component.
    fn value(
        &mut self,
        value: &QmlBindingValue<'_>,
        own: &[String],
        roots: &[Vec<String>],
        starts: bool,
    ) {
        let root = |object: &QmlObject<'_>| {
            let mut inner = roots.to_vec();
            if starts {
                inner.push(declared(object));
            }
            inner
        };
        match value {
            QmlBindingValue::Expression(expression) => {
                self.script(expression.span(), Form::Expression, own, roots);
            }
            QmlBindingValue::Statement(statement) => {
                self.script(statement.span(), Form::Statement, own, roots);
            }
            QmlBindingValue::Object(object) => self.object(object, &root(object)),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object, &root(object));
                }
            }
        }
    }

    /// Reads the script at `span` of the object whose members are `own`,
    /// as the compiler reads it: a macro the document binds where the
    /// script is (an id, a member of the object or of a component's root)
    /// is the document's, and one the script declares is the script's.
    fn script(&mut self, span: Span, form: Form, own: &[String], roots: &[Vec<String>]) {
        let Some(text) = self.source.get(span.start as usize..span.end as usize) else {
            return;
        };
        if !MACROS.iter().any(|name| text.contains(name)) {
            return;
        }
        let bound: Vec<&str> = MACROS
            .iter()
            .copied()
            .filter(|name| {
                self.ids.contains(*name)
                    || own
                        .iter()
                        .chain(roots.iter().flatten())
                        .any(|declared| declared == name)
            })
            .collect();
        // A script that does not parse is the compiler's to report.
        let Ok(analysis) = js::analyze_with(text, form, &[], &bound) else {
            return;
        };
        let shift = |local: Span| Span::new(span.start + local.start, span.start + local.end);
        self.found.extend(analysis.messages.into_iter().map(|site| {
            site.message
                .map(|call| call.map_spans(shift))
                .map_err(|error| error.map_spans(shift))
        }));
    }
}

/// Walks a module and lists the macros in it, outermost first: a macro
/// inside another's message is part of that message.
struct Finder<'s> {
    scoping: &'s Scoping,
    /// The symbols of its imports from `@ljos/core`.
    symbols: HashSet<SymbolId>,
    found: Vec<Result<MessageCall, MessageError>>,
}

impl Finder<'_> {
    fn is_macro(&self, id: &IdentifierReference<'_>) -> bool {
        id.reference_id
            .get()
            .and_then(|reference| self.scoping.get_reference(reference).symbol_id())
            .is_some_and(|symbol| self.symbols.contains(&symbol))
    }

    /// Whether the macro `callee` names, at `span`, is one to read: the
    /// file's, and not part of a message already read.
    fn wanted(&self, callee: &Expression<'_>, span: Span) -> bool {
        let Expression::Identifier(id) = callee else {
            return false;
        };
        self.is_macro(id)
            && !self.found.iter().any(|outer| match outer {
                Ok(outer) => outer.covers(span),
                Err(outer) => outer.span.start <= span.start && span.end <= outer.span.end,
            })
    }
}

fn finish(
    found: Vec<Result<MessageCall, MessageError>>,
    source: &str,
    map: impl Fn(Span) -> Span,
) -> Messages {
    let lines = Lines::new(source);
    let mut messages = Messages::default();
    for found in found {
        match found {
            Ok(call) => {
                let call = call.map_spans(&map);
                let Some(key) = call.key() else {
                    continue;
                };
                // A catalogue's entry of `msgid ""` is its header: an
                // empty message would be read back as a second one.
                if key.is_empty() {
                    messages.notes.push(Note {
                        line: lines.line(call.span.start),
                        message: "an empty message has nothing to translate".to_owned(),
                    });
                    continue;
                }
                let line = lines.line(call.span.start);
                messages.messages.push(FoundMessage { call, line });
            }
            Err(error) => {
                let error = error.map_spans(&map);
                messages.notes.push(Note {
                    line: lines.line(error.span.start),
                    message: error.message,
                });
            }
        }
    }
    messages.messages.sort_by_key(|found| found.call.span.start);
    messages
}

impl<'a> Visit<'a> for Finder<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.wanted(&it.callee, it.span)
            && let Some(found) = call_message_in(it, &|id| self.is_macro(id))
        {
            self.found.push(found);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if self.wanted(&it.tag, it.span)
            && let Some(found) = tagged_message_in(it, &|id| self.is_macro(id))
        {
            self.found.push(found);
        }
        walk::walk_tagged_template_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(messages: &Messages) -> Vec<(String, Option<String>, u32)> {
        messages
            .messages
            .iter()
            .map(|m| {
                (
                    m.call.key().unwrap().to_owned(),
                    m.call.context.clone(),
                    m.line,
                )
            })
            .collect()
    }

    #[test]
    fn a_qml_file_that_imports_core() {
        let source = "import Ljos.Ui\nimport Ljos.Core\nItem {\n  property string name: \"Ada\"\n  property string hello: t`Hello ${name}`\n  property string open: t({ message: \"Open\", context: \"menu\", comment: \"The File menu\" })\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![
                ("Hello {name}".to_owned(), None, 5),
                ("Open".to_owned(), Some("menu".to_owned()), 6),
            ]
        );
        assert_eq!(
            found.messages[1].call.comment.as_deref(),
            Some("The File menu")
        );
    }

    #[test]
    fn a_qml_file_without_core_has_no_macros() {
        let found = messages_qml("import QtQuick\nItem {\n  property string a: t`Open`\n}\n");
        assert!(found.messages.is_empty());
    }

    #[test]
    fn a_form_inside_a_message_is_part_of_it() {
        let source = "import Ljos.Core\nItem {\n  property string a: t`${name} has ${plural(n, { one: \"# file\", other: \"# files\" })}`\n  property string b: t`x ${t`inner`}`\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![
                (
                    "{name} has {n, plural, one {# file} other {# files}}".to_owned(),
                    None,
                    3
                ),
                ("x {0}".to_owned(), None, 4),
                ("inner".to_owned(), None, 4),
            ]
        );
    }

    #[test]
    fn a_message_made_elsewhere_is_not_found_and_a_wrong_one_is_noted() {
        let source = "import Ljos.Core\nItem {\n  property var m\n  property string a: t(m)\n  property string b: plural(n, { one: \"# file\" })\n}\n";
        let found = messages_qml(source);
        assert!(found.messages.is_empty());
        assert_eq!(found.notes.len(), 1);
        assert_eq!(found.notes[0].line, 5);
    }

    #[test]
    fn an_empty_message_is_noted_and_not_found() {
        let source = "import Ljos.Core\nItem {\n  property string a: t``\n  property string b: t({ message: \"\" })\n}\n";
        let found = messages_qml(source);
        assert!(found.messages.is_empty());
        let lines: Vec<u32> = found.notes.iter().map(|n| n.line).collect();
        assert_eq!(lines, vec![3, 4]);
    }

    #[test]
    fn a_module_has_the_macros_it_imports() {
        let source = "import { t, msg as m } from \"@ljos/core\";\n\nexport const a: string = t`Save`;\nexport const b = m`Not read`;\nexport const c = msg`Nor this`;\n";
        let found = messages_module("app.ts", source);
        assert_eq!(keys(&found), vec![("Save".to_owned(), None, 3)]);
    }

    #[test]
    fn a_choice_imported_only_as_a_type_is_a_value() {
        let source = "import { type plural, t } from \"@ljos/core\";\nimport type { select } from \"@ljos/core\";\nexport const a = t`${plural(n, { other: \"#\" })}`;\nexport const b = t`${select(r, { other: \"x\" })}`;\n";
        let found = messages_module("app.ts", source);
        assert_eq!(
            keys(&found),
            vec![("{0}".to_owned(), None, 3), ("{0}".to_owned(), None, 4)]
        );
    }

    #[test]
    fn a_choice_a_module_does_not_import_is_a_value() {
        let source = "import { t, select } from \"@ljos/core\";\nfunction plural(n: number, f: object) { return String(n); }\nexport const a = t`${plural(n, { one: \"# file\", other: \"# files\" })}`;\nexport const b = t`${select(r, { other: \"Member\" })}`;\nexport function c(select: unknown) { return t`${select}`; }\n";
        let found = messages_module("app.ts", source);
        assert_eq!(
            keys(&found),
            vec![
                ("{0}".to_owned(), None, 3),
                ("{r, select, other {Member}}".to_owned(), None, 4),
                ("{select}".to_owned(), None, 5),
            ]
        );
    }

    #[test]
    fn a_choice_the_document_binds_itself_is_a_value() {
        let source = "import QtQuick\nimport Ljos.Core\nItem {\n  function select(role, forms) { return forms.other }\n  property string a: t`${select(\"admin\", { other: \"Member\" })}`\n  property string b: t`${plural(n, { one: \"# file\", other: \"# files\" })}`\n  Item {\n    property string c: t`${select(\"admin\", { other: \"Member\" })}`\n    property string d: { const plural = (n) => n; return t`${plural(n, { other: \"#\" })}` }\n  }\n  Component {\n    Item {\n      property string e: t`${select(\"admin\", { other: \"Member\" })}`\n    }\n  }\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![
                ("{0}".to_owned(), None, 5),
                (
                    "{n, plural, one {# file} other {# files}}".to_owned(),
                    None,
                    6
                ),
                ("{0}".to_owned(), None, 8),
                ("{0}".to_owned(), None, 9),
                ("{0}".to_owned(), None, 13),
            ]
        );
    }

    #[test]
    fn a_choice_a_component_binds_is_a_value_only_inside_it() {
        let source = "import QtQuick\nimport Ljos.Core\nItem {\n  Component {\n    Item {\n      property string select\n    }\n  }\n  Item {\n    property string a: t`${select(r, { other: \"Member\" })}`\n  }\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![("{r, select, other {Member}}".to_owned(), None, 10)]
        );
    }

    /// A delegate, or a `property Component`'s object, is a component's
    /// root too, as `qml_front::scope` reads one without types.
    #[test]
    fn a_choice_a_delegate_binds_is_a_value_inside_it() {
        let source = "import QtQuick\nimport Ljos.Core\nItem {\n  ListView {\n    delegate: Item {\n      property string select\n      Text { text: t`${select(r, { other: \"Member\" })}` }\n    }\n  }\n  property Component row: Item {\n    function plural(n, forms) { return forms.other }\n    Text { text: t`${plural(n, { other: \"# rows\" })}` }\n  }\n  property string outside: t`${select(r, { other: \"Guest\" })}`\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![
                ("{0}".to_owned(), None, 7),
                ("{0}".to_owned(), None, 12),
                ("{r, select, other {Guest}}".to_owned(), None, 14),
            ]
        );
    }

    #[test]
    fn a_macro_a_script_declares_is_its_own() {
        let source = "import Ljos.Core\nItem {\n  function f() { const t = (s) => s; return t`Not a message` }\n  function g(msg) { return msg`Nor this` }\n  property string a: t`Hello`\n  property string b: { let t = String.raw; return t`Not this either` }\n}\n";
        let found = messages_qml(source);
        assert_eq!(keys(&found), vec![("Hello".to_owned(), None, 5)]);
    }

    /// A member or an id of the name is the document's, which the compiler
    /// does not lower either.
    #[test]
    fn a_macro_the_document_binds_is_its_own() {
        let source = "import Ljos.Core\nItem {\n  Item {\n    function t(parts) { return parts[0] }\n    property string a: t`Not a message`\n  }\n  Item { id: msg }\n  property string b: msg`Nor this`\n  property string c: t`Hello`\n}\n";
        let found = messages_qml(source);
        assert_eq!(keys(&found), vec![("Hello".to_owned(), None, 9)]);
    }

    #[test]
    fn a_message_beside_match_keeps_its_line() {
        let source = "import Ljos.Core\nItem {\n  property int n: 1\n  property string a: match (n) { when 1: t`One`; default: t`Many`; }\n  property string b: t`After`\n}\n";
        let found = messages_qml(source);
        assert_eq!(
            keys(&found),
            vec![
                ("One".to_owned(), None, 4),
                ("Many".to_owned(), None, 4),
                ("After".to_owned(), None, 5),
            ]
        );
    }

    #[test]
    fn an_lts_module_keeps_its_lines() {
        let source = "import { t } from \"@ljos/core\";\nconst k = 1 |> String(%) |> String(%);\n\nexport const a = t({ id: \"nav.home\", message: \"Home\" });\n";
        let found = messages_module("app.lts", source);
        assert_eq!(keys(&found), vec![("nav.home".to_owned(), None, 4)]);
        assert_eq!(found.messages[0].call.message.as_deref(), Some("Home"));
    }
}
