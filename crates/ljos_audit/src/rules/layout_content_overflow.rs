//! `layout-content-overflow`: what a control's content draws runs past the
//! box its padding leaves, so it hangs off-centre on the control's own
//! background.
//!
//! A control sizes its `contentItem` to the padding box; a layout there
//! keeps its children at their own sizes from its top, and they hang out
//! when the box is smaller (a row given a fixed height that keeps the
//! style's padding). Only a `contentItem` the application wrote is held to
//! it: what a style puts in a control is the style's to fit, and a control
//! squeezed below its implicit size is `layout-under-minimum`'s.

use ljos_controls::Control;
use ljos_engine::{Engine, ObjectId};
use ljos_render::kurbo::{Point, Rect};

use crate::cx::{
    Cx, DEPTH, declared_by_app, is_a, is_scroller, number, overhang, overhang_words, scene_rect,
    shown_items, shown_opacity, trim, words,
};

/// The box the control's padding leaves, in the scene.
fn padding_box(e: &Engine, control: ObjectId) -> Rect {
    let (l, t) = (
        number(e, control, "leftPadding"),
        number(e, control, "topPadding"),
    );
    let (w, h) = (
        number(e, control, "availableWidth"),
        number(e, control, "availableHeight"),
    );
    let a = ljos_quick::map_to_scene(e, control, Point::new(l, t));
    let b = ljos_quick::map_to_scene(e, control, Point::new(l + w, t + h));
    Rect::from_points(a, b)
}

/// What the items in `item` draw, in the scene: their boxes together. A
/// control or a scroller in it is its own box: what is in those is theirs.
fn drawn(e: &Engine, shown: &[ObjectId], item: ObjectId, depth: usize) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for c in e.visual_children(item) {
        if !shown.contains(&c) || shown_opacity(e, c) <= 0.0 {
            continue;
        }
        let own = scene_rect(e, c).filter(|r| r.area() > 0.0);
        let inner = if depth >= DEPTH || is_a::<Control>(e, c) || is_scroller(e, c) {
            None
        } else {
            drawn(e, shown, c, depth + 1)
        };
        for r in [own, inner].into_iter().flatten() {
            out = Some(out.map_or(r, |o| o.union(r)));
        }
    }
    out
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    for &control in &shown {
        if !is_a::<Control>(e, control) || shown_opacity(e, control) <= 0.0 {
            continue;
        }
        let Some(content) = e
            .untrack(|| e.read_by_name(control, "contentItem"))
            .as_object()
            .filter(|c| shown.contains(c) && declared_by_app(e, *c))
        else {
            continue;
        };
        let room = padding_box(e, control);
        if room.width() <= 0.0 || room.height() <= 0.0 {
            continue;
        }
        let Some(draws) = drawn(e, &shown, content, 0) else {
            continue;
        };
        let Some(past) = overhang_words(overhang(draws, room)) else {
            continue;
        };
        let pad = ["topPadding", "bottomPadding", "leftPadding", "rightPadding"]
            .map(|p| trim(number(e, control, p)));
        let message = format!(
            "{} draws its content {}x{}, but its padding (top {}, bottom {}, left {}, right {}) \
             leaves {}x{} of its {}x{}, so the content runs past {past}; set the padding so \
             the content fits, or let the size follow `implicitWidth` and `implicitHeight`",
            words(cx, control),
            trim(draws.width()),
            trim(draws.height()),
            pad[0],
            pad[1],
            pad[2],
            pad[3],
            trim(room.width()),
            trim(room.height()),
            trim(number(e, control, "width")),
            trim(number(e, control, "height")),
        );
        cx.report("layout-content-overflow", Some(control), message)
            .measured = Some(format!(
            "{}x{} in {}x{}",
            trim(draws.width()),
            trim(draws.height()),
            trim(room.width()),
            trim(room.height())
        ));
    }
}
