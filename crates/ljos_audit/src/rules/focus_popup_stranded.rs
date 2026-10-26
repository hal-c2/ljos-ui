//! `focus-popup-stranded`: a modal popup is open and keyboard focus is
//! not in it.
//!
//! A modal popup takes the pointer from the window behind it; when focus
//! stays behind it too (on an item of the window, or on nothing), keys go
//! where the user cannot see or reach, and Tab never gets into the popup.
//! Whether Escape closes it is `focus-escape-dismiss`'s. What a popup does
//! with focus once it closes depends on where it was opened from, which an
//! audit of the scene as it is cannot know; that half is left.

use ljos_quick::focus::active_focus_item;

use crate::cx::{Cx, within, words};
use crate::surfaces::open_surfaces;

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let Some(root) = cx.scene.root() else { return };
    let focus = active_focus_item(e, root);
    let read = |o, n: &str| e.untrack(|| e.read_by_name(o, n)).to_boolean();
    let stranded: Vec<_> = open_surfaces(cx)
        .into_iter()
        .filter(|s| read(s.popup, "modal") && read(s.popup, "opened"))
        .filter(|s| !focus.is_some_and(|f| within(e, f, s.item)))
        .collect();
    // The top one is where keys should go; the ones under it wait.
    let Some(top) = stranded.last() else { return };
    let behind = match focus {
        Some(f) => format!("on {}, behind it", words(cx, f)),
        None => "on nothing".to_string(),
    };
    let message = format!(
        "{} is modal and open, and keyboard focus is {behind}: keys go where the user cannot \
         reach; give the popup `focus: true`",
        words(cx, top.popup),
    );
    cx.report("focus-popup-stranded", Some(top.popup), message);
}
