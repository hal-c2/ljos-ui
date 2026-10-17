//! Which rows have a delegate and where each goes.
//!
//! With `n` rows and `k` places on the path (`pathItemCount`, or every
//! row), row `i` is at place `(i + offset + shift) mod n`, where the shift
//! brings the current row to `preferredHighlightBegin` unless the view has
//! no highlight range. Place `p` is the share `p / k` of the path; a row at
//! place `k` or beyond is off the path and has a delegate only when it is
//! the current row or within the cache, and then sits at the path's end
//! (qml6). Each delegate is centred on its point.

use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ModelChange, ObjectId, Urgency, Value, ViewHost};

use super::track::Track;
use super::{
    Frame, PathView, State, attached_of, enrol, highlight_range, put, range_mode, rd, state_of,
    strike, tell,
};
use crate::item::{Item, is_item, write_placed};
use crate::tree;
use crate::views::warn;

/// What the engine's delegates call on a view.
pub(crate) struct Host(pub ObjectId);

impl ViewHost for Host {
    fn allocated(&self, e: &Engine, _: usize, object: ObjectId) {
        let view = self.0;
        if !is_item(e, object) {
            e.log(
                LogLevel::Warning,
                "qml",
                "QML PathView: Delegate must be of Item type".to_owned(),
            );
            return;
        }
        enrol(e, object, view);
        e.batch(|| {
            warn(e, "parent", tree::set_parent_item(e, object, Some(view)));
            if let Some(att) = attached_of(e, object)
                && let Some(st) = state_of(e, view)
                && let Some(row) = st.delegates().and_then(|d| d.index_of(object))
            {
                tell(e, view, &st, row, att);
            }
        });
    }

    fn destroying(&self, e: &Engine, object: ObjectId) {
        strike(e, object);
        if e.is_alive(object) && is_item(e, object) {
            let _ = tree::set_parent_item(e, object, None);
        }
    }

    fn changed(&self, e: &Engine, change: &ModelChange, gone: &[(usize, ObjectId)]) {
        let view = self.0;
        let Some(st) = state_of(e, view) else {
            return;
        };
        let Some(d) = st.delegates() else {
            return;
        };
        for (_, obj) in gone {
            d.release(e, *obj, false);
        }
        if matches!(
            change,
            ModelChange::RowsAboutToBeInserted { .. }
                | ModelChange::RowsAboutToBeRemoved { .. }
                | ModelChange::RolesChanged
        ) {
            return;
        }
        super::motion::rows_changed(e, view, &st, d.count());
        super::bump(e, &st);
    }
}

/// Gives every delegate back, before the rows they show go.
pub(crate) fn release_all(e: &Engine, _view: ObjectId, st: &State) {
    if let Some(d) = st.delegates() {
        for (_, obj) in d.live() {
            d.release(e, obj, false);
        }
    }
}

/// What the layout follows. Tracked.
pub(crate) fn read_inputs(e: &Engine, view: ObjectId, st: &State) {
    for p in [
        PathView::OFFSET.index,
        PathView::CURRENT_INDEX.index,
        PathView::PATH_ITEM_COUNT.index,
        PathView::HIGHLIGHT_RANGE_MODE.index,
        PathView::PREFERRED_HIGHLIGHT_BEGIN.index,
        PathView::PREFERRED_HIGHLIGHT_END.index,
        PathView::CACHE_ITEM_COUNT.index,
        PathView::HIGHLIGHT.index,
    ] {
        let _ = e.read(view, p);
    }
    let path = e.get(view, PathView::PATH).ok().flatten();
    let track = path.and_then(|p| Track::of(e, p));
    let mut have = st.track.borrow_mut();
    // The same track again keeps the one there is, so that nothing that
    // compares them sees a change.
    if have.as_deref() != track.as_ref() {
        *have = track.map(Rc::new);
    }
    drop(have);
    if let Some(n) = st.trigger.get() {
        let _ = e.graph().get(n);
    }
}

/// Reads, tracked, the sizes the delegates were centred by.
pub(crate) fn read_sizes(e: &Engine, _view: ObjectId, st: &State) {
    let Some(d) = st.delegates() else {
        return;
    };
    let mut items: Vec<ObjectId> = d.live().into_iter().map(|l| l.1).collect();
    items.extend(st.highlight.get().map(|h| h.1));
    for o in items {
        if e.is_alive(o) {
            let _ = e.read(o, Item::WIDTH.index);
            let _ = e.read(o, Item::HEIGHT.index);
        }
    }
}

/// The frame of the view as it is now.
pub(crate) fn frame_of(e: &Engine, view: ObjectId, st: &State, n: usize) -> Option<Frame> {
    let track = st.track.borrow().clone()?;
    if n == 0 {
        return None;
    }
    let count = rd(e, view, PathView::PATH_ITEM_COUNT.index) as i64;
    let k = if count < 0 {
        n
    } else {
        (count as usize).min(n)
    };
    let begin = rd(e, view, PathView::PREFERRED_HIGHLIGHT_BEGIN.index);
    let offset = rd(e, view, PathView::OFFSET.index);
    let shift = if range_mode(e, view) == highlight_range::NONE {
        offset
    } else {
        offset + begin * k as f64
    };
    let current = rd(e, view, PathView::CURRENT_INDEX.index) as i64;
    let current = usize::try_from(current).ok().filter(|c| *c < n);
    let cache = rd(e, view, PathView::CACHE_ITEM_COUNT.index).max(0.0) as usize;
    Some(Frame {
        n,
        k,
        shift,
        current,
        track,
        cache,
    })
}

/// The rows that are to have a delegate, in order.
fn wanted(f: &Frame) -> Vec<usize> {
    let mut rows: Vec<usize> = if f.k.saturating_add(f.cache) >= f.n {
        (0..f.n).filter(|i| f.wanted(*i)).collect()
    } else {
        // The first place is taken by the row that comes to it next; the
        // rows just around the `k` after it, and the cache either side,
        // are looked at, so that a sum that comes out just under a whole
        // number loses none.
        let n = f.n as i64;
        let first = ((f.n as f64 - f.shift).ceil() as i64).rem_euclid(n);
        let reach = f.cache.div_ceil(2) as i64 + 1;
        (-reach..=f.k as i64 + reach)
            .map(|d| (first + d).rem_euclid(n) as usize)
            .filter(|i| f.wanted(*i))
            .collect()
    };
    if let Some(c) = f.current
        && !rows.contains(&c)
    {
        rows.push(c);
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// Lays the rows out now.
pub(crate) fn pass(e: &Engine, view: ObjectId, st: &State) {
    let Some(d) = st.delegates() else {
        return;
    };
    let n = d.count();
    put(e, view, PathView::COUNT.index, Value::Int(n as i32));
    let frame = frame_of(e, view, st, n);
    *st.frame.borrow_mut() = frame.clone();
    let rows = frame.as_ref().map(wanted).unwrap_or_default();
    for (i, obj) in d.live() {
        if !rows.contains(&i) {
            d.release(e, obj, false);
        }
    }
    let mut current_item = None;
    if let Some(f) = &frame {
        for &i in &rows {
            let Some((obj, _)) = d.request(e, i, Urgency::Now) else {
                continue;
            };
            if !is_item(e, obj) {
                continue;
            }
            place(e, f, obj, i);
            if let Some(att) = attached_of(e, obj) {
                tell(e, view, st, i, att);
            }
            if f.current == Some(i) {
                current_item = Some(obj);
            }
        }
    }
    let was = e
        .untrack(|| e.get(view, PathView::CURRENT_ITEM))
        .ok()
        .flatten();
    if was != current_item {
        warn(
            e,
            "currentItem",
            e.set_internal(view, PathView::CURRENT_ITEM, current_item),
        );
        if let Some(item) = current_item {
            crate::views::current::give_focus(e, item);
        }
    }
    highlight(e, view, st, frame.as_ref());
}

/// Centres `obj` on its row's point.
fn place(e: &Engine, f: &Frame, obj: ObjectId, row: usize) {
    let at = f.track.point(f.len_of(row));
    let w = rd(e, obj, Item::WIDTH.index);
    let h = rd(e, obj, Item::HEIGHT.index);
    warn(
        e,
        "x",
        write_placed(e, obj, Item::X.index, Value::Real(at.x - w / 2.0)),
    );
    warn(
        e,
        "y",
        write_placed(e, obj, Item::Y.index, Value::Real(at.y - h / 2.0)),
    );
}

/// The highlight is made of its component and goes where the current row
/// is.
fn highlight(e: &Engine, view: ObjectId, st: &State, f: Option<&Frame>) {
    let component = e
        .untrack(|| e.get(view, PathView::HIGHLIGHT))
        .ok()
        .flatten();
    let have = st.highlight.get();
    let item = match (have, component) {
        (Some((c, item)), Some(want)) if c == want && e.is_alive(item) => Some(item),
        _ => {
            if let Some((_, old)) = st.highlight.take()
                && e.is_alive(old)
            {
                e.announce_destruction(old);
                let _ = tree::set_parent_item(e, old, None);
                e.destroy(old);
            }
            component.and_then(|c| {
                let made = e.create_in_context(c, Some(view), &[], |e, root| {
                    if is_item(e, root) {
                        warn(e, "parent", tree::set_parent_item(e, root, Some(view)));
                    }
                });
                match made {
                    Ok(o) if is_item(e, o) => {
                        st.highlight.set(Some((c, o)));
                        Some(o)
                    }
                    Ok(o) => {
                        e.destroy(o);
                        None
                    }
                    Err(err) => {
                        warn(e, "highlight", Err(err));
                        None
                    }
                }
            })
        }
    };
    let was = e
        .untrack(|| e.get(view, PathView::HIGHLIGHT_ITEM))
        .ok()
        .flatten();
    if was != item {
        warn(
            e,
            "highlightItem",
            e.set_internal(view, PathView::HIGHLIGHT_ITEM, item),
        );
    }
    if let (Some(item), Some(f), Some(row)) = (item, f, f.and_then(|f| f.current)) {
        place(e, f, item, row);
    }
}

/// The row whose delegate is at `x`, `y` of the view, and the delegate.
pub(crate) fn hit(e: &Engine, view: ObjectId, x: f64, y: f64) -> Option<(usize, ObjectId)> {
    let st = state_of(e, view)?;
    let mut live = st.delegates()?.live();
    live.sort_unstable();
    live.into_iter().find(|(_, o)| contains(e, *o, x, y))
}

/// Whether a delegate covers a point of its view: its left and top edges
/// do, its right and bottom ones do not (qml6).
pub(crate) fn contains(e: &Engine, obj: ObjectId, x: f64, y: f64) -> bool {
    if !e.is_alive(obj) {
        return false;
    }
    let ix = rd(e, obj, Item::X.index);
    let iy = rd(e, obj, Item::Y.index);
    let w = rd(e, obj, Item::WIDTH.index);
    let h = rd(e, obj, Item::HEIGHT.index);
    x >= ix && x < ix + w && y >= iy && y < iy + h
}
