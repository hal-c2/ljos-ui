//! `focus-arrow-keys`: a list, grid, tab bar or radio group takes one Tab
//! stop per item.
//!
//! The WAI-ARIA Authoring Practices make a composite widget one stop of
//! the Tab order, with the arrow keys moving inside it: Tab past a list of
//! forty rows is one key, not forty. The stops of each Tab round are
//! counted by the view, tab bar or radio group they are in (the nearest
//! `ListView`, `GridView`, `PathView` or `TabBar` above them, else the
//! radio buttons that share a parent); more than
//! `limits::keyboard::COMPOSITE_TAB_STOPS_MAX` is reported on the group. A hint: Qt Quick Controls give each radio and tab button a stop
//! of its own, and some lists are meant to be tabbed.

use ljos_controls::{Menu, RadioButton, TabBar};
use ljos_design::limits::keyboard::COMPOSITE_TAB_STOPS_MAX;
use ljos_engine::ObjectId;
use ljos_quick::{GridView, ListView, PathView};

use crate::cx::{Cx, ancestors, declared_by_app, is_a, item_words, words};
use crate::focus_walk::rounds;

/// What `stop` is one item of, and what a message calls it.
fn composite(cx: &Cx, stop: ObjectId) -> Option<(ObjectId, &'static str)> {
    let e = cx.engine;
    // A tab bar's style may lay its buttons out in a list of its own.
    if let Some(bar) = ancestors(e, stop).find(|a| is_a::<TabBar>(e, *a)) {
        return Some((bar, "tab bar"));
    }
    for a in ancestors(e, stop) {
        if is_a::<ListView>(e, a) {
            return Some((a, "list"));
        }
        if is_a::<GridView>(e, a) {
            return Some((a, "grid"));
        }
        if is_a::<PathView>(e, a) {
            return Some((a, "path view"));
        }
    }
    if is_a::<RadioButton>(e, stop) {
        return e.visual_parent(stop).map(|p| (p, "radio group"));
    }
    None
}

/// Whether the application made the group, and it is not a menu's: a
/// menu moves with the arrows whatever Tab does, and a style's list is
/// the style's.
fn own(cx: &Cx, owner: ObjectId) -> bool {
    let e = cx.engine;
    let in_menu = std::iter::once(owner)
        .chain(ancestors(e, owner))
        .filter_map(|a| ljos_controls::item_popup(e, a))
        .any(|p| is_a::<Menu>(e, p));
    !in_menu && declared_by_app(e, owner)
}

pub(crate) fn check(cx: &mut Cx) {
    let mut found = Vec::new();
    for (_, round) in rounds(cx) {
        let mut groups: Vec<(ObjectId, &'static str, Vec<ObjectId>)> = Vec::new();
        for stop in round {
            let Some((owner, what)) = composite(cx, stop) else {
                continue;
            };
            if !own(cx, owner) {
                continue;
            }
            match groups.iter_mut().find(|(o, _, _)| *o == owner) {
                Some((_, _, v)) => v.push(stop),
                None => groups.push((owner, what, vec![stop])),
            }
        }
        found.extend(
            groups
                .into_iter()
                .filter(|(_, _, v)| v.len() > COMPOSITE_TAB_STOPS_MAX),
        );
    }
    let e = cx.engine;
    for (owner, what, stops) in found {
        let message = format!(
            "Tab stops {} times inside the {what} {}, starting at {}: make it one stop and move \
             inside it with the arrow keys (`activeFocusOnTab: false` on the items, \
             `KeyNavigation` or `keyNavigationEnabled` on the {what})",
            stops.len(),
            item_words(e, owner),
            words(cx, stops[0]),
        );
        let f = cx.report("focus-arrow-keys", Some(owner), message);
        f.measured = Some(format!("{} > {COMPOSITE_TAB_STOPS_MAX}", stops.len()));
    }
}
