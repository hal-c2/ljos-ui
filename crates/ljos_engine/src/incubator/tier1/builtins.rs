//! The builtins of tier 1: each gives what the JavaScript function of the
//! same name gives, or nothing.
//!
//! "Nothing" (`None`) is for every call the JS function would answer in a
//! way this does not reproduce to the bit: an argument that is not a
//! primitive, a count of arguments it throws for, a message it logs. The
//! VM then runs the whole expression as JavaScript.
//!
//! These assume the globals are the ones the engine installed. A script
//! that replaces `Math.max` or `qsTr` is not followed (the cross-check
//! would say so).

use ljos_ir::Builtin;

use super::ops::{self, Prim, prim};
use crate::atom::Atom;
use crate::engine::Engine;
use crate::value::{Color, Value};
use crate::{number_to_string, to_int32};

/// Whether `v` is a receiver the method `f` is the builtin of: JavaScript
/// finds the method before it evaluates the arguments, and for anything
/// else finds another one or none.
pub(super) fn check(f: Builtin, v: &Value) -> bool {
    match f {
        Builtin::Arg | Builtin::ToUpperCase | Builtin::ToLowerCase | Builtin::Trim => {
            matches!(v, Value::String(_))
        }
        Builtin::ToFixed => matches!(v, Value::Real(_)),
        Builtin::ToString => matches!(v, Value::Real(_) | Value::String(_) | Value::Bool(_)),
        _ => false,
    }
}

pub(super) fn call(e: &Engine, f: Builtin, args: &[Value]) -> Option<Value> {
    // An argument that was not passed is `undefined`.
    let num = |i: usize| match args.get(i) {
        Some(v) => ops::number(v),
        None => Some(f64::NAN),
    };
    let real = |x: f64| Some(Value::Real(x));
    match f {
        Builtin::MathAbs => real(num(0)?.abs()),
        Builtin::MathAcos => real(num(0)?.acos()),
        Builtin::MathAsin => real(num(0)?.asin()),
        Builtin::MathAtan => real(num(0)?.atan()),
        Builtin::MathAtan2 => {
            let y = num(0)?;
            real(y.atan2(num(1)?))
        }
        Builtin::MathCbrt => real(num(0)?.cbrt()),
        Builtin::MathCeil => real(num(0)?.ceil()),
        Builtin::MathCos => real(num(0)?.cos()),
        Builtin::MathExp => real(num(0)?.exp()),
        Builtin::MathFloor => real(num(0)?.floor()),
        Builtin::MathLog => real(num(0)?.ln()),
        Builtin::MathLog10 => real(num(0)?.log10()),
        Builtin::MathLog2 => real(num(0)?.log2()),
        Builtin::MathMax => extreme(args, f64::NEG_INFINITY, |a, b| {
            if a == 0.0 && b == 0.0 {
                // Of the two zeros the positive one is the greater.
                if a.is_sign_negative() { b } else { a }
            } else if a > b {
                a
            } else {
                b
            }
        }),
        Builtin::MathMin => extreme(args, f64::INFINITY, |a, b| {
            if a == 0.0 && b == 0.0 {
                if a.is_sign_negative() { a } else { b }
            } else if a < b {
                a
            } else {
                b
            }
        }),
        Builtin::MathPow => {
            let base = num(0)?;
            real(ops::pow(base, num(1)?))
        }
        Builtin::MathRound => real(round(num(0)?)),
        Builtin::MathSign => {
            let x = num(0)?;
            real(if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                // A zero of either sign, or NaN: itself.
                x
            })
        }
        Builtin::MathSin => real(num(0)?.sin()),
        Builtin::MathSqrt => real(num(0)?.sqrt()),
        Builtin::MathTan => real(num(0)?.tan()),
        Builtin::MathTrunc => real(num(0)?.trunc()),
        Builtin::Number => match args.first() {
            None => real(0.0),
            Some(v) => real(ops::number(v)?),
        },
        Builtin::String => match args.first() {
            None => Some(Value::String(Atom::EMPTY)),
            Some(Value::String(s)) => Some(Value::String(s.clone())),
            Some(v) => Some(ops::string(prim(v)?.text().into_owned())),
        },
        Builtin::Boolean => Some(Value::Bool(args.first().is_some_and(Value::to_boolean))),
        Builtin::IsNaN => Some(Value::Bool(num(0)?.is_nan())),
        Builtin::IsFinite => Some(Value::Bool(num(0)?.is_finite())),
        Builtin::QtRgba => {
            let [r, g, b, a] = channels(args)?;
            Some(color(to16(r), to16(g), to16(b), to16(a)))
        }
        Builtin::QtHsla => {
            let [h, s, l, a] = channels(args)?;
            Some(hsla(h, s, l, a))
        }
        Builtin::QtHsva => {
            let [h, s, v, a] = channels(args)?;
            Some(hsva(h, s, v, a))
        }
        Builtin::QtPoint => {
            let [x, y] = doubles(args)?;
            Some(Value::point(x, y))
        }
        Builtin::QtSize => {
            let [width, height] = doubles(args)?;
            Some(Value::size(width, height))
        }
        Builtin::QtRect => {
            let [x, y, width, height] = doubles(args)?;
            Some(Value::rect(x, y, width, height))
        }
        Builtin::QtVector2d => {
            let [x, y] = singles(args)?;
            Some(Value::vector2d(x, y))
        }
        Builtin::QtVector3d => {
            let [x, y, z] = singles(args)?;
            Some(Value::vector3d(x, y, z))
        }
        Builtin::QtVector4d => {
            let [x, y, z, w] = singles(args)?;
            Some(Value::vector4d(x, y, z, w))
        }
        Builtin::QtQuaternion => {
            let [scalar, x, y, z] = singles(args)?;
            Some(Value::quaternion(scalar, x, y, z))
        }
        Builtin::QtLighter => shade(args, 1.5, Color::lighter),
        Builtin::QtDarker => shade(args, 2.0, Color::darker),
        Builtin::QsTr => {
            // The first register is the file's translation context.
            let (context, args) = args.split_first()?;
            let context = text(context)?;
            let source = text(args.first()?)?;
            let comment = match args.get(1) {
                Some(v) => text(v)?,
                None => "",
            };
            let n = match args.get(2) {
                Some(Value::Real(n)) => count(*n),
                Some(_) => return None,
                None => None,
            };
            Some(ops::string(e.translate(context, source, comment, n)))
        }
        Builtin::QsTranslate => {
            let context = text(args.first()?)?;
            let source = text(args.get(1)?)?;
            let comment = match args.get(2) {
                Some(v) => text(v)?,
                None => "",
            };
            // Unlike `qsTr`, a count that is no number is no count.
            let n = match args.get(3) {
                Some(Value::Real(n)) => count(*n),
                _ => None,
            };
            Some(ops::string(e.translate(context, source, comment, n)))
        }
        Builtin::QsTrId => {
            let id = text(args.first()?)?;
            let n = match args.get(1) {
                Some(Value::Real(n)) => count(*n),
                Some(_) => return None,
                None => None,
            };
            Some(ops::string(e.translate_id(id, n)))
        }
        Builtin::TrNoop => Some(args.first().cloned().unwrap_or_default()),
        Builtin::TranslateNoop => Some(args.get(1).cloned().unwrap_or_default()),
        Builtin::Arg => {
            let [template, value] = args else {
                return None;
            };
            let template = text(template)?;
            let filled = match prim(value)? {
                Prim::Number(n) => e.arg_number(template, n),
                Prim::String(s) => ljos_i18n::arg::arg(template, s, s),
                Prim::Bool(b) => {
                    let s = if b { "1" } else { "0" };
                    ljos_i18n::arg::arg(template, s, s)
                }
                other => {
                    let s = other.text();
                    ljos_i18n::arg::arg(template, &s, &s)
                }
            };
            // With no marker to fill, the JS function logs a warning.
            filled.map(ops::string)
        }
        Builtin::ToFixed => {
            let Value::Real(x) = args.first()? else {
                return None;
            };
            let digits = match args.get(1) {
                None => 0.0,
                Some(v) => {
                    let d = ops::number(v)?;
                    if d.is_nan() { 0.0 } else { d.trunc() }
                }
            };
            if !(0.0..=100.0).contains(&digits) {
                // A RangeError.
                return None;
            }
            Some(ops::string(to_fixed(*x, digits as usize)))
        }
        Builtin::ToString => match args {
            [Value::Real(x)] => Some(ops::string(number_to_string(*x))),
            [Value::Bool(b)] => Some(Value::String(Atom::intern(if *b {
                "true"
            } else {
                "false"
            }))),
            [Value::String(s)] => Some(Value::String(s.clone())),
            _ => None,
        },
        Builtin::ToUpperCase | Builtin::ToLowerCase => {
            let [Value::String(s)] = args else {
                return None;
            };
            // Past ASCII the two languages' tables need not agree on every
            // character of every Unicode version.
            if !s.is_ascii() {
                return None;
            }
            Some(ops::string(if f == Builtin::ToUpperCase {
                s.to_ascii_uppercase()
            } else {
                s.to_ascii_lowercase()
            }))
        }
        Builtin::Trim => {
            let [Value::String(s)] = args else {
                return None;
            };
            let trimmed = s.trim_matches(is_js_whitespace);
            Some(if trimmed.len() == s.len() {
                Value::String(s.clone())
            } else {
                Value::string(trimmed)
            })
        }
    }
}

/// A string argument, where anything else is an error in JavaScript.
fn text(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}

/// The count of a plural call, read as the JS functions read it: an int32,
/// and a negative one is no count.
fn count(n: f64) -> Option<u32> {
    u32::try_from(to_int32(n)).ok()
}

/// `Math.max` and `Math.min`: every argument is converted, and one NaN
/// makes the result NaN.
fn extreme(args: &[Value], start: f64, pick: impl Fn(f64, f64) -> f64) -> Option<Value> {
    let mut out = start;
    let mut nan = false;
    for v in args {
        let x = ops::number(v)?;
        if x.is_nan() {
            nan = true;
        } else {
            out = pick(out, x);
        }
    }
    Some(Value::Real(if nan { f64::NAN } else { out }))
}

/// `Math.round`: to the nearest integer, a half towards positive infinity,
/// keeping the sign of a zero.
pub(super) fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let r = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if r == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        r
    }
}

/// ECMAScript's `StrWhiteSpaceChar`, which is what `trim` removes.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}'
            | '\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// `Number.prototype.toFixed` for 0 to 100 digits.
///
/// Rust writes the exact decimal expansion of a double rounded to the
/// digits asked for, which is what ECMAScript asks for too, except where
/// the value lies exactly half way between two: there ECMAScript takes
/// the larger and Rust the even one. A double lies half way exactly when
/// it is an odd number of halves of the last digit's unit, which is an
/// odd integer once scaled by a power of two.
pub(super) fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if !x.is_finite() || x.abs() >= 1e21 {
        return number_to_string(x);
    }
    if x == 0.0 {
        // Neither zero has a sign here.
        return format!("{:.*}", digits, 0.0);
    }
    let magnitude = x.abs();
    let scaled = magnitude * 2f64.powi(digits as i32 + 1);
    let tie = scaled.is_finite() && scaled.fract() == 0.0 && scaled % 2.0 == 1.0;
    if !tie {
        return format!("{x:.digits$}");
    }
    // One more digit holds the value exactly, and that digit is the 5.
    let exact = format!("{:.*}", digits + 1, magnitude);
    let mut out: Vec<u8> = exact.into_bytes();
    out.pop();
    if out.last() == Some(&b'.') {
        out.pop();
    }
    // Up: add one to the last digit, carrying.
    let mut at = out.len();
    let mut carry = true;
    while carry && at > 0 {
        at -= 1;
        match out[at] {
            b'.' => {}
            b'9' => out[at] = b'0',
            d => {
                out[at] = d + 1;
                carry = false;
            }
        }
    }
    if carry {
        out.insert(0, b'1');
    }
    if x < 0.0 {
        out.insert(0, b'-');
    }
    String::from_utf8(out).unwrap_or_default()
}

// ---- colours ---------------------------------------------------------------
//
// The arithmetic below is the prelude's (js.rs), step for step and in the
// same precision at each step: that is what makes the channels the same.

/// The three or four arguments of a colour function as numbers, the alpha
/// 1 when it is not given. Any other count is an error in JavaScript.
fn channels(args: &[Value]) -> Option<[f64; 4]> {
    if !(3..=4).contains(&args.len()) {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, v) in out.iter_mut().zip(args) {
        let n = ops::number(v)?;
        *slot = if n.is_nan() { 0.0 } else { n };
    }
    match args.get(3) {
        // `alpha(a)`: one, unless something was given.
        None | Some(Value::Undefined) => out[3] = 1.0,
        Some(_) => {}
    }
    Some(out)
}

/// `Math.max(0, Math.min(1, v))` for a number that is not NaN.
fn unit(v: f64) -> f64 {
    if v > 1.0 {
        1.0
    } else if v > 0.0 {
        v
    } else {
        // Zero, and what is below it: the zero `Math.max` picks is the
        // positive one.
        0.0
    }
}

fn to16(v: f64) -> f64 {
    let single = unit(v) as f32;
    round(f64::from((f64::from(single) * 65535.0) as f32))
}

fn turn(h: f64) -> f64 {
    let single = unit(h) as f32;
    (round(f64::from((f64::from(single) * 36000.0) as f32)) % 36000.0) / 36000.0
}

fn ch(v: f64) -> f64 {
    let v = if v.is_nan() { 0.0 } else { v };
    round(unit(v) * 65535.0)
}

/// The host's colour maker: sixteen bits of each channel.
/// `Qt.lighter` and `Qt.darker` as the colour script (`value_faces.rs`)
/// has them, step for step: of a colour, by a factor that is a number or
/// none. Anything else that would be a colour is the script's to read.
fn shade(args: &[Value], usual: f64, by: fn(Color, f64) -> Color) -> Option<Value> {
    if !(1..=2).contains(&args.len()) {
        return None;
    }
    let c = match &args[0] {
        Value::Color(c) => *c,
        Value::Undefined | Value::Null | Value::Bool(_) | Value::Real(_) => {
            return Some(Value::Null);
        }
        _ => return None,
    };
    let factor = match args.get(1) {
        None | Some(Value::Undefined) => usual,
        Some(Value::Real(n)) => *n,
        Some(_) => return None,
    };
    // The nearest whole hundredths: 1.15 is 115 though it falls short.
    let percent = (factor * 100.0).round();
    // What an infinite factor makes of a channel is the script's.
    if percent.is_infinite() {
        return None;
    }
    Some(Value::Color(by(c, percent)))
}

fn color(r: f64, g: f64, b: f64, a: f64) -> Value {
    let c = |n: f64| n.clamp(0.0, 65535.0) as u16;
    Value::Color(Color::rgba16(c(r), c(g), c(b), c(a)))
}

fn hue(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsla(h: f64, s: f64, l: f64, a: f64) -> Value {
    let h = turn(h);
    let s = to16(s) / 65535.0;
    let l = to16(l) / 65535.0;
    let a = to16(a);
    if s == 0.0 {
        return color(ch(l), ch(l), ch(l), a);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hsl_ch = |v: f64| {
        let n = ch(v);
        if n == 1.0 { 0.0 } else { n }
    };
    color(
        hsl_ch(hue(p, q, h + 1.0 / 3.0)),
        hsl_ch(hue(p, q, h)),
        hsl_ch(hue(p, q, h - 1.0 / 3.0)),
        a,
    )
}

fn hsva(h: f64, s: f64, v: f64, a: f64) -> Value {
    let h = turn(h) * 6.0;
    let s = to16(s) / 65535.0;
    let v = to16(v) / 65535.0;
    let a = to16(a);
    let i = h.floor();
    let f = h - i;
    let p = ch(v * (1.0 - s));
    let q = ch(v * (1.0 - s * f));
    let t = ch(v * (1.0 - s * (1.0 - f)));
    let v = ch(v);
    match i as i64 {
        0 => color(v, t, p, a),
        1 => color(q, v, p, a),
        2 => color(p, v, t, a),
        3 => color(p, q, v, a),
        4 => color(t, p, v, a),
        _ => color(v, p, q, a),
    }
}

// ---- value types -----------------------------------------------------------

/// The arguments of `Qt.point` and its like: exactly as many as the type
/// has fields, each `Number(v) + 0`.
fn doubles<const N: usize>(args: &[Value]) -> Option<[f64; N]> {
    if args.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, v) in out.iter_mut().zip(args) {
        *slot = ops::number(v)? + 0.0;
    }
    Some(out)
}

/// The same for the single-precision types.
fn singles<const N: usize>(args: &[Value]) -> Option<[f32; N]> {
    if args.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, v) in out.iter_mut().zip(args) {
        *slot = (ops::number(v)? as f32) + 0.0;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_goes_up_from_a_half_and_keeps_a_negative_zero() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.499_999_999_999_999_94), 0.0);
        assert_eq!(round(4_503_599_627_370_497.0), 4_503_599_627_370_497.0);
        for x in [-0.0, -0.2, -0.5] {
            let r = round(x);
            assert!(r == 0.0 && r.is_sign_negative(), "{x}");
        }
        assert!(round(f64::NAN).is_nan());
        assert_eq!(round(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn to_fixed_rounds_a_half_up_and_everything_else_exactly() {
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(1.5, 0), "2");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(9.5, 0), "10");
        assert_eq!(to_fixed(99.5, 0), "100");
        assert_eq!(to_fixed(0.96875, 4), "0.9688");
        // Not half way: 1.005 is a little below, 1.45 a little above.
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(1.45, 1), "1.4");
        assert_eq!(to_fixed(8.345, 2), "8.35");
        assert_eq!(to_fixed(1.0, 2), "1.00");
        assert_eq!(to_fixed(-0.0, 2), "0.00");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
        assert_eq!(to_fixed(123.456, 0), "123");
        assert_eq!(to_fixed(f64::NAN, 2), "NaN");
        assert_eq!(to_fixed(0.000_001, 7), "0.0000010");
    }

    #[test]
    fn trim_removes_what_ecmascript_calls_white_space() {
        let e = Engine::new();
        let trim = |s: &str| match call(&e, Builtin::Trim, &[Value::string(s)]) {
            Some(Value::String(s)) => s.to_string(),
            other => panic!("{other:?}"),
        };
        assert_eq!(trim(" \t\n a b \u{FEFF}\u{A0}"), "a b");
        // U+0085 is white space to Rust and not to ECMAScript.
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn max_and_min_convert_every_argument() {
        let e = Engine::new();
        let n = |x: f64| Value::Real(x);
        let real = |v: Option<Value>| match v {
            Some(Value::Real(r)) => r,
            other => panic!("{other:?}"),
        };
        assert_eq!(real(call(&e, Builtin::MathMax, &[])), f64::NEG_INFINITY);
        assert_eq!(real(call(&e, Builtin::MathMin, &[])), f64::INFINITY);
        assert!(real(call(&e, Builtin::MathMax, &[n(1.0), n(f64::NAN), n(3.0)])).is_nan());
        let z = real(call(&e, Builtin::MathMax, &[n(-0.0), n(0.0)]));
        assert!(z.is_sign_positive());
        let z = real(call(&e, Builtin::MathMin, &[n(0.0), n(-0.0)]));
        assert!(z.is_sign_negative());
        assert_eq!(
            real(call(&e, Builtin::MathMax, &[Value::string("5"), n(2.0)])),
            5.0
        );
        // An object is converted by a script, also after a NaN.
        assert_eq!(
            call(&e, Builtin::MathMax, &[n(f64::NAN), Value::point(0.0, 0.0)]),
            None
        );
    }

    #[test]
    fn colour_functions_refuse_the_counts_javascript_throws_for() {
        let e = Engine::new();
        let n = |x: f64| Value::Real(x);
        assert_eq!(call(&e, Builtin::QtRgba, &[n(1.0), n(0.0)]), None);
        assert_eq!(
            call(
                &e,
                Builtin::QtRgba,
                &[n(1.0), n(0.0), n(0.0), n(1.0), n(1.0)]
            ),
            None
        );
        assert_eq!(
            call(&e, Builtin::QtRgba, &[n(1.0), n(0.0), n(0.0)]),
            Some(Value::Color(Color::rgba16(65535, 0, 0, 65535)))
        );
        assert_eq!(call(&e, Builtin::QtPoint, &[n(1.0)]), None);
        assert_eq!(call(&e, Builtin::QtPoint, &[n(1.0), n(2.0), n(3.0)]), None);
    }
}
