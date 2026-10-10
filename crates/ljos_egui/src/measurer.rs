//! The text-measuring stop-gap until `ljos_text` exists (ARCHITECTURE 7.5).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, Context, FontId, Galley};
use ljos_quick::{FontMetrics, FontSpec, TextMeasurer};

/// Measures text with egui's fonts, exactly as B0 lays it out when it paints
/// `DrawOp::Text`: `layout_no_wrap` with `FontId::proportional(pixel_size)`.
///
/// B0 ignores family, weight and slant, so this does too; measuring a bold
/// face that is then painted regular would make Text items the wrong size.
///
/// Glyph hinting depends on pixels-per-point, so a new measurer is installed
/// whenever it changes (which also relayouts every Text).
#[derive(Debug)]
pub struct EguiTextMeasurer {
    ctx: Context,
    metrics: RefCell<HashMap<u64, FontMetrics>>,
}

impl EguiTextMeasurer {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            metrics: RefCell::new(HashMap::new()),
        }
    }

    fn layout(&self, text: &str, font: &FontSpec) -> Arc<Galley> {
        // The colour does not affect the size. egui caches galleys per
        // colour too, so one fixed colour keeps these lookups cached.
        self.ctx.fonts_mut(|f| {
            f.layout_no_wrap(
                text.to_owned(),
                FontId::proportional(font.pixel_size as f32),
                Color32::PLACEHOLDER,
            )
        })
    }
}

impl TextMeasurer for EguiTextMeasurer {
    fn width(&self, text: &str, font: &FontSpec) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        f64::from(self.layout(text, font).size().x)
    }

    fn metrics(&self, font: &FontSpec) -> FontMetrics {
        let key = font.pixel_size.to_bits();
        if let Some(m) = self.metrics.borrow().get(&key) {
            return *m;
        }
        // B0 places each galley at the top of a line box of height
        // `ascent + descent`; using the galley's own row height makes the
        // box exactly as tall as what is painted, so nothing is clipped.
        let g = self.layout("Hg", font);
        let height = f64::from(g.size().y);
        let ascent = g
            .rows
            .first()
            .and_then(|r| r.row.glyphs.first())
            .map_or(height * 0.8, |gl| f64::from(gl.font_ascent));
        let m = FontMetrics {
            ascent,
            descent: height - ascent,
        };
        self.metrics.borrow_mut().insert(key, m);
        m
    }
}
