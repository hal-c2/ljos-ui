//! The text of a page: its characters in the order the page draws them,
//! each where its glyph is, with the line breaks and spaces a reader sees
//! put back in. Selections and searches are worked out on it.
//!
//! What qml6 does, measured (features/pdf/pdf-text.feature,
//! pdf-search.feature):
//! - Lines are joined by a carriage return and a line feed, and a gap
//!   between words that the page leaves without a space is one.
//! - A character's rectangle is its glyph's ink, in points from the top
//!   left of the page as it is shown (turned by the page's rotation).
//! - A selection starts at the character under the first point, or the
//!   nearest one close by, and ends at the one under the last point, which
//!   is in it only when the point is past its middle. Dragged back, it is
//!   from the character under the last point to the one under the first,
//!   which is left out.

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::util::TransformExt as _;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, hayro_cmap::BfString, interpret_page,
};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Point, Rect, Shape as _, Vec2};

/// How far from a character, in points, a point still picks it when it is
/// on none: a reader starting a drag just left of a line means the line.
const NEAR: f64 = 10.0;

/// Context a search result shows each side of it, in characters.
const CONTEXT: usize = 64;

/// One character of a page.
#[derive(Clone, Debug)]
pub(crate) struct Char {
    pub c: char,
    /// Where its ink is; `None` for a space and a line break, drawn or
    /// put back in, and for a glyph with no ink.
    pub rect: Option<Rect>,
}

/// The text of one page.
#[derive(Debug, Default)]
pub(crate) struct PageText {
    pub chars: Vec<Char>,
}

/// A glyph as the page draws it.
struct Drawn {
    text: String,
    /// Where the pen was, and where it goes next.
    origin: Point,
    end: Point,
    /// Along the baseline, one unit long.
    dir: Vec2,
    /// The size of the font as drawn.
    em: f64,
    rect: Option<Rect>,
}

#[derive(Default)]
struct Collect {
    drawn: Vec<Drawn>,
}

impl<'a> Device<'a> for Collect {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    // Invisible text is read too: it is what a scanned page's recognised
    // words are.
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            // A glyph whose character the document does not say is left out:
            // a reader cannot select what it cannot copy.
            let Some(text) = glyph.as_unicode().map(|u| match u {
                BfString::Char(c) => c.to_string(),
                BfString::String(s) => s,
            }) else {
                continue;
            };
            let t: Affine = props.transform * glyph.transform();
            let origin = t * Point::ZERO;
            // Outline glyphs are 1000 units to the em.
            let (advance, rect) = match &**glyph {
                Glyph::Outline(g) => {
                    let mut path = g.outline();
                    let rect = (!path.elements().is_empty()).then(|| {
                        path.apply_affine(t);
                        path.bounding_box()
                    });
                    (f64::from(g.advance_width().unwrap_or(0.0)), rect)
                }
                // A Type 3 glyph is a drawing, with no outline to measure:
                // its character is still text, though not one a point picks.
                Glyph::Type3(_) => (0.0, None),
            };
            let along = t * Point::new(1000.0, 0.0) - origin;
            let up = t * Point::new(0.0, 1000.0) - origin;
            let dir = if along.hypot() > 0.0 {
                along / along.hypot()
            } else {
                Vec2::new(1.0, 0.0)
            };
            self.drawn.push(Drawn {
                text,
                origin,
                end: t * Point::new(advance, 0.0),
                dir,
                em: up.hypot(),
                rect: rect.filter(|r| r.width() > 0.0 || r.height() > 0.0),
            });
        }
    }
}

impl PageText {
    /// Reads the text of `page`.
    pub(crate) fn of(page: &Page<'_>) -> Self {
        let cache = InterpreterCache::new();
        let (w, h) = page.render_dimensions();
        let mut ctx = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut device = Collect::default();
        interpret_page(page, &mut ctx, &mut device);
        Self::from_drawn(&device.drawn)
    }

    fn from_drawn(drawn: &[Drawn]) -> Self {
        let mut chars: Vec<Char> = Vec::new();
        let mut last: Option<&Drawn> = None;
        let blank = |chars: &[Char]| chars.last().is_none_or(|c| c.c.is_whitespace());
        for d in drawn {
            if let Some(prev) = last {
                let em = prev.em.max(d.em);
                let step = d.origin - prev.origin;
                let off_line = prev.dir.cross(step).abs();
                let gap = prev.dir.dot(d.origin - prev.end);
                if off_line > 0.5 * em {
                    // A line ends with whatever spaces it had.
                    while chars.last().is_some_and(|c| c.c == ' ') {
                        chars.pop();
                    }
                    chars.push(Char {
                        c: '\r',
                        rect: None,
                    });
                    chars.push(Char {
                        c: '\n',
                        rect: None,
                    });
                } else if gap > 0.15 * em
                    && !blank(&chars)
                    && !d.text.starts_with(char::is_whitespace)
                {
                    chars.push(Char { c: ' ', rect: None });
                }
            }
            last = Some(d);
            let n = d.text.chars().count();
            for (i, c) in d.text.chars().enumerate() {
                if c.is_whitespace() {
                    // One space between words, however many the page drew.
                    if !blank(&chars) {
                        chars.push(Char { c: ' ', rect: None });
                    }
                    continue;
                }
                // A ligature's characters share its glyph, side by side.
                let rect = d.rect.map(|r| {
                    let part = r.width() / n as f64;
                    Rect::new(
                        r.x0 + part * i as f64,
                        r.y0,
                        r.x0 + part * (i + 1) as f64,
                        r.y1,
                    )
                });
                chars.push(Char { c, rect });
            }
        }
        while chars.last().is_some_and(|c| c.c.is_whitespace()) {
            chars.pop();
        }
        Self { chars }
    }

    pub(crate) fn len(&self) -> usize {
        self.chars.len()
    }

    /// The text from `start` to `end`, not including `end`.
    pub(crate) fn text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.chars.len());
        self.chars
            .get(start.min(end)..end)
            .unwrap_or_default()
            .iter()
            .map(|c| c.c)
            .collect()
    }

    /// The character a point picks: one it is on, or else the nearest
    /// within [`NEAR`].
    pub(crate) fn hit(&self, p: Point) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, c) in self.chars.iter().enumerate() {
            let Some(r) = c.rect else { continue };
            if r.contains(p) {
                return Some(i);
            }
            let dx = (r.x0 - p.x).max(p.x - r.x1).max(0.0);
            let dy = (r.y0 - p.y).max(p.y - r.y1).max(0.0);
            let d = dx.hypot(dy);
            if d <= NEAR && best.is_none_or(|(_, b)| d < b) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The characters between two points: from the one at the first, to
    /// the one at the second when the point is past its middle, or back to
    /// the one at the second, without the one at the first. `None` when
    /// either point is on no text, or they are the same point.
    pub(crate) fn between(&self, from: Point, to: Point) -> Option<(usize, usize)> {
        if from == to {
            return None;
        }
        let (a, b) = (self.hit(from)?, self.hit(to)?);
        if a > b {
            // Dragged back: the character it started on is left out
            // wherever on it the drag started (qml6).
            return Some((b, a));
        }
        let middle = self.chars[b].rect.map_or(f64::INFINITY, |r| r.center().x);
        let end = if to.x > middle { b + 1 } else { b };
        Some((a, end.max(a + 1)))
    }

    /// A rectangle around each line of the characters from `start` to
    /// `end`.
    pub(crate) fn lines(&self, start: usize, end: usize) -> Vec<Rect> {
        let mut out = Vec::new();
        let mut line: Option<Rect> = None;
        let end = end.min(self.chars.len());
        for c in self.chars.get(start.min(end)..end).unwrap_or_default() {
            if c.c == '\n' {
                out.extend(line.take());
                continue;
            }
            if let Some(r) = c.rect {
                line = Some(line.map_or(r, |l| l.union(r)));
            }
        }
        out.extend(line);
        out
    }

    /// Where each of `needle` is, whatever the case, without overlapping:
    /// the index of its first character and of the one after its last.
    pub(crate) fn find(&self, needle: &str) -> Vec<(usize, usize)> {
        // Character by character, so that an index here is one in the text.
        let fold = |c: char| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        };
        let needle: Vec<char> = needle.chars().map(fold).collect();
        let hay: Vec<char> = self.chars.iter().map(|c| fold(c.c)).collect();
        let mut found = Vec::new();
        if needle.is_empty() {
            return found;
        }
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            if hay[i..i + needle.len()] == needle[..] {
                found.push((i, i + needle.len()));
                i += needle.len();
            } else {
                i += 1;
            }
        }
        found
    }

    /// The text before and after the characters from `start` to `end`, as a
    /// search result shows it: a line break is "⏎".
    pub(crate) fn context(&self, start: usize, end: usize) -> (String, String) {
        let shown = |s: String| s.replace("\r\n", "\u{23ce}");
        (
            shown(self.text(start.saturating_sub(CONTEXT), start)),
            shown(self.text(end, end.saturating_add(CONTEXT))),
        )
    }
}

/// `r` with each side on a whole point, as a search result's rectangle is.
pub(crate) fn whole(r: Rect) -> Rect {
    Rect::new(r.x0.round(), r.y0.round(), r.x1.round(), r.y1.round())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(text: &str, x: f64, y: f64, width: f64) -> Drawn {
        Drawn {
            text: text.to_owned(),
            origin: Point::new(x, y),
            end: Point::new(x + width, y),
            dir: Vec2::new(1.0, 0.0),
            em: 10.0,
            rect: (!text.trim().is_empty()).then(|| Rect::new(x, y - 7.0, x + width, y)),
        }
    }

    fn page(drawn: &[Drawn]) -> PageText {
        PageText::from_drawn(drawn)
    }

    #[test]
    fn a_gap_between_words_is_a_space_and_a_new_baseline_a_line_break() {
        let t = page(&[
            glyph("a", 0.0, 10.0, 5.0),
            glyph("b", 9.0, 10.0, 5.0),
            glyph("c", 0.0, 30.0, 5.0),
        ]);
        assert_eq!(t.text(0, t.len()), "a b\r\nc");
    }

    #[test]
    fn spaces_the_page_draws_are_one_and_none_end_a_line() {
        let t = page(&[
            glyph("a", 0.0, 10.0, 5.0),
            glyph(" ", 5.0, 10.0, 3.0),
            glyph(" ", 8.0, 10.0, 3.0),
            glyph("b", 11.0, 10.0, 5.0),
            glyph(" ", 16.0, 10.0, 3.0),
            glyph("c", 0.0, 30.0, 5.0),
        ]);
        assert_eq!(t.text(0, t.len()), "a b\r\nc");
    }

    #[test]
    fn a_ligature_is_its_characters_side_by_side() {
        let t = page(&[glyph("fi", 0.0, 10.0, 10.0)]);
        assert_eq!(t.text(0, t.len()), "fi");
        assert_eq!(t.hit(Point::new(7.0, 8.0)), Some(1));
    }

    #[test]
    fn the_last_character_is_in_only_past_its_middle() {
        let t = page(&[glyph("a", 0.0, 10.0, 10.0), glyph("b", 10.0, 10.0, 10.0)]);
        assert_eq!(
            t.between(Point::new(1.0, 5.0), Point::new(15.0, 5.0)),
            Some((0, 1))
        );
        assert_eq!(
            t.between(Point::new(1.0, 5.0), Point::new(16.0, 5.0)),
            Some((0, 2))
        );
        assert_eq!(
            t.between(Point::new(16.0, 5.0), Point::new(1.0, 5.0)),
            Some((0, 1))
        );
        assert_eq!(
            t.between(Point::new(4.0, 5.0), Point::new(1.0, 5.0)),
            Some((0, 1))
        );
        assert_eq!(t.between(Point::new(1.0, 5.0), Point::new(1.0, 5.0)), None);
        assert_eq!(
            t.between(Point::new(1.0, 50.0), Point::new(16.0, 5.0)),
            None
        );
    }

    #[test]
    fn a_search_ignores_case_and_does_not_overlap() {
        let t = page(&[glyph("AaAa", 0.0, 10.0, 20.0)]);
        assert_eq!(t.find("aa"), vec![(0, 2), (2, 4)]);
        assert_eq!(t.find(""), vec![]);
    }

    #[test]
    fn context_shows_a_line_break_as_a_return_symbol() {
        let t = page(&[glyph("a", 0.0, 10.0, 5.0), glyph("b", 0.0, 30.0, 5.0)]);
        assert_eq!(t.context(3, 4), ("a\u{23ce}".to_owned(), String::new()));
    }
}
