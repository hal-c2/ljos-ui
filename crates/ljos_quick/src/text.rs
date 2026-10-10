//! `Text`: the M0 stop-gap.
//!
//! Layout here is deliberately simple (ARCHITECTURE §7.5 replaces it with
//! `ljos_text`): lines are broken at `\n` and, with `wrapMode`, at spaces;
//! only `Text.ElideRight` elides; every line is painted as one
//! `DrawOp::Text`. Measuring is the host's job through [`TextMeasurer`].

use std::sync::Arc;

use ljos_engine::{Atom, Color, Engine, Font, ObjectId, QmlObject, Value};
use ljos_render::{DisplayListBuilder, TextAlign, kurbo};

use crate::enums::{align, elide, line_height_mode, wrap_mode};
use crate::item::Item;
use crate::rectangle::to_rgba;

/// `Text` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Text", extends = Item)]
pub struct Text {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub font: Font,
    /// [`crate::enums::align`] `LEFT`, `RIGHT`, `H_CENTER` or `JUSTIFY`.
    #[qml(notify)]
    pub horizontal_alignment: i32,
    /// [`crate::enums::align`] `TOP`, `BOTTOM` or `V_CENTER`.
    #[qml(notify)]
    pub vertical_alignment: i32,
    /// [`crate::enums::wrap_mode`].
    #[qml(notify)]
    pub wrap_mode: i32,
    /// [`crate::enums::elide`]; only `RIGHT` is implemented.
    #[qml(notify)]
    pub elide: i32,
    #[qml(notify)]
    pub line_height: f64,
    /// [`crate::enums::line_height_mode`].
    #[qml(notify)]
    pub line_height_mode: i32,
    #[qml(notify, readonly)]
    pub content_width: f64,
    #[qml(notify, readonly)]
    pub content_height: f64,
    #[qml(notify, readonly)]
    pub line_count: i32,
    #[qml(notify, readonly)]
    pub truncated: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            item: Item::default(),
            text: Atom::EMPTY,
            color: Color::BLACK,
            font: Font::default(),
            horizontal_alignment: align::LEFT,
            vertical_alignment: align::TOP,
            wrap_mode: wrap_mode::NO_WRAP,
            elide: elide::NONE,
            line_height: 1.0,
            line_height_mode: line_height_mode::PROPORTIONAL,
            content_width: 0.0,
            content_height: 0.0,
            line_count: 0,
            truncated: false,
        }
    }
}

/// Font size used when neither `pixelSize` nor `pointSize` is set.
pub const DEFAULT_PIXEL_SIZE: f64 = 14.0;

/// The parts of a QML `font` a measurer needs.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    pub family: Atom,
    /// Logical pixels.
    pub pixel_size: f64,
    pub bold: bool,
    pub italic: bool,
}

impl FontSpec {
    /// `pixelSize` wins over `pointSize` (1 pt = 4/3 px at 96 dpi).
    pub fn from_font(f: &Font) -> Self {
        let pixel_size = if f.pixel_size > 0 {
            f64::from(f.pixel_size)
        } else if f.point_size > 0.0 {
            f.point_size * 4.0 / 3.0
        } else {
            DEFAULT_PIXEL_SIZE
        };
        FontSpec {
            family: f.family.clone(),
            pixel_size,
            bold: f.bold(),
            italic: f.italic,
        }
    }
}

/// Vertical metrics of a font, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
}

impl FontMetrics {
    pub fn height(&self) -> f64 {
        self.ascent + self.descent
    }
}

/// Measures text for layout. The host installs one on the scene
/// ([`crate::QuickScene::set_text_measurer`]); the shell's will use egui's
/// fonts, tests use [`FixedAdvanceMeasurer`].
pub trait TextMeasurer {
    /// Advance width of a single line of text.
    fn width(&self, text: &str, font: &FontSpec) -> f64;
    fn metrics(&self, font: &FontSpec) -> FontMetrics;
}

/// A deterministic measurer: every character is `advance × pixel_size`
/// wide (bold adds 10 %), ascent is 0.8 and descent 0.2 of the size.
#[derive(Clone, Copy, Debug)]
pub struct FixedAdvanceMeasurer {
    pub advance: f64,
}

impl Default for FixedAdvanceMeasurer {
    fn default() -> Self {
        Self { advance: 0.5 }
    }
}

impl TextMeasurer for FixedAdvanceMeasurer {
    fn width(&self, text: &str, font: &FontSpec) -> f64 {
        let bold = if font.bold { 1.1 } else { 1.0 };
        text.chars().count() as f64 * self.advance * font.pixel_size * bold
    }

    fn metrics(&self, font: &FontSpec) -> FontMetrics {
        FontMetrics {
            ascent: font.pixel_size * 0.8,
            // Derived from the ascent so the line height is exactly the
            // pixel size (0.8 + 0.2 is not exact in binary).
            descent: font.pixel_size - font.pixel_size * 0.8,
        }
    }
}

/// Everything a Text's layout and paint depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputs {
    pub text: Atom,
    pub color: Color,
    pub font: FontSpec,
    pub h_align: i32,
    pub v_align: i32,
    pub wrap: i32,
    pub elide: i32,
    pub line_height: f64,
    pub line_height_mode: i32,
}

impl TextInputs {
    /// Reads the text properties (tracked).
    pub fn read(engine: &Engine, obj: ObjectId) -> Self {
        let int = |p: ljos_engine::PropIndex| engine.read(obj, p).to_number() as i32;
        TextInputs {
            text: engine.read(obj, Text::TEXT.index).to_js_string(),
            color: match engine.read(obj, Text::COLOR.index) {
                Value::Color(c) => c,
                _ => Color::BLACK,
            },
            font: match engine.read(obj, Text::FONT.index) {
                Value::Font(f) => FontSpec::from_font(&f),
                _ => FontSpec::from_font(&Font::default()),
            },
            h_align: int(Text::HORIZONTAL_ALIGNMENT.index),
            v_align: int(Text::VERTICAL_ALIGNMENT.index),
            wrap: int(Text::WRAP_MODE.index),
            elide: int(Text::ELIDE.index),
            line_height: engine.read(obj, Text::LINE_HEIGHT.index).to_number(),
            line_height_mode: int(Text::LINE_HEIGHT_MODE.index),
        }
    }

    fn line_advance(&self, m: &FontMetrics) -> f64 {
        if self.line_height_mode == line_height_mode::FIXED {
            self.line_height
        } else {
            m.height() * self.line_height
        }
    }
}

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text: Arc<str>,
    pub width: f64,
}

/// The result of [`layout`].
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<TextLine>,
    /// Distance between baselines.
    pub line_advance: f64,
    pub metrics: FontMetrics,
    pub truncated: bool,
}

impl TextLayout {
    pub fn width(&self) -> f64 {
        self.lines.iter().map(|l| l.width).fold(0.0, f64::max)
    }

    pub fn height(&self) -> f64 {
        match self.lines.len() {
            0 => 0.0,
            n => self.line_advance * n as f64,
        }
    }
}

/// Lays text out. `width` is the item's width when it constrains the text
/// (wrapping or eliding), `None` to measure the natural size.
pub fn layout(m: &dyn TextMeasurer, t: &TextInputs, width: Option<f64>) -> TextLayout {
    let metrics = m.metrics(&t.font);
    let line_advance = t.line_advance(&metrics);
    let mut lines = Vec::new();
    let mut truncated = false;
    let text = t.text.as_str();
    for para in text.split('\n') {
        match width {
            Some(w) if t.wrap != wrap_mode::NO_WRAP => wrap_words(m, &t.font, para, w, &mut lines),
            Some(w) if t.elide == elide::RIGHT => {
                let full = m.width(para, &t.font);
                if full > w {
                    truncated = true;
                    let s = elide_right(m, &t.font, para, w);
                    lines.push(TextLine {
                        width: m.width(&s, &t.font),
                        text: s.into(),
                    });
                } else {
                    lines.push(TextLine {
                        text: para.into(),
                        width: full,
                    });
                }
            }
            _ => lines.push(TextLine {
                text: para.into(),
                width: m.width(para, &t.font),
            }),
        }
    }
    if text.is_empty() {
        lines.clear();
    }
    TextLayout {
        lines,
        line_advance,
        metrics,
        truncated,
    }
}

fn wrap_words(
    m: &dyn TextMeasurer,
    font: &FontSpec,
    para: &str,
    max: f64,
    out: &mut Vec<TextLine>,
) {
    let mut line = String::new();
    for word in para.split(' ') {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if line.is_empty() || m.width(&candidate, font) <= max {
            line = candidate;
        } else {
            out.push(TextLine {
                width: m.width(&line, font),
                text: std::mem::take(&mut line).into(),
            });
            line = word.to_owned();
        }
    }
    out.push(TextLine {
        width: m.width(&line, font),
        text: line.into(),
    });
}

fn elide_right(m: &dyn TextMeasurer, font: &FontSpec, s: &str, max: f64) -> String {
    let chars: Vec<char> = s.chars().collect();
    for n in (0..chars.len()).rev() {
        let mut c: String = chars[..n].iter().collect();
        c.push('…');
        if m.width(&c, font) <= max {
            return c;
        }
    }
    "…".to_owned()
}

/// Paints laid-out text into an item of `size`.
pub fn paint(
    m: &dyn TextMeasurer,
    t: &TextInputs,
    size: kurbo::Size,
    out: &mut DisplayListBuilder,
) {
    if t.text.is_empty() || t.color.a == 0 {
        return;
    }
    let constrained = t.wrap != wrap_mode::NO_WRAP || t.elide == elide::RIGHT;
    let l = layout(m, t, constrained.then_some(size.width));
    let total = l.height();
    let top = if t.v_align & align::BOTTOM != 0 {
        size.height - total
    } else if t.v_align & align::V_CENTER != 0 {
        (size.height - total) / 2.0
    } else {
        0.0
    };
    let text_align = if t.h_align & align::RIGHT != 0 {
        TextAlign::Right
    } else if t.h_align & align::H_CENTER != 0 {
        TextAlign::Center
    } else {
        TextAlign::Left
    };
    let color = to_rgba(t.color);
    for (i, line) in l.lines.iter().enumerate() {
        let y = top + i as f64 * l.line_advance;
        let rect = kurbo::Rect::new(0.0, y, size.width.max(line.width), y + l.line_advance);
        out.text_stopgap(
            rect,
            Arc::clone(&line.text),
            t.font.pixel_size as f32,
            color,
            text_align,
        );
    }
}
