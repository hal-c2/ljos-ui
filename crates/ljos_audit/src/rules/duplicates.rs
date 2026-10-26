//! `a11y-duplicate-name`: two things to act on under one parent, with the
//! same role and the same name, cannot be told apart by ear or by voice.

use std::collections::HashMap;

use ljos_a11y::NodeKey;

use crate::cx::{Cx, acted_on, node_words, place_of};

pub(crate) fn check(cx: &mut Cx) {
    let mut first: HashMap<(NodeKey, String, String), NodeKey> = HashMap::new();
    for (key, parent, data) in cx.nodes().iter() {
        let name = data.name.trim();
        if name.is_empty() || !acted_on(data) {
            continue;
        }
        let id = (*parent, format!("{:?}", data.role), name.to_string());
        match first.get(&id) {
            None => {
                first.insert(id, *key);
            }
            Some(other) => {
                let other = cx
                    .item(*other)
                    .and_then(|i| place_of(cx.engine, i))
                    .map(|p| format!(" at {p}"))
                    .unwrap_or_default();
                let item = cx.item(*key);
                cx.report(
                    "a11y-duplicate-name",
                    item,
                    format!(
                        "{} has the same name as the one beside it{other}; give each a name \
                         of its own",
                        node_words(data)
                    ),
                );
            }
        }
    }
}
