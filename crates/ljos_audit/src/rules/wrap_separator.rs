//! `layout-wrap-separator` (Hint): a row that wraps puts a separator at
//! the start or the end of a line, where it separates nothing from
//! nothing. Only a `Flow` that has wrapped onto more than one line is
//! looked at; a separator is a `ToolSeparator`, a `MenuSeparator`, or a
//! thin line (at most `SEPARATOR_PX` across and longer than that). One
//! drawn at an opacity of 0 is how a separator is dropped at a line's edge
//! while it keeps its width, so the wrap points stay put: it shows nothing
//! there and is not reported.

use ljos_controls::{MenuSeparator, ToolSeparator};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::Flow;
use ljos_render::kurbo::Rect;

use crate::cx::{Cx, is_a, scene_rect, shown_items, shown_opacity, slack};

/// A line one draws as a separator is a pixel or two across.
const SEPARATOR_PX: f64 = 2.0;

fn shown(e: &Engine, o: ObjectId) -> bool {
    e.untrack(|| e.read_by_name(o, "visible").to_boolean())
}

fn separator(e: &Engine, o: ObjectId) -> bool {
    if shown_opacity(e, o) <= 0.0 {
        return false;
    }
    if is_a::<ToolSeparator>(e, o) || is_a::<MenuSeparator>(e, o) {
        return true;
    }
    let Some(r) = scene_rect(e, o) else {
        return false;
    };
    let (thin, long) = (r.width().min(r.height()), r.width().max(r.height()));
    thin > 0.0
        && thin <= SEPARATOR_PX
        && long > SEPARATOR_PX * 4.0
        && e.visual_children(o).is_empty()
}

/// The separator a line starts (`first`) or ends with: the child itself,
/// or the outermost shown child of a group the line holds.
fn edge_separator(e: &Engine, child: ObjectId, first: bool) -> Option<ObjectId> {
    if separator(e, child) {
        return Some(child);
    }
    let mut kids: Vec<(ObjectId, Rect)> = e
        .visual_children(child)
        .into_iter()
        .filter(|k| shown(e, *k))
        .filter_map(|k| scene_rect(e, k).map(|r| (k, r)))
        .collect();
    kids.sort_by(|a, b| a.1.x0.total_cmp(&b.1.x0));
    let edge = if first { kids.first() } else { kids.last() }?;
    separator(e, edge.0).then_some(edge.0)
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    for flow in shown_items(cx) {
        if !is_a::<Flow>(e, flow) {
            continue;
        }
        let mut kids: Vec<(ObjectId, Rect)> = e
            .visual_children(flow)
            .into_iter()
            .filter(|k| shown(e, *k))
            .filter_map(|k| scene_rect(e, k).map(|r| (k, r)))
            .filter(|(_, r)| r.width() > 0.0 && r.height() > 0.0)
            .collect();
        kids.sort_by(|a, b| a.1.y0.total_cmp(&b.1.y0).then(a.1.x0.total_cmp(&b.1.x0)));
        // Lines: a child that starts below the line's top begins a new one.
        let mut lines: Vec<Vec<ObjectId>> = Vec::new();
        let mut top = f64::NEG_INFINITY;
        for (k, r) in &kids {
            if lines.is_empty() || r.y0 > top + slack() {
                lines.push(Vec::new());
                top = r.y0;
            }
            lines.last_mut().expect("pushed").push(*k);
        }
        if lines.len() < 2 {
            continue;
        }
        let found = lines.iter().enumerate().find_map(|(n, line)| {
            let start = line.first().and_then(|c| edge_separator(e, *c, true));
            let end = line.last().and_then(|c| edge_separator(e, *c, false));
            start
                .map(|s| (s, n + 1, "starts"))
                .or(end.map(|s| (s, n + 1, "ends")))
        });
        if let Some((sep, line, how)) = found {
            cx.report(
                "layout-wrap-separator",
                Some(sep),
                format!(
                    "line {line} of the wrapped row {how} with a separator, which separates \
                     nothing there; put separators between whole groups, or hide one that \
                     lands at a line's edge"
                ),
            );
        }
    }
}
