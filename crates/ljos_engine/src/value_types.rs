//! The vector value types, and what the value types share: how their
//! numbers print and how a string is read as one.
//!
//! A value type prints as Qt prints it (`QPointF(1, 2)`), and Qt writes each
//! number the way C's `%g` does: six significant digits, an exponent where
//! the number is very large or very small. The same text is made in JS by
//! the faces in [`crate::value_faces`]; the two must agree, because whether
//! a value turns into text in a script or in a `string` property is not
//! something a QML author sees.

use crate::value::Font;

/// `vector2d` value type. Single precision, as in Qt.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector2d {
    pub x: f32,
    pub y: f32,
}

/// `vector3d` value type.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector3d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// `vector4d` value type.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vector4d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// `quaternion` value type. Its default is the identity rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub scalar: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quaternion {
    fn default() -> Self {
        Quaternion {
            scalar: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

/// How many digits [`general`] keeps.
const DIGITS: usize = 6;

/// A number as C's `%g` writes it, with a half rounded away from zero.
///
/// Rust rounds a half to even when it formats, and JS (`toFixed`) rounds it
/// up, which is also what Qt shows (`100000.5` is `100001`). So the digits
/// are taken exactly and rounded here.
pub fn general(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.into();
    }
    if v == 0.0 {
        return "0".into();
    }
    // Thirty places are far more than the one digit past the sixth that
    // decides the rounding.
    let exact = format!("{:.30e}", v.abs());
    let (mantissa, exp) = exact.split_once('e').unwrap_or((&exact, "0"));
    let mut exp: i32 = exp.parse().unwrap_or(0);
    let all: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let mut digits: Vec<u8> = all[..DIGITS].to_vec();
    if all[DIGITS] >= 5 {
        let mut at = DIGITS;
        loop {
            if at == 0 {
                // 9.999995 became 10.0000: one digit more to the left.
                digits.insert(0, 1);
                digits.pop();
                exp += 1;
                break;
            }
            at -= 1;
            if digits[at] == 9 {
                digits[at] = 0;
            } else {
                digits[at] += 1;
                break;
            }
        }
    }
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    let text = |d: &[u8]| -> String { d.iter().map(|d| char::from(b'0' + d)).collect() };
    let mut out = String::with_capacity(16);
    if v < 0.0 {
        out.push('-');
    }
    if exp < -4 || exp >= DIGITS as i32 {
        out.push_str(&text(&digits[..1]));
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&text(&digits[1..]));
        }
        out.push('e');
        out.push(if exp < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exp.unsigned_abs()));
    } else if exp < 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-exp - 1) as usize));
        out.push_str(&text(&digits));
    } else {
        let whole = exp as usize + 1;
        if digits.len() <= whole {
            out.push_str(&text(&digits));
            out.extend(std::iter::repeat_n('0', whole - digits.len()));
        } else {
            out.push_str(&text(&digits[..whole]));
            out.push('.');
            out.push_str(&text(&digits[whole..]));
        }
    }
    out
}

/// `Name(a, b, c)` with each number as [`general`] writes it.
pub(crate) fn named(name: &str, numbers: &[f64]) -> String {
    let mut out = String::with_capacity(name.len() + 8 * numbers.len());
    out.push_str(name);
    out.push('(');
    for (i, n) in numbers.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&general(*n));
    }
    out.push(')');
    out
}

/// A font as Qt writes one: the family and then every setting, most of
/// which a QML font cannot change and are written as Qt has them by
/// default.
pub(crate) fn font_text(f: &Font) -> String {
    let flag = |b: bool| u8::from(b);
    format!(
        "QFont({},{},{},5,{},{},{},{},0,0,{},{},{},{},0,1,,0,0)",
        f.family,
        crate::number::number_to_string(f.point_size),
        f.pixel_size,
        f.weight,
        flag(f.italic),
        flag(f.underline),
        flag(f.strikeout),
        f.capitalization,
        flag(f.letter_spacing != 0.0),
        crate::number::number_to_string(f.letter_spacing),
        crate::number::number_to_string(f.word_spacing),
    )
}

/// `"1, 2, 3"` as that many numbers: what a vector or a point is when it is
/// written as a string.
pub(crate) fn list<const N: usize>(s: &str) -> Option<[f64; N]> {
    let mut out = [0.0; N];
    let mut parts = s.split(',');
    for slot in &mut out {
        *slot = parts.next()?.trim().parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// `"3x4"`: a size written as a string.
pub(crate) fn extent(s: &str) -> Option<[f64; 2]> {
    let (w, h) = s.split_once('x')?;
    Some([w.trim().parse().ok()?, h.trim().parse().ok()?])
}

/// `"1,2,3x4"`: a rect written as a string.
pub(crate) fn area(s: &str) -> Option<[f64; 4]> {
    let mut parts = s.splitn(3, ',');
    let x = parts.next()?.trim().parse().ok()?;
    let y = parts.next()?.trim().parse().ok()?;
    let [w, h] = extent(parts.next()?)?;
    Some([x, y, w, h])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_percent_g() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-2.0, "-2"),
            (1.5, "1.5"),
            (-0.25, "-0.25"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.333333"),
            (2.0 / 3.0, "0.666667"),
            (1_234_567.891, "1.23457e+06"),
            (0.000_001_234, "1.234e-06"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e-7, "1e-07"),
            (1e6, "1e+06"),
            (999_999.0, "999999"),
            (999_999.5, "1e+06"),
            (100_000.5, "100001"),
            (123_456.0, "123456"),
            (12_345.678, "12345.7"),
            (1e21, "1e+21"),
            (1e100, "1e+100"),
            (9.999_995, "10"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::from(0.1_f32), "0.1"),
            (f64::from(12_345_678.0_f32), "1.23457e+07"),
        ];
        for (v, text) in cases {
            assert_eq!(general(*v), *text, "{v:?}");
        }
    }

    #[test]
    fn strings_are_read_as_values() {
        assert_eq!(list::<2>("7,8"), Some([7.0, 8.0]));
        assert_eq!(list::<2>(" 1.5 , -2 "), Some([1.5, -2.0]));
        assert_eq!(list::<3>("1,2"), None);
        assert_eq!(list::<2>("1,2,3"), None);
        assert_eq!(list::<2>("nope"), None);
        assert_eq!(extent("7x8"), Some([7.0, 8.0]));
        assert_eq!(extent("7,8"), None);
        assert_eq!(area("1,2,3x4"), Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(area("1, 2, 3 x 4"), Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(area("1,2"), None);
    }

    #[test]
    fn a_font_prints_every_setting() {
        let mut f = Font {
            family: "Arial".into(),
            pixel_size: 12,
            ..Font::default()
        };
        assert_eq!(
            font_text(&f),
            "QFont(Arial,-1,12,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0)"
        );
        f.pixel_size = -1;
        f.point_size = 10.0;
        f.weight = 700;
        f.italic = true;
        f.underline = true;
        f.strikeout = true;
        f.letter_spacing = 2.0;
        f.word_spacing = 3.0;
        assert_eq!(
            font_text(&f),
            "QFont(Arial,10,-1,5,700,1,1,1,0,0,0,1,2,3,0,1,,0,0)"
        );
    }
}
