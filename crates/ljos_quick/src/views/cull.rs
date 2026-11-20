//! Rows a view made but does not show.
//!
//! A view makes rows past what it shows, in its `cacheBuffer`, so that
//! they are ready when they come in. Those rows are items like any other
//! and read `visible: true`, but a row wholly outside the view and its
//! display margins is not drawn, clipped view or not: qml6 leaves the
//! window under an unclipped list blank where its next cached row lies.
//! A row partly inside, or touching the edge, is drawn whole. The view
//! says which rows those are each time it lays them out, and the scene
//! leaves them out of the picture; they still take the pointer, as an
//! item does that only is not painted.
//!
//! A row the view made and has not put anywhere yet is not drawn either:
//! made by the incubator, it is the view's child from the moment it is
//! allocated, at its own `x` and `y`, and half made for as many frames as
//! its making takes. qml6 culls a delegate from `QQuickItemView::initItem`
//! until the view lays it out.
//!
//! A delegate waiting in a view's pool (`reuseItems`) is neither drawn
//! nor given the pointer, and is not told so either: it reads `visible`
//! as it set it, and so does what is in it, as on qml6. Telling it would
//! cost each delegate two changes of `visible` through everything in it
//! on its way to another row.

use std::cell::{Cell, RefCell};

use ljos_engine::{Engine, ObjectId};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Default)]
struct Culled {
    /// The rows each view does not show.
    by_view: RefCell<FxHashMap<ObjectId, FxHashSet<ObjectId>>>,
    /// The delegates in the pools of all views.
    pooled: RefCell<FxHashSet<ObjectId>>,
    /// The rows made that their view has not placed yet.
    unplaced: RefCell<FxHashSet<ObjectId>>,
    /// Counts the changes, so a scene asks again only after one.
    generation: Cell<u64>,
}

/// `view` shows none of `rows` and every other row of its own.
pub(crate) fn set(e: &Engine, view: ObjectId, rows: FxHashSet<ObjectId>) {
    let c = e.extension::<Culled>();
    let mut by_view = c.by_view.borrow_mut();
    let same = match by_view.get(&view) {
        Some(was) => *was == rows,
        None => rows.is_empty(),
    };
    if same {
        return;
    }
    by_view.retain(|v, _| e.is_alive(*v));
    if rows.is_empty() {
        by_view.remove(&view);
    } else {
        by_view.insert(view, rows);
    }
    c.generation.set(c.generation.get() + 1);
}

/// `row` went to its view's pool, or (`pooled` false) came out of it or
/// is gone.
pub(crate) fn pool(e: &Engine, row: ObjectId, pooled: bool) {
    let c = e.extension::<Culled>();
    let changed = if pooled {
        c.pooled.borrow_mut().insert(row)
    } else {
        c.pooled.borrow_mut().remove(&row)
    };
    if changed {
        c.generation.set(c.generation.get() + 1);
    }
}

/// The rows the set of unplaced ones keeps room for once it is empty.
const UNPLACED_KEPT: usize = 16;

/// `row` was made for its view (`made`) and is not drawn until the view
/// puts it in its place, or the view put it there (`made` false) or it
/// is gone.
pub(crate) fn unplaced(e: &Engine, row: ObjectId, made: bool) {
    let c = e.extension::<Culled>();
    let changed = if made {
        c.unplaced.borrow_mut().insert(row)
    } else {
        // Every row the view places, every pass: most were placed before.
        let mut unplaced = c.unplaced.borrow_mut();
        let removed = !unplaced.is_empty() && unplaced.remove(&row);
        if removed && unplaced.is_empty() {
            // A view made a thousand rows at once has a table for a
            // thousand: given back once they are all placed, not kept as
            // 18 bytes a row for the life of the engine. A screenful's
            // room stays, so the next rows made do not grow it again.
            unplaced.shrink_to(UNPLACED_KEPT);
        }
        removed
    };
    if changed {
        c.generation.set(c.generation.get() + 1);
    }
}

/// Every delegate waiting in a pool.
pub(crate) fn pooled(e: &Engine) -> FxHashSet<ObjectId> {
    let c = e.extension::<Culled>();
    c.pooled.borrow_mut().retain(|o| e.is_alive(*o));
    c.pooled.borrow().clone()
}

/// How many times what is culled or pooled changed.
pub(crate) fn generation(e: &Engine) -> u64 {
    e.extension::<Culled>().generation.get()
}

/// Every row a view made and does not show, or has not placed yet.
pub(crate) fn all(e: &Engine) -> FxHashSet<ObjectId> {
    let c = e.extension::<Culled>();
    {
        let mut unplaced = c.unplaced.borrow_mut();
        unplaced.retain(|o| e.is_alive(*o));
        if unplaced.is_empty() {
            unplaced.shrink_to(UNPLACED_KEPT);
        }
    }
    c.by_view
        .borrow()
        .iter()
        .filter(|(view, _)| e.is_alive(**view))
        .flat_map(|(_, rows)| rows.iter().copied())
        .chain(c.unplaced.borrow().iter().copied())
        .collect()
}
