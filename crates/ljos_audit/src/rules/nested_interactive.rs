//! `a11y-nested-interactive`: something to press inside something else to
//! press. A reader announces the outer one with the inner one's words in
//! its name, a press lands on one of them by where it falls, and a switch
//! user cannot reach the inner one alone.
//!
//! Read off the accessibility tree: a pressable node under a pressable
//! node. A field, a spin box or a combo box holds its own buttons by
//! design and is left out; so are the rows of a popup list.

use std::collections::HashMap;

use ljos_a11y::{NodeData, NodeKey, Role};

use crate::cx::{Cx, DEPTH, acted_on, is_a, node_words, place_of};

/// What holds its parts to press as one control.
fn composite(data: &NodeData) -> bool {
    matches!(
        data.qt_role,
        Role::ComboBox
            | Role::SpinBox
            | Role::EditableText
            | Role::ScrollBar
            | Role::Slider
            | Role::PopupMenu
            | Role::MenuBar
            | Role::Dialog
            | Role::ToolBar
            | Role::List
            | Role::Table
            | Role::Tree
            | Role::PageTabList
    )
}

fn pressable(data: &NodeData) -> bool {
    acted_on(data) && data.press && !composite(data)
}

pub(crate) fn check(cx: &mut Cx) {
    let all = cx.nodes();
    let by_key: HashMap<NodeKey, (NodeKey, &NodeData)> =
        all.iter().map(|(k, p, d)| (*k, (*p, d))).collect();
    for (key, parent, data) in all.iter() {
        if !pressable(data) {
            continue;
        }
        let mut cur = *parent;
        let mut outer = None;
        for _ in 0..DEPTH {
            let Some((up, d)) = by_key.get(&cur) else {
                break;
            };
            if composite(d) {
                break;
            }
            // A `Page` has the role of a tab and is a pane, not pressed.
            let page = cx
                .item(cur)
                .is_some_and(|i| is_a::<ljos_controls::Page>(cx.engine, i));
            if pressable(d) && !page {
                outer = Some((cur, *d));
                break;
            }
            cur = *up;
        }
        let Some((outer_key, outer_data)) = outer else {
            continue;
        };
        let at = cx
            .item(outer_key)
            .and_then(|i| place_of(cx.engine, i))
            .map(|p| format!(" ({p})"))
            .unwrap_or_default();
        let item = cx.item(*key);
        cx.report(
            "a11y-nested-interactive",
            item,
            format!(
                "{} is inside {}{at}, which is pressed too; an assistive technology reads \
                 them as one and a press lands on either; put it beside the other, not in it",
                node_words(data),
                node_words(outer_data)
            ),
        );
    }
}
