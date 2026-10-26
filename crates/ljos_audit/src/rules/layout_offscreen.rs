//! `layout-offscreen`: something to operate lies outside the window.
//!
//! The item's box in the scene against the window's. What a `Flickable`
//! or `ScrollView` holds can be scrolled into view and is left out, and so
//! is what is shown fully transparent.

use crate::cx::{
    Cx, acted_on, overhang, overhang_words, scene_rect, scrolls, shown_opacity, words,
};

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
        if scrolls(cx.engine, item) || shown_opacity(cx.engine, item) <= 0.0 {
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
