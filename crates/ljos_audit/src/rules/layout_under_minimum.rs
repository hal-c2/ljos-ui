//! `layout-under-minimum`: an item laid out smaller than it can be.
//!
//! Two minimums are known for sure: a button's implicit size, which is
//! what its label and padding take (a button squeezed below it cuts or
//! overlaps its label), and a `Layout.minimumWidth` or `minimumHeight`
//! the application gave. Other items' implicit sizes are only a
//! preference and are not held to.

use ljos_controls::AbstractButton;
use ljos_engine::{Engine, MetaRef, ObjectId, PropIndex, QmlType};
use ljos_quick::{Layout, LayoutAttached};

use crate::cx::{Cx, is_a, shown_items, shown_opacity, slack, trim, words};

fn number(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_number()
}

/// The least size the item holds to, and where it comes from.
fn least(engine: &Engine, item: ObjectId) -> Option<([f64; 2], &'static str)> {
    if let Some(att) =
        engine.existing_attached_object(item, &MetaRef::Static(Layout::static_meta()))
    {
        let read = |i: PropIndex| engine.untrack(|| engine.read(att, i)).to_number();
        let w = read(LayoutAttached::MINIMUM_WIDTH.index);
        let h = read(LayoutAttached::MINIMUM_HEIGHT.index);
        if w > 0.0 || h > 0.0 {
            return Some(([w.max(0.0), h.max(0.0)], "its Layout.minimum size"));
        }
    }
    if is_a::<AbstractButton>(engine, item) {
        let w = number(engine, item, "implicitWidth");
        let h = number(engine, item, "implicitHeight");
        return Some(([w, h], "its implicit size"));
    }
    None
}

pub(crate) fn check(cx: &mut Cx) {
    let engine = cx.engine;
    for item in shown_items(cx) {
        let Some(([lw, lh], whose)) = least(engine, item) else {
            continue;
        };
        let (w, h) = (
            number(engine, item, "width"),
            number(engine, item, "height"),
        );
        let narrow = lw.is_finite() && w + slack() < lw;
        let short = lh.is_finite() && h + slack() < lh;
        // An item sized to nothing is hidden, not squeezed.
        if !(narrow || short) || w <= 0.0 || h <= 0.0 || shown_opacity(engine, item) <= 0.0 {
            continue;
        }
        let message = format!(
            "{} is laid out {}x{}, below {whose} of {}x{}",
            words(cx, item),
            trim(w),
            trim(h),
            trim(lw),
            trim(lh)
        );
        let f = cx.report("layout-under-minimum", Some(item), message);
        f.measured = Some(format!(
            "{}x{} < {}x{}",
            trim(w),
            trim(h),
            trim(lw),
            trim(lh)
        ));
    }
}
