//! The dynamic [`Value`] every property, argument and binding result goes
//! through (ARCHITECTURE.md §6.1), with ECMAScript coercions.
//!
//! # Layout
//!
//! `Value` is 16 bytes: a tag plus an 8-byte payload. Payloads that would
//! not fit inline are shared behind an `Arc` (`Point`, `Size`, `Rect`,
//! `Font`, `List`); they are immutable, so a clone is a reference-count
//! bump and a write builds a new one, which is what QML's value-type
//! write-back does anyway. Strings are [`Atom`]s (8 bytes, see
//! [`crate::atom`]). `Value` is `Send + Sync` so services can ship values in
//! messages; `Object` and `Js` ids only mean something on the engine's
//! thread.
//!
//! # Equality
//!
//! - [`Value::strict_equals`] is JS `===`: NaN is unequal to itself, `+0`
//!   equals `-0`, `Int` and `Real` compare as numbers, lists and JS handles
//!   compare by identity. Value types (`color`, `point`, `url`, …) compare
//!   by value with their own kind only, so `url === string` is false.
//! - `PartialEq` (`==` in Rust) is the reactive graph's change detection.
//!   It differs from `===` in one place: NaN equals NaN, so writing NaN
//!   twice does not notify twice. `+0`/`-0` are equal, as Qt's `double`
//!   comparison has them.

use std::fmt;
use std::sync::Arc;

use crate::atom::Atom;
use crate::ids::ObjectId;
use crate::number::{number_to_string, string_to_number};

/// A colour with 16 bits per channel, **not premultiplied**.
///
/// QML colours are straight alpha (`Qt.rgba(1, 0, 0, 0.5)` reads back as
/// red at half alpha); premultiplication happens at paint time. Sixteen
/// bits per channel matches QColor's precision, so values set from floats
/// read back the way Qt reads them, and four `u16`s fit `Value`'s payload.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u16,
    pub g: u16,
    pub b: u16,
    pub a: u16,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Color = Color::from_rgba8(0, 0, 0, 255);
    pub const WHITE: Color = Color::from_rgba8(255, 255, 255, 255);

    /// From 8-bit channels (`#rrggbb`). Each byte is replicated into 16 bits
    /// so 255 maps to 65535.
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        const fn w(c: u8) -> u16 {
            (c as u16) * 257
        }
        Color {
            r: w(r),
            g: w(g),
            b: w(b),
            a: w(a),
        }
    }

    /// From float channels in `0.0..=1.0` (`Qt.rgba`); out-of-range values
    /// are clamped.
    pub fn from_rgba_f(r: f64, g: f64, b: f64, a: f64) -> Self {
        fn w(c: f64) -> u16 {
            let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
            (c * 65535.0).round() as u16
        }
        Color {
            r: w(r),
            g: w(g),
            b: w(b),
            a: w(a),
        }
    }

    /// The 8-bit channels, rounded.
    pub fn to_rgba8(self) -> [u8; 4] {
        fn n(c: u16) -> u8 {
            ((u32::from(c) * 255 + 32767) / 65535) as u8
        }
        [n(self.r), n(self.g), n(self.b), n(self.a)]
    }

    /// Float channels in `0.0..=1.0`.
    pub fn to_rgba_f(self) -> [f64; 4] {
        let f = |c: u16| f64::from(c) / 65535.0;
        [f(self.r), f(self.g), f(self.b), f(self.a)]
    }

    /// Parses `#rgb`, `#argb`, `#rrggbb`, `#aarrggbb` (alpha first, as QML
    /// writes it) and the CSS/SVG colour keywords, case-insensitively.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            let digit = |i: usize| -> Option<u8> {
                hex.as_bytes()
                    .get(i)
                    .and_then(|c| (*c as char).to_digit(16))
                    .map(|d| d as u8)
            };
            let pair = |i: usize| -> Option<u8> { Some(digit(i)? * 16 + digit(i + 1)?) };
            let short = |i: usize| -> Option<u8> { Some(digit(i)? * 17) };
            return match hex.len() {
                3 => Some(Color::from_rgba8(short(0)?, short(1)?, short(2)?, 255)),
                4 => Some(Color::from_rgba8(
                    short(1)?,
                    short(2)?,
                    short(3)?,
                    short(0)?,
                )),
                6 => Some(Color::from_rgba8(pair(0)?, pair(2)?, pair(4)?, 255)),
                8 => Some(Color::from_rgba8(pair(2)?, pair(4)?, pair(6)?, pair(0)?)),
                _ => None,
            };
        }
        let lower = s.to_ascii_lowercase();
        if lower == "transparent" {
            return Some(Color::TRANSPARENT);
        }
        crate::color_names::lookup(&lower)
            .map(|rgb| Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255))
    }
}

impl fmt::Display for Color {
    /// `#rrggbb`, or `#aarrggbb` when not opaque: what QML prints for a
    /// colour converted to a string.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.to_rgba8();
        if a == 255 {
            write!(f, "#{r:02x}{g:02x}{b:02x}")
        } else {
            write!(f, "#{a:02x}{r:02x}{g:02x}{b:02x}")
        }
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Color({self})")
    }
}

/// `point` value type.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// `size` value type.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// `rect` value type.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `font` value type. Unset sizes are `-1`, as in QML.
///
/// This is the value, not a loaded face: `ljos_text` resolves it to a font
/// handle at layout time.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: Atom,
    pub point_size: f64,
    pub pixel_size: i32,
    /// CSS-style weight, 100..=900; 400 is normal, 700 bold.
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub letter_spacing: f64,
    pub word_spacing: f64,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            family: Atom::EMPTY,
            point_size: -1.0,
            pixel_size: -1,
            weight: 400,
            italic: false,
            underline: false,
            strikeout: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }
    }
}

impl Font {
    /// QML's `font.bold`: heavier than medium (500).
    pub fn bold(&self) -> bool {
        self.weight > 500
    }
}

/// A QML enum value. In JS it behaves as its number; the type is kept for
/// printing and for the compiler's type checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct EnumValue {
    /// Registry id of the enum type; 0 when unknown.
    pub ty: u32,
    pub value: i32,
}

/// A JS value owned by the engine's JS host, used by `var` properties
/// holding JS objects and functions. Equality is by reference: two handles
/// taken from one JS object at different times differ, which only costs a
/// spurious change notification.
#[derive(Clone, Debug)]
pub struct JsHandle(pub ljos_js::JsRef);

impl PartialEq for JsHandle {
    fn eq(&self, other: &Self) -> bool {
        ljos_js::JsRef::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for JsHandle {}

impl std::hash::Hash for JsHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id().hash(state);
    }
}

/// The dynamic value of a QML property, argument or binding result. See the
/// module docs for layout and equality.
#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    /// QML `int` (32-bit, as in Qt).
    Int(i32),
    /// QML `real`/`double`, JS `number`.
    Real(f64),
    String(Atom),
    Color(Color),
    Point(Arc<Point>),
    Size(Arc<Size>),
    Rect(Arc<Rect>),
    Font(Arc<Font>),
    /// A URL as written; resolution against the component's base URL
    /// happens where it is used.
    Url(Atom),
    Object(ObjectId),
    List(Arc<Vec<Value>>),
    Enum(EnumValue),
    Js(JsHandle),
}

const _: () = assert!(std::mem::size_of::<Value>() <= 16);

impl Value {
    /// A string value from anything string-like (uninterned).
    pub fn string(s: impl Into<Atom>) -> Self {
        Value::String(s.into())
    }

    pub fn point(x: f64, y: f64) -> Self {
        Value::Point(Arc::new(Point { x, y }))
    }

    pub fn size(width: f64, height: f64) -> Self {
        Value::Size(Arc::new(Size { width, height }))
    }

    pub fn rect(x: f64, y: f64, width: f64, height: f64) -> Self {
        Value::Rect(Arc::new(Rect {
            x,
            y,
            width,
            height,
        }))
    }

    pub fn list(items: Vec<Value>) -> Self {
        Value::List(Arc::new(items))
    }

    /// `null` or `undefined`.
    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    /// The number, if this is a JS number (`Int`, `Real` or `Enum`).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(f64::from(*i)),
            Value::Real(r) => Some(*r),
            Value::Enum(e) => Some(f64::from(e.value)),
            _ => None,
        }
    }

    /// The object id, if this is an object reference.
    pub fn as_object(&self) -> Option<ObjectId> {
        match self {
            Value::Object(id) => Some(*id),
            _ => None,
        }
    }

    /// The JS `typeof` of the value as the JS host would expose it.
    pub fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Bool(_) => "boolean",
            Value::Int(_) | Value::Real(_) | Value::Enum(_) => "number",
            Value::String(_) => "string",
            // Value types are wrapper objects in QML; urls are objects too.
            Value::Null
            | Value::Color(_)
            | Value::Point(_)
            | Value::Size(_)
            | Value::Rect(_)
            | Value::Font(_)
            | Value::Url(_)
            | Value::Object(_)
            | Value::List(_) => "object",
            // Could be a function; only the JS host knows.
            Value::Js(_) => "object",
        }
    }

    /// ECMAScript `ToBoolean`.
    ///
    /// `undefined`, `null`, `false`, `0`, `-0`, `NaN` and `""` are false.
    /// Objects, lists (even empty), colours and the other value types are
    /// true. A `Url` is false when empty, because an empty url reads as an
    /// empty string in the common `if (image.source)` idiom (to be verified
    /// against Qt by a `@qt-parity` scenario). JS handles are true; the host
    /// overrides that for primitive-wrapping handles if it ever makes them.
    pub fn to_boolean(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Real(r) => !(*r == 0.0 || r.is_nan()),
            Value::Enum(e) => e.value != 0,
            Value::String(s) | Value::Url(s) => !s.is_empty(),
            Value::Color(_)
            | Value::Point(_)
            | Value::Size(_)
            | Value::Rect(_)
            | Value::Font(_)
            | Value::Object(_)
            | Value::List(_)
            | Value::Js(_) => true,
        }
    }

    /// ECMAScript `ToNumber`.
    ///
    /// `undefined` is NaN, `null` is 0, booleans are 0/1, strings follow
    /// `StringToNumber` (`""` and blanks are 0, junk is NaN). A list goes
    /// through `ToPrimitive`, i.e. its joined string (`[]` is 0, `[5]` is 5).
    /// Objects and value types are NaN (their `valueOf` is the object).
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::Int(i) => f64::from(*i),
            Value::Real(r) => *r,
            Value::Enum(e) => f64::from(e.value),
            Value::String(s) | Value::Url(s) => string_to_number(s),
            Value::List(_) => string_to_number(&self.to_js_string()),
            Value::Color(_)
            | Value::Point(_)
            | Value::Size(_)
            | Value::Rect(_)
            | Value::Font(_)
            | Value::Object(_)
            | Value::Js(_) => f64::NAN,
        }
    }

    /// ECMAScript `ToString`, for what the engine can print alone.
    ///
    /// Numbers follow `Number::toString`; lists join their elements with
    /// `,` (`null`/`undefined` elements print empty). Value types print as
    /// QML does (`#rrggbb`, `QPointF(1, 2)`). Objects print a generic
    /// `[object Object]` here; [`crate::Engine::to_js_string`] prints the
    /// QML type name instead.
    pub fn to_js_string(&self) -> Atom {
        match self {
            Value::Undefined => Atom::intern("undefined"),
            Value::Null => Atom::intern("null"),
            Value::Bool(true) => Atom::intern("true"),
            Value::Bool(false) => Atom::intern("false"),
            Value::Int(i) => Atom::from(i.to_string()),
            Value::Real(r) => Atom::from(number_to_string(*r)),
            Value::Enum(e) => Atom::from(e.value.to_string()),
            Value::String(s) | Value::Url(s) => s.clone(),
            Value::Color(c) => Atom::from(c.to_string()),
            Value::Point(p) => Atom::from(format!(
                "QPointF({}, {})",
                number_to_string(p.x),
                number_to_string(p.y)
            )),
            Value::Size(s) => Atom::from(format!(
                "QSizeF({}, {})",
                number_to_string(s.width),
                number_to_string(s.height)
            )),
            Value::Rect(r) => Atom::from(format!(
                "QRectF({}, {}, {}, {})",
                number_to_string(r.x),
                number_to_string(r.y),
                number_to_string(r.width),
                number_to_string(r.height)
            )),
            Value::Font(f) => Atom::from(format!("QFont({})", f.family)),
            Value::List(items) => {
                let mut out = String::new();
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if !v.is_nullish() {
                        out.push_str(&v.to_js_string());
                    }
                }
                Atom::from(out)
            }
            Value::Object(_) | Value::Js(_) => Atom::intern("[object Object]"),
        }
    }

    /// JS `===`. See the module docs.
    pub fn strict_equals(&self, other: &Value) -> bool {
        if let (Some(a), Some(b)) = (self.as_number(), other.as_number()) {
            return a == b;
        }
        self.same_kind_equals(other)
    }

    /// Change detection for the reactive graph: `===` except that NaN
    /// equals NaN.
    pub fn change_equals(&self, other: &Value) -> bool {
        if let (Some(a), Some(b)) = (self.as_number(), other.as_number()) {
            return a == b || (a.is_nan() && b.is_nan());
        }
        self.same_kind_equals(other)
    }

    fn same_kind_equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) | (Value::Url(a), Value::Url(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::Point(a), Value::Point(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Size(a), Value::Size(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Rect(a), Value::Rect(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Font(a), Value::Font(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Object(a), Value::Object(b)) => a == b,
            // Arrays are references in JS.
            (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b),
            (Value::Js(a), Value::Js(b)) => a == b,
            _ => false,
        }
    }
}

impl PartialEq for Value {
    /// Change detection; see [`Value::change_equals`].
    fn eq(&self, other: &Self) -> bool {
        self.change_equals(other)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => f.write_str("undefined"),
            Value::Null => f.write_str("null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}i"),
            Value::Real(r) => write!(f, "{r:?}"),
            Value::String(s) => write!(f, "{s:?}"),
            Value::Color(c) => write!(f, "{c:?}"),
            Value::Point(p) => write!(f, "{p:?}"),
            Value::Size(s) => write!(f, "{s:?}"),
            Value::Rect(r) => write!(f, "{r:?}"),
            Value::Font(x) => write!(f, "{x:?}"),
            Value::Url(u) => write!(f, "url({u:?})"),
            Value::Object(o) => write!(f, "{o:?}"),
            Value::List(l) => f.debug_list().entries(l.iter()).finish(),
            Value::Enum(e) => write!(f, "enum({}:{})", e.ty, e.value),
            Value::Js(h) => write!(f, "{h:?}"),
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Real(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(Atom::new(v))
    }
}

impl From<Atom> for Value {
    fn from(v: Atom) -> Self {
        Value::String(v)
    }
}

impl From<Color> for Value {
    fn from(v: Color) -> Self {
        Value::Color(v)
    }
}

impl From<ObjectId> for Value {
    fn from(v: ObjectId) -> Self {
        Value::Object(v)
    }
}
