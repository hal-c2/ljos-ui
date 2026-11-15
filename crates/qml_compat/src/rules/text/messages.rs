//! Qt's translated texts written as Ljós's (docs/DIALECT.md §4.13):
//! `qsTr("Open")` is `` t`Open` ``, `.arg` fills the message's values, a
//! count is `plural`, a disambiguation or `qsTranslate`'s context is the
//! message's context, and `QT_TR_NOOP` marks a message with `msg`.
//!
//! A call is written out by replacing only what lies between its values:
//! each value stays where it is, so what the rest of the pass changes in
//! it is changed all the same, and no line is added or removed.
//!
//! `t`, `plural` and `msg` are `Ljos.Core`'s and come last in a lowered
//! file, so the pass writes them only where each is no other name of the
//! file ([`Members::free`](crate::qt_members::Members::free)); otherwise
//! the file is kept.

use std::ops::Range;

use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::GetSpan;
use qml_edit::Change;

use crate::{Kept, cause::Message};

/// Qt's translation functions of QML's global object.
const NAMES: &[&str] = &[
    "qsTr",
    "qsTranslate",
    "qsTrId",
    "qsTrNoOp",
    "qsTranslateNoOp",
    "qsTrIdNoOp",
    "QT_TR_NOOP",
    "QT_TRANSLATE_NOOP",
    "QT_TRID_NOOP",
];

/// Whether `name` is one of Qt's translation functions.
pub(crate) fn is_qt(name: &str) -> bool {
    NAMES.contains(&name)
}

/// The translated texts of a file, as the pass reads its scripts.
#[derive(Debug, Default)]
pub(crate) struct Messages {
    pub(crate) changes: Vec<Change>,
    /// The `Ljos.Core` names the changes write, and the first place each
    /// is written.
    pub(crate) names: Vec<(&'static str, usize)>,
    /// Why a call cannot be written out, the first one.
    pub(crate) kept: Option<Kept>,
    /// Where the calls written out, or being written, start.
    handled: Vec<u32>,
    /// Where the names of Qt's functions are called.
    callees: Vec<u32>,
}

impl Messages {
    fn keep(&mut self, cause: Message, at: u32) {
        self.kept.get_or_insert(Kept {
            cause: cause.into(),
            at: at as usize,
        });
    }

    /// `name`, one of Qt's functions, read at `at`: anywhere but as what a
    /// call calls, it is a value the pass cannot follow.
    pub(crate) fn read(&mut self, name: &str, at: u32) {
        if !self.callees.contains(&at) {
            self.keep(
                Message::NotCalled {
                    name: name.to_owned(),
                },
                at,
            );
        }
    }

    /// `call`, where it calls one of Qt's functions (a `.arg` of one too)
    /// whose name `global` says is the global's. `rewrite` is false where
    /// the call stays as written (a `ListElement`'s value).
    pub(crate) fn call(
        &mut self,
        source: &str,
        call: &CallExpression<'_>,
        rewrite: bool,
        global: impl Fn(&str, u32) -> bool,
    ) {
        let Some(chain) = Chain::of(call).filter(|c| global(c.name, c.callee_at)) else {
            return;
        };
        let at = chain.call.span.start;
        self.callees.push(chain.callee_at);
        if !rewrite || self.handled.contains(&at) {
            return;
        }
        self.handled.push(at);
        match chain.write(source) {
            Ok(written) => {
                for name in written.names {
                    if !self.names.iter().any(|(n, _)| *n == name) {
                        self.names.push((name, at as usize));
                    }
                }
                self.changes.extend(written.changes);
            }
            Err(cause) => self.keep(cause, at),
        }
    }
}

/// A call of one of Qt's functions and the `.arg`s of what it gives.
struct Chain<'c, 'a> {
    /// The outermost call: the last `.arg`, or the call itself.
    outer: &'c CallExpression<'a>,
    call: &'c CallExpression<'a>,
    name: &'static str,
    callee_at: u32,
    /// The `.arg`s, in the order they are called.
    args: Vec<&'c CallExpression<'a>>,
}

impl<'c, 'a> Chain<'c, 'a> {
    fn of(outer: &'c CallExpression<'a>) -> Option<Self> {
        let mut args = Vec::new();
        let mut call = outer;
        loop {
            match call.callee.without_parentheses() {
                Expression::Identifier(id) => {
                    let name = NAMES.iter().find(|n| **n == id.name.as_str())?;
                    args.reverse();
                    return Some(Chain {
                        outer,
                        call,
                        name,
                        callee_at: id.span.start,
                        args,
                    });
                }
                Expression::StaticMemberExpression(member) if member.property.name == "arg" => {
                    let Expression::CallExpression(inner) = member.object.without_parentheses()
                    else {
                        return None;
                    };
                    args.push(call);
                    call = inner;
                }
                _ => return None,
            }
        }
    }

    fn not_written(&self) -> Message {
        Message::NotWrittenOut {
            name: self.name.to_owned(),
        }
    }

    /// The `index`th argument of the call, as written text.
    fn text(&self, index: usize) -> Result<Option<String>, Message> {
        match self.call.arguments.get(index) {
            None => Ok(None),
            Some(argument) => literal(argument)
                .map(Some)
                .ok_or_else(|| self.not_written()),
        }
    }

    fn write(&self, source: &str) -> Result<Written, Message> {
        let arguments = &self.call.arguments;
        if arguments.iter().any(Argument::is_spread) {
            return Err(self.not_written());
        }
        let values = self
            .args
            .iter()
            .map(|arg| match arg.arguments.as_slice() {
                [value] if !value.is_spread() => Ok(range(value.span())),
                _ => Err(Message::Placeholders {
                    name: self.name.to_owned(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (form, max) = match self.name {
            "qsTr" | "qsTrNoOp" | "QT_TR_NOOP" => (Form::Tr, 3),
            "qsTranslate" | "qsTranslateNoOp" | "QT_TRANSLATE_NOOP" => (Form::Translate, 4),
            _ => (Form::Id, 2),
        };
        if arguments.is_empty() || arguments.len() > max {
            return Err(self.not_written());
        }
        let marked = self.name.contains("NoOp") || self.name.contains("NOOP");
        let mut message = Built {
            tag: if marked { "msg" } else { "t" },
            ..Built::default()
        };
        match form {
            Form::Id => {
                let id = self.text(0)?.unwrap_or_default();
                if arguments.len() > 1 || !values.is_empty() {
                    return Err(Message::IdWithValues {
                        name: self.name.to_owned(),
                    });
                }
                message.text(&format!("({{ id: {} }})", quoted(&id)));
                return Ok(message.done(self.outer, source));
            }
            Form::Tr => {
                let text = self.text(0)?.unwrap_or_default();
                let context = self.text(1)?.filter(|c| !c.is_empty());
                let count = arguments.get(2).map(|n| range(n.span()));
                self.message(source, message, &text, context, count, &values, marked)
            }
            Form::Translate => {
                let context = self.text(0)?.unwrap_or_default();
                let text = self.text(1)?.ok_or_else(|| self.not_written())?;
                if self.text(2)?.is_some_and(|d| !d.is_empty()) {
                    return Err(Message::TwoContexts {
                        name: self.name.to_owned(),
                    });
                }
                let count = arguments.get(3).map(|n| range(n.span()));
                let context = Some(context).filter(|c| !c.is_empty());
                self.message(source, message, &text, context, count, &values, marked)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn message(
        &self,
        source: &str,
        mut message: Built,
        text: &str,
        context: Option<String>,
        count: Option<Range<usize>>,
        values: &[Range<usize>],
        marked: bool,
    ) -> Result<Written, Message> {
        let parts = parts(text, count.is_some(), values.len()).ok_or_else(|| {
            Message::Placeholders {
                name: self.name.to_owned(),
            }
        })?;
        if marked && (count.is_some() || !values.is_empty()) {
            return Err(Message::MarkedFilled {
                name: self.name.to_owned(),
            });
        }
        if count.is_some()
            && parts
                .iter()
                .any(|p| matches!(p, Part::Text(t) if t.contains(['#', '{', '}'])))
        {
            return Err(Message::CountedBraces {
                name: self.name.to_owned(),
            });
        }
        let described = context.is_some();
        // `` t`…` `` alone is a template; a descriptor or a `plural` holds a
        // string, or a template where values fill it.
        let templated = !values.is_empty() || !(described || count.is_some());
        if described {
            message.text("({ message: ");
        }
        if let Some(count) = &count {
            message.uses.push("plural");
            message.text("plural(");
            message.value(count.clone());
            message.text(", { other: ");
        }
        message.text(if templated { "`" } else { "\"" });
        let mut values = values.iter();
        for part in &parts {
            match part {
                Part::Text(t) if templated => message.text(&in_template(t)),
                Part::Text(t) => message.text(&in_string(t)),
                Part::Count => message.text("#"),
                Part::Value => {
                    message.text("${");
                    message.value(values.next().expect("parts counts the values").clone());
                    message.text("}");
                }
            }
        }
        message.text(if templated { "`" } else { "\"" });
        if count.is_some() {
            message.text(" })");
        }
        if let Some(context) = context {
            message.text(&format!(", context: {} }})", quoted(&context)));
        }
        // A count alone is `plural(…)`, untagged.
        message.untagged = !described && count.is_some();
        Ok(message.done(self.outer, source))
    }
}

enum Form {
    Tr,
    Translate,
    Id,
}

/// One part of a message's text, read as Qt reads it.
#[derive(Debug, PartialEq)]
enum Part {
    Text(String),
    /// `%n` or `%Ln`: the count.
    Count,
    /// `%N`: the next `.arg`'s value.
    Value,
}

/// The parts of `text`, or `None` where its `%N` are not each filled by
/// one of `values` `.arg`s, in order. Without `.arg`s, `%N` is text.
fn parts(text: &str, counted: bool, values: usize) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    let mut plain = String::new();
    let mut next = 1;
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        plain.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let (local, after) = match after.strip_prefix('L') {
            Some(after) => (true, after),
            None => (false, after),
        };
        let digits = after.bytes().take_while(u8::is_ascii_digit).count().min(2);
        let mut taken = 0;
        if counted && after.starts_with('n') {
            parts.push(Part::Text(std::mem::take(&mut plain)));
            parts.push(Part::Count);
            taken = 1;
        } else if digits > 0 && values > 0 {
            // `%L1` is a number in the locale's digits, which a value of
            // `t` is not yet.
            if local || after[..digits].parse::<usize>().ok()? != next {
                return None;
            }
            next += 1;
            parts.push(Part::Text(std::mem::take(&mut plain)));
            parts.push(Part::Value);
            taken = digits;
        } else {
            plain.push('%');
            if local {
                plain.push('L');
            }
        }
        rest = &after[taken..];
    }
    plain.push_str(rest);
    parts.push(Part::Text(plain));
    parts.retain(|p| !matches!(p, Part::Text(t) if t.is_empty()));
    (next - 1 == values).then_some(parts)
}

/// What a call is written as: text, and values that stay where they are.
#[derive(Default)]
struct Built {
    tag: &'static str,
    /// A `plural` alone has no tag.
    untagged: bool,
    pieces: Vec<Piece>,
    uses: Vec<&'static str>,
}

enum Piece {
    Text(String),
    Value(Range<usize>),
}

/// The changes that write a call, and the names they write.
struct Written {
    changes: Vec<Change>,
    names: Vec<&'static str>,
}

impl Built {
    fn text(&mut self, text: &str) {
        match self.pieces.last_mut() {
            Some(Piece::Text(last)) => last.push_str(text),
            _ => self.pieces.push(Piece::Text(text.to_owned())),
        }
    }

    fn value(&mut self, at: Range<usize>) {
        self.pieces.push(Piece::Value(at));
    }

    /// The changes that make `call`'s text the pieces. Each run of text
    /// replaces what lies between two values; the lines it replaced go
    /// where a script may have them: after the text before a value (an
    /// open `${` or `(`), else before the text after one.
    fn done(mut self, call: &CallExpression<'_>, source: &str) -> Written {
        let mut names = self.uses.clone();
        if !self.untagged {
            names.insert(0, self.tag);
            match self.pieces.first_mut() {
                Some(Piece::Text(first)) => first.insert_str(0, self.tag),
                _ => self.pieces.insert(0, Piece::Text(self.tag.to_owned())),
            }
        }
        let span = range(call.span);
        let mut changes = Vec::new();
        let mut from = span.start;
        let mut text: Option<String> = None;
        let lines = |r: Range<usize>| "\n".repeat(source[r].matches('\n').count());
        let any_value = self.pieces.iter().any(|p| matches!(p, Piece::Value(_)));
        for piece in self.pieces {
            match piece {
                Piece::Text(t) => text = Some(t),
                Piece::Value(at) => {
                    let between = from..at.start;
                    let mut replacement = text.take().unwrap_or_default();
                    replacement.push_str(&lines(between.clone()));
                    changes.push(Change {
                        range: between,
                        replacement,
                    });
                    from = at.end;
                }
            }
        }
        let last = from..span.end;
        let text = text.unwrap_or_default();
        let lines = lines(last.clone());
        let replacement = if lines.is_empty() {
            text
        } else if any_value {
            format!("{lines}{text}")
        } else {
            format!("({text}{lines})")
        };
        changes.push(Change {
            range: last,
            replacement,
        });
        changes.retain(|c| !(c.range.is_empty() && c.replacement.is_empty()));
        Written { changes, names }
    }
}

fn range(span: oxc_span::Span) -> Range<usize> {
    span.start as usize..span.end as usize
}

/// The text of `argument` where it is written out: a string, or a
/// template without values.
fn literal(argument: &Argument<'_>) -> Option<String> {
    match argument.as_expression()?.without_parentheses() {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => t
            .quasis
            .first()
            .and_then(|q| q.value.cooked.as_ref())
            .map(|c| c.to_string()),
        _ => None,
    }
}

/// `text` in a template literal: only what a template reads as its own is
/// escaped. A line break is written `\n`, so no line is added.
fn in_template(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// `text` in a double-quoted string, without its quotes.
fn in_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", in_string(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_in_order_are_values() {
        assert_eq!(
            parts("%1 of %2", false, 2),
            Some(vec![
                Part::Value,
                Part::Text(" of ".into()),
                Part::Value
            ])
        );
        assert_eq!(parts("%2 of %1", false, 2), None);
        assert_eq!(parts("%1 and %1", false, 2), None);
        assert_eq!(parts("%1", false, 2), None);
        assert_eq!(parts("%L1", false, 1), None);
        // Without `.arg`, `%1` is what the text says.
        assert_eq!(parts("50%1", false, 0), Some(vec![Part::Text("50%1".into())]));
        assert_eq!(
            parts("%Ln file(s)", true, 0),
            Some(vec![Part::Count, Part::Text(" file(s)".into())])
        );
        assert_eq!(parts("%n", false, 0), Some(vec![Part::Text("%n".into())]));
    }

    #[test]
    fn a_template_escapes_only_its_own() {
        assert_eq!(in_template("a`b${c}$d\\e\"f'g\nh"), "a\\`b\\${c}$d\\\\e\"f'g\\nh");
        assert_eq!(in_string("say \"hi\"\n"), "say \\\"hi\\\"\\n");
    }
}
