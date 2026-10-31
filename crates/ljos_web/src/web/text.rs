//! Text as the browser lays it out (A22): the shape of `ljos_text`'s, so a
//! hook compiles as it is, without linking a shaper and fonts.

use std::sync::Arc;

use js_sys::Reflect;
use ljos_render::peniko::{Blob, FontData};
use ljos_render::{Decorations, GlyphRun, kurbo};
use wasm_bindgen::JsValue;

use super::paint::{Ctx, document_canvas};
use crate::pure::{break_lines, css_font, encode_run};

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub family: Arc<str>,
    pub pixel_size: f32,
    pub weight: f32,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub letter_spacing: f32,
    pub word_spacing: f32,
}

impl TextStyle {
    pub fn sized(pixel_size: f32) -> Self {
        Self {
            family: Arc::from(""),
            pixel_size,
            weight: 400.0,
            italic: false,
            underline: false,
            strikeout: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::sized(12.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WrapMode {
    #[default]
    NoWrap,
    WordWrap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ElideMode {
    #[default]
    None,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutParams {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub wrap: WrapMode,
    pub elide: ElideMode,
    pub max_lines: u32,
}

#[derive(Clone, Debug)]
pub struct LineRun {
    pub glyphs: Arc<GlyphRun>,
    pub decorations: Decorations,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub top: f64,
    pub baseline: f64,
    pub width: f64,
    pub height: f64,
    pub runs: Vec<LineRun>,
    pub range: std::ops::Range<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    pub lines: Vec<Line>,
    pub advance: f64,
    pub natural_width: f64,
    pub content_width: f64,
    pub content_height: f64,
    pub implicit_height: f64,
}

/// What a hook lays text out with. It holds a canvas of its own to measure
/// on, made as the first text is.
#[derive(Debug)]
pub struct TextEngine {
    ctx: Ctx,
}

fn number(of: &JsValue, name: &str) -> f64 {
    Reflect::get(of, &JsValue::from_str(name))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

impl TextEngine {
    pub(super) fn new() -> Self {
        Self {
            ctx: document_canvas(1, 1),
        }
    }

    pub fn layout(&mut self, text: &str, style: &TextStyle, params: &LayoutParams) -> TextLayout {
        let font = css_font(&style.family, style.pixel_size, style.weight, style.italic);
        self.ctx.set_font(&font);
        let ctx = &self.ctx;
        let mut measure = |s: &str| number(&ctx.measure_text(s), "width");
        let wrap = (params.wrap == WrapMode::WordWrap)
            .then_some(params.width)
            .flatten();
        let ranges = break_lines(text, wrap, &mut measure);
        let metrics = ctx.measure_text("Hg");
        let ascent = number(&metrics, "fontBoundingBoxAscent");
        let descent = number(&metrics, "fontBoundingBoxDescent");
        let height = ascent + descent;
        let mut out = TextLayout::default();
        let mut top = 0.0;
        for range in ranges {
            if params.max_lines != 0 && out.lines.len() as u32 >= params.max_lines {
                break;
            }
            let content = &text[range.clone()];
            let width = measure(content);
            let font_data = FontData::new(Blob::from(encode_run(&font, content)), 0);
            let bounds = kurbo::Rect::new(0.0, -ascent, width, descent);
            let run = GlyphRun::new(
                font_data,
                style.pixel_size,
                Vec::new(),
                ascent as f32,
                bounds,
            );
            let bar = |at: f64| {
                kurbo::Rect::new(
                    0.0,
                    at,
                    width,
                    at + (style.pixel_size as f64 / 14.0).max(1.0),
                )
            };
            let decorations = Decorations {
                underline: style.underline.then(|| bar(ascent + 1.0)),
                strike_through: style.strikeout.then(|| bar(ascent * 0.65)),
            };
            out.lines.push(Line {
                top,
                baseline: top + ascent,
                width,
                height,
                runs: vec![LineRun {
                    glyphs: Arc::new(run),
                    decorations,
                }],
                range,
            });
            out.natural_width = out.natural_width.max(width);
            top += height;
        }
        out.advance = out.natural_width;
        out.content_width = out.natural_width;
        out.content_height = top;
        out.implicit_height = top;
        out
    }
}
