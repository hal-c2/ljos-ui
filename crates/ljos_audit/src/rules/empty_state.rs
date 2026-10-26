//! `content-empty-state`: a list or grid with no rows shows only its
//! frame (and its header), so a user cannot tell empty from broken or
//! still loading.
//!
//! Something that says so is any text shown over the view other than its
//! header and footer, or a busy indicator that runs there. A view too
//! small to look into (a strip of chips, `limits::EMPTY_VIEW_MIN_PX`) is
//! left out, and so is a combo box's list, which is opened to choose.

use ljos_controls::{BusyIndicator, item_popup};
use ljos_design::limits::EMPTY_VIEW_MIN_PX;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::ItemView;
use ljos_render::kurbo::Rect;

use super::controls::{shown_texts, text_of};
use crate::cx::{Cx, is_a, scene_rect, shown_items, within};

fn view_part(e: &Engine, view: ObjectId, name: &str) -> Option<ObjectId> {
    e.untrack(|| e.read_by_name(view, name)).as_object()
}

fn over(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.0 && i.height() > 0.0
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    let texts = shown_texts(cx);
    let busy: Vec<Rect> = shown
        .iter()
        .filter(|i| is_a::<BusyIndicator>(e, **i))
        .filter(|i| e.untrack(|| e.read_by_name(**i, "running")).to_boolean())
        .filter_map(|i| scene_rect(e, *i))
        .collect();
    for view in shown.iter().copied().filter(|i| is_a::<ItemView>(e, *i)) {
        let Ok(count) = e.untrack(|| e.with::<ItemView, _>(view, |v| v.count)) else {
            continue;
        };
        if count > 0 || item_popup(e, view).is_some() {
            continue;
        }
        let Some(r) = scene_rect(e, view) else {
            continue;
        };
        let least = f64::from(EMPTY_VIEW_MIN_PX);
        if r.width() < least || r.height() < least {
            continue;
        }
        let ends: Vec<ObjectId> = ["headerItem", "footerItem"]
            .into_iter()
            .filter_map(|n| view_part(e, view, n))
            .collect();
        let said = texts.iter().any(|(t, _)| {
            !ends.iter().any(|end| within(e, *t, *end))
                && scene_rect(e, *t).is_some_and(|tr| over(tr, r))
        }) || busy.iter().any(|b| over(*b, r));
        if said {
            continue;
        }
        let heading = ends
            .iter()
            .find_map(|end| {
                std::iter::once(*end)
                    .chain(e.visual_children(*end))
                    .find_map(|i| text_of(e, i))
            })
            .filter(|t| !t.trim().is_empty())
            .map(|t| format!(" under the heading {:?}", t.trim()))
            .unwrap_or_default();
        cx.report(
            "content-empty-state",
            Some(view),
            format!(
                "the view has no rows and shows nothing in their place{heading}, so empty \
                 cannot be told from broken or still loading; show what is empty and what to \
                 do (\"No recent files yet\"), or a busy indicator while it loads"
            ),
        );
    }
}
