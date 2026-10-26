//! `content-inconsistent-words`: one window names one action two ways
//! ("Retry" and "Try again"), so a user wonders whether they differ.
//!
//! Only whole labels of controls a user can act on are compared, against
//! the small list of certain synonyms in `ljos_design::words`.

use ljos_design::words::SYNONYMS;
use ljos_engine::ObjectId;

use super::controls::without_mnemonic;
use crate::cx::{Cx, acted_on};

fn label(name: &str) -> String {
    without_mnemonic(name)
        .trim()
        .trim_end_matches(['…', '.', '!'])
        .trim()
        .to_lowercase()
}

pub(crate) fn check(cx: &mut Cx) {
    let mut labelled: Vec<(ObjectId, String, String)> = Vec::new();
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        labelled.push((item, label(&data.name), data.name.trim().to_string()));
    }
    for group in SYNONYMS {
        let used: Vec<&(ObjectId, String, String)> = labelled
            .iter()
            .filter(|(_, l, _)| group.contains(&l.as_str()))
            .collect();
        let mut kinds: Vec<&str> = used.iter().map(|(_, l, _)| l.as_str()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        if kinds.len() < 2 {
            continue;
        }
        for (item, own, shown) in &used {
            let others: Vec<String> = used
                .iter()
                .filter(|(_, l, _)| l != own)
                .map(|(_, _, s)| format!("{s:?}"))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            cx.report(
                "content-inconsistent-words",
                Some(*item),
                format!(
                    "{shown:?} names the action {} names too; use one word for it throughout",
                    others.join(" and ")
                ),
            );
        }
    }
}
