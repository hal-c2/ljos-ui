//! `focus-shortcut-under-modal`: a `Shortcut` of the window still acts
//! while a modal popup is open over it.
//!
//! A shortcut is matched wherever focus is, so a modal popup does not
//! stop it: Ctrl+W behind a rename dialog closes the tab the dialog is
//! about. The shortcuts that would act now are the enabled ones with a
//! sequence, of this window or the application, not declared in the
//! popup. Nothing is pressed.

use ljos_engine::{Engine, ObjectId};
use ljos_quick::keys::Shortcut;
use ljos_quick::keys::shortcut_context::{APPLICATION, WINDOW};

use crate::cx::{Cx, is_a, within};
use crate::surfaces::open_surfaces;

/// Every Shortcut the objects under `top` own.
fn shortcuts(e: &Engine, top: ObjectId, out: &mut Vec<ObjectId>) {
    let mut stack = vec![top];
    let mut seen = 0usize;
    while let Some(o) = stack.pop() {
        seen += 1;
        if seen > 1 << 16 {
            break;
        }
        if is_a::<Shortcut>(e, o) {
            out.push(o);
        }
        stack.extend(e.children(o));
    }
}

/// The item a Shortcut was declared in.
fn declared_in(e: &Engine, s: ObjectId) -> Option<ObjectId> {
    let mut cur = e.owner(s);
    for _ in 0..crate::cx::DEPTH {
        match cur {
            Some(o) if ljos_quick::is_item(e, o) => return Some(o),
            Some(o) => cur = e.owner(o),
            None => break,
        }
    }
    None
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let Some(root) = cx.scene.root() else { return };
    let read = |o, n: &str| e.untrack(|| e.read_by_name(o, n));
    let Some(modal) = open_surfaces(cx)
        .into_iter()
        .rfind(|s| read(s.popup, "modal").to_boolean() && read(s.popup, "opened").to_boolean())
    else {
        return;
    };
    let mut top = root;
    while let Some(o) = e.owner(top) {
        top = o;
    }
    let mut all = Vec::new();
    shortcuts(e, top, &mut all);
    let mut found = Vec::new();
    for s in all {
        let context = read(s, "context").to_number() as i32;
        let has_keys = !read(s, "portableText").to_js_string().as_str().is_empty();
        if !read(s, "enabled").to_boolean() || !has_keys {
            continue;
        }
        if context != WINDOW && context != APPLICATION {
            continue;
        }
        // One declared in the popup belongs to what the popup shows, and
        // one that is the popup's (`within` the popup object) too.
        let inside = declared_in(e, s).is_some_and(|i| within(e, i, modal.item))
            || std::iter::successors(e.owner(s), |o| e.owner(*o)).any(|o| o == modal.popup);
        if !inside {
            found.push((
                s,
                read(s, "portableText").to_js_string().as_str().to_string(),
            ));
        }
    }
    let popup = crate::cx::words(cx, modal.popup);
    for (s, keys) in found {
        let message = format!(
            "the shortcut {keys} still acts on the window while {popup} is modal and open over \
             it; set `enabled: !popup.opened`, or move it into the popup"
        );
        cx.report("focus-shortcut-under-modal", Some(s), message);
    }
}
