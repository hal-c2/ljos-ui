//! `layout-popup-fit`: a popup far larger than what it holds, or too
//! small for it.
//!
//! Each open popup's size against its implicit size, which is what its
//! content, header, footer and padding ask for: filled to less than
//! `limits::POPUP_MIN_FILL` it is too large; asked for more than it has
//! on a side it is too small, unless the content scrolls. A popup reaching
//! past the window is too large for it. Tool tips size themselves and are
//! left out, and a drawer is not held to its fill: it runs along an edge
//! of the window by design, whatever it holds.

use ljos_controls::{Drawer, ScrollView, ToolTip, item_popup};
use ljos_design::limits::POPUP_MIN_FILL;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Flickable, child_items};

use crate::cx::{Cx, is_a, overhang, overhang_words, scene_rect, shown_items, slack, trim};

fn number(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_number()
}

/// Whether the content, or an item near its top, scrolls.
fn scrolls(engine: &Engine, content: ObjectId) -> bool {
    let scroller = |o: ObjectId| is_a::<Flickable>(engine, o) || is_a::<ScrollView>(engine, o);
    let mut level = vec![content];
    for _ in 0..3 {
        if level.iter().any(|o| scroller(*o)) {
            return true;
        }
        level = level.iter().flat_map(|o| child_items(engine, *o)).collect();
    }
    false
}

fn name_of(engine: &Engine, popup: ObjectId) -> String {
    let ty = engine
        .meta_of(popup)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| "Popup".into());
    let name = engine
        .untrack(|| engine.read_by_name(popup, "objectName"))
        .to_js_string();
    if name.is_empty() {
        format!("the {ty}")
    } else {
        format!("the {ty} {:?}", name.as_str())
    }
}

pub(crate) fn check(cx: &mut Cx) {
    let engine = cx.engine;
    let window = cx.scene.viewport();
    for item in shown_items(cx) {
        let Some(popup) = item_popup(engine, item) else {
            continue;
        };
        if is_a::<ToolTip>(engine, popup) {
            continue;
        }
        let what = name_of(engine, popup);
        let (aw, ah) = (
            number(engine, popup, "width"),
            number(engine, popup, "height"),
        );
        let (cw, ch) = (
            number(engine, popup, "implicitWidth"),
            number(engine, popup, "implicitHeight"),
        );
        let has = format!("{}x{}", trim(aw), trim(ah));
        let asks = format!("{}x{}", trim(cw), trim(ch));
        if let Some(rect) = scene_rect(engine, item)
            && let Some(past) = overhang_words(overhang(rect, window))
        {
            let message = format!(
                "{what} is {}x{} and reaches past {past} of the {}x{} window",
                trim(rect.width()),
                trim(rect.height()),
                window.width(),
                window.height()
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!("past {past}"));
            continue;
        }
        if !(aw > 0.0 && ah > 0.0 && cw.is_finite() && ch.is_finite()) {
            continue;
        }
        let content = engine
            .untrack(|| engine.read_by_name(popup, "contentItem"))
            .as_object();
        let scrolled = content.is_some_and(|c| scrolls(engine, c));
        if (cw > aw + slack() || ch > ah + slack()) && !scrolled {
            let message = format!(
                "{what} is {has} px where what it holds asks for {asks}, and it does not \
                 scroll"
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!("{has} < {asks}"));
            continue;
        }
        // Content that sizes to nothing is laid out by hand; its fill
        // cannot be told.
        if cw <= 0.0 || ch <= 0.0 || is_a::<Drawer>(engine, popup) {
            continue;
        }
        let fill = (cw.min(aw) * ch.min(ah)) / (aw * ah);
        if fill < f64::from(POPUP_MIN_FILL) {
            let message = format!(
                "{what} is {has} px and what it holds takes {asks}, {:.0} % of it",
                fill * 100.0
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!(
                "fills {:.0} % < {:.0} %",
                fill * 100.0,
                POPUP_MIN_FILL * 100.0
            ));
        }
    }
}
