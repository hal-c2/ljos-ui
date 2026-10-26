//! ECMAScript number conversions (ECMA-262 §7.1.4, §7.1.6, §7.1.4.1.1 and
//! Number::toString, §6.1.6.1.20). Tier-1 bindings and the JS host must
//! format and parse numbers exactly as QuickJS does, or the debug
//! cross-check between tiers fails.

/// `Number::toString(x)` in radix 10.
///
/// Rust's `{:e}` already yields the shortest digit string that round-trips
/// (the same digits ECMAScript asks for, but for a tie: see
/// [`even_on_a_tie`]); only the layout differs, so we take the digits and
/// exponent from it and lay them out per the spec.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x == 0.0 {
        // Both +0 and -0.
        return "0".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if x < 0.0 {
        return format!("-{}", number_to_string(-x));
    }
    let sci = format!("{x:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digits = even_on_a_tie(x, digits);
    let e: i32 = exp.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = e + 1;
    let mut out = String::with_capacity(24);
    if k <= n && n <= 21 {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-n) as usize));
        out.push_str(&digits);
    } else {
        let e = n - 1;
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if e >= 0 { '+' } else { '-' });
        out.push_str(&e.unsigned_abs().to_string());
    }
    out
}

/// The shortest digits of `x`, the even of the two when `x` is exactly
/// halfway between two that are as short: ECMAScript asks for the one
/// nearest `x` and then the even one, where Rust's rounds the tie up
/// (0.50000762939453125 is 0.5000076293945312, not …313).
fn even_on_a_tie(x: f64, digits: String) -> String {
    let k = digits.len();
    // One digit more, rounded: halfway ends in 5 there, which most never do.
    // It is looked at on the stack first, so that every other number, an
    // index joined into a delegate's text among them, allocates nothing here.
    let mut buf = [0u8; 40];
    let mut w = &mut buf[..];
    if std::io::Write::write_fmt(&mut w, format_args!("{x:.k$e}")).is_err() {
        return digits;
    }
    let left = w.len();
    let len = buf.len() - left;
    let e_at = buf[..len].iter().position(|&b| b == b'e').unwrap_or(len);
    if e_at == 0 || buf[e_at - 1] != b'5' {
        return digits;
    }
    let longer = format!("{x:.k$e}");
    let longer: String = longer
        .split_once('e')
        .map_or(longer.as_str(), |(m, _)| m)
        .chars()
        .filter(|c| *c != '.')
        .collect();
    let (base, last) = longer.split_at(k);
    if last != "5" || base.ends_with('9') || exact_digits(x) != longer {
        return digits;
    }
    let lower = base.as_bytes()[k - 1] - b'0';
    if lower.is_multiple_of(2) {
        return base.to_owned();
    }
    let mut upper = base.to_owned();
    upper.pop();
    upper.push(char::from(b'0' + lower + 1));
    upper
}

/// Every significant digit of `x`'s exact value, which has as many places
/// after the point as its lowest set bit is below the units.
fn exact_digits(x: f64) -> String {
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exp) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, biased - 1075)
    };
    let places = (-(exp + mantissa.trailing_zeros() as i32)).max(0) as usize;
    let fixed = format!("{x:.places$}");
    fixed
        .chars()
        .filter(|c| *c != '.')
        .collect::<String>()
        .trim_matches('0')
        .to_owned()
}

/// ECMAScript `StrWhiteSpaceChar`: WhiteSpace plus LineTerminator. Not
/// Rust's `char::is_whitespace`, which includes U+0085 and excludes U+FEFF.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `StringToNumber` (ECMA-262 §7.1.4.1.1). Empty or blank strings are 0;
/// anything that is not a complete `StringNumericLiteral` is NaN.
pub fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches(is_js_whitespace);
    if t.is_empty() {
        return 0.0;
    }
    let b = t.as_bytes();
    if b.len() > 2 && b[0] == b'0' {
        let radix = match b[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if radix != 0 {
            return parse_radix(&t[2..], radix);
        }
    }
    let (sign, body) = match b[0] {
        b'+' => (1.0, &t[1..]),
        b'-' => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    if !is_decimal_literal(body.as_bytes()) {
        return f64::NAN;
    }
    // The grammar is checked above; Rust's parser accepts the same strings
    // and rounds correctly.
    body.parse::<f64>().map_or(f64::NAN, |v| sign * v)
}

/// `StrUnsignedDecimalLiteral` without `Infinity`: digits with an optional
/// fraction (either side of the dot may be empty, not both) and an optional
/// signed exponent. No numeric separators.
fn is_decimal_literal(b: &[u8]) -> bool {
    let mut i = 0;
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac_start;
    }
    if digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == b.len()
}

fn parse_radix(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    // u128 keeps every literal up to 128 bits exact before the single
    // rounding to f64; longer ones accumulate in f64 (rare, slightly off
    // from the spec's single rounding).
    let mut exact: u128 = 0;
    let mut approx: Option<f64> = None;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else {
            return f64::NAN;
        };
        match approx.as_mut() {
            Some(a) => *a = *a * f64::from(radix) + f64::from(d),
            None => match exact
                .checked_mul(u128::from(radix))
                .and_then(|v| v.checked_add(u128::from(d)))
            {
                Some(v) => exact = v,
                None => approx = Some(exact as f64 * f64::from(radix) + f64::from(d)),
            },
        }
    }
    approx.unwrap_or(exact as f64)
}

/// `ToInt32` (ECMA-262 §7.1.6): truncate, then wrap modulo 2^32. NaN and
/// infinities are 0. QML `int` properties store this.
pub fn to_int32(x: f64) -> i32 {
    // Nearly every number a script gives an int is one already: the
    // remainder below costs a call of `fmod`.
    #[allow(clippy::float_cmp)]
    if x == f64::from(x as i32) {
        return x as i32;
    }
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    let m = t.rem_euclid(4_294_967_296.0);
    (m as u64 as u32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_ecmascript() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1, "0.1"),
            (0.1 + 0.2, "0.30000000000000004"),
            (100.0, "100"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (2.5e25, "2.5e+25"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::MAX, "1.7976931348623157e+308"),
            (5e-324, "5e-324"),
            (12.5, "12.5"),
            // 0.50000762939453125 exactly: halfway between …312 and …313.
            (0.5 + 2f64.powi(-17), "0.5000076293945312"),
            (-0.5 - 2f64.powi(-17), "-0.5000076293945312"),
            (9007199254740993.0, "9007199254740992"),
        ];
        for (x, want) in cases {
            assert_eq!(number_to_string(*x), *want, "{x:?}");
        }
    }

    #[test]
    fn parses_like_ecmascript() {
        let cases: &[(&str, f64)] = &[
            ("", 0.0),
            ("   ", 0.0),
            ("\u{FEFF}\n 42 \t", 42.0),
            ("0", 0.0),
            ("-0", -0.0),
            ("+1.5", 1.5),
            (".5", 0.5),
            ("5.", 5.0),
            ("1e3", 1000.0),
            ("1E-2", 0.01),
            ("0x1F", 31.0),
            ("0X1f", 31.0),
            ("0b101", 5.0),
            ("0o17", 15.0),
            ("Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("+Infinity", f64::INFINITY),
            ("007", 7.0),
        ];
        for (s, want) in cases {
            let got = string_to_number(s);
            assert!(
                got == *want && got.is_sign_negative() == want.is_sign_negative(),
                "{s:?}: {got:?}"
            );
        }
        for s in [
            "abc",
            "1a",
            "0x",
            "-0x10",
            "1e",
            "e5",
            ".",
            "+",
            "inf",
            "infinity",
            "NaN",
            "1_000",
            "1 2",
            "\u{0085}1",
            "0x1g",
        ] {
            assert!(string_to_number(s).is_nan(), "{s:?}");
        }
    }

    #[test]
    fn int32_wraps() {
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(-0.0), 0);
        assert_eq!(to_int32(5.9), 5);
        assert_eq!(to_int32(-5.9), -5);
        assert_eq!(to_int32(2_147_483_648.0), i32::MIN);
        assert_eq!(to_int32(4_294_967_297.0), 1);
        assert_eq!(to_int32(-1.0), -1);
    }
}
