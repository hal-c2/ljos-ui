//! The operators of tier 1, with JavaScript's coercions.
//!
//! Every function here is pure and answers for primitives only: `None`
//! means "not something this can do exactly as JavaScript does", and the
//! VM gives the expression to tier 2. An object, a value type or a list as
//! an operand of an operator that would coerce it is always `None`: what
//! JavaScript does there (`valueOf`, `toString`) is the script's business.

use std::borrow::Cow;
use std::cmp::Ordering;

use ljos_ir::{BinaryOp, UnaryOp};

use crate::atom::Atom;
use crate::value::Value;
use crate::{number_to_string, string_to_number, to_int32};

/// A primitive, as an operator sees it.
#[derive(Clone, Copy)]
pub(super) enum Prim<'a> {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(&'a str),
}

/// The primitive `v` is, if it is one. A URL is not: in a register it has
/// already become the string JavaScript sees.
pub(super) fn prim(v: &Value) -> Option<Prim<'_>> {
    Some(match v {
        Value::Undefined => Prim::Undefined,
        Value::Null => Prim::Null,
        Value::Bool(b) => Prim::Bool(*b),
        Value::Int(i) => Prim::Number(f64::from(*i)),
        Value::Real(r) => Prim::Number(*r),
        Value::Enum(e) => Prim::Number(f64::from(e.value)),
        Value::String(s) => Prim::String(s.as_str()),
        _ => return None,
    })
}

impl Prim<'_> {
    /// `ToNumber`.
    pub(super) fn number(self) -> f64 {
        match self {
            Prim::Undefined => f64::NAN,
            Prim::Null => 0.0,
            Prim::Bool(b) => f64::from(u8::from(b)),
            Prim::Number(n) => n,
            Prim::String(s) => string_to_number(s),
        }
    }

    /// `ToString`.
    pub(super) fn text(self) -> Cow<'static, str> {
        match self {
            Prim::Undefined => Cow::Borrowed("undefined"),
            Prim::Null => Cow::Borrowed("null"),
            Prim::Bool(true) => Cow::Borrowed("true"),
            Prim::Bool(false) => Cow::Borrowed("false"),
            Prim::Number(n) => Cow::Owned(number_to_string(n)),
            Prim::String(s) => Cow::Owned(s.to_owned()),
        }
    }

    fn push_to(self, out: &mut String) {
        match self {
            Prim::String(s) => out.push_str(s),
            other => out.push_str(&other.text()),
        }
    }
}

/// `ToNumber` of a primitive.
pub(super) fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Real(r) => Some(*r),
        other => prim(other).map(Prim::number),
    }
}

pub(super) fn string(s: String) -> Value {
    Value::String(Atom::from(s))
}

/// `a ** b`, and `Math.pow`: C's `pow` but for the two cases ECMAScript
/// decided otherwise (1 to the power of infinity or NaN is NaN).
pub(super) fn pow(a: f64, b: f64) -> f64 {
    if !b.is_finite() && a.abs() == 1.0 {
        f64::NAN
    } else {
        a.powf(b)
    }
}

/// Whether two strings are in that order by UTF-16 code unit, which is how
/// JavaScript compares them. It differs from the order of the bytes only
/// past the basic plane.
fn compare(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.as_bytes().cmp(b.as_bytes())
    } else {
        a.encode_utf16().cmp(b.encode_utf16())
    }
}

/// The length of a string as JavaScript counts it.
pub(super) fn utf16_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

/// The operators that need nothing but the two numbers they are given.
/// `None`: one of the others, which [`binary`] has.
#[inline]
pub(super) fn numbers(op: BinaryOp, x: f64, y: f64) -> Option<Value> {
    Some(match op {
        BinaryOp::Add => Value::Real(x + y),
        BinaryOp::Sub => Value::Real(x - y),
        BinaryOp::Mul => Value::Real(x * y),
        BinaryOp::Div => Value::Real(x / y),
        // IEEE equality is JavaScript's for numbers: a NaN equals nothing,
        // the two zeros each other.
        BinaryOp::Eq | BinaryOp::StrictEq => Value::Bool(x == y),
        BinaryOp::Ne | BinaryOp::StrictNe => Value::Bool(x != y),
        BinaryOp::Lt => Value::Bool(x < y),
        BinaryOp::Le => Value::Bool(x <= y),
        BinaryOp::Gt => Value::Bool(x > y),
        BinaryOp::Ge => Value::Bool(x >= y),
        _ => return None,
    })
}

pub(super) fn binary(op: BinaryOp, a: &Value, b: &Value) -> Option<Value> {
    match op {
        BinaryOp::Eq => return equals(a, b, false).map(Value::Bool),
        BinaryOp::Ne => return equals(a, b, false).map(|e| Value::Bool(!e)),
        BinaryOp::StrictEq => return equals(a, b, true).map(Value::Bool),
        BinaryOp::StrictNe => return equals(a, b, true).map(|e| Value::Bool(!e)),
        _ => {}
    }
    // The common case first: two numbers.
    let (x, y) = match (a, b) {
        (Value::Real(x), Value::Real(y)) => (Prim::Number(*x), Prim::Number(*y)),
        _ => (prim(a)?, prim(b)?),
    };
    Some(match op {
        BinaryOp::Add => match (x, y) {
            (Prim::Number(x), Prim::Number(y)) => Value::Real(x + y),
            (Prim::String(_), _) | (_, Prim::String(_)) => {
                let mut out = String::new();
                x.push_to(&mut out);
                y.push_to(&mut out);
                string(out)
            }
            _ => Value::Real(x.number() + y.number()),
        },
        BinaryOp::Sub => Value::Real(x.number() - y.number()),
        BinaryOp::Mul => Value::Real(x.number() * y.number()),
        BinaryOp::Div => Value::Real(x.number() / y.number()),
        // Rust's `%` on floats is C's `fmod`, which is ECMAScript's.
        BinaryOp::Rem => Value::Real(x.number() % y.number()),
        BinaryOp::Exp => Value::Real(pow(x.number(), y.number())),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            Value::Bool(match (x, y) {
                (Prim::String(x), Prim::String(y)) => {
                    let order = compare(x, y);
                    match op {
                        BinaryOp::Lt => order == Ordering::Less,
                        BinaryOp::Le => order != Ordering::Greater,
                        BinaryOp::Gt => order == Ordering::Greater,
                        _ => order != Ordering::Less,
                    }
                }
                _ => {
                    // With a NaN on either side every one of these is false,
                    // in Rust as in JavaScript.
                    let (x, y) = (x.number(), y.number());
                    match op {
                        BinaryOp::Lt => x < y,
                        BinaryOp::Le => x <= y,
                        BinaryOp::Gt => x > y,
                        _ => x >= y,
                    }
                }
            })
        }
        BinaryOp::BitAnd => int(to_int32(x.number()) & to_int32(y.number())),
        BinaryOp::BitOr => int(to_int32(x.number()) | to_int32(y.number())),
        BinaryOp::BitXor => int(to_int32(x.number()) ^ to_int32(y.number())),
        BinaryOp::Shl => int(to_int32(x.number()).wrapping_shl(shift(y))),
        BinaryOp::Shr => int(to_int32(x.number()).wrapping_shr(shift(y))),
        BinaryOp::UShr => Value::Real(f64::from(
            (to_int32(x.number()) as u32).wrapping_shr(shift(y)),
        )),
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::StrictEq | BinaryOp::StrictNe => return None,
    })
}

fn int(i: i32) -> Value {
    Value::Real(f64::from(i))
}

/// The count of a shift: the low five bits of the operand as a `uint32`.
fn shift(by: Prim<'_>) -> u32 {
    (to_int32(by.number()) as u32) & 31
}

pub(super) fn unary(op: UnaryOp, v: &Value) -> Option<Value> {
    Some(match op {
        // Every value has a truth, and none of them runs a script to tell.
        UnaryOp::Not => Value::Bool(!v.to_boolean()),
        UnaryOp::Neg => Value::Real(-number(v)?),
        UnaryOp::Plus => Value::Real(number(v)?),
        UnaryOp::BitNot => int(!to_int32(number(v)?)),
        UnaryOp::Typeof => Value::String(Atom::intern(type_of(v)?)),
    })
}

/// `typeof v`. A JS value the engine only holds a handle of may be a
/// function, so that is not answered here.
fn type_of(v: &Value) -> Option<&'static str> {
    Some(match v {
        Value::Undefined => "undefined",
        Value::Bool(_) => "boolean",
        Value::Int(_) | Value::Real(_) | Value::Enum(_) => "number",
        Value::String(_) | Value::Url(_) => "string",
        Value::Null
        | Value::Color(_)
        | Value::Point(_)
        | Value::IntPoint(_)
        | Value::Size(_)
        | Value::Rect(_)
        | Value::Font(_)
        | Value::Vector2d(_)
        | Value::Vector3d(_)
        | Value::Vector4d(_)
        | Value::Quaternion(_)
        | Value::Object(_)
        | Value::List(_)
        | Value::Map(_)
        | Value::Custom(_) => "object",
        Value::Js(_) => return None,
    })
}

/// A template string: its parts, each as a string.
pub(super) fn concat(parts: &[Value]) -> Option<Value> {
    let mut out = String::new();
    for part in parts {
        prim(part)?.push_to(&mut out);
    }
    Some(string(out))
}

/// What kind of thing a value is to `==`.
enum Kind<'a> {
    Prim(Prim<'a>),
    /// One object per value: equal when every field is.
    Typed,
    Object,
    List,
    Opaque,
}

fn kind(v: &Value) -> Kind<'_> {
    match v {
        Value::Object(_) => Kind::Object,
        Value::List(_) => Kind::List,
        Value::Js(_) => Kind::Opaque,
        Value::Color(_)
        | Value::Point(_)
        | Value::IntPoint(_)
        | Value::Size(_)
        | Value::Rect(_)
        | Value::Font(_)
        | Value::Vector2d(_)
        | Value::Vector3d(_)
        | Value::Vector4d(_)
        | Value::Quaternion(_) => Kind::Typed,
        Value::Url(s) => Kind::Prim(Prim::String(s.as_str())),
        other => match prim(other) {
            Some(p) => Kind::Prim(p),
            None => Kind::Opaque,
        },
    }
}

/// `a == b` (`strict`: `a === b`).
pub(super) fn equals(a: &Value, b: &Value, strict: bool) -> Option<bool> {
    let nullish = |p: Prim<'_>| matches!(p, Prim::Undefined | Prim::Null);
    Some(match (kind(a), kind(b)) {
        (Kind::Prim(x), Kind::Prim(y)) => match (x, y) {
            (Prim::Undefined, Prim::Undefined) | (Prim::Null, Prim::Null) => true,
            (Prim::Number(x), Prim::Number(y)) => x == y,
            (Prim::String(x), Prim::String(y)) => x == y,
            (Prim::Bool(x), Prim::Bool(y)) => x == y,
            _ if strict => false,
            (Prim::Undefined, Prim::Null) | (Prim::Null, Prim::Undefined) => true,
            _ if nullish(x) || nullish(y) => false,
            // A number, a string and a boolean of different types compare
            // as numbers, whichever two they are.
            _ => x.number() == y.number(),
        },
        // What is behind a handle may be anything, but never `null` or
        // `undefined`: those do not cross as handles.
        (Kind::Opaque, Kind::Prim(p)) | (Kind::Prim(p), Kind::Opaque) if nullish(p) => false,
        (Kind::Opaque, _) | (_, Kind::Opaque) => return None,
        (Kind::Prim(p), _) | (_, Kind::Prim(p)) => {
            if strict || nullish(p) {
                false
            } else {
                // `ToPrimitive` of the object.
                return None;
            }
        }
        (Kind::Object, Kind::Object) => match (a, b) {
            (Value::Object(x), Value::Object(y)) => x == y,
            _ => return None,
        },
        (Kind::Typed, Kind::Typed) => typed_equal(a, b),
        // Whether two arrays are the one array is not something a register
        // knows.
        (Kind::List, Kind::List) => return None,
        _ => false,
    })
}

/// Two value types, as the one object per value JavaScript has: the same
/// when every field is, where a NaN is the same as a NaN.
fn typed_equal(a: &Value, b: &Value) -> bool {
    fn d(a: f64, b: f64) -> bool {
        a == b || (a.is_nan() && b.is_nan())
    }
    fn s(a: f32, b: f32) -> bool {
        a == b || (a.is_nan() && b.is_nan())
    }
    match (a, b) {
        (Value::Color(x), Value::Color(y)) => x == y,
        (Value::Point(x), Value::Point(y)) => d(x.x, y.x) && d(x.y, y.y),
        (Value::IntPoint(x), Value::IntPoint(y)) => x == y,
        // qml6: a wheel delta is `===` the same `Qt.point`.
        (Value::IntPoint(x), Value::Point(y)) | (Value::Point(y), Value::IntPoint(x)) => {
            d(f64::from(x.x), y.x) && d(f64::from(x.y), y.y)
        }
        (Value::Size(x), Value::Size(y)) => d(x.width, y.width) && d(x.height, y.height),
        (Value::Rect(x), Value::Rect(y)) => {
            d(x.x, y.x) && d(x.y, y.y) && d(x.width, y.width) && d(x.height, y.height)
        }
        (Value::Vector2d(x), Value::Vector2d(y)) => s(x.x, y.x) && s(x.y, y.y),
        (Value::Vector3d(x), Value::Vector3d(y)) => s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z),
        (Value::Vector4d(x), Value::Vector4d(y)) => {
            s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z) && s(x.w, y.w)
        }
        (Value::Quaternion(x), Value::Quaternion(y)) => {
            s(x.scalar, y.scalar) && s(x.x, y.x) && s(x.y, y.y) && s(x.z, y.z)
        }
        (Value::Font(x), Value::Font(y)) => {
            x.family == y.family
                && d(x.point_size, y.point_size)
                && x.pixel_size == y.pixel_size
                && x.weight == y.weight
                && x.italic == y.italic
                && x.underline == y.underline
                && x.strikeout == y.strikeout
                && d(x.letter_spacing, y.letter_spacing)
                && d(x.word_spacing, y.word_spacing)
                && x.capitalization == y.capitalization
                && x.hinting_preference == y.hinting_preference
                && x.scale_with_system == y.scale_with_system
                && x.style_name == y.style_name
                && x.overline == y.overline
                && x.kerning == y.kerning
                && x.prefer_shaping == y.prefer_shaping
                && x.context_font_merging == y.context_font_merging
                && x.prefer_typo_line_metrics == y.prefer_typo_line_metrics
                && x.features == y.features
                && x.variable_axes == y.variable_axes
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(x: f64) -> Value {
        Value::Real(x)
    }

    fn s(x: &str) -> Value {
        Value::string(x)
    }

    fn text(v: Option<Value>) -> String {
        match v {
            Some(Value::String(s)) => s.to_string(),
            other => panic!("not a string: {other:?}"),
        }
    }

    fn real(v: Option<Value>) -> f64 {
        match v {
            Some(Value::Real(r)) => r,
            other => panic!("not a number: {other:?}"),
        }
    }

    fn truth(v: Option<Value>) -> bool {
        match v {
            Some(Value::Bool(b)) => b,
            other => panic!("not a boolean: {other:?}"),
        }
    }

    #[test]
    fn plus_joins_when_either_side_is_a_string() {
        assert_eq!(text(binary(BinaryOp::Add, &s("a"), &n(1.0))), "a1");
        assert_eq!(text(binary(BinaryOp::Add, &n(1.5), &s("a"))), "1.5a");
        assert_eq!(text(binary(BinaryOp::Add, &s("a"), &Value::Null)), "anull");
        assert_eq!(
            text(binary(BinaryOp::Add, &Value::Undefined, &s(""))),
            "undefined"
        );
        assert_eq!(
            text(binary(BinaryOp::Add, &s("x"), &Value::Bool(true))),
            "xtrue"
        );
        assert_eq!(text(binary(BinaryOp::Add, &s("x"), &n(-0.0))), "x0");
    }

    #[test]
    fn plus_adds_everything_else_as_numbers() {
        assert_eq!(
            real(binary(BinaryOp::Add, &n(1.0), &Value::Bool(true))),
            2.0
        );
        assert_eq!(real(binary(BinaryOp::Add, &Value::Null, &n(3.0))), 3.0);
        assert!(real(binary(BinaryOp::Add, &Value::Undefined, &n(3.0))).is_nan());
        assert_eq!(
            real(binary(
                BinaryOp::Add,
                &Value::Bool(true),
                &Value::Bool(true)
            )),
            2.0
        );
    }

    #[test]
    fn the_other_arithmetic_converts_strings_to_numbers() {
        assert_eq!(real(binary(BinaryOp::Sub, &s("5"), &n(2.0))), 3.0);
        assert_eq!(real(binary(BinaryOp::Mul, &s(" 4 "), &s("0x10"))), 64.0);
        assert!(real(binary(BinaryOp::Mul, &s("a"), &n(1.0))).is_nan());
        assert_eq!(real(binary(BinaryOp::Div, &n(1.0), &n(0.0))), f64::INFINITY);
        assert_eq!(
            real(binary(BinaryOp::Div, &n(-1.0), &n(0.0))),
            f64::NEG_INFINITY
        );
        assert!(real(binary(BinaryOp::Rem, &n(5.0), &n(0.0))).is_nan());
        assert_eq!(real(binary(BinaryOp::Rem, &n(-5.0), &n(3.0))), -2.0);
        assert_eq!(real(binary(BinaryOp::Rem, &n(5.5), &n(2.0))), 1.5);
    }

    #[test]
    fn a_zero_keeps_its_sign() {
        let z = real(binary(BinaryOp::Mul, &n(-1.0), &n(0.0)));
        assert!(z == 0.0 && z.is_sign_negative());
        let z = real(unary(UnaryOp::Neg, &n(0.0)));
        assert!(z == 0.0 && z.is_sign_negative());
        let z = real(binary(BinaryOp::Rem, &n(-4.0), &n(2.0)));
        assert!(z == 0.0 && z.is_sign_negative());
        let z = real(binary(BinaryOp::Add, &n(-0.0), &n(0.0)));
        assert!(z == 0.0 && z.is_sign_positive());
    }

    #[test]
    fn power_follows_ecmascript_where_it_leaves_c() {
        assert!(pow(1.0, f64::INFINITY).is_nan());
        assert!(pow(-1.0, f64::NEG_INFINITY).is_nan());
        assert!(pow(1.0, f64::NAN).is_nan());
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(pow(2.0, f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn strings_compare_by_code_unit() {
        assert!(truth(binary(BinaryOp::Lt, &s("a"), &s("b"))));
        assert!(truth(binary(BinaryOp::Lt, &s("B"), &s("a"))));
        assert!(truth(binary(BinaryOp::Lt, &s("a"), &s("ab"))));
        assert!(truth(binary(BinaryOp::Le, &s("a"), &s("a"))));
        // U+1F600 is the surrogate pair D83D DE00, which is below U+FF5E.
        assert!(truth(binary(BinaryOp::Lt, &s("\u{1F600}"), &s("\u{FF5E}"))));
        assert!("\u{1F600}" > "\u{FF5E}");
        // A string and a number compare as numbers.
        assert!(truth(binary(BinaryOp::Lt, &s("9"), &n(10.0))));
        assert!(!truth(binary(BinaryOp::Lt, &s("9"), &s("10"))));
    }

    #[test]
    fn nothing_is_in_order_with_nan() {
        for op in [BinaryOp::Lt, BinaryOp::Le, BinaryOp::Gt, BinaryOp::Ge] {
            assert!(!truth(binary(op, &n(f64::NAN), &n(1.0))));
            assert!(!truth(binary(op, &n(1.0), &Value::Undefined)));
            assert!(!truth(binary(op, &s("a"), &n(1.0))));
        }
        assert!(truth(binary(BinaryOp::Le, &Value::Null, &n(0.0))));
        assert!(truth(binary(
            BinaryOp::Ge,
            &Value::Null,
            &Value::Bool(false)
        )));
    }

    #[test]
    fn loose_and_strict_equality_differ_where_javascript_says() {
        let eq = |a: &Value, b: &Value| equals(a, b, false);
        let seq = |a: &Value, b: &Value| equals(a, b, true);
        assert_eq!(eq(&n(1.0), &s("1")), Some(true));
        assert_eq!(seq(&n(1.0), &s("1")), Some(false));
        assert_eq!(eq(&Value::Null, &Value::Undefined), Some(true));
        assert_eq!(seq(&Value::Null, &Value::Undefined), Some(false));
        assert_eq!(eq(&Value::Null, &n(0.0)), Some(false));
        assert_eq!(eq(&Value::Undefined, &s("")), Some(false));
        assert_eq!(eq(&Value::Bool(true), &n(1.0)), Some(true));
        assert_eq!(eq(&Value::Bool(true), &s("1")), Some(true));
        assert_eq!(eq(&Value::Bool(false), &s("")), Some(true));
        assert_eq!(eq(&s("a"), &Value::Bool(true)), Some(false));
        assert_eq!(eq(&n(f64::NAN), &n(f64::NAN)), Some(false));
        assert_eq!(seq(&n(f64::NAN), &n(f64::NAN)), Some(false));
        assert_eq!(seq(&n(0.0), &n(-0.0)), Some(true));
        assert_eq!(seq(&s("a"), &s("a")), Some(true));
    }

    #[test]
    fn value_types_are_equal_by_value_and_objects_coerce_nowhere() {
        let p = Value::point(1.0, 2.0);
        assert_eq!(equals(&p, &Value::point(1.0, 2.0), true), Some(true));
        assert_eq!(equals(&p, &Value::point(1.0, 3.0), false), Some(false));
        assert_eq!(equals(&p, &Value::size(1.0, 2.0), true), Some(false));
        assert_eq!(
            equals(
                &Value::point(f64::NAN, 0.0),
                &Value::point(f64::NAN, 0.0),
                true
            ),
            Some(true)
        );
        assert_eq!(equals(&p, &Value::Null, false), Some(false));
        assert_eq!(equals(&p, &n(1.0), true), Some(false));
        // `point == "QPointF(1, 2)"` is JavaScript's to answer.
        assert_eq!(equals(&p, &s("QPointF(1, 2)"), false), None);
        let list = Value::list(vec![]);
        assert_eq!(equals(&list, &Value::Undefined, false), Some(false));
        assert_eq!(equals(&list, &n(0.0), true), Some(false));
        assert_eq!(equals(&list, &n(0.0), false), None);
        assert_eq!(equals(&list, &list.clone(), true), None);
        assert_eq!(equals(&list, &p, false), Some(false));
        assert_eq!(binary(BinaryOp::Add, &p, &n(1.0)), None);
        assert_eq!(binary(BinaryOp::Lt, &list, &n(1.0)), None);
    }

    #[test]
    fn bit_operators_work_on_int32() {
        assert_eq!(real(binary(BinaryOp::BitOr, &n(1.5), &n(2.0))), 3.0);
        assert_eq!(
            real(binary(BinaryOp::BitAnd, &n(4_294_967_295.0), &n(255.0))),
            255.0
        );
        assert_eq!(
            real(binary(BinaryOp::Shl, &n(1.0), &n(31.0))),
            -2_147_483_648.0
        );
        assert_eq!(real(binary(BinaryOp::Shl, &n(1.0), &n(32.0))), 1.0);
        assert_eq!(real(binary(BinaryOp::Shr, &n(-8.0), &n(1.0))), -4.0);
        assert_eq!(
            real(binary(BinaryOp::UShr, &n(-1.0), &n(0.0))),
            4_294_967_295.0
        );
        assert_eq!(
            real(binary(BinaryOp::UShr, &n(-8.0), &n(-31.0))),
            2_147_483_644.0
        );
        assert_eq!(real(unary(UnaryOp::BitNot, &n(0.0))), -1.0);
        assert_eq!(real(unary(UnaryOp::BitNot, &s("x"))), -1.0);
    }

    #[test]
    fn typeof_and_not_answer_for_every_value_but_a_handle() {
        assert_eq!(text(unary(UnaryOp::Typeof, &Value::Null)), "object");
        assert_eq!(text(unary(UnaryOp::Typeof, &Value::Undefined)), "undefined");
        assert_eq!(text(unary(UnaryOp::Typeof, &n(1.0))), "number");
        assert_eq!(text(unary(UnaryOp::Typeof, &s(""))), "string");
        assert_eq!(text(unary(UnaryOp::Typeof, &Value::Bool(false))), "boolean");
        assert_eq!(
            text(unary(UnaryOp::Typeof, &Value::point(0.0, 0.0))),
            "object"
        );
        assert!(truth(unary(UnaryOp::Not, &s(""))));
        assert!(truth(unary(UnaryOp::Not, &n(f64::NAN))));
        assert!(!truth(unary(UnaryOp::Not, &Value::list(vec![]))));
        assert!(!truth(unary(UnaryOp::Not, &s("0"))));
    }

    #[test]
    fn a_template_joins_primitives_only() {
        let parts = [s("a"), n(1.0), Value::Null, Value::Bool(false), n(-0.0)];
        assert_eq!(text(concat(&parts)), "a1nullfalse0");
        assert_eq!(concat(&[s("a"), Value::point(0.0, 0.0)]), None);
    }
}
