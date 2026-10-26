//! `focus-escape-dismiss`: an open popup that can be closed some other
//! way does not close on Escape.
//!
//! A popup whose `closePolicy` keeps `Popup.CloseOnEscape` closes on
//! Escape. One that does not may still handle it itself (`Keys.onEscapePressed`
//! calling `close()`), so Escape is pressed, with focus inside the popup
//! as a user would have it, and the popup looked at once what that
//! started has run. If it closed, it is opened again.
//!
//! Only a popup that has another way to close is held to it: a press
//! outside in its `closePolicy`, a dialog's standard buttons, or a button
//! in it that says close, cancel, done or the like. One with no way out
//! at all is one the application closes itself (a busy overlay, say).
//! Pressing Escape runs what the application binds to it, as a user's
//! press would.

use ljos_controls::{AbstractButton, Dialog};
use ljos_engine::ObjectId;
use ljos_quick::KeyEvent;
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::keys::key;

use crate::acting::{Act, hold_focus, restore_focus};
use crate::cx::{Cx, is_a, shown_items, within};
use crate::focus_walk::stops;
use crate::surfaces::{Surface, open_surfaces};

/// `Popup.ClosePolicy` (doc.qt.io/qt-6/qml-qtquick-controls-popup.html):
/// the press and release flags, and Escape.
const CLOSE_ON_POINTER: i32 = 0x01 | 0x02 | 0x04 | 0x08;
const CLOSE_ON_ESCAPE: i32 = 0x10;

/// What a button that closes a popup says.
const CLOSE_WORDS: &[&str] = &["close", "cancel", "done", "dismiss", "ok", "back", "×", "✕"];

fn number(cx: &Cx, obj: ObjectId, name: &str) -> i32 {
    let e = cx.engine;
    e.untrack(|| e.read_by_name(obj, name)).to_number() as i32
}

fn is_open(act: &Act, popup: ObjectId) -> bool {
    let e = act.engine;
    e.untrack(|| e.read_by_name(popup, "opened")).to_boolean()
        || e.untrack(|| e.read_by_name(popup, "visible")).to_boolean()
}

/// Whether the popup can be closed some way other than Escape.
fn closes_otherwise(cx: &Cx, s: &Surface) -> bool {
    let e = cx.engine;
    if number(cx, s.popup, "closePolicy") & CLOSE_ON_POINTER != 0 {
        return true;
    }
    if is_a::<Dialog>(e, s.popup) && number(cx, s.popup, "standardButtons") != 0 {
        return true;
    }
    shown_items(cx).into_iter().any(|b| {
        within(e, b, s.item) && is_a::<AbstractButton>(e, b) && {
            let said = |n: &str| e.untrack(|| e.read_by_name(b, n)).to_js_string();
            [said("text"), said("Accessible.name")].iter().any(|t| {
                let t = t.as_str().trim().to_lowercase();
                CLOSE_WORDS
                    .iter()
                    .any(|w| t == *w || t.split_whitespace().next() == Some(*w))
            })
        }
    })
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let e = act.engine;
    let candidates: Vec<(ObjectId, ObjectId)> = {
        let cx = act.cx();
        open_surfaces(&cx)
            .into_iter()
            .filter(|s| number(&cx, s.popup, "closePolicy") & CLOSE_ON_ESCAPE == 0)
            .filter(|s| closes_otherwise(&cx, s))
            .map(|s| {
                let mut inside = Vec::new();
                stops(&cx, s.item, 0, &mut inside);
                (s.popup, inside.first().copied().unwrap_or(s.item))
            })
            .collect()
    };
    if candidates.is_empty() {
        return;
    }
    let held = hold_focus(act);
    let mut found = Vec::new();
    for (popup, target) in candidates {
        if !e.is_alive(popup) || !is_open(act, popup) {
            continue;
        }
        force_active_focus(e, target, FocusReason::Popup);
        act.settle();
        act.scene.key_event(e, &KeyEvent::press(key::ESCAPE));
        act.scene.key_event(e, &KeyEvent::release(key::ESCAPE));
        act.finish();
        if !e.is_alive(popup) {
            continue;
        }
        if is_open(act, popup) {
            found.push(popup);
        } else {
            let _ = e.invoke_by_name(popup, "open", &[]);
            act.finish();
        }
    }
    restore_focus(act, &held);
    act.rest();
    for popup in found {
        let words = crate::cx::words(&act.cx(), popup);
        let message = format!(
            "{words} stays open on Escape, and can be closed another way: keep \
             `Popup.CloseOnEscape` in its `closePolicy`, or close it in `Keys.onEscapePressed`"
        );
        act.report("focus-escape-dismiss", Some(popup), message);
    }
}
