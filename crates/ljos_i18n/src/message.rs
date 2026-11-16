//! ICU MessageFormat 1: the text of a `t` message and of its translations.
//!
//! What a message can say is what Lingui and most translation tools write
//! (<https://unicode-org.github.io/icu/userguide/format_parse/messages/>):
//! text, `{name}`, `{name, number}`, `{n, plural, …}` with `=N` and the
//! CLDR categories and `#`, `{n, selectordinal, …}` and `{x, select, …}`.
//! An `offset` and the date and time types are refused: Ljós writes none,
//! and a translation that uses one is not used rather than shown half
//! formatted.
//!
//! Apostrophes quote as ICU's default mode does: `''` is one apostrophe,
//! and an apostrophe before `{`, `}` (or `#` in a plural form) starts
//! quoted text that ends at the next lone apostrophe. Any other apostrophe
//! is itself, so `don't` needs no escaping.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use icu_decimal::input::Decimal;
use icu_locale::{LanguageIdentifier, Locale};
use icu_plurals::{PluralCategory, PluralOperands, PluralRules};

use crate::{LocaleInfo, locale};

/// A message that does not parse, and where it stops making sense.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason} (at character {at})")]
pub struct MessageError {
    /// Characters from the start of the message.
    pub at: usize,
    pub reason: String,
}

/// A value a message is formatted with.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    /// The value as a string, as the script would write it (`String(x)`):
    /// what a plain `{name}` shows.
    pub text: String,
    /// The value as a number, when it is one: what a plural selects by and
    /// `#` shows in the locale's digits.
    pub number: Option<f64>,
}

impl Value {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            number: None,
        }
    }

    /// A number, written as JavaScript writes it where the two agree (all
    /// integers below 2⁵³ and the shortest decimals).
    #[must_use]
    pub fn number(n: f64) -> Self {
        let text = if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
            // An integral f64 below 2^53 converts to i64 exactly.
            #[allow(clippy::cast_possible_truncation)]
            let i = n as i64;
            i.to_string()
        } else {
            n.to_string()
        };
        Self {
            text,
            number: Some(n),
        }
    }

    fn as_number(&self) -> Option<f64> {
        self.number.or_else(|| self.text.trim().parse().ok())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    Arg(String),
    /// `{name, number}`.
    Number(String),
    /// `#` in a plural form: the plural's value in the locale's digits.
    Pound,
    Plural {
        arg: String,
        ordinal: bool,
        forms: Vec<(Selector, Vec<Part>)>,
    },
    Select {
        arg: String,
        forms: Vec<(String, Vec<Part>)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Selector {
    /// `=2`.
    Exact(f64),
    Category(PluralCategory),
}

/// A parsed message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    parts: Vec<Part>,
}

const CATEGORIES: [(&str, PluralCategory); 6] = [
    ("zero", PluralCategory::Zero),
    ("one", PluralCategory::One),
    ("two", PluralCategory::Two),
    ("few", PluralCategory::Few),
    ("many", PluralCategory::Many),
    ("other", PluralCategory::Other),
];

impl Message {
    /// Reads a message.
    ///
    /// # Errors
    /// When the text is not a message this module formats: a brace that is
    /// not closed, a form without `other`, an unknown argument type.
    pub fn parse(text: &str) -> Result<Self, MessageError> {
        let mut parser = Parser {
            chars: text.chars().collect(),
            at: 0,
        };
        let parts = parser.message(false, false)?;
        Ok(Self { parts })
    }

    /// Every name the message reads a value of, nested ones included.
    #[must_use]
    pub fn names(&self) -> BTreeSet<&str> {
        fn walk<'a>(parts: &'a [Part], out: &mut BTreeSet<&'a str>) {
            for part in parts {
                match part {
                    Part::Text(_) | Part::Pound => {}
                    Part::Arg(name) | Part::Number(name) => {
                        out.insert(name);
                    }
                    Part::Plural { arg, forms, .. } => {
                        out.insert(arg);
                        forms.iter().for_each(|(_, f)| walk(f, out));
                    }
                    Part::Select { arg, forms } => {
                        out.insert(arg);
                        forms.iter().for_each(|(_, f)| walk(f, out));
                    }
                }
            }
        }
        let mut out = BTreeSet::new();
        walk(&self.parts, &mut out);
        out
    }

    /// Writes the message with `values`, choosing plural forms by `rules`
    /// and writing numbers in `locale`. A name without a value stays as it
    /// is written, `{name}`, so the gap shows.
    #[must_use]
    pub fn format<'v>(
        &self,
        values: impl Fn(&str) -> Option<&'v Value>,
        rules: &Plurals,
        locale: &LocaleInfo,
    ) -> String {
        let mut out = String::new();
        let context = Context {
            values: &values,
            rules,
            locale,
        };
        context.write(&self.parts, None, &mut out);
        out
    }
}

struct Context<'c, 'v> {
    values: &'c dyn Fn(&str) -> Option<&'v Value>,
    rules: &'c Plurals,
    locale: &'c LocaleInfo,
}

impl Context<'_, '_> {
    fn number(&self, value: &Value) -> String {
        if value.as_number().is_some() {
            self.locale.localize_number(value.text.trim())
        } else {
            value.text.clone()
        }
    }

    fn write(&self, parts: &[Part], pound: Option<&Value>, out: &mut String) {
        for part in parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Arg(name) => match (self.values)(name) {
                    Some(value) => out.push_str(&value.text),
                    None => {
                        let _ = write!(out, "{{{name}}}");
                    }
                },
                Part::Number(name) => match (self.values)(name) {
                    Some(value) => out.push_str(&self.number(value)),
                    None => {
                        let _ = write!(out, "{{{name}}}");
                    }
                },
                Part::Pound => match pound {
                    Some(value) => out.push_str(&self.number(value)),
                    None => out.push('#'),
                },
                Part::Plural {
                    arg,
                    ordinal,
                    forms,
                } => {
                    let value = (self.values)(arg);
                    let number = value.and_then(Value::as_number);
                    let exact = number.and_then(|n| {
                        forms
                            .iter()
                            .find(|(s, _)| matches!(s, Selector::Exact(e) if *e == n))
                    });
                    let category =
                        value.map_or(PluralCategory::Other, |v| self.rules.category(*ordinal, v));
                    let by = |c: PluralCategory| {
                        forms
                            .iter()
                            .find(|(s, _)| matches!(s, Selector::Category(k) if *k == c))
                    };
                    if let Some((_, form)) = exact
                        .or_else(|| by(category))
                        .or_else(|| by(PluralCategory::Other))
                    {
                        self.write(form, value, out);
                    }
                }
                Part::Select { arg, forms } => {
                    let key = (self.values)(arg).map(|v| v.text.as_str());
                    let form = forms
                        .iter()
                        .find(|(k, _)| Some(k.as_str()) == key)
                        .or_else(|| forms.iter().find(|(k, _)| k == "other"));
                    if let Some((_, form)) = form {
                        self.write(form, pound, out);
                    }
                }
            }
        }
    }
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

/// The characters a name or a select key cannot have.
fn is_syntax(c: char) -> bool {
    c.is_whitespace() || matches!(c, '{' | '}' | ',' | '#' | '\'' | '=')
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn error<T>(&self, reason: impl Into<String>) -> Result<T, MessageError> {
        Err(MessageError {
            at: self.at,
            reason: reason.into(),
        })
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), MessageError> {
        if self.peek() == Some(c) {
            self.at += 1;
            Ok(())
        } else {
            self.error(format!("expected `{c}`"))
        }
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while self.peek().is_some_and(|c| !is_syntax(c)) {
            self.at += 1;
        }
        self.chars[start..self.at].iter().collect()
    }

    /// Text and arguments up to the end, or up to the `}` that closes a
    /// form when `nested`.
    fn message(&mut self, plural: bool, nested: bool) -> Result<Vec<Part>, MessageError> {
        let mut parts = Vec::new();
        let mut text = String::new();
        let flush = |text: &mut String, parts: &mut Vec<Part>| {
            if !text.is_empty() {
                parts.push(Part::Text(std::mem::take(text)));
            }
        };
        loop {
            match self.peek() {
                None if nested => return self.error("a form is not closed with `}`"),
                None => break,
                Some('}') if nested => break,
                Some('}') => return self.error("a `}` that no `{` opened"),
                Some('{') => {
                    flush(&mut text, &mut parts);
                    parts.push(self.argument(plural)?);
                }
                Some('#') if plural => {
                    self.at += 1;
                    flush(&mut text, &mut parts);
                    parts.push(Part::Pound);
                }
                Some('\'') => {
                    self.at += 1;
                    match self.peek() {
                        Some('\'') => {
                            self.at += 1;
                            text.push('\'');
                        }
                        Some(c) if matches!(c, '{' | '}') || (plural && c == '#') => {
                            self.quoted(&mut text);
                        }
                        _ => text.push('\''),
                    }
                }
                Some(c) => {
                    self.at += 1;
                    text.push(c);
                }
            }
        }
        flush(&mut text, &mut parts);
        Ok(parts)
    }

    /// Quoted text after its opening apostrophe, to the next lone one or
    /// the end of the message.
    fn quoted(&mut self, text: &mut String) {
        while let Some(c) = self.peek() {
            self.at += 1;
            if c != '\'' {
                text.push(c);
            } else if self.peek() == Some('\'') {
                self.at += 1;
                text.push('\'');
            } else {
                return;
            }
        }
    }

    fn argument(&mut self, plural: bool) -> Result<Part, MessageError> {
        self.expect('{')?;
        self.space();
        let name = self.word();
        if name.is_empty() {
            return self.error("an argument has no name");
        }
        self.space();
        if self.peek() == Some('}') {
            self.at += 1;
            return Ok(Part::Arg(name));
        }
        if self.peek().is_none() {
            return self.error("expected `}`");
        }
        self.expect(',')?;
        self.space();
        let kind = self.word();
        self.space();
        match kind.as_str() {
            "number" => {
                if self.peek() != Some('}') {
                    return self.error("a `number` argument has no style in Ljós");
                }
                self.at += 1;
                Ok(Part::Number(name))
            }
            "plural" | "selectordinal" => {
                self.expect(',')?;
                let forms = self.forms(true, |p, key| {
                    if let Some(n) = key.strip_prefix('=') {
                        return n
                            .parse::<f64>()
                            .map(Selector::Exact)
                            .or_else(|_| p.error(format!("`={n}` is not a number")));
                    }
                    CATEGORIES
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, c)| Selector::Category(*c))
                        .map_or_else(
                            || {
                                p.error(format!(
                                    "`{key}` is not a plural category (zero, one, two, few, many, other) or `=N`"
                                ))
                            },
                            Ok,
                        )
                })?;
                if !forms
                    .iter()
                    .any(|(s, _)| *s == Selector::Category(PluralCategory::Other))
                {
                    return self.error(format!("the forms of `{name}` need `other`"));
                }
                Ok(Part::Plural {
                    arg: name,
                    ordinal: kind == "selectordinal",
                    forms,
                })
            }
            "select" => {
                self.expect(',')?;
                let forms = self.forms(plural, |_, key| Ok(key.to_owned()))?;
                if !forms.iter().any(|(k, _)| k == "other") {
                    return self.error(format!("the forms of `{name}` need `other`"));
                }
                Ok(Part::Select { arg: name, forms })
            }
            "" => self.error("an argument's type is missing after `,`"),
            other => self.error(format!("`{other}` arguments are not supported")),
        }
    }

    fn forms<K: PartialEq>(
        &mut self,
        plural: bool,
        key: impl Fn(&Self, &str) -> Result<K, MessageError>,
    ) -> Result<Vec<(K, Vec<Part>)>, MessageError> {
        let mut forms: Vec<(K, Vec<Part>)> = Vec::new();
        loop {
            self.space();
            match self.peek() {
                Some('}') => {
                    self.at += 1;
                    return Ok(forms);
                }
                None => return self.error("the forms are not closed with `}`"),
                Some(_) => {}
            }
            let start = self.at;
            if self.peek() == Some('=') {
                self.at += 1;
            }
            let rest = self.word();
            let written: String = self.chars[start..self.at].iter().collect();
            if rest.is_empty() {
                if self.peek() == Some('{') {
                    return self.error("a form has no key");
                }
                if written == "=" {
                    return self.error("`=` is not followed by a number");
                }
                return self.error("expected a form's key");
            }
            if written == "offset:" || written.starts_with("offset") {
                return self.error("an `offset` is not supported");
            }
            let k = key(self, &written)?;
            if forms.iter().any(|(f, _)| *f == k) {
                return self.error(format!("the form `{written}` is there twice"));
            }
            self.space();
            self.expect('{')?;
            let form = self.message(plural, true)?;
            self.expect('}')?;
            forms.push((k, form));
        }
    }
}

/// Writes text so that a message reads it back as that text, as ICU
/// quotes: one quote from the first `{` or `}` (or, where it would be a
/// plural's value, `#`) to the last, its apostrophes doubled, and an
/// apostrophe outside it doubled.
#[must_use]
pub fn escape(text: &str, in_plural: bool) -> String {
    // The compiler's rule (`ljos_compiler::i18n::message::escape`), so a
    // tool that keys text with this gets the keys the compiler emits: an
    // apostrophe outside the quote is doubled only where it would start
    // one, so `Don't` stays `Don't`.
    let chars: Vec<char> = text.chars().collect();
    let syntax = |c: &char| matches!(c, '{' | '}') || (in_plural && *c == '#');
    let quoted = chars
        .iter()
        .position(syntax)
        .zip(chars.iter().rposition(syntax));
    let mut out = String::with_capacity(text.len() + 2);
    for (i, c) in chars.iter().enumerate() {
        let inside = quoted.is_some_and(|(first, last)| (first..=last).contains(&i));
        if quoted.is_some_and(|(first, _)| i == first) {
            out.push('\'');
        }
        if *c == '\''
            && (inside || matches!(chars.get(i + 1), Some('\'' | '{' | '}' | '#' | '|') | None))
        {
            out.push('\'');
        }
        out.push(*c);
        if quoted.is_some_and(|(_, last)| i == last) {
            out.push('\'');
        }
    }
    out
}

/// The plural rules of one language as messages use them: CLDR's
/// categories, cardinal and ordinal, with none of the numerus forms of
/// `.ts` files ([`crate::PluralForms`]).
#[derive(Debug)]
pub struct Plurals {
    cardinal: Option<PluralRules>,
    ordinal: Option<PluralRules>,
}

impl Plurals {
    /// The rules of a language named as Qt or BCP 47 name it. A language
    /// CLDR does not know says `other` for every number.
    #[must_use]
    pub fn for_language(language: &str) -> Self {
        let Some(parsed) = locale::parse(language) else {
            return Self {
                cardinal: None,
                ordinal: None,
            };
        };
        // As in `PluralForms`: rules belong to a language and sometimes a
        // region, never to a script.
        let mut id = LanguageIdentifier::from(parsed.id.language);
        id.region = parsed.id.region;
        let language = Locale::from(id);
        Self {
            cardinal: PluralRules::try_new_cardinal((&language).into()).ok(),
            ordinal: PluralRules::try_new_ordinal((&language).into()).ok(),
        }
    }

    fn category(&self, ordinal: bool, value: &Value) -> PluralCategory {
        let rules = if ordinal {
            &self.ordinal
        } else {
            &self.cardinal
        };
        let Some(rules) = rules else {
            return PluralCategory::Other;
        };
        // The written number keeps its fraction digits (`1.0` is not `1`
        // in English), which is what CLDR's operands are about.
        let text = value.text.trim();
        let operands = Decimal::try_from_str(text)
            .ok()
            .or_else(|| {
                value
                    .as_number()
                    .and_then(|n| Decimal::try_from_str(&Value::number(n).text).ok())
            })
            .map(|d| PluralOperands::from(&d));
        operands.map_or(PluralCategory::Other, |o| rules.category_for(o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(message: &str, language: &str, values: &[(&str, Value)]) -> String {
        let m = Message::parse(message).unwrap();
        let rules = Plurals::for_language(language);
        let locale = LocaleInfo::new(language);
        m.format(
            |name| values.iter().find(|(n, _)| *n == name).map(|(_, v)| v),
            &rules,
            &locale,
        )
    }

    fn n(x: f64) -> Value {
        Value::number(x)
    }

    #[test]
    fn text_and_arguments() {
        assert_eq!(
            say("Hello {name}", "en", &[("name", Value::text("Ada"))]),
            "Hello Ada"
        );
        assert_eq!(say("Hello {name}", "en", &[]), "Hello {name}");
        assert_eq!(
            say("{ a }{b}", "en", &[("a", n(1.0)), ("b", n(2.5))]),
            "12.5"
        );
    }

    #[test]
    fn apostrophes_quote_only_syntax() {
        assert_eq!(say("don't", "en", &[]), "don't");
        assert_eq!(say("it''s", "en", &[]), "it's");
        assert_eq!(say("'{name}' is literal", "en", &[]), "{name} is literal");
        assert_eq!(say("'{'''", "en", &[]), "{'");
        assert_eq!(
            say("{n, plural, other {'#' is # }}", "en", &[("n", n(3.0))]),
            "# is 3 "
        );
        assert_eq!(say("a # b", "en", &[]), "a # b");
    }

    #[test]
    fn escape_doubles_an_apostrophe_only_where_it_would_quote() {
        assert_eq!(escape("Don't", false), "Don't");
        assert_eq!(escape("it'", false), "it''");
        assert_eq!(escape("Use {braces}", false), "Use '{braces}'");
        assert_eq!(escape("{'}", false), "'{''}'");
    }

    #[test]
    fn escape_round_trips() {
        for text in [
            "it's",
            "{a}",
            "'{'",
            "# and '#'",
            "}}{{",
            "''",
            "it'",
            "a'#",
        ] {
            let m = Message::parse(&escape(text, false)).unwrap();
            assert_eq!(m.parts, vec![Part::Text(text.to_owned())], "{text}");
            let inner = format!("{{n, plural, other {{{}}}}}", escape(text, true));
            assert_eq!(say(&inner, "en", &[("n", n(1.0))]), text);
        }
    }

    #[test]
    fn plurals_choose_by_exact_value_then_category() {
        let files = "{count, plural, =0 {No files} one {# file} other {# files}}";
        let at = |c: f64, lang| say(files, lang, &[("count", n(c))]);
        assert_eq!(at(0.0, "en"), "No files");
        assert_eq!(at(1.0, "en"), "1 file");
        assert_eq!(at(1000.0, "en"), "1,000 files");
        assert_eq!(at(1.5, "en"), "1.5 files");
        let skra = "{count, plural, one {# skrá} other {# skrár}}";
        assert_eq!(say(skra, "is", &[("count", n(21.0))]), "21 skrá");
        assert_eq!(say(skra, "is", &[("count", n(11.0))]), "11 skrár");
        assert_eq!(say(skra, "is", &[("count", n(1000.0))]), "1.000 skrár");
    }

    #[test]
    fn ordinals_use_the_ordinal_rules() {
        let place = "{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}";
        let got: Vec<_> = [1.0, 2.0, 3.0, 4.0, 11.0, 22.0]
            .iter()
            .map(|x| say(place, "en", &[("n", n(*x))]))
            .collect();
        assert_eq!(got, ["1st", "2nd", "3rd", "4th", "11th", "22nd"]);
    }

    #[test]
    fn select_and_nesting() {
        let role = "{role, select, admin {Admin} other {Member}}";
        assert_eq!(say(role, "en", &[("role", Value::text("admin"))]), "Admin");
        assert_eq!(say(role, "en", &[("role", Value::text("guest"))]), "Member");
        let nested =
            "{name} has {n, plural, one {{g, select, f {her #} other {# file}}} other {# files}}";
        assert_eq!(
            say(
                nested,
                "en",
                &[
                    ("name", Value::text("Ada")),
                    ("n", n(1.0)),
                    ("g", Value::text("f"))
                ]
            ),
            "Ada has her 1"
        );
    }

    #[test]
    fn names_are_every_value_read() {
        let m = Message::parse("{a} {n, plural, one {{b}} other {{c, select, x {} other {}}}}")
            .unwrap();
        assert_eq!(
            m.names().into_iter().collect::<Vec<_>>(),
            ["a", "b", "c", "n"]
        );
    }

    #[test]
    fn broken_messages_are_refused() {
        for (text, reason) in [
            ("Hello {name", "expected `}`"),
            ("a } b", "a `}` that no `{` opened"),
            ("{n, plural, one {x}}", "the forms of `n` need `other`"),
            ("{n, select, a {x}}", "the forms of `n` need `other`"),
            (
                "{n, plural, lots {x} other {y}}",
                "`lots` is not a plural category",
            ),
            (
                "{n, plural, offset:1 other {y}}",
                "an `offset` is not supported",
            ),
            ("{d, date}", "`date` arguments are not supported"),
            ("{}", "an argument has no name"),
            (
                "{n, plural, one {x} one {y} other {z}}",
                "the form `one` is there twice",
            ),
        ] {
            let error = Message::parse(text).unwrap_err();
            assert!(error.reason.starts_with(reason), "{text}: {error}");
        }
    }
}
