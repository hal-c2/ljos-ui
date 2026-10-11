//! Builds [`EditLayout`]s: the lines of [`TextEngine::layout`] with a place
//! for every character.

use parley::PositionedLayoutItem;

use super::{
    BreakClass, RawLine, Shape, TextEngine, Vertical, break_lines, ceil64, extract, reported_width,
    snapped,
};
use crate::edit::{CaretCluster, EditLayout, EditLine, EditParams, EditRun};
use crate::style::{Ranged, TextStyle, WrapMode};

/// The characters of one line and, for every glyph run that has glyphs, the
/// byte each glyph's character starts at.
pub(super) struct LineCarets {
    pub(super) clusters: Vec<CaretCluster>,
    pub(super) glyph_bytes: Vec<Vec<usize>>,
}

/// Where the characters of every line are. Walks the shape the way
/// [`extract`] does and takes the same amount off every advance, so a cursor
/// stands exactly between two glyphs.
pub(super) fn carets(shape: &Shape, text: &str) -> Vec<LineCarets> {
    let mut out = Vec::with_capacity(shape.len());
    for line in shape.lines() {
        let m = *line.metrics();
        let content = m.advance - m.hanging_advance;
        let hangs = |pen: f32, advance: f32| {
            m.hanging_advance > 0.0 && (pen >= content - 1e-3 || pen + advance <= 1e-3)
        };
        let mut shift = 0.0_f32;
        let mut clusters = Vec::new();
        let mut glyph_bytes = Vec::new();
        // A run whose text changes brush (a span that only tags or
        // underlines) comes as several glyph runs: each takes the next
        // clusters of the run, as many as its glyphs belong to.
        let mut taken: Option<((usize, std::ops::Range<usize>), usize)> = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let (placed, advances) = snapped(&gr);
            let mut x = gr.offset() - shift;
            let mut pen = gr.offset();
            let mut next = 0;
            let mut bytes = Vec::with_capacity(placed.len());
            let run_key = (gr.run().index(), gr.run().cluster_range());
            let skip = match &taken {
                Some((key, n)) if *key == run_key => *n,
                _ => 0,
            };
            let mut used = skip;
            for cluster in gr.run().visual_clusters().skip(skip) {
                let range = cluster.text_range();
                let count = cluster.glyphs().count();
                // A cluster without glyphs (the rest of a ligature) stays
                // with the glyph run before it.
                if count > 0 && next >= placed.len() {
                    break;
                }
                used += 1;
                let (mut shaped, mut qt) = (0.0_f32, 0.0_f32);
                for (g, snapped) in placed.iter().zip(&advances).skip(next).take(count) {
                    shaped += g.advance;
                    qt += snapped;
                    if !hangs(pen, g.advance) {
                        shift += g.advance - snapped;
                    }
                    pen += g.advance;
                    bytes.push(range.start);
                }
                next += count;
                // A character with glyphs of its own is as wide as Qt makes
                // them, to the bit. A ligature shares its glyphs' width out
                // between its characters, so only the difference is taken.
                let advance = if count > 0 && (cluster.advance() - shaped).abs() < 1e-3 {
                    qt
                } else {
                    cluster.advance() - (shaped - qt)
                };
                let breaks = text
                    .get(range.clone())
                    .is_some_and(|s| s.starts_with(['\n', '\r']));
                if !breaks && !range.is_empty() {
                    clusters.push(CaretCluster {
                        range,
                        x0: f64::from(x),
                        x1: f64::from(x + advance),
                        rtl: cluster.is_rtl(),
                    });
                }
                x += advance;
            }
            taken = Some((run_key, used));
            if !placed.is_empty() {
                glyph_bytes.push(bytes);
            }
        }
        out.push(LineCarets {
            clusters,
            glyph_bytes,
        });
    }
    out
}

impl TextEngine {
    /// Lay out `text` for editing. Not cached as a whole (the item keeps the
    /// result while nothing changes); the shaping behind it is.
    pub fn edit_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        params: &EditParams,
    ) -> EditLayout {
        self.edit_layout_ranged(text, style, &Ranged::NONE, params)
    }

    /// [`Self::edit_layout`] for a text whose parts differ in font (syntax
    /// highlighting): every run says which span it draws.
    pub fn edit_layout_ranged(
        &mut self,
        text: &str,
        style: &TextStyle,
        ranged: &Ranged<'_>,
        params: &EditParams,
    ) -> EditLayout {
        let metrics = self.style_metrics(style);
        let fallback = Vertical {
            ascent: metrics.font.ascent,
            descent: metrics.font.descent,
        };
        let width = params.width.filter(|w| w.is_finite() && *w > 0.0);
        let wrap = match width {
            Some(_) => params.wrap,
            None => WrapMode::NoWrap,
        };
        let wraps = wrap != WrapMode::NoWrap;
        let max_advance = width.filter(|_| wraps).map(|w| w as f32);

        let (mut raw, natural, rtl): (Vec<(RawLine, LineCarets)>, f32, bool) = if text.is_empty() {
            (Vec::new(), 0.0, false)
        } else {
            let entry = self.shaped(text, style, ranged, BreakClass::of(wrap));
            // As in `build_layout`: a text that fits as Qt counts it is not
            // broken because the shaper's advances are a touch wider.
            let max_advance = max_advance.filter(|w| entry.natural_width > *w);
            break_lines(&mut entry.shape, max_advance, false);
            let lines = extract(&entry.shape, fallback, None, ranged.objects);
            let places = carets(&entry.shape, text);
            (
                lines.into_iter().zip(places).collect(),
                entry.natural_width,
                entry.rtl,
            )
        };
        // The cursor needs a line to stand on after a line break that ends
        // the text, and in an empty text.
        let needs_last = raw
            .last()
            .is_none_or(|(line, _)| text.ends_with('\n') && line.range.start < text.len());
        if needs_last {
            let mut line = RawLine::empty(fallback);
            line.range = text.len()..text.len();
            raw.push((
                line,
                LineCarets {
                    clusters: Vec::new(),
                    glyph_bytes: Vec::new(),
                },
            ));
        }

        let limit = width.filter(|_| wraps);
        let count = raw.len();
        let mut lines = Vec::with_capacity(count);
        let (mut top, mut content_width) = (0.0_f64, 0.0_f64);
        for (index, (line, places)) in raw.into_iter().enumerate() {
            let height = line.vertical.natural();
            let width = reported_width(line.width, limit);
            content_width = content_width.max(width);
            let range = line.range.clone();
            let own = text.get(range.clone()).unwrap_or("");
            let trimmed = if line.hard_break {
                own.trim_end_matches(['\n', '\r'])
            } else if index + 1 < count {
                own.trim_end_matches(char::is_whitespace)
            } else {
                own
            };
            let finished = line.finish(top, width);
            let mut glyph_bytes = places.glyph_bytes.into_iter();
            lines.push(EditLine {
                end: range.start + trimmed.len(),
                range,
                top,
                height,
                baseline: finished.baseline,
                width,
                clusters: places.clusters,
                runs: finished
                    .runs
                    .into_iter()
                    .map(|run| EditRun {
                        glyph_bytes: glyph_bytes.next().unwrap_or_default(),
                        run,
                    })
                    .collect(),
            });
            top += height;
        }
        EditLayout {
            baseline: lines.first().map_or(0.0, |l| l.baseline),
            lines,
            natural_width: ceil64(natural),
            content_width,
            content_height: top,
            rtl,
            len: text.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The numbers are Qt 6.11's (qml6, offscreen) for Liberation Sans at
    //! 14 px, from `positionToRectangle`, `positionAt` and the content sizes
    //! of `TextInput` and `TextEdit`.

    use super::*;

    fn lay(text: &str, width: Option<f64>, wrap: WrapMode) -> EditLayout {
        TextEngine::new().edit_layout(text, &TextStyle::default(), &EditParams { width, wrap })
    }

    fn one_line(text: &str) -> EditLayout {
        lay(text, None, WrapMode::NoWrap)
    }

    #[test]
    fn empty_text_has_one_line_for_the_cursor() {
        let l = one_line("");
        assert_eq!(l.lines.len(), 1);
        assert_eq!((l.content_width, l.content_height), (0.0, 16.0));
        assert_eq!(l.baseline, 12.671875);
        let c = l.caret(0);
        assert_eq!((c.line, c.x, c.top, c.height), (0, 0.0, 0.0, 16.0));
        assert_eq!(l.hit(50.0, 5.0, false), 0);
    }

    #[test]
    fn the_cursor_stands_where_qt_puts_it() {
        let l = one_line("hello brave world");
        assert_eq!(l.content_width, 105.75);
        assert_eq!(l.caret(0).x, 0.0);
        assert_eq!(l.caret(1).x, 7.78125);
        assert_eq!(l.caret(5).x, 29.5625);
        assert_eq!(l.caret(17).x, 105.75);
    }

    #[test]
    fn a_point_is_the_nearest_gap() {
        let l = one_line("hello brave world");
        assert_eq!(l.hit(3.0, 5.0, false), 0);
        assert_eq!(l.hit(4.0, 5.0, false), 1);
        assert_eq!(l.hit(7.0, 5.0, true), 0);
        assert_eq!(l.hit(500.0, 5.0, false), 17);
        assert_eq!(l.hit(-3.0, 5.0, false), 0);
    }

    #[test]
    fn a_line_break_at_the_end_starts_a_line() {
        let l = one_line("a\n");
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.content_height, 32.0);
        let c = l.caret(2);
        assert_eq!((c.line, c.x, c.top), (1, 0.0, 16.0));
        let l = one_line("\n\n");
        assert_eq!((l.lines.len(), l.content_height), (3, 48.0));
        assert_eq!(l.caret(0).line, 0);
        assert_eq!(l.caret(1).line, 1);
        assert_eq!(l.caret(2).line, 2);
    }

    #[test]
    fn positions_in_a_document() {
        let l = one_line("hello brave world\nsecond line");
        assert_eq!(l.lines.len(), 2);
        let c = l.caret(20);
        assert_eq!((c.x, c.top, c.height), (14.78125, 16.0, 16.0));
        assert_eq!((l.caret(18).x, l.caret(18).top), (0.0, 16.0));
        assert_eq!((l.caret(17).x, l.caret(17).top), (105.75, 0.0));
        assert_eq!(l.hit(5.0, 20.0, false), 19);
        assert_eq!(l.hit(500.0, 5.0, false), 17);
        assert_eq!(l.hit(50.0, 15.0, false), 9);
        assert_eq!(l.hit(50.0, 31.0, false), 25);
        assert_eq!(l.hit(50.0, 32.0, false), 29);
        assert_eq!(l.hit(50.0, -1.0, false), 0);
        assert_eq!((l.lines[0].range.clone(), l.lines[0].end), (0..18, 17));
    }

    #[test]
    fn wrapped_lines_keep_their_places() {
        let l = lay("hello brave world again", Some(60.0), WrapMode::Wrap);
        assert_eq!(l.lines.len(), 4);
        assert_eq!(l.content_height, 64.0);
        assert_eq!(l.content_width, 35.0);
        assert_eq!(l.natural_width, 143.859375);
        // After the space the cursor is on the next line.
        assert_eq!((l.caret(6).line, l.caret(6).x), (1, 0.0));
        assert_eq!((l.caret(5).line, l.caret(5).x), (0, 29.5625));
        // Beyond the end of a wrapped line is before its hanging space.
        assert_eq!(l.hit(500.0, 5.0, false), 5);
    }

    #[test]
    fn a_range_is_one_box_per_line() {
        let l = one_line("hello\nworld");
        let rects = l.range_rects(2..8);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].0, 0);
        assert_eq!((rects[0].1.x0, rects[0].1.x1), (l.caret(2).x, l.caret(5).x));
        assert_eq!((rects[1].0, rects[1].1.x0, rects[1].1.y0), (1, 0.0, 16.0));
        assert_eq!(rects[1].1.x1, l.caret(8).x);
        assert!(l.range_rects(3..3).is_empty());
    }

    #[test]
    fn right_to_left_text_starts_at_the_right() {
        let text = "שלום";
        let l = one_line(text);
        assert!(l.rtl);
        assert_eq!(l.content_width, 30.734375);
        assert_eq!(l.caret(0).x, 30.734375);
        assert_eq!(l.caret(2).x, 20.515625);
        assert_eq!(l.caret(text.len()).x, 0.0);
        assert_eq!(l.hit(30.0, 5.0, false), 0);
        assert_eq!(l.hit(1.0, 5.0, false), text.len());
        assert_eq!(l.hit(-5.0, 5.0, false), text.len());
        assert_eq!(l.hit(500.0, 5.0, false), 0);
    }

    #[test]
    fn every_glyph_knows_its_character() {
        let l = one_line("abc");
        let run = &l.lines[0].runs[0];
        assert_eq!(run.glyph_bytes, vec![0, 1, 2]);
        assert_eq!(run.run.glyphs.glyphs.len(), 3);
    }

    #[test]
    fn spaces_at_the_end_have_a_width() {
        let l = one_line("ab ");
        assert!(l.content_width > one_line("ab").content_width);
        assert_eq!(l.caret(3).x, l.content_width);
        // Also before a line break.
        assert_eq!(one_line("ab\ncd").content_width, 15.5625);
        let l = one_line("ab \ncd");
        assert_eq!(l.content_width, 19.4375);
        assert_eq!(l.caret(3).x, 19.4375);
    }
}
