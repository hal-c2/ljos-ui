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
//! A choice (`plural`, `select`, `selectOrdinal`) inside a message is a
//! form of it only where the name is the macro, as the compiler reads it:
//! in a module, where the name is the import; in a QML file, where neither
//! the script, an id, the object the script is on nor the root of a
//! component around it binds the name. What a type gives its objects is
//! not known without types: a choice named as a member of the object's type
//! (`TextInput`'s `select`) is read as a form here and as a value by the
//! compiler.

use oxc_allocator::Allocator;
use std::collections::HashSet;

use oxc_ast::ast::{
    BindingIdentifier, CallExpression, Expression, IdentifierReference, ImportDeclarationSpecifier,
    ModuleDeclaration, TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::Span;
use oxc_syntax::scope::ScopeFlags;
use oxc_syntax::symbol::SymbolId;
use qml_syntax::ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject};

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
    let parsed = qml_syntax::parse(&allocator, source);
    let core = parsed.document.imports.iter().any(|import| {
        import.alias.is_none()
            && matches!(&import.source,
                QmlImportSource::Module(name) if name.to_string() == qml_front::names::CORE)
    });
    let mut finder = Finder::new(Macros::Qml {
        names: if core { MACROS.to_vec() } else { Vec::new() },
        ids: HashSet::new(),
        bound: HashSet::new(),
    });
    if core && !parsed.panicked {
        if let Macros::Qml { ids, .. } = &mut finder.macros {
            ids_of(&parsed.document.root, ids);
        }
        let root = declared(&parsed.document.root);
        finder.object(&parsed.document.root, &[root]);
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
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let mut symbols = HashSet::new();
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
                && MACROS.contains(&it.local.name.as_str())
            {
                symbols.insert(it.local.symbol_id());
            }
        }
    }
    let any = !symbols.is_empty();
    let mut finder = Finder::new(Macros::Module {
        scoping: semantic.scoping(),
        symbols,
    });
    if any {
        finder.visit_program(&parsed.program);
    }
    let mut messages = finder.finish(source, map);
    messages.syntax_errors = parsed.diagnostics.len();
    messages
}

const MACROS: &[&str] = super::message::MACROS;

/// The choices a message may hold, which a QML document may bind itself.
const CHOICES: &[&str] = &["plural", "select", "selectOrdinal"];

/// Which names a file's scripts call the macros by.
enum Macros<'s> {
    /// A QML file's: by name, but for a choice bound in sight.
    Qml {
        names: Vec<&'static str>,
        /// Every id of the file. A component's ids are in sight of the
        /// components inside it, so this is more than the compiler sees
        /// only where an inner component's id names a choice.
        ids: HashSet<String>,
        /// The choices the script being read sees bound: by the object it
        /// is on, a component root around it, or the script itself.
        bound: HashSet<String>,
    },
    /// A module's: the symbols of its imports from `@ljos/core`.
    Module {
        scoping: &'s Scoping,
        symbols: HashSet<SymbolId>,
    },
}

impl Macros<'_> {
    fn is_macro(&self, id: &IdentifierReference<'_>) -> bool {
        match self {
            Macros::Qml { names, ids, bound } => {
                let name = id.name.as_str();
                names.contains(&name)
                    && !(CHOICES.contains(&name) && (ids.contains(name) || bound.contains(name)))
            }
            Macros::Module { scoping, symbols } => id
                .reference_id
                .get()
                .and_then(|reference| scoping.get_reference(reference).symbol_id())
                .is_some_and(|symbol| symbols.contains(&symbol)),
        }
    }
}

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

/// The names a script declares anywhere in it. One declared in a function
/// the message is not inside is counted too: a script that binds a choice's
/// name for something else is rare enough not to need its scopes.
#[derive(Default)]
struct Bindings(HashSet<String>);

impl<'a> Visit<'a> for Bindings {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if CHOICES.contains(&it.name.as_str()) {
            self.0.insert(it.name.to_string());
        }
    }
}

/// Walks a file's scripts and lists the macros in it, outermost first: a
/// macro inside another's message is part of that message.
struct Finder<'s> {
    macros: Macros<'s>,
    found: Vec<Result<MessageCall, MessageError>>,
}

impl Finder<'_> {
    fn new(macros: Macros<'_>) -> Finder<'_> {
        Finder {
            macros,
            found: Vec::new(),
        }
    }

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
                QmlMember::Binding(binding) => self.value(&binding.value, &own, roots),
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.value(value, &own, roots);
                    }
                }
                QmlMember::Function(function) => {
                    self.script(&own, roots, |finder| {
                        let mut bindings = Bindings::default();
                        bindings.visit_function(function, ScopeFlags::Function);
                        finder.bind(bindings);
                        finder.visit_function(function, ScopeFlags::Function);
                    });
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

    fn value(&mut self, value: &QmlBindingValue<'_>, own: &[String], roots: &[Vec<String>]) {
        match value {
            QmlBindingValue::Expression(expression) => self.script(own, roots, |finder| {
                let mut bindings = Bindings::default();
                bindings.visit_expression(expression);
                finder.bind(bindings);
                finder.visit_expression(expression);
            }),
            QmlBindingValue::Statement(statement) => self.script(own, roots, |finder| {
                let mut bindings = Bindings::default();
                bindings.visit_statement(statement);
                finder.bind(bindings);
                finder.visit_statement(statement);
            }),
            QmlBindingValue::Object(object) => self.object(object, roots),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object, roots);
                }
            }
        }
    }

    /// Reads one script of the object whose members are `own`.
    fn script(&mut self, own: &[String], roots: &[Vec<String>], read: impl FnOnce(&mut Self)) {
        if let Macros::Qml { bound, .. } = &mut self.macros {
            bound.clear();
            bound.extend(
                own.iter()
                    .chain(roots.iter().flatten())
                    .filter(|name| CHOICES.contains(&name.as_str()))
                    .cloned(),
            );
        }
        read(self);
    }

    fn bind(&mut self, bindings: Bindings) {
        if let Macros::Qml { bound, .. } = &mut self.macros {
            bound.extend(bindings.0);
        }
    }

    /// Whether the macro `callee` names, at `span`, is one to read: the
    /// file's, and not part of a message already read.
    fn wanted(&self, callee: &Expression<'_>, span: Span) -> bool {
        let Expression::Identifier(id) = callee else {
            return false;
        };
        self.macros.is_macro(id)
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

impl<'a> Visit<'a> for Finder<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.wanted(&it.callee, it.span)
            && let Some(found) = call_message_in(it, &|id| self.macros.is_macro(id))
        {
            self.found.push(found);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        if self.wanted(&it.tag, it.span)
            && let Some(found) = tagged_message_in(it, &|id| self.macros.is_macro(id))
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

    #[test]
    fn an_lts_module_keeps_its_lines() {
        let source = "import { t } from \"@ljos/core\";\nconst k = 1 |> String(%) |> String(%);\n\nexport const a = t({ id: \"nav.home\", message: \"Home\" });\n";
        let found = messages_module("app.lts", source);
        assert_eq!(keys(&found), vec![("nav.home".to_owned(), None, 4)]);
        assert_eq!(found.messages[0].call.message.as_deref(), Some("Home"));
    }
}
