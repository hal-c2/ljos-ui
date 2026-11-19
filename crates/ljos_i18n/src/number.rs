//! Reading a number the way a locale writes it: `Number.fromLocaleString`.
//!
//! What is accepted is what qml6 accepts (Qt 6.12), case by case: the
//! locale's digits or ASCII ones, its decimal point only, its group
//! separator only in the whole part and only where the locale would put
//! one, a sign before the number, an exponent, and `inf` or `nan`.

use crate::locale::{Grouping, LocaleInfo};

/// The marks a right-to-left locale writes around its signs.
const BIDI_MARKS: [char; 3] = ['\u{061C}', '\u{200E}', '\u{200F}'];

/// What the locale writes a number with, without its direction marks.
struct Symbols {
    digits: [char; 10],
    decimal_point: String,
    group_separator: String,
    negative_sign: String,
    grouping: Option<Grouping>,
}

impl Symbols {
    fn of(locale: &LocaleInfo) -> Self {
        let unmarked = |s: String| s.chars().filter(|c| !BIDI_MARKS.contains(c)).collect();
        Self {
            digits: locale.digit_set(),
            decimal_point: unmarked(locale.decimal_point()),
            group_separator: unmarked(locale.group_separator()),
            negative_sign: unmarked(locale.negative_sign()),
            grouping: locale.grouping(),
        }
    }

    fn digit(&self, c: char) -> Option<char> {
        if c.is_ascii_digit() {
            return Some(c);
        }
        let i = self.digits.iter().position(|d| *d == c)?;
        char::from_digit(u32::try_from(i).ok()?, 10)
    }
}

/// The text not yet read.
struct Cursor<'t>(&'t str);

impl Cursor<'_> {
    fn eat(&mut self, s: &str) -> bool {
        match self.0.strip_prefix(s) {
            Some(rest) if !s.is_empty() => {
                self.0 = rest;
                true
            }
            _ => false,
        }
    }

    fn eat_char(&mut self, f: impl Fn(char) -> bool) -> Option<char> {
        let c = self.0.chars().next().filter(|c| f(*c))?;
        self.0 = &self.0[c.len_utf8()..];
        Some(c)
    }

    /// A sign: ASCII, U+2212 or the locale's. `Some(true)` is negative.
    fn sign(&mut self, symbols: &Symbols) -> Option<bool> {
        if self.eat(&symbols.negative_sign) || self.eat("-") || self.eat("\u{2212}") {
            Some(true)
        } else if self.eat("+") {
            Some(false)
        } else {
            None
        }
    }

    fn digits(&mut self, symbols: &Symbols, into: &mut String) -> usize {
        let mut n = 0;
        while let Some(c) = self.eat_char(|c| symbols.digit(c).is_some()) {
            into.extend(symbols.digit(c));
            n += 1;
        }
        n
    }
}

impl LocaleInfo {
    /// `Number.fromLocaleString(locale, text)`: the number `text` is as
    /// this locale writes numbers, `None` when it is none. Surrounding
    /// white space is ignored; an empty text is NaN, as in Qt.
    #[must_use]
    pub fn parse_number(&self, text: &str) -> Option<f64> {
        if text.is_empty() {
            return Some(f64::NAN);
        }
        let text = text.trim();
        // Qt skips a direction mark before anything, but not one that ends
        // the text.
        if text.is_empty() || text.ends_with(BIDI_MARKS) {
            return None;
        }
        let text: String = text.chars().filter(|c| !BIDI_MARKS.contains(c)).collect();
        let symbols = Symbols::of(self);
        let mut at = Cursor(&text);
        let negative = at.sign(&symbols);
        if at.0.eq_ignore_ascii_case("inf") {
            return Some(if negative == Some(true) {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            });
        }
        if negative.is_none() && at.0.eq_ignore_ascii_case("nan") {
            return Some(f64::NAN);
        }
        let mut ascii = String::from(if negative == Some(true) { "-" } else { "" });
        let whole = whole_part(&mut at, &symbols, &mut ascii)?;
        let mut fraction = 0;
        if at.eat(&symbols.decimal_point) {
            ascii.push('.');
            fraction = at.digits(&symbols, &mut ascii);
        }
        if whole + fraction == 0 {
            return None;
        }
        // Ljós writes every locale's exponent as `e` (crate::arg::printf),
        // so that is what it reads.
        if at.eat_char(|c| c == 'e' || c == 'E').is_some() {
            ascii.push('e');
            if at.sign(&symbols) == Some(true) {
                ascii.push('-');
            }
            if at.digits(&symbols, &mut ascii) == 0 {
                return None;
            }
        }
        if !at.0.is_empty() {
            return None;
        }
        let value: f64 = ascii.parse().ok()?;
        // Too large, or so small that it is lost: Qt reads neither.
        let lost = value == 0.0
            && ascii
                .split('e')
                .next()
                .is_some_and(|m| m.contains(['1', '2', '3', '4', '5', '6', '7', '8', '9']));
        (value.is_finite() && !lost).then_some(value)
    }
}

/// Reads the digits before the decimal point into `ascii`, with group
/// separators only where the locale puts them; their count, or `None` when
/// a separator is out of place.
fn whole_part(at: &mut Cursor<'_>, symbols: &Symbols, ascii: &mut String) -> Option<usize> {
    let separator = symbols.group_separator.as_str();
    // A locale that groups with a space-like separator is read with a
    // plain space too, as Qt reads it.
    let space = separator.chars().count() == 1 && separator.chars().all(char::is_whitespace);
    let mut runs = vec![at.digits(symbols, ascii)];
    while !separator.is_empty() && (at.eat(separator) || (space && at.eat(" "))) {
        runs.push(at.digits(symbols, ascii));
    }
    let digits = runs.iter().sum();
    if runs.len() > 1 {
        let g = symbols.grouping?;
        let (first, rest) = runs.split_first()?;
        let (last, middle) = rest.split_last()?;
        let fits = (1..=g.others).contains(first)
            && middle.iter().all(|r| *r == g.others)
            && *last == g.last
            && digits >= g.last + g.minimum;
        if !fits {
            return None;
        }
    }
    Some(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What qml6 (Qt 6.12) reads each text as; `None` where it throws.
    fn reads(locale: &str, cases: &[(&str, Option<f64>)]) {
        let info = LocaleInfo::new(locale);
        for (text, want) in cases {
            let got = info.parse_number(text);
            let same = match (got, want) {
                (Some(g), Some(w)) => g.to_bits() == w.to_bits() || (g.is_nan() && w.is_nan()),
                (None, None) => true,
                _ => false,
            };
            assert!(same, "{locale} {text:?}: got {got:?}, want {want:?}");
        }
    }

    #[test]
    fn a_german_number_is_grouped_by_dots() {
        reads(
            "de_DE",
            &[
                ("1.000", Some(1000.0)),
                ("1.234,56", Some(1234.56)),
                ("1234,56", Some(1234.56)),
                ("10.000,5", Some(10000.5)),
                ("1.000.000", Some(1_000_000.0)),
                ("-1.000", Some(-1000.0)),
                ("+5", Some(5.0)),
                (",5", Some(0.5)),
                ("5,", Some(5.0)),
                (" 12 ", Some(12.0)),
                ("1,5e3", Some(1500.0)),
                ("1,5E-3", Some(0.0015)),
                ("1.5", None),
                ("1.23", None),
                ("1.2345", None),
                ("12.34", None),
                ("1.2.3", None),
                ("1234.5", None),
                ("1\u{a0}000", None),
                ("1,5%", None),
                ("E5", None),
                ("abc", None),
                ("0x10", None),
            ],
        );
    }

    #[test]
    fn groups_go_only_where_the_locale_puts_them() {
        reads(
            "en_US",
            &[
                ("1,000.5", Some(1000.5)),
                ("0,123", Some(123.0)),
                ("00,000", Some(0.0)),
                ("1,000,000.5", Some(1_000_000.5)),
                ("1,000e2", Some(100_000.0)),
                ("1,00", None),
                ("1,0000", None),
                (",100", None),
                ("1,,000", None),
                ("1,000,", None),
                ("1,", None),
                (",", None),
                ("1234,567", None),
                ("12,34,567", None),
                ("1,234,5678", None),
                ("1e1,000", None),
                ("1,000.000,5", None),
                ("1.000,5", None),
                ("1 000", None),
                ("1_000", None),
            ],
        );
        reads(
            "hi_IN",
            &[
                ("1,00,000", Some(100_000.0)),
                ("10,00,000", Some(1_000_000.0)),
                ("1,23,45,678", Some(12_345_678.0)),
                ("1,000", Some(1000.0)),
                ("12,345", Some(12345.0)),
                ("00,000", Some(0.0)),
                ("100,000", None),
                ("123,45,678", None),
                ("1,00,0000", None),
            ],
        );
        reads(
            "es_ES",
            &[
                ("10.000", Some(10000.0)),
                ("1.000.000", Some(1_000_000.0)),
                ("01.000", Some(1000.0)),
                ("1000", Some(1000.0)),
                ("1.000", None),
                ("-1.000", None),
                ("1.000,5", None),
            ],
        );
        reads(
            "C",
            &[("1,000", Some(1000.0)), ("1.5", Some(1.5)), ("1,5", None)],
        );
    }

    #[test]
    fn a_space_like_separator_is_read_as_a_plain_space_too() {
        reads(
            "fr_FR",
            &[
                ("1\u{202f}234,5", Some(1234.5)),
                ("1 234,5", Some(1234.5)),
                ("1 234\u{202f}567,5", Some(1_234_567.5)),
                ("1\u{a0}234", None),
                ("1  000", None),
                ("1\t234", None),
            ],
        );
        reads(
            "ru_RU",
            &[("1 000", Some(1000.0)), ("1\u{a0}000", Some(1000.0))],
        );
        reads(
            "pl_PL",
            &[
                ("10 000", Some(10000.0)),
                ("10\u{a0}000", Some(10000.0)),
                ("10\u{202f}000", None),
                ("1 000", None),
            ],
        );
    }

    #[test]
    fn a_locale_reads_its_own_digits_and_ascii_ones() {
        reads(
            "ar_EG",
            &[
                ("١٢٣", Some(123.0)),
                ("123", Some(123.0)),
                ("1٢٣", Some(123.0)),
                ("١٫٥", Some(1.5)),
                ("١٬٠٠٠", Some(1000.0)),
                ("\u{61c}-١٢", Some(-12.0)),
                ("-12", Some(-12.0)),
                ("+5", Some(5.0)),
                ("\u{61c}+5", Some(5.0)),
                ("\u{61c}5", Some(5.0)),
                ("\u{61c}\u{61c}5", Some(5.0)),
                ("-\u{61c}5", Some(-5.0)),
                ("1\u{61c}2", Some(12.0)),
                ("12\u{61c}", None),
                ("1.5", None),
                ("1,000", None),
                ("inf", Some(f64::INFINITY)),
            ],
        );
        reads(
            "fa_IR",
            &[
                ("۱۲", Some(12.0)),
                ("۱٫۵", Some(1.5)),
                ("۱٬۰۰۰", Some(1000.0)),
                ("-۱", Some(-1.0)),
                ("\u{200e}\u{2212}۱", Some(-1.0)),
                ("١٢", None),
            ],
        );
        reads("en_US", &[("١٢", None), ("１２", None)]);
    }

    #[test]
    fn a_sign_and_an_exponent_are_read_as_qt_reads_them() {
        reads(
            "sv_SE",
            &[
                ("\u{2212}5", Some(-5.0)),
                ("-5", Some(-5.0)),
                ("+5", Some(5.0)),
                ("1 000,5", Some(1000.5)),
                ("1.5", None),
            ],
        );
        reads(
            "en_US",
            &[
                ("\u{2212}5", Some(-5.0)),
                ("\u{200e}5", Some(5.0)),
                ("\u{200e}-5", Some(-5.0)),
                ("5\u{200e}", None),
                ("-0", Some(-0.0)),
                ("-0.0", Some(-0.0)),
                ("-0e5", Some(-0.0)),
                ("007", Some(7.0)),
                ("1.", Some(1.0)),
                (".5", Some(0.5)),
                ("-.5", Some(-0.5)),
                ("5.e5", Some(500_000.0)),
                (".5e5", Some(50000.0)),
                ("1E5", Some(100_000.0)),
                ("1e05", Some(100_000.0)),
                ("1e+2", Some(100.0)),
                ("1e-0", Some(1.0)),
                ("1e\u{2212}2", Some(0.01)),
                ("5e-1", Some(0.5)),
                ("\t5\n", Some(5.0)),
                ("\n\r 1,000 \u{a0}", Some(1000.0)),
                ("\u{3000}5", Some(5.0)),
                ("", Some(f64::NAN)),
                ("-", None),
                (".", None),
                ("e5", None),
                (".e5", None),
                ("1e", None),
                ("1ee5", None),
                ("1e5.5", None),
                ("1e+-2", None),
                ("1e--2", None),
                ("1.5.", None),
                ("++5", None),
                ("+-5", None),
                ("+\u{2212}5", None),
                ("- 5", None),
                ("1 e5", None),
                ("1e 5", None),
                ("1 .5", None),
                ("1,000 x", None),
                ("   ", None),
                ("\u{feff}5", None),
            ],
        );
    }

    #[test]
    fn infinities_and_nan_are_words_and_out_of_range_is_no_number() {
        reads(
            "en_US",
            &[
                ("inf", Some(f64::INFINITY)),
                ("INF", Some(f64::INFINITY)),
                ("inF", Some(f64::INFINITY)),
                ("+inf", Some(f64::INFINITY)),
                ("-inf", Some(f64::NEG_INFINITY)),
                ("nan", Some(f64::NAN)),
                ("NaN", Some(f64::NAN)),
                ("Infinity", None),
                ("infinity", None),
                ("-nan", None),
                ("+nan", None),
                ("nan5", None),
                ("-in f", None),
                ("1e400", None),
                ("1.8e308", None),
                ("1e1000000000000", None),
                ("1e-400", None),
                ("2e-324", None),
                ("4e-324", Some(5e-324)),
                ("1e-320", Some(1e-320)),
                ("0e-400", Some(0.0)),
                ("0e400", Some(0.0)),
                ("1.7976931348623157e308", Some(f64::MAX)),
                ("9007199254740993", Some(9_007_199_254_740_992.0)),
                ("0.30000000000000004", Some(0.300_000_000_000_000_04)),
            ],
        );
    }

    #[test]
    fn what_a_locale_writes_it_reads_back() {
        for name in [
            "en_US", "de_DE", "fr_FR", "hi_IN", "ar_EG", "fa_IR", "sv_SE", "es_ES", "C",
        ] {
            let info = LocaleInfo::new(name);
            for value in [-1_234_567.25, 0.5, 1000.0, 12.0, 1.5e-7] {
                for format in ['f', 'e', 'g'] {
                    let text = info.format_double(value, format, 6);
                    // The text keeps only `precision` digits (`1.5e-7` is
                    // "0.000000" in 'f'), so what it reads as must write the
                    // same text again, not equal the value it came from.
                    let read = info.parse_number(&text);
                    let again = read.map(|r| info.format_double(r, format, 6));
                    assert_eq!(
                        again.as_deref(),
                        Some(text.as_str()),
                        "{name} {text:?} read as {read:?}, written from {value}"
                    );
                }
            }
        }
    }
}
