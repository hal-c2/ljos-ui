//! The Tab walk: from the first stop, Tab by Tab, until it is back.
//!
//! The walk asks where Tab would go ([`ljos_quick::keys::tab_target`]:
//! `KeyNavigation.tab`, then the Tab order) and presses nothing, so an
//! audit leaves focus where it was and runs no handler. A `Keys` handler
//! that keeps Tab for itself is therefore not seen.

use ljos_a11y::Role;
use ljos_engine::ObjectId;
use ljos_quick::keys::tab_target;
use ljos_quick::{Item, child_items, is_item};

use crate::cx::{
    Cx, DEPTH, at, clipped_away, item_words, node_words, shown_items, takes_focus, words,
};
use crate::focus_walk::scopes;

/// The items Tab stops at, in the order of the document.
fn stops(cx: &Cx, item: ObjectId, depth: usize, out: &mut Vec<ObjectId>) {
    let e = cx.engine;
    if depth > DEPTH || !is_item(e, item) || !takes_focus(e, item) {
        return;
    }
    if e.read(item, Item::ACTIVE_FOCUS_ON_TAB.index).to_boolean() {
        out.push(item);
    }
    for c in child_items(e, item) {
        stops(cx, c, depth + 1, out);
    }
}

/// The nearest item above `stop` that the tree shows and that takes the
/// keyboard: who speaks for a stop that has no node of its own.
fn speaker(cx: &Cx, stop: ObjectId) -> Option<ObjectId> {
    let tree = cx.scene.accessibility_tree();
    let mut cur = cx.engine.visual_parent(stop);
    for _ in 0..DEPTH {
        let above = cur?;
        if let Some(data) = cx.scene.accessible_node(above).and_then(|k| tree.get(k)) {
            return data.focusable.then_some(above);
        }
        cur = cx.engine.visual_parent(above);
    }
    None
}

/// Once round from `first`. Adds what Tab stops at to `seen` and says
/// whether the round never came back.
fn round(
    cx: &mut Cx,
    root: ObjectId,
    first: ObjectId,
    stops: usize,
    seen: &mut Vec<ObjectId>,
) -> bool {
    let e = cx.engine;
    let from = seen.len();
    seen.push(first);
    let mut cur = first;
    // One more than there are stops is once round and back.
    for _ in 0..=stops {
        match tab_target(e, root, Some(cur), true) {
            None => break,
            Some(next) if next == first => break,
            Some(next) if seen[from..].contains(&next) => {
                let message = if next == cur {
                    format!(
                        "Tab does not leave {}; it must go on to the next stop",
                        words(cx, cur)
                    )
                } else {
                    format!(
                        "Tab from {} goes back to {}{} and never returns to the first \
                         stop, {}{}; the stops after it cannot be left by keyboard",
                        words(cx, cur),
                        words(cx, next),
                        at(cx.engine, next),
                        words(cx, first),
                        at(cx.engine, first),
                    )
                };
                cx.report("a11y-tab-trap", Some(cur), message);
                return true;
            }
            Some(next) => {
                seen.push(next);
                cur = next;
            }
        }
    }
    false
}

/// `a11y-tab-trap`, `a11y-tab-unreachable`, and `a11y-unnamed-interactive`
/// for a stop that says nothing of itself.
pub(crate) fn check(cx: &mut Cx) {
    let Some(root) = cx.scene.root() else { return };
    let e = cx.engine;
    let mut expected = Vec::new();
    stops(cx, root, 0, &mut expected);

    let mut seen: Vec<ObjectId> = Vec::new();
    let mut trapped = false;
    if let Some(first) = tab_target(e, root, None, true) {
        trapped = round(cx, root, first, expected.len(), &mut seen);
    }
    // A popup that is open keeps Tab to itself: its stops are a round of
    // their own, from the first of them.
    for scope in scopes(cx) {
        let mut inside = Vec::new();
        stops(cx, scope, 0, &mut inside);
        if let Some(first) = inside.first().copied() {
            trapped |= round(cx, root, first, inside.len(), &mut seen);
        }
        expected.extend(inside);
    }

    // After a trap everything behind it is unreachable; the trap says so
    // once.
    if !trapped {
        for stop in expected.iter().filter(|s| !seen.contains(s)) {
            let message = format!(
                "Tab stops at {} by the Tab order and never gets there; something before it \
                 sends Tab elsewhere (`KeyNavigation.tab`)",
                words(cx, *stop)
            );
            cx.report("a11y-tab-unreachable", Some(*stop), message);
        }
    }

    // A stop nobody can hear of. A stop that is a part of something that
    // takes the keyboard (the editor of a spin box) is heard of as that:
    // the tree names the nearest item above the focus item that it shows.
    let tree = cx.scene.accessibility_tree();
    let mut silent = Vec::new();
    // What Tab reaches, as an assistive technology hears it.
    let mut reached = seen.clone();
    // A stop that is not on screen (a page of a swipe view that is not
    // the current one) is neither laid out nor described until it is
    // shown, so whether it would say anything is not known.
    let drawn: std::collections::HashSet<ObjectId> = shown_items(cx).into_iter().collect();
    for stop in &seen {
        if !drawn.contains(stop) || clipped_away(cx, *stop) {
            continue;
        }
        if cx.scene.accessible_node(*stop).is_some() {
            // Tab that lands inside something that takes the keyboard is
            // in it too: the number of a spin box is its box's.
            reached.extend(speaker(cx, *stop));
            continue;
        }
        match speaker(cx, *stop) {
            Some(above) => reached.push(above),
            None => silent.push(*stop),
        }
    }
    for stop in silent {
        // An ignored stop has a rule of its own.
        if cx.options.wants("a11y-ignored-focusable") && crate::cx::ignored(e, stop) {
            continue;
        }
        let message = format!(
            "Tab stops at {}, which tells an assistive technology nothing; set \
             `Accessible.role` and `Accessible.name`",
            item_words(e, stop)
        );
        cx.report("a11y-unnamed-interactive", Some(stop), message);
    }

    // What says it takes the keyboard, and Tab does not get there. One of
    // a set (a row, a menu item, a tab) is reached with the arrows from
    // the one that has focus, and what is inside a stop through the stop.
    let mut unreached = Vec::new();
    tree.walk(|key, _, data| {
        if key.is_announcement() || !data.focusable || data.disabled || data.qt_role.in_set() {
            return;
        }
        // Focusable by their role alone, as Qt has it, and no stop of
        // anybody's Tab order: a text is read, a scroll bar is dragged.
        if matches!(data.qt_role, Role::StaticText | Role::ScrollBar) {
            return;
        }
        let Some(item) = cx.scene.accessible_item(key) else {
            return;
        };
        let mut cur = Some(item);
        let mut depth = 0;
        while let Some(i) = cur {
            // A stop of the Tab order that is not on the round has been
            // said above.
            if reached.contains(&i) || expected.contains(&i) {
                return;
            }
            // Inside something hidden or switched off nothing is reached.
            if !takes_focus(e, i) || depth > DEPTH {
                return;
            }
            depth += 1;
            cur = e.visual_parent(i);
        }
        unreached.push((item, node_words(data)));
    });
    for (item, words) in unreached {
        cx.report(
            "a11y-tab-unreachable",
            Some(item),
            format!(
                "{words} takes the keyboard (`Accessible.focusable`) and Tab never reaches it; \
                 set `activeFocusOnTab: true`, or `Accessible.focusable: false` if it is not \
                 for the keyboard"
            ),
        );
    }
}
