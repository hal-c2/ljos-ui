//! `layout-reflow`: at the width WCAG 2.2 SC 1.4.10 names (320 CSS px,
//! `limits::REFLOW_WIDTH_PX`), reading needs scrolling both ways: a view
//! of text wider and taller than itself, so every line is read by
//! scrolling sideways and back.
//!
//! Only at a point of the matrix where the window is that narrow or
//! narrower (the window's minimum size, or a size asked for). A table, a
//! tree, a map, a picture or a canvas scrolls both ways by its nature
//! (WCAG's own exception) and is left out: a view counts only when it
//! holds text and is no table.

use ljos_design::limits::REFLOW_WIDTH_PX;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Flickable, Item, TableView, Text};

use crate::cx::{Cx, DEPTH, ancestors, is_a, shown_items, slack, trim};

fn holds_text(e: &Engine, view: ObjectId) -> bool {
    let mut stack = e.visual_children(view);
    let mut n = 0;
    while let Some(o) = stack.pop() {
        n += 1;
        if n > DEPTH {
            return false;
        }
        if is_a::<Text>(e, o)
            && !e
                .untrack(|| e.read(o, Text::TEXT.index))
                .to_js_string()
                .as_str()
                .trim()
                .is_empty()
        {
            return true;
        }
        stack.extend(e.visual_children(o));
    }
    false
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let window = cx.scene.viewport();
    if window.width() > f64::from(REFLOW_WIDTH_PX) + slack() {
        return;
    }
    for view in shown_items(cx) {
        if !is_a::<Flickable>(e, view) || is_a::<TableView>(e, view) {
            continue;
        }
        // One view inside another that scrolls the same way: the outer one
        // is reported.
        if ancestors(e, view).any(|a| is_a::<Flickable>(e, a)) {
            continue;
        }
        let (w, h, cw, ch) = e.untrack(|| {
            let (cw, ch) = e
                .with::<Flickable, _>(view, |f| (f.content_width, f.content_height))
                .unwrap_or_default();
            (
                e.read(view, Item::WIDTH.index).to_number(),
                e.read(view, Item::HEIGHT.index).to_number(),
                cw,
                ch,
            )
        });
        if cw <= w + slack() || ch <= h + slack() || !holds_text(e, view) {
            continue;
        }
        cx.report(
            "layout-reflow",
            Some(view),
            format!(
                "at {} px wide this view of text is {} px wide and {} px tall inside {} by {}, \
                 so reading it needs scrolling both ways; let its content take the view's \
                 width and wrap",
                trim(window.width()),
                trim(cw),
                trim(ch),
                trim(w),
                trim(h)
            ),
        )
        .measured = Some(format!("content {} px wide in {}", trim(cw), trim(w)));
    }
}
