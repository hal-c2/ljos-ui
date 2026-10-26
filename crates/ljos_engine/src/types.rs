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
//! | `int` | numbers, bools, decimal strings, `null` | `ToNumber` then `ToInt32` (truncates, wraps; NaN is 0); a string is rounded, half away from 0, and is the least int past the range |
//! | `real` | numbers, bools, strings, `null` | `ToNumber` (`"abc"` is NaN) |
//! | `string` | anything but `undefined`, a point, size, rect or vector | `ToString`; `null` is `""`, NaN `"nan"`, a list a character per item, a font Qt's text of it |
//! | `color` | anything but `undefined` | `#rgb`, `#argb`, `#rrggbb`, `#aarrggbb`, CSS keywords; anything else is an invalid colour |
//! | `url` | urls, strings | |
//! | `point` | points, `"x,y"`, a vector's or a rect's x and y | |
//! | `size` | sizes, `"WxH"`, a rect's | |
//! | `rect` | rects, `"x,y,WxH"`, a size at the origin, a point with no size | |
//! | `vector2d`/`vector3d` | the same, numbers in a string, another vector's or a point's parts | |
//! | `vector4d`/`quaternion` | the same, numbers in a string | |
//! | `font` | fonts, a family's name | |
//!
//! A point, size, rect, `vector2d`, `vector3d` or font refuses a number, a
//! bool and a url; anything else it cannot be made from (a list, an
//! object, a colour) is its empty value, without a word, as in qml6.
//! | object types | objects, `null` | the engine checks the class |
//! | `list<T>` | lists, a single object (a list of one), `null` (empty) | |
//! | enums | numbers (a real is rounded), bools, the name of a key | the engine looks the key up in the property's type ([`crate::MetaObject::coerce_enum`]); the value is the number |
//!
//! `undefined` is rejected everywhere but `var`, matching Qt's "Unable to
//! assign [undefined] to …"; a property that can be taken back never gets
//! here with it (`crate::reset`). The `null`-to-`string` and
//! `null`-to-number cases are recorded as to be verified against Qt.

use std::borrow::Cow;
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
    /// A `var` that a Rust type reads as data (`Vec<T>`, `Option<T>`,
    /// `HashMap<String, T>`, `serde_json::Value`): a script's plain object
    /// arrives as a [`Value::Map`] and its array as a [`Value::List`], all
    /// the way down, where a `var` keeps the script's own object.
    Data,
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
    /// A value type a module brings, by its name (value_custom.rs).
    Custom(Atom),
    /// No value (method returns).
    Void,
}

impl TypeTag {
    /// What `v`, held as a variant (a `Binding`'s `value`, a state's plain
    /// value), gives a property of this type before it converts as an
    /// assignment does: a number given an `int` is rounded rather than
    /// truncated (qml6). `None` for what gives nothing, so that the
    /// property keeps what it has: NaN given an `int`.
    pub fn from_variant(&self, v: Value) -> Option<Value> {
        match v {
            Value::Real(n) if *self == TypeTag::Int => variant_to_int(n).map(Value::Int),
            v => Some(v),
        }
    }

    /// The name as QML writes it and the type dump records it.
    pub fn name(&self) -> String {
        match self {
            TypeTag::Var | TypeTag::Data => "var".into(),
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
            TypeTag::Object(Some(n))
            | TypeTag::Enum(n)
            | TypeTag::Flags(n)
            | TypeTag::Group(n)
            | TypeTag::Custom(n) => n.to_string(),
            TypeTag::List(None) => "list".into(),
            TypeTag::List(Some(n)) => format!("list<{n}>"),
            TypeTag::Void => "void".into(),
        }
    }

    /// The C++ type qml6 names a property of this type by in a binding's
    /// warning (`Unable to assign QUrl to double`); the QML name where it
    /// is not one of these.
    pub fn qt_name(&self) -> String {
        match self {
            TypeTag::Real => "double",
            TypeTag::String => "QString",
            TypeTag::Color => "QColor",
            TypeTag::Point => "QPointF",
            TypeTag::Size => "QSizeF",
            TypeTag::Rect => "QRectF",
            TypeTag::Font => "QFont",
            TypeTag::Vector2d => "QVector2D",
            TypeTag::Vector3d => "QVector3D",
            TypeTag::Vector4d => "QVector4D",
            TypeTag::Quaternion => "QQuaternion",
            TypeTag::Url => "QUrl",
            _ => return self.name(),
        }
        .into()
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
                None if crate::value_custom::find(other).is_some() => {
                    TypeTag::Custom(Atom::intern(other))
                }
                None => TypeTag::Object(Some(Atom::intern(other))),
            },
        }
    }

    /// The value a property of this type has before anything is assigned.
    pub fn default_value(&self) -> Value {
        match self {
            TypeTag::Var | TypeTag::Data | TypeTag::Group(_) | TypeTag::Void => Value::Undefined,
            TypeTag::Bool => Value::Bool(false),
            TypeTag::Int => Value::Int(0),
            TypeTag::Real => Value::Real(0.0),
            TypeTag::String => Value::String(Atom::EMPTY),
            // QML's default colour is invalid; it reads as opaque black.
            TypeTag::Color => Value::Color(Color::UNWRITTEN),
            TypeTag::Point => Value::Point(Arc::default()),
            TypeTag::Size => Value::size(-1.0, -1.0),
            TypeTag::Rect => Value::Rect(Arc::default()),
            TypeTag::Font => Value::Font(Arc::new(Font::unwritten())),
            TypeTag::Vector2d => Value::Vector2d(Vector2d::default()),
            TypeTag::Vector3d => Value::Vector3d(Vector3d::default()),
            TypeTag::Vector4d => Value::Vector4d(Arc::default()),
            TypeTag::Quaternion => Value::Quaternion(Arc::default()),
            TypeTag::Url => Value::Url(Atom::EMPTY),
            TypeTag::Object(_) => Value::Null,
            TypeTag::List(_) => Value::List(Arc::default()),
            TypeTag::Enum(_) | TypeTag::Flags(_) => Value::Enum(EnumValue::default()),
            TypeTag::Custom(name) => crate::value_custom::default_of(name),
        }
    }

    /// Converts `v` for assignment to a property of this type. See the
    /// module docs for the rules. Object class checks are the engine's.
    pub fn coerce(&self, v: &Value) -> Result<Value, ConvertError> {
        let fail = || Err(ConvertError::new(describe(v), self.qt_name()));
        if matches!(v, Value::Undefined) && !matches!(self, TypeTag::Var | TypeTag::Data) {
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
            TypeTag::Var | TypeTag::Data => v.clone(),
            TypeTag::Bool => match v {
                Value::Bool(_) => v.clone(),
                _ => Value::Bool(v.to_boolean()),
            },
            TypeTag::Int => match v {
                Value::Int(_) => v.clone(),
                Value::String(s) => match string_to_int(s) {
                    Some(n) => Value::Int(n),
                    None => return fail(),
                },
                _ => match numeric(v) {
                    Some(n) => Value::Int(to_int32(n)),
                    None => return fail(),
                },
            },
            TypeTag::Real => match v {
                Value::Real(_) => v.clone(),
                _ => match numeric(v) {
                    Some(n) => Value::Real(n),
                    // What JavaScript makes a number of it; a script's own
                    // object is made one before it comes here.
                    None => match v {
                        Value::List(items) => match &items[..] {
                            [] => Value::Real(0.0),
                            // An array is the number its text is.
                            [Value::Null | Value::Undefined] => Value::Real(0.0),
                            [one] => Value::Real(Value::String(one.to_js_string()).to_number()),
                            _ => Value::Real(f64::NAN),
                        },
                        Value::Js(_) => return fail(),
                        _ => Value::Real(f64::NAN),
                    },
                },
            },
            TypeTag::String => match v {
                Value::String(_) => v.clone(),
                Value::Null => Value::String(Atom::EMPTY),
                // As C++ writes a double that is no number.
                Value::Real(n) if n.is_nan() => Value::String(Atom::intern("nan")),
                Value::Real(n) if n.is_infinite() => {
                    Value::String(Atom::intern(if *n > 0.0 { "inf" } else { "-inf" }))
                }
                // A list is a character for each item: a number's code, a
                // text's first character, and 0 for anything else.
                Value::List(items) => Value::String(Atom::from(String::from_utf16_lossy(
                    &items.iter().map(char_code).collect::<Vec<_>>(),
                ))),
                Value::Font(f) => Value::String(Atom::from(crate::value_types::font_string(f))),
                Value::Point(_)
                | Value::IntPoint(_)
                | Value::Size(_)
                | Value::Rect(_)
                | Value::Vector2d(_)
                | Value::Vector3d(_)
                | Value::Vector4d(_)
                | Value::Quaternion(_) => return fail(),
                // An object, though it has a toString, as qml6 refuses it
                // and names it in a binding's warning.
                Value::Map(_) | Value::Js(_) => {
                    return Err(ConvertError::new("QJSValue", "QString"));
                }
                Value::Object(_) => {
                    return Err(ConvertError::new("QObject", "QString"));
                }
                _ => Value::String(v.to_js_string()),
            },
            TypeTag::Color => match v {
                Value::Color(c) => Value::Color(c.written()),
                // Text that names no colour, `""` too, and anything else makes
                // an invalid colour without a word, as in qml6. A literal one
                // is refused when the file is compiled, before this.
                Value::String(s) => Value::Color(Color::parse(s).unwrap_or(Color::INVALID)),
                // A script's function: its object is read before it comes here.
                Value::Js(_) => return fail(),
                _ => Value::Color(Color::INVALID),
            },
            TypeTag::Url => match v {
                Value::Url(_) => v.clone(),
                Value::String(s) => Value::Url(match crate::url_text::url_text(s) {
                    Some(Cow::Borrowed(_)) => s.clone(),
                    Some(Cow::Owned(t)) => t.into(),
                    None => Atom::EMPTY,
                }),
                _ => return fail(),
            },
            TypeTag::Point => match v {
                Value::Point(_) => v.clone(),
                Value::String(s) => match list(s) {
                    Some([x, y]) => Value::point(x, y),
                    None => return fail(),
                },
                _ if refused_as_value(v) => return fail(),
                _ => match spread(v) {
                    Some([x, y, _]) => Value::point(x, y),
                    None => self.default_value(),
                },
            },
            TypeTag::Size => match v {
                Value::Size(_) => v.clone(),
                Value::Rect(r) => Value::size(r.width, r.height),
                Value::String(s) => match extent(s) {
                    Some([w, h]) => Value::size(w, h),
                    None => return fail(),
                },
                _ if refused_as_value(v) => return fail(),
                _ => self.default_value(),
            },
            TypeTag::Rect => match v {
                Value::Rect(_) => v.clone(),
                Value::Size(s) => Value::rect(0.0, 0.0, s.width, s.height),
                Value::String(s) => match area(s) {
                    Some([x, y, w, h]) => Value::rect(x, y, w, h),
                    None => return fail(),
                },
                _ if refused_as_value(v) => return fail(),
                _ => match spread(v) {
                    Some([x, y, _]) => Value::rect(x, y, 0.0, 0.0),
                    None => self.default_value(),
                },
            },
            TypeTag::Vector2d => match v {
                Value::Vector2d(_) => v.clone(),
                // Text that is no vector is the empty one, without a word.
                Value::String(s) => match list(s) {
                    Some([x, y]) => Value::vector2d(x as f32, y as f32),
                    None => self.default_value(),
                },
                _ if refused_as_value(v) => return fail(),
                _ => match spread(v) {
                    Some([x, y, _]) => Value::vector2d(x as f32, y as f32),
                    None => self.default_value(),
                },
            },
            TypeTag::Vector3d => match v {
                Value::Vector3d(_) => v.clone(),
                Value::String(s) => match list(s) {
                    Some([x, y, z]) => Value::vector3d(x as f32, y as f32, z as f32),
                    None => self.default_value(),
                },
                _ if refused_as_value(v) => return fail(),
                _ => match spread(v) {
                    Some([x, y, z]) => Value::vector3d(x as f32, y as f32, z as f32),
                    None => self.default_value(),
                },
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
                Value::Font(f) if f.is_unwritten() => Value::Font(Arc::new(Font {
                    unwritten: Default::default(),
                    ..(**f).clone()
                })),
                Value::Font(_) => v.clone(),
                // A text is the family of a font that is otherwise the
                // default one.
                Value::String(s) => Value::Font(Arc::new(Font {
                    family: s.clone(),
                    ..Font::default()
                })),
                _ if refused_as_value(v) => return fail(),
                _ => Value::Font(Arc::default()),
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
            TypeTag::Custom(name) => match crate::value_custom::coerce(name, v) {
                Some(v) => v,
                None => return fail(),
            },
            TypeTag::Group(_) | TypeTag::Void => return fail(),
        })
    }
}

/// What qml6 refuses to make a point, size, rect, vector or font of, with a
/// warning: what is a number or a bool, a url, and a script's function or
/// Date (a plain object of the script's is read for its fields before it
/// comes here). Anything else it cannot make one of (a list, an object, a
/// colour) is the type's empty value, without a word.
fn refused_as_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Bool(_)
            | Value::Int(_)
            | Value::Real(_)
            | Value::Enum(_)
            | Value::Url(_)
            | Value::Object(_)
            | Value::Custom(_)
            | Value::Js(_)
    )
}

/// The first three numbers of a value with an x and a y (a point, a
/// vector, a quaternion's vector part, a rect's corner), the third 0
/// where there is none.
fn spread(v: &Value) -> Option<[f64; 3]> {
    Some(match v {
        Value::Point(p) => [p.x, p.y, 0.0],
        Value::IntPoint(p) => [p.x.into(), p.y.into(), 0.0],
        Value::Rect(r) => [r.x, r.y, 0.0],
        Value::Vector2d(p) => [p.x.into(), p.y.into(), 0.0],
        Value::Vector3d(p) => [p.x.into(), p.y.into(), p.z.into()],
        Value::Vector4d(p) => [p.x.into(), p.y.into(), p.z.into()],
        Value::Quaternion(q) => [q.x.into(), q.y.into(), q.z.into()],
        _ => return None,
    })
}

/// The UTF-16 code an item of a list is as a character of a string.
fn char_code(v: &Value) -> u16 {
    match v {
        Value::String(s) => s.encode_utf16().next().unwrap_or(0),
        Value::Int(_) | Value::Real(_) | Value::Enum(_) | Value::Bool(_) => {
            to_int32(v.to_number()) as u16
        }
        _ => 0,
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
        // A script's number is an int while it is a whole one.
        Value::Real(n) if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => "int".into(),
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
        Value::List(_) | Value::Map(_) => "QJSValue".into(),
        Value::Enum(_) => "int".into(),
        Value::Js(_) => "QJSValue".into(),
        Value::Custom(c) => c.ty().name().into(),
    }
}

/// A value could not be assigned to a typed property or argument.
///
/// A scalar says it as Qt does (`Unable to assign [undefined] to double`).
/// Data (`serde_json::Value`, `Vec<T>`, maps) says where in what it was
/// given, in the Rust author's terms: `Sink.json: payload.item is a QML
/// object, not JSON`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertError {
    pub from: String,
    pub to: String,
    /// For data, the path below the value (`.item`, `[2].name`), empty
    /// for the value itself; `None` for a scalar.
    pub at: Option<String>,
    /// The method and parameter a data argument was for.
    pub call: Option<(String, String)>,
}

impl ConvertError {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            at: None,
            call: None,
        }
    }

    /// Data refused `v` as `to` ("JSON", "a list").
    pub fn data(v: &Value, to: impl Into<String>) -> Self {
        Self {
            at: Some(String::new()),
            ..Self::new(describe_data(v), to)
        }
    }

    /// The error of a part of data, at `step` (`.item`, `[2]`) in it.
    #[must_use]
    pub fn within(mut self, step: &str) -> Self {
        self.at = Some(format!("{step}{}", self.at.as_deref().unwrap_or("")));
        self
    }

    /// Names the method (`Sink.json`) and parameter (`payload`) a data
    /// argument was for; a scalar's error is left as Qt words it.
    #[must_use]
    pub fn in_call(mut self, method: &str, param: &str) -> Self {
        if self.at.is_some() {
            self.call = Some((method.into(), param.into()));
        }
        self
    }
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(at) = &self.at else {
            return write!(f, "Unable to assign {} to {}", self.from, self.to);
        };
        let param = match &self.call {
            Some((method, param)) => {
                write!(f, "{method}: ")?;
                param.as_str()
            }
            None => "value",
        };
        write!(f, "{param}{at} is {}, not {}", self.from, self.to)
    }
}

impl std::error::Error for ConvertError {}

/// What a number held as a variant gives an `int`, as qml6 converts one:
/// rounded half away from 0, past the range of a 64-bit int the nearest
/// one, and cut to its low 32 bits. NaN gives nothing.
fn variant_to_int(n: f64) -> Option<i32> {
    // `as` saturates a double past i64's range, and then wraps to i32.
    (!n.is_nan()).then(|| n.round() as i64 as i32)
}

/// What an `int` makes of a text, as Qt does: a decimal number, rounded
/// half away from 0, and past the range of an int the least one, as a
/// double cast to one is. `nan` and `inf`, signed or not, are such
/// doubles; `infinity` and a number too great for a double are no number.
fn string_to_int(s: &str) -> Option<i32> {
    let t = s.trim();
    let word = t.to_ascii_lowercase();
    if matches!(word.as_str(), "nan" | "inf" | "+inf" | "-inf") {
        return Some(i32::MIN);
    }
    if word.contains("nan") || word.contains("inf") {
        return None;
    }
    let n = t.parse::<f64>().ok().filter(|n| n.is_finite())?.round();
    Some(if n < f64::from(i32::MIN) || n > f64::from(i32::MAX) {
        i32::MIN
    } else {
        n as i32
    })
}

/// What a value is, for a data error: in a script's words, not Qt's.
fn describe_data(v: &Value) -> String {
    match v {
        Value::Undefined => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(_) => "a bool".into(),
        Value::Int(_) | Value::Real(_) | Value::Enum(_) => "a number".into(),
        Value::String(_) => "a string".into(),
        Value::Url(_) => "a url".into(),
        Value::Color(_) => "a color".into(),
        Value::Object(_) => "a QML object".into(),
        Value::List(_) => "a list".into(),
        Value::Map(_) => "an object".into(),
        Value::Js(_) => "a function".into(),
        Value::Point(_)
        | Value::IntPoint(_)
        | Value::Size(_)
        | Value::Rect(_)
        | Value::Font(_)
        | Value::Vector2d(_)
        | Value::Vector3d(_)
        | Value::Vector4d(_)
        | Value::Quaternion(_)
        | Value::Custom(_) => format!("a {}", describe(v)),
    }
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
                // Nearly always the tag's own variant already, which
                // coercing would only copy: a field set after the engine
                // coerced the write.
                if let $pat = v.clone() {
                    return Ok($from);
                }
                match Self::type_tag().coerce(v)? {
                    $pat => Ok($from),
                    _ => unreachable!("coerce returns the tag's own variant"),
                }
            }
        }
    };
    // A plain-data payload is read in place: cloning the value first costs
    // an animated write as much as the rest of its setter.
    (copy $t:ty, $tag:expr, |$s:ident| $to:expr, $pat:pat => $from:expr) => {
        impl QmlValue for $t {
            fn type_tag() -> TypeTag {
                $tag
            }
            fn to_value(&self) -> Value {
                let $s = self;
                $to
            }
            fn from_value(v: &Value) -> Result<Self, ConvertError> {
                if let $pat = *v {
                    return Ok($from);
                }
                match Self::type_tag().coerce(v)? {
                    $pat => Ok($from),
                    _ => unreachable!("coerce returns the tag's own variant"),
                }
            }
        }
    };
}

qml_value!(copy bool, TypeTag::Bool, |s| Value::Bool(*s), Value::Bool(b) => b);
qml_value!(copy i32, TypeTag::Int, |s| Value::Int(*s), Value::Int(i) => i);
qml_value!(copy f64, TypeTag::Real, |s| Value::Real(*s), Value::Real(r) => r);
qml_value!(copy f32, TypeTag::Real, |s| Value::Real(f64::from(*s)), Value::Real(r) => r as f32);
qml_value!(Atom, TypeTag::String, |s| Value::String(s.clone()), Value::String(a) => a);
qml_value!(String, TypeTag::String, |s| Value::String(Atom::new(s)), Value::String(a) => a.to_string());
qml_value!(copy Color, TypeTag::Color, |s| Value::Color(*s), Value::Color(c) => c);
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

impl QmlValue for ObjectId {
    fn type_tag() -> TypeTag {
        TypeTag::Object(None)
    }
    fn to_value(&self) -> Value {
        Value::Object(*self)
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        Self::type_tag()
            .coerce(v)?
            .as_object()
            .ok_or_else(|| convert_error(v, "QObject*"))
    }
}

fn convert_error(v: &Value, to: &str) -> ConvertError {
    ConvertError::new(describe(v), to)
}

/// A list: `list` for a `Vec<Value>`, which keeps a script's objects as
/// the script's own; for any other item type data ([`TypeTag::Data`]),
/// so a list of records arrives as one.
///
/// `undefined` (an argument left out) and `null` are the empty list, and
/// one value that is not a list is a list of it, as a QML `list`
/// property takes one: `names("a")` is `["a"]`. Use `Option<Vec<T>>` to
/// tell a list left out from an empty one.
impl<T: QmlValue> QmlValue for Vec<T> {
    fn type_tag() -> TypeTag {
        match T::type_tag() {
            TypeTag::Var => TypeTag::List(None),
            _ => TypeTag::Data,
        }
    }
    fn to_value(&self) -> Value {
        Value::list(self.iter().map(T::to_value).collect())
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        let item =
            |(i, v): (usize, &Value)| T::from_value(v).map_err(|e| e.within(&format!("[{i}]")));
        match v {
            Value::Undefined | Value::Null => Ok(Vec::new()),
            Value::List(items) => items.iter().enumerate().map(item).collect(),
            _ => Ok(vec![T::from_value(v)?]),
        }
    }
}

/// `None` is `null`, and `null` or `undefined` is `None`. An object stays
/// an object type; any other is data, so `null` is not made a `T`.
impl<T: QmlValue> QmlValue for Option<T> {
    fn type_tag() -> TypeTag {
        match T::type_tag() {
            t @ (TypeTag::Object(_) | TypeTag::Var) => t,
            _ => TypeTag::Data,
        }
    }
    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Null, T::to_value)
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        match v {
            Value::Undefined | Value::Null => Ok(None),
            _ => T::from_value(v).map(Some),
        }
    }
}

/// A record as a map of its fields. A script's plain object arrives as
/// one; a script is given a new plain object with the fields in key
/// order.
impl<T: QmlValue, S: std::hash::BuildHasher + Default + 'static> QmlValue
    for std::collections::HashMap<String, T, S>
{
    fn type_tag() -> TypeTag {
        TypeTag::Data
    }
    fn to_value(&self) -> Value {
        let mut fields: Vec<(&String, &T)> = self.iter().collect();
        fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
        Value::map(fields.into_iter().map(|(k, v)| (k.as_str(), v.to_value())))
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        record_fields(v)?
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_string(),
                    T::from_value(v).map_err(|e| e.within(&field(k)))?,
                ))
            })
            .collect()
    }
}

/// As the `HashMap`, in key order.
impl<T: QmlValue> QmlValue for std::collections::BTreeMap<String, T> {
    fn type_tag() -> TypeTag {
        TypeTag::Data
    }
    fn to_value(&self) -> Value {
        Value::map(self.iter().map(|(k, v)| (k.as_str(), v.to_value())))
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        record_fields(v)?
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_string(),
                    T::from_value(v).map_err(|e| e.within(&field(k)))?,
                ))
            })
            .collect()
    }
}

fn record_fields(v: &Value) -> Result<&[(Atom, Value)], ConvertError> {
    v.as_map().ok_or_else(|| ConvertError::data(v, "a map"))
}

/// The step to a field in a data error's path: `.name`, or `["a b"]`
/// for a key that is not a name.
fn field(key: &str) -> String {
    let name = key
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    if name {
        format!(".{key}")
    } else {
        format!("[{key:?}]")
    }
}

/// JSON as data: objects are records, arrays lists, numbers that fit an
/// `int` are one. Coming back, it reads what `JSON.stringify` would
/// write: a field that is `undefined` or a function is left out, and in
/// a list or on its own (an argument left out) either is `null`. A
/// number JSON cannot write (NaN, an infinity) is `null`, and a url,
/// colour or enum is its string or number.
///
/// A QML object or a value type (a `point`, a `font`) is refused, with
/// the path to it (`Sink.json: payload.item is a QML object, not JSON`):
/// it has no JSON form that would read back as the same thing. Take a
/// [`Value`] to be handed one.
///
/// A JSON object stored in a property or a `PropertyMap` entry is one
/// script object until it is stored again: reads share it, and a field
/// read does not copy it.
impl QmlValue for serde_json::Value {
    fn type_tag() -> TypeTag {
        TypeTag::Data
    }
    fn to_value(&self) -> Value {
        use serde_json::Value as J;
        match self {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
                Some(i) => Value::Int(i),
                None => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
            },
            J::String(s) => Value::from(s.as_str()),
            J::Array(items) => Value::list(items.iter().map(Self::to_value).collect()),
            J::Object(fields) => Value::map(fields.iter().map(|(k, v)| (k.as_str(), v.to_value()))),
        }
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        use serde_json::Value as J;
        let number = |n: f64| {
            if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
                J::from(n as i64)
            } else {
                serde_json::Number::from_f64(n).map_or(J::Null, J::Number)
            }
        };
        Ok(match v {
            // In a list, as `JSON.stringify` writes them.
            Value::Undefined | Value::Null | Value::Js(_) => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Int(i) => J::from(*i),
            Value::Real(r) => number(*r),
            Value::Enum(e) => J::from(e.value),
            Value::String(s) | Value::Url(s) => J::String(s.to_string()),
            Value::Color(_) => J::String(v.to_js_string().to_string()),
            Value::List(items) => J::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Self::from_value(v).map_err(|e| e.within(&format!("[{i}]"))))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Map(fields) => J::Object(
                fields
                    .iter()
                    // A script's function arrives as its own value.
                    .filter(|(_, v)| !matches!(v, Value::Undefined | Value::Js(_)))
                    .map(|(k, v)| {
                        let v = Self::from_value(v).map_err(|e| e.within(&field(k)))?;
                        Ok((k.to_string(), v))
                    })
                    .collect::<Result<_, ConvertError>>()?,
            ),
            _ => return Err(ConvertError::data(v, "JSON")),
        })
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
    fn a_variant_number_gives_an_int_as_qml6_does() {
        for (n, int) in [
            (0.5, Some(1)),
            (-0.5, Some(-1)),
            (2.5, Some(3)),
            (0.49999999999999994, Some(0)),
            (2147483647.5, Some(i32::MIN)),
            (-2147483648.5, Some(i32::MAX)),
            (4294967296.5, Some(1)),
            (1e10, Some(1410065408)),
            (9.2e18, Some(-1650982912)),
            (9.3e18, Some(-1)),
            (f64::INFINITY, Some(-1)),
            (-9.3e18, Some(0)),
            (f64::NEG_INFINITY, Some(0)),
            (f64::NAN, None),
        ] {
            assert_eq!(variant_to_int(n), int, "{n}");
        }
    }

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
        // Text that names no colour is an invalid colour, as in qml6.
        assert_eq!(Color::from_value(&Value::string("#12")), Ok(Color::INVALID));
        assert_eq!(Color::from_value(&Value::Bool(true)), Ok(Color::INVALID));
        assert_eq!(<Option<ObjectId>>::from_value(&Value::Undefined), Ok(None));
        let err = f64::from_value(&Value::Undefined).unwrap_err();
        assert_eq!(err.to_string(), "Unable to assign [undefined] to double");
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
            "Unable to assign QString to QPointF"
        );
        assert!(to("point", Value::Int(5)).is_err());
        assert!(to("rect", text("1,2")).is_err());
        assert!(to("size", Value::Bool(true)).is_err());
        assert!(to("vector3d", Value::Int(5)).is_err());
        // What has no corner is the empty point, without a word.
        assert_eq!(to("point", Value::size(1.0, 2.0)).unwrap(), "QPointF(0, 0)");
        assert_eq!(
            to("rect", Value::size(1.0, 2.0)).unwrap(),
            "QRectF(0, 0, 1, 2)"
        );
        assert_eq!(
            to("point", Value::rect(1.0, 2.0, 3.0, 4.0)).unwrap(),
            "QPointF(1, 2)"
        );
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
