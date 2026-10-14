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
//! | `point` | points, `"x,y"`, a `vector2d`, `null` (the origin) | |
//! | `size` | sizes, `"WxH"` | |
//! | `rect` | rects, `"x,y,WxH"` | |
//! | `vector2d` | the same, `"x,y"`, a `point`, a longer vector (cut short) | |
//! | `vector3d`/`vector4d`/`quaternion` | the same, numbers in a string | |
//! | `font` | fonts | |
//! | object types | objects, `null` | the engine checks the class |
//! | `list<T>` | lists, a single object (a list of one), `null` (empty) | |
//! | enums | numbers (a real is rounded), bools, the name of a key | the engine looks the key up in the property's type ([`crate::MetaObject::coerce_enum`]); the value is the number |
//!
//! `undefined` is rejected everywhere but `var`, matching Qt's "Unable to
//! assign [undefined] to …"; a property that can be taken back never gets
//! here with it (`crate::reset`). The `null`-to-`string` and
//! `null`-to-number cases are recorded as to be verified against Qt.

use std::sync::Arc;

use crate::atom::Atom;
use crate::ids::ObjectId;
use crate::number::to_int32;
use crate::value::{Color, EnumValue, Font, Point, Rect, Size, Value};
use crate::value_types::{IntPoint, Quaternion, Vector2d, Vector3d, Vector4d, area, extent, list};

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
    Vector2d,
    Vector3d,
    Vector4d,
    Quaternion,
    Url,
    /// An object reference. `Some(name)` restricts it to a QML type (and
    /// what derives from it).
    Object(Option<Atom>),
    /// `list<T>`.
    List(Option<Atom>),
    /// An enum, by its name: `TransformOrigin`. A property of this type
    /// holds one of the enum's numbers, and reads as a number; the enum is
    /// looked up in the type of the object that has the property.
    Enum(Atom),
    /// Flags, by the name of their enum: `ClosePolicy`. As [`TypeTag::Enum`],
    /// but the number is any combination of the keys' numbers, and a string
    /// may name several (`"CloseOnEscape|CloseOnPressOutside"`).
    Flags(Atom),
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
            TypeTag::Vector2d => "vector2d".into(),
            TypeTag::Vector3d => "vector3d".into(),
            TypeTag::Vector4d => "vector4d".into(),
            TypeTag::Quaternion => "quaternion".into(),
            TypeTag::Url => "url".into(),
            TypeTag::Object(None) => "QtObject".into(),
            TypeTag::Object(Some(n)) | TypeTag::Enum(n) | TypeTag::Flags(n) | TypeTag::Group(n) => {
                n.to_string()
            }
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
            "vector2d" => TypeTag::Vector2d,
            "vector3d" => TypeTag::Vector3d,
            "vector4d" => TypeTag::Vector4d,
            "quaternion" => TypeTag::Quaternion,
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
            TypeTag::Vector2d => Value::Vector2d(Vector2d::default()),
            TypeTag::Vector3d => Value::Vector3d(Vector3d::default()),
            TypeTag::Vector4d => Value::Vector4d(Arc::default()),
            TypeTag::Quaternion => Value::Quaternion(Arc::default()),
            TypeTag::Url => Value::Url(Atom::EMPTY),
            TypeTag::Object(_) => Value::Null,
            TypeTag::List(_) => Value::List(Arc::default()),
            TypeTag::Enum(_) | TypeTag::Flags(_) => Value::Enum(EnumValue::default()),
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
        if matches!(v, Value::Undefined) && !matches!(self, TypeTag::Var) {
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
                Value::IntPoint(p) => Value::point(f64::from(p.x), f64::from(p.y)),
                Value::Vector2d(p) => Value::point(p.x.into(), p.y.into()),
                Value::Null => self.default_value(),
                Value::String(s) => match list(s) {
                    Some([x, y]) => Value::point(x, y),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Size => match v {
                Value::Size(_) => v.clone(),
                Value::String(s) => match extent(s) {
                    Some([w, h]) => Value::size(w, h),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Rect => match v {
                Value::Rect(_) => v.clone(),
                Value::String(s) => match area(s) {
                    Some([x, y, w, h]) => Value::rect(x, y, w, h),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Vector2d => match v {
                Value::Vector2d(_) => v.clone(),
                Value::Point(p) => Value::vector2d(p.x as f32, p.y as f32),
                Value::IntPoint(p) => Value::vector2d(p.x as f32, p.y as f32),
                Value::Vector3d(p) => Value::vector2d(p.x, p.y),
                Value::Vector4d(p) => Value::vector2d(p.x, p.y),
                Value::String(s) => match list(s) {
                    Some([x, y]) => Value::vector2d(x as f32, y as f32),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Vector3d => match v {
                Value::Vector3d(_) => v.clone(),
                Value::String(s) => match list(s) {
                    Some([x, y, z]) => Value::vector3d(x as f32, y as f32, z as f32),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Vector4d => match v {
                Value::Vector4d(_) => v.clone(),
                Value::String(s) => match list(s) {
                    Some([x, y, z, w]) => Value::vector4d(x as f32, y as f32, z as f32, w as f32),
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Quaternion => match v {
                Value::Quaternion(_) => v.clone(),
                Value::String(s) => match list(s) {
                    Some([scalar, x, y, z]) => {
                        Value::quaternion(scalar as f32, x as f32, y as f32, z as f32)
                    }
                    None => return fail(),
                },
                _ => return fail(),
            },
            TypeTag::Font => match v {
                Value::Font(_) => v.clone(),
                _ => return fail(),
            },
            TypeTag::Object(_) => match v {
                Value::Object(_) | Value::Null => v.clone(),
                _ => return fail(),
            },
            TypeTag::List(_) => match v {
                Value::List(_) => v.clone(),
                Value::Object(_) => Value::list(vec![v.clone()]),
                Value::Null => Value::List(Arc::default()),
                _ => return fail(),
            },
            TypeTag::Enum(_) | TypeTag::Flags(_) => match v {
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
pub(crate) fn describe(v: &Value) -> String {
    match v {
        Value::Undefined => "[undefined]".into(),
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Int(_) => "int".into(),
        Value::Real(_) => "double".into(),
        Value::String(_) => "QString".into(),
        Value::Color(_) => "QColor".into(),
        Value::Point(_) => "QPointF".into(),
        Value::IntPoint(_) => "QPoint".into(),
        Value::Size(_) => "QSizeF".into(),
        Value::Rect(_) => "QRectF".into(),
        Value::Font(_) => "QFont".into(),
        Value::Vector2d(_) => "QVector2D".into(),
        Value::Vector3d(_) => "QVector3D".into(),
        Value::Vector4d(_) => "QVector4D".into(),
        Value::Quaternion(_) => "QQuaternion".into(),
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

/// A wheel event's deltas: declared as `point`, held whole. Reading one
/// back from a real point rounds it, as Qt makes a `QPoint` of a
/// `QPointF`.
impl QmlValue for IntPoint {
    fn type_tag() -> TypeTag {
        TypeTag::Point
    }
    fn to_value(&self) -> Value {
        Value::IntPoint(*self)
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        Ok(match Self::type_tag().coerce(v)? {
            Value::Point(p) => IntPoint::rounded(p.x, p.y),
            Value::IntPoint(p) => p,
            _ => unreachable!("coerce returns the tag's own variant"),
        })
    }
}
qml_value!(Size, TypeTag::Size, |s| Value::Size(Arc::new(*s)), Value::Size(p) => *p);
qml_value!(Rect, TypeTag::Rect, |s| Value::Rect(Arc::new(*s)), Value::Rect(p) => *p);
qml_value!(Font, TypeTag::Font, |s| Value::Font(Arc::new(s.clone())), Value::Font(f) => (*f).clone());
qml_value!(Vector2d, TypeTag::Vector2d, |s| Value::Vector2d(*s), Value::Vector2d(v) => v);
qml_value!(Vector3d, TypeTag::Vector3d, |s| Value::Vector3d(*s), Value::Vector3d(v) => v);
qml_value!(Vector4d, TypeTag::Vector4d, |s| Value::Vector4d(Arc::new(*s)), Value::Vector4d(v) => *v);
qml_value!(Quaternion, TypeTag::Quaternion, |s| Value::Quaternion(Arc::new(*s)), Value::Quaternion(q) => *q);
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
        // A property refuses `undefined`; an argument or a value read out
        // of a `var` that holds none is no object.
        if matches!(v, Value::Undefined) {
            return Ok(None);
        }
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
    fn value_types_are_read_from_strings_and_from_each_other() {
        let to = |ty: &str, v: Value| {
            TypeTag::parse(ty)
                .coerce(&v)
                .map(|v| v.to_js_string().to_string())
        };
        let text = |s: &str| Value::string(s);
        assert_eq!(to("point", text("7,8")).unwrap(), "QPointF(7, 8)");
        assert_eq!(
            to("point", Value::vector2d(1.0, 2.0)).unwrap(),
            "QPointF(1, 2)"
        );
        assert_eq!(to("point", Value::Null).unwrap(), "QPointF(0, 0)");
        assert_eq!(to("size", text("7x8")).unwrap(), "QSizeF(7, 8)");
        assert_eq!(
            to("rect", text("1, 2, 3 x 4")).unwrap(),
            "QRectF(1, 2, 3, 4)"
        );
        assert_eq!(
            to("vector2d", Value::point(7.0, 8.0)).unwrap(),
            "QVector2D(7, 8)"
        );
        assert_eq!(
            to("vector2d", Value::vector3d(1.0, 2.0, 3.0)).unwrap(),
            "QVector2D(1, 2)"
        );
        assert_eq!(to("vector3d", text("7,8,9")).unwrap(), "QVector3D(7, 8, 9)");
        assert_eq!(
            to("vector4d", text("5,6,7,8")).unwrap(),
            "QVector4D(5, 6, 7, 8)"
        );
        assert_eq!(
            to("quaternion", text("5,6,7,8")).unwrap(),
            "QQuaternion(5, 6, 7, 8)"
        );
        assert_eq!(to("quaternion", Value::Null).ok(), None);
        assert_eq!(
            to("point", text("nope")).unwrap_err().to_string(),
            "Unable to assign QString to point"
        );
        assert!(to("point", Value::Int(5)).is_err());
        assert!(to("rect", text("1,2")).is_err());
        assert!(to("size", Value::Bool(true)).is_err());
        assert!(to("vector3d", Value::Int(5)).is_err());
        assert!(to("point", Value::size(1.0, 2.0)).is_err());
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
