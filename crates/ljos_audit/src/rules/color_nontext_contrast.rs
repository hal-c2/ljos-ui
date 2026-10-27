//! `color-nontext-contrast`: the part of a control that tells where it is
//! or what state it is in stands out 3:1 from what is around it (WCAG 2.2
//! SC 1.4.11).
//!
//! The parts are a text field's frame, the box of a check box or a radio
//! button, a switch's track, a slider's groove and handle, and what an
//! icon button shows. The part is drawn to pixels with the rest of the
//! scene and again without it: where it changed a pixel, the two colours
//! are the part and what is behind it, and the most any pixel stands out
//! is the part's contrast. So a frame passes on its border or on its fill,
//! whichever stands out, as SC 1.4.11 has it. A part that paints, and
//! changes no pixel, is drawn in exactly what is behind it: 1:1.
//!
//! Only what the application draws is held to it. A part the style makes
//! is the toolkit's, and its contrast is the style's own test; SC 1.4.11
//! also leaves out what the user agent decides. A control that is
//! switched off is left out too ("inactive" in SC 1.4.11).

use ljos_controls::{AbstractButton, CheckBox, RadioButton, Slider, Switch, TextField};
use ljos_design::color::{Srgb, wcag_contrast};
use ljos_design::limits::NONTEXT_CONTRAST;
use ljos_engine::ObjectId;
use ljos_quick::Item;
use ljos_render::{DrawOp, FrameSnapshot};

use crate::cx::{Cx, declared_by_app, is_a, scene_rect, shown_items, takes_focus, within, words};
use crate::pixels::{Painter, backdrop};

/// The parts of `control` the rule holds, each with what a message calls
/// it.
fn parts(cx: &Cx, control: ObjectId) -> Vec<(ObjectId, &'static str)> {
    let e = cx.engine;
    let part = |name: &str| {
        e.untrack(|| e.read_by_name(control, name))
            .as_object()
            .filter(|o| e.is_alive(*o) && ljos_quick::is_item(e, *o))
    };
    let mut out = Vec::new();
    if is_a::<TextField>(e, control) {
        out.extend(part("background").map(|p| (p, "frame")));
    } else if is_a::<CheckBox>(e, control)
        || is_a::<RadioButton>(e, control)
        || is_a::<Switch>(e, control)
    {
        out.extend(part("indicator").map(|p| (p, "indicator")));
    } else if is_a::<Slider>(e, control) {
        out.extend(part("background").map(|p| (p, "groove")));
        out.extend(part("handle").map(|p| (p, "handle")));
    } else if is_a::<AbstractButton>(e, control) && icon_only(cx, control) {
        out.extend(part("contentItem").map(|p| (p, "icon")));
    }
    out
}

/// A button that shows no text, so what it draws is all that says what
/// it is.
fn icon_only(cx: &Cx, button: ObjectId) -> bool {
    let e = cx.engine;
    let text = e.untrack(|| e.read_by_name(button, "text")).to_js_string();
    text.as_str().trim().is_empty()
}

/// Whether anything under `part` is painted in a colour that can be seen.
fn paints(cx: &Cx, snapshot: &FrameSnapshot, part: ObjectId) -> bool {
    snapshot.items.iter().any(|p| {
        p.opacity > 0.0
            && cx
                .scene
                .item_of(p.node)
                .is_some_and(|i| within(cx.engine, i, part))
            && p.list.as_ref().is_some_and(|l| {
                l.ops().iter().any(|op| match op {
                    DrawOp::Rect { fill, border, .. } => {
                        !fill.is_invisible()
                            || border
                                .as_ref()
                                .is_some_and(|b| b.width > 0.0 && !b.paint.is_invisible())
                    }
                    DrawOp::Glyphs { .. }
                    | DrawOp::Text { .. }
                    | DrawOp::Path { .. }
                    | DrawOp::Shape { .. }
                    | DrawOp::Image { .. }
                    | DrawOp::NinePatch { .. } => true,
                    _ => false,
                })
            })
    })
}

/// The most a pixel of `part` stands out from what is behind it, or
/// `None` when it paints nothing.
fn contrast(
    cx: &Cx,
    painter: &mut Painter,
    snapshot: &FrameSnapshot,
    part: ObjectId,
) -> Option<(f32, Srgb, Srgb)> {
    if !paints(cx, snapshot, part) {
        return None;
    }
    let region = scene_rect(cx.engine, part)?.inflate(2.0, 2.0);
    let with = painter.draw(snapshot, region, |_| true)?;
    let without = painter.draw(snapshot, region, |p| {
        !cx.scene
            .item_of(p.node)
            .is_some_and(|i| within(cx.engine, i, part))
    })?;
    let mut best: Option<(f32, Srgb, Srgb)> = None;
    for (a, b) in with.pairs(&without) {
        let r = wcag_contrast(a, b);
        if best.is_none_or(|(m, _, _)| r > m) {
            best = Some((r, a, b));
        }
    }
    best
}

pub(crate) fn check(cx: &mut Cx) {
    let Some(snapshot) = cx.scene.snapshot().cloned() else {
        return;
    };
    let e = cx.engine;
    let mut painter = Painter::new(backdrop(e, cx.options));
    let mut found = Vec::new();
    for control in shown_items(cx) {
        if !takes_focus(e, control) {
            continue;
        }
        for (part, what) in parts(cx, control) {
            let own = declared_by_app(e, part);
            if !own || !e.untrack(|| e.read(part, Item::VISIBLE.index)).to_boolean() {
                continue;
            }
            let Some((r, shown, behind)) = contrast(cx, &mut painter, &snapshot, part) else {
                continue;
            };
            if r + 0.005 < NONTEXT_CONTRAST {
                found.push((control, part, what, r, shown, behind));
            }
        }
    }
    for (control, part, what, r, shown, behind) in found {
        let message = format!(
            "the {what} of {} stands out {r:.2}:1 from what is behind it ({} on {}), where \
             {}:1 is the least for what a user needs to see of a control",
            words(cx, control),
            shown.to_hex(),
            behind.to_hex(),
            trim(NONTEXT_CONTRAST),
        );
        let f = cx.report("color-nontext-contrast", Some(part), message);
        f.measured = Some(format!("{r:.2}:1 < {}:1", trim(NONTEXT_CONTRAST)));
    }
}

fn trim(n: f32) -> String {
    let s = format!("{n:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}
