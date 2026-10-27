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
    // The exponent of the number rounded to the wanted digits is the one
    // `%g` chooses its notation by.
    let (mantissa, exponent) = scientific(value, digits - 1);
    let trim = |mut s: String| -> String {
        if s.contains('.') {
            let kept = s.trim_end_matches('0').trim_end_matches('.').len();
            s.truncate(kept);
        }
        s
    };
    let wanted = i32::try_from(digits).unwrap_or(i32::MAX);
    if exponent < -4 || exponent >= wanted {
        let mut out = trim(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(out, "e{sign}{:02}", exponent.unsigned_abs());
        out
    } else {
        let decimals = usize::try_from(wanted - 1 - exponent).unwrap_or(0);
        trim(fixed(value, decimals))
    }
}

/// Digits past the ones kept, enough that a tie is seen as one: Rust
/// writes a float's exact decimal expansion, rounded only this far out.
const GUARD: usize = 30;

/// `value` with `decimals` digits after the point, a tie rounded away from
/// zero as Qt rounds it (`1234.5` to no decimals is `1235`), where Rust
/// rounds a tie to even.
fn fixed(value: f64, decimals: usize) -> String {
    let exact = format!("{:.*}", decimals + GUARD, value.abs());
    let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits: Vec<u8> = whole
        .bytes()
        .chain(fraction.bytes().take(decimals))
        .collect();
    let up = fraction
        .as_bytes()
        .get(decimals)
        .is_some_and(|d| *d >= b'5');
    if up && carry(&mut digits) {
        digits.insert(0, b'1');
    }
    let point = digits.len() - decimals;
    let mut out = String::new();
    if value.is_sign_negative() && digits.iter().any(|d| *d != b'0') {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&digits[..point]).unwrap_or("0"));
    if decimals > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&digits[point..]).unwrap_or(""));
    }
    out
}

/// `value` as a mantissa with `decimals` digits after the point and its
/// exponent, a tie rounded away from zero.
fn scientific(value: f64, decimals: usize) -> (String, i32) {
    let exact = format!("{:.*e}", decimals + GUARD, value.abs());
    let (mantissa, exponent) = exact.split_once('e').unwrap_or((&exact, "0"));
    let mut exponent: i32 = exponent.parse().unwrap_or(0);
    let all: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut digits = all[..=decimals.min(all.len() - 1)].to_vec();
    let up = all.get(decimals + 1).is_some_and(|d| *d >= b'5');
    if up && carry(&mut digits) {
        // 9.99 rounded up is 10.0, written 1.00 with the next exponent.
        digits.insert(0, b'1');
        digits.pop();
        exponent += 1;
    }
    let mut out = String::new();
    if value.is_sign_negative() && value != 0.0 {
        out.push('-');
    }
    out.push(char::from(digits[0]));
    if decimals > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&digits[1..]).unwrap_or(""));
    }
    (out, exponent)
}

/// Adds one to the last of `digits`; true when it carried out of the first.
fn carry(digits: &mut [u8]) -> bool {
    for d in digits.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            return false;
        }
    }
    true
}

/// C's `printf` with the conversion `format` (`f`, `e`, `E`, `g` or `G`;
/// anything else is `g`) and `precision`, as `Number.toLocaleString`
/// takes them: `nan` and `inf` as they are, the exponent with a sign and at
/// least two digits.
#[must_use]
pub fn printf(value: f64, format: char, precision: usize) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    let value = value + 0.0;
    match format.to_ascii_lowercase() {
        'f' => fixed(value, precision),
        'e' => {
            let (mantissa, exponent) = scientific(value, precision);
            let sign = if exponent < 0 { '-' } else { '+' };
            format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
        }
        _ => general(value, precision.max(1)),
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
    fn printf_conversions() {
        assert_eq!(printf(1234.5, 'f', 2), "1234.50");
        assert_eq!(printf(1234.5, 'f', 0), "1235");
        assert_eq!(printf(-0.5, 'f', 0), "-1");
        assert_eq!(printf(9.996, 'e', 2), "1.00e+01");
        assert_eq!(printf(999_999.5, 'g', 6), "1e+06");
        assert_eq!(printf(1234.5, 'e', 2), "1.23e+03");
        assert_eq!(printf(1234.567, 'g', 2), "1.2e+03");
        assert_eq!(printf(1.5, 'g', 2), "1.5");
        assert_eq!(printf(0.000_12, 'e', 1), "1.2e-04");
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
