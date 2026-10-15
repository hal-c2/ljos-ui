//! Finds the translatable strings of QML and JavaScript sources: every call
//! of `qsTr`, `qsTranslate`, `qsTrId` and the `QT_TR*_NOOP` markers whose
//! text is written out, with the translator comments in front of it
//! (ARCHITECTURE.md §15).
//!
//! This needs no types, so it works on a file alone and on files the compiler
//! cannot compile yet. Which call a comment belongs to, and what counts as a
//! written-out text, follow what `lupdate` of Qt 6.11 does with the same
//! file, observed from outside.

use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, BinaryExpression, CallExpression, Comment, Expression, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::{operator::BinaryOperator, scope::ScopeFlags};
use qml_syntax::ast::{QmlBindingValue, QmlImportSource, QmlMember, QmlObject};

/// One translatable string as it stands in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// `""` for a text found by id.
    pub context: String,
    /// The text to translate. For a text id, what `//% "…"` gives, or `""`.
    pub source: String,
    /// The second argument of `qsTr`, `""` for none.
    pub disambiguation: String,
    /// The argument of `qsTrId`, or what `//= id` names.
    pub id: Option<String>,
    /// The call passes a count, so the text has plural forms.
    pub plural: bool,
    /// `//: …`, the note for the translator.
    pub extra_comment: Option<String>,
    /// `//~ key value`, in the order written.
    pub extras: Vec<(String, String)>,
    /// The line of the call, from 1.
    pub line: u32,
}

/// Something in a file that the translator will not get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub line: u32,
    pub message: String,
}

/// What one file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraction {
    /// In source order.
    pub messages: Vec<Message>,
    pub notes: Vec<Note>,
    /// The JavaScript files this file imports, as written.
    pub scripts: Vec<String>,
    /// How many places did not parse. What did parse is still searched.
    pub syntax_errors: usize,
}

/// The translatable strings of a QML file. `stem` is its name without
/// `.qml`: the context of `qsTr` unless the file has `pragma Translator`.
pub fn extract_qml(source: &str, stem: &str) -> Extraction {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let context = crate::compile::translator(&parsed.document).unwrap_or(stem);
    let mut finder = Finder::default();
    if !parsed.panicked {
        finder.object(&parsed.document.root);
    }
    let scripts = parsed
        .document
        .imports
        .iter()
        .filter_map(|import| match &import.source {
            QmlImportSource::Path(path) if path.ends_with(".js") || path.ends_with(".mjs") => {
                Some((*path).to_owned())
            }
            _ => None,
        })
        .collect();
    let mut extraction = finder.finish(source, &parsed.comments, context);
    extraction.scripts = scripts;
    extraction.syntax_errors = parsed.diagnostics.len();
    extraction
}

/// The translatable strings of a JavaScript file a QML file imports. `stem`
/// is its name without `.js`, which is its context.
pub fn extract_js(source: &str, stem: &str) -> Extraction {
    let (text, scripts) = without_directives(source);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &text, SourceType::script()).parse();
    let mut finder = Finder::default();
    finder.visit_program(&parsed.program);
    let mut extraction = finder.finish(&text, &parsed.program.comments, stem);
    extraction.scripts = scripts;
    extraction.syntax_errors = parsed.diagnostics.len();
    extraction
}

/// A QML script file with its `.pragma` and `.import` lines blanked, which
/// are not JavaScript, and the script files those lines import. Blanked
/// rather than removed, so that every position stays where it was.
pub(crate) fn without_directives(source: &str) -> (String, Vec<String>) {
    let mut text = String::with_capacity(source.len());
    let mut scripts = Vec::new();
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with(".pragma") || trimmed.starts_with(".import") {
            if let Some(rest) = trimmed.strip_prefix(".import") {
                let rest = rest.trim_start();
                if let Some(path) = rest
                    .strip_prefix('"')
                    .and_then(|rest| rest.split_once('"'))
                    .map(|(path, _)| path)
                    && path.ends_with(".js")
                {
                    scripts.push(path.to_owned());
                }
            }
            text.extend(
                line.chars()
                    .map(|c| if c == '\n' || c == '\r' { c } else { ' ' }),
            );
        } else {
            text.push_str(line);
        }
    }
    (text, scripts)
}

/// Which translation function a call names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Function {
    Tr,
    Translate,
    TrId,
}

impl Function {
    /// The `QT_TR*_NOOP` markers are read as the function they stand for:
    /// what they mark is looked up later with the same arguments.
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "qsTr" | "QT_TR_NOOP" => Self::Tr,
            "qsTranslate" | "QT_TRANSLATE_NOOP" => Self::Translate,
            "qsTrId" | "QT_TRID_NOOP" => Self::TrId,
            _ => return None,
        })
    }
}

/// A call found in the tree, before its comments are known.
#[derive(Debug)]
struct Call {
    function: Function,
    /// `None`: the context of the file.
    context: Option<String>,
    text: String,
    disambiguation: String,
    plural: bool,
}

#[derive(Debug)]
enum Event {
    Call(Call),
    /// A binding or a statement ends here: comments nothing has taken by
    /// now belong to nothing.
    End,
    Note(String),
}

/// Walks the scripts of a file and lists, by position, the translation calls
/// and where each binding and statement ends.
#[derive(Default)]
struct Finder {
    events: Vec<(u32, Event)>,
}

impl Finder {
    fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Object(child) => self.object(child),
                QmlMember::Binding(binding) => self.value(&binding.value, binding.span.end),
                QmlMember::Property(property) => {
                    if let Some(value) = &property.value {
                        self.value(value, property.span.end);
                    }
                }
                QmlMember::Function(function) => {
                    self.visit_function(function, ScopeFlags::Function);
                    self.events.push((function.span.end, Event::End));
                }
                QmlMember::InlineComponent(component) => self.object(&component.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'_>, end: u32) {
        match value {
            QmlBindingValue::Expression(expression) => {
                self.visit_expression(expression);
                self.events.push((end, Event::End));
            }
            QmlBindingValue::Statement(statement) => self.visit_statement(statement),
            QmlBindingValue::Object(object) => self.object(object),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.object(object);
                }
            }
        }
    }

    fn call(&mut self, call: &CallExpression<'_>) {
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let Some(function) = Function::named(&callee.name) else {
            return;
        };
        let name = callee.name.as_str();
        let args: Vec<Text> = call.arguments.iter().map(text_of).collect();
        let at = call.span.start;
        // Where the text is among the arguments: after the context for the
        // functions that name one.
        let first = usize::from(function == Function::Translate);
        let context = match (function, args.first()) {
            (Function::Translate, Some(Text::Written(context))) => Some(context.clone()),
            // The context is worked out when the program runs: nothing a
            // catalogue could list.
            (Function::Translate, _) => return,
            _ => None,
        };
        let text = match args.get(first) {
            Some(Text::Written(text)) => text.clone(),
            Some(Text::Template) => {
                self.events.push((
                    at,
                    Event::Note(format!(
                        "{name}() template strings with arguments are not supported for translation."
                    )),
                ));
                return;
            }
            _ => return,
        };
        let (disambiguation, plural) = match function {
            Function::TrId => (String::new(), args.len() > 1),
            _ => (
                match args.get(first + 1) {
                    Some(Text::Written(text)) => text.clone(),
                    _ => String::new(),
                },
                args.len() > first + 2,
            ),
        };
        self.events.push((
            at,
            Event::Call(Call {
                function,
                context,
                text,
                disambiguation,
                plural,
            }),
        ));
    }

    /// Joins the calls with the comments in front of them.
    fn finish(mut self, source: &str, comments: &[Comment], context: &str) -> Extraction {
        let lines = Lines::new(source);
        let mut events: Vec<(u32, Step)> = self
            .events
            .drain(..)
            .map(|(at, event)| (at, Step::Event(event)))
            .collect();
        for comment in comments {
            let text = &source[comment.span.start as usize..comment.span.end as usize];
            if let Some(meta) = Meta::parse(text) {
                events.push((comment.span.start, Step::Meta(meta)));
            }
        }
        // Stable, so that a call and the end of its binding at one position
        // (there are none in valid code) keep the order they were found in.
        events.sort_by_key(|(at, _)| *at);

        let mut extraction = Extraction::default();
        let mut pending = Pending::default();
        for (at, step) in events {
            let line = lines.line(at);
            match step {
                Step::Meta(meta) => pending.add(meta),
                Step::Event(Event::Note(message)) => extraction.notes.push(Note { line, message }),
                Step::Event(Event::End) => {
                    if !pending.is_empty() {
                        extraction.notes.push(Note {
                            line,
                            message: "Discarding unconsumed meta data".to_owned(),
                        });
                        pending = Pending::default();
                    }
                }
                Step::Event(Event::Call(call)) => {
                    let taken = std::mem::take(&mut pending);
                    let message = match call.function {
                        Function::TrId => Message {
                            context: String::new(),
                            source: taken.source.unwrap_or_default(),
                            disambiguation: String::new(),
                            id: Some(call.text),
                            plural: call.plural,
                            extra_comment: taken.comment,
                            extras: taken.extras,
                            line,
                        },
                        Function::Tr | Function::Translate => {
                            if taken.source.is_some() {
                                extraction.notes.push(Note {
                                    line,
                                    message: "//% cannot be used with qsTr(). Ignoring".to_owned(),
                                });
                            }
                            Message {
                                context: call.context.unwrap_or_else(|| context.to_owned()),
                                source: call.text,
                                disambiguation: call.disambiguation,
                                id: taken.id,
                                plural: call.plural,
                                extra_comment: taken.comment,
                                extras: taken.extras,
                                line,
                            }
                        }
                    };
                    extraction.messages.push(message);
                }
            }
        }
        extraction
    }
}

impl<'a> Visit<'a> for Finder {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.call(it);
        walk::walk_call_expression(self, it);
    }

    fn visit_statement(&mut self, it: &Statement<'a>) {
        walk::walk_statement(self, it);
        self.events
            .push((oxc_span::GetSpan::span(it).end, Event::End));
    }
}

enum Step {
    Event(Event),
    Meta(Meta),
}

/// What an argument of a translation call is.
enum Text {
    /// A text that stands in the source: a string, a template without
    /// placeholders, or such texts joined with `+`.
    Written(String),
    /// A template with placeholders, which has no one text.
    Template,
    Other,
}

fn text_of(argument: &Argument<'_>) -> Text {
    match argument {
        Argument::SpreadElement(_) => Text::Other,
        argument => written(argument.to_expression()),
    }
}

fn written(expression: &Expression<'_>) -> Text {
    match expression {
        Expression::StringLiteral(string) => Text::Written(string.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map_or(Text::Other, |cooked| Text::Written(cooked.to_string())),
        Expression::TemplateLiteral(_) => Text::Template,
        Expression::ParenthesizedExpression(inner) => written(&inner.expression),
        Expression::BinaryExpression(binary) => joined(binary),
        _ => Text::Other,
    }
}

fn joined(binary: &BinaryExpression<'_>) -> Text {
    if binary.operator != BinaryOperator::Addition {
        return Text::Other;
    }
    match (written(&binary.left), written(&binary.right)) {
        (Text::Written(left), Text::Written(right)) => Text::Written(left + &right),
        _ => Text::Other,
    }
}

/// A comment that says something about the next translation call.
#[derive(Debug, PartialEq, Eq)]
enum Meta {
    /// `//: …`
    Comment(String),
    /// `//~ key value`
    Extra(String, String),
    /// `//= id`
    Id(String),
    /// `//% "text"`
    Source(String),
}

impl Meta {
    /// `text` is the whole comment, with its `//` or `/* */`.
    fn parse(text: &str) -> Option<Self> {
        let body = match text.strip_prefix("//") {
            Some(body) => body,
            None => text.strip_prefix("/*")?.strip_suffix("*/")?,
        };
        let mut chars = body.chars();
        let kind = chars.next()?;
        let rest = chars.as_str();
        // Qt takes the mark only when white space follows it, which keeps
        // `//=====` and `//:)` ordinary comments.
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        match kind {
            ':' => Some(simplified(rest))
                .filter(|note| !note.is_empty())
                .map(Self::Comment),
            '~' => {
                let rest = rest.trim();
                let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                (!key.is_empty()).then(|| Self::Extra(key.to_owned(), value.trim().to_owned()))
            }
            '=' => Some(Self::Id(rest.trim().to_owned())),
            '%' => quoted(rest.trim()).map(Self::Source),
            _ => None,
        }
    }
}

/// White space runs as one space, none at the ends.
fn simplified(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of a `"…"` literal with C escapes, as `//%` writes it.
fn quoted(text: &str) -> Option<String> {
    let mut chars = text.strip_prefix('"')?.chars();
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
}

/// The comments waiting for a call.
#[derive(Default)]
struct Pending {
    comment: Option<String>,
    extras: Vec<(String, String)>,
    id: Option<String>,
    source: Option<String>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.comment.is_none()
            && self.extras.is_empty()
            && self.id.is_none()
            && self.source.is_none()
    }

    fn add(&mut self, meta: Meta) {
        match meta {
            // Several lines make one note.
            Meta::Comment(text) => match &mut self.comment {
                Some(comment) => {
                    comment.push(' ');
                    comment.push_str(&text);
                }
                None => self.comment = Some(text),
            },
            Meta::Extra(key, value) => self.extras.push((key, value)),
            Meta::Id(id) => self.id = Some(id),
            Meta::Source(text) => self.source.get_or_insert_default().push_str(&text),
        }
    }
}

/// Line numbers of byte positions.
struct Lines {
    starts: Vec<u32>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(at, _)| at as u32 + 1),
        );
        Self { starts }
    }

    fn line(&self, at: u32) -> u32 {
        self.starts.partition_point(|start| *start <= at) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qml(body: &str) -> Extraction {
        extract_qml(&format!("import QtQuick\nItem {{\n{body}\n}}\n"), "Main")
    }

    /// `source`, `disambiguation`, `plural` of each message.
    fn texts(extraction: &Extraction) -> Vec<(&str, &str, bool)> {
        extraction
            .messages
            .iter()
            .map(|m| (m.source.as_str(), m.disambiguation.as_str(), m.plural))
            .collect()
    }

    fn comments(extraction: &Extraction) -> Vec<(&str, Option<&str>)> {
        extraction
            .messages
            .iter()
            .map(|m| (m.source.as_str(), m.extra_comment.as_deref()))
            .collect()
    }

    #[test]
    fn every_function_of_the_family_is_found() {
        let found = qml(r#"
            property string a: qsTr("A")
            property string b: qsTr("B", "dis")
            property string c: qsTr("C", "dis", 3)
            property string d: qsTranslate("Ctx", "D", "dis", 2)
            property string e: QT_TR_NOOP("E", "dis")
            property string f: QT_TRANSLATE_NOOP("Ctx", "F")
            property string g: qsTrId("g-id")
            property string h: qsTrId("h-id", 2)
            property string i: QT_TRID_NOOP("i-id")
        "#);
        let seen: Vec<_> = found
            .messages
            .iter()
            .map(|m| {
                (
                    m.context.as_str(),
                    m.source.as_str(),
                    m.disambiguation.as_str(),
                    m.id.as_deref(),
                    m.plural,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("Main", "A", "", None, false),
                ("Main", "B", "dis", None, false),
                ("Main", "C", "dis", None, true),
                ("Ctx", "D", "dis", None, true),
                ("Main", "E", "dis", None, false),
                ("Ctx", "F", "", None, false),
                ("", "", "", Some("g-id"), false),
                ("", "", "", Some("h-id"), true),
                ("", "", "", Some("i-id"), false),
            ]
        );
        assert_eq!(found.notes, []);
    }

    #[test]
    fn calls_are_found_wherever_a_script_can_stand() {
        let found = qml(r#"
            property var list: [qsTr("in a list")]
            function f(a = qsTr("default")) { if (a) { return qsTr("in a function") } }
            onWidthChanged: { let s = () => qsTr("in a handler"); }
            Text { text: enabled ? qsTr("in a child") : "" }
            component Tag: Text { text: qsTr("in an inline component") }
            Repeater { delegate: Text { text: qsTr("in a delegate") } }
            states: [ State { name: qsTr("in an object list") } ]
        "#);
        assert_eq!(
            texts(&found).iter().map(|t| t.0).collect::<Vec<_>>(),
            [
                "in a list",
                "default",
                "in a function",
                "in a handler",
                "in a child",
                "in an inline component",
                "in a delegate",
                "in an object list",
            ]
        );
    }

    #[test]
    fn only_a_text_that_is_written_out_is_a_message() {
        let found = qml(r#"
            property string a: qsTr("one " + 'two ' + `three`)
            property string b: qsTr(("paren"))
            property string c: qsTr(a)
            property string d: qsTr("x" + a)
            property string e: qsTr(`with ${a}`)
            property string f: qsTr("escapes \n\t\"q\" \u00e9 \x41")
            property string g: qsTranslate(a, "no context")
            property string h: qsTr("T", undefined, 1)
            property string i: Qt.qsTr("not the global")
        "#);
        assert_eq!(
            texts(&found),
            [
                ("one two three", "", false),
                ("paren", "", false),
                ("escapes \n\t\"q\" \u{e9} A", "", false),
                ("T", "", true),
            ]
        );
        assert_eq!(
            found.notes,
            [Note {
                line: 8,
                message:
                    "qsTr() template strings with arguments are not supported for translation."
                        .to_owned()
            }]
        );
    }

    #[test]
    fn the_line_is_where_the_call_starts() {
        let found = qml("property string a: qsTr(\n\"A\")\nproperty string b:\n  qsTr(\"B\")");
        let lines: Vec<_> = found.messages.iter().map(|m| m.line).collect();
        assert_eq!(lines, [3, 6]);
    }

    #[test]
    fn a_comment_belongs_to_the_next_call_of_the_same_binding_or_statement() {
        let found = qml(r#"
            //: first line
            //: second   line
            // an ordinary comment
            property string a: qsTr("A")
            property string b: qsTr("B") //: after b
            property string c: qsTr("C")
            /*: a block
                comment */
            property string d: "x" + qsTr("D") + qsTr("D2")
            //: on an object
            Text { text: qsTr("E") }
            property string f: qsTr("F" /*: inside f */) + qsTr("F2")
            function g() {
                //: in a function
                return qsTr("G")
            }
        "#);
        assert_eq!(
            comments(&found),
            [
                ("A", Some("first line second line")),
                ("B", None),
                ("C", Some("after b")),
                ("D", Some("a block comment")),
                ("D2", None),
                ("E", Some("on an object")),
                ("F", None),
                ("F2", Some("inside f")),
                ("G", Some("in a function")),
            ]
        );
        assert_eq!(found.notes, []);
    }

    #[test]
    fn a_comment_nothing_takes_is_dropped_where_its_binding_or_statement_ends() {
        let found = qml(r#"
            //: lost on a plain property
            property int z: 1
            property string a: qsTr("A")
            function f() {
                //: lost on a plain statement
                var x = 1
                return qsTr("B")
            }
            //: lost on the first binding of an object
            Text {
                width: 1
                text: qsTr("C")
            }
            property string d: qsTr(
                //: lost inside the call
                "D")
            //: lost on a text that is not written out
            property string e: qsTr(a)
            property string g: qsTr("G")
        "#);
        assert_eq!(
            comments(&found),
            [
                ("A", None),
                ("B", None),
                ("C", None),
                ("D", None),
                ("G", None)
            ]
        );
        let lines: Vec<_> = found.notes.iter().map(|n| n.line).collect();
        assert_eq!(lines, [5, 9, 14, 19, 21]);
        assert!(
            found
                .notes
                .iter()
                .all(|n| n.message.starts_with("Discarding"))
        );
    }

    #[test]
    fn extras_ids_and_id_sources() {
        let found = qml(r#"
            //~ Scope global
            //~ Group  the  rest
            property string a: qsTr("A")
            //= explicit-id
            property string b: qsTr("B")
            //% "Id source"
            //% " and \"more\""
            //: for the id
            property string c: qsTrId("c-id")
            //% "not for qsTr"
            property string d: qsTr("D")
            //===== a rule, not an id
            //:) not a note
            property string e: qsTr("E")
        "#);
        let a = &found.messages[0];
        assert_eq!(
            a.extras,
            [
                ("Scope".to_owned(), "global".to_owned()),
                ("Group".to_owned(), "the  rest".to_owned())
            ]
        );
        let b = &found.messages[1];
        assert_eq!(
            (b.context.as_str(), b.id.as_deref()),
            ("Main", Some("explicit-id"))
        );
        let c = &found.messages[2];
        assert_eq!(c.id.as_deref(), Some("c-id"));
        assert_eq!(c.source, "Id source and \"more\"");
        assert_eq!(c.extra_comment.as_deref(), Some("for the id"));
        let d = &found.messages[3];
        assert_eq!((d.source.as_str(), d.id.as_deref()), ("D", None));
        let e = &found.messages[4];
        assert_eq!((e.id.as_deref(), e.extra_comment.as_deref()), (None, None));
        assert_eq!(found.notes.len(), 1);
        assert!(
            found.notes[0]
                .message
                .contains("//% cannot be used with qsTr()")
        );
    }

    #[test]
    fn the_context_is_the_file_or_what_the_pragma_names() {
        let plain = extract_qml(
            "import QtQuick\nItem { property string a: qsTr(\"A\") }",
            "Page",
        );
        assert_eq!(plain.messages[0].context, "Page");
        let named = extract_qml(
            "pragma Translator: Shared\nimport QtQuick\nItem { property string a: qsTr(\"A\") }",
            "Page",
        );
        assert_eq!(named.messages[0].context, "Shared");
    }

    #[test]
    fn script_files_and_what_they_import() {
        let found = extract_qml(
            "import QtQuick\nimport \"util.js\" as Util\nimport \"parts\"\nItem {}",
            "Main",
        );
        assert_eq!(found.scripts, ["util.js"]);

        let found = extract_js(
            ".pragma library\n.import \"other.js\" as Other\n.import QtQuick as Q\n\
             //: from a script\nfunction f() { return qsTr(\"JS\") }\n",
            "util",
        );
        assert_eq!(found.scripts, ["other.js"]);
        assert_eq!(found.syntax_errors, 0);
        let m = &found.messages[0];
        assert_eq!(
            (
                m.context.as_str(),
                m.source.as_str(),
                m.line,
                m.extra_comment.as_deref()
            ),
            ("util", "JS", 5, Some("from a script"))
        );
    }

    #[test]
    fn a_file_with_a_mistake_still_gives_what_parses() {
        let found =
            qml("property string a: qsTr(\"A\")\nwidth: = 3\nproperty string b: qsTr(\"B\")");
        assert!(found.syntax_errors > 0);
        assert_eq!(texts(&found), [("A", "", false), ("B", "", false)]);
    }
}
