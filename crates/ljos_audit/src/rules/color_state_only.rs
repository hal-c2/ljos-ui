//! `color-state-only`: a control shows its state (checked or not) by a
//! change of hue alone (WCAG 2.2 SC 1.4.1).
//!
//! Each checkable button the application declares is flipped, its
//! animations let run to their end, and the control is drawn to pixels
//! in both states. A state shown by a mark, a shape, a weight or a fill
//! that stands out changes some pixel by 3:1 or more (SC 1.4.1's
//! Understanding page accepts a difference of 3:1 in lightness with
//! something more); when the pixels that changed all stay under that, the
//! two states differ by hue or a faint tint. A state shown by where a part
//! is, or by a part that shows only in it (a switch's handle that moves, a
//! mark whose picture has not loaded), is a change of shape whatever its
//! colours, and not reported. Then the state is flipped
//! back, and every other button's too, should the first have been one of
//! an exclusive group.
//!
//! Flipping runs `onCheckedChanged`, as a user flipping it would; what a
//! handler changes elsewhere is not put back.

use ljos_controls::AbstractButton;
use ljos_design::color::{delta_e_ok, wcag_contrast};
use ljos_design::limits::NONTEXT_CONTRAST;
use ljos_design::limits::audit::{VISIBLE_CHANGE_MIN_PX, VISIBLE_DELTA_E};
use ljos_engine::ObjectId;
use ljos_render::kurbo::Rect;

use super::controls::{checked, hold_checked, restore_checked, set_checked};
use crate::acting::Act;
use crate::cx::{
    declared_by_app, is_a, scene_rect, shown_items, shown_opacity, takes_focus, within,
};
use crate::pixels::{Painter, Picture, backdrop};

/// No audit flips more than this many buttons: a long list of check
/// boxes says the same of each.
const MOST: usize = 64;

/// How far a part moves or grows before the move counts as a change of
/// shape: under that, a rounding of the layout.
const MOVED_PX: f64 = 1.0;

/// Where every part of `b` that shows is, in the scene.
fn places(act: &Act, b: ObjectId) -> Vec<(ObjectId, Rect)> {
    let e = act.engine;
    shown_items(&act.cx())
        .into_iter()
        .filter(|i| *i != b && within(e, *i, b) && shown_opacity(e, *i) > 0.0)
        .filter_map(|i| scene_rect(e, i).map(|r| (i, r)))
        .collect()
}

/// Whether a part showed or hid, moved or changed its size.
fn reshaped(before: &[(ObjectId, Rect)], after: &[(ObjectId, Rect)]) -> bool {
    let far = |a: Rect, b: Rect| {
        [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
            .iter()
            .any(|d| d.abs() > MOVED_PX)
    };
    before.len() != after.len()
        || before.iter().any(|(i, r)| {
            after
                .iter()
                .find(|(j, _)| j == i)
                .is_none_or(|(_, s)| far(*r, *s))
        })
}

fn draw(act: &Act, painter: &mut Painter, region: Rect) -> Option<Picture> {
    let snapshot = act.scene.snapshot()?.clone();
    painter.draw(&snapshot, region, |_| true)
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let e = act.engine;
    let (buttons, all): (Vec<ObjectId>, Vec<ObjectId>) = {
        let cx = act.cx();
        let all: Vec<ObjectId> = shown_items(&cx)
            .into_iter()
            .filter(|i| is_a::<AbstractButton>(e, *i))
            .collect();
        let buttons = all
            .iter()
            .copied()
            .filter(|b| {
                takes_focus(e, *b)
                    && e.untrack(|| e.read_by_name(*b, "checkable")).to_boolean()
                    && declared_by_app(e, *b)
            })
            .take(MOST)
            .collect();
        (buttons, all)
    };
    let mut painter = Painter::new(backdrop(e, act.options));
    let mut found = Vec::new();
    for b in buttons {
        let Some(region) = scene_rect(e, b).map(|r| r.inflate(2.0, 2.0)) else {
            continue;
        };
        let held = hold_checked(e, &all);
        let Some(before) = draw(act, &mut painter, region) else {
            continue;
        };
        let shape = places(act, b);
        let was = checked(e, b);
        set_checked(e, b, !was);
        act.finish();
        let after = draw(act, &mut painter, region);
        let moved = reshaped(&shape, &places(act, b));
        restore_checked(e, &held);
        act.finish();
        let Some(after) = after
            .filter(|a| a.width == before.width && a.height == before.height)
            .filter(|_| !moved)
        else {
            continue;
        };
        let mut changed = 0;
        let mut most = 1.0f32;
        for (x, y) in before.pairs(&after) {
            if delta_e_ok(x, y) >= VISIBLE_DELTA_E {
                changed += 1;
                most = most.max(wcag_contrast(x, y));
            }
        }
        if changed >= VISIBLE_CHANGE_MIN_PX && most + 0.005 < NONTEXT_CONTRAST {
            found.push((b, was, most));
        }
    }
    for (b, was, most) in found {
        let words = crate::cx::words(&act.cx(), b);
        let (from, to) = if was {
            ("checked", "unchecked")
        } else {
            ("unchecked", "checked")
        };
        let message = format!(
            "{words} looks the same {from} and {to} but for its colour: no pixel changes by \
             more than {most:.2}:1, where a state shown by colour needs {}:1 and a change of \
             shape, mark or weight as well",
            NONTEXT_CONTRAST
        );
        let f = act.report("color-state-only", Some(b), message);
        f.measured = Some(format!("{most:.2}:1 < {NONTEXT_CONTRAST}:1"));
    }
}
