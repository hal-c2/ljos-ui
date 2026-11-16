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

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, Expression, ImportDeclarationSpecifier, ModuleDeclaration,
    TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::Span;
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject};

use super::message::{MessageCall, MessageError, call_message, tagged_message};
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
    let parsed = qml_syntax::parse(&allocator, source);
    let core = parsed.document.imports.iter().any(|import| {
        import.alias.is_none()
            && matches!(&import.source,
                QmlImportSource::Module(name) if name.to_string() == qml_front::names::CORE)
    });
    let mut finder = Finder::new(if core { MACROS.to_vec() } else { Vec::new() });
    if core && !parsed.panicked {
        finder.object(&parsed.document.root);
    }
    let mut messages = finder.finish(source, |span| span);
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
    let mut macros = Vec::new();
    for statement in &parsed.program.body {
        let Some(ModuleDeclaration::ImportDeclaration(import)) = statement.as_module_declaration()
        else {
            continue;
        };
        if import.source.value != crate::script::module::CORE_MODULE {
            continue;
        }
        for specifier in import.specifiers.iter().flatten() {
            if let ImportDeclarationSpecifier::ImportSpecifier(it) = specifier
                && it.imported.name() == it.local.name
                && let Some(name) = MACROS.iter().find(|m| **m == it.local.name.as_str())
            {
                macros.push(*name);
            }
        }
    }
    let mut finder = Finder::new(macros);
    if !finder.macros.is_empty() {
        finder.visit_program(&parsed.program);
    }
    let mut messages = finder.finish(source, map);
    messages.syntax_errors = parsed.diagnostics.len();
    messages
}

const MACROS: &[&str] = super::message::MACROS;

/// Walks a file's scripts and lists the macros in it, outermost first: a
/// macro inside another's message is part of that message.
struct Finder {
    macros: Vec<&'static str>,
    found: Vec<Result<MessageCall, MessageError>>,
}

impl Finder {
    fn new(macros: Vec<&'static str>) -> Self {
        Self {
            macros,
            found: Vec::new(),
        }
    }

    fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Object(child) => self.object(child),
                QmlMember::Binding(binding) => self.value(&binding.value),
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.value(value);
                    }
                }
                QmlMember::Function(function) => {
                    self.visit_function(function, ScopeFlags::Function);
                }
                QmlMember::InlineComponent(component) => self.object(&component.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'_>) {
        match value {
            QmlBindingValue::Expression(expression) => self.visit_expression(expression),
            QmlBindingValue::Statement(statement) => self.visit_statement(statement),
            QmlBindingValue::Object(object) => self.object(object),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object);
                }
            }
        }
    }

    /// Whether the macro `callee` names, at `span`, is one to read: the
    /// file's, and not part of a message already read.
    fn wanted(&self, callee: &Expression<'_>, span: Span) -> bool {
        let Expression::Identifier(id) = callee else {
            return false;
        };
        self.macros.contains(&id.name.as_str())
            && !self.found.iter().any(|outer| match outer {
                Ok(outer) => outer.covers(span),
                Err(outer) => outer.span.start <= span.start && span.end <= outer.span.end,
            })
    }

    fn finish(self, source: &str, map: impl Fn(Span) -> Span) -> Messages {
        let lines = Lines::new(source);
        let mut messages = Messages::default();
        for found in self.found {
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
}

impl<'a> Visit<'a> for Finder {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.wanted(&it.callee, it.span)
            && let Some(found) = call_message(it)
        {
            self.found.push(found);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if self.wanted(&it.tag, it.span)
            && let Some(found) = tagged_message(it)
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
    fn an_lts_module_keeps_its_lines() {
        let source = "import { t } from \"@ljos/core\";\nconst k = 1 |> String(%) |> String(%);\n\nexport const a = t({ id: \"nav.home\", message: \"Home\" });\n";
        let found = messages_module("app.lts", source);
        assert_eq!(keys(&found), vec![("nav.home".to_owned(), None, 4)]);
        assert_eq!(found.messages[0].call.message.as_deref(), Some("Home"));
    }
}
