//! The string and template literals of a document that run over lines.

use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;

use crate::{
    Parsed, Span,
    ast::{QmlBindingValue, QmlMember, QmlObject},
    js::{StringLiteral, TemplateLiteral},
};

/// The spans of the string and template literals in `parsed` that run over
/// more than one line of `source`, in source order and not overlapping. Their
/// lines after the first are the literal's text, not code: a tool that
/// indents or trims lines leaves those as they are. A template is one span
/// with what is nested in its `${…}` inside it.
pub fn multiline_literals(source: &str, parsed: &Parsed<'_>) -> Vec<Span> {
    let mut found = Literals {
        source,
        spans: Vec::new(),
    };
    found.object(&parsed.document.root);
    found.spans.sort_by_key(|s| s.start);
    found.spans
}

struct Literals<'s> {
    source: &'s str,
    spans: Vec<Span>,
}

impl Literals<'_> {
    fn keep(&mut self, span: Span) {
        let text = self.source.get(span.start as usize..span.end as usize);
        if text.is_some_and(|t| t.contains('\n')) {
            self.spans.push(span);
        }
    }

    fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Object(o) => self.object(o),
                QmlMember::Binding(b) => self.value(&b.value),
                QmlMember::Property(p) => {
                    if let Some(v) = &p.value {
                        self.value(v);
                    }
                }
                QmlMember::Function(f) => self.visit_function(f, ScopeFlags::Function),
                QmlMember::InlineComponent(c) => self.object(&c.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value(&mut self, value: &QmlBindingValue<'_>) {
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
}

impl<'a> Visit<'a> for Literals<'_> {
    // Not walked into: what is nested in a template is inside its span.
    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.keep(it.span);
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.keep(it.span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Allocator, parse};

    fn texts(source: &str) -> Vec<&str> {
        let allocator = Allocator::default();
        let parsed = parse(&allocator, source);
        multiline_literals(source, &parsed)
            .into_iter()
            .map(|s| crate::text(source, s))
            .collect()
    }

    #[test]
    fn only_literals_over_lines_are_found_and_a_template_takes_its_nested_ones() {
        let source = "Item {\n    a: \"one\"\n    b: `x ${`y\nz`} w`\n    function f() { return \"p\n  q\" }\n}\n";
        assert_eq!(texts(source), ["`x ${`y\nz`} w`", "\"p\n  q\""]);
    }
}
