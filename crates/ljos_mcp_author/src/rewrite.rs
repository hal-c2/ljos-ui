//! Cheaper forms of an expression that mean the same: what
//! `binding_tier_explain` offers for a binding left to JavaScript.
//!
//! A form is proposed here from the shape of the expression alone. Whether
//! it really runs in a lower tier is not decided here: the caller compiles
//! the file with the form in place and asks the compiler, and offers it
//! only if the answer is yes. So a rule may be narrow, but it must never
//! change what the expression gives.

use qml_syntax::{
    Allocator, GetSpan,
    ast::{QmlBindingValue, QmlMember, QmlObject},
    js::{ArrowFunctionBody, Expression, FunctionBody, Statement},
};

/// A form to try, and what to tell the author about it.
pub(crate) struct Proposal {
    pub source: String,
    pub note: &'static str,
}

/// The form to try for the binding of `source` whose value starts at byte
/// `start`, if a rule knows one.
pub(crate) fn propose(source: &str, start: u32) -> Option<Proposal> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return None;
    }
    let mut found = None;
    visit(&parsed.document.root, &mut |value| {
        if found.is_none() && span_start(value) == Some(start) {
            found = of_value(source, value);
        }
    });
    found
}

fn span_start(value: &QmlBindingValue<'_>) -> Option<u32> {
    match value {
        QmlBindingValue::Expression(e) => Some(e.span().start),
        QmlBindingValue::Statement(s) => Some(s.span().start),
        QmlBindingValue::Object(_) | QmlBindingValue::Objects(_) => None,
    }
}

/// Every binding value of the object and of the objects written in it.
fn visit<'a>(object: &QmlObject<'a>, f: &mut impl FnMut(&QmlBindingValue<'a>)) {
    for member in &object.members {
        match member {
            QmlMember::Object(inner) => visit(inner, f),
            QmlMember::Binding(binding) => value(&binding.value, f),
            QmlMember::Property(declared) => {
                if let Some(given) = &declared.value {
                    value(given, f);
                }
            }
            QmlMember::InlineComponent(component) => visit(&component.object, f),
            QmlMember::Signal(_) | QmlMember::Function(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn value<'a>(given: &QmlBindingValue<'a>, f: &mut impl FnMut(&QmlBindingValue<'a>)) {
    f(given);
    match given {
        QmlBindingValue::Object(object) => visit(object, f),
        QmlBindingValue::Objects(objects) => {
            for object in objects {
                visit(object, f);
            }
        }
        QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
    }
}

fn of_value(source: &str, value: &QmlBindingValue<'_>) -> Option<Proposal> {
    match value {
        QmlBindingValue::Statement(Statement::BlockStatement(block)) => switch_of_returns(
            source,
            &block.body,
        )
        .map(|source| Proposal {
            source,
            note: "A `switch` whose every case only returns a value is a chain of conditionals, \
                       which the binding VM runs; `switch` compares with `===`, and so does this.",
        }),
        QmlBindingValue::Expression(expression) => {
            called_where_written(source, expression).map(|source| Proposal {
                source,
                note: "A function that is called where it is written, takes nothing and only \
                       returns a value is that value; without the function the binding VM runs it.",
            })
        }
        _ => None,
    }
}

fn text<'s>(source: &'s str, node: &impl GetSpan) -> &'s str {
    let span = node.span();
    &source[span.start as usize..span.end as usize]
}

fn unwrapped<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    while let Expression::ParenthesizedExpression(inner) = expression {
        expression = &inner.expression;
    }
    expression
}

/// A name or a path of names: reading it twice gives the same value and
/// does nothing else, so it may be written once per case.
fn is_path(expression: &Expression<'_>) -> bool {
    match unwrapped(expression) {
        Expression::Identifier(_) => true,
        Expression::StaticMemberExpression(member) => !member.optional && is_path(&member.object),
        _ => false,
    }
}

/// The text of `expression` as an operand of `===` or an arm of `?:`: in
/// parentheses unless it is plainly tighter than both.
fn operand(source: &str, expression: &Expression<'_>) -> String {
    let written = text(source, expression);
    match expression {
        Expression::Identifier(_)
        | Expression::StaticMemberExpression(_)
        | Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::ParenthesizedExpression(_)
        | Expression::CallExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::TemplateLiteral(_) => written.to_owned(),
        _ => format!("({written})"),
    }
}

/// The text of `expression` as the value of a `?:`, where anything tighter
/// than a conditional needs no parentheses.
fn arm(source: &str, expression: &Expression<'_>) -> String {
    let written = text(source, expression);
    match expression {
        Expression::ConditionalExpression(_)
        | Expression::AssignmentExpression(_)
        | Expression::SequenceExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::YieldExpression(_) => format!("({written})"),
        _ => written.to_owned(),
    }
}

/// The value a case gives, when all it does is return one.
fn returned<'s, 'a>(statements: &'s [Statement<'a>]) -> Option<&'s Expression<'a>> {
    match statements {
        [Statement::ReturnStatement(ret)] => ret.argument.as_ref(),
        [Statement::BlockStatement(block)] => returned(&block.body),
        _ => None,
    }
}

/// `switch (d) { case a: return x; case b: return y; default: return z }`
/// as `d === a ? x : d === b ? y : z`. The default may also be a `return`
/// after the switch; cases that share a value (`case a: case b: return x`)
/// share a test.
fn switch_of_returns(source: &str, body: &[Statement<'_>]) -> Option<String> {
    let (switch, after) = match body {
        [Statement::SwitchStatement(switch)] => (switch, None),
        [Statement::SwitchStatement(switch), rest @ ..] => (switch, Some(returned(rest)?)),
        _ => return None,
    };
    if !is_path(&switch.discriminant) {
        return None;
    }
    let subject = text(source, unwrapped(&switch.discriminant));
    let mut out = String::new();
    let mut otherwise = None;
    let mut tests: Vec<String> = Vec::new();
    let last = switch.cases.len().checked_sub(1)?;
    for (index, case) in switch.cases.iter().enumerate() {
        let Some(test) = &case.test else {
            // A default anywhere but last is reached by falling through,
            // which a conditional cannot say.
            if index != last {
                return None;
            }
            otherwise = Some(returned(&case.consequent)?);
            tests.clear();
            break;
        };
        tests.push(format!("{subject} === {}", operand(source, test)));
        if case.consequent.is_empty() {
            continue;
        }
        let gives = returned(&case.consequent)?;
        out.push_str(&tests.join(" || "));
        out.push_str(" ? ");
        out.push_str(&arm(source, gives));
        out.push_str(" : ");
        tests.clear();
    }
    // Cases left with nothing to do fall out of the switch to what follows.
    let otherwise = otherwise.or(after)?;
    if out.is_empty() {
        return None;
    }
    out.push_str(&arm(source, otherwise));
    Some(out)
}

fn single_return<'s, 'a>(body: &'s FunctionBody<'a>) -> Option<&'s Expression<'a>> {
    if !body.directives.is_empty() {
        return None;
    }
    returned(&body.statements)
}

/// `(function() { return e })()` and `(() => e)()` as `e`.
fn called_where_written(source: &str, expression: &Expression<'_>) -> Option<String> {
    let Expression::CallExpression(call) = unwrapped(expression) else {
        return None;
    };
    if call.optional || !call.arguments.is_empty() {
        return None;
    }
    let inner = match unwrapped(&call.callee) {
        Expression::FunctionExpression(function) => {
            if function.r#async
                || function.generator
                || !function.params.items.is_empty()
                || function.params.rest.is_some()
            {
                return None;
            }
            let inner = single_return(function.body.as_ref()?)?;
            // A function has a `this` and `arguments` of its own: outside
            // it the words would mean something else.
            let written = text(source, inner);
            if has_word(written, "this") || has_word(written, "arguments") {
                return None;
            }
            inner
        }
        Expression::ArrowFunctionExpression(arrow) => {
            if arrow.r#async || !arrow.params.items.is_empty() || arrow.params.rest.is_some() {
                return None;
            }
            match &arrow.body {
                ArrowFunctionBody::FunctionBody(body) => single_return(body)?,
                concise => concise.as_expression()?,
            }
        }
        _ => return None,
    };
    Some(text(source, unwrapped(inner)).to_owned())
}

/// Whether `word` stands in `text` as a word of its own.
fn has_word(text: &str, word: &str) -> bool {
    let is_part = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(is_part) && !after.is_some_and(is_part)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The proposal for the one binding `value` of an item.
    fn proposed(value: &str) -> Option<String> {
        let source = format!("import QtQuick\nItem {{\n    opacity: {value}\n}}\n");
        let start = source.find("opacity: ").unwrap() + "opacity: ".len();
        propose(&source, start as u32).map(|p| p.source)
    }

    #[test]
    fn a_switch_of_returns_is_a_chain_of_conditionals() {
        assert_eq!(
            proposed("{ switch (kind) { case \"a\": return 0.5; case \"b\": return 0.8; default: return 1 } }")
                .as_deref(),
            Some("kind === \"a\" ? 0.5 : kind === \"b\" ? 0.8 : 1")
        );
    }

    #[test]
    fn cases_that_share_a_value_share_a_test() {
        assert_eq!(
            proposed(
                "{ switch (root.kind) { case 1: case 2: return a + b; default: return c ? 1 : 2 } }"
            )
            .as_deref(),
            Some("root.kind === 1 || root.kind === 2 ? a + b : (c ? 1 : 2)")
        );
    }

    #[test]
    fn the_default_may_follow_the_switch() {
        assert_eq!(
            proposed("{ switch (kind) { case x + 1: return 0.5 } return 1 }").as_deref(),
            Some("kind === (x + 1) ? 0.5 : 1")
        );
    }

    #[test]
    fn a_switch_that_does_more_than_choose_is_left_alone() {
        // No value when nothing matches.
        assert_eq!(proposed("{ switch (kind) { case 1: return 0.5 } }"), None);
        // A case that does something first.
        assert_eq!(
            proposed("{ switch (kind) { case 1: count++; return 0.5; default: return 1 } }"),
            None
        );
        // A subject that is computed would be computed once per case.
        assert_eq!(
            proposed("{ switch (next()) { case 1: return 0.5; default: return 1 } }"),
            None
        );
        // A default that is fallen into.
        assert_eq!(
            proposed("{ switch (kind) { default: return 1; case 1: return 0.5 } }"),
            None
        );
    }

    #[test]
    fn a_function_called_where_it_is_written_is_what_it_returns() {
        assert_eq!(
            proposed("(function() { return count / 2 })()").as_deref(),
            Some("count / 2")
        );
        assert_eq!(
            proposed("(() => count / 2)()").as_deref(),
            Some("count / 2")
        );
        assert_eq!(
            proposed("(() => { return (count / 2) })()").as_deref(),
            Some("count / 2")
        );
    }

    #[test]
    fn a_function_that_is_more_than_its_value_is_left_alone() {
        assert_eq!(proposed("(function(n) { return n / 2 })(count)"), None);
        assert_eq!(proposed("(function() { return this.count })()"), None);
        assert_eq!(
            proposed("(function() { let n = count; return n / 2 })()"),
            None
        );
        assert_eq!(proposed("count / 2"), None);
    }

    #[test]
    fn a_word_is_not_part_of_another() {
        assert!(has_word("this.count", "this"));
        assert!(!has_word("thistle.count", "this"));
        assert!(!has_word("a.arguments_", "arguments"));
    }
}
