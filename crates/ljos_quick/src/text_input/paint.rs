//! What an editor paints: the selection behind the text, the text with the
//! selected part in its own colour, a line under what an input method is
//! composing, and the cursor.
//!
//! The picture is taken from the layout the editor already has; painting
//! shapes nothing.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, Rect, Value};
use ljos_render::{Decorations, DisplayListBuilder, GlyphRun, Rgba, kurbo};
use ljos_text::{EditLayout, EditLine};

use super::base::{TextEditor, state_of};
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
        let Some(st) = state_of(engine, obj) else {
            return EditorPaint::default();
        };
        // Copied out first: reading the node may run what borrows the
        // state.
        let epoch = st.borrow().paint_epoch;
        if let Some(n) = epoch {
            let _ = engine.graph().get(n);
        }
        let s = st.borrow();
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
        }
    }

    fn line_x(&self, line: usize) -> f64 {
        self.geo.line_x.get(line).copied().unwrap_or(0.0)
    }

    pub(crate) fn paint(&self, _size: kurbo::Size, out: &mut DisplayListBuilder) {
        let Some(layout) = &self.layout else {
            return;
        };
        let top = self.geo.top;
        for (line, r) in layout.range_rects(self.selection.clone()) {
            let x = self.line_x(line);
            out.fill_rect(
                kurbo::Rect::new(x + r.x0, top + r.y0, x + r.x1, top + r.y1),
                self.selection_color,
            );
        }
        for (index, line) in layout.lines.iter().enumerate() {
            let origin = kurbo::Vec2::new(self.line_x(index), top);
            self.glyphs(line, origin, out);
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

    fn glyphs(&self, line: &EditLine, origin: kurbo::Vec2, out: &mut DisplayListBuilder) {
        let sel = &self.selection;
        let touches = !sel.is_empty() && line.range.start < sel.end && line.range.end > sel.start;
        for run in &line.runs {
            let selected = |i: usize| run.glyph_bytes.get(i).is_some_and(|b| sel.contains(b));
            let count = run.run.glyphs.glyphs.len();
            let picked = if touches {
                (0..count).filter(|i| selected(*i)).count()
            } else {
                0
            };
            if picked == 0 || picked == count {
                // The run is of one colour: the shaped run is drawn as it
                // is, nothing is copied.
                let color = if picked == 0 {
                    self.color
                } else {
                    self.selected_text_color
                };
                if color.a > 0.0 {
                    out.glyphs(
                        Arc::clone(&run.run.glyphs),
                        origin,
                        color,
                        run.run.decorations,
                    );
                }
                continue;
            }
            let part = |want: bool| {
                let mut part = GlyphRun::clone(&run.run.glyphs);
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
            out.glyphs(part(false), origin, self.color, run.run.decorations);
            out.glyphs(
                part(true),
                origin,
                self.selected_text_color,
                Decorations::default(),
            );
        }
    }
}
