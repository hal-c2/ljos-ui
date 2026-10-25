//! `Value` coercions and equality against ECMAScript rules.

use ljos_engine::{Atom, Color, EnumValue, TypeTag, Value};
use proptest::prelude::*;

#[test]
fn value_is_sixteen_bytes() {
    assert_eq!(std::mem::size_of::<Value>(), 16);
    assert_eq!(std::mem::size_of::<Atom>(), 8);
}

#[test]
fn to_boolean() {
    let falsy = [
        Value::Undefined,
        Value::Null,
        Value::Bool(false),
        Value::Int(0),
        Value::Real(0.0),
        Value::Real(-0.0),
        Value::Real(f64::NAN),
        Value::string(""),
    ];
    for v in falsy {
        assert!(!v.to_boolean(), "{v:?} should be falsy");
    }
    let truthy = [
        Value::string("0"),
        Value::string("false"),
        Value::string(" "),
        Value::Int(-1),
        Value::Real(f64::INFINITY),
        Value::list(vec![]),
        Value::Color(Color::TRANSPARENT),
    ];
    for v in truthy {
        assert!(v.to_boolean(), "{v:?} should be truthy");
    }
}

#[test]
fn to_number() {
    assert!(Value::Undefined.to_number().is_nan());
    assert_eq!(Value::Null.to_number(), 0.0);
    assert_eq!(Value::Bool(true).to_number(), 1.0);
    assert_eq!(Value::string("").to_number(), 0.0);
    assert_eq!(Value::string("  ").to_number(), 0.0);
    assert_eq!(Value::string("0").to_number(), 0.0);
    assert_eq!(Value::string(" 12.5 ").to_number(), 12.5);
    assert_eq!(Value::string("0x10").to_number(), 16.0);
    assert!(Value::string("12px").to_number().is_nan());
    assert_eq!(Value::string("-Infinity").to_number(), f64::NEG_INFINITY);
    assert_eq!(Value::list(vec![]).to_number(), 0.0);
    assert_eq!(Value::list(vec![Value::Int(5)]).to_number(), 5.0);
    assert!(
        Value::list(vec![Value::Int(1), Value::Int(2)])
            .to_number()
            .is_nan()
    );
    assert_eq!(Value::Enum(EnumValue { ty: 3, value: 7 }).to_number(), 7.0);
    let neg_zero = Value::string("-0").to_number();
    assert!(neg_zero == 0.0 && neg_zero.is_sign_negative());
}

#[test]
fn to_string() {
    let s = |v: Value| v.to_js_string().to_string();
    assert_eq!(s(Value::Undefined), "undefined");
    assert_eq!(s(Value::Null), "null");
    assert_eq!(s(Value::Real(-0.0)), "0");
    assert_eq!(s(Value::Real(f64::NAN)), "NaN");
    assert_eq!(s(Value::Real(0.1 + 0.2)), "0.30000000000000004");
    assert_eq!(s(Value::Real(1e21)), "1e+21");
    assert_eq!(
        s(Value::Real(123456789012345680000.0)),
        "123456789012345680000"
    );
    assert_eq!(s(Value::Real(1e-7)), "1e-7");
    assert_eq!(s(Value::Int(-3)), "-3");
    assert_eq!(s(Value::Bool(false)), "false");
    assert_eq!(
        s(Value::list(vec![
            Value::Int(1),
            Value::Null,
            Value::Undefined,
            Value::string("a")
        ])),
        "1,,,a"
    );
    assert_eq!(
        s(Value::Color(Color::from_rgba8(255, 0, 0, 255))),
        "#ff0000"
    );
    assert_eq!(s(Value::point(1.0, 2.5)), "QPointF(1, 2.5)");
    // A wheel delta is whole, and prints as qml6 prints it, in full.
    assert_eq!(s(Value::int_point(0, 120)), "QPoint(0, 120)");
    assert_eq!(s(Value::int_point(-1_234_567, 8)), "QPoint(-1234567, 8)");
}

#[test]
fn strict_equality() {
    let nan = Value::Real(f64::NAN);
    assert!(!nan.strict_equals(&nan));
    assert!(Value::Real(0.0).strict_equals(&Value::Real(-0.0)));
    assert!(Value::Int(1).strict_equals(&Value::Real(1.0)));
    assert!(!Value::Int(0).strict_equals(&Value::string("0")));
    assert!(!Value::string("").strict_equals(&Value::Int(0)));
    assert!(!Value::Null.strict_equals(&Value::Undefined));
    assert!(Value::Null.strict_equals(&Value::Null));
    assert!(Value::Undefined.strict_equals(&Value::Undefined));
    assert!(Value::string("a").strict_equals(&Value::string("a")));
    // qml6: `wheel.angleDelta === Qt.point(0, 120)` is true.
    assert!(Value::int_point(0, 120).strict_equals(&Value::point(0.0, 120.0)));
    assert!(!Value::int_point(0, 120).strict_equals(&Value::point(0.5, 120.0)));
    // Arrays compare by reference.
    let l = Value::list(vec![Value::Int(1)]);
    assert!(l.strict_equals(&l.clone()));
    assert!(!l.strict_equals(&Value::list(vec![Value::Int(1)])));
}

#[test]
fn change_equality_treats_nan_as_unchanged() {
    let nan = Value::Real(f64::NAN);
    assert_eq!(nan, nan.clone());
    assert_eq!(Value::Real(0.0), Value::Real(-0.0));
    assert_ne!(Value::Null, Value::Undefined);
    assert_ne!(Value::string("0"), Value::Int(0));
}

#[test]
fn property_coercion() {
    let real = TypeTag::Real;
    assert_eq!(real.coerce(&Value::Int(2)), Ok(Value::Real(2.0)));
    assert_eq!(real.coerce(&Value::string("2.5")), Ok(Value::Real(2.5)));
    let err = real.coerce(&Value::Undefined).unwrap_err();
    assert_eq!(err.to_string(), "Unable to assign [undefined] to double");
    assert_eq!(
        TypeTag::Color.coerce(&Value::string("red")),
        Ok(Value::Color(Color::from_rgba8(255, 0, 0, 255)))
    );
    assert_eq!(TypeTag::Var.coerce(&Value::Undefined), Ok(Value::Undefined));
    assert_eq!(
        TypeTag::String.coerce(&Value::Int(3)),
        Ok(Value::string("3"))
    );
    assert_eq!(
        TypeTag::Bool.coerce(&Value::string("")),
        Ok(Value::Bool(false))
    );
}

proptest! {
    #[test]
    fn number_string_round_trip(x in any::<f64>()) {
        let s = Value::Real(x).to_js_string();
        let back = Value::String(s).to_number();
        prop_assert!(back == x || (back.is_nan() && x.is_nan()));
    }

    #[test]
    fn strict_equals_is_symmetric(a in -3i32..3, b in -3.0f64..3.0) {
        let (a, b) = (Value::Int(a), Value::Real(b.round()));
        prop_assert_eq!(a.strict_equals(&b), b.strict_equals(&a));
    }
}
