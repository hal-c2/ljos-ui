//! The pointer, the wheel, keys and the input method, handed to a view's
//! page.
//!
//! The scene delivers them to the view in its own coordinates, as to any
//! item; they go on to the browser in logical pixels, and it scales them
//! as it scales the picture. A view with no page yet takes none.

use std::ops::Range;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::keys::{self, KeyItem, Surrounding, TextInputClient, key};
use ljos_quick::{
    HoverMove, KeyEvent, KeyKind, Modifiers, MouseButtons, PointerEvent, PointerItem, WheelEvent,
};
use ljos_render::kurbo;

use crate::backend::{Key, Mouse, MouseKind, modifier};
use crate::view::{WebEngineView, browser_of};

pub(crate) fn register(engine: &Engine) {
    ljos_quick::register_pointer_item(
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
    let hover_move: HoverMove = hover_moved;
    ljos_quick::register_hover_move(engine, meta(), hover_move);
    ljos_quick::flickable::register_wheel_item(
        engine,
        meta(),
        ljos_quick::flickable::WheelItem { wheel },
    );
    keys::register_key_item(
        engine,
        meta(),
        KeyItem {
            shortcut_override,
            press: key_press,
            release: key_release,
        },
    );
    keys::register_text_input_client(engine, meta(), Rc::new(PageInput));
}

fn meta() -> ljos_engine::MetaRef {
    ljos_engine::MetaRef::Static(<WebEngineView as ljos_engine::QmlType>::static_meta())
}

fn modifiers_of(m: Modifiers) -> u32 {
    let mut out = 0;
    if m.contains(Modifiers::SHIFT) {
        out |= modifier::SHIFT;
    }
    if m.contains(Modifiers::CONTROL) {
        out |= modifier::CONTROL;
    }
    if m.contains(Modifiers::ALT) {
        out |= modifier::ALT;
    }
    if m.contains(Modifiers::META) {
        out |= modifier::COMMAND;
    }
    out
}

fn buttons_of(b: MouseButtons) -> u32 {
    let mut out = 0;
    if b.contains(MouseButtons::LEFT) {
        out |= modifier::LEFT_BUTTON;
    }
    if b.contains(MouseButtons::MIDDLE) {
        out |= modifier::MIDDLE_BUTTON;
    }
    if b.contains(MouseButtons::RIGHT) {
        out |= modifier::RIGHT_BUTTON;
    }
    out
}

fn button_of(b: Option<MouseButtons>) -> u8 {
    match b {
        Some(b) if b.contains(MouseButtons::MIDDLE) => 1,
        Some(b) if b.contains(MouseButtons::RIGHT) => 2,
        _ => 0,
    }
}

fn mouse(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point, kind: MouseKind) -> bool {
    let Some(b) = browser_of(e, me) else {
        return false;
    };
    b.mouse(Mouse {
        x: at.x,
        y: at.y,
        button: button_of(ev.button),
        kind,
        clicks: 1,
        modifiers: modifiers_of(ev.modifiers) | buttons_of(ev.buttons),
    });
    true
}

fn press(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    let took = mouse(e, me, ev, at, MouseKind::Press);
    if took
        && e.get(me, WebEngineView::ACTIVE_FOCUS_ON_PRESS)
            .unwrap_or(true)
    {
        force_active_focus(e, me, FocusReason::Mouse);
    }
    took
}

fn moved(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    mouse(e, me, ev, at, MouseKind::Move);
}

fn release(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    mouse(e, me, ev, at, MouseKind::Release);
}

fn cancel(e: &Engine, me: ObjectId) {
    if let Some(b) = browser_of(e, me) {
        b.mouse(Mouse {
            x: 0.0,
            y: 0.0,
            button: 0,
            kind: MouseKind::Leave,
            clicks: 0,
            modifiers: 0,
        });
    }
}

fn hover(e: &Engine, me: ObjectId, inside: bool) {
    if !inside {
        cancel(e, me);
    }
}

fn hover_moved(e: &Engine, me: ObjectId, at: kurbo::Point) {
    if let Some(b) = browser_of(e, me) {
        b.mouse(Mouse {
            x: at.x,
            y: at.y,
            button: 0,
            kind: MouseKind::Move,
            clicks: 0,
            modifiers: 0,
        });
    }
}

fn wheel(e: &Engine, me: ObjectId, ev: &WheelEvent, at: kurbo::Point) -> bool {
    let Some(b) = browser_of(e, me) else {
        return false;
    };
    // A notch scrolls what Chromium scrolls for one: 120 units, 3 lines.
    let (dx, dy) = if ev.pixel_delta.x != 0.0 || ev.pixel_delta.y != 0.0 {
        (ev.pixel_delta.x, ev.pixel_delta.y)
    } else {
        (ev.angle_delta.x, ev.angle_delta.y)
    };
    b.wheel(at.x, at.y, dx, dy, modifiers_of(ev.modifiers));
    true
}

/// A page wants every key it can use before the shortcuts see it: what a
/// text field takes, and what moves in a page.
fn shortcut_override(e: &Engine, me: ObjectId, ev: &KeyEvent) -> bool {
    browser_of(e, me).is_some()
        && !ev
            .modifiers
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
}

fn key_press(e: &Engine, me: ObjectId, ev: &KeyEvent) -> bool {
    send_key(e, me, ev, true)
}

fn key_release(e: &Engine, me: ObjectId, ev: &KeyEvent) -> bool {
    send_key(e, me, ev, false)
}

fn send_key(e: &Engine, me: ObjectId, ev: &KeyEvent, press: bool) -> bool {
    let Some(b) = browser_of(e, me) else {
        return false;
    };
    debug_assert_eq!(press, ev.kind == KeyKind::Press);
    b.key(&Key {
        press,
        windows_code: windows_key_code(ev.key),
        native_code: ev.native_scan_code as i32,
        modifiers: modifiers_of(ev.modifiers),
        text: ev.text.clone(),
    });
    true
}

/// The Windows virtual key a `Qt.Key` is, which Chromium reads on every
/// platform; 0 for one it has none for.
pub(crate) fn windows_key_code(k: i32) -> i32 {
    match k {
        key::A..=key::Z | key::DIGIT_0..=key::DIGIT_9 => k,
        0x61..=0x7a => k - 0x20,
        key::SPACE => 0x20,
        key::BACKSPACE => 0x08,
        key::TAB | key::BACKTAB => 0x09,
        key::RETURN | key::ENTER => 0x0d,
        key::SHIFT => 0x10,
        key::CONTROL => 0x11,
        key::ALT => 0x12,
        key::PAUSE => 0x13,
        key::CAPS_LOCK => 0x14,
        key::ESCAPE => 0x1b,
        key::PAGE_UP => 0x21,
        key::PAGE_DOWN => 0x22,
        key::END => 0x23,
        key::HOME => 0x24,
        key::LEFT => 0x25,
        key::UP => 0x26,
        key::RIGHT => 0x27,
        key::DOWN => 0x28,
        key::INSERT => 0x2d,
        key::DELETE => 0x2e,
        key::META | key::SUPER_L => 0x5b,
        key::SUPER_R => 0x5c,
        key::MENU => 0x5d,
        key::F1..=0x0100_0047 => 0x70 + (k - key::F1),
        key::NUM_LOCK => 0x90,
        key::SCROLL_LOCK => 0x91,
        key::ASTERISK => 0x6a,
        key::PLUS => 0xbb,
        key::COMMA => 0xbc,
        key::MINUS => 0xbd,
        0x2e => 0xbe,
        0x2f => 0xbf,
        0x3b => 0xba,
        0x3d => 0xbb,
        0x5b => 0xdb,
        0x5c => 0xdc,
        0x5d => 0xdd,
        0x27 => 0xde,
        0x60 => 0xc0,
        _ => 0,
    }
}

/// What the platform's input method composes in a page goes to the
/// page's focused field; the view itself holds no text.
struct PageInput;

impl TextInputClient for PageInput {
    fn surrounding(&self, _engine: &Engine, _item: ObjectId) -> Surrounding {
        Surrounding {
            text: String::new(),
            anchor: 0,
            cursor: 0,
        }
    }

    fn replace(&self, engine: &Engine, item: ObjectId, _range: Range<usize>, text: &str) {
        if let Some(b) = browser_of(engine, item) {
            b.commit(text);
        }
    }

    fn set_selection(&self, _engine: &Engine, _item: ObjectId, _anchor: usize, _cursor: usize) {}

    fn cursor_rect(&self, _engine: &Engine, _item: ObjectId) -> kurbo::Rect {
        kurbo::Rect::ZERO
    }

    fn hints(&self, _engine: &Engine, _item: ObjectId) -> i32 {
        0
    }

    fn set_preedit(
        &self,
        engine: &Engine,
        item: ObjectId,
        text: &str,
        cursor: Option<usize>,
    ) -> bool {
        let Some(b) = browser_of(engine, item) else {
            return false;
        };
        b.compose(text, cursor.unwrap_or(text.chars().count()));
        true
    }

    fn commit(&self, engine: &Engine, item: ObjectId, text: &str) -> bool {
        let Some(b) = browser_of(engine, item) else {
            return false;
        };
        b.commit(text);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_windows_virtual_keys() {
        assert_eq!(windows_key_code(key::A), 0x41);
        assert_eq!(windows_key_code(0x61), 0x41);
        assert_eq!(windows_key_code(key::RETURN), 0x0d);
        assert_eq!(windows_key_code(key::LEFT), 0x25);
        assert_eq!(windows_key_code(key::F1), 0x70);
        assert_eq!(windows_key_code(key::F10), 0x79);
        assert_eq!(windows_key_code(key::BACK), 0);
    }
}
