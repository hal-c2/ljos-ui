//! `String.prototype.arg`, with the semantics of Qt's `QString::arg`
//! (<https://doc.qt.io/qt-6/qml-string.html>,
//! <https://doc.qt.io/qt-6/qstring.html#arg>).
//!
//! A place marker is `%` and one or two digits, with an optional `L` before
//! the digits asking for the localised form of a number. One call replaces
//! every marker carrying the lowest number in the string, so
//! `"%2 of %1".arg(a).arg(b)` puts `a` at `%1` whatever the order.

use std::fmt::Write as _;

use crate::LocaleInfo;

/// One place marker: the bytes it covers, its number and whether it asks
/// for the localised form.
#[derive(Debug, Clone, Copy)]
struct Marker {
    start: usize,
    end: usize,
    number: u8,
    localized: bool,
}

fn markers(template: &str) -> Vec<Marker> {
    let bytes = template.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        let start = i;
        let mut at = i + 1;
        let localized = bytes.get(at) == Some(&b'L');
        if localized {
            at += 1;
        }
        let Some(first) = bytes.get(at).filter(|b| b.is_ascii_digit()) else {
            i += 1;
            continue;
        };
        let mut number = first - b'0';
        at += 1;
        // At most two digits: `%100` is marker 10 followed by a zero.
        if let Some(second) = bytes.get(at).filter(|b| b.is_ascii_digit()) {
            number = number * 10 + (second - b'0');
            at += 1;
        }
        out.push(Marker {
            start,
            end: at,
            number,
            localized,
        });
        i = at;
    }
    out
}

/// Replaces the markers with the lowest number: `%N` by `plain`, `%LN` by
/// `localized`. `None` when the string has no marker left, which Qt reports
/// as a warning and answers with the string unchanged.
#[must_use]
pub fn arg(template: &str, plain: &str, localized: &str) -> Option<String> {
    let markers = markers(template);
    let lowest = markers.iter().map(|m| m.number).min()?;
    let mut out = String::with_capacity(template.len() + plain.len());
    let mut copied = 0;
    for m in markers.iter().filter(|m| m.number == lowest) {
        out.push_str(&template[copied..m.start]);
        out.push_str(if m.localized { localized } else { plain });
        copied = m.end;
    }
    out.push_str(&template[copied..]);
    Some(out)
}

/// How `arg` writes a number that is not localised: whole numbers that fit
/// 32 bits in full, everything else to six significant digits, switching to
/// an exponent when that is shorter (C's `%g`).
#[must_use]
pub fn number(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    if value.fract() == 0.0 && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value) {
        // Adding zero turns the negative zero into the one that prints as "0".
        return format!("{:.0}", value + 0.0);
    }
    general(value, 6)
}

/// C's `%g` with `digits` significant digits.
fn general(value: f64, digits: usize) -> String {
    // Rust's `{:e}` rounds to the wanted digits and names the exponent that
    // results, which is the exponent `%g` chooses its notation by.
    let scientific = format!("{value:.*e}", digits - 1);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let trim = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    let wanted = i32::try_from(digits).unwrap_or(i32::MAX);
    if exponent < -4 || exponent >= wanted {
        let mut out = trim(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(out, "e{sign}{:02}", exponent.unsigned_abs());
        out
    } else {
        let decimals = usize::try_from(wanted - 1 - exponent).unwrap_or(0);
        trim(&format!("{value:.decimals$}"))
    }
}

/// The two spellings `arg` needs for a number: plain, and in `locale`'s
/// digits and separators for `%L1`.
#[must_use]
pub fn number_pair(value: f64, locale: &LocaleInfo) -> (String, String) {
    let plain = number(value);
    let localized = locale.localize_number(&plain);
    (plain, localized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lowest_marker_is_replaced_wherever_it_stands() {
        assert_eq!(arg("%2 of %1", "a", "a").as_deref(), Some("%2 of a"));
        assert_eq!(arg("%3 %7 %3", "a", "a").as_deref(), Some("a %7 a"));
        assert_eq!(arg("%0 %1", "a", "a").as_deref(), Some("a %1"));
    }

    #[test]
    fn a_marker_has_at_most_two_digits() {
        assert_eq!(arg("%100", "a", "a").as_deref(), Some("a0"));
        assert_eq!(arg("%123", "a", "a").as_deref(), Some("a3"));
        assert_eq!(arg("%01 %1", "a", "a").as_deref(), Some("a a"));
    }

    #[test]
    fn a_string_without_a_marker_has_nothing_to_replace() {
        assert_eq!(arg("none", "x", "x"), None);
        assert_eq!(arg("%l1 100%% %", "x", "x"), None);
    }

    #[test]
    fn the_replacement_is_not_scanned_again_in_the_same_call() {
        let once = arg("%1%2", "%2", "%2").unwrap();
        assert_eq!(once, "%2%2");
        assert_eq!(arg(&once, "b", "b").as_deref(), Some("bb"));
    }

    #[test]
    fn an_l_asks_for_the_localized_spelling() {
        assert_eq!(
            arg("%L1 %1", "1234.5", "1,234.5").as_deref(),
            Some("1,234.5 1234.5")
        );
    }

    #[test]
    fn numbers_are_written_as_qt_writes_them() {
        let cases: &[(f64, &str)] = &[
            (5.0, "5"),
            (-5000.0, "-5000"),
            (0.0, "0"),
            (1234.5, "1234.5"),
            (0.1 + 0.2, "0.3"),
            (1e10, "1e+10"),
            (2_147_483_647.0, "2147483647"),
            (2_147_483_648.0, "2.14748e+09"),
            (-1_234_567.891, "-1.23457e+06"),
            (0.000_012_34, "1.234e-05"),
            (0.000_123_4, "0.0001234"),
            (999_999.5, "1e+06"),
            (123_456.7, "123457"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
        ];
        for (value, text) in cases {
            assert_eq!(number(*value), *text, "{value}");
        }
    }

    #[test]
    fn localized_numbers_use_the_locale() {
        let en = LocaleInfo::new("en_US");
        assert_eq!(
            number_pair(1_234_567.0, &en),
            ("1234567".to_owned(), "1,234,567".to_owned())
        );
        assert_eq!(number_pair(-5000.0, &en).1, "-5,000");
        assert_eq!(number_pair(-1_234_567.891, &en).1, "-1.23457e+06");
        let is = LocaleInfo::new("is_IS");
        assert_eq!(number_pair(1234.5, &is).1, "1.234,5");
    }
}
