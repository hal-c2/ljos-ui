//! `layout-under-minimum`: an item laid out smaller than it can be.
//!
//! Three minimums are known for sure: a button's implicit size, which is
//! what its label and padding take (a button squeezed below it cuts or
//! overlaps its label), a `Layout.minimumWidth` or `minimumHeight` the
//! application gave, and an implicit size the application's own QML
//! writes on an item, which says what the item needs. Other implicit
//! sizes (a text's, a layout's, what a style writes inside a control)
//! are only a preference and are not held to.

use ljos_controls::{AbstractButton, is_style_url};
use ljos_engine::{Engine, MetaRef, ObjectId, PropIndex, Provenance, QmlType};
use ljos_quick::{Item, Layout, LayoutAttached};

use crate::cx::{Cx, is_a, number, place_of, shown_items, shown_opacity, slack, trim, words};

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
    written_implicit(engine, item).map(|least| (least, "the implicit size its QML gives it"))
}

/// Whether the item's native type is `Item` or `Rectangle`, which give
/// themselves no implicit size.
fn sizes_nothing(engine: &Engine, item: ObjectId) -> bool {
    let mut meta = engine.meta_of(item);
    while let Some(m) = meta {
        if m.native.is_some() {
            return matches!(&*m.name, "Item" | "Rectangle");
        }
        meta = m.base.clone();
    }
    false
}

/// The implicit size the application's QML writes on the item, a side it
/// does not write at 0.
fn written_implicit(engine: &Engine, item: ObjectId) -> Option<[f64; 2]> {
    let place = place_of(engine, item)?;
    if is_style_url(&place) {
        return None;
    }
    let file = |at: &str| at.rsplitn(3, ':').nth(2).map(str::to_owned);
    let own = [Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_HEIGHT.index].map(|p| {
        matches!(
            engine.provenance(item, p),
            Some(Provenance::Assigned | Provenance::Bound)
        )
    });
    if !own.contains(&true) {
        return None;
    }
    // Only what the file that declares the item writes: a binding in a
    // component's own file (its size from its children) is a preference.
    let declared = file(&place);
    let plain = sizes_nothing(engine, item);
    let mut sides = [0.0; 2];
    for report in engine.inspect_properties(item) {
        let side = match report.name.as_str() {
            "implicitWidth" => 0,
            "implicitHeight" => 1,
            _ => continue,
        };
        // A number written as it is keeps no place, and the toolkit's
        // own writes (a layout's, a text's) look the same: only on an item
        // that sizes nothing itself is it the QML's.
        let by_app = match &report.written_at {
            Some(at) => Some(at.file.as_str()) == declared.as_deref(),
            None => report.provenance == Provenance::Assigned && plain,
        };
        if own[side] && by_app {
            sides[side] = number(engine, item, report.name.as_str());
        }
    }
    (sides != [0.0; 2]).then_some(sides)
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
        // A side with no minimum holds to nothing.
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
