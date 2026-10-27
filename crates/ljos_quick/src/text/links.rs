//! The links of a `Text`: what the pointer and the keyboard do with them,
//! and what an assistive technology is told.
//!
//! A link reports its address and nothing more (`linkActivated`): what an
//! address means is the program's to decide, and a document cannot make the
//! toolkit open anything.
//!
//! A `Text` takes the pointer only while its document has links, and then
//! only a press that lands on one: a press beside them goes to whatever
//! lies beneath, as in Qt (measured on qml6).

use std::any::TypeId;
use std::sync::Arc;

use ljos_a11y::accesskit::Role as AkRole;
use ljos_engine::{Atom, Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_render::kurbo;

use super::{Text, rich};
use crate::accessible::{AccessiblePart, register_accessible_parts};
use crate::focus::{FocusReason, focus_reason};
use crate::input::{KeyEvent, Modifiers, MouseButtons, PointerEvent};
use crate::item::write_computed;
use crate::keys::{KeyItem, key, register_key_item, tab_direction};
use crate::pointer_item::{
    PointerItem, register_hover_move, register_pointer_condition, register_pointer_item,
};

/// The address of the link at a point of the item.
pub(crate) fn link_at(engine: &Engine, obj: ObjectId, at: kurbo::Point) -> Option<Arc<str>> {
    let shared = rich::state(engine, obj)?;
    if !shared.borrow().has_links {
        return None;
    }
    let (shared, view, top) = rich::current(engine, obj)?;
    let s = shared.borrow();
    s.doc.link_at(at.x, at.y - top, &view).map(Arc::from)
}

/// The links of `obj` in reading order, as the parts of it an assistive
/// technology can go to. Every read is tracked: called in an effect, the
/// effect runs again when the links change or move.
fn parts(engine: &Engine, obj: ObjectId) -> Vec<AccessiblePart> {
    if !engine.is_alive(obj) {
        return Vec::new();
    }
    let t = super::TextInputs::read(engine, obj);
    let Some(handle) = t.rich.clone() else {
        return Vec::new();
    };
    if !handle.has_links() {
        return Vec::new();
    }
    let te = super::text_engine();
    let metrics = handle.layout(&mut te.borrow_mut(), &t);
    let width = crate::item::num(engine, obj, crate::item::Item::WIDTH.index);
    let height = crate::item::num(engine, obj, crate::item::Item::HEIGHT.index);
    let view = ljos_text::rich::View {
        width,
        align: rich::h_align(t.h_align),
    };
    let down = kurbo::Vec2::new(
        0.0,
        super::vertical_offset(t.v_align, height, metrics.content_height),
    );
    let s = handle.0.borrow();
    s.doc
        .links(&view)
        .into_iter()
        .filter_map(|link| {
            let first = *link.rects.first()?;
            let rect = link.rects.iter().fold(first, |a, b| a.union(*b));
            Some(AccessiblePart {
                role: AkRole::Link,
                name: link.text,
                rect: rect + down,
                at: first.center() + down,
            })
        })
        .collect()
}

fn is_left(ev: &PointerEvent) -> bool {
    ev.button.is_none_or(|b| b == MouseButtons::LEFT)
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !is_left(ev) {
        return false;
    }
    let link = link_at(engine, obj, at);
    let took = link.is_some();
    if let Some(shared) = rich::state(engine, obj) {
        shared.borrow_mut().pressed = link;
    }
    took
}

fn moved(_engine: &Engine, _obj: ObjectId, _ev: &PointerEvent, _at: kurbo::Point) {}

fn release(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(shared) = rich::state(engine, obj) else {
        return;
    };
    let pressed = shared.borrow_mut().pressed.take();
    // Released where it was pressed: on the same link.
    if let Some(pressed) = pressed
        && link_at(engine, obj, at).as_deref() == Some(&*pressed)
    {
        activate(engine, obj, &pressed);
    }
}

fn cancel(engine: &Engine, obj: ObjectId) {
    if let Some(shared) = rich::state(engine, obj) {
        shared.borrow_mut().pressed = None;
    }
}

fn activate(engine: &Engine, obj: ObjectId, href: &str) {
    let r = engine.emit_by_name(obj, "linkActivated", &[Value::String(Atom::from(href))]);
    if let Err(err) = r {
        engine.log(
            ljos_engine::LogLevel::Warning,
            crate::scene::SCENE_CATEGORY,
            format!("linkActivated: {err}"),
        );
    }
}

/// Makes `hoveredLink` say `href` and tells of the change.
fn set_hovered(engine: &Engine, obj: ObjectId, href: &str) {
    let p = Text::HOVERED_LINK.index;
    let old = engine.untrack(|| engine.read(obj, p)).to_js_string();
    if old.as_str() == href {
        return;
    }
    let href = Atom::from(href);
    if write_computed(engine, obj, p, Value::String(href.clone())).is_ok() {
        let _ = engine.emit_by_name(obj, "linkHovered", &[Value::String(href)]);
    }
}

fn hover(engine: &Engine, obj: ObjectId, over: bool) {
    if !over {
        set_hovered(engine, obj, "");
    }
}

fn hover_moved(engine: &Engine, obj: ObjectId, at: kurbo::Point) {
    let link = link_at(engine, obj, at);
    set_hovered(engine, obj, link.as_deref().unwrap_or(""));
}

/// Whether a `Text` takes the pointer just now. A type that derives from
/// `Text` natively (a control's label) says for itself when it does: the
/// answer here is for `Text` and what QML derives from it.
fn takes_pointer(engine: &Engine, obj: ObjectId) -> bool {
    let native = engine
        .meta_of(obj)
        .and_then(|meta| meta.chain().find_map(|m| m.native));
    if native != Some(TypeId::of::<Text>()) {
        return true;
    }
    rich::state(engine, obj).is_some_and(|s| s.borrow().has_links)
}

/// The link the keyboard starts on when it comes to a `Text` with links:
/// the first for Tab, the last for Shift+Tab, none when it came some other
/// way (a click shows no frame; the first Tab then goes to the first link).
pub(crate) fn arrive(engine: &Engine, obj: ObjectId, handle: &rich::RichHandle) {
    let mut s = handle.0.borrow_mut();
    if s.keyboard {
        return;
    }
    s.keyboard = true;
    s.key_link = match engine.untrack(|| focus_reason(engine, obj)) {
        FocusReason::Tab => Some(0),
        // Held to the last link wherever the links are counted.
        FocusReason::Backtab => Some(usize::MAX),
        _ => None,
    };
}

/// Tab goes from link to link and leaves the item after the last one;
/// Return, Enter and Space activate the link the keyboard is on.
fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    let Some(shared) = rich::state(engine, obj) else {
        return false;
    };
    if !shared.borrow().has_links {
        return false;
    }
    let Some((shared, view, _)) = rich::current(engine, obj) else {
        return false;
    };
    let (count, on) = {
        let s = shared.borrow();
        let count = s.doc.links(&view).len();
        (count, s.key_link.map(|i| i.min(count.saturating_sub(1))))
    };
    if count == 0 {
        return false;
    }
    if let Some(forward) = tab_direction(ev) {
        let next = match (on, forward) {
            (None, true) => Some(0),
            (None, false) => Some(count - 1),
            (Some(i), true) => (i + 1 < count).then_some(i + 1),
            (Some(i), false) => i.checked_sub(1),
        };
        // Past either end the key is not taken: focus moves on.
        let Some(next) = next else {
            return false;
        };
        shared.borrow_mut().key_link = Some(next);
        rich::bump(engine, obj, &shared);
        return true;
    }
    let plain = !ev
        .modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT);
    if plain
        && matches!(ev.key, key::RETURN | key::ENTER | key::SPACE)
        && let Some(i) = on
    {
        let href = shared
            .borrow()
            .doc
            .links(&view)
            .get(i)
            .map(|l| l.href.clone());
        if let Some(href) = href {
            activate(engine, obj, &href);
            return true;
        }
    }
    false
}

fn no_key(_engine: &Engine, _obj: ObjectId, _ev: &KeyEvent) -> bool {
    false
}

pub(crate) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(Text::static_meta());
    register_pointer_item(
        engine,
        meta(),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
    register_pointer_condition(engine, meta(), takes_pointer);
    register_hover_move(engine, meta(), hover_moved);
    register_accessible_parts(engine, meta(), parts);
    register_key_item(
        engine,
        meta(),
        KeyItem {
            shortcut_override: no_key,
            press: key_press,
            release: no_key,
        },
    );
}
