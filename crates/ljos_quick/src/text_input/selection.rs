//! `TextEdit.cursorSelection` (doc.qt.io, `qml-qtquick-textselection.html`):
//! the selection, or the cursor without one, as something a toolbar reads
//! and writes. Reading gives the format there; writing `font`, `color` or
//! `alignment` formats the selected text (or, for `alignment`, its
//! paragraphs) as one step of the history, and writing `text` replaces the
//! selection.
//!
//! A font given without a selection is the format of what is typed next at
//! the cursor. In plain text there are no formats: what is read is the
//! editor's own font and colour, and writing them does nothing.

use std::sync::Arc;

use ljos_engine::{Atom, Color, Engine, EngineError, Font, ObjectId, QmlObject, Value};
use ljos_text::rich::{BlockFormat, CharFormat, HAlign, RichText};

use super::base::{EditKind, SharedState, TextEditor, check, publish, state_of};
use super::edit::TextEdit;
use super::ops::{self, How};
use super::rich::{color_name, color_of, font_of, format_step};
use crate::enums::align;
use crate::text::text_style;

/// What `cursorSelection` of a `TextEdit` is. Qt lets QML make one, so
/// Ljós does too; one that no editor made selects nothing, and what is
/// written to it stays there.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "TextSelection")]
pub struct TextSelection {
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub font: Font,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify, flags = "Qt.Alignment")]
    pub alignment: i32,
}

/// What the selection last showed, so that writing it is not taken for a
/// program's write.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    text: String,
    font: Font,
    color: Color,
    alignment: i32,
}

fn to_align(a: Option<HAlign>) -> i32 {
    match a {
        Some(HAlign::Right) => align::RIGHT,
        Some(HAlign::Center) => align::H_CENTER,
        Some(HAlign::Justify) => align::JUSTIFY,
        _ => align::LEFT,
    }
}

fn from_align(a: i32) -> HAlign {
    if a & align::RIGHT != 0 {
        HAlign::Right
    } else if a & align::H_CENTER != 0 {
        HAlign::Center
    } else if a & align::JUSTIFY != 0 {
        HAlign::Justify
    } else {
        HAlign::Left
    }
}

/// The editor's font as a run of format `f` has it.
fn font_with(base: &Font, f: &CharFormat) -> Font {
    let mut font = base.clone();
    let i = &f.inline;
    if i.bold {
        font.weight = 700;
    }
    font.italic |= i.italic;
    font.underline |= i.underline;
    font.strikeout |= i.strike;
    if let Some(family) = &i.family {
        font.family = Atom::from(&**family);
    }
    let base_px = text_style(base).pixel_size;
    let px = i.pixel_size.unwrap_or(base_px) * i.scale;
    if (px - base_px).abs() > 0.01 {
        font.pixel_size = px.round() as i32;
        font.point_size = -1.0;
    }
    font
}

/// The format the selection reads as: that of its first character, or
/// without one what would be typed at the cursor.
fn format_here(s: &super::base::State) -> Option<Arc<CharFormat>> {
    let r = s.rich.as_ref()?;
    let sel = s.model.selection_bytes();
    Some(if sel.is_empty() {
        match &r.typing {
            Some((at, f)) if *at == sel.start => Arc::clone(f),
            _ => r.doc.cursor_format(sel.start),
        }
    } else {
        r.doc.format_at(sel.start)
    })
}

fn current(engine: &Engine, obj: ObjectId, st: &SharedState) -> Shown {
    let base_font = font_of(engine, obj);
    let base_color = match engine.untrack(|| engine.read(obj, TextEditor::COLOR.index)) {
        Value::Color(c) => c,
        _ => Color::BLACK,
    };
    let s = st.borrow();
    let selected = s.model.selected_text();
    let Some(r) = s.rich.as_ref() else {
        return Shown {
            text: selected.to_owned(),
            font: (*base_font).clone(),
            color: base_color,
            alignment: align::LEFT,
        };
    };
    let f = format_here(&s).unwrap_or_default();
    let at = s.model.selection_bytes().start;
    let block = r
        .doc
        .blocks()
        .get(r.doc.block_index(at))
        .and_then(|b| b.align);
    Shown {
        text: RichText::plain_of(selected),
        font: font_with(&base_font, &f),
        color: color_of(&f).unwrap_or(base_color),
        alignment: to_align(block),
    }
}

/// Writes what the selection is now to `cursorSelection`.
pub(crate) fn sync(engine: &Engine, obj: ObjectId, st: &SharedState) {
    let Some(sel) = st.borrow().selection.filter(|o| engine.is_alive(*o)) else {
        return;
    };
    let now = current(engine, obj, st);
    if st.borrow().selection_shown.as_ref() == Some(&now) {
        return;
    }
    st.borrow_mut().selection_shown = Some(now.clone());
    let r = engine.batch(|| -> Result<(), EngineError> {
        let put = |p: ljos_engine::PropIndex, v: Value| -> Result<(), EngineError> {
            let old = engine.untrack(|| engine.read(sel, p));
            if old.change_equals(&v) {
                return Ok(());
            }
            engine.write_derived(sel, p, v)
        };
        put(TextSelection::TEXT.index, Value::from(now.text.as_str()))?;
        put(TextSelection::FONT.index, Value::Font(Arc::new(now.font)))?;
        put(TextSelection::COLOR.index, Value::Color(now.color))?;
        put(TextSelection::ALIGNMENT.index, Value::Int(now.alignment))
    });
    check(engine, "cursorSelection", r);
}

/// Makes `cursorSelection` of a `TextEdit` that completes.
pub(crate) fn init(engine: &Engine, edit: ObjectId, st: &SharedState) -> Result<(), EngineError> {
    let sel = engine.create_native_incomplete(
        TextSelection {
            alignment: align::LEFT,
            ..TextSelection::default()
        },
        Some(edit),
    )?;
    st.borrow_mut().selection = Some(sel);
    engine.set_internal(edit, TextEdit::CURSOR_SELECTION, Some(sel))?;

    // A write of the program's, not one of `sync`: it differs from what
    // was shown.
    let written = move |e: &Engine, pick: fn(&Shown, &Value) -> bool, v: &Value| {
        let st = state_of(e, edit)?;
        let ours = st
            .borrow()
            .selection_shown
            .as_ref()
            .is_some_and(|s| pick(s, v));
        (!ours).then_some(st)
    };
    engine.watch(sel, TextSelection::FONT.index, move |e, v| {
        let Value::Font(font) = v else { return };
        if written(e, |s, v| matches!(v, Value::Font(f) if **f == s.font), v).is_some() {
            set_font(e, edit, font);
        }
    })?;
    engine.watch(sel, TextSelection::COLOR.index, move |e, v| {
        let Value::Color(c) = v else { return };
        if written(e, |s, v| matches!(v, Value::Color(c) if *c == s.color), v).is_some() {
            let name = color_name(*c);
            format(e, edit, move |doc, range| {
                doc.set_char_format(range, |f| {
                    let mut f = f.clone();
                    f.inline.color = Some(Arc::clone(&name));
                    f
                });
            });
        }
    })?;
    engine.watch(sel, TextSelection::ALIGNMENT.index, move |e, v| {
        let a = v.to_number() as i32;
        if written(e, |s, v| v.to_number() as i32 == s.alignment, v).is_some() {
            let h = from_align(a);
            format(e, edit, move |doc, range| {
                doc.set_block_format(range, |b| BlockFormat {
                    align: Some(h),
                    ..b.clone()
                });
            });
        }
    })?;
    engine.watch(sel, TextSelection::TEXT.index, move |e, v| {
        let text = v.to_js_string();
        let same = |s: &Shown, v: &Value| v.to_js_string().as_str() == s.text;
        if written(e, same, v).is_some() {
            let how = How {
                kind: EditKind::Other,
                user: false,
                emits: false,
                start: None,
                end: None,
                anywhere: false,
            };
            // A paragraph separator written back is a paragraph again.
            let text = text.as_str().replace('\u{2029}', "\n");
            ops::edit(e, edit, how, |m| m.replace_selection(&text));
        }
    })?;
    sync(engine, edit, st);
    Ok(())
}

/// Formats the selection as one step of the history and shows it.
fn format(engine: &Engine, edit: ObjectId, f: impl FnOnce(&mut RichText, std::ops::Range<usize>)) {
    let Some(st) = ops::ready(engine, edit) else {
        return;
    };
    format_step(&mut st.borrow_mut(), f);
    check(engine, "cursorSelection", publish(engine, edit, &st));
}

/// `cursorSelection.font = ...`: the selected text in that font, or
/// without a selection what is typed next.
fn set_font(engine: &Engine, edit: ObjectId, font: &Font) {
    let base = font_of(engine, edit);
    let (base_px, px) = (text_style(&base).pixel_size, text_style(font).pixel_size);
    let font = font.clone();
    let map = move |f: &CharFormat| -> CharFormat {
        let mut inline = f.inline.clone();
        inline.bold = font.weight >= 600;
        inline.italic = font.italic;
        inline.underline = font.underline;
        inline.strike = font.strikeout;
        inline.family = (!font.family.is_empty() && font.family != base.family)
            .then(|| Arc::from(font.family.as_str()));
        if (px - base_px * inline.scale).abs() > 0.01 {
            inline.pixel_size = Some(px / inline.scale.max(0.01));
        }
        CharFormat {
            inline,
            whole_font: true,
            image: f.image.clone(),
        }
    };
    let Some(st) = ops::ready(engine, edit) else {
        return;
    };
    {
        let mut s = st.borrow_mut();
        if s.rich.is_none() {
            return;
        }
        if s.model.has_selection() {
            format_step(&mut s, |doc, range| doc.set_char_format(range, &map));
        } else {
            let here = format_here(&s).unwrap_or_default();
            let at = s.model.cursor_byte();
            if let Some(r) = s.rich.as_mut() {
                r.typing = Some((at, Arc::new(map(&here))));
            }
        }
    }
    check(engine, "cursorSelection", publish(engine, edit, &st));
}
