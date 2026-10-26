//! `text-clipped`: text cut off without an ellipsis.
//!
//! A `Text` (or `Label`) that does not elide and whose laid-out text is
//! larger than its box spills past it, as far as its alignment sends it.
//! Where that, or the box itself, reaches past a clip (its own or an
//! item's above it), the text is cut; spilt text is cut by the window's
//! edge too. A clip at or above something that scrolls is left out: what
//! it hides can be scrolled to. Text wholly outside a clip is not cut but
//! hidden, which is another rule's.
//!
//! Text spilling over a sibling without being cut is not told here.

use ljos_engine::{Engine, ObjectId};
use ljos_quick::enums::{align, elide};
use ljos_quick::{Flickable, Item, Text};
use ljos_render::kurbo::{Point, Rect};

use crate::cx::{
    Cx, ancestors, at, is_a, item_words, overhang, overhang_words, scene_rect, shown_items,
    shown_opacity, slack, trim,
};

fn number(engine: &Engine, item: ObjectId, name: &str) -> f64 {
    engine
        .untrack(|| engine.read_by_name(item, name))
        .to_number()
}

fn clips(engine: &Engine, item: ObjectId) -> bool {
    engine
        .untrack(|| engine.read(item, Item::CLIP.index))
        .to_boolean()
}

fn scrolls_here(engine: &Engine, item: ObjectId) -> bool {
    is_a::<Flickable>(engine, item) || is_a::<ljos_controls::ScrollView>(engine, item)
}

/// The rectangle `local` of `item` in the scene.
fn to_scene(engine: &Engine, item: ObjectId, local: Rect) -> Rect {
    let a = ljos_quick::map_to_scene(engine, item, Point::new(local.x0, local.y0));
    let b = ljos_quick::map_to_scene(engine, item, Point::new(local.x1, local.y1));
    Rect::from_points(a, b)
}

/// Where the text lies in its own coordinates, as wide and high as it is
/// laid out and placed by its alignment.
fn laid_out(engine: &Engine, text: ObjectId, w: f64, h: f64, need: (f64, f64)) -> Rect {
    let h_align = number(engine, text, "effectiveHorizontalAlignment") as i32;
    let v_align = number(engine, text, "verticalAlignment") as i32;
    let (nw, nh) = need;
    let x0 = if h_align & align::RIGHT != 0 {
        w - nw
    } else if h_align & align::H_CENTER != 0 {
        (w - nw) / 2.0
    } else {
        0.0
    };
    let y0 = if v_align & align::BOTTOM != 0 {
        h - nh
    } else if v_align & align::V_CENTER != 0 {
        (h - nh) / 2.0
    } else {
        0.0
    };
    Rect::new(x0, y0, x0 + nw, y0 + nh)
}

pub(crate) fn check(cx: &mut Cx) {
    let engine = cx.engine;
    for text in shown_items(cx) {
        if !is_a::<Text>(engine, text) {
            continue;
        }
        let said = engine
            .untrack(|| engine.read_by_name(text, "text"))
            .to_js_string();
        if said.as_str().trim().is_empty() || shown_opacity(engine, text) <= 0.0 {
            continue;
        }
        if number(engine, text, "elide") as i32 != elide::NONE {
            continue;
        }
        let (w, h) = (
            number(engine, text, "width"),
            number(engine, text, "height"),
        );
        let pad = |side: &str| number(engine, text, side).max(0.0);
        let need_w =
            number(engine, text, "contentWidth") + pad("leftPadding") + pad("rightPadding");
        let need_h =
            number(engine, text, "contentHeight") + pad("topPadding") + pad("bottomPadding");
        let wide = need_w > w + slack();
        let tall = need_h > h + slack();
        if !(w.is_finite() && h.is_finite()) {
            continue;
        }
        let spills = wide || tall;
        let local = laid_out(engine, text, w, h, (need_w.max(w), need_h.max(h)));
        let needed = to_scene(engine, text, local);
        // What shows it: the clips from the text out to what scrolls, or
        // to the window.
        let mut shown = spills.then(|| (cx.scene.viewport(), None));
        let mut cutters: Vec<(Rect, Option<ObjectId>)> = Vec::new();
        for a in std::iter::once(text).chain(ancestors(engine, text)) {
            if scrolls_here(engine, a) {
                shown = None;
                break;
            }
            if ljos_quick::is_item(engine, a)
                && clips(engine, a)
                && let Some(r) = scene_rect(engine, a)
            {
                cutters.push((r, Some(a)));
            }
        }
        cutters.extend(shown);
        let mut first = None;
        for (r, by) in &cutters {
            let seen = r.intersect(needed);
            if seen.width() <= 0.0 || seen.height() <= 0.0 {
                break;
            }
            if let Some(words) = overhang_words(overhang(needed, *r)) {
                first = Some((seen, words, *by));
                break;
            }
        }
        let Some((cut, past, by)) = first else {
            continue;
        };
        let by = match by {
            Some(a) if a == text => "its own clip".to_string(),
            Some(a) => format!("the clip of {}{}", item_words(engine, a), at(engine, a)),
            None => "the window".to_string(),
        };
        let shown_text: String = said.as_str().chars().take(40).collect();
        let message = format!(
            "the text {shown_text:?} needs {}x{} px and is cut off by {by}, past {past}",
            trim(needed.width()),
            trim(needed.height()),
        );
        let f = cx.report("text-clipped", Some(text), message);
        f.measured = Some(format!(
            "needs {}x{} px, shows {}x{}",
            trim(needed.width()),
            trim(needed.height()),
            trim(cut.width().max(0.0)),
            trim(cut.height().max(0.0)),
        ));
    }
}
