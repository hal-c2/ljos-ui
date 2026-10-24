//! The dynamic [`Value`] every property, argument and binding result goes
//! through (ARCHITECTURE.md §6.1), with ECMAScript coercions.
//!
//! # Layout
//!
//! `Value` is 16 bytes: a tag plus an 8-byte payload. Payloads that would
//! not fit inline are shared behind an `Arc` (`Point`, `Size`, `Rect`,
//! `Font`, `Vector4d`, `Quaternion`, `List`, `Map`); they are immutable, so a
//! clone is a reference-count bump and a write builds a new one, which is
//! what QML's value-type write-back does anyway. The two small vectors are
//! single precision and fit beside the tag. Strings are [`Atom`]s (8 bytes, see
//! [`crate::atom`]). `Value` is `Send + Sync` so services can ship values in
//! messages; `Object` and `Js` ids only mean something on the engine's
//! thread.
//!
//! # Equality
//!
//! - [`Value::strict_equals`] is JS `===`: NaN is unequal to itself, `+0`
//!   equals `-0`, `Int` and `Real` compare as numbers, lists, maps and JS
//!   handles compare by identity. Value types (`color`, `point`, `url`, …) compare
//!   by value with their own kind only, so `url === string` is false.
//! - `PartialEq` (`==` in Rust) is the reactive graph's change detection.
//!   It differs from `===` in one place: NaN equals NaN, so writing NaN
//!   twice does not notify twice. `+0`/`-0` are equal, as Qt's `double`
//!   comparison has them.

use std::fmt;
use std::sync::Arc;

use ljos_runtime::{Platform, pixels_to_points, points_to_pixels};

use crate::atom::Atom;
use crate::ids::ObjectId;
use crate::number::{number_to_string, string_to_number};
use crate::types::TypeTag;
use crate::value_types::{IntPoint, Quaternion, Vector2d, Vector3d, Vector4d, font_text, named};

/// A colour with 16 bits per channel, **not premultiplied**.
///
/// QML colours are straight alpha (`Qt.rgba(1, 0, 0, 0.5)` reads back as
/// red at half alpha); premultiplication happens at paint time. Sixteen
/// bits per channel matches QColor's precision, so values set from floats
/// read back the way Qt reads them, and four `u16`s fit `Value`'s payload.
///
/// A colour may also be invalid: one that was never given ([`Color::INVALID`],
/// what a declared `property color` starts as, and what `""` makes). It
/// still has channels, as in Qt (black, opaque, unless an alpha was set on
/// it), so it reads and prints as those, but `valid` is false, it equals no
/// valid colour, and it paints as its channels do, as in qml6.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u16,
    pub g: u16,
    pub b: u16,
    pub a: u16,
    invalid: bool,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba16(0, 0, 0, 0);
    /// The colour that was not given.
    pub const INVALID: Color = Color {
        invalid: true,
        ..Color::rgba16(0, 0, 0, 65535)
    };
    pub const BLACK: Color = Color::from_rgba8(0, 0, 0, 255);
    pub const WHITE: Color = Color::from_rgba8(255, 255, 255, 255);

    /// From 16-bit channels.
    pub const fn rgba16(r: u16, g: u16, b: u16, a: u16) -> Self {
        Color {
            r,
            g,
            b,
            a,
            invalid: false,
        }
    }

    /// False for a colour that was not given ([`Color::INVALID`]).
    pub const fn is_valid(self) -> bool {
        !self.invalid
    }

    /// The same channels, invalid: what Qt makes of an invalid colour
    /// given a new alpha.
    pub const fn invalidated(self) -> Self {
        Color {
            invalid: true,
            ..self
        }
    }

    /// The same channels, valid: what an invalid colour paints (black,
    /// unless an alpha was set on it), and what Qt keeps where a property
    /// holds plain channels, as Text's colours do.
    pub const fn painted(self) -> Self {
        Color {
            invalid: false,
            ..self
        }
    }

    /// From 8-bit channels (`#rrggbb`). Each byte is replicated into 16 bits
    /// so 255 maps to 65535.
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        const fn w(c: u8) -> u16 {
            (c as u16) * 257
        }
        Color::rgba16(w(r), w(g), w(b), w(a))
    }

    /// From float channels in `0.0..=1.0` (`Qt.rgba`); out-of-range values
    /// are clamped.
    ///
    /// The rounding is done in single precision, as Qt does it, so a number
    /// such as 0.9271 lands on the same sixteen bits as there.
    pub fn from_rgba_f(r: f64, g: f64, b: f64, a: f64) -> Self {
        fn w(c: f64) -> u16 {
            let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
            (c as f32 * 65535.0).round() as u16
        }
        Color::rgba16(w(r), w(g), w(b), w(a))
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

    /// Parses the colour text QML reads (see [`ljos_ir::color::parse`]):
    /// `#rrggbb`, `#aarrggbb` (alpha first, as QML writes it), the other hex
    /// forms, and the CSS/SVG colour keywords.
    pub fn parse(s: &str) -> Option<Color> {
        ljos_ir::color::parse(s).map(|[r, g, b, a]| Color::rgba16(r, g, b, a))
    }
}

/// `Qt.lighter` and `Qt.darker` step for step, for what shades a colour
/// natively as a script would (a style's shades of its palette).
impl Color {
    /// Lighter by `percent` (150 is half as light again); below 100 it is
    /// darker. What does not fit in the value comes off the saturation.
    pub fn lighter(self, percent: f64) -> Color {
        // A NaN is no factor either.
        if percent.is_nan() || percent <= 0.0 || !self.is_valid() {
            return self;
        }
        if percent < 100.0 {
            return self.darker((10000.0 / percent).trunc());
        }
        let [h, mut s, mut v] = self.hsv16();
        v = (v * percent / 100.0).trunc();
        if v > U16 {
            s = (s - (v - U16)).max(0.0);
            v = U16;
        }
        self.with_hsv16(h, s, v)
    }

    /// Darker by `percent` (200 is half the value); below 100 it is lighter.
    pub fn darker(self, percent: f64) -> Color {
        if percent.is_nan() || percent <= 0.0 || !self.is_valid() {
            return self;
        }
        if percent < 100.0 {
            return self.lighter((10000.0 / percent).trunc());
        }
        let [h, s, v] = self.hsv16();
        self.with_hsv16(h, s, (v * 100.0 / percent).trunc())
    }

    /// The hue in hundredths of a degree (0 for a grey), and the saturation
    /// and value in 16 bits, as the colour's members have them.
    pub fn hsv16(self) -> [f64; 3] {
        let [r, g, b] = [self.r, self.g, self.b].map(f64::from);
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let (h, s) = if delta == 0.0 {
            (-1.0, 0.0)
        } else {
            let mut h = if r == max {
                (g - b) / delta
            } else if g == max {
                2.0 + (b - r) / delta
            } else {
                4.0 + (r - g) / delta
            };
            h *= 60.0;
            if h < 0.0 {
                h += 360.0;
            }
            let h = single((round(h * 100.0) % 36000.0) / 36000.0);
            (h, single(round(delta / max * U16) / U16))
        };
        let v = single(max / U16);
        [
            if h < 0.0 { 0.0 } else { round(h * 36000.0) },
            round(s * U16),
            round(v * U16),
        ]
    }

    /// The colour of a hue, saturation and value as [`Color::hsv16`] gives
    /// them, with this colour's alpha.
    pub fn with_hsv16(self, hue: f64, sat: f64, val: f64) -> Color {
        let h = single(hue / 6000.0);
        let i = h.floor();
        let f = single(h - i);
        let s = single(sat / U16);
        let v = single(val / U16);
        let ch = |x: f64| round(single(single(v * x) * U16));
        let p = ch(single(1.0 - s));
        let q = ch(single(1.0 - single(s * f)));
        let t = ch(single(1.0 - single(s * single(1.0 - f))));
        let a = round(single(f64::from(self.a) / U16) * U16);
        let (r, g, b) = match i as i64 {
            0 => (val, t, p),
            1 => (q, val, p),
            2 => (p, val, t),
            3 => (p, q, val),
            4 => (t, p, val),
            _ => (val, p, q),
        };
        let c16 = |n: f64| n.clamp(0.0, U16) as u16;
        Color::rgba16(c16(r), c16(g), c16(b), c16(a))
    }
}

const U16: f64 = 65535.0;

/// To single precision and back, as `Math.fround`.
fn single(x: f64) -> f64 {
    f64::from(x as f32)
}

/// `Math.round` of a number that is not negative: a half goes up.
fn round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
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
        if self.invalid {
            write!(f, "Color(invalid {self})")
        } else {
            write!(f, "Color({self})")
        }
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
/// The default is the application font, as in Qt: the desktop's when the
/// host read one ([`Font::set_application_font`]), else the platform's
/// default family at its default point size ([`Font::DEFAULT_FAMILY`],
/// [`Font::DEFAULT_POINT_SIZE`]). The fields are what was asked for;
/// [`Font::read_pixel_size`] and [`Font::read_point_size`] are what a
/// script reads, each size in both units.
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
    /// `Font.MixedCase` (0), `AllUppercase`, `AllLowercase`, `SmallCaps` or
    /// `Capitalize`: the letters the text is drawn in ([`Font::drawn`]).
    pub capitalization: i32,
    /// `Font.PreferDefaultHinting` (0) and the others. Kept; glyphs are
    /// hinted as the rasterizer does it.
    pub hinting_preference: i32,
    /// Ours, not Qt's: whether the text grows with the system's text size
    /// (`Qt.styleHints.accessibility.fontScale`). The sizes above stay as
    /// written either way; the scale is applied where text is laid out.
    pub scale_with_system: bool,
    /// `font.styleName`: kept as written. Faces are picked by weight and
    /// italic, so a style name changes nothing that is drawn.
    pub style_name: Atom,
    /// `font.overline`: a line above the text.
    pub overline: bool,
    /// `font.kerning`, `font.preferShaping`, `font.contextFontMerging` and
    /// `font.preferTypoLineMetrics`: kept and read back. Text is always
    /// kerned and shaped by the text stack, so these change nothing drawn.
    pub kerning: bool,
    pub prefer_shaping: bool,
    pub context_font_merging: bool,
    pub prefer_typo_line_metrics: bool,
    /// `font.features` and `font.variableAxes`: the JS maps as assigned
    /// (`Undefined` when never), kept and read back.
    pub features: Value,
    pub variable_axes: Value,
}

/// What a host set with [`Font::set_application_font`]: the part of a font
/// a desktop names, which unlike a whole `Font` may be shared by threads.
#[derive(Clone)]
struct ApplicationFont {
    family: Atom,
    point_size: f64,
    pixel_size: i32,
    weight: i32,
    italic: bool,
    style_name: Atom,
}

static APPLICATION_FONT: std::sync::RwLock<Option<ApplicationFont>> = std::sync::RwLock::new(None);
/// Whether [`APPLICATION_FONT`] is set: most programs never set it, and
/// every default font would otherwise take the lock.
static APPLICATION_FONT_SET: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// The application font of the engine on this thread
    /// ([`crate::Engine::set_default_font`]), over the process's: what a
    /// font made from now on starts from.
    static ENGINE_FONT: std::cell::RefCell<Option<ApplicationFont>> =
        const { std::cell::RefCell::new(None) };
}

/// The engine's hold on [`ENGINE_FONT`]: an engine that set it takes it
/// back when it goes, so the next engine on the thread starts from the
/// process's font.
#[derive(Default)]
pub(crate) struct EngineFont(std::cell::Cell<bool>);

impl EngineFont {
    pub(crate) fn set(&self, font: Option<&Font>) {
        self.0.set(font.is_some());
        let font = font.map(ApplicationFont::of);
        ENGINE_FONT.with(|f| *f.borrow_mut() = font);
    }
}

impl Drop for EngineFont {
    fn drop(&mut self) {
        if self.0.get() {
            let _ = ENGINE_FONT.try_with(|f| f.borrow_mut().take());
        }
    }
}

impl ApplicationFont {
    fn of(f: &Font) -> Self {
        ApplicationFont {
            family: f.family.clone(),
            point_size: f.point_size,
            pixel_size: f.pixel_size,
            weight: f.weight,
            italic: f.italic,
            style_name: f.style_name.clone(),
        }
    }

    fn lay_on(&self, font: &mut Font) {
        font.family = self.family.clone();
        font.point_size = self.point_size;
        font.pixel_size = self.pixel_size;
        font.weight = self.weight;
        font.italic = self.italic;
        font.style_name = self.style_name.clone();
    }
}

impl Default for Font {
    /// The font a new one starts from: the application font of the engine
    /// on this thread, else the process's ([`Font::process_default`]).
    fn default() -> Self {
        let mut font = Font::fallback();
        let laid = ENGINE_FONT.with(|f| match f.borrow().as_ref() {
            Some(app) => {
                app.lay_on(&mut font);
                true
            }
            None => false,
        });
        if laid {
            return font;
        }
        Font::process_default()
    }
}

impl Font {
    /// The process's application font ([`Font::set_application_font`]), or
    /// [`Font::fallback`] when the host set none: what an engine's starts
    /// from.
    pub fn process_default() -> Self {
        use std::sync::atomic::Ordering;
        let mut font = Font::fallback();
        if APPLICATION_FONT_SET.load(Ordering::Acquire)
            && let Ok(set) = APPLICATION_FONT.read()
            && let Some(app) = set.as_ref()
        {
            app.lay_on(&mut font);
        }
        font
    }

    /// The application font when no desktop names one: Qt's for the
    /// platform, [`Font::DEFAULT_FAMILY`] at
    /// [`Font::DEFAULT_POINT_SIZE`].
    pub fn fallback() -> Self {
        Font {
            family: Atom::DEFAULT_FONT_FAMILY,
            point_size: Font::DEFAULT_POINT_SIZE,
            pixel_size: -1,
            weight: 400,
            italic: false,
            underline: false,
            strikeout: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            capitalization: 0,
            hinting_preference: 0,
            scale_with_system: true,
            style_name: Atom::EMPTY,
            overline: false,
            kerning: true,
            prefer_shaping: true,
            context_font_merging: false,
            prefer_typo_line_metrics: false,
            features: Value::Undefined,
            variable_axes: Value::Undefined,
        }
    }

    /// Makes the family, sizes, weight, italics and style name of `font`
    /// those of every default font from now on (`QGuiApplication::setFont`),
    /// or with `None` [`Font::fallback`]'s again. A host calls it once, with
    /// the desktop's font, before it builds an engine: what was made
    /// before keeps the font it had. It is the process's; to change the
    /// font of a running application, so that the controls follow, use the
    /// engine's (`ljos::set_application_font`).
    pub fn set_application_font(font: Option<&Font>) {
        use std::sync::atomic::Ordering;
        let app = font.map(ApplicationFont::of);
        let set = app.is_some();
        if let Ok(mut held) = APPLICATION_FONT.write() {
            *held = app;
        }
        APPLICATION_FONT_SET.store(set, Ordering::Release);
    }
}

/// The keys of `Font.Capitalization`, by number.
const CAPITALIZATION: [&str; 5] = [
    "MixedCase",
    "AllUppercase",
    "AllLowercase",
    "SmallCaps",
    "Capitalize",
];
/// The keys of `Font.HintingPreference`, by number.
const HINTING_PREFERENCE: [&str; 4] = [
    "PreferDefaultHinting",
    "PreferNoHinting",
    "PreferVerticalHinting",
    "PreferFullHinting",
];

/// A key of an enum of the font value as it is assigned: its number, or its
/// name as a string, which Qt takes too.
fn font_key(v: &Value, names: &[&str]) -> i32 {
    match v {
        Value::String(s) => names
            .iter()
            .position(|n| *n == s.as_str())
            .map_or(0, |i| i as i32),
        _ => crate::number::to_int32(v.to_number()),
    }
}

impl Font {
    pub const MIXED_CASE: i32 = 0;
    pub const ALL_UPPERCASE: i32 = 1;
    pub const ALL_LOWERCASE: i32 = 2;
    pub const SMALL_CAPS: i32 = 3;
    pub const CAPITALIZE: i32 = 4;

    /// The family of the application font when no desktop names one, as
    /// Qt has it ([`Platform::default_family`]). On macOS it is the system
    /// UI font. Elsewhere it is Sans Serif, a name for the system's
    /// sans-serif face, which the platform's font configuration resolves
    /// when text is laid out (fontconfig on Unix, a bundled face where
    /// there is none).
    pub const DEFAULT_FAMILY: &str = Platform::CURRENT.default_family();
    /// The point size of the application font: 13 pt on macOS (13 px at
    /// its 72 dpi), 9 pt elsewhere (12 px at 96 dpi).
    pub const DEFAULT_POINT_SIZE: f64 = Platform::CURRENT.default_point_size();

    /// `font.pixelSize` as a script reads it: the pixel size asked for, or
    /// the point size in whole pixels (cut, not rounded, as Qt reads it;
    /// 11 pt reads 14), or `-1` when neither is set.
    pub fn read_pixel_size(&self) -> i32 {
        if self.pixel_size > 0 {
            self.pixel_size
        } else if self.point_size > 0.0 {
            points_to_pixels(self.point_size) as i32
        } else {
            -1
        }
    }

    /// `font.pointSize` as a script reads it: a pixel size, which wins over
    /// a point size, in points, else the point size asked for.
    pub fn read_point_size(&self) -> f64 {
        if self.pixel_size > 0 {
            pixels_to_points(f64::from(self.pixel_size))
        } else {
            self.point_size
        }
    }

    /// `text` in the letters `capitalization` asks for: what is laid out
    /// and drawn, while the text itself stays as written. Every character
    /// stays one character of as many UTF-16 units, so positions in the
    /// two are the same: a letter whose capital is several (`ß`) is left
    /// as it is, as Qt leaves it. Small capitals are not drawn; the text is
    /// as written.
    pub fn drawn<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        fn one(mut to: impl Iterator<Item = char>, from: char) -> char {
            match (to.next(), to.next()) {
                (Some(c), None) if c.len_utf16() == from.len_utf16() => c,
                _ => from,
            }
        }
        let upper = |c: char| one(c.to_uppercase(), c);
        match self.capitalization {
            Self::ALL_UPPERCASE => text.chars().map(upper).collect::<String>().into(),
            Self::ALL_LOWERCASE => text
                .chars()
                .map(|c| one(c.to_lowercase(), c))
                .collect::<String>()
                .into(),
            Self::CAPITALIZE => {
                let mut starts = true;
                text.chars()
                    .map(|c| {
                        let c = if starts { upper(c) } else { c };
                        starts = c.is_whitespace();
                        c
                    })
                    .collect::<String>()
                    .into()
            }
            _ => text.into(),
        }
    }

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
    /// A point of whole numbers: a wheel event's deltas. It compares
    /// equal to the `Point` with the same numbers, as qml6 has
    /// `wheel.angleDelta === Qt.point(0, 120)`, but prints as `QPoint`.
    IntPoint(IntPoint),
    Size(Arc<Size>),
    Rect(Arc<Rect>),
    Font(Arc<Font>),
    Vector2d(Vector2d),
    Vector3d(Vector3d),
    Vector4d(Arc<Vector4d>),
    Quaternion(Arc<Quaternion>),
    /// A URL as written; resolution against the component's base URL
    /// happens where it is used.
    Url(Atom),
    Object(ObjectId),
    List(Arc<Vec<Value>>),
    /// A record made in Rust (`{ path, size }`): a script is given a new
    /// plain object with these fields, in this order, as a [`Value::List`]
    /// gives it a new array. A value going to a data type
    /// ([`TypeTag::Data`]) brings a script's plain object back as one.
    Map(Arc<Vec<(Atom, Value)>>),
    Enum(EnumValue),
    Js(JsHandle),
}

const _: () = assert!(std::mem::size_of::<Value>() <= 16);

impl Value {
    /// The member `name` of this value that is not an object
    /// (`pixelSize` of a font, `x` of a point); `None` when it has none.
    pub fn member(&self, name: &str) -> Option<Value> {
        crate::incubator::tier1::member_of(self, name)
    }

    /// This value of the value type `ty` (`font`, `point`, `size`, `rect`,
    /// a vector, `quaternion`) with its member `name` (`pixelSize`) set to
    /// `v`: what a QML
    /// `font.pixelSize: 24` does. A value of another type starts from the
    /// type's default. `None` when `ty` has no member `name`.
    pub fn with_member(&self, ty: &TypeTag, name: &str, v: &Value) -> Option<Value> {
        let n = v.to_number();
        match ty {
            TypeTag::Font => {
                let mut f = match self {
                    Value::Font(f) => (**f).clone(),
                    _ => Font::default(),
                };
                match name {
                    "family" => f.family = v.to_js_string(),
                    // A font has one size. A pixel size takes its place;
                    // a point size does not, as in Qt, where the pixel size
                    // stays and wins.
                    "pointSize" if f.pixel_size > 0 => {}
                    "pointSize" => f.point_size = n,
                    "pixelSize" => {
                        f.pixel_size = crate::number::to_int32(n);
                        if f.pixel_size > 0 {
                            f.point_size = -1.0;
                        }
                    }
                    "weight" => f.weight = crate::number::to_int32(n),
                    "bold" => f.weight = if v.to_boolean() { 700 } else { 400 },
                    "italic" => f.italic = v.to_boolean(),
                    "underline" => f.underline = v.to_boolean(),
                    "strikeout" => f.strikeout = v.to_boolean(),
                    "letterSpacing" => f.letter_spacing = n,
                    "wordSpacing" => f.word_spacing = n,
                    "capitalization" => f.capitalization = font_key(v, &CAPITALIZATION),
                    "hintingPreference" => {
                        f.hinting_preference = font_key(v, &HINTING_PREFERENCE);
                    }
                    "scaleWithSystem" => f.scale_with_system = v.to_boolean(),
                    "styleName" => f.style_name = v.to_js_string(),
                    "overline" => f.overline = v.to_boolean(),
                    "kerning" => f.kerning = v.to_boolean(),
                    "preferShaping" => f.prefer_shaping = v.to_boolean(),
                    "contextFontMerging" => f.context_font_merging = v.to_boolean(),
                    "preferTypoLineMetrics" => f.prefer_typo_line_metrics = v.to_boolean(),
                    "features" => {
                        f.features = if matches!(v, Value::Js(_)) {
                            v.clone()
                        } else {
                            Value::Undefined
                        };
                    }
                    "variableAxes" => {
                        f.variable_axes = if matches!(v, Value::Js(_)) {
                            v.clone()
                        } else {
                            Value::Undefined
                        };
                    }
                    _ => return None,
                }
                Some(Value::Font(Arc::new(f)))
            }
            TypeTag::Point => {
                let mut p = match self {
                    Value::Point(p) => **p,
                    Value::IntPoint(p) => Point {
                        x: f64::from(p.x),
                        y: f64::from(p.y),
                    },
                    _ => Point::default(),
                };
                match name {
                    "x" => p.x = n,
                    "y" => p.y = n,
                    _ => return None,
                }
                Some(Value::Point(Arc::new(p)))
            }
            TypeTag::Size => {
                let mut z = match self {
                    Value::Size(z) => **z,
                    _ => Size::default(),
                };
                match name {
                    "width" => z.width = n,
                    "height" => z.height = n,
                    _ => return None,
                }
                Some(Value::Size(Arc::new(z)))
            }
            TypeTag::Rect => {
                let mut r = match self {
                    Value::Rect(r) => **r,
                    _ => Rect::default(),
                };
                match name {
                    "x" => r.x = n,
                    "y" => r.y = n,
                    "width" => r.width = n,
                    "height" => r.height = n,
                    _ => return None,
                }
                Some(Value::Rect(Arc::new(r)))
            }
            TypeTag::Vector2d => {
                let mut p = match self {
                    Value::Vector2d(p) => *p,
                    _ => Vector2d::default(),
                };
                match name {
                    "x" => p.x = n as f32,
                    "y" => p.y = n as f32,
                    _ => return None,
                }
                Some(Value::Vector2d(p))
            }
            TypeTag::Vector3d => {
                let mut p = match self {
                    Value::Vector3d(p) => *p,
                    _ => Vector3d::default(),
                };
                match name {
                    "x" => p.x = n as f32,
                    "y" => p.y = n as f32,
                    "z" => p.z = n as f32,
                    _ => return None,
                }
                Some(Value::Vector3d(p))
            }
            TypeTag::Vector4d => {
                let mut p = match self {
                    Value::Vector4d(p) => **p,
                    _ => Vector4d::default(),
                };
                match name {
                    "x" => p.x = n as f32,
                    "y" => p.y = n as f32,
                    "z" => p.z = n as f32,
                    "w" => p.w = n as f32,
                    _ => return None,
                }
                Some(Value::Vector4d(Arc::new(p)))
            }
            TypeTag::Quaternion => {
                let mut q = match self {
                    Value::Quaternion(q) => **q,
                    _ => Quaternion::default(),
                };
                match name {
                    "scalar" => q.scalar = n as f32,
                    "x" => q.x = n as f32,
                    "y" => q.y = n as f32,
                    "z" => q.z = n as f32,
                    _ => return None,
                }
                Some(Value::Quaternion(Arc::new(q)))
            }
            _ => None,
        }
    }

    /// A string value from anything string-like (uninterned).
    pub fn string(s: impl Into<Atom>) -> Self {
        Value::String(s.into())
    }

    pub fn point(x: f64, y: f64) -> Self {
        Value::Point(Arc::new(Point { x, y }))
    }

    pub fn int_point(x: i32, y: i32) -> Self {
        Value::IntPoint(IntPoint { x, y })
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

    pub fn vector2d(x: f32, y: f32) -> Self {
        Value::Vector2d(Vector2d { x, y })
    }

    pub fn vector3d(x: f32, y: f32, z: f32) -> Self {
        Value::Vector3d(Vector3d { x, y, z })
    }

    pub fn vector4d(x: f32, y: f32, z: f32, w: f32) -> Self {
        Value::Vector4d(Arc::new(Vector4d { x, y, z, w }))
    }

    pub fn quaternion(scalar: f32, x: f32, y: f32, z: f32) -> Self {
        Value::Quaternion(Arc::new(Quaternion { scalar, x, y, z }))
    }

    pub fn list(items: Vec<Value>) -> Self {
        Value::List(Arc::new(items))
    }

    /// A record of these fields, in this order.
    ///
    /// ```
    /// use ljos_engine::Value;
    ///
    /// let row = Value::map([("path", Value::from("/a")), ("size", Value::Int(3))]);
    /// assert_eq!(row.get("size"), Some(&Value::Int(3)));
    /// ```
    pub fn map<K: Into<Atom>>(fields: impl IntoIterator<Item = (K, Value)>) -> Self {
        Value::Map(Arc::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }

    /// The items, if this is a list.
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    /// The fields, if this is a record ([`Value::Map`]).
    pub fn as_map(&self) -> Option<&[(Atom, Value)]> {
        match self {
            Value::Map(fields) => Some(fields),
            _ => None,
        }
    }

    /// The field `key` of a record; `None` for a record without it and
    /// for any other value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
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
            | Value::IntPoint(_)
            | Value::Size(_)
            | Value::Rect(_)
            | Value::Font(_)
            | Value::Vector2d(_)
            | Value::Vector3d(_)
            | Value::Vector4d(_)
            | Value::Quaternion(_)
            | Value::Url(_)
            | Value::Object(_)
            | Value::List(_)
            | Value::Map(_) => "object",
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
            | Value::Vector2d(_)
            | Value::Vector3d(_)
            | Value::Vector4d(_)
            | Value::Quaternion(_)
            | Value::Object(_)
            | Value::List(_)
            | Value::Map(_)
            | Value::IntPoint(_)
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
            | Value::Vector2d(_)
            | Value::Vector3d(_)
            | Value::Vector4d(_)
            | Value::Quaternion(_)
            | Value::Object(_)
            | Value::Map(_)
            | Value::IntPoint(_)
            | Value::Js(_) => f64::NAN,
        }
    }

    /// ECMAScript `ToString`, for what the engine can print alone.
    ///
    /// Numbers follow `Number::toString`; lists join their elements with
    /// `,` (`null`/`undefined` elements print empty). Value types print as
    /// QML does (`#rrggbb`, `QPointF(1, 2)`, their numbers as
    /// [`crate::value_types::general`] writes them). Objects print a generic
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
            Value::Point(p) => Atom::from(named("QPointF", &[p.x, p.y])),
            Value::IntPoint(p) => Atom::from(format!("QPoint({}, {})", p.x, p.y)),
            Value::Size(s) => Atom::from(named("QSizeF", &[s.width, s.height])),
            Value::Rect(r) => Atom::from(named("QRectF", &[r.x, r.y, r.width, r.height])),
            Value::Font(f) => Atom::from(font_text(f)),
            Value::Vector2d(v) => Atom::from(named("QVector2D", &[v.x.into(), v.y.into()])),
            Value::Vector3d(v) => {
                Atom::from(named("QVector3D", &[v.x.into(), v.y.into(), v.z.into()]))
            }
            Value::Vector4d(v) => Atom::from(named(
                "QVector4D",
                &[v.x.into(), v.y.into(), v.z.into(), v.w.into()],
            )),
            Value::Quaternion(q) => Atom::from(named(
                "QQuaternion",
                &[q.scalar.into(), q.x.into(), q.y.into(), q.z.into()],
            )),
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
            Value::Object(_) | Value::Map(_) | Value::Js(_) => Atom::intern("[object Object]"),
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
            (Value::IntPoint(a), Value::IntPoint(b)) => a == b,
            // qml6: `wheel.angleDelta === Qt.point(0, 120)` is true.
            (Value::IntPoint(a), Value::Point(b)) | (Value::Point(b), Value::IntPoint(a)) => {
                f64::from(a.x) == b.x && f64::from(a.y) == b.y
            }
            (Value::Size(a), Value::Size(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Rect(a), Value::Rect(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Font(a), Value::Font(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Vector2d(a), Value::Vector2d(b)) => a == b,
            (Value::Vector3d(a), Value::Vector3d(b)) => a == b,
            (Value::Vector4d(a), Value::Vector4d(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Quaternion(a), Value::Quaternion(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Object(a), Value::Object(b)) => a == b,
            // Arrays are references in JS.
            (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b),
            (Value::Map(a), Value::Map(b)) => Arc::ptr_eq(a, b),
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
            Value::IntPoint(p) => write!(f, "{p:?}"),
            Value::Size(s) => write!(f, "{s:?}"),
            Value::Rect(r) => write!(f, "{r:?}"),
            Value::Font(x) => write!(f, "{x:?}"),
            Value::Vector2d(v) => write!(f, "{v:?}"),
            Value::Vector3d(v) => write!(f, "{v:?}"),
            Value::Vector4d(v) => write!(f, "{v:?}"),
            Value::Quaternion(q) => write!(f, "{q:?}"),
            Value::Url(u) => write!(f, "url({u:?})"),
            Value::Object(o) => write!(f, "{o:?}"),
            Value::List(l) => f.debug_list().entries(l.iter()).finish(),
            Value::Map(m) => f
                .debug_map()
                .entries(m.iter().map(|(k, v)| (k.as_str(), v)))
                .finish(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_font_is_the_application_font() {
        let f = Font::default();
        assert_eq!(f.family.as_str(), Font::DEFAULT_FAMILY);
        assert_eq!((f.read_point_size(), f.read_pixel_size()), (9.0, 12));
    }

    #[test]
    fn a_font_has_one_size_and_reads_it_in_both_units() {
        let font = Value::Font(Arc::default());
        let size = |v: &Value| match v {
            Value::Font(f) => (
                f.point_size,
                f.pixel_size,
                f.read_point_size(),
                f.read_pixel_size(),
            ),
            _ => unreachable!(),
        };
        let px = font
            .with_member(&TypeTag::Font, "pixelSize", &Value::Int(20))
            .unwrap();
        assert_eq!(size(&px), (-1.0, 20, 15.0, 20));
        // A point size does not take a pixel size's place.
        let both = px
            .with_member(&TypeTag::Font, "pointSize", &Value::Real(9.0))
            .unwrap();
        assert_eq!(size(&both), (-1.0, 20, 15.0, 20));
        // Points read as whole pixels, cut: 20 pt is 26.67 px.
        let pt = font
            .with_member(&TypeTag::Font, "pointSize", &Value::Real(20.0))
            .unwrap();
        assert_eq!(size(&pt), (20.0, -1, 20.0, 26));
    }
}
