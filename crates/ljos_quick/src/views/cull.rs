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
//! A row the incubator makes over later frames is a child of the view's
//! content from when it is allocated, at its own place (0, 0) until a
//! pass lays it out. Until then it is not drawn either: qml6 shows the
//! list in order from its first frame, never a row over the first.
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
    /// The rows views allocated and have not placed yet.
    unplaced: RefCell<FxHashSet<ObjectId>>,
    /// The delegates in the pools of all views.
    pooled: RefCell<FxHashSet<ObjectId>>,
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

/// A view allocated `row`, which it has not placed yet.
pub(crate) fn allocated(e: &Engine, row: ObjectId) {
    let c = e.extension::<Culled>();
    if c.unplaced.borrow_mut().insert(row) {
        c.generation.set(c.generation.get() + 1);
    }
}

/// A view placed `rows`, or (one row) let it go.
pub(crate) fn placed(e: &Engine, rows: impl IntoIterator<Item = ObjectId>) {
    let c = e.extension::<Culled>();
    let mut unplaced = c.unplaced.borrow_mut();
    if unplaced.is_empty() {
        return;
    }
    let before = unplaced.len();
    for row in rows {
        unplaced.remove(&row);
    }
    if unplaced.len() != before {
        c.generation.set(c.generation.get() + 1);
    }
    // It holds only the rows in flight; a pass that made a thousand at
    // once would otherwise keep their room for good.
    if unplaced.is_empty() && unplaced.capacity() > 64 {
        *unplaced = FxHashSet::default();
    }
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

/// Every row a view made and does not show, placed or not.
pub(crate) fn all(e: &Engine) -> FxHashSet<ObjectId> {
    let c = e.extension::<Culled>();
    c.unplaced.borrow_mut().retain(|o| e.is_alive(*o));
    c.by_view
        .borrow()
        .iter()
        .filter(|(view, _)| e.is_alive(**view))
        .flat_map(|(_, rows)| rows.iter().copied())
        .chain(c.unplaced.borrow().iter().copied())
        .collect()
}
