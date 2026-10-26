//! `a11y-target-size`: what is pressed is large enough to hit.

use ljos_a11y::Role;

use crate::cx::{Cx, acted_on, node_words, trim};

pub(crate) fn check(cx: &mut Cx) {
    let least = cx.options.target_size;
    for (key, _, data) in cx.nodes().iter() {
        // A link sits in a line of text, and what a scroll bar does the
        // wheel, the keys and a drag of the content do too: neither is
        // held to the size (WCAG 2.5.8, "inline" and "equivalent").
        if !acted_on(data) || data.disabled || matches!(data.qt_role, Role::Link | Role::ScrollBar)
        {
            continue;
        }
        let (w, h) = (
            data.bounds.x1 - data.bounds.x0,
            data.bounds.y1 - data.bounds.y0,
        );
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
