//! The checker's rules of translated texts (docs/DIALECT.md §4.13):
//! Qt's `qsTr` family is not in a file of Ljós's API (`qt-tr`), and a
//! translated text joined to another value with `+` is two messages
//! (`i18n-concat`).

use qml_front::{TypeOracle, names::Target};
use qml_syntax::{
    GetSpan, Span,
    js::{BinaryExpression, BinaryOperator, Expression},
};

use super::Checker;
use crate::Code;

/// What Ljós writes for each of Qt's translation functions.
fn instead(name: &str) -> Option<&'static str> {
    Some(match name {
        "qsTr" => "t`…`",
        "qsTranslate" => "t({ message, context })",
        "qsTrId" => "t({ id })",
        "qsTrNoOp" | "QT_TR_NOOP" => "msg`…`",
        "qsTranslateNoOp" | "QT_TRANSLATE_NOOP" => "msg({ message, context })",
        "qsTrIdNoOp" | "QT_TRID_NOOP" => "msg({ id })",
        _ => return None,
    })
}

/// The `Ljos.Core` functions whose value is a translated text.
const TRANSLATED: &[&str] = &["t", "plural", "select", "selectOrdinal"];

impl<'a, O: TypeOracle> Checker<'_, '_, 'a, '_, O> {
    /// `name`, read where no local has it, is one of Qt's translation
    /// functions: reported, and whether it is.
    pub(super) fn qt_tr(&mut self, name: &str, span: Span) -> bool {
        let Some(instead) = instead(name) else {
            return false;
        };
        self.report(
            Code::QtTr,
            format!("`{name}` is Qt's: use {instead} (Ljos.Core)"),
            span,
        );
        true
    }

    /// Whether `name`, read at `at`, is `Ljos.Core`'s: no local, property
    /// or id of the file has it.
    fn core_name(&self, name: &str, at: u32) -> bool {
        self.local(name).is_none()
            && match self.c.lookup(name, at) {
                None => true,
                Some(resolution) => matches!(resolution.target, Some(Target::Global(_)) | None),
            }
    }

    /// Whether `e` is a translated text: `` t`…` ``, or a call of `t`,
    /// `plural`, `select` or `selectOrdinal`.
    fn translated(&self, e: &Expression<'a>) -> bool {
        let tag = match e.without_parentheses() {
            Expression::TaggedTemplateExpression(tagged) => &tagged.tag,
            Expression::CallExpression(call) => &call.callee,
            _ => return false,
        };
        matches!(
            tag.without_parentheses(),
            Expression::Identifier(id)
                if TRANSLATED.contains(&id.name.as_str()) && self.core_name(&id.name, id.span.start)
        )
    }

    /// `binary`, a `+`: where its sum holds a translated text beside
    /// another value, says so once, at the whole sum, with the one message
    /// that says it.
    pub(super) fn concat(&mut self, binary: &BinaryExpression<'a>) {
        if binary.operator != BinaryOperator::Addition || self.summed.contains(&binary.span) {
            return;
        }
        let mut parts = Vec::new();
        self.sum(&binary.left, &mut parts);
        self.sum(&binary.right, &mut parts);
        if !parts.iter().any(|p| self.translated(p)) {
            return;
        }
        let mut one = String::new();
        for part in &parts {
            match part.without_parentheses() {
                Expression::StringLiteral(s) => one.push_str(&in_template(&s.value)),
                Expression::TemplateLiteral(t) => one.push_str(inside(self.text(t.span))),
                Expression::TaggedTemplateExpression(t)
                    if matches!(&t.tag, Expression::Identifier(id) if id.name == "t") =>
                {
                    one.push_str(inside(self.text(t.quasi.span)));
                }
                other => {
                    one.push_str("${");
                    one.push_str(self.text(other.span()));
                    one.push('}');
                }
            }
        }
        self.report(
            Code::I18nConcat,
            format!(
                "a translated text joined to another value is two messages: write one, t`{one}`"
            ),
            binary.span,
        );
    }

    /// The values `e` adds, left to right; the sums inside it are this
    /// one's, and say nothing of their own.
    fn sum<'e>(&mut self, e: &'e Expression<'a>, parts: &mut Vec<&'e Expression<'a>>) {
        match e.without_parentheses() {
            Expression::BinaryExpression(inner) if inner.operator == BinaryOperator::Addition => {
                self.summed.push(inner.span);
                self.sum(&inner.left, parts);
                self.sum(&inner.right, parts);
            }
            _ => parts.push(e),
        }
    }
}

/// What a template literal's text holds between its backticks.
fn inside(template: &str) -> &str {
    template
        .strip_prefix('`')
        .and_then(|t| t.strip_suffix('`'))
        .unwrap_or(template)
}

/// `text` as a template literal's text.
fn in_template(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}
