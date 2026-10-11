//! `FontMetrics` and `TextMetrics`: what a font and a text measure, for code
//! that has to know before (or without) showing any text.
//!
//! Both ask the UI thread's text engine, so they agree with `Text` to the
//! last fraction of a pixel. Their properties are lazy bindings: an object
//! nobody reads from shapes nothing.

use ljos_engine::{
    Atom, Cx, Engine, EngineError, Font, ObjectId, PropIndex, QmlObject, QtObject, Rect, Value,
    qml_impl,
};
use ljos_render::kurbo;
use ljos_text::{TextBounds, TextStyle};

use crate::enums::elide;
use crate::text::{elide_mode, read_style, text_engine};

fn rect(r: kurbo::Rect) -> Rect {
    Rect {
        x: r.x0,
        y: r.y0,
        width: r.width(),
        height: r.height(),
    }
}

fn measure(text: &str, style: &TextStyle) -> TextBounds {
    text_engine().borrow_mut().bounds(text, style)
}

/// `text` cut to `width` in `style`. `ElideNone` (and anything that is not a
/// mode) leaves it whole, as in Qt.
fn elided(text: &Atom, style: &TextStyle, mode: i32, width: f64) -> Atom {
    if !matches!(mode, elide::LEFT | elide::RIGHT | elide::MIDDLE) {
        return text.clone();
    }
    let cut = text_engine()
        .borrow_mut()
        .elided_text(text.as_str(), style, elide_mode(mode), width);
    Atom::from(cut)
}

/// `FontMetrics` (QtQuick): the numbers of a font, and the size of text set
/// in it.
///
/// The numbers are those of the font the text engine had when they were last
/// worked out: registering a font later does not change them until `font`
/// changes.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "FontMetrics", extends = QtObject)]
pub struct FontMetrics {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub font: Font,
    #[qml(notify, readonly)]
    pub ascent: f64,
    #[qml(notify, readonly)]
    pub descent: f64,
    /// `ascent + descent`.
    #[qml(notify, readonly)]
    pub height: f64,
    #[qml(notify, readonly)]
    pub leading: f64,
    /// `height + leading`.
    #[qml(notify, readonly)]
    pub line_spacing: f64,
    #[qml(notify, readonly)]
    pub minimum_left_bearing: f64,
    #[qml(notify, readonly)]
    pub minimum_right_bearing: f64,
    #[qml(notify, readonly)]
    pub maximum_character_width: f64,
    #[qml(notify, readonly)]
    pub x_height: f64,
    #[qml(notify, readonly)]
    pub average_character_width: f64,
    /// Below the baseline.
    #[qml(notify, readonly)]
    pub underline_position: f64,
    /// Above the baseline, as is `strikeOutPosition`.
    #[qml(notify, readonly)]
    pub overline_position: f64,
    #[qml(notify, readonly)]
    pub strike_out_position: f64,
    #[qml(notify, readonly)]
    pub line_width: f64,
}

/// The style a `FontMetrics` measures with when one of its methods is
/// called: its font at the system's text size, as its properties have it.
/// Read without following it: a call is answered once.
fn own_style(cx: &Cx) -> TextStyle {
    cx.engine
        .untrack(|| read_style(cx.engine, cx.this, FontMetrics::FONT.index))
}

#[qml_impl]
impl FontMetrics {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        type Pick = fn(&ljos_text::FontMetrics) -> f64;
        let numbers: [(PropIndex, Pick); 14] = [
            (Self::ASCENT.index, |m| m.ascent),
            (Self::DESCENT.index, |m| m.descent),
            (Self::HEIGHT.index, |m| m.ascent + m.descent),
            (Self::LEADING.index, |m| m.leading),
            (Self::LINE_SPACING.index, |m| {
                m.ascent + m.descent + m.leading
            }),
            (Self::MINIMUM_LEFT_BEARING.index, |m| m.min_left_bearing),
            (Self::MINIMUM_RIGHT_BEARING.index, |m| m.min_right_bearing),
            (Self::MAXIMUM_CHARACTER_WIDTH.index, |m| m.max_char_width),
            (Self::X_HEIGHT.index, |m| m.x_height),
            (Self::AVERAGE_CHARACTER_WIDTH.index, |m| {
                m.average_char_width
            }),
            (Self::UNDERLINE_POSITION.index, |m| m.underline_position),
            // Qt places these two from the ascent, whatever the font's own
            // tables say; the text engine draws a strike-through where the
            // font wants it, so its number is not the one reported here.
            (Self::OVERLINE_POSITION.index, |m| m.ascent + 1.0),
            (Self::STRIKE_OUT_POSITION.index, |m| m.ascent / 3.0),
            (Self::LINE_WIDTH.index, |m| m.line_width),
        ];
        let (engine, me) = (cx.engine, cx.this);
        for (prop, pick) in numbers {
            engine.bind_initial(me, prop, move |e| {
                let style = read_style(e, me, Self::FONT.index);
                let metrics = text_engine().borrow_mut().font_metrics(&style);
                Value::Real(pick(&metrics))
            })?;
        }
        Ok(())
    }

    /// `advanceWidth(text)`: how far `text` moves the pen.
    #[qml(invokable, name = "advanceWidth")]
    fn advance_width(cx: &Cx, text: Value) -> f64 {
        measure(text.to_js_string().as_str(), &own_style(cx)).advance
    }

    /// `boundingRect(text)`: the room `text` takes on a line, relative to
    /// the start of its baseline.
    #[qml(invokable, name = "boundingRect")]
    fn bounding_rect(cx: &Cx, text: Value) -> Rect {
        rect(measure(text.to_js_string().as_str(), &own_style(cx)).bounding)
    }

    /// `tightBoundingRect(text)`: the ink of `text`, relative to the start
    /// of its baseline.
    #[qml(invokable, name = "tightBoundingRect")]
    fn tight_bounding_rect(cx: &Cx, text: Value) -> Rect {
        rect(measure(text.to_js_string().as_str(), &own_style(cx)).tight)
    }

    /// `elidedText(text, mode, width, flags)`: `text` cut to `width` with an
    /// ellipsis. `flags` is accepted and has no effect.
    #[qml(invokable, name = "elidedText")]
    fn elided_text(cx: &Cx, text: Value, mode: Value, width: Value, _flags: Value) -> Atom {
        elided(
            &text.to_js_string(),
            &own_style(cx),
            mode.to_number() as i32,
            width.to_number(),
        )
    }
}

/// `TextMetrics` (QtQuick): the size of one text in one font.
///
/// Like [`FontMetrics`], it does not notice a font registered later.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "TextMetrics", extends = QtObject)]
pub struct TextMetrics {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub font: Font,
    #[qml(notify)]
    pub text: Atom,
    /// [`crate::enums::elide`].
    #[qml(notify, enumeration = "Qt.TextElideMode")]
    pub elide: i32,
    #[qml(notify)]
    pub elide_width: f64,
    #[qml(notify, readonly)]
    pub advance_width: f64,
    #[qml(notify, readonly)]
    pub bounding_rect: Rect,
    #[qml(notify, readonly)]
    pub tight_bounding_rect: Rect,
    /// `boundingRect.width`.
    #[qml(notify, readonly)]
    pub width: f64,
    /// `boundingRect.height`.
    #[qml(notify, readonly)]
    pub height: f64,
    #[qml(notify, readonly)]
    pub elided_text: Atom,
    /// The last measurement, so that the five properties it feeds measure
    /// once between them.
    #[qml(skip)]
    measured: Option<(Atom, TextStyle, TextBounds)>,
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            font: Font::default(),
            text: Atom::EMPTY,
            elide: elide::NONE,
            elide_width: 0.0,
            advance_width: 0.0,
            bounding_rect: Rect::default(),
            tight_bounding_rect: Rect::default(),
            width: 0.0,
            height: 0.0,
            elided_text: Atom::EMPTY,
            measured: None,
        }
    }
}

/// The measurement of a TextMetrics' text in its font (both read tracked).
fn measured(engine: &Engine, obj: ObjectId) -> TextBounds {
    let text = engine.read(obj, TextMetrics::TEXT.index).to_js_string();
    let style = read_style(engine, obj, TextMetrics::FONT.index);
    let known = engine.with(obj, |t: &TextMetrics| match &t.measured {
        Some((for_text, for_style, bounds)) if *for_text == text && *for_style == style => {
            Some(*bounds)
        }
        _ => None,
    });
    if let Ok(Some(bounds)) = known {
        return bounds;
    }
    let bounds = measure(text.as_str(), &style);
    // A dead object keeps nothing, and that is fine.
    let _ = engine.with_mut(obj, |t: &mut TextMetrics| {
        t.measured = Some((text, style, bounds));
    });
    bounds
}

#[qml_impl]
impl TextMetrics {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        type Pick = fn(&TextBounds) -> Value;
        let numbers: [(PropIndex, Pick); 5] = [
            (Self::ADVANCE_WIDTH.index, |b| Value::Real(b.advance)),
            (Self::BOUNDING_RECT.index, |b| {
                Value::Rect(std::sync::Arc::new(rect(b.bounding)))
            }),
            (Self::TIGHT_BOUNDING_RECT.index, |b| {
                Value::Rect(std::sync::Arc::new(rect(b.tight)))
            }),
            (Self::WIDTH.index, |b| Value::Real(b.bounding.width())),
            (Self::HEIGHT.index, |b| Value::Real(b.bounding.height())),
        ];
        let (engine, me) = (cx.engine, cx.this);
        for (prop, pick) in numbers {
            engine.bind_initial(me, prop, move |e| pick(&measured(e, me)))?;
        }
        engine.bind_initial(me, Self::ELIDED_TEXT.index, move |e| {
            let text = e.read(me, Self::TEXT.index).to_js_string();
            let mode = e.read(me, Self::ELIDE.index).to_number() as i32;
            if mode == elide::NONE {
                // Not reading the font and the width here keeps a plain
                // TextMetrics from doing this again when they change.
                return Value::String(text);
            }
            let style = read_style(e, me, Self::FONT.index);
            let width = e.read(me, Self::ELIDE_WIDTH.index).to_number();
            Value::String(elided(&text, &style, mode, width))
        })
    }
}
