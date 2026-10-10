//! Property types ([`TypeTag`]), assignment coercion and the [`QmlValue`]
//! bridge between Rust field types and [`Value`].
//!
//! Every write to a typed property goes through [`TypeTag::coerce`], for
//! native fields and QML-declared properties alike, so both kinds behave the
//! same. The rules follow what QML does when JS assigns to a typed
//! property:
//!
//! | target | accepts | notes |
//! |---|---|---|
//! | `var` | anything | |
//! | `bool` | anything but `undefined` | `ToBoolean` |
//! | `int` | numbers, bools, strings, `null` | `ToNumber` then `ToInt32` (truncates, wraps; NaN is 0) |
//! | `real` | numbers, bools, strings, `null` | `ToNumber` (`"abc"` is NaN) |
//! | `string` | anything but `undefined` | `ToString`; `null` is `""` |
//! | `color` | colours, colour strings | `#rgb`, `#argb`, `#rrggbb`, `#aarrggbb`, CSS keywords |
//! | `url` | urls, strings | |
//! | `point`/`size`/`rect`/`font` | the same value type | |
//! | object types | objects, `null`, `undefined` (as `null`) | the engine checks the class |
//! | `list<T>` | lists, a single object (a list of one), `null` (empty) | |
//! | enums | numbers | |
//!
//! `undefined` is rejected everywhere but `var` and object types, matching
//! Qt's "Unable to assign [undefined] to …". The `null`-to-`string` and
//! `null`-to-number cases are recorded as to be verified against Qt.

use std::sync::Arc;

use crate::atom::Atom;
use crate::ids::ObjectId;
use crate::number::to_int32;
use crate::value::{Color, EnumValue, Font, Point, Rect, Size, Value};

/// The declared type of a property, signal parameter or method return.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeTag {
    Var,
    Bool,
    Int,
    Real,
    String,
    Color,
    Point,
    Size,
    Rect,
    Font,
    Url,
    /// An object reference. `Some(name)` restricts it to a QML type (and
    /// what derives from it).
    Object(Option<Atom>),
    /// `list<T>`.
    List(Option<Atom>),
    /// An enum, by its (qualified) name.
    Enum(Atom),
    /// A grouped property (`border` in `border.width`); the group's type.
    Group(Atom),
    /// No value (method returns).
    Void,
}

impl TypeTag {
    /// The name as QML writes it and the type dump records it.
    pub fn name(&self) -> String {
        match self {
            TypeTag::Var => "var".into(),
            TypeTag::Bool => "bool".into(),
            TypeTag::Int => "int".into(),
            TypeTag::Real => "real".into(),
            TypeTag::String => "string".into(),
            TypeTag::Color => "color".into(),
            TypeTag::Point => "point".into(),
            TypeTag::Size => "size".into(),
            TypeTag::Rect => "rect".into(),
            TypeTag::Font => "font".into(),
            TypeTag::Url => "url".into(),
            TypeTag::Object(None) => "QtObject".into(),
            TypeTag::Object(Some(n)) | TypeTag::Enum(n) | TypeTag::Group(n) => n.to_string(),
            TypeTag::List(None) => "list".into(),
            TypeTag::List(Some(n)) => format!("list<{n}>"),
            TypeTag::Void => "void".into(),
        }
    }

    /// Parses a QML property type as written in `property <type> name`.
    /// Unknown names are taken as object types; the compiler, which knows
    /// the imports, decides whether they are enums instead.
    pub fn parse(s: &str) -> TypeTag {
        match s.trim() {
            "var" | "variant" | "date" => TypeTag::Var,
            "bool" => TypeTag::Bool,
            "int" => TypeTag::Int,
            "real" | "double" | "number" => TypeTag::Real,
            "string" => TypeTag::String,
            "color" => TypeTag::Color,
            "point" => TypeTag::Point,
            "size" => TypeTag::Size,
            "rect" => TypeTag::Rect,
            "font" => TypeTag::Font,
            "url" => TypeTag::Url,
            "QtObject" => TypeTag::Object(None),
            "list" => TypeTag::List(None),
            "void" => TypeTag::Void,
            other => match other
                .strip_prefix("list<")
                .and_then(|r| r.strip_suffix('>'))
            {
                Some(inner) => TypeTag::List(Some(Atom::intern(inner.trim()))),
                None => TypeTag::Object(Some(Atom::intern(other))),
            },
        }
    }

    /// The value a property of this type has before anything is assigned.
    pub fn default_value(&self) -> Value {
        match self {
            TypeTag::Var | TypeTag::Group(_) | TypeTag::Void => Value::Undefined,
            TypeTag::Bool => Value::Bool(false),
            TypeTag::Int => Value::Int(0),
            TypeTag::Real => Value::Real(0.0),
            TypeTag::String => Value::String(Atom::EMPTY),
            // QML's default colour is invalid; it reads as transparent black.
            TypeTag::Color => Value::Color(Color::TRANSPARENT),
            TypeTag::Point => Value::Point(Arc::default()),
            TypeTag::Size => Value::Size(Arc::default()),
            TypeTag::Rect => Value::Rect(Arc::default()),
            TypeTag::Font => Value::Font(Arc::default()),
            TypeTag::Url => Value::Url(Atom::EMPTY),
            TypeTag::Object(_) => Value::Null,
            TypeTag::List(_) => Value::List(Arc::default()),
            TypeTag::Enum(_) => Value::Enum(EnumValue::default()),
        }
    }

    /// Converts `v` for assignment to a property of this type. See the
    /// module docs for the rules. Object class checks are the engine's.
    pub fn coerce(&self, v: &Value) -> Result<Value, ConvertError> {
        let fail = || {
            Err(ConvertError {
                from: describe(v),
                to: self.name(),
            })
        };
        if matches!(v, Value::Undefined) && !matches!(self, TypeTag::Var | TypeTag::Object(_)) {
            return fail();
        }
        let numeric = |v: &Value| match v {
            Value::Int(_)
            | Value::Real(_)
            | Value::Enum(_)
            | Value::Bool(_)
            | Value::String(_)
            | Value::Null => Some(v.to_number()),
            _ => None,
        };
        Ok(match self {
            TypeTag::Var => v.clone(),
            TypeTag::Bool => match v {
                Value::Bool(_) => v.clone(),
                _ => Value::Bool(v.to_boolean()),
            },
            TypeTag::Int => match v {
                Value::Int(_) => v.clone(),
                _ => match numeric(v) {
                    Some(n) => Value::Int(to_int32(n)),
                    None => return fail(),
                },
            },
            TypeTag::Real => match v {
                Value::Real(_) => v.clone(),
                _ => match numeric(v) {
                    Some(n) => Value::Real(n),
                    None => return fail(),
                },
            },
            TypeTag::String => match v {
                Value::String(_) => v.clone(),
                Value::Null => Value::String(Atom::EMPTY),
                _ => Value::String(v.to_js_string()),
            },
            TypeTag::Color => match v {
                Value::Color(_) => v.clone(),
                Value::String(s) => match Color::parse(s) {
                    Some(c) => Value::Color(c),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Url => match v {
                Value::Url(_) => v.clone(),
                Value::String(s) => Value::Url(s.clone()),
                _ => return fail(),
            },
            TypeTag::Point => match v {
                Value::Point(_) => v.clone(),
                _ => return fail(),
            },
            TypeTag::Size => match v {
                Value::Size(_) => v.clone(),
                _ => return fail(),
            },
            TypeTag::Rect => match v {
                Value::Rect(_) => v.clone(),
                _ => return fail(),
            },
            TypeTag::Font => match v {
                Value::Font(_) => v.clone(),
                _ => return fail(),
            },
            TypeTag::Object(_) => match v {
                Value::Object(_) | Value::Null => v.clone(),
                Value::Undefined => Value::Null,
                _ => return fail(),
            },
            TypeTag::List(_) => match v {
                Value::List(_) => v.clone(),
                Value::Object(_) => Value::list(vec![v.clone()]),
                Value::Null => Value::List(Arc::default()),
                _ => return fail(),
            },
            TypeTag::Enum(_) => match v {
                Value::Enum(_) => v.clone(),
                Value::Int(_) | Value::Real(_) => Value::Enum(EnumValue {
                    ty: 0,
                    value: to_int32(v.to_number()),
                }),
                _ => return fail(),
            },
            TypeTag::Group(_) | TypeTag::Void => return fail(),
        })
    }
}

/// Qt-style description of a value in assignment errors: `[undefined]`,
/// `QString`, `double`.
fn describe(v: &Value) -> String {
    match v {
        Value::Undefined => "[undefined]".into(),
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Int(_) => "int".into(),
        Value::Real(_) => "double".into(),
        Value::String(_) => "QString".into(),
        Value::Color(_) => "QColor".into(),
        Value::Point(_) => "QPointF".into(),
        Value::Size(_) => "QSizeF".into(),
        Value::Rect(_) => "QRectF".into(),
        Value::Font(_) => "QFont".into(),
        Value::Url(_) => "QUrl".into(),
        Value::Object(_) => "QObject*".into(),
        Value::List(_) => "QJSValue".into(),
        Value::Enum(_) => "int".into(),
        Value::Js(_) => "QJSValue".into(),
    }
}

/// A value could not be assigned to a typed property.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Unable to assign {from} to {to}")]
pub struct ConvertError {
    pub from: String,
    pub to: String,
}

/// A `url` value as a Rust type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Url(pub Atom);

/// Rust types that can be QML property, argument and return types.
///
/// `from_value` coerces first (see [`TypeTag::coerce`]), so a field of type
/// `f64` accepts `5`, `"5"` and `true` the way a QML `real` does.
pub trait QmlValue: Sized + 'static {
    /// The QML type this Rust type stands for.
    fn type_tag() -> TypeTag;
    fn to_value(&self) -> Value;
    fn from_value(v: &Value) -> Result<Self, ConvertError>;
}

macro_rules! qml_value {
    ($t:ty, $tag:expr, |$s:ident| $to:expr, $pat:pat => $from:expr) => {
        impl QmlValue for $t {
            fn type_tag() -> TypeTag {
                $tag
            }
            fn to_value(&self) -> Value {
                let $s = self;
                $to
            }
            fn from_value(v: &Value) -> Result<Self, ConvertError> {
                match Self::type_tag().coerce(v)? {
                    $pat => Ok($from),
                    _ => unreachable!("coerce returns the tag's own variant"),
                }
            }
        }
    };
}

qml_value!(bool, TypeTag::Bool, |s| Value::Bool(*s), Value::Bool(b) => b);
qml_value!(i32, TypeTag::Int, |s| Value::Int(*s), Value::Int(i) => i);
qml_value!(f64, TypeTag::Real, |s| Value::Real(*s), Value::Real(r) => r);
qml_value!(f32, TypeTag::Real, |s| Value::Real(f64::from(*s)), Value::Real(r) => r as f32);
qml_value!(Atom, TypeTag::String, |s| Value::String(s.clone()), Value::String(a) => a);
qml_value!(String, TypeTag::String, |s| Value::String(Atom::new(s)), Value::String(a) => a.to_string());
qml_value!(Color, TypeTag::Color, |s| Value::Color(*s), Value::Color(c) => c);
qml_value!(Url, TypeTag::Url, |s| Value::Url(s.0.clone()), Value::Url(u) => Url(u));
qml_value!(Point, TypeTag::Point, |s| Value::Point(Arc::new(*s)), Value::Point(p) => *p);
qml_value!(Size, TypeTag::Size, |s| Value::Size(Arc::new(*s)), Value::Size(p) => *p);
qml_value!(Rect, TypeTag::Rect, |s| Value::Rect(Arc::new(*s)), Value::Rect(p) => *p);
qml_value!(Font, TypeTag::Font, |s| Value::Font(Arc::new(s.clone())), Value::Font(f) => (*f).clone());
qml_value!(EnumValue, TypeTag::Enum(Atom::intern("enum")), |s| Value::Enum(*s), Value::Enum(e) => e);
qml_value!(
    Vec<Value>,
    TypeTag::List(None),
    |s| Value::List(Arc::new(s.clone())),
    Value::List(l) => (*l).clone()
);

impl QmlValue for Value {
    fn type_tag() -> TypeTag {
        TypeTag::Var
    }
    fn to_value(&self) -> Value {
        self.clone()
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        Ok(v.clone())
    }
}

impl QmlValue for Option<ObjectId> {
    fn type_tag() -> TypeTag {
        TypeTag::Object(None)
    }
    fn to_value(&self) -> Value {
        self.map_or(Value::Null, Value::Object)
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        Ok(Self::type_tag().coerce(v)?.as_object())
    }
}

impl QmlValue for () {
    fn type_tag() -> TypeTag {
        TypeTag::Void
    }
    fn to_value(&self) -> Value {
        Value::Undefined
    }
    fn from_value(_: &Value) -> Result<Self, ConvertError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coercions() {
        assert_eq!(f64::from_value(&Value::Int(3)), Ok(3.0));
        assert_eq!(f64::from_value(&Value::string("2.5")), Ok(2.5));
        assert!(f64::from_value(&Value::string("x")).unwrap().is_nan());
        assert!(f64::from_value(&Value::Undefined).is_err());
        assert_eq!(i32::from_value(&Value::Real(5.9)), Ok(5));
        assert_eq!(i32::from_value(&Value::Real(f64::NAN)), Ok(0));
        assert_eq!(bool::from_value(&Value::string("")), Ok(false));
        assert_eq!(bool::from_value(&Value::string("0")), Ok(true));
        assert_eq!(String::from_value(&Value::Real(-0.0)), Ok("0".to_string()));
        assert_eq!(String::from_value(&Value::Null), Ok(String::new()));
        assert_eq!(
            Color::from_value(&Value::string("#80ff0000")),
            Ok(Color::from_rgba8(255, 0, 0, 128))
        );
        assert_eq!(
            Color::from_value(&Value::string("Red")),
            Ok(Color::from_rgba8(255, 0, 0, 255))
        );
        assert!(Color::from_value(&Value::string("#12")).is_err());
        assert_eq!(<Option<ObjectId>>::from_value(&Value::Undefined), Ok(None));
        let err = f64::from_value(&Value::Undefined).unwrap_err();
        assert_eq!(err.to_string(), "Unable to assign [undefined] to real");
    }

    #[test]
    fn parse_names() {
        assert_eq!(TypeTag::parse("double"), TypeTag::Real);
        assert_eq!(
            TypeTag::parse("list<Item>"),
            TypeTag::List(Some(Atom::new("Item")))
        );
        assert_eq!(TypeTag::parse("list<Item>").name(), "list<Item>");
        assert_eq!(
            TypeTag::parse("Rectangle"),
            TypeTag::Object(Some(Atom::new("Rectangle")))
        );
    }
}
