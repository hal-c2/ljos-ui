//! `a11y-unnamed-interactive`: what a user acts on says what it is.
//!
//! A node that can be acted on needs a name. A `MouseArea` or a
//! `TapHandler` the application's QML gives a click handler is acted on
//! too, and when neither it nor an item it covers is a node, an assistive technology
//! does not know it is there. One over the whole window (a backdrop
//! that closes something, not the root itself) is left out: what it does is done
//! another way too, as Escape closes a popup.

use ljos_a11y::{NodeKey, Role};
use ljos_engine::{Engine, MetaObject, ObjectId, QmlType};
use ljos_quick::{MouseArea, TapHandler, tap_handler_target};

use crate::Bounds;
use crate::cx::{
    Cx, acted_on, ancestors, app_place, has_name, ignored, is_a, node_words, place_of, scene_rect,
    shown_items, slack,
};

/// The signals a click handler connects to, on a `MouseArea` and on a
/// `TapHandler`.
const CLICKS: [&str; 4] = ["clicked", "pressed", "released", "doubleClicked"];
const TAPS: [&str; 4] = ["tapped", "singleTapped", "doubleTapped", "longPressed"];

/// `Qt.LeftButton`.
const LEFT_BUTTON: i64 = 1;

/// Whether `obj` takes a plain click and does something with it. One that
/// takes only the other buttons is a second way in (a context menu), and
/// the plain way is what must be named.
fn handles(engine: &Engine, obj: ObjectId, meta: &MetaObject, signals: &[&str]) -> bool {
    let read = |name| engine.untrack(|| engine.read_by_name(obj, name));
    read("enabled").to_boolean()
        && read("acceptedButtons").to_number() as i64 & LEFT_BUTTON != 0
        && signals
            .iter()
            .filter_map(|s| meta.signal_index(s))
            .any(|s| engine.signal_connected(obj, s))
}

/// What reacts to a click on a shown item: a `MouseArea` that is the item,
/// and the `TapHandler`s it holds, each with the item it takes clicks on.
fn clickers(engine: &Engine, item: ObjectId) -> Vec<(ObjectId, &'static str)> {
    let mut out = Vec::new();
    if is_a::<MouseArea>(engine, item) && handles(engine, item, MouseArea::static_meta(), &CLICKS) {
        out.push((item, "MouseArea"));
    }
    out.extend(
        engine
            .children(item)
            .into_iter()
            .filter(|h| {
                is_a::<TapHandler>(engine, *h)
                    && tap_handler_target(engine, *h) == Some(item)
                    && handles(engine, *h, TapHandler::static_meta(), &TAPS)
            })
            .map(|h| (h, "TapHandler")),
    );
    out
}

/// Mouse areas and tap handlers with a click handler that nothing exposes.
fn unexposed_areas(cx: &mut Cx) {
    let engine = cx.engine;
    let window = cx.scene.viewport();
    for item in shown_items(cx) {
        for (clicker, kind) in clickers(engine, item) {
            // The application's own: a style's are its control's.
            if place_of(engine, clicker) != app_place(engine, clicker) || ignored(engine, item) {
                continue;
            }
            let Some(rect) = scene_rect(engine, item) else {
                continue;
            };
            // A backdrop over the window; the scene's root is no backdrop
            // but the whole interface.
            if rect.width() >= window.width() - slack()
                && rect.height() >= window.height() - slack()
                && cx.scene.root() != Some(item)
            {
                continue;
            }
            // The item and the items it covers, the delegate it fills: one
            // of them a node is what a reader announces for it.
            let covered = std::iter::once(item).chain(ancestors(engine, item).take_while(|a| {
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
            let bounds = cx.report(
                "a11y-unnamed-interactive",
                Some(clicker),
                format!(
                    "the {kind} reacts to a click and is no accessible node, so an assistive \
                     technology cannot find or press it; set `Accessible.role` and \
                     `Accessible.name` on the item it covers, or use a control"
                ),
            );
            // A handler is no item: the box is the item it takes clicks on.
            if bounds.bounds.is_none() {
                bounds.bounds = Some(Bounds {
                    x: rect.x0,
                    y: rect.y0,
                    width: rect.width(),
                    height: rect.height(),
                });
            }
        }
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
        // An icon button with no name has a rule of its own.
        if acted_on(data) && !has_name(data) && !super::icon_buttons::unnamed_icon_button(cx, item)
        {
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
