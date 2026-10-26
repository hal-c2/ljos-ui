//! `a11y-unnamed-interactive`: what a user acts on says what it is.
//!
//! A node that can be acted on needs a name. A `MouseArea` the
//! application's QML gives a click handler is acted on too, and when
//! neither it nor an item it covers is a node, an assistive technology
//! does not know it is there. One that covers the whole window (a
//! backdrop that closes something) is left out: what it does is done
//! another way too, as Escape closes a popup.

use ljos_a11y::{NodeKey, Role};
use ljos_engine::{Engine, ObjectId, QmlType};
use ljos_quick::MouseArea;

use crate::cx::{
    Cx, acted_on, ancestors, app_place, has_name, ignored, is_a, node_words, place_of, scene_rect,
    shown_items, slack,
};

/// The signals a click handler connects to.
const CLICKS: [&str; 4] = ["clicked", "pressed", "released", "doubleClicked"];

fn handles_clicks(engine: &Engine, area: ObjectId) -> bool {
    let meta = MouseArea::static_meta();
    engine
        .untrack(|| engine.read_by_name(area, "enabled"))
        .to_boolean()
        && CLICKS
            .iter()
            .filter_map(|s| meta.signal_index(s))
            .any(|s| engine.signal_connected(area, s))
}

/// Mouse areas with a click handler that nothing exposes.
fn unexposed_areas(cx: &mut Cx) {
    let engine = cx.engine;
    let window = cx.scene.viewport();
    for area in shown_items(cx) {
        if !is_a::<MouseArea>(engine, area) || !handles_clicks(engine, area) {
            continue;
        }
        // The application's own: a style's mouse areas are its control's.
        if place_of(engine, area) != app_place(engine, area) || ignored(engine, area) {
            continue;
        }
        let Some(rect) = scene_rect(engine, area) else {
            continue;
        };
        if rect.width() >= window.width() - slack() && rect.height() >= window.height() - slack() {
            continue;
        }
        // The area and the items it covers, the delegate it fills: one of
        // them a node is what a reader announces for it.
        let covered = std::iter::once(area).chain(ancestors(engine, area).take_while(|a| {
            scene_rect(engine, *a).is_some_and(|r| {
                r.width() <= rect.width() + slack() && r.height() <= rect.height() + slack()
            })
        }));
        let mut exposed = false;
        for i in covered {
            if cx.scene.accessible_node(i).is_some() || ignored(engine, i) {
                exposed = true;
                break;
            }
        }
        if exposed {
            continue;
        }
        cx.report(
            "a11y-unnamed-interactive",
            Some(area),
            "the MouseArea reacts to a click and is no accessible node, so an assistive \
             technology cannot find or press it; set `Accessible.role` and `Accessible.name` \
             on the item it covers, or use a control"
                .to_string(),
        );
    }
}

pub(crate) fn check(cx: &mut Cx) {
    let all = cx.nodes();
    // The number of a spin box is told inside the box, as Qt has it; the
    // box's name says what it counts.
    let boxes: Vec<NodeKey> = all
        .iter()
        .filter(|(_, _, d)| d.qt_role == Role::SpinBox)
        .map(|(k, _, _)| *k)
        .collect();
    for (key, parent, data) in all.iter() {
        // A scroll bar says what it is by its role and which way it goes;
        // nobody gives one a name.
        if data.qt_role == Role::ScrollBar || boxes.contains(parent) {
            continue;
        }
        let item = cx.item(*key);
        // A `Page` has the role of a tab, as Qt gives it, and is a pane
        // that is shown, not a tab that is pressed: its title is a name it
        // may go without.
        if item.is_some_and(|i| is_a::<ljos_controls::Page>(cx.engine, i)) {
            continue;
        }
        if acted_on(data) && !has_name(data) {
            cx.report(
                "a11y-unnamed-interactive",
                item,
                format!(
                    "{} can be acted on and has no name, so an assistive technology cannot \
                     say what it does; set `Accessible.name`, or name a label with \
                     `Accessible.labelledBy`",
                    node_words(data)
                ),
            );
        }
    }
    unexposed_areas(cx);
}
