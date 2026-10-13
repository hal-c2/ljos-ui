//! What a Flickable tells an assistive technology, and the scrolling one
//! asks of it (ARCHITECTURE.md §14).
//!
//! A Flickable is in the accessibility tree when its author gave it a
//! role or a name, as any item is. Its node then says how far the content
//! is scrolled and takes the scroll actions. What is scrolled out of view
//! stays in the tree: a Flickable that clips says so, and the assistive
//! technology works out from the bounds which children are off screen.

use ljos_a11y::{NodeData, Scroll};
use ljos_engine::{Engine, ObjectId};

use super::{
    WHEEL_NOTCH_PIXELS, X, Y, bounds, dir, flag, fling, is_flickable, num, scroll_to, state_of,
};
use crate::accessible::{AccessibleAction, AccessibleItem};
use crate::item::Item;

/// How far the content of `flickable` is scrolled and can be (tracked).
pub fn scroll_extent(engine: &Engine, flickable: ObjectId) -> Scroll {
    let (x, y) = (bounds(engine, flickable, X), bounds(engine, flickable, Y));
    Scroll {
        x: x.position,
        x_min: x.min,
        x_max: x.max,
        y: y.position,
        y_min: y.min,
        y_max: y.max,
    }
}

/// Does a scroll action on `flickable`: a page is what fits in it, any
/// other step is one notch of a wheel. Returns whether the content moved;
/// an action that is not a scroll moves nothing.
pub fn scroll_by_action(engine: &Engine, flickable: ObjectId, action: AccessibleAction) -> bool {
    use AccessibleAction as A;
    let (axis, forward, page) = match action {
        A::ScrollUp { page } => (Y, false, page),
        A::ScrollDown { page } => (Y, true, page),
        A::ScrollLeft { page } => (X, false, page),
        A::ScrollRight { page } => (X, true, page),
        A::Increase | A::Decrease | A::SetValue(_) | A::ShowOnScreen | A::Expand | A::Collapse => {
            return false;
        }
    };
    if !is_flickable(engine, flickable) {
        return false;
    }
    engine.untrack(|| {
        let at = bounds(engine, flickable, axis);
        let by = if page {
            num(engine, flickable, dir(axis).view)
        } else {
            WHEEL_NOTCH_PIXELS
        };
        let to = if forward {
            at.position + by
        } else {
            at.position - by
        };
        let to = to.clamp(at.min, at.max);
        if to == at.position {
            return false;
        }
        // The assistive technology's step wins over a fling under way.
        if let Some(st) = state_of(engine, flickable) {
            let mut axes = [false; 2];
            axes[axis] = true;
            fling::stop(engine, flickable, &st, axes);
        }
        scroll_to(engine, flickable, axis, to);
        true
    })
}

fn describe(engine: &Engine, obj: ObjectId, data: &mut NodeData) {
    data.scroll = Some(scroll_extent(engine, obj));
    data.clips_children = flag(engine, obj, Item::CLIP.index);
}

pub(super) fn hooks() -> AccessibleItem {
    AccessibleItem {
        describe,
        act: scroll_by_action,
    }
}
