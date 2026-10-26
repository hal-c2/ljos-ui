//! `a11y-unnamed-image`: a picture has a name, or says that it is only
//! decoration, or is part of something that is named for it.

use ljos_quick::Image;

use crate::cx::{Cx, has_name, ignored, is_a, item_words, node_words, shown_items};

pub(crate) fn check(cx: &mut Cx) {
    let tree = cx.scene.accessibility_tree();
    let mut found = Vec::new();
    for item in shown_items(cx) {
        if !is_a::<Image>(cx.engine, item) || ignored(cx.engine, item) {
            continue;
        }
        if let Some(own) = cx.scene.accessible_node(item) {
            if !tree.get(own).is_some_and(has_name) {
                found.push(item);
            }
            continue;
        }
        // The icon of a button is the button's; so is a picture right
        // inside something that has a name.
        let parent = cx.engine.visual_parent(item);
        let mut cur = parent;
        let mut named_for = false;
        while let Some(above) = cur {
            if ignored(cx.engine, above) {
                named_for = true;
                break;
            }
            if let Some(data) = cx.scene.accessible_node(above).and_then(|k| tree.get(k)) {
                named_for = has_name(data) && (data.actionable() || Some(above) == parent);
                break;
            }
            cur = cx.engine.visual_parent(above);
        }
        if !named_for {
            found.push(item);
        }
    }
    for item in found {
        let words = item_words(cx.engine, item);
        cx.report(
            "a11y-unnamed-image",
            Some(item),
            format!(
                "{words} is a picture with neither `Accessible.name` nor `Accessible.ignored`; \
                 describe it, or set `Accessible.ignored: true` if it is only decoration"
            ),
        );
    }
    // What says it is a picture without being an `Image`.
    for (key, _, data) in cx.nodes().iter() {
        let Some(item) = cx.item(*key) else { continue };
        if data.qt_role.image() && !has_name(data) && !is_a::<Image>(cx.engine, item) {
            cx.report(
                "a11y-unnamed-image",
                Some(item),
                format!(
                    "{} is a picture and has no name; set `Accessible.name`, or \
                     `Accessible.ignored: true` if it is only decoration",
                    node_words(data)
                ),
            );
        }
    }
}
