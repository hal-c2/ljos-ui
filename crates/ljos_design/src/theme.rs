//! A theme file: other values for the roles of `Ljos.Design`, read from
//! JSON, with where each one is in the file so a mistake can be shown
//! where it was made.
//!
//! ```json
//! {
//!   "Colors": {
//!     "seedHue": 30,
//!     "accent": { "light": "#6a1b9a", "dark": "#ce93d8" },
//!     "scrim": "#80000000"
//!   },
//!   "Spacing": { "md": 20 },
//!   "Typography": { "bodySize": 15 },
//!   "Motion": { "enterSmall": 120 }
//! }
//! ```
//!
//! The sections are the module's singletons and the keys its properties:
//! [`roles`] lists every one a theme may set. A colour is a string, the
//! same in light and dark, or an object of `light`, `dark`, `lightHigh`
//! and `darkHigh`; a variant it does not give is the module's own, so a
//! user who asks for high contrast gets it unless the theme says how.
//! What the user or the platform chooses (`Colors.dark`,
//! `Colors.highContrast`, `Motion.reduced`) is not a theme's to set.
//!
//! [`Theme::check_contrast`] holds the colours to the pairs the palette
//! promises ([`palette::check_pairs`]): a theme cannot make text too
//! faint to read.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::color::{Srgb, Srgba};
use crate::limits;
use crate::module::{DEFAULT_HUE, HUE_STEP, camel, capital, qml_role};
use crate::motion::{self, Frequency, Size};
use crate::palette::{self, Contrast, ROLES, Scheme, Seed};
use crate::scale;

/// What a role holds, and so what a theme may give it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Color,
    /// A whole number from `min` to `max`.
    Int {
        min: i32,
        max: i32,
    },
    /// A number of at least `min`.
    Real {
        min: f64,
    },
    /// An `easing.bezierCurve` list: groups of six numbers, the control
    /// points and the end of each segment, the last ending at 1, 1.
    Curve,
}

/// One property a theme may set.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleSpec {
    /// `Colors`, `Spacing`, `Typography` or `Motion`.
    pub singleton: &'static str,
    /// The property's name in the singleton: `textOnAccent`, `md`.
    pub name: String,
    pub kind: Kind,
}

/// Weights CSS and Qt's `font.weight` take.
const WEIGHT: Kind = Kind::Int { min: 1, max: 1000 };
const PIXELS: Kind = Kind::Int {
    min: 0,
    max: i32::MAX,
};
const MILLISECONDS: Kind = PIXELS;
const AMOUNT: Kind = Kind::Real { min: 0.0 };

/// Every property of `Ljos.Design` a theme may set, singleton by
/// singleton, in the order the module declares them.
pub fn roles() -> Vec<RoleSpec> {
    let mut out = Vec::new();
    let mut add = |singleton, name: String, kind| {
        out.push(RoleSpec {
            singleton,
            name,
            kind,
        });
    };
    add("Colors", "seedHue".to_owned(), Kind::Real { min: f64::MIN });
    for role in ROLES {
        add("Colors", qml_role(role), Kind::Color);
    }
    add("Spacing", "unit".to_owned(), PIXELS);
    for step in scale::spacing() {
        add("Spacing", step.name.to_owned(), PIXELS);
    }
    for step in scale::type_scale() {
        add("Typography", format!("{}Size", step.name), PIXELS);
        add("Typography", format!("{}LineHeight", step.name), PIXELS);
        add("Typography", format!("{}Weight", step.name), WEIGHT);
    }
    for t in motion::tokens() {
        let suffix = match t.frequency {
            Frequency::Occasional => "",
            Frequency::Rare => "Rare",
            Frequency::Frequent => "Frequent",
        };
        let name = format!("{}{}{suffix}", camel(t.kind.name()), capital(t.size.name()));
        add("Motion", name, MILLISECONDS);
    }
    add("Motion", "press".to_owned(), MILLISECONDS);
    for c in motion::CURVES {
        add("Motion", c.name.to_owned(), Kind::Curve);
    }
    add("Motion", "enterScale".to_owned(), AMOUNT);
    add("Motion", "pressScale".to_owned(), AMOUNT);
    for size in Size::ALL {
        let name = format!("gesture{}", capital(size.name()));
        for part in ["Spring", "Damping", "Mass", "Epsilon"] {
            add("Motion", format!("{name}{part}"), AMOUNT);
        }
    }
    out
}

/// The properties of the singletons that are not roles, and why a theme
/// leaves them alone.
const NOT_ROLES: [(&str, &str, &str); 5] = [
    (
        "Colors",
        "dark",
        "is the user's to choose, as the colour scheme; a colour of the theme gives \"light\" \
         and \"dark\" values instead",
    ),
    (
        "Colors",
        "highContrast",
        "is the user's to choose; a colour of the theme gives \"lightHigh\" and \"darkHigh\" \
         values instead",
    ),
    (
        "Colors",
        "table",
        "is how the module finds its palettes, not a role; set the roles themselves",
    ),
    (
        "Colors",
        "roles",
        "is how the module finds its palettes, not a role; set the roles themselves",
    ),
    (
        "Motion",
        "reduced",
        "is the user's to choose, as reduced motion",
    ),
];

/// A place in a theme file, 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

/// What is wrong with a theme, and where.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}:{column}: {message}")]
pub struct ThemeError {
    /// The file as the theme was read from, as given to [`parse`].
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// A colour's value in each variant of the palette; None is the
/// module's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Variants {
    pub light: Option<String>,
    pub dark: Option<String>,
    pub light_high: Option<String>,
    pub dark_high: Option<String>,
}

impl Variants {
    /// The theme's colour for the scheme and contrast, `#rrggbb` or
    /// `#aarrggbb`.
    pub fn pick(&self, dark: bool, high: bool) -> Option<&str> {
        match (dark, high) {
            (false, false) => self.light.as_deref(),
            (true, false) => self.dark.as_deref(),
            (false, true) => self.light_high.as_deref(),
            (true, true) => self.dark_high.as_deref(),
        }
    }
}

/// What a theme gives one role.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    Color(Variants),
    Int(i32),
    Real(f64),
    Curve(Vec<f64>),
}

/// One role a theme sets.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub singleton: &'static str,
    pub name: String,
    pub value: Setting,
    /// Where the role is named.
    pub at: Pos,
}

/// A theme as read: what it sets, in the order the file says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub file: String,
    pub entries: Vec<Entry>,
}

/// The name of a theme file, looked for by [`find`].
pub const FILE: &str = "theme.json";

/// The theme file an application whose entry is in `entry_dir` reads when
/// none is named: the nearest [`FILE`] from `entry_dir` up to `project`,
/// both included, so a module's own theme is nearer its entry than the
/// project's. Nothing above `project` is looked in; an entry outside it
/// is looked for beside alone. None when there is none.
pub fn find(entry_dir: &Path, project: &Path) -> Option<PathBuf> {
    let mut dir = Some(entry_dir);
    while let Some(d) = dir {
        let file = d.join(FILE);
        if file.is_file() {
            return Some(file);
        }
        if d == project || !d.starts_with(project) {
            return None;
        }
        dir = d.parent();
    }
    None
}

/// Reads a theme. `file` names it in errors. Every mistake is told, the
/// first in the file first.
///
/// # Errors
/// The text is not JSON, names a singleton or a role the module does not
/// have, or gives a role a value of another kind.
pub fn parse(file: &str, text: &str) -> Result<Theme, Vec<ThemeError>> {
    let error = |at: Pos, message: String| ThemeError {
        file: file.to_owned(),
        line: at.line,
        column: at.column,
        message,
    };
    let doc = Reader::new(text)
        .document()
        .map_err(|(at, message)| vec![error(at, message)])?;
    let Json::Object(sections) = doc.value else {
        return Err(vec![error(
            doc.at,
            "a theme is an object of the singletons it sets: Colors, Motion, Spacing or \
             Typography"
                .to_owned(),
        )]);
    };
    let specs = roles();
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for (section, at, body) in sections {
        let Some(singleton) = crate::module::SINGLETONS
            .into_iter()
            .find(|s| *s == section)
        else {
            errors.push(error(
                at,
                format!(
                    "Ljos.Design has no singleton \"{section}\"; a theme sets Colors, Motion, \
                     Spacing or Typography"
                ),
            ));
            continue;
        };
        let Json::Object(members) = body.value else {
            errors.push(error(
                body.at,
                format!("{singleton} is an object of the roles it sets"),
            ));
            continue;
        };
        for (name, at, value) in members {
            if let Some((_, _, why)) = NOT_ROLES
                .iter()
                .find(|(s, n, _)| *s == singleton && *n == name)
            {
                errors.push(error(at, format!("{singleton}.{name} {why}")));
                continue;
            }
            let Some(spec) = specs
                .iter()
                .find(|r| r.singleton == singleton && r.name == name)
            else {
                errors.push(error(at, format!("{singleton} has no role \"{name}\"")));
                continue;
            };
            match setting(spec, value) {
                Ok(value) => entries.push(Entry {
                    singleton,
                    name,
                    value,
                    at,
                }),
                Err((at, message)) => errors.push(error(at, message)),
            }
        }
    }
    if errors.is_empty() {
        Ok(Theme {
            file: file.to_owned(),
            entries,
        })
    } else {
        Err(errors)
    }
}

/// What `value` gives the role `spec`.
fn setting(spec: &RoleSpec, value: Spanned) -> Result<Setting, (Pos, String)> {
    let role = format!("{}.{}", spec.singleton, spec.name);
    match spec.kind {
        Kind::Color => color_setting(&role, value),
        Kind::Int { min, max } => {
            let n = match value.value {
                Json::Number(n) => n,
                _ => return Err((value.at, format!("{role} is a whole number"))),
            };
            if n.fract() != 0.0 || n < f64::from(min) || n > f64::from(max) {
                let range = if max == i32::MAX {
                    format!("at least {min}")
                } else {
                    format!("from {min} to {max}")
                };
                return Err((
                    value.at,
                    format!("{role} is a whole number {range}, not {n}"),
                ));
            }
            #[allow(clippy::cast_possible_truncation)]
            Ok(Setting::Int(n as i32))
        }
        Kind::Real { min } => match value.value {
            Json::Number(n) if n >= min => Ok(Setting::Real(n)),
            Json::Number(n) => Err((value.at, format!("{role} is at least {min}, not {n}"))),
            _ => Err((value.at, format!("{role} is a number"))),
        },
        Kind::Curve => {
            let wrong = || {
                format!(
                    "{role} is an easing.bezierCurve list: six numbers for each segment, the \
                     last segment ending at 1, 1"
                )
            };
            let Json::Array(items) = value.value else {
                return Err((value.at, wrong()));
            };
            let numbers: Option<Vec<f64>> = items
                .iter()
                .map(|i| match i.value {
                    Json::Number(n) => Some(n),
                    _ => None,
                })
                .collect();
            match numbers {
                Some(n) if !n.is_empty() && n.len() % 6 == 0 && n[n.len() - 2..] == [1.0, 1.0] => {
                    Ok(Setting::Curve(n))
                }
                _ => Err((value.at, wrong())),
            }
        }
    }
}

fn color_setting(role: &str, value: Spanned) -> Result<Setting, (Pos, String)> {
    let one = |v: &Spanned| match &v.value {
        Json::String(s) => Srgba::parse(s)
            .map(Srgba::to_hex)
            .map_err(|_| (v.at, format!("{role}: \"{s}\" is no colour"))),
        _ => Err((
            v.at,
            format!("{role} is a colour: a string such as \"#6a1b9a\""),
        )),
    };
    match value.value {
        Json::String(_) => {
            let c = one(&value)?;
            Ok(Setting::Color(Variants {
                light: Some(c.clone()),
                dark: Some(c),
                ..Variants::default()
            }))
        }
        Json::Object(members) => {
            let mut v = Variants::default();
            for (name, at, value) in members {
                let slot = match name.as_str() {
                    "light" => &mut v.light,
                    "dark" => &mut v.dark,
                    "lightHigh" => &mut v.light_high,
                    "darkHigh" => &mut v.dark_high,
                    _ => {
                        return Err((
                            at,
                            format!(
                                "{role} has no variant \"{name}\"; a colour has \"light\", \
                                 \"dark\", \"lightHigh\" and \"darkHigh\""
                            ),
                        ));
                    }
                };
                *slot = Some(one(&value)?);
            }
            if v == Variants::default() {
                return Err((value.at, format!("{role} gives no variant")));
            }
            Ok(Setting::Color(v))
        }
        _ => Err((
            value.at,
            format!(
                "{role} is a colour: a string such as \"#6a1b9a\", or an object of \"light\", \
                 \"dark\", \"lightHigh\" and \"darkHigh\""
            ),
        )),
    }
}

/// The hue of the palette `Colors` shows for `seed_hue`: the nearest of
/// one every [`HUE_STEP`] degrees, as the module picks it.
pub fn palette_hue(seed_hue: f64) -> f32 {
    let step = f64::from(HUE_STEP);
    let n = 360 / HUE_STEP;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (seed_hue.rem_euclid(360.0) / step).round() as u32 % n;
    (i * HUE_STEP) as f32
}

/// The variants of the palette, as [`Variants::pick`] takes them.
const VARIANTS: [(Scheme, Contrast, &str); 4] = [
    (Scheme::Light, Contrast::Normal, "light"),
    (Scheme::Dark, Contrast::Normal, "dark"),
    (Scheme::Light, Contrast::High, "light high contrast"),
    (Scheme::Dark, Contrast::High, "dark high contrast"),
];

impl Theme {
    /// The seed hue the theme sets, if it does.
    pub fn seed_hue(&self) -> Option<f64> {
        self.entries.iter().find_map(|e| match e.value {
            Setting::Real(h) if e.singleton == "Colors" && e.name == "seedHue" => Some(h),
            _ => None,
        })
    }

    fn color(&self, role: &str) -> Option<(&Variants, Pos)> {
        let name = qml_role(role);
        self.entries.iter().find_map(|e| match &e.value {
            Setting::Color(v) if e.singleton == "Colors" && e.name == name => Some((v, e.at)),
            _ => None,
        })
    }

    /// Holds the theme's colours, in each variant of the palette of
    /// `seed_hue` (the theme's own when it sets one), to the pairs the
    /// palette promises: text 4.5:1 (7:1 in high contrast), marks and
    /// borders 3:1. Only a pair with a colour of the theme's in it is
    /// told; the module's own pass.
    ///
    /// # Errors
    /// Each pair that does not pass, and each colour another is read on
    /// that is not opaque.
    pub fn check_contrast(&self, seed_hue: Option<f64>) -> Result<(), Vec<ThemeError>> {
        let hue = palette_hue(
            self.seed_hue()
                .or(seed_hue)
                .unwrap_or(f64::from(DEFAULT_HUE)),
        );
        let error = |at: Pos, message: String| ThemeError {
            file: self.file.clone(),
            line: at.line,
            column: at.column,
            message,
        };
        let mut errors = Vec::new();
        let grounds = palette::backgrounds();
        for (scheme, contrast, variant) in VARIANTS {
            let (dark, high) = (scheme == Scheme::Dark, contrast == Contrast::High);
            let own = |role: &str| {
                self.color(role)
                    .and_then(|(v, at)| Some((v.pick(dark, high)?, at)))
            };
            for &ground in &grounds {
                if let Some((c, at)) = own(ground)
                    && Srgba::parse(c).is_ok_and(|c| c.a < 1.0)
                {
                    errors.push(error(
                        at,
                        format!(
                            "Colors.{} in {variant} is translucent; other colours are read on \
                             it, so it has to be opaque",
                            qml_role(ground)
                        ),
                    ));
                }
            }
            if !ROLES.iter().any(|r| own(r).is_some()) {
                continue;
            }
            let base = palette::palette(Seed::hue(hue), scheme, contrast);
            let color = |role: &str| {
                own(role)
                    .map(|(c, _)| c)
                    .or_else(|| base.role(role))
                    .and_then(|c| Srgba::parse(c).ok())
                    .unwrap_or(Srgb::BLACK.with_alpha(1.0))
            };
            for check in palette::check_pairs(color, contrast.text()) {
                if check.passes() {
                    continue;
                }
                let at = own(check.foreground).or_else(|| own(check.background));
                let Some((_, at)) = at else {
                    continue;
                };
                let kind = if check.kind == "text" {
                    "text"
                } else if check.required >= limits::TEXT_CONTRAST {
                    "a strong border"
                } else {
                    "a mark or border"
                };
                errors.push(error(
                    at,
                    format!(
                        "Colors.{} on {} is {:.2}:1 in {variant}; {kind} needs {}:1",
                        qml_role(check.foreground),
                        qml_role(check.background),
                        check.ratio,
                        check.required
                    ),
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            errors.sort_by_key(|e| (e.line, e.column));
            Err(errors)
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// JSON as read, each value with where it starts. An object keeps its
/// members in order and with where each key is.
#[derive(Debug)]
enum Json {
    Null,
    /// A theme takes no truth values, so which one it was does not matter.
    Bool,
    Number(f64),
    String(String),
    Array(Vec<Spanned>),
    Object(Vec<(String, Pos, Spanned)>),
}

#[derive(Debug)]
struct Spanned {
    at: Pos,
    value: Json,
}

/// A JSON reader (RFC 8259) that knows where it is: `serde_json` tells
/// where an error is, but not where a key that reads well was.
struct Reader<'a> {
    text: &'a str,
    i: usize,
    line: u32,
    column: u32,
}

type Read<T> = Result<T, (Pos, String)>;

/// Deeper than any theme; a file nested further is refused rather than
/// read on the stack.
const DEPTH: usize = 32;

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            i: 0,
            line: 1,
            column: 1,
        }
    }

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            column: self.column,
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.i..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.bump();
        }
    }

    fn document(mut self) -> Read<Spanned> {
        // A byte-order mark is no part of the text.
        if self.peek() == Some('\u{feff}') {
            self.i += '\u{feff}'.len_utf8();
        }
        self.space();
        let value = self.value(0)?;
        self.space();
        match self.peek() {
            None => Ok(value),
            Some(c) => Err((self.pos(), format!("`{c}` after the end of the theme"))),
        }
    }

    fn unexpected(&self, wanted: &str) -> (Pos, String) {
        match self.peek() {
            None => (
                self.pos(),
                format!("the file ends where {wanted} was expected"),
            ),
            Some(c) => (self.pos(), format!("`{c}` where {wanted} was expected")),
        }
    }

    fn value(&mut self, depth: usize) -> Read<Spanned> {
        let at = self.pos();
        if depth > DEPTH {
            return Err((at, format!("nested deeper than {DEPTH}")));
        }
        let value = match self.peek() {
            Some('{') => self.object(depth)?,
            Some('[') => self.array(depth)?,
            Some('"') => Json::String(self.string()?),
            Some('-' | '0'..='9') => Json::Number(self.number()?),
            Some('t') => self.word("true", Json::Bool)?,
            Some('f') => self.word("false", Json::Bool)?,
            Some('n') => self.word("null", Json::Null)?,
            _ => return Err(self.unexpected("a value")),
        };
        Ok(Spanned { at, value })
    }

    fn word(&mut self, word: &str, value: Json) -> Read<Json> {
        if self.text[self.i..].starts_with(word) {
            for _ in word.chars() {
                self.bump();
            }
            Ok(value)
        } else {
            Err(self.unexpected("a value"))
        }
    }

    fn object(&mut self, depth: usize) -> Read<Json> {
        self.bump();
        let mut members: Vec<(String, Pos, Spanned)> = Vec::new();
        self.space();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Json::Object(members));
        }
        loop {
            self.space();
            let at = self.pos();
            if self.peek() != Some('"') {
                return Err(self.unexpected("a name in quotes"));
            }
            let key = self.string()?;
            if members.iter().any(|(k, ..)| *k == key) {
                return Err((at, format!("\"{key}\" is given twice")));
            }
            self.space();
            if self.peek() != Some(':') {
                return Err(self.unexpected("`:`"));
            }
            self.bump();
            self.space();
            let value = self.value(depth + 1)?;
            members.push((key, at, value));
            self.space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.unexpected("`,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Read<Json> {
        self.bump();
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(Json::Array(items));
        }
        loop {
            self.space();
            items.push(self.value(depth + 1)?);
            self.space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.unexpected("`,` or `]`")),
            }
        }
    }

    fn string(&mut self) -> Read<String> {
        let start = self.pos();
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err((start, "a string that does not end".to_owned())),
                Some('"') => return Ok(out),
                Some('\\') => {
                    let at = self.pos();
                    let c = match self.bump() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => self.unicode(at)?,
                        _ => return Err((at, "an escape JSON does not have".to_owned())),
                    };
                    out.push(c);
                }
                Some(c) if (c as u32) < 0x20 => {
                    return Err((start, "a string that does not end on its line".to_owned()));
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn hex4(&mut self, at: Pos) -> Read<u32> {
        let mut v = 0;
        for _ in 0..4 {
            let d = self
                .bump()
                .and_then(|c| c.to_digit(16))
                .ok_or((at, "`\\u` takes four hexadecimal digits".to_owned()))?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn unicode(&mut self, at: Pos) -> Read<char> {
        let hi = self.hex4(at)?;
        let code = if (0xd800..0xdc00).contains(&hi) {
            // A pair of surrogates is one character.
            if !self.text[self.i..].starts_with("\\u") {
                return Err((at, "half of a surrogate pair".to_owned()));
            }
            self.bump();
            self.bump();
            let lo = self.hex4(at)?;
            if !(0xdc00..0xe000).contains(&lo) {
                return Err((at, "half of a surrogate pair".to_owned()));
            }
            0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
        } else {
            hi
        };
        char::from_u32(code).ok_or((at, "half of a surrogate pair".to_owned()))
    }

    fn number(&mut self) -> Read<f64> {
        let at = self.pos();
        let start = self.i;
        let bad = || (at, "a number JSON does not write so".to_owned());
        if self.peek() == Some('-') {
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                self.bump();
            }
            Some('1'..='9') => {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
            }
            _ => return Err(bad()),
        }
        if self.peek() == Some('.') {
            self.bump();
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(bad());
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(bad());
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        self.text[start..self.i]
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{Seeds, files};

    fn read(text: &str) -> Result<Theme, Vec<ThemeError>> {
        parse("theme.json", text)
    }

    fn one_error(text: &str) -> ThemeError {
        let mut e = read(text).unwrap_err();
        assert_eq!(e.len(), 1, "{e:?}");
        e.remove(0)
    }

    #[test]
    fn the_nearest_theme_from_the_entry_up_to_the_project_is_found() {
        let project = std::env::temp_dir().join(format!("ljos-theme-find-{}", std::process::id()));
        let module = project.join("app/Module");
        std::fs::create_dir_all(&module).unwrap();
        let write = |dir: &Path| std::fs::write(dir.join(FILE), "{}").unwrap();
        assert_eq!(find(&module, &project), None);
        write(&project);
        assert_eq!(find(&module, &project), Some(project.join(FILE)));
        // Not above the project.
        assert_eq!(find(&module, &project.join("app")), None);
        write(&module);
        assert_eq!(find(&module, &project), Some(module.join(FILE)));
        std::fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn every_role_is_a_property_of_the_module_and_every_property_is_one_or_refused() {
        let files = files(Seeds::Hues);
        let mut declared = Vec::new();
        for f in &files {
            let Some(singleton) = f.name.strip_suffix(".qml") else {
                continue;
            };
            if !crate::module::SINGLETONS.contains(&singleton) {
                continue;
            }
            for line in f.text.lines() {
                let Some(rest) = line.trim().split("property ").nth(1) else {
                    continue;
                };
                let mut words = rest.split_whitespace();
                let ty = words.next().unwrap().to_owned();
                let name = words.next().unwrap().trim_end_matches(':').to_owned();
                declared.push((singleton.to_owned(), name, ty));
            }
        }
        let specs = roles();
        for spec in &specs {
            let (_, _, ty) = declared
                .iter()
                .find(|(s, n, _)| s == spec.singleton && *n == spec.name)
                .unwrap_or_else(|| panic!("{}.{} is not declared", spec.singleton, spec.name));
            let want = match spec.kind {
                Kind::Color => "color",
                Kind::Int { .. } => "int",
                Kind::Real { .. } => "real",
                Kind::Curve => "var",
            };
            assert_eq!(ty, want, "{}.{}", spec.singleton, spec.name);
        }
        for (s, n, _) in &declared {
            let known = specs.iter().any(|r| r.singleton == s && r.name == *n)
                || NOT_ROLES.iter().any(|(rs, rn, _)| rs == s && rn == n);
            assert!(known, "{s}.{n} is neither a role nor refused");
        }
    }

    #[test]
    fn a_theme_reads_with_where_each_role_is() {
        let t = read(
            "{\n  \"Colors\": {\n    \"accent\": { \"light\": \"#6A1B9A\", \"dark\": \"plum\" },\n    \
             \"scrim\": \"#80000000\",\n    \"seedHue\": 30\n  },\n  \"Spacing\": { \"md\": 20 },\n  \
             \"Motion\": { \"easeOut\": [0.2, 0, 0, 1, 1, 1] }\n}",
        )
        .unwrap();
        let accent = &t.entries[0];
        assert_eq!(
            (accent.singleton, accent.name.as_str()),
            ("Colors", "accent")
        );
        assert_eq!(accent.at, Pos { line: 3, column: 5 });
        let Setting::Color(v) = &accent.value else {
            panic!()
        };
        assert_eq!(v.light.as_deref(), Some("#6a1b9a"));
        assert_eq!(v.dark.as_deref(), Some("#dda0dd"));
        assert_eq!(v.pick(false, true), None);
        assert_eq!(t.seed_hue(), Some(30.0));
        assert_eq!(t.entries[3].value, Setting::Int(20));
    }

    #[test]
    fn mistakes_are_told_where_they_are() {
        let e = one_error("{\n  \"Colors\": {\n    \"accent\": \"#1b5e20\",\n  }\n}");
        assert_eq!((e.line, e.column), (4, 3), "{e}");
        let e = one_error("{ \"Colors\": { \"acent\": \"#000\" } }");
        assert_eq!(e.message, "Colors has no role \"acent\"");
        assert_eq!(
            e.to_string(),
            "theme.json:1:15: Colors has no role \"acent\""
        );
        let e = one_error("{ \"Colours\": {} }");
        assert!(e.message.contains("no singleton \"Colours\""), "{e}");
        let e = one_error("{ \"Colors\": { \"accent\": 12 } }");
        assert!(e.message.starts_with("Colors.accent is a colour"), "{e}");
        let e = one_error("{ \"Colors\": { \"accent\": \"#6a1b9z\" } }");
        assert_eq!(e.message, "Colors.accent: \"#6a1b9z\" is no colour");
        let e = one_error("{ \"Colors\": { \"dark\": true } }");
        assert!(
            e.message.starts_with("Colors.dark is the user's to choose"),
            "{e}"
        );
        let e = one_error("{ \"Motion\": { \"reduced\": true } }");
        assert!(e.message.starts_with("Motion.reduced is the user's"), "{e}");
        let e = one_error("{ \"Spacing\": { \"md\": 2.5 } }");
        assert!(e.message.contains("whole number at least 0"), "{e}");
        let e = one_error("{ \"Typography\": { \"bodyWeight\": 1200 } }");
        assert!(e.message.contains("from 1 to 1000"), "{e}");
        let e = one_error("{ \"Motion\": { \"easeOut\": [0, 0, 1] } }");
        assert!(e.message.contains("bezierCurve"), "{e}");
        let e = one_error("{ \"Colors\": { \"accent\": { \"dim\": \"#000\" } } }");
        assert!(e.message.contains("no variant \"dim\""), "{e}");
        let e = one_error("{ \"Spacing\": { \"md\": 1, \"md\": 2 } }");
        assert!(e.message.contains("given twice"), "{e}");
        let e = one_error("");
        assert_eq!((e.line, e.column), (1, 1));
        let e = one_error("[]");
        assert!(e.message.starts_with("a theme is an object"), "{e}");
        // Every mistake is told, not only the first.
        let all = read("{ \"Colors\": { \"a\": 1, \"b\": 2 } }").unwrap_err();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn strings_numbers_and_nesting_are_json() {
        let t = read("{ \"Colors\": { \"accent\": \"\\u0023ABC\" } }").unwrap();
        let Setting::Color(v) = &t.entries[0].value else {
            panic!()
        };
        assert_eq!(v.light.as_deref(), Some("#aabbcc"));
        assert!(read("{ \"Spacing\": { \"md\": 1e1 } }").is_ok());
        assert!(read("{ \"Spacing\": { \"md\": 01 } }").is_err());
        assert!(read("{ \"Spacing\": { \"md\": -0.5e } }").is_err());
        let deep = format!("{}{}", "[".repeat(100), "]".repeat(100));
        assert!(one_error(&deep).message.contains("nested deeper"));
        assert!(read("\u{feff}{}").is_ok());
    }

    #[test]
    fn the_nearest_palette_is_the_module_s() {
        assert_eq!(palette_hue(240.0), 240.0);
        assert_eq!(palette_hue(14.0), 0.0);
        assert_eq!(palette_hue(15.0), 30.0);
        assert_eq!(palette_hue(-20.0), 330.0);
        assert_eq!(palette_hue(359.0), 0.0);
    }

    #[test]
    fn a_colour_too_faint_to_read_is_refused_with_its_pair() {
        let t = read("{ \"Colors\": { \"text\": \"#c0c0c0\" } }").unwrap();
        let e = t.check_contrast(None).unwrap_err();
        assert!(
            e[0].message.starts_with("Colors.text on canvas is 1.")
                && e[0].message.ends_with(" in light; text needs 4.5:1"),
            "{e:?}"
        );
        assert_eq!(e[0].line, 1);
        // A theme with its own colours that pass is taken, high contrast
        // keeping the module's.
        let t = read(
            "{ \"Colors\": { \"accent\": { \"light\": \"#6a1b9a\", \"dark\": \"#9c4dc4\" } } }",
        )
        .unwrap();
        t.check_contrast(None).unwrap();
        // A purple dark enough for light is too dark for dark: it does
        // not stand out from the dark grounds. A mid one serves both.
        let t = read("{ \"Colors\": { \"accent\": \"#6a1b9a\" } }").unwrap();
        let e = t.check_contrast(None).unwrap_err();
        assert!(e.iter().all(|e| e.message.contains(" in dark;")), "{e:?}");
        let t = read("{ \"Colors\": { \"accent\": \"#9c4dc4\" } }").unwrap();
        t.check_contrast(None).unwrap();
        // A translucent ground cannot be checked.
        let t = read("{ \"Colors\": { \"canvas\": \"#80ffffff\" } }").unwrap();
        let e = t.check_contrast(None).unwrap_err();
        assert!(e.iter().any(|e| e.message.contains("translucent")), "{e:?}");
        // A translucent mark is checked as it shows.
        let t = read("{ \"Colors\": { \"border\": \"#20000000\" } }").unwrap();
        assert!(t.check_contrast(None).is_err());
        // Spacing alone has nothing to check.
        read("{ \"Spacing\": { \"md\": 20 } }")
            .unwrap()
            .check_contrast(Some(10.0))
            .unwrap();
    }
}
