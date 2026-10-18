//! What an editor paints: the backgrounds a program highlighted stretches
//! with, the selection behind the text, the text in the colours a program
//! gave it with the selected part in its own colour, a line under what an
//! input method is composing, and the cursor.
//!
//! The picture is taken from the layout the editor already has; painting
//! shapes nothing.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, Rect, Value};
use ljos_render::{Decorations, DisplayListBuilder, GlyphRun, ImageHandle, Rgba, kurbo};
use ljos_text::{EditLayout, EditLine};

use super::base::{TextEditor, state_of};
use super::highlight::{self, LinePaint, Lines};
use super::layout::Geometry;
use crate::rectangle::to_rgba;

/// A snapshot of what an editor shows.
#[derive(Clone, Debug, Default)]
pub struct EditorPaint {
    layout: Option<Rc<EditLayout>>,
    geo: Geometry,
    color: Rgba,
    selection_color: Rgba,
    selected_text_color: Rgba,
    /// Bytes of the shown string that are selected.
    selection: Range<usize>,
    /// Bytes of the shown string an input method is composing.
    preedit: Range<usize>,
    /// The cursor, when the editor draws it and it is in its shown half.
    cursor: Option<Rect>,
    /// How the lines a program highlighted are painted.
    highlighted: Option<Lines>,
    /// The colour of each run of rich text, by a layout run's tag less
    /// one.
    run_colors: Option<Arc<Vec<Option<Rgba>>>>,
    /// The pictures of rich text's images, where they are drawn.
    images: Vec<(ImageHandle, kurbo::Rect)>,
    /// The left, top, right and bottom padding the text is clipped to,
    /// for a type given to [`super::register_clip_to_padding`].
    clip_inset: Option<[f64; 4]>,
}

fn color(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> Rgba {
    match engine.read(obj, p) {
        Value::Color(c) => to_rgba(c),
        _ => Rgba::default(),
    }
}

impl EditorPaint {
    /// Reads what the picture depends on (tracked): the colours, whether
    /// the cursor shows, and the count of everything else that changes it
    /// (the layout, the selection, the blink).
    pub(crate) fn read(engine: &Engine, obj: ObjectId) -> EditorPaint {
        let text = color(engine, obj, TextEditor::COLOR.index);
        let selection_color = color(engine, obj, TextEditor::SELECTION_COLOR.index);
        let selected_text_color = color(engine, obj, TextEditor::SELECTED_TEXT_COLOR.index);
        let cursor_visible = engine
            .read(obj, TextEditor::CURSOR_VISIBLE.index)
            .to_boolean();
        let has_delegate = !engine
            .read(obj, TextEditor::CURSOR_DELEGATE.index)
            .is_nullish();
        let clip_inset = super::clips_to_padding(engine, obj).then(|| {
            [
                TextEditor::LEFT_PADDING,
                TextEditor::TOP_PADDING,
                TextEditor::RIGHT_PADDING,
                TextEditor::BOTTOM_PADDING,
            ]
            .map(|p| engine.read(obj, p.index).to_number())
        });
        let Some(st) = state_of(engine, obj) else {
            return EditorPaint {
                clip_inset,
                ..EditorPaint::default()
            };
        };
        // Copied out first: reading the node may run what borrows the
        // state.
        let epoch = st.borrow().paint_epoch;
        if let Some(n) = epoch {
            let _ = engine.graph().get(n);
        }
        // Before the state is borrowed: this may ask a program's
        // highlighter for the spans of a text that changed.
        let highlighted = highlight::lines(&st);
        let run_colors = super::rich::run_colors(&mut st.borrow_mut());
        let s = st.borrow();
        let images = super::rich::placed_images(&s);
        let selection = s.model.selection();
        EditorPaint {
            layout: s.layout.clone(),
            geo: s.geo.clone(),
            color: text,
            selection_color,
            selected_text_color,
            selection: s.shown.byte_of(selection.start)..s.shown.byte_of(selection.end),
            preedit: s.shown.preedit_bytes(),
            cursor: (cursor_visible && s.blink_on && !has_delegate).then_some(s.cursor_rect),
            highlighted,
            run_colors,
            images,
            clip_inset,
        }
    }

    /// Where in an editor of `size` its text is drawn, when it is clipped.
    pub(crate) fn clip(&self, size: kurbo::Size) -> Option<kurbo::Rect> {
        let [l, t, r, b] = self.clip_inset?;
        let (x0, y0) = (l.max(0.0), t.max(0.0));
        Some(kurbo::Rect::new(
            x0,
            y0,
            (size.width - r).max(x0),
            (size.height - b).max(y0),
        ))
    }

    /// The colour a run of the layout is drawn in.
    fn run_color(&self, tag: u32) -> Rgba {
        let index = (tag as usize).wrapping_sub(1);
        self.run_colors
            .as_ref()
            .and_then(|c| c.get(index).copied().flatten())
            .unwrap_or(self.color)
    }

    fn line_x(&self, line: usize) -> f64 {
        self.geo.line_x.get(line).copied().unwrap_or(0.0)
    }

    /// How line `index` is painted when a program highlighted on it.
    fn highlighted(&self, index: usize) -> Option<&LinePaint> {
        self.highlighted.as_ref()?.get(index)?.as_ref()
    }

    pub(crate) fn paint(&self, _size: kurbo::Size, out: &mut DisplayListBuilder) {
        let Some(layout) = &self.layout else {
            return;
        };
        let top = self.geo.top;
        // A highlighted background is behind the selection, which is what
        // the user is doing now.
        for (index, line) in layout.lines.iter().enumerate() {
            let Some(marked) = self.highlighted(index) else {
                continue;
            };
            let x = self.line_x(index);
            for (x0, x1, color) in &marked.backgrounds {
                out.fill_rect(
                    kurbo::Rect::new(x + x0, top + line.top, x + x1, top + line.top + line.height),
                    *color,
                );
            }
        }
        for (line, r) in layout.range_rects(self.selection.clone()) {
            let x = self.line_x(line);
            out.fill_rect(
                kurbo::Rect::new(x + r.x0, top + r.y0, x + r.x1, top + r.y1),
                self.selection_color,
            );
        }
        for (index, line) in layout.lines.iter().enumerate() {
            let x = self.line_x(index);
            let origin = kurbo::Vec2::new(x, top);
            let touches = self.touches(line);
            let Some(marked) = self.highlighted(index) else {
                for run in &line.runs {
                    self.glyphs(
                        &run.run.glyphs,
                        &run.glyph_bytes,
                        self.run_color(run.run.tag),
                        run.run.decorations,
                        touches,
                        origin,
                        out,
                    );
                }
                continue;
            };
            for piece in &marked.pieces {
                self.glyphs(
                    &piece.glyphs,
                    &piece.bytes,
                    piece.color.unwrap_or(self.color),
                    piece.decorations,
                    touches,
                    origin,
                    out,
                );
            }
            let y = top + line.baseline + 1.0;
            for (x0, x1, color) in &marked.underlines {
                out.fill_rect(
                    kurbo::Rect::new(x + x0, y, x + x1, y + 1.0),
                    color.unwrap_or(self.color),
                );
            }
        }
        for (image, rect) in &self.images {
            out.image(image.clone(), *rect);
        }
        // The composition is marked with a line under it, in the colour of
        // the text.
        for (line, r) in layout.range_rects(self.preedit.clone()) {
            let x = self.line_x(line);
            let y = top + layout.lines[line].baseline + 1.0;
            out.fill_rect(kurbo::Rect::new(x + r.x0, y, x + r.x1, y + 1.0), self.color);
        }
        if let Some(c) = self.cursor {
            out.fill_rect(
                kurbo::Rect::new(c.x, c.y, c.x + c.width, c.y + c.height),
                self.color,
            );
        }
    }

    /// Whether part of `line` is selected.
    fn touches(&self, line: &EditLine) -> bool {
        let sel = &self.selection;
        !sel.is_empty() && line.range.start < sel.end && line.range.end > sel.start
    }

    /// Draws glyphs that are of `color` where they are not selected.
    /// `bytes` says for each the byte its character starts at.
    #[allow(clippy::too_many_arguments)]
    fn glyphs(
        &self,
        glyphs: &Arc<GlyphRun>,
        bytes: &[usize],
        color: Rgba,
        decorations: Decorations,
        touches: bool,
        origin: kurbo::Vec2,
        out: &mut DisplayListBuilder,
    ) {
        let sel = &self.selection;
        let selected = |i: usize| bytes.get(i).is_some_and(|b| sel.contains(b));
        let count = glyphs.glyphs.len();
        let picked = if touches {
            (0..count).filter(|i| selected(*i)).count()
        } else {
            0
        };
        if picked == 0 || picked == count {
            // The glyphs are of one colour: the shaped run is drawn as it
            // is, nothing is copied.
            let color = if picked == 0 {
                color
            } else {
                self.selected_text_color
            };
            if color.a > 0.0 {
                out.glyphs(Arc::clone(glyphs), origin, color, decorations);
            }
            return;
        }
        let part = |want: bool| {
            let mut part = GlyphRun::clone(glyphs);
            let mut i = 0;
            part.glyphs.retain(|_| {
                let keep = selected(i) == want;
                i += 1;
                keep
            });
            Arc::new(part)
        };
        // An underline or strike-through runs under both parts and is
        // drawn once, with the text that is not selected.
        out.glyphs(part(false), origin, color, decorations);
        out.glyphs(
            part(true),
            origin,
            self.selected_text_color,
            Decorations::default(),
        );
    }
}
