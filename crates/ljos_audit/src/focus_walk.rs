//! The Tab rounds as a user takes them: the window's from its first stop,
//! and each open popup's from its own first stop, as `rules/tab.rs`
//! walks them. Only where Tab would go is asked; nothing is pressed.

use ljos_engine::ObjectId;
use ljos_quick::keys::tab_target;
use ljos_quick::{Item, child_items, is_item, tab_scope};

use crate::cx::{Cx, DEPTH, shown_items, takes_focus};

/// The most stops a round is followed for.
const MAX_STOPS: usize = 256;

/// The items Tab stops at under `item`, in the order of the document.
pub(crate) fn stops(cx: &Cx, item: ObjectId, depth: usize, out: &mut Vec<ObjectId>) {
    let e = cx.engine;
    if depth > DEPTH || out.len() >= MAX_STOPS || !is_item(e, item) || !takes_focus(e, item) {
        return;
    }
    if e.untrack(|| e.read(item, Item::ACTIVE_FOCUS_ON_TAB.index))
        .to_boolean()
    {
        out.push(item);
    }
    for c in child_items(e, item) {
        stops(cx, c, depth + 1, out);
    }
}

/// The open popups' items that keep Tab to themselves.
pub(crate) fn scopes(cx: &Cx) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for item in shown_items(cx) {
        if let Some(scope) = tab_scope(cx.engine, item)
            && !out.contains(&scope)
        {
            out.push(scope);
        }
    }
    out
}

/// Once round from `first`, in the order Tab takes, until it is back or
/// goes round in a smaller circle (that is `a11y-tab-trap`'s).
fn round(cx: &Cx, root: ObjectId, first: ObjectId) -> Vec<ObjectId> {
    let mut seen = vec![first];
    let mut cur = first;
    while seen.len() < MAX_STOPS {
        match tab_target(cx.engine, root, Some(cur), true) {
            Some(next) if !seen.contains(&next) => {
                seen.push(next);
                cur = next;
            }
            _ => break,
        }
    }
    seen
}

/// Each round: the window's first, then each open popup's, with the item
/// that keeps it (`None` for the window).
pub(crate) fn rounds(cx: &Cx) -> Vec<(Option<ObjectId>, Vec<ObjectId>)> {
    let Some(root) = cx.scene.root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let popups = scopes(cx);
    if let Some(first) = tab_target(cx.engine, root, None, true) {
        // With a popup open Tab may start in it; the window's round is
        // then the one from the window's own first stop.
        let window_first = if popups
            .iter()
            .any(|s| crate::cx::within(cx.engine, first, *s))
        {
            let mut all = Vec::new();
            stops(cx, root, 0, &mut all);
            all.into_iter()
                .find(|s| !popups.iter().any(|p| crate::cx::within(cx.engine, *s, *p)))
        } else {
            Some(first)
        };
        if let Some(first) = window_first {
            out.push((None, round(cx, root, first)));
        }
    }
    for scope in popups {
        let mut inside = Vec::new();
        stops(cx, scope, 0, &mut inside);
        if let Some(first) = inside.first().copied() {
            out.push((Some(scope), round(cx, root, first)));
        }
    }
    out
}
