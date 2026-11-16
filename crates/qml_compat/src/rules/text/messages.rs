//! Qt's translated texts written as Ljós's (docs/DIALECT.md §4.13):
//! `qsTr("Open")` is `` t`Open` ``, `.arg` fills the message's values, a
//! count is `plural`, a disambiguation or `qsTranslate`'s context is the
//! message's context. `QT_TR_NOOP` and its family keep the file: their
//! value is a string to Qt, and `msg`'s is a message descriptor.
//!
//! A call is written out by replacing only what lies between its values:
//! each value stays where it is, so what the rest of the pass changes in
//! it is changed all the same, and no line is added or removed. A value
//! the message says in another place, or twice (Qt's `%n` is the count
//! beside `plural`'s, and `.arg`s fill `%2 of %1` out of order), is
//! written there once every change is known ([`fill`]). Only a value that
//! reads the same twice is: a name, a property of a QML object or value, or a literal.
//!
//! `t` and `plural` are `Ljos.Core`'s and come last in a lowered
//! file, so the pass writes them only where each is no other name of the
//! file ([`Members::free`](crate::qt_members::Members::free)); otherwise
//! the file is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::GetSpan;
use qml_edit::Change;

use crate::{Kept, cause::Message};

/// What `ljos migrate --catalogue` decided of a project's translations,
/// which the rewrite follows (docs/DIALECT.md §4.13 Lowering).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Translations {
    /// The texts of `qsTr` without a disambiguation that files translate
    /// differently: each such call takes the context Qt looks it up in, its
    /// file's name, so no translation is lost.
    pub named: BTreeSet<String>,
    /// The files whose translations would be lost written as Ljós's, each
    /// with where and why: they stay Qt's.
    pub kept: BTreeMap<String, (usize, String)>,
}

/// A call of one of Qt's functions the pass wrote as Ljós's: what a
/// catalogue of Qt's has it under, and what its rewrite is made from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translated {
    /// Where the call starts, as a byte offset into the file.
    pub at: usize,
    /// The call as written, with its `.arg`s: what [`rewrite_message`]
    /// writes again with a translation's text.
    pub call: String,
    /// `qsTr`, `qsTranslate` or `qsTrId`.
    pub function: &'static str,
    /// The context Qt looks the text up in: `qsTranslate`'s, or for `qsTr`
    /// the file's (its name, or what `pragma Translator` says). `None` for
    /// `qsTrId`, which is looked up by its id alone.
    pub qt_context: Option<String>,
    /// The text as written, or `qsTrId`'s id.
    pub source: String,
    /// `qsTr`'s disambiguation, `""` for none.
    pub disambiguation: String,
    /// The call has a count, so its translation has numerus forms.
    pub counted: bool,
    /// The context the rewrite gave a `qsTr` without a disambiguation, as
    /// [`Translations::named`] says: its file's.
    pub context: Option<String>,
}

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
    /// What the values written in another place are, and the changes they
    /// are written in.
    pub(crate) filled: Filled,
    /// The calls written, in the order they are read.
    pub(crate) translated: Vec<Translated>,
    /// The context Qt gives a `qsTr` of the file, where it is known.
    pub(crate) file_context: Option<String>,
    pub(crate) named: BTreeSet<String>,
}

/// The values a file's messages say in another place than where they
/// stand. A change's text has `\0{index}\0` where `copies[index]` goes; no
/// other text the pass writes has a `\0`.
#[derive(Debug, Default)]
pub(crate) struct Filled {
    copies: Vec<Range<usize>>,
    /// The changes that write the messages.
    written: Vec<Range<usize>>,
}

/// Writes each copied value with the changes made in it, and drops the
/// changes left inside the text a message replaced, where the value was
/// before it moved.
pub(crate) fn fill(source: &str, changes: &mut Vec<Change>, filled: &Filled) {
    if filled.copies.is_empty() {
        return;
    }
    let own = |c: &Change| filled.written.contains(&c.range);
    let texts: Vec<String> = filled
        .copies
        .iter()
        .map(|copy| {
            let inside: Vec<Change> = changes
                .iter()
                .filter(|c| !own(c) && copy.start <= c.range.start && c.range.end <= copy.end)
                .map(|c| Change {
                    range: c.range.start - copy.start..c.range.end - copy.start,
                    replacement: c.replacement.clone(),
                })
                .collect();
            qml_edit::splice(&source[copy.clone()], &inside)
        })
        .collect();
    let replaced = |c: &Change| {
        filled.written.iter().any(|w| {
            if c.range.is_empty() {
                w.start < c.range.start && c.range.start < w.end
            } else {
                w.start <= c.range.start && c.range.end <= w.end
            }
        })
    };
    changes.retain(|c| own(c) || !replaced(c));
    for change in changes.iter_mut().filter(|c| c.replacement.contains('\0')) {
        let mut out = String::with_capacity(change.replacement.len());
        let mut pieces = change.replacement.split('\0');
        out.push_str(pieces.next().unwrap_or_default());
        while let (Some(index), Some(after)) = (pieces.next(), pieces.next()) {
            let index: usize = index.parse().expect("a copy's marker is its index");
            out.push_str(&texts[index]);
            out.push_str(after);
        }
        change.replacement = out;
    }
}

impl Messages {
    /// The messages of a file whose `qsTr` Qt looks up in `file_context`,
    /// where `named` texts take it as theirs ([`Translations::named`]).
    pub(crate) fn new(file_context: Option<String>, named: BTreeSet<String>) -> Self {
        Self {
            file_context,
            named,
            ..Self::default()
        }
    }

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
    /// the call stays as written (a `ListElement`'s value). `whole` says
    /// whether a count is an `int`, `object` whether a value is a QML
    /// object or value of a known type, whose properties read the same
    /// each time.
    pub(crate) fn call(
        &mut self,
        source: &str,
        call: &CallExpression<'_>,
        rewrite: bool,
        global: impl Fn(&str, u32) -> bool,
        whole: impl Fn(&Expression<'_>) -> bool,
        object: impl Fn(&Expression<'_>) -> bool,
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
        let named = |text: &str| {
            self.file_context
                .clone()
                .filter(|_| self.named.contains(text))
        };
        let rewrite = Rewrite {
            forms: None,
            context: &named,
        };
        match chain.write(source, self.filled.copies.len(), &whole, &object, &rewrite) {
            Ok(written) => {
                let outer = range(chain.outer.span);
                self.translated.push(Translated {
                    at: at as usize,
                    call: source[outer].to_owned(),
                    qt_context: match written.message.function {
                        "qsTr" => self.file_context.clone(),
                        _ => written.message.qt_context.clone(),
                    },
                    ..written.message
                });
                for name in written.names {
                    if !self.names.iter().any(|(n, _)| *n == name) {
                        self.names.push((name, at as usize));
                    }
                }
                self.filled
                    .written
                    .extend(written.changes.iter().map(|c| c.range.clone()));
                self.filled.copies.extend(written.copies);
                self.changes.extend(written.changes);
            }
            Err(cause) => self.keep(cause, at),
        }
    }
}

/// What a call is written with besides what it says.
pub(crate) struct Rewrite<'r> {
    /// The forms to write in place of the call's text (a translation's),
    /// each with the CLDR category `plural` gives it; one, `other`, where
    /// the call has no count.
    pub(crate) forms: Option<&'r [(&'r str, &'r str)]>,
    /// The context of a `qsTr` with no disambiguation, by its text.
    pub(crate) context: &'r dyn Fn(&str) -> Option<String>,
}

/// `call`, a call of `qsTr`, `qsTranslate` or `qsTrId` with its `.arg`s as
/// written, written as Ljós's as the pass writes it: alone, with each value
/// as written. `forms` (a translation's, by CLDR category) stand for the
/// call's text where there are any, and `context` is the context of a
/// `qsTr` with no disambiguation. So a translation in a catalogue of Qt's is
/// written by the rewrite its call is, and says each value by the name the
/// call's message does.
pub fn rewrite_message(
    call: &str,
    forms: &[(&str, &str)],
    context: Option<&str>,
) -> Result<String, Message> {
    let allocator = oxc_allocator::Allocator::default();
    let not = || Message::NotWrittenOut {
        name: "qsTr".to_owned(),
    };
    let expression = oxc_parser::Parser::new(&allocator, call, oxc_span::SourceType::cjs())
        .parse_expression()
        .map_err(|_| not())?;
    let Expression::CallExpression(outer) = expression.without_parentheses() else {
        return Err(not());
    };
    let chain = Chain::of(outer).ok_or_else(not)?;
    let named = |_: &str| context.map(str::to_owned);
    let rewrite = Rewrite {
        forms: (!forms.is_empty()).then_some(forms),
        context: &named,
    };
    // The call is the whole source, from 0. Its migration already decided
    // which values are counts and which may be copied.
    let any = |_: &Expression<'_>| true;
    let written = chain.write(call, 0, &any, &any, &rewrite)?;
    let mut changes = written.changes;
    let filled = Filled {
        written: changes.iter().map(|c| c.range.clone()).collect(),
        copies: written.copies,
    };
    fill(call, &mut changes, &filled);
    let start = range(outer.span).start;
    let end = range(outer.span).end;
    let changes: Vec<Change> = changes
        .into_iter()
        .map(|c| Change {
            range: c.range.start - start..c.range.end - start,
            replacement: c.replacement,
        })
        .collect();
    Ok(qml_edit::splice(&call[start..end], &changes))
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

    /// The changes that write the call. Its copies are numbered from
    /// `copied`, the copies written before it.
    fn write(
        &self,
        source: &str,
        copied: usize,
        whole: &dyn Fn(&Expression<'_>) -> bool,
        object: &dyn Fn(&Expression<'_>) -> bool,
        rewrite: &Rewrite<'_>,
    ) -> Result<Written, Message> {
        let arguments = &self.call.arguments;
        if self.name.contains("NoOp") || self.name.contains("NOOP") {
            return Err(Message::Marked {
                name: self.name.to_owned(),
            });
        }
        if arguments.iter().any(Argument::is_spread) {
            return Err(self.not_written());
        }
        let values = self
            .args
            .iter()
            .map(|arg| match arg.arguments.as_slice() {
                [value] if !value.is_spread() => Ok(Value::of(value, object)),
                _ => Err(Message::Placeholders {
                    name: self.name.to_owned(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (form, max) = match self.name {
            "qsTr" => (Form::Tr, 3),
            "qsTranslate" => (Form::Translate, 4),
            _ => (Form::Id, 2),
        };
        if arguments.is_empty() || arguments.len() > max {
            return Err(self.not_written());
        }
        let count = |index: usize| -> Result<Option<Value>, Message> {
            let Some(count) = arguments.get(index) else {
                return Ok(None);
            };
            // Qt's count is whole (2.7 shows "2 file(s)"), `String(n)` not.
            match count.as_expression() {
                Some(n) if whole(n.without_parentheses()) => Ok(Some(Value::of(count, object))),
                _ => Err(Message::NotWhole {
                    name: self.name.to_owned(),
                }),
            }
        };
        let mut message = Built {
            tag: "t",
            copied,
            ..Built::default()
        };
        let mut said = Translated {
            at: 0,
            call: String::new(),
            function: self.name,
            qt_context: None,
            source: String::new(),
            disambiguation: String::new(),
            counted: false,
            context: None,
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
                said.source = id;
                Ok(message.done(self.outer, source, said))
            }
            Form::Tr => {
                let text = self.text(0)?.unwrap_or_default();
                let disambiguation = self.text(1)?.unwrap_or_default();
                let named = (rewrite.context)(&text).filter(|_| disambiguation.is_empty());
                let context = Some(disambiguation.clone())
                    .filter(|c| !c.is_empty())
                    .or_else(|| named.clone());
                let count = count(2)?;
                said.counted = count.is_some();
                said.disambiguation = disambiguation;
                said.context = named;
                said.source.clone_from(&text);
                self.message(
                    source, message, &text, rewrite, context, count, &values, said,
                )
            }
            Form::Translate => {
                let context = self.text(0)?.unwrap_or_default();
                let text = self.text(1)?.ok_or_else(|| self.not_written())?;
                if self.text(2)?.is_some_and(|d| !d.is_empty()) {
                    return Err(Message::TwoContexts {
                        name: self.name.to_owned(),
                    });
                }
                let count = count(3)?;
                said.counted = count.is_some();
                said.qt_context = Some(context.clone());
                said.source.clone_from(&text);
                let context = Some(context).filter(|c| !c.is_empty());
                self.message(
                    source, message, &text, rewrite, context, count, &values, said,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn message(
        &self,
        source: &str,
        mut message: Built,
        text: &str,
        rewrite: &Rewrite<'_>,
        context: Option<String>,
        count: Option<Value>,
        values: &[Value],
        said: Translated,
    ) -> Result<Written, Message> {
        let own = [("other", text)];
        let forms = rewrite.forms.unwrap_or(&own);
        // A text without a count has one form.
        if count.is_none() && forms.len() != 1 {
            return Err(Message::Placeholders {
                name: self.name.to_owned(),
            });
        }
        let mut written = Vec::with_capacity(forms.len());
        for (category, text) in forms {
            let parts = parts(text, count.is_some(), values.len()).ok_or_else(|| {
                Message::Placeholders {
                    name: self.name.to_owned(),
                }
            })?;
            if count.is_some()
                && parts
                    .iter()
                    .any(|p| matches!(p, Part::Text(t) if t.contains(['#', '{', '}'])))
            {
                return Err(Message::CountedBraces {
                    name: self.name.to_owned(),
                });
            }
            written.push((*category, parts));
        }
        let described = context.is_some();
        if described {
            message.text("({ message: ");
        }
        if let Some(count) = &count {
            message.uses.push("plural");
            message.text("plural(");
            message.value(count.at.clone());
            message.text(", { ");
        }
        for (index, (category, parts)) in written.iter().enumerate() {
            // `` t`…` `` alone is a template; a descriptor or a `plural`
            // holds a string, or a template where values fill it.
            let filled = parts
                .iter()
                .any(|p| matches!(p, Part::Value(_) | Part::Count));
            let templated = filled || !(described || count.is_some());
            if count.is_some() {
                if index > 0 {
                    message.text(", ");
                }
                message.text(&format!("{category}: "));
            }
            message.text(if templated { "`" } else { "\"" });
            for part in parts {
                match part {
                    Part::Text(t) if templated => message.text(&in_template(t)),
                    Part::Text(t) => message.text(&in_string(t)),
                    Part::Grouped => message.text("#"),
                    Part::Count | Part::Value(_) => {
                        let value = match part {
                            Part::Value(index) => &values[*index],
                            _ => count.as_ref().expect("a count part has a count"),
                        };
                        message.text("${");
                        message.value(value.at.clone());
                        message.text("}");
                    }
                }
            }
            message.text(if templated { "`" } else { "\"" });
        }
        // A copy is a value read in another order than it was, so it
        // reads the same only where it is plain and no value of the call
        // changes anything: `.arg(n).arg(n++)` read the other way round
        // gives another text.
        let plain = |at: &Range<usize>| count.iter().chain(values).any(|v| v.at == *at && v.plain);
        let all_plain = count.iter().chain(values).all(|v| v.plain);
        if !message.copies.iter().all(plain) || (!message.copies.is_empty() && !all_plain) {
            return Err(Message::Moved {
                name: self.name.to_owned(),
            });
        }
        if message
            .copies
            .iter()
            .any(|copy| source[copy.clone()].contains('\n'))
        {
            return Err(Message::MovedLines {
                name: self.name.to_owned(),
            });
        }
        if count.is_some() {
            message.text(" })");
        }
        if let Some(context) = context {
            message.text(&format!(", context: {} }})", quoted(&context)));
        }
        // A count alone is `plural(…)`, untagged.
        message.untagged = !described && count.is_some();
        Ok(message.done(self.outer, source, said))
    }
}

/// A value a message says: a count or what an `.arg` gives.
struct Value {
    at: Range<usize>,
    /// A name, a property of a QML object or value, or a literal: what reads the
    /// same twice.
    plain: bool,
}

impl Value {
    fn of(argument: &Argument<'_>, object: &dyn Fn(&Expression<'_>) -> bool) -> Self {
        Value {
            at: range(argument.span()),
            plain: argument
                .as_expression()
                .is_some_and(|e| plain(e.without_parentheses(), object)),
        }
    }
}

fn plain(expression: &Expression<'_>, object: &dyn Fn(&Expression<'_>) -> bool) -> bool {
    match expression {
        Expression::Identifier(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.is_empty(),
        // A JavaScript object's member may be a getter that gives another
        // value each read; a QML object's or value's property is read, not
        // run. A string's or array's `length` is a data property, as
        // `whole()` already counts on.
        Expression::StaticMemberExpression(member) => {
            let on = member.object.without_parentheses();
            plain(on, object) && (object(on) || member.property.name == "length")
        }
        _ => false,
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
    /// `%n`: the count as it is, `String(n)`.
    Count,
    /// `%Ln`: the count as the locale groups it, `plural`'s `#`.
    Grouped,
    /// A `%N` the `index`th `.arg` fills.
    Value(usize),
}

/// What Qt reads a `%` as.
enum Mark<'t> {
    Text(&'t str),
    /// `%n` or `%Ln`.
    Count {
        local: bool,
    },
    /// `%N` or `%LN`, `N` of one or two digits, and the text it is.
    Number {
        local: bool,
        number: u32,
        written: &'t str,
    },
}

fn marks(text: &str, counted: bool) -> Vec<Mark<'_>> {
    let mut marks = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        if at > 0 {
            marks.push(Mark::Text(&rest[..at]));
        }
        let after = &rest[at + 1..];
        let local = after.starts_with('L');
        let body = if local { &after[1..] } else { after };
        let digits = body.bytes().take_while(u8::is_ascii_digit).count().min(2);
        if counted && body.starts_with('n') {
            marks.push(Mark::Count { local });
            rest = &body[1..];
        } else if digits > 0 {
            let end = at + 1 + usize::from(local) + digits;
            marks.push(Mark::Number {
                local,
                number: body[..digits].parse().expect("digits"),
                written: &rest[at..end],
            });
            rest = &rest[end..];
        } else {
            marks.push(Mark::Text("%"));
            rest = after;
        }
    }
    if !rest.is_empty() {
        marks.push(Mark::Text(rest));
    }
    marks
}

/// The parts of `text` filled by `values` `.arg`s, as Qt fills them: each
/// `.arg` the lowest-numbered `%N` left, wherever it stands, and every
/// place that `%N` is. A `%N` no `.arg` fills is text. `None` where an
/// `.arg` has no `%N` left, or fills a `%LN`, a number in the locale's
/// digits, which a value of `t` is not yet.
fn parts(text: &str, counted: bool, values: usize) -> Option<Vec<Part>> {
    let marks = marks(text, counted);
    let mut numbers: Vec<u32> = marks
        .iter()
        .filter_map(|m| match m {
            Mark::Number { number, .. } => Some(*number),
            _ => None,
        })
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    // Qt's are `%1` to `%99`; what `%0` is to an `.arg` is not written down.
    if values > numbers.len() || (values > 0 && numbers.first() == Some(&0)) {
        return None;
    }
    let numbers = &numbers[..values];
    let mut parts = Vec::new();
    let mut plain = String::new();
    let part = |parts: &mut Vec<Part>, plain: &mut String, part: Part| {
        if !plain.is_empty() {
            parts.push(Part::Text(std::mem::take(plain)));
        }
        parts.push(part);
    };
    for mark in marks {
        match mark {
            Mark::Text(text) => plain.push_str(text),
            Mark::Count { local } => part(
                &mut parts,
                &mut plain,
                if local { Part::Grouped } else { Part::Count },
            ),
            Mark::Number {
                local,
                number,
                written,
            } => match numbers.iter().position(|n| *n == number) {
                Some(_) if local => return None,
                Some(index) => part(&mut parts, &mut plain, Part::Value(index)),
                None => plain.push_str(written),
            },
        }
    }
    if !plain.is_empty() {
        parts.push(Part::Text(plain));
    }
    Some(parts)
}

/// What a call is written as: text, and values that stay where they are.
#[derive(Default)]
struct Built {
    tag: &'static str,
    /// A `plural` alone has no tag.
    untagged: bool,
    pieces: Vec<Piece>,
    uses: Vec<&'static str>,
    /// The copies of the file's messages before this one.
    copied: usize,
    /// The values written in another place than where they stand.
    copies: Vec<Range<usize>>,
    /// Where the last value left where it stands ends.
    end: usize,
}

enum Piece {
    Text(String),
    Value(Range<usize>),
}

/// The changes that write a call, and the names they write.
struct Written {
    changes: Vec<Change>,
    names: Vec<&'static str>,
    copies: Vec<Range<usize>>,
    /// What the call says, for a catalogue.
    message: Translated,
}

impl Built {
    fn text(&mut self, text: &str) {
        match self.pieces.last_mut() {
            Some(Piece::Text(last)) => last.push_str(text),
            _ => self.pieces.push(Piece::Text(text.to_owned())),
        }
    }

    /// The value at `at`: left where it stands if it is after the last
    /// one left, and not left already; else written here once every
    /// change is known.
    fn value(&mut self, at: Range<usize>) {
        let left = self
            .pieces
            .iter()
            .any(|p| matches!(p, Piece::Value(v) if *v == at));
        if !left && at.start >= self.end {
            self.end = at.end;
            self.pieces.push(Piece::Value(at));
        } else {
            let marker = format!("\0{}\0", self.copied + self.copies.len());
            self.copies.push(at);
            self.text(&marker);
        }
    }

    /// The changes that make `call`'s text the pieces. Each run of text
    /// replaces what lies between two values; the lines it replaced go
    /// where a script may have them: after the text before a value (an
    /// open `${` or `(`), else before the text after one.
    fn done(mut self, call: &CallExpression<'_>, source: &str, message: Translated) -> Written {
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
        Written {
            changes,
            names,
            copies: self.copies,
            message,
        }
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
            // `\0` marks a copy ([`Filled`]).
            '\0' => out.push_str("\\x00"),
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
            '\0' => out.push_str("\\x00"),
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
    fn each_arg_fills_the_lowest_numbered_mark_left() {
        use Part::*;
        assert_eq!(
            parts("%1 of %2", false, 2),
            Some(vec![Value(0), Text(" of ".into()), Value(1)])
        );
        assert_eq!(
            parts("%2 of %1", false, 2),
            Some(vec![Value(1), Text(" of ".into()), Value(0)])
        );
        assert_eq!(
            parts("%1 and %1", false, 1),
            Some(vec![Value(0), Text(" and ".into()), Value(0)])
        );
        assert_eq!(
            parts("%3 then %7", false, 1),
            Some(vec![Value(0), Text(" then %7".into())])
        );
        assert_eq!(parts("%1", false, 2), None);
        assert_eq!(parts("%L1", false, 1), None);
        // Without `.arg`, `%1` is what the text says.
        assert_eq!(parts("50%1", false, 0), Some(vec![Text("50%1".into())]));
        assert_eq!(
            parts("%Ln of %n", true, 0),
            Some(vec![Grouped, Text(" of ".into()), Count])
        );
        assert_eq!(parts("%n", false, 0), Some(vec![Text("%n".into())]));
    }

    #[test]
    fn a_translation_is_written_by_the_rewrite_its_call_is() {
        let call = r#"qsTr("%1 of %2").arg(name).arg(total)"#;
        assert_eq!(
            rewrite_message(call, &[], None).unwrap(),
            "t`${name} of ${total}`"
        );
        // Each mark is the value its number names in the call.
        assert_eq!(
            rewrite_message(call, &[("other", "%2 alls, þar af %1")], None).unwrap(),
            "t`${total} alls, þar af ${name}`"
        );
        let counted = r#"qsTr("%n page(s)", "", count)"#;
        assert_eq!(
            rewrite_message(counted, &[("one", "%n síða"), ("other", "%n síður")], None).unwrap(),
            "plural(count, { one: `${count} síða`, other: `${count} síður` })"
        );
        assert_eq!(
            rewrite_message(r#"qsTr("Open")"#, &[("other", "Opna")], Some("Door")).unwrap(),
            r#"t({ message: "Opna", context: "Door" })"#
        );
        assert!(rewrite_message("open()", &[], None).is_err());
    }

    #[test]
    fn a_template_escapes_only_its_own() {
        assert_eq!(
            in_template("a`b${c}$d\\e\"f'g\nh"),
            "a\\`b\\${c}$d\\\\e\"f'g\\nh"
        );
        assert_eq!(in_string("say \"hi\"\n"), "say \\\"hi\\\"\\n");
    }
}
