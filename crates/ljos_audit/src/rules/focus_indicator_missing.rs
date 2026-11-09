//! `focus-indicator-missing`: a stop of the Tab order looks the same with
//! keyboard focus and without it (WCAG 2.2 SC 2.4.7).
//!
//! Each stop Tab reaches is drawn without focus (focus on the window, or
//! on the popup it is in, which lets go of the stop when it handed its
//! focus on to it), then given focus as Tab gives it, and drawn
//! again over its box and `limits::audit::FOCUS_MARGIN_PX` around it, for
//! a ring drawn outside. A stop whose pixels barely change
//! (`VISIBLE_CHANGE_MIN_PX` changed by `VISIBLE_DELTA_E`) shows nothing.
//! It is looked at at once, and again when what focus started has
//! run, so a caret that shows at once and a ring that fades in both count.
//! A stop is reported only when its own drawing is what the window shows:
//! one scrolled out of a clipping Flickable, drawn with an opacity of 0 or
//! covered by an open popup or its dim changes no pixel with focus,
//! whatever it draws.
//!
//! Only a stop whose look the application draws is held to it: an item
//! it declares, or a control it gives a part of its own (`background`,
//! `contentItem`, `indicator`). What a style draws, focus included, is the
//! style's, as `color-nontext-contrast` has it; a style's menu item shows
//! the keyboard by `highlighted`, which the menu sets and Tab does not.
//!
//! Focus is put back as it was, for the reason it had. Moving it runs the
//! application's `onActiveFocusChanged`, as Tab would.

use ljos_design::limits::audit::{FOCUS_MARGIN_PX, VISIBLE_CHANGE_MIN_PX};
use ljos_engine::{Engine, ObjectId, Value, WriteKind};
use ljos_quick::Item;
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_render::kurbo::Rect;

use crate::acting::{Act, hold_focus, restore_focus};
use crate::cx::{clipped_away, declared_by_app, is_a, scene_rect};
use crate::focus_walk::rounds;
use crate::pixels::{DrawOrder, Painter, Picture, backdrop, changed, shows};

/// No audit focuses more than this many stops.
const MOST: usize = 64;

/// The parts of a control that make its look.
const PARTS: [&str; 3] = ["background", "contentItem", "indicator"];

/// Whether the application, and not a style, draws how `stop` looks.
fn drawn_by_app(e: &ljos_engine::Engine, stop: ObjectId) -> bool {
    let own = |o| declared_by_app(e, o);
    if !own(stop) {
        return false;
    }
    if !is_a::<ljos_controls::Control>(e, stop) {
        return true;
    }
    PARTS
        .iter()
        .any(|p| crate::surfaces::object(e, stop, p).is_some_and(own))
}

fn draw(act: &Act, painter: &mut Painter, region: Rect) -> Option<Picture> {
    let snapshot = act.scene.snapshot()?.clone();
    painter.draw(&snapshot, region, |_| true)
}

fn has_active_focus(e: &Engine, item: ObjectId) -> bool {
    e.untrack(|| e.read(item, Item::ACTIVE_FOCUS.index))
        .to_boolean()
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let e = act.engine;
    let Some(root) = act.scene.root() else { return };
    let (stops, order): (Vec<(ObjectId, ObjectId)>, DrawOrder) = {
        let cx = act.cx();
        let order = DrawOrder::new(&cx);
        let stops = rounds(&cx)
            .into_iter()
            .flat_map(|(scope, round)| round.into_iter().map(move |s| (scope.unwrap_or(root), s)))
            .filter(|(_, s)| order.contains(*s) && !clipped_away(&cx, *s))
            .filter(|(_, s)| drawn_by_app(e, *s))
            .take(MOST)
            .collect();
        (stops, order)
    };
    if stops.is_empty() {
        return;
    }
    let held = hold_focus(act);
    let mut painter = Painter::new(backdrop(e, act.options));
    let mut found = Vec::new();
    for (away, stop) in stops {
        let Some(region) = scene_rect(e, stop)
            .filter(|r| r.width() >= 1.0 && r.height() >= 1.0)
            .map(|r| r.inflate(f64::from(FOCUS_MARGIN_PX), f64::from(FOCUS_MARGIN_PX)))
        else {
            continue;
        };
        force_active_focus(e, away, FocusReason::Other);
        // A popup or scope hands its focus on to the child that asks for
        // it (`focus: true`), which may be the stop: let go of the stop
        // so it is drawn without focus, as the toolkit lets go of it.
        // The flags are put back with the rest of focus at the end.
        if has_active_focus(e, stop) {
            let _ = e.write(stop, Item::FOCUS.index, Value::Bool(false), WriteKind::Host);
        }
        act.rest();
        act.settle();
        let Some(before) = draw(act, &mut painter, region) else {
            continue;
        };
        force_active_focus(e, stop, FocusReason::Tab);
        act.settle();
        let at_once = draw(act, &mut painter, region).map_or(0, |p| changed(&before, &p));
        let mut most = at_once;
        if most < VISIBLE_CHANGE_MIN_PX {
            act.finish();
            most = most.max(draw(act, &mut painter, region).map_or(0, |p| changed(&before, &p)));
        }
        // The stop still has focus, so the frame is the one the last
        // picture was drawn from.
        if most < VISIBLE_CHANGE_MIN_PX && shows(act, &mut painter, &order, stop) {
            found.push((stop, most));
        }
    }
    restore_focus(act, &held);
    act.rest();
    for (stop, most) in found {
        let words = crate::cx::words(&act.cx(), stop);
        let message = format!(
            "{words} looks the same with keyboard focus as without it ({most} pixels change): \
             someone using the keyboard cannot see where they are; draw `visualFocus` (a ring \
             or a change of fill)"
        );
        let f = act.report("focus-indicator-missing", Some(stop), message);
        f.measured = Some(format!("{most} px < {VISIBLE_CHANGE_MIN_PX} px"));
    }
}
