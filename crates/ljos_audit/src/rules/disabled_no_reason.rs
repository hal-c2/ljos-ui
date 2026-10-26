//! `content-disabled-no-reason`: a control the application switched off
//! says nothing of why, so a user cannot tell what would turn it on.
//!
//! Only a control whose own `enabled` is set false while what holds it is
//! enabled: a page switched off as a whole says it by being a page. A
//! tooltip or an accessible description is the reason.

use ljos_engine::Provenance;
use ljos_quick::Item;

use super::controls::tool_tip;
use crate::cx::{Cx, acted_on, ancestors, node_words};

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    for (key, _, data) in cx.nodes().iter() {
        if !acted_on(data) || !data.description.trim().is_empty() {
            continue;
        }
        let Some(item) = cx.item(*key) else { continue };
        let off = e.untrack(|| {
            !e.read(item, Item::ENABLED.index).to_boolean()
                && matches!(
                    e.provenance(item, Item::ENABLED.index),
                    Some(Provenance::Assigned | Provenance::Bound)
                )
                && ancestors(e, item)
                    .next()
                    .is_none_or(|p| e.read(p, Item::ENABLED.index).to_boolean())
        });
        if !off || tool_tip(e, item).is_some() {
            continue;
        }
        cx.report(
            "content-disabled-no-reason",
            Some(item),
            format!(
                "{} is switched off and nothing says why; say what turns it on in a tooltip \
                 (`ToolTip.text`, shown on hover even while disabled) or in \
                 `Accessible.description`, or leave it on and explain when it is used",
                node_words(data)
            ),
        );
    }
}
