//! Ljós's own translated messages (docs/DIALECT.md §4.13): what a call of
//! `t`, `msg`, `plural`, `select` or `selectOrdinal` from `Ljos.Core`, or
//! a template tagged with `t` or `msg`, says in ICU MessageFormat.
//!
//! [`message_of`] is the one reading of the macros: the compiler lowers
//! what it gives, and the extractor writes the same message to a
//! catalogue, so a translation is always looked up by the text the
//! program asks for.
//!
//! The reading is of the syntax alone. Whether `t` is `Ljos.Core`'s, and
//! not a local of the same name, is the caller's to know; the `_in` forms
//! ask the caller the same of each choice written inside a message, so a
//! local `select` stays a value the message shows.

use oxc_ast::ast::{
    Argument, CallExpression, Expression, IdentifierReference, ObjectExpression,
    ObjectPropertyKind, PropertyKey, TaggedTemplateExpression, TemplateLiteral,
};
use oxc_span::{GetSpan, Span};

/// The names `Ljos.Core` gives its macros.
pub const MACROS: &[&str] = &["t", "msg", "plural", "select", "selectOrdinal"];

/// Which macro a call is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroKind {
    /// `` t`…` ``, `t({ … })`: the text in the UI language.
    T,
    /// `` msg`…` ``, `msg({ … })`: the message itself, translated where
    /// `t` is given it.
    Msg,
    /// `plural(n, { one: …, other: … })`.
    Plural,
    /// `select(v, { a: …, other: … })`.
    Select,
    /// `selectOrdinal(n, { one: …, other: … })`.
    SelectOrdinal,
}

impl MacroKind {
    /// The macro `name` is, whatever it is bound to.
    pub fn named(name: &str) -> Option<MacroKind> {
        Some(match name {
            "t" => MacroKind::T,
            "msg" => MacroKind::Msg,
            "plural" => MacroKind::Plural,
            "select" => MacroKind::Select,
            "selectOrdinal" => MacroKind::SelectOrdinal,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MacroKind::T => "t",
            MacroKind::Msg => "msg",
            MacroKind::Plural => "plural",
            MacroKind::Select => "select",
            MacroKind::SelectOrdinal => "selectOrdinal",
        }
    }
}

/// A value a message shows, by the name the message gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// The name of a value read from a variable is the variable's; any
    /// other value is numbered, `0` first.
    pub name: String,
    /// Where the value's expression is, the first time it is shown.
    pub span: Span,
}

/// What one macro says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageCall {
    pub kind: MacroKind,
    /// The message in ICU MessageFormat: `Hello {name}`. `None` for a
    /// lookup by id alone, and for `t(m)` of a message made elsewhere.
    pub message: Option<String>,
    /// The id it is looked up by, where the program names one.
    pub id: Option<String>,
    pub context: Option<String>,
    /// The note for the translator.
    pub comment: Option<String>,
    /// In the order they are first written, each name once.
    pub placeholders: Vec<Placeholder>,
    /// `t(m)`: the expression whose value is a message `msg` made. It is
    /// translated when the program runs.
    pub given: Option<Span>,
    /// The whole call.
    pub span: Span,
}

impl MessageCall {
    /// The key a catalogue holds the message by: its id, or the message.
    pub fn key(&self) -> Option<&str> {
        self.id.as_deref().or(self.message.as_deref())
    }

    /// The same call, with each span `f` of what it was.
    pub fn map_spans(mut self, f: impl Fn(Span) -> Span) -> Self {
        for placeholder in &mut self.placeholders {
            placeholder.span = f(placeholder.span);
        }
        self.given = self.given.map(&f);
        self.span = f(self.span);
        self
    }

    /// How the program asks for the message where the call was: the
    /// replacements of the call's text around each placeholder, which is
    /// left as it is written, in order. A `t` of a message made elsewhere
    /// is left to the runtime's `t`, and is `None`.
    ///
    /// `__ljos_message(message, context, key, "a,b", a, b)` gives the text;
    /// `__ljos_msg(…)` with the same arguments makes the message `msg`
    /// gives, a frozen `{ id, message, context?, values? }`.
    pub fn lowering(&self) -> Option<Vec<(Span, String)>> {
        if self.given.is_some() {
            return None;
        }
        let function = match self.kind {
            MacroKind::Msg => "__ljos_msg",
            _ => "__ljos_message",
        };
        let names: Vec<&str> = self.placeholders.iter().map(|p| p.name.as_str()).collect();
        let head = format!(
            "{function}({}, {}, {}, {}",
            crate::compile::js_string(self.message.as_deref().unwrap_or_default()),
            crate::compile::js_string(self.context.as_deref().unwrap_or_default()),
            crate::compile::js_string(self.key().unwrap_or_default()),
            crate::compile::js_string(&names.join(",")),
        );
        let mut placeholders: Vec<Span> = self.placeholders.iter().map(|p| p.span).collect();
        placeholders.sort_by_key(|span| span.start);
        let mut out = Vec::with_capacity(placeholders.len() + 1);
        let mut at = self.span.start;
        let mut text = head;
        // Each value in parentheses: `${a, b}` is one value, `b`, and not
        // two arguments.
        for span in placeholders {
            out.push((Span::new(at, span.start), format!("{text}, (")));
            at = span.end;
            text = ")".to_owned();
        }
        out.push((Span::new(at, self.span.end), format!("{text})")));
        Some(out)
    }

    /// Whether `span` is a part of the call that the lowering replaces,
    /// and not inside one of its placeholders, nor inside the message a
    /// `t(m)` is given: that one is left as written, so a macro in it is
    /// lowered on its own.
    pub fn covers(&self, span: Span) -> bool {
        let inside = |p: Span| p.start <= span.start && span.end <= p.end;
        self.span.start <= span.start
            && span.end <= self.span.end
            && !self.placeholders.iter().any(|p| inside(p.span))
            && !self.given.is_some_and(inside)
    }
}

/// Why a macro does not say a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageError {
    pub kind: MessageErrorKind,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageErrorKind {
    /// A form, or a part of the message, that is not written out: the
    /// translator cannot see it (`i18n-literal`).
    Literal,
    /// Forms with no `other` (`i18n-other`).
    Other,
    /// Any other shape the macro does not take.
    Shape,
}

impl MessageError {
    fn new(kind: MessageErrorKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
        }
    }

    fn shape(message: impl Into<String>, span: Span) -> Self {
        Self::new(MessageErrorKind::Shape, message, span)
    }

    /// The same error, with its span `f` of what it was.
    pub fn map_spans(mut self, f: impl Fn(Span) -> Span) -> Self {
        self.span = f(self.span);
        self
    }
}

/// The message of `expression` when it is a macro: a call of one of
/// [`MACROS`], or a template tagged with `t` or `msg`. `None` for
/// anything else.
///
/// - A value written in a template is a placeholder named after the
///   variable it reads (`${name}` is `{name}`); any other value is
///   numbered, counting only those (`${user.name}` is `{0}`). One
///   variable shown twice is one placeholder.
/// - `plural`, `select` and `selectOrdinal` written in a template are part
///   of its message. Their forms are strings or templates; a number key
///   is an exact match (`0:` is `=0`), and `other` is required.
/// - `t({ message, context, comment, id })` says the rest of the message;
///   `t({ id })` looks it up by id alone.
/// - Text is escaped for ICU: `'` where it would start a quote, `{` and
///   `}` quoted. `#` in a form of `plural` is the count, as Lingui has it.
pub fn message_of(expression: &Expression<'_>) -> Option<Result<MessageCall, MessageError>> {
    message_of_in(expression, &|_| true)
}

/// Whether a name a choice is called by inside a message is the macro of
/// that name, and not a binding of the program's own.
pub type IsMacro<'f> = &'f dyn Fn(&IdentifierReference<'_>) -> bool;

/// [`message_of`], where a `plural`, `select` or `selectOrdinal` written
/// inside the message is part of it only when `is_macro` says its name is
/// the macro's; any other is a value the message shows.
pub fn message_of_in(
    expression: &Expression<'_>,
    is_macro: IsMacro<'_>,
) -> Option<Result<MessageCall, MessageError>> {
    match expression.without_parentheses() {
        Expression::TaggedTemplateExpression(tagged) => tagged_message_in(tagged, is_macro),
        Expression::CallExpression(call) => call_message_in(call, is_macro),
        _ => None,
    }
}

/// [`message_of`] the expression `text`, written alone: what
/// `ljos migrate --catalogue` reads a message of from the call it wrote.
/// `None` where `text` is not one expression, or not a message.
#[must_use]
pub fn message_in(text: &str) -> Option<Result<MessageCall, MessageError>> {
    let allocator = oxc_allocator::Allocator::default();
    let expression = oxc_parser::Parser::new(&allocator, text, oxc_span::SourceType::mjs())
        .parse_expression()
        .ok()?;
    message_of(&expression)
}

/// [`message_of`] a tagged template.
pub fn tagged_message(
    tagged: &TaggedTemplateExpression<'_>,
) -> Option<Result<MessageCall, MessageError>> {
    tagged_message_in(tagged, &|_| true)
}

/// [`tagged_message`], with [`message_of_in`]'s `is_macro`.
pub fn tagged_message_in(
    tagged: &TaggedTemplateExpression<'_>,
    is_macro: IsMacro<'_>,
) -> Option<Result<MessageCall, MessageError>> {
    let Expression::Identifier(tag) = &tagged.tag else {
        return None;
    };
    let kind = MacroKind::named(&tag.name)?;
    if !matches!(kind, MacroKind::T | MacroKind::Msg) {
        return None;
    }
    let mut writer = Writer::new(is_macro);
    Some(
        writer
            .template(&tagged.quasi)
            .map(|()| writer.finish(kind, tagged.span)),
    )
}

/// The name a macro is called by, and where it is written.
pub fn callee<'a>(expression: &'a Expression<'_>) -> Option<(&'a str, Span)> {
    let callee = match expression.without_parentheses() {
        Expression::TaggedTemplateExpression(tagged) => &tagged.tag,
        Expression::CallExpression(call) => &call.callee,
        _ => return None,
    };
    match callee {
        Expression::Identifier(id) if MacroKind::named(&id.name).is_some() => {
            Some((id.name.as_str(), id.span))
        }
        _ => None,
    }
}

/// [`message_of`] a call.
pub fn call_message(call: &CallExpression<'_>) -> Option<Result<MessageCall, MessageError>> {
    call_message_in(call, &|_| true)
}

/// [`call_message`], with [`message_of_in`]'s `is_macro`.
pub fn call_message_in(
    call: &CallExpression<'_>,
    is_macro: IsMacro<'_>,
) -> Option<Result<MessageCall, MessageError>> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let kind = MacroKind::named(&callee.name)?;
    Some(match kind {
        MacroKind::T | MacroKind::Msg => descriptor(kind, call, is_macro),
        _ => {
            let mut writer = Writer::new(is_macro);
            writer
                .choice(kind, call)
                .map(|()| writer.finish(kind, call.span))
        }
    })
}

/// `t({ … })`, `t(m)`.
fn descriptor(
    kind: MacroKind,
    call: &CallExpression<'_>,
    is_macro: IsMacro<'_>,
) -> Result<MessageCall, MessageError> {
    let name = kind.name();
    let [argument] = &call.arguments[..] else {
        return Err(MessageError::shape(
            format!("`{name}` takes one message: {name}`…` or {name}({{ message, context }})"),
            call.span,
        ));
    };
    let Some(argument) = argument.as_expression() else {
        return Err(MessageError::shape(
            format!("`{name}` takes one message, not a spread"),
            argument.span(),
        ));
    };
    let Expression::ObjectExpression(object) = argument.without_parentheses() else {
        if kind == MacroKind::T {
            return Ok(MessageCall {
                kind,
                message: None,
                id: None,
                context: None,
                comment: None,
                placeholders: Vec::new(),
                given: Some(argument.span()),
                span: call.span,
            });
        }
        return Err(MessageError::new(
            MessageErrorKind::Literal,
            format!("`{name}` is given its message as written: msg`…` or msg({{ message }})"),
            argument.span(),
        ));
    };
    let mut writer = Writer::new(is_macro);
    let mut message = None;
    let (mut id, mut context, mut comment) = (None, None, None);
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return Err(MessageError::shape(
                format!("the message of `{name}` is written out, with no spread"),
                property.span(),
            ));
        };
        let key = match (&property.key, property.computed) {
            (PropertyKey::StaticIdentifier(key), false) => key.name.as_str(),
            (PropertyKey::StringLiteral(key), false) => key.value.as_str(),
            _ => {
                return Err(MessageError::shape(
                    format!("the message of `{name}` has `message`, `id`, `context` and `comment`"),
                    property.key.span(),
                ));
            }
        };
        match key {
            "message" => {
                writer.value(&property.value, &format!("the message of `{name}`"))?;
                message = Some(());
            }
            "id" | "context" | "comment" => {
                let Some(text) = written(&property.value) else {
                    return Err(MessageError::new(
                        MessageErrorKind::Literal,
                        format!("the `{key}` of a message is a string, as written"),
                        property.value.span(),
                    ));
                };
                *match key {
                    "id" => &mut id,
                    "context" => &mut context,
                    _ => &mut comment,
                } = Some(text);
            }
            other => {
                return Err(MessageError::shape(
                    format!(
                        "a message has `message`, `id`, `context` and `comment`, not `{other}`"
                    ),
                    property.key.span(),
                ));
            }
        }
    }
    if message.is_none() && id.is_none() {
        return Err(MessageError::shape(
            format!("the message of `{name}` needs its `message`, or an `id`"),
            object.span,
        ));
    }
    let mut call_message = writer.finish(kind, call.span);
    if message.is_none() {
        call_message.message = None;
    }
    call_message.id = id;
    call_message.context = context;
    call_message.comment = comment;
    Ok(call_message)
}

/// A string as written: a string literal, or a template with no values.
fn written(expression: &Expression<'_>) -> Option<String> {
    match expression.without_parentheses() {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|cooked| cooked.to_string()),
        _ => None,
    }
}

/// Builds the message and its placeholders, in the order they are written.
struct Writer<'f> {
    is_macro: IsMacro<'f>,
    out: String,
    placeholders: Vec<Placeholder>,
    numbered: usize,
    /// How many forms of `plural` or `selectOrdinal` the text is in,
    /// where `#` is the count.
    plurals: usize,
}

impl<'f> Writer<'f> {
    fn new(is_macro: IsMacro<'f>) -> Self {
        Self {
            is_macro,
            out: String::new(),
            placeholders: Vec::new(),
            numbered: 0,
            plurals: 0,
        }
    }

    fn finish(self, kind: MacroKind, span: Span) -> MessageCall {
        MessageCall {
            kind,
            message: Some(self.out),
            id: None,
            context: None,
            comment: None,
            placeholders: self.placeholders,
            given: None,
            span,
        }
    }

    fn text(&mut self, text: &str) {
        escape_into(&mut self.out, text, self.plurals > 0);
    }

    /// The text of a template, its values placeholders.
    fn template(&mut self, template: &TemplateLiteral<'_>) -> Result<(), MessageError> {
        for (index, quasi) in template.quasis.iter().enumerate() {
            let Some(cooked) = quasi.value.cooked.as_ref() else {
                return Err(MessageError::shape(
                    "a message's text has an escape that is no character",
                    quasi.span,
                ));
            };
            self.text(cooked);
            if let Some(expression) = template.expressions.get(index) {
                self.shown(expression)?;
            }
        }
        Ok(())
    }

    /// A value written in a message: a placeholder, or a choice that is
    /// part of the message.
    fn shown(&mut self, expression: &Expression<'_>) -> Result<(), MessageError> {
        if let Expression::CallExpression(call) = expression.without_parentheses()
            && let Expression::Identifier(callee) = &call.callee
            && let Some(kind @ (MacroKind::Plural | MacroKind::Select | MacroKind::SelectOrdinal)) =
                MacroKind::named(&callee.name)
            && (self.is_macro)(callee)
        {
            return self.choice(kind, call);
        }
        let name = self.placeholder(expression);
        self.out.push('{');
        self.out.push_str(&name);
        self.out.push('}');
        Ok(())
    }

    /// The name of the value `expression` is, kept as a placeholder.
    fn placeholder(&mut self, expression: &Expression<'_>) -> String {
        if let Expression::Identifier(id) = expression.without_parentheses() {
            let name = id.name.as_str();
            if !self.placeholders.iter().any(|p| p.name == name) {
                self.placeholders.push(Placeholder {
                    name: name.to_owned(),
                    span: expression.span(),
                });
            }
            return name.to_owned();
        }
        let name = self.numbered.to_string();
        self.numbered += 1;
        self.placeholders.push(Placeholder {
            name: name.clone(),
            span: expression.span(),
        });
        name
    }

    /// A message as written: a string, or a template. `what` is what it
    /// is the message of, for the error when it is neither.
    fn value(&mut self, expression: &Expression<'_>, what: &str) -> Result<(), MessageError> {
        match expression.without_parentheses() {
            Expression::StringLiteral(s) => {
                self.text(&s.value);
                Ok(())
            }
            Expression::TemplateLiteral(template) => self.template(template),
            other => Err(MessageError::new(
                MessageErrorKind::Literal,
                format!(
                    "{what} is translated as written, so it is a string or a template, not `{}`",
                    shown(other)
                ),
                other.span(),
            )),
        }
    }

    /// `plural(n, { … })`: `{n, plural, one {…} other {…}}`.
    fn choice(&mut self, kind: MacroKind, call: &CallExpression<'_>) -> Result<(), MessageError> {
        let name = kind.name();
        let (Some(Argument::ObjectExpression(forms)), Some(value), 2) = (
            call.arguments.get(1),
            call.arguments.first().and_then(Argument::as_expression),
            call.arguments.len(),
        ) else {
            return Err(MessageError::shape(
                format!(
                    "`{name}` takes a value and its forms: {name}(n, {{ one: \"…\", other: \"…\" }})"
                ),
                call.span,
            ));
        };
        let arg = self.placeholder(value);
        self.out.push('{');
        self.out.push_str(&arg);
        self.out.push_str(match kind {
            MacroKind::Plural => ", plural,",
            MacroKind::SelectOrdinal => ", selectordinal,",
            _ => ", select,",
        });
        self.forms(kind, forms)?;
        self.out.push('}');
        Ok(())
    }

    fn forms(&mut self, kind: MacroKind, forms: &ObjectExpression<'_>) -> Result<(), MessageError> {
        let name = kind.name();
        let mut keys: Vec<String> = Vec::new();
        for property in &forms.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return Err(MessageError::new(
                    MessageErrorKind::Literal,
                    format!("the forms of `{name}` are written out, with no spread"),
                    property.span(),
                ));
            };
            let key = match (&property.key, property.computed) {
                (PropertyKey::NumericLiteral(n), false)
                    if kind != MacroKind::Select && n.value.fract() == 0.0 && n.value >= 0.0 =>
                {
                    format!("={}", n.value as u64)
                }
                (PropertyKey::StaticIdentifier(key), false) => key.name.to_string(),
                (PropertyKey::StringLiteral(key), false) => key.value.to_string(),
                _ => {
                    return Err(MessageError::shape(
                        format!("a form of `{name}` is named as written"),
                        property.key.span(),
                    ));
                }
            };
            if let Some(n) = key.strip_prefix('=')
                && kind != MacroKind::Select
                && !is_exact(n)
            {
                return Err(MessageError::shape(
                    format!("`{key}` is no form of `{name}`: `=` is followed by a number"),
                    property.key.span(),
                ));
            }
            if kind != MacroKind::Select && !key.starts_with('=') && !is_category(&key) {
                return Err(MessageError::shape(
                    format!(
                        "`{key}` is no form of `{name}`: zero, one, two, few, many, other, or a number"
                    ),
                    property.key.span(),
                ));
            }
            if kind == MacroKind::Select
                && !key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err(MessageError::shape(
                    format!("`{key}` is no key of `select`: a word"),
                    property.key.span(),
                ));
            }
            if keys.contains(&key) {
                return Err(MessageError::shape(
                    format!("the form `{key}` of `{name}` is there twice"),
                    property.key.span(),
                ));
            }
            self.out.push(' ');
            self.out.push_str(&key);
            self.out.push_str(" {");
            let counted = kind != MacroKind::Select;
            self.plurals += usize::from(counted);
            let value = self.value(&property.value, &format!("a form of `{name}`"));
            self.plurals -= usize::from(counted);
            value?;
            self.out.push('}');
            keys.push(key);
        }
        if !keys.iter().any(|key| key == "other") {
            return Err(MessageError::new(
                MessageErrorKind::Other,
                format!("the forms of `{name}` need `other`, the form shown when none matches"),
                Span::new(forms.span.start, forms.span.start + 1),
            ));
        }
        Ok(())
    }
}

fn is_category(key: &str) -> bool {
    matches!(key, "zero" | "one" | "two" | "few" | "many" | "other")
}

/// How a form that is no text is named in a message about it: a name as
/// it is, anything else as what it is.
fn shown(expression: &Expression<'_>) -> String {
    match expression.without_parentheses() {
        Expression::Identifier(id) => id.name.to_string(),
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(object) => format!("{}.{}", object.name, member.property.name),
            _ => "an expression".to_owned(),
        },
        Expression::CallExpression(_) => "a call".to_owned(),
        _ => "an expression".to_owned(),
    }
}

/// `text` as ICU MessageFormat's literal text, with apostrophes as
/// ICU's DOUBLE_OPTIONAL mode reads them: an apostrophe is doubled only
/// where it would start a quote, so "Don't" stays as it is.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_into(&mut out, text, false);
    out
}

/// A number after `=` as ICU writes one and the runtime reads it: digits,
/// with a sign and a fraction, and no `inf` or `NaN`.
fn is_exact(n: &str) -> bool {
    let digits = n.strip_prefix(['-', '+']).unwrap_or(n);
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && n.parse::<f64>().is_ok()
}

fn escape_into(out: &mut String, text: &str, in_plural: bool) {
    // As ICU quotes: one quote from the first brace to the last, so
    // `Use {braces}` is `Use '{braces}'`, its apostrophes doubled.
    let chars: Vec<char> = text.chars().collect();
    let brace = |c: &char| matches!(c, '{' | '}');
    // Apostrophes right after the last brace stay in the quote, doubled:
    // outside it, the first would read as doubled and the quote go on.
    let quoted = chars
        .iter()
        .position(brace)
        .zip(chars.iter().rposition(brace))
        .map(|(first, last)| (first, last + apostrophes_after(&chars, last)));
    let mut i = 0;
    while i < chars.len() {
        if let Some((first, last)) = quoted
            && i == first
        {
            // In a form the count stays out of the quote (`{#}` is the
            // count in braces): the quote closes before it and opens again
            // at the next brace or hash. Reopened anywhere else, its
            // apostrophe would be an apostrophe.
            let mut open = false;
            while i <= last {
                // A hash written as ICU writes one is a hash.
                let hash = in_plural && chars[i..].starts_with(&['\'', '#', '\'']);
                let c = chars[i];
                if in_plural && c == '#' {
                    if open {
                        out.push('\'');
                        open = false;
                    }
                    out.push('#');
                } else if c == '\'' && !hash {
                    // Doubled, an apostrophe in a quote or out of one.
                    out.push_str("''");
                } else if hash || brace(&c) {
                    if !open {
                        out.push('\'');
                        open = true;
                    }
                    out.push(if hash { '#' } else { c });
                } else {
                    out.push(c);
                }
                i += if hash { 3 } else { 1 };
            }
            if open {
                out.push('\'');
            }
            continue;
        }
        let c = chars[i];
        if c == '\'' && in_plural && chars[i..].starts_with(&['\'', '#', '\'']) {
            // In a form, `#` is the count and `'#'` a hash, as in ICU.
            out.push_str("'#'");
            i += 3;
            continue;
        }
        match c {
            // At the end of a text a placeholder may follow, whose `{`
            // would make the apostrophe a quote.
            '\'' => match chars.get(i + 1) {
                Some('\'' | '{' | '}' | '#' | '|') | None => out.push_str("''"),
                Some(_) => out.push('\''),
            },
            c => out.push(c),
        }
        i += 1;
    }
}

fn apostrophes_after(chars: &[char], at: usize) -> usize {
    chars[at + 1..].iter().take_while(|c| **c == '\'').count()
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::Statement;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    use super::*;

    fn read(source: &str) -> Result<MessageCall, MessageError> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{source}");
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("{source}");
        };
        message_of(&statement.expression).expect("a macro")
    }

    fn message(source: &str) -> String {
        read(source).unwrap().message.unwrap()
    }

    fn names(source: &str) -> Vec<String> {
        read(source)
            .unwrap()
            .placeholders
            .into_iter()
            .map(|p| p.name)
            .collect()
    }

    #[test]
    fn a_tagged_template_names_its_values() {
        assert_eq!(message("t`Open`"), "Open");
        assert_eq!(message("t`Hello ${name}`"), "Hello {name}");
        assert_eq!(names("t`Hello ${name}`"), ["name"]);
        assert_eq!(message("t`${a} and ${a}`"), "{a} and {a}");
        assert_eq!(names("t`${a} and ${a}`"), ["a"]);
        assert_eq!(
            message("t`${user.name} has ${n} of ${f()}`"),
            "{0} has {n} of {1}"
        );
        assert_eq!(names("t`${user.name} has ${n} of ${f()}`"), ["0", "n", "1"]);
    }

    #[test]
    fn placeholders_know_where_their_values_are() {
        let call = read("t`Hello ${name}!`").unwrap();
        assert_eq!(call.placeholders[0].span, Span::new(10, 14));
        assert_eq!(call.span, Span::new(0, 17));
    }

    #[test]
    fn the_lowering_keeps_each_value_where_it_is() {
        let source = r##"t`${name} has ${plural(n, { one: "# file", other: "# files" })}`"##;
        let call = read(source).unwrap();
        let mut out = source.to_owned();
        for (span, text) in call.lowering().unwrap().into_iter().rev() {
            out.replace_range(span.start as usize..span.end as usize, &text);
        }
        assert_eq!(
            out,
            r#"__ljos_message("{name} has {n, plural, one {# file} other {# files}}", "", "{name} has {n, plural, one {# file} other {# files}}", "name,n", (name), (n))"#
        );
        let source = "t`${1, 2} and ${x}`";
        let mut out = source.to_owned();
        for (span, text) in read(source).unwrap().lowering().unwrap().into_iter().rev() {
            out.replace_range(span.start as usize..span.end as usize, &text);
        }
        assert_eq!(
            out,
            r#"__ljos_message("{0} and {x}", "", "{0} and {x}", "0,x", (1, 2), (x))"#
        );
        let call = read(r#"msg({ id: "nav.home", message: "Home" })"#).unwrap();
        let lowered = call.lowering().unwrap();
        assert_eq!(lowered.len(), 1);
        assert_eq!(lowered[0].1, r#"__ljos_msg("Home", "", "nav.home", "")"#);
        assert!(read("t(m)").unwrap().lowering().is_none());
    }

    #[test]
    fn text_is_escaped_for_icu() {
        assert_eq!(message("t`Don't`"), "Don't");
        assert_eq!(message("t`a {b} c`"), "a '{b}' c");
        assert_eq!(message("t`Use {braces}`"), "Use '{braces}'");
        assert_eq!(message("t`'${x}'`"), "''{x}''");
        assert_eq!(message("t`it''s`"), "it'''s");
        assert_eq!(escape("50% #1"), "50% #1");
        assert_eq!(escape("}} {{x"), "'}} {{'x");
        assert_eq!(
            message(r#"plural(n, { other: "'#' is #, {'#'}" })"#),
            "{n, plural, other {'#' is #, '{#}'}}"
        );
        assert_eq!(
            message(r#"plural(n, { other: "{#} files" })"#),
            "{n, plural, other {'{'#'}' files}}"
        );
        assert_eq!(
            message(r#"plural(n, { other: "{# files}" })"#),
            "{n, plural, other {'{'# files'}'}}"
        );
        assert_eq!(
            message(r#"plural(n, { other: "{##}" })"#),
            "{n, plural, other {'{'##'}'}}"
        );
        assert_eq!(escape("{#}"), "'{#}'");
        assert_eq!(escape("{'}"), "'{''}'");
        assert_eq!(escape("{'x"), "'{'''x");
        assert_eq!(escape("a}''b"), "a'}'''''b");
    }

    #[test]
    fn choices_are_written_in_the_message() {
        assert_eq!(
            message(r##"plural(count, { one: "# file", other: "# files" })"##),
            "{count, plural, one {# file} other {# files}}"
        );
        assert_eq!(
            message(r##"plural(count, { 0: "No files", one: "# file", other: "# files" })"##),
            "{count, plural, =0 {No files} one {# file} other {# files}}"
        );
        assert_eq!(
            message(
                r##"selectOrdinal(place, { one: "#st", two: "#nd", few: "#rd", other: "#th" })"##
            ),
            "{place, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}"
        );
        assert_eq!(
            message(r#"select(role, { admin: "Administrator", other: "Member" })"#),
            "{role, select, admin {Administrator} other {Member}}"
        );
        assert_eq!(
            message("plural(items.length, { one: `# of ${who}`, other: `# of ${who}` })"),
            "{0, plural, one {# of {who}} other {# of {who}}}"
        );
    }

    #[test]
    fn a_choice_in_a_template_is_part_of_its_message() {
        let source = r##"t`${name} has ${plural(n, { one: "# file", other: "# files" })}`"##;
        assert_eq!(
            message(source),
            "{name} has {n, plural, one {# file} other {# files}}"
        );
        assert_eq!(names(source), ["name", "n"]);
    }

    #[test]
    fn a_choice_the_program_binds_itself_is_a_value_shown() {
        let source = r#"t`${select(role, { other: "Member" })}`"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("{source}");
        };
        let local = |_: &IdentifierReference<'_>| false;
        let call = message_of_in(&statement.expression, &local)
            .expect("a macro")
            .unwrap();
        assert_eq!(call.message.as_deref(), Some("{0}"));
        assert_eq!(call.placeholders[0].span, Span::new(4, 37));
    }

    #[test]
    fn a_descriptor_says_context_comment_and_id() {
        let call = read(r#"t({ message: "Open", context: "menu", comment: "the verb" })"#).unwrap();
        assert_eq!(call.message.as_deref(), Some("Open"));
        assert_eq!(call.context.as_deref(), Some("menu"));
        assert_eq!(call.comment.as_deref(), Some("the verb"));
        assert_eq!(call.id, None);
        let call = read(r#"t({ id: "nav.home", message: "Home" })"#).unwrap();
        assert_eq!(call.id.as_deref(), Some("nav.home"));
        assert_eq!(call.message.as_deref(), Some("Home"));
        let call = read(r#"t({ id: "nav.home" })"#).unwrap();
        assert_eq!(call.message, None);
        let call = read("t({ message: `Hi ${name}` })").unwrap();
        assert_eq!(call.message.as_deref(), Some("Hi {name}"));
        let call = read(r#"msg({ id: "nav.home", message: "Home" })"#).unwrap();
        assert_eq!(call.kind, MacroKind::Msg);
    }

    #[test]
    fn t_of_a_value_is_translated_when_it_runs() {
        let call = read("t(Menu.items[0])").unwrap();
        assert_eq!(call.message, None);
        assert_eq!(call.given, Some(Span::new(2, 15)));
        // A macro in the message it is given is lowered on its own.
        assert!(!call.covers(Span::new(2, 15)));
        assert!(call.covers(Span::new(0, 1)));
        assert_eq!(read("msg(m)").unwrap_err().kind, MessageErrorKind::Literal);
    }

    #[test]
    fn a_form_not_written_out_is_an_error_at_it() {
        let source = r##"plural(count, { one: "# file", other: many })"##;
        let error = read(source).unwrap_err();
        assert_eq!(error.kind, MessageErrorKind::Literal);
        assert_eq!(
            error.message,
            "a form of `plural` is translated as written, so it is a string or a template, not `many`"
        );
        assert_eq!(
            &source[error.span.start as usize..error.span.end as usize],
            "many"
        );
    }

    #[test]
    fn an_exact_form_may_have_a_sign() {
        assert_eq!(
            message(r#"plural(n, { "=-1": "x", "=2.5": "y", other: "z" })"#),
            "{n, plural, =-1 {x} =2.5 {y} other {z}}"
        );
    }

    #[test]
    fn forms_need_other() {
        let source = r#"select(role, { admin: "Administrator" })"#;
        let error = read(source).unwrap_err();
        assert_eq!(error.kind, MessageErrorKind::Other);
        assert_eq!(
            error.message,
            "the forms of `select` need `other`, the form shown when none matches"
        );
        assert_eq!(error.span, Span::new(13, 14));
    }

    #[test]
    fn odd_shapes_are_said() {
        for source in [
            r#"plural(n, { lots: "x", other: "y" })"#,
            r#"plural(n, { one: "x", one: "y", other: "z" })"#,
            r#"plural(n, { "=abc": "x", other: "y" })"#,
            r#"plural(n, { "=": "x", other: "y" })"#,
            r#"plural(n)"#,
            r#"t({ text: "x" })"#,
            r#"t({ context: "x" })"#,
            r#"t("a", "b")"#,
        ] {
            assert_eq!(
                read(source).unwrap_err().kind,
                MessageErrorKind::Shape,
                "{source}"
            );
        }
        assert_eq!(
            read(r#"t({ message: "x", context: c })"#).unwrap_err().kind,
            MessageErrorKind::Literal
        );
    }

    /// A form's text reads back as written, its `#` the count wherever it
    /// stands among braces and apostrophes.
    #[test]
    fn a_form_reads_back_as_written() {
        let rules = ljos_i18n::Plurals::for_language("en");
        let locale = ljos_i18n::LocaleInfo::new("en");
        let two = ljos_i18n::Value::number(2.0);
        for (form, shown) in [
            ("{#} files", "{2} files"),
            ("{# files}", "{2 files}"),
            ("{#x}", "{2x}"),
            ("{a#b}", "{a2b}"),
            ("{##}", "{22}"),
            ("{#'}", "{2'}"),
            ("{'#'}", "{#}"),
            ("{#x'#'}", "{2x#}"),
            ("'{# it's}'", "'{2 it's}'"),
            ("# of {them}", "2 of {them}"),
        ] {
            let source = format!("plural(n, {{ other: {form:?} }})");
            let written = message(&source);
            let read = ljos_i18n::Message::parse(&written)
                .unwrap_or_else(|e| panic!("{form}: {written}: {e}"));
            let said = read.format(|name| (name == "n").then_some(&two), &rules, &locale);
            assert_eq!(said, shown, "{form}: {written}");
        }
    }

    /// The runtime reads the keys the compiler writes: `ljos_i18n`'s
    /// escaping of plain text is this one.
    #[test]
    fn the_runtime_escapes_text_the_same_way() {
        for text in [
            "Don't",
            "it'",
            "Use {braces}",
            "{'}",
            "}} {{x",
            "a '{b}' c",
            "''",
            "x|y'|",
            "#'#",
            "{'x",
            "a}''b",
        ] {
            assert_eq!(
                escape(text),
                ljos_i18n::message::escape(text, false),
                "{text}"
            );
        }
    }

    #[test]
    fn anything_else_is_no_macro() {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, "f`x`; tr(1); o.t`x`", SourceType::ts()).parse();
        for statement in &parsed.program.body {
            let Statement::ExpressionStatement(statement) = statement else {
                panic!()
            };
            assert!(message_of(&statement.expression).is_none());
        }
    }
}
