//! `layout-offscreen`: something to operate lies outside the window.
//!
//! The item's box in the scene against the window's. What a `Flickable`
//! can scroll into the part of it the window shows is left out, and so is
//! what is shown fully transparent. A Flickable that is itself past the
//! window, or as large as what it holds, brings nothing back (the list of
//! a popup opened past the window's edge).

use ljos_engine::{Engine, ObjectId};
use ljos_quick::Flickable;
use ljos_render::kurbo::Rect;

use crate::cx::{
    Cx, acted_on, ancestors, is_a, number, overhang, overhang_words, scene_rect, shown_opacity,
    words,
};

/// Whether a Flickable that holds `item` (its box `rect`) can scroll it
/// into the part of that Flickable the window shows. A `ScrollView`
/// scrolls with the Flickable it holds; one with none is taken at its
/// word.
fn scrolls_into_view(engine: &Engine, item: ObjectId, rect: Rect, window: Rect) -> bool {
    for a in ancestors(engine, item) {
        if is_a::<ljos_controls::ScrollView>(engine, a)
            && !ancestors(engine, item)
                .take_while(|b| *b != a)
                .any(|b| is_a::<Flickable>(engine, b))
        {
            return true;
        }
        if !is_a::<Flickable>(engine, a) {
            continue;
        }
        let Some(shown) = scene_rect(engine, a).map(|r| r.intersect(window)) else {
            continue;
        };
        if shown.width() <= 0.0 || shown.height() <= 0.0 {
            continue;
        }
        let n = |name: &str| number(engine, a, name);
        // How far what it holds can move each way along an axis: back to
        // its origin, and on to its far end.
        let room = |at: f64, origin: f64, content: f64, size: f64| {
            let last = origin + (content - size).max(0.0);
            ((last - at).max(0.0), (at - origin).max(0.0))
        };
        let (left, right) = room(n("contentX"), n("originX"), n("contentWidth"), n("width"));
        let (up, down) = room(n("contentY"), n("originY"), n("contentHeight"), n("height"));
        // Brought in when moved far enough to cross into what shows.
        let fits = |past_far: f64, past_near: f64, towards_near: f64, towards_far: f64| {
            (past_far <= 0.0 || towards_near > past_far)
                && (past_near <= 0.0 || towards_far > past_near)
        };
        if fits(rect.x0 - shown.x1, shown.x0 - rect.x1, left, right)
            && fits(rect.y0 - shown.y1, shown.y0 - rect.y1, up, down)
        {
            return true;
        }
    }
    false
}

pub(crate) fn check(cx: &mut Cx) {
    let window = cx.scene.viewport();
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let Some(rect) = scene_rect(cx.engine, item) else {
            continue;
        };
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        let Some(past) = overhang_words(overhang(rect, window)) else {
            continue;
        };
        if scrolls_into_view(cx.engine, item, rect, window) || shown_opacity(cx.engine, item) <= 0.0
        {
            continue;
        }
        let gone = rect.intersect(window);
        let how = if gone.width() <= 0.0 || gone.height() <= 0.0 {
            "wholly"
        } else {
            "partly"
        };
        let message = format!(
            "{} lies {how} outside the {}x{} window, past {past}",
            words(cx, item),
            window.width(),
            window.height()
        );
        let f = cx.report("layout-offscreen", Some(item), message);
        f.measured = Some(format!("past {past}"));
    }
}
