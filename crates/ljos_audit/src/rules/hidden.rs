//! `a11y-hidden-exposed`: what the user cannot see is not in the
//! accessibility tree either.
//!
//! An item is unseen when it is drawn fully transparent (its opacity, or
//! an ancestor's, is 0), or when neither it nor anything inside it has an
//! area to draw in (a control squeezed to 0 by 0). Only a node an
//! assistive technology reads or acts on counts: something it can find,
//! say and press while the screen shows nothing there. What lies outside
//! the window is `layout-offscreen`'s, and what a clip hides can most
//! often be scrolled to, so neither is said here.

use std::collections::HashSet;

use ljos_engine::ObjectId;

use crate::cx::{
    Cx, acted_on, has_name, node_words, scene_rect, shown_items, shown_opacity, within,
};

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    let drawn: HashSet<ObjectId> = shown.iter().copied().collect();
    // What has an area to draw in, so that a box of none that holds one
    // (a row of no height whose content hangs out of it) is still seen.
    let with_area: Vec<ObjectId> = shown
        .iter()
        .copied()
        .filter(|i| scene_rect(e, *i).is_some_and(|r| r.width() > 0.0 && r.height() > 0.0))
        .collect();
    let tree = cx.scene.accessibility_tree();
    let mut found: Vec<(ObjectId, String, &str, bool)> = Vec::new();
    for (key, _, data) in cx.nodes().iter() {
        if !(acted_on(data) || has_name(data)) || tree.off_screen(*key) {
            continue;
        }
        let Some(item) = cx.item(*key).filter(|i| drawn.contains(i)) else {
            continue;
        };
        // One finding for what is inside an unseen item: the outer one.
        if found.iter().any(|(f, ..)| within(e, item, *f)) {
            continue;
        }
        let why = if shown_opacity(e, item) <= 0.0 {
            "it is drawn with an opacity of 0"
        } else if !with_area.iter().any(|i| within(e, *i, item)) {
            "neither it nor anything in it has an area to be drawn in"
        } else {
            continue;
        };
        found.push((item, node_words(data), why, acted_on(data)));
    }
    for (item, words, why, pressed) in found {
        let does = if pressed {
            "finds it and can press it"
        } else {
            "finds it and reads it out"
        };
        cx.report(
            "a11y-hidden-exposed",
            Some(item),
            format!(
                "{words} cannot be seen, {why}, and an assistive technology still {does}; \
                 hide it with `visible: false`, or set `Accessible.ignored` while it cannot be \
                 seen"
            ),
        );
    }
}
