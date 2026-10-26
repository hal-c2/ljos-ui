//! `a11y-set-size`: among rows of which one says its place, each says
//! its place and how many there are. Rows that are made as they scroll
//! into view cannot be counted, so the count has to be told; rows that
//! tell only half, or some of which do not tell, are read wrong.

use std::collections::HashMap;

use ljos_a11y::NodeKey;

use crate::cx::{Cx, node_words};

pub(crate) fn check(cx: &mut Cx) {
    let all = cx.nodes();
    let mut telling: HashMap<NodeKey, bool> = HashMap::new();
    for (_, parent, data) in all.iter() {
        if data.qt_role.in_set() {
            *telling.entry(*parent).or_default() |=
                data.position_in_set.is_some() || data.size_of_set.is_some();
        }
    }
    for (key, parent, data) in all.iter() {
        if !data.qt_role.in_set() || !telling.get(parent).copied().unwrap_or(false) {
            continue;
        }
        let wrong = match (data.position_in_set, data.size_of_set) {
            (Some(place), Some(of)) if place >= 1 && place <= of => continue,
            (Some(place), Some(of)) => format!("says it is {place} of {of}"),
            (Some(_), None) => "says its place and not how many there are".to_string(),
            (None, Some(_)) => "says how many there are and not its place".to_string(),
            (None, None) => "does not say its place among rows that do".to_string(),
        };
        let item = cx.item(*key);
        cx.report(
            "a11y-set-size",
            item,
            format!(
                "{} {wrong}; rows made from a model tell both, so make this row from the \
                 model too",
                node_words(data)
            ),
        );
    }
}
