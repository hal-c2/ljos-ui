//! `a11y-ambiguous-name`: two things to act on in one window with one
//! role and one name, not side by side. A list of the window's buttons
//! (a reader's, or speech control's "show numbers") has the same words
//! twice and nothing to tell which is which.
//!
//! Left out, as context tells them apart: two under one parent (that is
//! `a11y-duplicate-name`), rows of a list, table or tree, the items of
//! different menus, and two that run one `Action` (they do the same
//! thing) or have a description.

use std::collections::HashMap;

use ljos_a11y::{NodeData, NodeKey, Role};
use ljos_engine::ObjectId;

use crate::cx::{Cx, DEPTH, acted_on, node_words, place_of};

/// A node above that gives a name its context: a row says which row's
/// "Edit" it is.
fn in_context(data: &NodeData) -> bool {
    matches!(
        data.qt_role,
        Role::ListItem
            | Role::Row
            | Role::Cell
            | Role::TreeItem
            | Role::PopupMenu
            | Role::MenuBar
            | Role::Dialog
    )
}

fn action_of(cx: &Cx, item: Option<ObjectId>) -> Option<ObjectId> {
    let e = cx.engine;
    let item = item?;
    e.untrack(|| e.read_by_name(item, "action")).as_object()
}

pub(crate) fn check(cx: &mut Cx) {
    let all = cx.nodes();
    let up: HashMap<NodeKey, (NodeKey, &NodeData)> =
        all.iter().map(|(k, p, d)| (*k, (*p, d))).collect();
    let has_context = |mut key: NodeKey| {
        for _ in 0..DEPTH {
            let Some((parent, _)) = up.get(&key) else {
                return false;
            };
            match up.get(parent) {
                Some((_, d)) if in_context(d) => return true,
                Some(_) => key = *parent,
                None => return false,
            }
        }
        false
    };
    let mut first: HashMap<(String, String), (NodeKey, NodeKey)> = HashMap::new();
    for (key, parent, data) in all.iter() {
        let name = data.name.trim().to_lowercase();
        if name.is_empty()
            || !acted_on(data)
            || data.disabled
            || !data.description.trim().is_empty()
            || in_context(data)
            || has_context(*key)
        {
            continue;
        }
        let id = (format!("{:?}", data.role), name);
        let Some((other, other_parent)) = first.get(&id).copied() else {
            first.insert(id, (*key, *parent));
            continue;
        };
        if other_parent == *parent {
            continue;
        }
        let item = cx.item(*key);
        let other_item = cx.item(other);
        let action = action_of(cx, item);
        if action.is_some() && action == action_of(cx, other_item) {
            continue;
        }
        let at = other_item
            .and_then(|i| place_of(cx.engine, i))
            .map(|p| format!(" at {p}"))
            .unwrap_or_default();
        cx.report(
            "a11y-ambiguous-name",
            item,
            format!(
                "{} has the name of another one in the window{at}, and a list of what is \
                 there cannot tell them apart; name each by what it acts on",
                node_words(data)
            ),
        );
    }
}
