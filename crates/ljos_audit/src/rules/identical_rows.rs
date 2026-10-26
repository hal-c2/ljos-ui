//! `content-identical-rows`: rows of one list carry the same label, so
//! nobody can tell them apart or choose one, by eye or by ear.
//!
//! A row's label is its accessible name, or what it shows when it has
//! none. Rows the view has made and shows are compared;
//! `limits::IDENTICAL_ROWS_MIN` of one label is the finding.

use std::collections::HashMap;

use ljos_design::limits::IDENTICAL_ROWS_MIN;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{ItemView, view_item_at};

use super::controls::text_of;
use crate::cx::{Cx, DEPTH, is_a, shown_items};

fn shown_words(e: &Engine, row: ObjectId) -> String {
    let mut out = Vec::new();
    let mut stack = vec![row];
    let mut n = 0;
    while let Some(i) = stack.pop() {
        n += 1;
        if n > DEPTH {
            break;
        }
        if let Some(t) = text_of(e, i).filter(|t| !t.trim().is_empty()) {
            out.push(t.trim().to_string());
        }
        stack.extend(e.visual_children(i).into_iter().rev());
    }
    out.join(" ")
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let shown = shown_items(cx);
    for view in shown.iter().copied().filter(|i| is_a::<ItemView>(e, *i)) {
        let Ok(count) = e.untrack(|| e.with::<ItemView, _>(view, |v| v.count)) else {
            continue;
        };
        let mut labels: HashMap<String, usize> = HashMap::new();
        for i in 0..usize::try_from(count).unwrap_or(0).min(DEPTH) {
            let Some(row) = view_item_at(e, view, i).filter(|r| shown.contains(r)) else {
                continue;
            };
            let name = cx
                .scene
                .accessible_node(row)
                .and_then(|k| cx.scene.accessibility_tree().get(k).map(|d| d.name.clone()))
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| shown_words(e, row));
            let name = name.trim().to_string();
            if !name.is_empty() {
                *labels.entry(name).or_default() += 1;
            }
        }
        let Some((label, n)) = labels
            .into_iter()
            .filter(|(_, n)| *n >= IDENTICAL_ROWS_MIN)
            .max_by_key(|(_, n)| *n)
        else {
            continue;
        };
        cx.report(
            "content-identical-rows",
            Some(view),
            format!(
                "{n} rows of the view are all labelled {label:?}, so none can be told from \
                 another; put what tells them apart in the label (a name, a date, a value)"
            ),
        );
    }
}
