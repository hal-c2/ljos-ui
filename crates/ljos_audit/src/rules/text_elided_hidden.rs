//! `text-elided-hidden`: elided text whose whole is nowhere to be read.
//!
//! A `Text` that is `truncated` shows only part of what it says. The rest
//! is reachable when the text, or an item it lies in, has a tool tip
//! (`ToolTip.text`, or a `ToolTip` declared in it) or an
//! `Accessible.description` that holds the whole text. Its accessible
//! name does not count: a text is named by what it says by default, and
//! that reaches a screen reader only.

use ljos_controls::{ToolTip, ToolTipAttached};
use ljos_engine::{Engine, MetaRef, ObjectId, PropIndex, QmlType};
use ljos_quick::{Accessible, AccessibleAttached, Text};

use crate::cx::{Cx, ancestors, is_a, shown_items, shown_opacity};

fn says(engine: &Engine, obj: ObjectId, index: PropIndex) -> String {
    engine
        .untrack(|| engine.read(obj, index))
        .to_js_string()
        .as_str()
        .to_string()
}

/// What the item tells of itself beside what it shows.
fn told(engine: &Engine, item: ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(att) =
        engine.existing_attached_object(item, &MetaRef::Static(ToolTip::static_meta()))
    {
        out.push(says(engine, att, ToolTipAttached::TEXT.index));
    }
    if let Some(att) =
        engine.existing_attached_object(item, &MetaRef::Static(Accessible::static_meta()))
    {
        out.push(says(engine, att, AccessibleAttached::DESCRIPTION.index));
    }
    for r in ljos_quick::resources(engine, item) {
        if is_a::<ToolTip>(engine, r) {
            out.push(
                engine
                    .untrack(|| engine.read_by_name(r, "text"))
                    .to_js_string()
                    .as_str()
                    .to_string(),
            );
        }
    }
    out
}

pub(crate) fn check(cx: &mut Cx) {
    let engine = cx.engine;
    for text in shown_items(cx) {
        if !is_a::<Text>(engine, text) {
            continue;
        }
        let cut = engine
            .untrack(|| engine.read_by_name(text, "truncated"))
            .to_boolean();
        if !cut || shown_opacity(engine, text) <= 0.0 {
            continue;
        }
        let whole = engine
            .untrack(|| engine.read_by_name(text, "text"))
            .to_js_string()
            .as_str()
            .trim()
            .to_string();
        if whole.is_empty() {
            continue;
        }
        let reachable = std::iter::once(text)
            .chain(ancestors(engine, text))
            .flat_map(|a| told(engine, a))
            .any(|t| t.contains(&whole));
        if reachable {
            continue;
        }
        let shown: String = whole.chars().take(40).collect();
        let more = if whole.chars().count() > 40 {
            "…"
        } else {
            ""
        };
        let message = format!(
            "the text {shown:?}{more} is elided and its whole is in no tool tip or \
             Accessible.description"
        );
        cx.report("text-elided-hidden", Some(text), message);
    }
}
