//! `IntValidator`, `DoubleValidator` and `RegularExpressionValidator`.
//!
//! A validator says of a text whether it is acceptable, could become
//! acceptable with more typing (intermediate), or is invalid. A `TextInput`
//! refuses typing that would make its text invalid and reports
//! `acceptableInput` for the rest.
//!
//! The verdicts follow doc.qt.io (`qml-qtquick-intvalidator.html`,
//! `qml-qtquick-doublevalidator.html`,
//! `qml-qtquick-regularexpressionvalidator.html`) and the cases
//! `features/text-input/validator.feature` checks on qml6. Numbers are read
//! in the C locale: `locale` is kept and not used.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ObjectId, QmlObject, QtObject, Value, qml_impl};
use regex_automata::hybrid::dfa::{Cache, DFA};
use regex_automata::util::syntax;
use regex_automata::{Anchored, Input, MatchKind};

use crate::enums::EnumTable;
use crate::item::is_a;

/// The log category of a validator that cannot do its work.
pub const CATEGORY: &str = "qt.quick.validator";

/// What a validator says of a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    /// No more typing makes it acceptable.
    Invalid,
    /// Not acceptable as it is, but the start of something that is.
    Intermediate,
    Acceptable,
}

/// `IntValidator` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "IntValidator", extends = QtObject)]
pub struct IntValidator {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub bottom: i32,
    #[qml(notify)]
    pub top: i32,
    /// Kept; numbers are read in the C locale.
    #[qml(notify)]
    pub locale: String,
}

impl Default for IntValidator {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            bottom: i32::MIN,
            top: i32::MAX,
            locale: String::new(),
        }
    }
}

#[qml_impl]
impl IntValidator {}

/// `DoubleValidator.notation`.
pub mod notation {
    pub const STANDARD: i32 = 0;
    pub const SCIENTIFIC: i32 = 1;
}

fn double_enums() -> EnumTable {
    vec![(
        "Notation",
        vec![
            ("StandardNotation", i64::from(notation::STANDARD)),
            ("ScientificNotation", i64::from(notation::SCIENTIFIC)),
        ],
    )]
}

/// `DoubleValidator` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "DoubleValidator",
    extends = QtObject,
    enums = double_enums
)]
pub struct DoubleValidator {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub bottom: f64,
    #[qml(notify)]
    pub top: f64,
    /// The most digits after the decimal point.
    #[qml(notify)]
    pub decimals: i32,
    /// One of [`notation`].
    #[qml(notify, enumeration = "Notation")]
    pub notation: i32,
    /// Kept; numbers are read in the C locale.
    #[qml(notify)]
    pub locale: String,
}

impl Default for DoubleValidator {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            bottom: f64::NEG_INFINITY,
            top: f64::INFINITY,
            decimals: 1000,
            notation: notation::SCIENTIFIC,
            locale: String::new(),
        }
    }
}

#[qml_impl]
impl DoubleValidator {}

/// A pattern made into something that judges texts.
enum Matcher {
    /// A regular pattern: one automaton gives every verdict.
    Plain(Box<(DFA, Cache)>),
    /// A pattern with look-around or back-references. `exact` says whether
    /// a text is acceptable; `relaxed` matches at least what the pattern
    /// does, so where it rules a text out no more typing helps.
    Fancy {
        exact: fancy_regex::Regex,
        relaxed: Box<(DFA, Cache)>,
    },
}

/// A compiled pattern, kept until the pattern changes.
#[derive(Default)]
struct Compiled {
    source: String,
    flags: String,
    /// `None`: the pattern does not compile, and everything is accepted.
    matcher: Option<Matcher>,
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Compiled")
            .field("source", &self.source)
            .field("flags", &self.flags)
            .field("compiles", &self.matcher.is_some())
            .finish()
    }
}

/// `RegularExpressionValidator` (QtQuick).
///
/// A regular pattern runs as a DFA, which also says when a text can still
/// become a match. Look-around and back-references need a backtracking
/// matcher (`fancy-regex`), which says only whether the text matches; the
/// pattern with those parts loosened runs as a DFA beside it to say when a
/// text can never match. Where a look-ahead would decide only on text not
/// yet typed, that leaves the text intermediate.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "RegularExpressionValidator",
    extends = QtObject
)]
pub struct RegularExpressionValidator {
    #[qml(base)]
    pub object: QtObject,
    /// A JavaScript regular expression (`/[a-z]+/`).
    #[qml(notify)]
    pub regular_expression: Value,
    #[qml(skip)]
    compiled: Rc<RefCell<Option<Compiled>>>,
}

impl Default for RegularExpressionValidator {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            regular_expression: Value::Undefined,
            compiled: Rc::default(),
        }
    }
}

#[qml_impl]
impl RegularExpressionValidator {}

/// What `validator` says of `text`. Reads the validator's properties
/// (tracked, so an effect that asks runs again when the range changes).
/// An object that is no validator accepts everything.
pub fn validate(engine: &Engine, validator: ObjectId, text: &str) -> Validation {
    if !engine.is_alive(validator) {
        return Validation::Acceptable;
    }
    if is_a::<IntValidator>(engine, validator) {
        let int = |p: ljos_engine::Prop<i32>| engine.read(validator, p.index).to_number();
        validate_int(text, int(IntValidator::BOTTOM), int(IntValidator::TOP))
    } else if is_a::<DoubleValidator>(engine, validator) {
        let num = |p: ljos_engine::PropIndex| engine.read(validator, p).to_number();
        validate_double(
            text,
            num(DoubleValidator::BOTTOM.index),
            num(DoubleValidator::TOP.index),
            num(DoubleValidator::DECIMALS.index) as i32,
            num(DoubleValidator::NOTATION.index) as i32 == notation::STANDARD,
        )
    } else if is_a::<RegularExpressionValidator>(engine, validator) {
        validate_regex(engine, validator, text)
    } else {
        Validation::Acceptable
    }
}

/// How many digits a whole number is written with.
fn digits(v: f64) -> u32 {
    let v = v.abs().floor();
    if v < 10.0 {
        1
    } else {
        v.log10().floor() as u32 + 1
    }
}

/// `IntValidator`: a whole number between `bottom` and `top`.
pub fn validate_int(text: &str, bottom: f64, top: f64) -> Validation {
    if text.is_empty() {
        return Validation::Intermediate;
    }
    let (negative, rest) = match text.as_bytes()[0] {
        // A sign that no number in the range has cannot be typed.
        b'-' if bottom >= 0.0 => return Validation::Invalid,
        b'+' if top < 0.0 => return Validation::Invalid,
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if rest.is_empty() {
        return Validation::Intermediate;
    }
    // Digits, and the separator between groups of them after the first.
    let mut value = 0.0_f64;
    for (i, c) in rest.chars().enumerate() {
        match c.to_digit(10) {
            Some(d) => value = value * 10.0 + f64::from(d),
            None if c == ',' && i > 0 => {}
            None => return Validation::Invalid,
        }
    }
    let v = if negative { -value } else { value };
    if v >= bottom && v <= top {
        return Validation::Acceptable;
    }
    let hopeless = if v >= 0.0 {
        // More digits only make it larger: too large already, and longer
        // than the largest number allowed, never comes back.
        v > top && -v < bottom && digits(v) > digits(top)
    } else {
        v < bottom
    };
    if hopeless {
        Validation::Invalid
    } else {
        Validation::Intermediate
    }
}

/// `DoubleValidator`: a number between `bottom` and `top` with at most
/// `decimals` digits after the point; with `standard` no exponent.
pub fn validate_double(
    text: &str,
    bottom: f64,
    top: f64,
    decimals: i32,
    standard: bool,
) -> Validation {
    if text.is_empty() {
        return Validation::Intermediate;
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    match bytes[0] {
        b'-' if bottom >= 0.0 => return Validation::Invalid,
        b'+' if top < 0.0 => return Validation::Invalid,
        b'-' | b'+' => i = 1,
        _ => {}
    }
    // The number without what `f64` does not parse (group separators).
    let mut plain = String::with_capacity(text.len());
    plain.push_str(&text[..i]);
    let mut whole = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b',' && whole > 0)) {
        if bytes[i] != b',' {
            plain.push(char::from(bytes[i]));
            whole += 1;
        }
        i += 1;
    }
    let mut fraction = 0;
    if i < bytes.len() && bytes[i] == b'.' {
        plain.push('.');
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            plain.push(char::from(bytes[i]));
            fraction += 1;
            i += 1;
        }
        if fraction > decimals.max(0) {
            return Validation::Invalid;
        }
    }
    let mut complete = whole + fraction > 0;
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        if standard {
            return Validation::Invalid;
        }
        plain.push('e');
        i += 1;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            plain.push(char::from(bytes[i]));
            i += 1;
        }
        let mut exponent = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            plain.push(char::from(bytes[i]));
            exponent += 1;
            i += 1;
        }
        complete &= exponent > 0;
    }
    if i < bytes.len() {
        return Validation::Invalid;
    }
    if !complete {
        return Validation::Intermediate;
    }
    let Ok(v) = plain.parse::<f64>() else {
        return Validation::Intermediate;
    };
    if v >= bottom && v <= top {
        return Validation::Acceptable;
    }
    // In standard notation a number with more digits before the point than
    // either bound has is out of range whatever follows. With an exponent
    // anything can still come back.
    let widest = bottom.abs().max(top.abs());
    if standard && widest.is_finite() && digits(v) > digits(widest) {
        Validation::Invalid
    } else {
        Validation::Intermediate
    }
}

/// Compiles a JavaScript pattern so that it has to match the whole text.
fn compile(source: &str, flags: &str) -> Option<Matcher> {
    if let Some((dfa, cache)) = automaton(source, flags) {
        return Some(Matcher::Plain(Box::new((dfa, cache))));
    }
    let exact = fancy_regex::RegexBuilder::new(&format!(r"\A(?:{source})\z"))
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .build()
        .ok()?;
    // The flags go inline so that the tree carries them to every part.
    let inline: String = flags
        .chars()
        .filter(|c| matches!(c, 'i' | 'm' | 's'))
        .collect();
    let flagged = if inline.is_empty() {
        format!("(?:{source})")
    } else {
        format!("(?{inline})(?:{source})")
    };
    let tree = fancy_regex::Expr::parse_tree(&flagged).ok()?;
    let mut loose = String::new();
    relax(&tree.expr, &mut loose);
    let relaxed = automaton(&loose, "")?;
    Some(Matcher::Fancy {
        exact,
        relaxed: Box::new(relaxed),
    })
}

/// The anchored automaton of a pattern the `regex` crates take.
fn automaton(source: &str, flags: &str) -> Option<(DFA, Cache)> {
    let dfa = DFA::builder()
        .configure(DFA::config().match_kind(MatchKind::All))
        .syntax(
            syntax::Config::new()
                .case_insensitive(flags.contains('i'))
                .multi_line(flags.contains('m'))
                .dot_matches_new_line(flags.contains('s')),
        )
        .build(&format!(r"(?:{source})\z"))
        .ok()?;
    let cache = dfa.create_cache();
    Some((dfa, cache))
}

/// Writes `expr` as a pattern the automaton takes that matches at least
/// what `expr` does: an assertion it cannot check always holds, and a
/// back-reference, like anything else only backtracking can do, matches
/// any text.
fn relax(expr: &fancy_regex::Expr, buf: &mut String) {
    use fancy_regex::{Assertion, Expr};
    let group = |child: &Expr, buf: &mut String| {
        buf.push_str("(?:");
        relax(child, buf);
        buf.push(')');
    };
    match expr {
        Expr::Concat(children) => {
            buf.push_str("(?:");
            for child in children {
                relax(child, buf);
            }
            buf.push(')');
        }
        Expr::Alt(children) => {
            buf.push_str("(?:");
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    buf.push('|');
                }
                relax(child, buf);
            }
            buf.push(')');
        }
        Expr::Group(child) => group(child, buf),
        Expr::AtomicGroup(child) => group(child, buf),
        Expr::Repeat { child, lo, hi, .. } => {
            group(child, buf);
            if *hi == usize::MAX {
                buf.push_str(&format!("{{{lo},}}"));
            } else {
                buf.push_str(&format!("{{{lo},{hi}}}"));
            }
        }
        Expr::Assertion(
            Assertion::StartText
            | Assertion::EndText
            | Assertion::StartLine { .. }
            | Assertion::EndLine { .. },
        )
        | Expr::Empty
        | Expr::Any { .. }
        | Expr::Literal { .. }
        | Expr::Delegate { .. } => {
            buf.push_str("(?:");
            expr.to_str(buf, 0);
            buf.push(')');
        }
        Expr::LookAround(..) | Expr::Assertion(_) => {}
        _ => buf.push_str("(?s:.*)"),
    }
}

/// What a matcher says of `text`.
fn judge(matcher: &mut Matcher, text: &str) -> Option<Validation> {
    match matcher {
        Matcher::Plain(plain) => {
            let (dfa, cache) = &mut **plain;
            run(dfa, cache, text)
        }
        Matcher::Fancy { exact, relaxed } => {
            // Past its backtracking limit the matcher cannot tell; the text
            // is then left as it is, neither accepted nor refused.
            if exact.is_match(text).unwrap_or(false) {
                return Some(Validation::Acceptable);
            }
            let (dfa, cache) = &mut **relaxed;
            Some(match run(dfa, cache, text)? {
                Validation::Invalid => Validation::Invalid,
                _ => Validation::Intermediate,
            })
        }
    }
}

/// Walks the text through the pattern's automaton: a dead state means no
/// text starting like this one matches.
fn run(dfa: &DFA, cache: &mut Cache, text: &str) -> Option<Validation> {
    let input = Input::new(text).anchored(Anchored::Yes);
    let mut state = dfa.start_state_forward(cache, &input).ok()?;
    for &b in text.as_bytes() {
        state = dfa.next_state(cache, state, b).ok()?;
        if state.is_dead() {
            return Some(Validation::Invalid);
        }
    }
    let end = dfa.next_eoi_state(cache, state).ok()?;
    Some(if end.is_match() {
        Validation::Acceptable
    } else {
        Validation::Intermediate
    })
}

fn validate_regex(engine: &Engine, validator: ObjectId, text: &str) -> Validation {
    let pattern = engine.read(
        validator,
        RegularExpressionValidator::REGULAR_EXPRESSION.index,
    );
    let (source, flags) = match &pattern {
        // A string is taken as a pattern too, for a host that sets one.
        Value::String(s) => (s.as_str().to_owned(), String::new()),
        other => {
            let member = |key| {
                engine
                    .js_member(other, key)
                    .map(|v| v.to_js_string().as_str().to_owned())
            };
            match member("source") {
                Some(source) => (source, member("flags").unwrap_or_default()),
                // No pattern: everything is acceptable.
                None => return Validation::Acceptable,
            }
        }
    };
    let Ok(slot) = engine.with(validator, |v: &RegularExpressionValidator| {
        Rc::clone(&v.compiled)
    }) else {
        return Validation::Acceptable;
    };
    let mut slot = slot.borrow_mut();
    if !slot
        .as_ref()
        .is_some_and(|c| c.source == source && c.flags == flags)
    {
        let matcher = compile(&source, &flags);
        if matcher.is_none() {
            engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!(
                    "RegularExpressionValidator: /{source}/{flags} is not a pattern Ljós can \
                     compile; it accepts everything"
                ),
            );
        }
        *slot = Some(Compiled {
            source,
            flags,
            matcher,
        });
    }
    match slot.as_mut().and_then(|c| c.matcher.as_mut()) {
        Some(matcher) => judge(matcher, text).unwrap_or(Validation::Acceptable),
        None => Validation::Acceptable,
    }
}

#[cfg(test)]
mod tests {
    use super::Validation::{Acceptable, Intermediate, Invalid};
    use super::*;

    #[test]
    fn int_validator_judges_as_qt_does() {
        for (bottom, top, text, want) in [
            (10, 99, "42", Acceptable),
            (10, 99, "5", Intermediate),
            (10, 99, "100", Invalid),
            (10, 99, "4a", Invalid),
            (10, 99, "4 ", Invalid),
            (10, 99, "4.", Invalid),
            (10, 99, "-", Invalid),
            (10, 99, "+5", Intermediate),
            (10, 99, "05", Intermediate),
            (100, 900, "0123", Acceptable),
            (100, 900, "999", Intermediate),
            (100, 900, "1234", Invalid),
            (5, 20, "30", Intermediate),
            (5, 20, "200", Invalid),
            (-10, 99, "-", Intermediate),
            (-10, 99, "-05", Acceptable),
            (-10, 99, "-11", Invalid),
            (-900, -100, "5", Intermediate),
            (-900, -100, "-5", Intermediate),
            (-900, -100, "-950", Invalid),
            (-900, -100, "+", Invalid),
            (-900, -100, "-500", Acceptable),
            (i32::MIN, i32::MAX, "1,000", Acceptable),
            (i32::MIN, i32::MAX, "12345678901", Invalid),
            (i32::MIN, i32::MAX, "", Intermediate),
        ] {
            assert_eq!(
                validate_int(text, f64::from(bottom), f64::from(top)),
                want,
                "{text:?} in {bottom}..{top}"
            );
        }
    }

    #[test]
    fn double_validator_judges_as_qt_does() {
        for (bottom, top, decimals, standard, text, want) in [
            (0.0, 1.0, 2, true, "0.5", Acceptable),
            (0.0, 1.0, 2, true, ".5", Acceptable),
            (0.0, 1.0, 2, true, "+.5", Acceptable),
            (0.0, 1.0, 2, true, "+", Intermediate),
            (0.0, 1.0, 2, true, "7", Intermediate),
            (0.0, 1.0, 2, true, "1.5", Intermediate),
            (0.0, 1.0, 2, true, "-", Invalid),
            (0.0, 1.0, 2, true, "12", Invalid),
            (0.0, 1.0, 2, true, "0.555", Invalid),
            (0.0, 1.0, 2, true, "1e", Invalid),
            (0.0, 1.0, 2, true, "0,5", Intermediate),
            (-5.5, 100.0, 5, true, "-5.6", Intermediate),
            (-5.5, 100.0, 5, true, "-55", Intermediate),
            (-5.5, 100.0, 5, true, "100.5", Intermediate),
            (-5.5, 100.0, 5, true, "1000", Invalid),
            (-5.5, 100.0, 5, true, "3.14159", Acceptable),
            (0.0, 1.0, 2, false, "0.5e0", Acceptable),
            (0.0, 1.0, 2, false, "12", Intermediate),
            (0.0, 1.0, 2, false, "1e5", Intermediate),
            (0.0, 1.0, 2, false, "1e", Intermediate),
            (0.0, 1.0, 2, false, "1e-", Intermediate),
            (0.0, 1.0, 2, false, "1ee", Invalid),
            (0.0, 1.0, 2, false, "1x", Invalid),
            (
                f64::NEG_INFINITY,
                f64::INFINITY,
                1000,
                false,
                "-1e3",
                Acceptable,
            ),
            (
                f64::NEG_INFINITY,
                f64::INFINITY,
                1000,
                false,
                "1.2.",
                Invalid,
            ),
            (f64::NEG_INFINITY, f64::INFINITY, 1000, false, "1 ", Invalid),
        ] {
            assert_eq!(
                validate_double(text, bottom, top, decimals, standard),
                want,
                "{text:?} in {bottom}..{top}"
            );
        }
    }

    fn regex(source: &str, flags: &str, text: &str) -> Validation {
        let mut matcher = compile(source, flags).expect("the pattern compiles");
        judge(&mut matcher, text).expect("the automaton runs")
    }

    #[test]
    fn a_pattern_has_to_match_the_whole_text() {
        assert_eq!(regex("b+", "", "bb"), Acceptable);
        assert_eq!(regex("b+", "", "a"), Invalid);
        assert_eq!(regex("b+", "", "bbx"), Invalid);
        assert_eq!(regex("b+", "", ""), Intermediate);
        assert_eq!(regex("[a-c]+x", "", "abc"), Intermediate);
        assert_eq!(regex("[a-c]+x", "", "abd"), Invalid);
        assert_eq!(regex("[a-c]+x", "", "abcx"), Acceptable);
        assert_eq!(regex("AB+", "i", "abb"), Acceptable);
        assert_eq!(regex("AB+", "", "abb"), Invalid);
        assert_eq!(regex("[0-9]{3}", "", "12"), Intermediate);
        assert_eq!(regex("[0-9]{3}", "", "1234"), Invalid);
    }

    #[test]
    fn a_regular_pattern_needs_no_backtracking() {
        assert!(matches!(compile("[a-c]+x", ""), Some(Matcher::Plain(..))));
        assert!(matches!(
            compile(r"^(?!\s*$).+", ""),
            Some(Matcher::Fancy { .. })
        ));
    }

    #[test]
    fn look_around_decides_acceptance_and_typing_can_still_be_refused() {
        assert_eq!(regex(r"^(?!\s*$).+", "", "a"), Acceptable);
        assert_eq!(regex(r"^(?!\s*$).+", "", "   "), Intermediate);
        assert_eq!(regex(r"^(?!\s*$).+", "", " x "), Acceptable);
        assert_eq!(regex(r"(?=.*[0-9])[a-z0-9]+", "", "ab"), Intermediate);
        assert_eq!(regex(r"(?=.*[0-9])[a-z0-9]+", "", "ab1"), Acceptable);
        assert_eq!(regex(r"(?=.*[0-9])[a-z0-9]+", "", "a!"), Invalid);
        assert_eq!(regex(r"[a-z]+(?<=c)", "", "abc"), Acceptable);
        assert_eq!(regex(r"[a-z]+(?<=c)", "", "ab"), Intermediate);
        assert_eq!(regex(r"[a-z]+(?<=c)", "", "ab1"), Invalid);
        assert_eq!(regex(r"(?=A)ab", "i", "AB"), Acceptable);
        assert_eq!(regex(r"\bab\b", "", "ab"), Acceptable);
    }

    #[test]
    fn a_back_reference_matches_what_its_group_did() {
        assert_eq!(regex(r"(a+)-\1", "", "aa-aa"), Acceptable);
        assert_eq!(regex(r"(a+)-\1", "", "aa-a"), Intermediate);
        assert_eq!(regex(r"(a+)-\1", "", "b"), Invalid);
    }

    #[test]
    fn a_pattern_neither_engine_takes_is_none() {
        assert!(compile(r"(a", "").is_none());
    }
}
