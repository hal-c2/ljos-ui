//! `a11y-target-size`: what is pressed is large enough to hit.

use ljos_a11y::Role;

use ljos_engine::{Engine, ObjectId};
use ljos_quick::Item;

use crate::cx::{Cx, acted_on, node_words, trim};

fn on_tab(engine: &Engine, item: ObjectId) -> bool {
    engine
        .untrack(|| engine.read(item, Item::ACTIVE_FOCUS_ON_TAB.index))
        .to_boolean()
}

pub(crate) fn check(cx: &mut Cx) {
    let least = cx.options.target_size;
    for (key, _, data) in cx.nodes().iter() {
        // A link sits in a line of text, and what a scroll bar does the
        // wheel, the keys and a drag of the content do too: neither is
        // held to the size (WCAG 2.5.8, "inline" and "equivalent"). A
        // splitter that Tab stops at has its equivalent in the keys, and is
        // drawn narrow so as not to take presses from what is beside it.
        // The Tab stop stands for the keys: what a `Keys` handler does
        // cannot be read, and a splitter is only made a Tab stop to be
        // moved from the keyboard.
        let equivalent = matches!(data.qt_role, Role::Link | Role::ScrollBar)
            || (data.qt_role == Role::Splitter
                && cx.item(*key).is_some_and(|i| on_tab(cx.engine, i)));
        if !acted_on(data) || data.disabled || equivalent {
            continue;
        }
        let (w, h) = (
            data.bounds.x1 - data.bounds.x0,
            data.bounds.y1 - data.bounds.y0,
        );
        // A box of no area is not small but unseen: `a11y-hidden-exposed`.
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        // A hair under is a rounding, not a finding.
        if w + 0.01 >= least && h + 0.01 >= least {
            continue;
        }
        let item = cx.item(*key);
        let f = cx.report(
            "a11y-target-size",
            item,
            format!(
                "{} is {} by {}, smaller than the {least} by {least} there must be to press",
                node_words(data),
                trim(w),
                trim(h),
            ),
        );
        f.measured = Some(format!("{}x{} < {least}x{least}", trim(w), trim(h)));
    }
}
