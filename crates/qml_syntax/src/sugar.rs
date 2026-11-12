//! `match` and `is` written away (docs/DIALECT.md §4.21): one desugar,
//! which every reader of a script calls before it reads one.

use oxc_allocator::Allocator;
use oxc_ast::ast::Comment;
use oxc_ast_visit::VisitMut;
use oxc_parser::{
    Parser,
    qml::{
        ast::{
            QmlBindingValue, QmlDocument, QmlImport, QmlMember, QmlObject, QmlQualifiedName,
            QmlTypeName,
        },
        sugar::{self, Edit},
    },
};
use oxc_span::{SourceType, Span};

pub use oxc_parser::qml::sugar::SpanMap;

use crate::{Diagnostic, Parsed};

/// A script's text with `match` and `is` written away.
#[derive(Debug, Clone)]
pub struct Desugared {
    /// Plain TypeScript: line N of it is line N of the input.
    pub text: String,
    /// From offsets in `text` to offsets in the input.
    pub map: SpanMap,
    /// The input's syntax errors, the desugar's among them, at the input's
    /// spans. `text` is still written where there are some: a pattern in
    /// error matches nothing.
    pub diagnostics: Vec<Diagnostic>,
    /// The keyword of each `match` and `is` in the input, in order.
    pub uses: Vec<Span>,
}

/// `text`, a script or module, with `match` and `is` written away. It is
/// read as a TypeScript module, which a QML file's script, an `.lts` module
/// and a `.ts` module all parse as.
pub fn desugar(text: &str) -> Desugared {
    let allocator = Allocator::default();
    let source_type = SourceType::ts().with_module(true);
    let (parsed, edits) = Parser::new(&allocator, text, source_type).parse_with_sugar();
    let (lowered, map) = lower(text, &edits);
    Desugared {
        text: lowered,
        map,
        diagnostics: parsed
            .diagnostics
            .into_iter()
            .map(Diagnostic::from)
            .collect(),
        uses: uses(&edits),
    }
}

fn lower(text: &str, edits: &[Edit]) -> (String, SpanMap) {
    if edits.is_empty() {
        (text.to_owned(), SpanMap::identity(text.len() as u32))
    } else {
        sugar::lower(text, edits)
    }
}

fn uses(edits: &[Edit]) -> Vec<Span> {
    let mut uses: Vec<Span> = edits.iter().filter_map(|e| e.keyword).collect();
    uses.sort_by_key(|s| s.start);
    uses
}

/// A QML document's desugared text, and its tree.
#[derive(Debug)]
pub struct Lowered<'a> {
    /// The document with `match` and `is` written away: `source` itself
    /// when it has neither.
    pub text: &'a str,
    /// The tree of `text`, at `text`'s spans. Its diagnostics are the
    /// source's, at the source's spans.
    pub parsed: Parsed<'a>,
    /// From offsets in `text` to offsets in the source.
    pub map: SpanMap,
    /// The keyword of each `match` and `is` in the source, in order.
    pub uses: Vec<Span>,
}

/// Parses the QML document `source`, and parses it again desugared when it
/// uses `match` or `is`.
pub fn parse_lowered<'a>(allocator: &'a Allocator, source: &'a str) -> Lowered<'a> {
    let (first, edits) = crate::parse_first(allocator, source);
    if edits.is_empty() || first.panicked {
        return Lowered {
            text: source,
            parsed: first,
            map: SpanMap::identity(source.len() as u32),
            uses: uses(&edits),
        };
    }
    let (text, map) = sugar::lower(source, &edits);
    let text: &'a str = allocator.alloc_str(&text);
    let (mut parsed, more) = crate::parse_first(allocator, text);
    debug_assert!(more.is_empty(), "the desugared text has no `match` or `is`");
    // What the source has wrong is said at its own spans; the desugared
    // text is wrong where the source is, or in no other place.
    parsed.diagnostics = first.diagnostics;
    Lowered {
        text,
        parsed,
        map,
        uses: uses(&edits),
    }
}

/// Moves every span of `parsed`, a desugared document's tree, to the
/// source's.
pub(crate) fn remap(parsed: &mut Parsed<'_>, map: &SpanMap) {
    let mut remap = Remap(map);
    let document: &mut QmlDocument<'_> = &mut parsed.document;
    document.span = map.span(document.span);
    for pragma in &mut document.pragmas {
        pragma.span = map.span(pragma.span);
    }
    for import in &mut document.imports {
        remap.import(import);
    }
    remap.object(&mut document.root);
    for comment in &mut parsed.comments {
        remap.comment(comment);
    }
}

struct Remap<'m>(&'m SpanMap);

impl Remap<'_> {
    fn import(&mut self, import: &mut QmlImport<'_>) {
        import.span = self.0.span(import.span);
        if let oxc_parser::qml::ast::QmlImportSource::Module(name) = &mut import.source {
            self.name(name);
        }
        if let Some(alias) = &mut import.alias {
            self.visit_identifier_name(alias);
        }
    }

    fn name(&mut self, name: &mut QmlQualifiedName<'_>) {
        name.span = self.0.span(name.span);
    }

    fn type_name(&mut self, ty: &mut QmlTypeName<'_>) {
        ty.span = self.0.span(ty.span);
        self.name(&mut ty.name);
    }

    fn object(&mut self, object: &mut QmlObject<'_>) {
        object.span = self.0.span(object.span);
        self.name(&mut object.type_name);
        if let Some(on) = &mut object.on {
            self.name(on);
        }
        for member in &mut object.members {
            self.member(member);
        }
    }

    fn value(&mut self, value: &mut QmlBindingValue<'_>) {
        match value {
            QmlBindingValue::Expression(e) => self.visit_expression(e),
            QmlBindingValue::Statement(s) => self.visit_statement(s),
            QmlBindingValue::Object(o) => self.object(o),
            QmlBindingValue::Objects(list) => {
                for o in list {
                    self.object(o);
                }
            }
        }
    }

    fn member(&mut self, member: &mut QmlMember<'_>) {
        match member {
            QmlMember::Object(o) => self.object(o),
            QmlMember::Binding(b) => {
                b.span = self.0.span(b.span);
                self.name(&mut b.name);
                self.value(&mut b.value);
            }
            QmlMember::Property(p) => {
                p.span = self.0.span(p.span);
                if let Some(ty) = &mut p.type_name {
                    self.type_name(ty);
                }
                self.visit_identifier_name(&mut p.name);
                if let Some(value) = &mut p.value {
                    self.value(value);
                }
            }
            QmlMember::Signal(s) => {
                s.span = self.0.span(s.span);
                self.visit_identifier_name(&mut s.name);
                for param in &mut s.params {
                    self.visit_identifier_name(&mut param.name);
                    if let Some(ty) = &mut param.type_name {
                        self.type_name(ty);
                    }
                }
            }
            QmlMember::Function(f) => {
                self.visit_function(f, oxc_syntax::scope::ScopeFlags::Function)
            }
            QmlMember::InlineComponent(c) => {
                c.span = self.0.span(c.span);
                self.visit_identifier_name(&mut c.name);
                self.object(&mut c.object);
            }
            QmlMember::Enum(e) => {
                e.span = self.0.span(e.span);
                self.visit_identifier_name(&mut e.name);
                for m in &mut e.members {
                    self.visit_identifier_name(&mut m.name);
                }
            }
        }
    }

    fn comment(&mut self, comment: &mut Comment) {
        comment.span = self.0.span(comment.span);
        comment.attached_to = self.0.start(comment.attached_to);
    }
}

impl<'a> VisitMut<'a> for Remap<'_> {
    fn visit_span(&mut self, it: &mut Span) {
        *it = self.0.span(*it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowered(text: &str) -> String {
        let out = desugar(text);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(
            out.text.lines().count(),
            text.lines().count(),
            "lines kept: {}",
            out.text
        );
        out.text
    }

    #[test]
    fn a_match_on_a_name_tests_the_name() {
        assert_eq!(
            lowered("const n = match (k) { when \"a\": 1; when \"b\" or \"c\": 2; default: 0; }\n"),
            "const n = (() => { if (k === \"a\") return 1; if ((k === \"b\" || k === \"c\")) return 2; return 0;})()\n"
        );
    }

    #[test]
    fn a_match_on_an_expression_reads_it_once() {
        assert_eq!(
            lowered("f(match (a.b) { when 1: \"one\"; })\n"),
            "f((($s = a.b) => { if ($s === 1) return \"one\"; return $nomatch($s as never);})())\n"
        );
    }

    #[test]
    fn a_binding_and_a_guard_nest() {
        assert_eq!(
            lowered(
                "x = match (p) { when { kind: \"c\", let r } and if (r > 0): r; default: 0; }\n"
            ),
            "x = (() => { { if (typeof p === \"object\" && p !== null && \"kind\" in p && p.kind === \"c\" && \"r\" in p) { const r = p.r; if ((r > 0)) { return r; } } } return 0;})()\n"
        );
    }

    #[test]
    fn lines_stay_where_they_are() {
        let text = "const n = match (k) {\n  when 1: 2;\n  default: 3;\n}\nconst m = n\n";
        let out = desugar(text);
        assert!(out.diagnostics.is_empty());
        let lines: Vec<&str> = out.text.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[4], "const m = n");
        // `return 2` is on the line of `2`.
        assert!(lines[1].contains("return 2"), "{lines:?}");
        let two = out.text.find("return 2").unwrap() as u32 + 7;
        assert_eq!(out.map.start(two), text.find("2;").unwrap() as u32);
    }

    #[test]
    fn an_is_with_a_binding_declares_it_before_the_statement() {
        assert_eq!(
            lowered("if (v is [let a, ...]) use(a)\n"),
            "var a; if ((Array.isArray(v) && v.length >= 1 && ((a = v[0]), true))) use(a)\n"
        );
    }

    #[test]
    fn a_match_in_a_match_is_written_away_too() {
        let out = lowered(
            "y = match (a) { when 1: match (b) { when 2: 3; default: 4; }; default: 5; }\n",
        );
        assert!(!out.contains("match"), "{out}");
        assert!(!out.contains("when"), "{out}");
    }

    #[test]
    fn match_as_a_name_is_still_a_name() {
        let text = "const match = 1; match(2); match\n(3)\n";
        assert_eq!(lowered(text), text);
    }

    #[test]
    fn what_the_proposal_leaves_out_is_an_error_at_it() {
        for (text, at) in [
            ("x = match (v) { when Some(let y): y; default: 0; }", "Some"),
            ("x = match (v) { when < 3: 1; default: 0; }", "<"),
            ("x = match (v) { when 1 and 2 or 3: 1; default: 0; }", "or"),
            ("x = match (v) { when /a/g: 1; default: 0; }", "/a/g"),
            ("x = match (v) { when { ...let r }: 1; default: 0; }", "..."),
            ("x = match (v) { when not let y: 1; default: 0; }", "let"),
        ] {
            let out = desugar(text);
            let first = out.diagnostics.first().unwrap_or_else(|| panic!("{text}"));
            assert_eq!(
                first.span.start as usize,
                text.find(at).unwrap(),
                "{text}: {first:?}"
            );
        }
    }

    #[test]
    fn a_document_parses_desugared_with_its_spans_as_written() {
        let allocator = Allocator::default();
        let source =
            "Item {\n    width: match (k) { when 1: 10; default: 20; }\n    height: 3\n}\n";
        let parsed = crate::parse(&allocator, source);
        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        let QmlMember::Binding(width) = &parsed.document.root.members[0] else {
            panic!()
        };
        let QmlBindingValue::Expression(e) = &width.value else {
            panic!()
        };
        let span = oxc_span::GetSpan::span(e);
        assert_eq!(
            crate::text(source, span),
            "match (k) { when 1: 10; default: 20; }"
        );
        let QmlMember::Binding(height) = &parsed.document.root.members[1] else {
            panic!()
        };
        assert_eq!(crate::text(source, height.span), "height: 3");
    }
}
