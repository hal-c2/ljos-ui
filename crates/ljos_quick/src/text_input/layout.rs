//! Where an editor's text is: what is shown (plain, masked or nothing, with
//! an input method's composition in it), the layout of that, and where the
//! layout sits in the item.

use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, EngineError, Font, ObjectId, Rect, Value};
use ljos_render::kurbo;
use ljos_text::{
    EditLayout, EditParams, Ranged, TextModel, TextStyle, WrapMode, byte_to_utf16, utf16_len,
    utf16_to_byte,
};

use super::base::{SharedState, TextEditor, bump};
use super::edit::TextEdit;
use super::input::TextInput;
use super::{cursor_position, echo_mode};
use crate::enums::{align, wrap_mode};
use crate::item::{Item, num, size_follows_implicit, write_computed};
use crate::text::{read_style, text_engine};
use ljos_text::rich::HAlign;

/// How the text is put on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Plain,
    /// One mask character of this many bytes for every UTF-16 unit.
    Masked(usize),
    /// `NoEcho`: nothing is shown.
    Hidden,
}

/// The string that is laid out, and how positions in the text (UTF-16
/// units, as QML counts) map to bytes of it. They differ where the text is
/// masked and where an input method's composition stands in the middle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shown {
    pub text: String,
    mode: Mode,
    /// The cursor and the length of the text, in UTF-16 units.
    cursor: usize,
    len: usize,
    /// The composition is shown at the cursor: its length, and where the
    /// input method's own cursor is in it.
    preedit_units: usize,
    preedit_caret: usize,
}

impl Shown {
    pub fn build(
        model: &TextModel,
        preedit: &str,
        preedit_cursor: Option<usize>,
        echo: Echo,
        multi: bool,
    ) -> Shown {
        let preedit_units = utf16_len(preedit);
        let (cursor, len) = (model.cursor(), model.len());
        let (text, mode) = match echo {
            Echo::Hidden => (String::new(), Mode::Hidden),
            Echo::Masked(c) => (
                std::iter::repeat_n(c, len + preedit_units).collect(),
                Mode::Masked(c.len_utf8()),
            ),
            Echo::Plain => {
                let at = model.cursor_byte();
                let source = model.text();
                let mut text = String::with_capacity(source.len() + preedit.len());
                text.push_str(&source[..at]);
                text.push_str(preedit);
                text.push_str(&source[at..]);
                if !multi && text.contains(['\n', '\r']) {
                    // One line shows a line break as a space; both are one
                    // byte, so positions stay where they are.
                    text = text.replace(['\n', '\r'], " ");
                }
                (text, Mode::Plain)
            }
        };
        Shown {
            text,
            mode,
            cursor,
            len,
            preedit_units,
            preedit_caret: preedit_cursor.map_or(preedit_units, |c| c.min(preedit_units)),
        }
    }

    /// Byte of the shown string at so many UTF-16 units of it.
    fn byte_at_units(&self, units: usize) -> usize {
        match self.mode {
            Mode::Plain => utf16_to_byte(&self.text, units),
            Mode::Masked(unit) => (units * unit).min(self.text.len()),
            Mode::Hidden => 0,
        }
    }

    /// Byte of the shown string where position `pos` of the text is. The
    /// cursor's own position is before the composition.
    pub fn byte_of(&self, pos: usize) -> usize {
        let pos = pos.min(self.len);
        self.byte_at_units(if pos <= self.cursor {
            pos
        } else {
            pos + self.preedit_units
        })
    }

    /// The position of the text a byte of the shown string is at. Inside
    /// the composition that is the cursor.
    pub fn pos_of(&self, byte: usize) -> usize {
        let units = match self.mode {
            Mode::Plain => byte_to_utf16(&self.text, byte.min(self.text.len())),
            Mode::Masked(unit) => byte / unit.max(1),
            Mode::Hidden => return self.len,
        };
        let pos = if units <= self.cursor {
            units
        } else if units < self.cursor + self.preedit_units {
            self.cursor
        } else {
            units - self.preedit_units
        };
        pos.min(self.len)
    }

    /// Where the cursor is drawn: at its position, or where the input
    /// method has its own cursor in the composition.
    pub fn caret_byte(&self) -> usize {
        self.byte_at_units(self.cursor + self.preedit_caret)
    }

    /// Whether what is shown is the text itself, so that a byte of the one
    /// is a byte of the other (but for the composition).
    pub fn is_plain(&self) -> bool {
        self.mode == Mode::Plain
    }

    /// The bytes the composition takes.
    pub fn preedit_bytes(&self) -> std::ops::Range<usize> {
        self.byte_at_units(self.cursor)..self.byte_at_units(self.cursor + self.preedit_units)
    }
}

/// `echoMode` as it applies now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Echo {
    Plain,
    Masked(char),
    Hidden,
}

/// Everything the layout and its place in the item depend on, besides the
/// text itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutInputs {
    style: TextStyle,
    h_align: i32,
    v_align: i32,
    wrap: WrapMode,
    width: f64,
    height: f64,
    follows: [bool; 2],
    /// The item says itself how large it would like to be (a control):
    /// the text is aligned in the size it has, also while that is its
    /// implicit one.
    own_size: bool,
    /// Left, top, right, bottom.
    pad: [f64; 4],
    auto_scroll: bool,
    echo: i32,
    mask: char,
    active: bool,
    /// The font, for the letters it draws the text in.
    font: Arc<Font>,
}

fn wrap(v: i32) -> WrapMode {
    match v {
        wrap_mode::WORD_WRAP => WrapMode::WordWrap,
        wrap_mode::WRAP_ANYWHERE => WrapMode::WrapAnywhere,
        wrap_mode::WRAP => WrapMode::Wrap,
        _ => WrapMode::NoWrap,
    }
}

impl LayoutInputs {
    /// Reads the properties (tracked).
    pub fn read(engine: &Engine, obj: ObjectId, multi: bool) -> LayoutInputs {
        // Highlight spans that change the glyphs lay the text out again.
        let epoch = super::base::state_of(engine, obj).and_then(|st| st.borrow().shape_epoch);
        if let Some(n) = epoch {
            let _ = engine.graph().get(n);
        }
        let int = |p: ljos_engine::PropIndex| engine.read(obj, p).to_number() as i32;
        let flag = |p: ljos_engine::PropIndex| engine.read(obj, p).to_boolean();
        // A TextEdit's `textMargin` is space around the text like padding.
        let margin = if multi {
            num(engine, obj, TextEdit::TEXT_MARGIN.index)
        } else {
            0.0
        };
        let pad = [
            TextEditor::LEFT_PADDING,
            TextEditor::TOP_PADDING,
            TextEditor::RIGHT_PADDING,
            TextEditor::BOTTOM_PADDING,
        ]
        .map(|p| num(engine, obj, p.index) + margin);
        let (echo, mask, auto_scroll) = if multi {
            (echo_mode::NORMAL, ' ', false)
        } else {
            (
                int(TextInput::ECHO_MODE.index),
                engine
                    .read(obj, TextInput::PASSWORD_CHARACTER.index)
                    .to_js_string()
                    .as_str()
                    .chars()
                    .next()
                    .unwrap_or('\u{2022}'),
                flag(TextInput::AUTO_SCROLL.index),
            )
        };
        LayoutInputs {
            style: read_style(engine, obj, TextEditor::FONT.index),
            h_align: int(TextEditor::EFFECTIVE_HORIZONTAL_ALIGNMENT.index),
            v_align: int(TextEditor::VERTICAL_ALIGNMENT.index),
            wrap: if multi {
                wrap(int(TextEditor::WRAP_MODE.index))
            } else {
                WrapMode::NoWrap
            },
            width: num(engine, obj, Item::WIDTH.index),
            height: num(engine, obj, Item::HEIGHT.index),
            follows: [
                size_follows_implicit(engine, obj, true),
                size_follows_implicit(engine, obj, false),
            ],
            own_size: super::has_own_implicit_size(engine, obj),
            pad,
            auto_scroll,
            echo,
            mask,
            // Only an echo mode that depends on it reads the focus.
            active: echo == echo_mode::PASSWORD_ECHO_ON_EDIT && flag(Item::ACTIVE_FOCUS.index),
            font: match engine.read(obj, TextEditor::FONT.index) {
                Value::Font(f) => f,
                _ => Arc::default(),
            },
        }
    }

    fn echo(&self, editing: bool) -> Echo {
        match self.echo {
            echo_mode::NO_ECHO => Echo::Hidden,
            echo_mode::PASSWORD => Echo::Masked(self.mask),
            echo_mode::PASSWORD_ECHO_ON_EDIT if !(editing && self.active) => {
                Echo::Masked(self.mask)
            }
            _ => Echo::Plain,
        }
    }

    /// The width the text has in the item, inside the padding.
    pub fn available_width(&self) -> f64 {
        (self.width - self.pad[0] - self.pad[2]).max(0.0)
    }

    fn params(&self) -> EditParams {
        let wraps = self.wrap != WrapMode::NoWrap && !self.follows[0];
        EditParams {
            width: wraps.then(|| (self.width - self.pad[0] - self.pad[2]).max(0.0)),
            wrap: self.wrap,
        }
    }
}

/// Where the layout sits in the item.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Geometry {
    /// The left edge of every line.
    pub line_x: Vec<f64>,
    /// The top of the first line.
    pub top: f64,
    /// The left edge and the right edge of the document in a multi-line
    /// editor: inside the padding, as wide as the item or as the text.
    pub doc_x: (f64, f64),
    /// The left and the top padding.
    pub pad: (f64, f64),
}

/// The layout the state holds, laying the text out when it has none for
/// what is shown now.
fn layout_of(st: &SharedState, inputs: &LayoutInputs) -> Rc<EditLayout> {
    let params = inputs.params();
    let mut held = st.borrow_mut();
    let s = &mut *held;
    let echo = inputs.echo(s.editing);
    let mut shown = Shown::build(&s.model, &s.preedit, s.preedit_cursor, echo, s.multi);
    // Positions are counted in the shown string, and every letter stays
    // one of the same length in them. A mask is not a letter of the text.
    if echo == Echo::Plain
        && let std::borrow::Cow::Owned(drawn) = inputs.font.drawn(&shown.text)
    {
        shown.text = drawn;
    }
    // An image's character is laid out as nothing; its box takes the room.
    if s.rich.is_some()
        && let std::borrow::Cow::Owned(text) = ljos_text::rich::RichText::shown(&shown.text)
    {
        shown.text = text;
    }
    let same_text = s.shown.text == shown.text;
    if !same_text {
        // Spans a highlighter made are those of the text that was shown.
        s.highlights.stale = true;
    }
    s.shown = shown;
    // Spans count in the text, so only plain text is laid out with them,
    // and rich text has fonts of its own.
    let highlighted = s.rich.is_none() && s.shown.is_plain();
    let pre = s.shown.preedit_bytes();
    drop(held);
    // With the state free: this may ask a program's highlighter.
    let spans = if highlighted {
        super::highlight::style_spans(
            &super::highlight::current(st),
            &inputs.style,
            (pre.start, pre.len()),
        )
    } else {
        Vec::new()
    };
    let mut held = st.borrow_mut();
    let s = &mut *held;
    let fresh = s.layout.is_some()
        && same_text
        && s.laid.as_ref() == Some(&(inputs.style.clone(), params))
        && s.laid_spans == spans;
    if !fresh {
        let layout = match &s.rich {
            Some(r) => {
                // The composition stands at the cursor and is set as the
                // text before it.
                let (at, extra) = (s.model.cursor_byte(), s.preedit.len());
                let shift = |b: usize| if b >= at { b + extra } else { b };
                let spans = r.doc.style_spans(&inputs.style, shift);
                let objects = r
                    .doc
                    .objects(shift, |im| super::rich::image_size(&r.images, im));
                let ranged = Ranged {
                    spans: &spans,
                    objects: &objects,
                    strut: false,
                };
                match super::rich::edit_blocks(r) {
                    Some(blocks) => text_engine().borrow_mut().edit_layout_blocks(
                        &s.shown.text,
                        &inputs.style,
                        &ranged,
                        &params,
                        &blocks,
                    ),
                    None => text_engine().borrow_mut().edit_layout_ranged(
                        &s.shown.text,
                        &inputs.style,
                        &ranged,
                        &params,
                    ),
                }
            }
            None if !spans.is_empty() => text_engine().borrow_mut().edit_layout_ranged(
                &s.shown.text,
                &inputs.style,
                &Ranged {
                    spans: &spans,
                    ..Ranged::NONE
                },
                &params,
            ),
            None => text_engine()
                .borrow_mut()
                .edit_layout(&s.shown.text, &inputs.style, &params),
        };
        s.layout = Some(Rc::new(layout));
        s.laid = Some((inputs.style.clone(), params));
        s.laid_spans = spans;
    }
    Rc::clone(s.layout.as_ref().expect("set above"))
}

fn vertical_offset(v_align: i32, height: f64, content_height: f64) -> f64 {
    if v_align & align::BOTTOM != 0 {
        height - content_height
    } else if v_align & align::V_CENTER != 0 {
        (height - content_height) / 2.0
    } else {
        0.0
    }
}

fn horizontal_offset(h_align: i32, free: f64) -> f64 {
    if h_align & align::RIGHT != 0 {
        free
    } else if h_align & align::H_CENTER != 0 {
        free / 2.0
    } else {
        0.0
    }
}

/// Lays the text out (when what is shown changed), places it in the item
/// and writes what follows from that: sizes, `cursorRectangle`, the
/// baseline and `displayText` or `lineCount`.
pub(crate) fn relayout(
    engine: &Engine,
    obj: ObjectId,
    st: &SharedState,
    inputs: &LayoutInputs,
) -> Result<(), EngineError> {
    let layout = layout_of(st, inputs);
    let multi = st.borrow().multi;
    let [left, top_pad, right, bottom] = inputs.pad;
    let (content_width, content_height) = (layout.content_width, layout.content_height);
    let caret = layout.caret(st.borrow().shown.caret_byte());

    // An item that follows its text is as large as the text, whatever its
    // size still says while the new implicit size is on its way. A control
    // sizes its content item without the item ceasing to follow; that size
    // is not the one the text last asked for, and it is the room there is.
    let sized_by_text = |axis: usize, size: f64, implicit: ljos_engine::PropIndex| {
        inputs.follows[axis]
            && !inputs.own_size
            // A field is narrower than its implicit width, by what that
            // was rounded up.
            && (size - engine.peek(obj, implicit).to_number()).abs() < 1.0
    };
    let follows = [
        sized_by_text(0, inputs.width, Item::IMPLICIT_WIDTH.index),
        sized_by_text(1, inputs.height, Item::IMPLICIT_HEIGHT.index),
    ];

    let implicit_width;
    let mut geo = Geometry::default();
    if multi {
        implicit_width = layout.natural_width + left + right;
        let avail = if follows[0] {
            content_width
        } else {
            inputs.width - left - right
        };
        geo.doc_x = (left, left + avail.max(0.0));
        // A paragraph of rich text may have an alignment of its own.
        let held = st.borrow();
        let aligns = super::rich::block_aligns(&held);
        let shown = &held.shown.text;
        let (mut block, mut counted) = (0, 0);
        geo.line_x = layout
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                // A paragraph that stands in is aligned in what it leaves.
                let (indent, inset) = layout
                    .block_of_line(index)
                    .map_or((0.0, 0.0), |b| (b.indent, b.indent + b.indent_right));
                let h_align = match &aligns {
                    Some(aligns) => {
                        let upto = line.range.start.min(shown.len());
                        block += shown.as_bytes()[counted.min(upto)..upto]
                            .iter()
                            .filter(|b| **b == b'\n')
                            .count();
                        counted = counted.max(upto);
                        match aligns.get(block).copied().flatten() {
                            Some(HAlign::Left) => align::LEFT,
                            Some(HAlign::Right) => align::RIGHT,
                            Some(HAlign::Center) => align::H_CENTER,
                            Some(HAlign::Justify) => align::JUSTIFY,
                            None => inputs.h_align,
                        }
                    }
                    None => inputs.h_align,
                };
                left + indent + horizontal_offset(h_align, avail - inset - line.width)
            })
            .collect();
    } else {
        // A field's implicit width is a whole number of pixels, as in Qt;
        // see `followed_width` for the width.
        implicit_width = content_width.ceil() + left + right;
        let avail = if follows[0] {
            content_width
        } else {
            inputs.width - left - right
        };
        let mut s = st.borrow_mut();
        s.natural_width =
            (!inputs.own_size).then_some((implicit_width, content_width + left + right));
        let x = if content_width <= avail {
            s.hscroll = 0.0;
            // Qt holds the free room in 64ths of a pixel and halves it
            // whole, so centred text starts up to a 128th left of the
            // middle: measured on Qt 6.11 (features/text-input/geometry.feature).
            let free = (avail - content_width) * 64.0;
            if inputs.h_align & align::H_CENTER != 0 {
                (free.round() / 2.0).floor() / 64.0
            } else {
                horizontal_offset(inputs.h_align, free / 64.0)
            }
        } else if inputs.auto_scroll {
            // The text moves as little as keeps the cursor inside.
            let mut scroll = s.hscroll;
            if caret.x - scroll > avail {
                scroll = caret.x - avail;
            }
            if caret.x - scroll < 0.0 {
                scroll = caret.x;
            }
            s.hscroll = scroll.clamp(0.0, content_width - avail);
            -s.hscroll
        } else {
            // Only `ensureVisible` moves a text that does not follow its
            // cursor.
            s.hscroll = s.hscroll.clamp(0.0, content_width - avail);
            -s.hscroll
        };
        geo.line_x = vec![left + x];
    }
    let room = if follows[1] {
        content_height
    } else {
        inputs.height - top_pad - bottom
    };
    geo.top = top_pad + vertical_offset(inputs.v_align, room, content_height);
    geo.pad = (left, top_pad);

    let cursor = Rect {
        x: geo.line_x.get(caret.line).copied().unwrap_or(left) + caret.x,
        y: geo.top + caret.top,
        width: 1.0,
        height: caret.height,
    };
    let (shown_text, epochs) = {
        let mut s = st.borrow_mut();
        s.geo = geo.clone();
        s.cursor_rect = cursor;
        (s.shown.text.clone(), [s.layout_epoch, s.paint_epoch])
    };
    engine.batch(|| -> Result<(), EngineError> {
        let put = |p: ljos_engine::PropIndex, v: Value| write_computed(engine, obj, p, v);
        put(TextEditor::CONTENT_WIDTH.index, Value::Real(content_width))?;
        put(
            TextEditor::CONTENT_HEIGHT.index,
            Value::Real(content_height),
        )?;
        if !super::has_own_implicit_size(engine, obj) {
            put(Item::IMPLICIT_WIDTH.index, Value::Real(implicit_width))?;
            put(
                Item::IMPLICIT_HEIGHT.index,
                Value::Real(content_height + top_pad + bottom),
            )?;
        }
        put(
            TextEditor::CURSOR_RECTANGLE.index,
            Value::Rect(std::sync::Arc::new(cursor)),
        )?;
        put(
            Item::BASELINE_OFFSET.index,
            Value::Real(layout.baseline + geo.top),
        )?;
        if multi {
            put(
                TextEdit::LINE_COUNT.index,
                Value::Int(layout.lines.len() as i32),
            )?;
            put(TextEdit::PAINTED_WIDTH.index, Value::Real(content_width))?;
            put(TextEdit::PAINTED_HEIGHT.index, Value::Real(content_height))?;
        } else {
            put(
                TextInput::DISPLAY_TEXT.index,
                Value::from(shown_text.as_str()),
            )?;
        }
        Ok(())
    })?;
    for n in epochs.into_iter().flatten() {
        bump(engine, n);
    }
    Ok(())
}

/// `positionToRectangle`: where the cursor would stand at `pos`. A position
/// the text does not have gives an empty rectangle. A `TextInput` leaves
/// its left and top padding out, unlike its `cursorRectangle` and unlike a
/// `TextEdit`, as Qt 6.11 does (features/text-input/geometry.feature).
pub(crate) fn position_rect(st: &SharedState, pos: i32) -> Rect {
    let s = st.borrow();
    let (Some(layout), Ok(pos)) = (s.layout.as_ref(), usize::try_from(pos)) else {
        return Rect::default();
    };
    if pos > s.model.len() {
        return Rect::default();
    }
    let caret = layout.caret(s.shown.byte_of(pos));
    let (left, top) = if s.multi { (0.0, 0.0) } else { s.geo.pad };
    Rect {
        x: s.geo.line_x.get(caret.line).copied().unwrap_or(0.0) + caret.x - left,
        y: s.geo.top + caret.top - top,
        width: 1.0,
        height: caret.height,
    }
}

/// `positionAt`: the position nearest a point of the item. One line takes
/// every `y` as on it.
pub(crate) fn position_at(st: &SharedState, at: kurbo::Point, mode: i32) -> usize {
    let s = st.borrow();
    let Some(layout) = s.layout.as_ref() else {
        return 0;
    };
    let on_character = mode == cursor_position::ON_CHARACTER;
    let line = if s.multi {
        let y = at.y - s.geo.top;
        if y < 0.0 {
            return 0;
        }
        if y >= layout.content_height {
            return s.model.len();
        }
        if let Some(grid) = layout.grid_at(y) {
            let x = at.x - s.geo.line_x.get(grid.lines.start).copied().unwrap_or(0.0);
            return s.shown.pos_of(layout.hit_grid(grid, x, y, on_character));
        }
        let index = layout.line_at(y);
        // Between two paragraphs that stand apart, measured on Qt 6.11: a
        // pixel under the one is its end, and the rest of the room the
        // start of the next, wherever across.
        if let Some(line) = layout.lines.get(index)
            && let Some(next) = layout.lines.get(index + 1)
            && !layout.blocks.is_empty()
            && y >= line.top + line.height
        {
            let byte = if y < line.top + line.height + 1.0 {
                line.end
            } else {
                next.range.start
            };
            return s.shown.pos_of(byte);
        }
        index
    } else {
        0
    };
    let x = at.x - s.geo.line_x.get(line).copied().unwrap_or(0.0);
    s.shown.pos_of(layout.hit_line(line, x, on_character))
}

/// The position of the character under a point of a multi-line editor, if
/// there is one. Above the text there is none; below it the last line is
/// read, as Qt 6.11 does for a drag that selects words.
pub(crate) fn character_at(st: &SharedState, at: kurbo::Point) -> Option<usize> {
    let s = st.borrow();
    let layout = s.layout.as_ref()?;
    let y = at.y - s.geo.top;
    if y < 0.0 || layout.lines.is_empty() {
        return None;
    }
    let line = if y >= layout.content_height {
        layout.lines.len() - 1
    } else {
        layout.line_at(y)
    };
    let x = at.x - s.geo.line_x.get(line).copied().unwrap_or(0.0);
    let range = layout.cluster_at(line, x)?;
    Some(s.shown.pos_of(range.start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(text: &str, cursor: usize) -> TextModel {
        let mut m = TextModel::new();
        m.set_text(text);
        m.move_to(cursor, false);
        m
    }

    #[test]
    fn plain_text_maps_positions_to_bytes() {
        let s = Shown::build(&model("a\u{e9}\u{1f600}b", 1), "", None, Echo::Plain, false);
        assert_eq!(s.text, "a\u{e9}\u{1f600}b");
        assert_eq!(s.byte_of(0), 0);
        assert_eq!(s.byte_of(2), 3);
        // The emoji is two units and four bytes.
        assert_eq!(s.byte_of(4), 7);
        assert_eq!(s.pos_of(7), 4);
        assert_eq!(s.pos_of(3), 2);
        assert_eq!(s.caret_byte(), 1);
    }

    #[test]
    fn a_composition_stands_at_the_cursor() {
        let s = Shown::build(&model("abcd", 2), "ni", None, Echo::Plain, false);
        assert_eq!(s.text, "abnicd");
        assert_eq!(s.preedit_bytes(), 2..4);
        // The cursor is drawn after the composition; its position is
        // before it, and what follows is moved along.
        assert_eq!(s.caret_byte(), 4);
        assert_eq!(s.byte_of(2), 2);
        assert_eq!(s.byte_of(3), 5);
        assert_eq!(s.pos_of(3), 2);
        assert_eq!(s.pos_of(5), 3);
        let s = Shown::build(&model("abcd", 2), "ni", Some(1), Echo::Plain, false);
        assert_eq!(s.caret_byte(), 3);
    }

    #[test]
    fn masked_text_is_one_mask_per_unit() {
        let s = Shown::build(
            &model("a\u{1f600}", 3),
            "",
            None,
            Echo::Masked('\u{2022}'),
            false,
        );
        assert_eq!(s.text, "\u{2022}\u{2022}\u{2022}");
        assert_eq!(s.byte_of(1), 3);
        assert_eq!(s.pos_of(6), 2);
        assert_eq!(s.caret_byte(), 9);
    }

    #[test]
    fn hidden_text_shows_nothing() {
        let s = Shown::build(&model("abc", 1), "", None, Echo::Hidden, false);
        assert_eq!(s.text, "");
        assert_eq!(s.byte_of(2), 0);
        assert_eq!(s.pos_of(0), 3);
    }

    #[test]
    fn one_line_shows_a_line_break_as_a_space() {
        let s = Shown::build(&model("a\nb", 0), "", None, Echo::Plain, false);
        assert_eq!(s.text, "a b");
        let s = Shown::build(&model("a\nb", 0), "", None, Echo::Plain, true);
        assert_eq!(s.text, "a\nb");
    }
}
