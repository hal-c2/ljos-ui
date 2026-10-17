//! Measuring a text on one line: its advance and the boxes around its ink.
//!
//! The rounding follows what Qt 6.11 reports (measured as a black box): every
//! glyph's ink is widened to whole pixels where it stands, the text's box is
//! widened to whole pixels again, and ink that overshoots the baseline by less
//! than half a pixel sits on it, the way a hinted outline does.

use ljos_render::glyph_ink_bounds;
use ljos_render::kurbo::Rect;

use crate::engine::TextEngine;
use crate::layout::TextBounds;
use crate::style::{LayoutParams, TextStyle};

/// The ink of a text in pixels, y down from the baseline, before the text's
/// own rounding.
#[derive(Clone, Copy)]
struct Ink {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl Ink {
    fn add(slot: &mut Option<Self>, left: f64, right: f64, top: f64, bottom: f64) {
        *slot = Some(match *slot {
            None => Ink {
                left,
                right,
                top,
                bottom,
            },
            Some(i) => Ink {
                left: i.left.min(left),
                right: i.right.max(right),
                top: i.top.min(top),
                bottom: i.bottom.max(bottom),
            },
        });
    }
}

/// A glyph edge on the pixel grid: outwards, but an overshoot of less than
/// half a pixel past the baseline does not count.
fn below(y: f64) -> f64 {
    if (0.0..0.5).contains(&y) {
        0.0
    } else {
        y.ceil()
    }
}

impl TextEngine {
    /// Measures `text` set on one line in `style`. Line breaks in the text
    /// take no room, as in Qt's `FontMetrics`.
    pub fn bounds(&mut self, text: &str, style: &TextStyle) -> TextBounds {
        if text.is_empty() {
            return TextBounds::default();
        }
        let font = self.font_metrics(style);
        let mut pen = 0.0;
        // Where glyphs stand, inked or not: a space has no ink but it does
        // widen the box.
        let mut span: Option<(f64, f64)> = None;
        let mut ink: Option<Ink> = None;
        for part in text.split(['\n', '\r']) {
            if part.is_empty() {
                continue;
            }
            let layout = self.layout(part, style, &LayoutParams::default());
            for run in layout.lines.iter().flat_map(|line| &line.runs) {
                let base = f64::from(run.glyphs.baseline);
                let boxes = glyph_ink_bounds(&run.glyphs);
                for (glyph, bounds) in run.glyphs.glyphs.iter().zip(boxes) {
                    let origin = pen + f64::from(glyph.x);
                    let (left, right) = match bounds {
                        Some(b) => {
                            let x = f64::from(glyph.x);
                            let (l, r) = (origin + (b.x0 - x), origin + (b.x1 - x).ceil());
                            Ink::add(&mut ink, l, r, b.y0 - base, b.y1 - base);
                            (l, r)
                        }
                        None => (origin, origin),
                    };
                    span = Some(match span {
                        None => (left, right),
                        Some((l, r)) => (l.min(left), r.max(right)),
                    });
                }
            }
            pen += layout.advance;
        }
        let Some((left, right)) = span else {
            return TextBounds {
                advance: pen,
                ..TextBounds::default()
            };
        };
        let x0 = left.floor();
        let x1 = x0 + (right - x0).ceil();
        let (tight, top, bottom) = match ink {
            Some(ink) => {
                let top = ink.top.floor();
                // Ink that ends above the baseline still reaches down to it.
                let bottom = below(ink.bottom).max(0.0);
                (Rect::new(x0, top, x1, bottom.max(top)), -top, bottom)
            }
            None => (Rect::ZERO, 0.0, 0.0),
        };
        TextBounds {
            advance: pen,
            bounding: Rect::new(x0, -font.ascent.max(top), x1, font.descent.max(bottom)),
            tight,
        }
    }
}
