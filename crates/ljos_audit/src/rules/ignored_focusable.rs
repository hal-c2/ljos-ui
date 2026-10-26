//! `a11y-ignored-focusable`: an item taken out of the accessibility tree
//! with `Accessible.ignored` that Tab still stops on. The keyboard lands
//! on it and an assistive technology has nothing to say: the user is
//! somewhere with no name.
//!
//! What an ignored item holds is still in the tree, as Qt has it, so only
//! the item itself counts.

use crate::cx::{Cx, ignored, item_words, shown_items, takes_focus};

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    for item in shown_items(cx) {
        if !ignored(e, item) || !takes_focus(e, item) {
            continue;
        }
        let on_tab = e
            .untrack(|| e.read_by_name(item, "activeFocusOnTab"))
            .to_boolean();
        if !on_tab {
            continue;
        }
        let message = format!(
            "{} is left out of the accessibility tree (`Accessible.ignored`) and Tab still \
             stops on it, so the keyboard lands where an assistive technology says nothing; \
             drop `Accessible.ignored`, or `activeFocusOnTab: false`",
            item_words(e, item)
        );
        cx.report("a11y-ignored-focusable", Some(item), message);
    }
}
