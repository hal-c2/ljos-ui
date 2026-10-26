//! What a view does when the engine tells it something about its rows: a
//! delegate was allocated or is ready, rows came, went or moved.
//!
//! The view keeps what the user sees where it is. The anchor of the
//! layout is a row and a place; rows that arrive before that row push its
//! number up and leave its place alone, so the content grows upwards (the
//! origin moves) and nothing in sight jumps. qml6 does the same.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ModelChange, ObjectId, Value, ViewHost};

use super::state::{OpKind, PendingOp, SharedTargets, Targets, ViewState};
use super::{
    ItemView, Part, ViewAttached, attached_of, enrol, pass, put, stack, state_of, strike,
    type_name, warn,
};
use crate::animation::{Transition, run_transition, stop_transition};
use crate::flickable;
use crate::item::{is_a, is_item, write_computed};
use crate::tree;

/// How many rows that arrived are remembered until the next layout tells
/// those that got a delegate: a model that grows by a million rows has
/// none of them in sight but the first few.
const MAX_ADDED: usize = 4096;

/// The views told of delegates that are ready while the scene runs its
/// creation jobs: each lays its rows out once when the jobs stop, not
/// once a delegate.
#[derive(Default)]
struct Incubating {
    open: Cell<bool>,
    due: RefCell<Vec<ObjectId>>,
}

/// The scene starts running creation jobs.
pub(crate) fn incubation_begins(e: &Engine) {
    e.extension::<Incubating>().open.set(true);
}

/// The scene stops running creation jobs: the views whose delegates were
/// made lay their rows out, before the frame shows them.
pub(crate) fn incubation_ends(e: &Engine) {
    let inc = e.extension::<Incubating>();
    inc.open.set(false);
    let due = std::mem::take(&mut *inc.due.borrow_mut());
    for view in due {
        pass::refresh(e, view);
    }
}

/// The views that laid their rows out while the scene was laying its
/// items out, and so did not take the lengths of delegates still being
/// laid out (see `Fill::lay_row`).
#[derive(Default)]
struct Polishing(RefCell<Vec<ObjectId>>);

pub(crate) fn polished_later(e: &Engine, view: ObjectId) {
    let due = e.extension::<Polishing>();
    let mut due = due.0.borrow_mut();
    if !due.contains(&view) {
        due.push(view);
    }
}

/// The views whose model changed since they last laid their rows out.
#[derive(Default)]
struct Changed(RefCell<Vec<ObjectId>>);

/// The model of `view` changed: its rows are laid out when the scene is
/// next polished, once for all that changes until then, as in qml6.
pub(crate) fn lay_out_later(e: &Engine, view: ObjectId, st: &ViewState) {
    if st.due.replace(true) {
        return;
    }
    e.extension::<Changed>().0.borrow_mut().push(view);
    e.request_wake();
}

/// The scene has laid its items out: the views that waited for that lay
/// their rows out with the lengths the delegates have now, and those whose
/// model changed lay out what it has now. Whether one did.
pub(crate) fn polish_ends(e: &Engine) -> bool {
    let changed = std::mem::take(&mut *e.extension::<Changed>().0.borrow_mut());
    let mut due = std::mem::take(&mut *e.extension::<Polishing>().0.borrow_mut());
    for view in changed {
        // Laid out since by a method that needed the rows at once.
        if !due.contains(&view) && state_of(e, view).is_some_and(|st| st.due.get()) {
            due.push(view);
        }
    }
    let mut laid = false;
    for view in due {
        if e.is_alive(view) {
            pass::refresh(e, view);
            laid = true;
        }
    }
    laid
}

/// What the engine's delegates call on a view.
pub(crate) struct Host(pub ObjectId);

impl ViewHost for Host {
    fn allocated(&self, e: &Engine, _: usize, object: ObjectId) {
        let view = self.0;
        if !is_item(e, object) {
            e.log(
                LogLevel::Warning,
                "qml",
                format!(
                    "QML {}: Delegate must be of Item type",
                    e.warning_type_name(view, type_name(e, view))
                ),
            );
            return;
        }
        let Some(content) = flickable::content_item(e, view) else {
            return;
        };
        enrol(e, object, view, Part::Row);
        // Before the delegate's bindings run, so `parent` and
        // `ListView.view` read right in them.
        e.batch(|| {
            warn(e, "parent", tree::set_parent_item(e, object, Some(content)));
            stack(e, object, 1.0);
            // An item that was there before the view took it (a row of an
            // `ObjectModel`) may have asked for its view already.
            if let Some(att) = attached_of(e, view, object) {
                let v = Value::Object(view);
                warn(
                    e,
                    "attach",
                    write_computed(e, att, ViewAttached::VIEW.index, v),
                );
            }
        });
    }

    fn ready(&self, e: &Engine, _: usize, object: ObjectId) {
        // A row the incubator made has not been laid out either: it is
        // measured after a polish. It is not waited for, as rows after it
        // may be being made already.
        if let Some(st) = state_of(e, self.0) {
            st.fresh
                .borrow_mut()
                .insert(object, crate::scene::polishes());
        }
        let inc = e.extension::<Incubating>();
        if inc.open.get() {
            let mut due = inc.due.borrow_mut();
            if !due.contains(&self.0) {
                due.push(self.0);
            }
        } else {
            pass::refresh(e, self.0);
        }
    }

    fn destroying(&self, e: &Engine, object: ObjectId) {
        let view = self.0;
        strike(e, object);
        // Out of the tree now; the object itself goes at the next frame.
        if e.is_alive(object) && is_item(e, object) {
            let _ = tree::set_parent_item(e, object, None);
        }
        let Some(st) = state_of(e, view) else {
            return;
        };
        st.leaving.borrow_mut().retain(|o| *o != object);
        st.removing.borrow_mut().retain(|r| r.0 != object);
        if let Some((_, told)) = st.removed.borrow_mut().as_mut() {
            told.retain(|r| r.0 != object);
        }
        if st.hidden.borrow_mut().remove(&object) {
            super::cull::pool(e, object, false);
        }
        if st.current.get() == Some(object) {
            st.current.set(None);
        }
        let stopped: Vec<ObjectId> = {
            let mut running = st.running.borrow_mut();
            let of_it = running.iter().filter(|r| r.1 == object).map(|r| r.0);
            let of_it: Vec<ObjectId> = of_it.collect();
            running.retain(|r| r.1 != object);
            of_it
        };
        for t in stopped {
            stop_transition(e, t);
        }
        super::view_transition::forget(e, view, object);
    }

    fn changed(&self, e: &Engine, change: &ModelChange, gone: &[(usize, ObjectId)]) {
        let view = self.0;
        let Some(st) = state_of(e, view) else {
            return;
        };
        let Some(d) = st.delegates() else {
            return;
        };
        // Before the first layout there is nothing to keep in place.
        let laid_out = st.seen_index.get().is_some();
        let current = pass::current_row(e, view, &st);
        let rows = d.count() as i64;
        let (line, top) = st.anchor.get();
        match *change {
            // Rows are about to change, or the roles did on the way to a
            // change of rows: the notice of that change is what the view
            // acts on, when its rows and the model's are the same again.
            ModelChange::RowsAboutToBeInserted { .. } => {
                return lay_out_other_change(e, view, &st, OpKind::Add);
            }
            ModelChange::RowsAboutToBeRemoved { .. } => {
                return lay_out_other_change(e, view, &st, OpKind::Remove);
            }
            ModelChange::RolesChanged => return,
            ModelChange::RowsInserted { first, count } => {
                // A row that arrives exactly where the view starts is the
                // first one shown; rows before that push the view's rows
                // on without moving them (both observed on qml6).
                if !st.grid.get() && first < line {
                    st.anchor.set((line + count, top));
                }
                // Rows that arrive in a model with none move no current
                // row: there was none, whatever number it was given. One
                // the view would choose itself is the first of them, and
                // rows that come before it later move it (qml6).
                if laid_out && rows == count as i64 {
                    if current < 0 && !st.cleared.get() {
                        st.auto_index.set(true);
                        move_current_later(&st, 0);
                    }
                } else if laid_out && current >= first as i64 {
                    move_current_later(&st, current + count as i64);
                }
                let mut keys = Vec::new();
                let mut added = st.added.borrow_mut();
                for row in first..first.saturating_add(count) {
                    if added.len() >= MAX_ADDED {
                        break;
                    }
                    if let Some(k) = d.key_at(row) {
                        added.insert(k);
                        keys.push(k);
                    }
                }
                drop(added);
                let indexes = (first..first.saturating_add(count)).map(|i| i as i32);
                let last = first.saturating_add(count) as i32 - 1;
                let mut pending = st.pending.borrow_mut();
                match pending.as_mut().filter(|op| op.kind == OpKind::Add) {
                    // Rows added since the last pass are one change to it:
                    // those that came before are where the new ones put them.
                    Some(op) => {
                        if op.highest >= first as i32 {
                            for i in &mut op.target_indexes {
                                if *i >= first as i32 {
                                    *i += count as i32;
                                }
                            }
                            op.highest += count as i32;
                        }
                        op.highest = op.highest.max(last);
                        op.target_keys.extend(keys);
                        op.target_indexes.extend(indexes);
                    }
                    None => {
                        *pending = Some(PendingOp {
                            kind: OpKind::Add,
                            target_keys: keys.into_iter().collect(),
                            target_indexes: indexes.collect(),
                            highest: last,
                            target_items: Vec::new(),
                            gone_runs: Vec::new(),
                            gone_items: Vec::new(),
                        });
                    }
                }
                drop(pending);
                st.places.borrow_mut().clear();
                stale_from(&st, first);
                untold_count(e, view, &st, rows);
            }
            ModelChange::RowsRemoved { first, count } => {
                let end = first.saturating_add(count);
                if !st.grid.get() {
                    if line >= end {
                        st.anchor.set((line - count, top));
                    } else if line >= first {
                        // The row the view started with is gone: the one
                        // that follows is there instead.
                        st.anchor.set((first, top));
                    }
                }
                if laid_out && current >= end as i64 {
                    move_current_later(&st, current - count as i64);
                } else if laid_out && current >= first as i64 {
                    move_current_later(&st, (first as i64).min(rows - 1));
                }
                let mut pending = st.pending.borrow_mut();
                if !pending.as_ref().is_some_and(|op| op.kind == OpKind::Remove) {
                    *pending = Some(PendingOp {
                        kind: OpKind::Remove,
                        target_keys: Default::default(),
                        target_indexes: Vec::new(),
                        // Only added rows read it.
                        highest: -1,
                        target_items: Vec::new(),
                        gone_runs: Vec::new(),
                        gone_items: Vec::new(),
                    });
                }
                if let Some(op) = pending.as_mut() {
                    removed(op, first, count, gone);
                }
                drop(pending);
                st.places.borrow_mut().clear();
                stale_from(&st, first);
                untold_count(e, view, &st, rows);
            }
            ModelChange::RowsMoved { from, to, count } => {
                let (from, to, count) = (from as i64, to as i64, count as i64);
                if laid_out && current >= 0 {
                    let now = if current >= from && current < from + count {
                        to + (current - from)
                    } else {
                        let without = if current >= from + count {
                            current - count
                        } else {
                            current
                        };
                        if without >= to {
                            without + count
                        } else {
                            without
                        }
                    };
                    set_current(e, view, &st, now);
                }
                *st.pending.borrow_mut() = Some(PendingOp {
                    kind: OpKind::Move,
                    target_keys: (to as usize..(to + count) as usize)
                        .filter_map(|row| d.key_at(row))
                        .collect(),
                    target_indexes: (to..to + count).map(|i| i as i32).collect(),
                    highest: (to + count) as i32 - 1,
                    target_items: Vec::new(),
                    gone_runs: Vec::new(),
                    gone_items: Vec::new(),
                });
                st.places.borrow_mut().clear();
                st.slots_stale.set(true);
            }
            ModelChange::DataChanged { .. } => {
                // The delegates show the new values already. Only what
                // names the sections moves anything.
                if !pass::has_sections(e, view, &st) {
                    return;
                }
            }
            ModelChange::ModelReset => {
                forget(e, view, &st);
                st.slots_stale.set(true);
                untold_count(e, view, &st, rows);
                if laid_out {
                    // The first row is the current one again, unless the
                    // author said there is none.
                    st.auto_index.set(!st.cleared.get());
                    set_current(e, view, &st, -1);
                }
            }
        }
        for &(row, object) in gone {
            leave(e, view, &st, object, row as i32);
        }
        lay_out_later(e, view, &st);
    }
}

/// Rows `first..first + count` left the model, `gone` their delegates,
/// numbered as the model had them just before. Rows removed since the
/// last pass are one change to it, as in qml6: each row is numbered as
/// the model numbers it once the rows gone before it in the model are
/// gone, a run of rows counting on from where it starts, and the last
/// comes first.
fn removed(op: &mut PendingOp, first: usize, count: usize, gone: &[(usize, ObjectId)]) {
    let (runs, items) = (&mut op.gone_runs, &mut op.gone_items);
    // The row the model numbers `row` now, as it numbered it before
    // the first of these removals.
    let before = |runs: &[(usize, usize)], row: usize| {
        let mut at = row;
        for &(start, n) in runs {
            if start > at {
                break;
            }
            at += n;
        }
        at
    };
    for &(row, object) in gone {
        items.push((before(runs, row), object));
    }
    // The rows gone now lie between runs gone before.
    let mut at = before(runs, first);
    let mut left = count;
    let mut more = Vec::new();
    for &(start, n) in runs.iter() {
        if left == 0 {
            break;
        }
        if start < at {
            continue;
        }
        let here = left.min(start - at);
        more.push((at, here));
        left -= here;
        at = start + n;
    }
    if left > 0 {
        more.push((at, left));
    }
    runs.extend(more);
    runs.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(runs.len());
    for &(start, n) in runs.iter() {
        match merged.last_mut() {
            Some(last) if last.0 + last.1 == start => last.1 += n,
            _ => merged.push((start, n)),
        }
    }
    *runs = merged;
    // A model that loses a million rows has few of them in sight.
    let mut indexes = Vec::new();
    let mut earlier = 0;
    for &(start, n) in runs.iter() {
        let from = start - earlier;
        indexes.extend((from..from + n).take(MAX_ADDED - indexes.len().min(MAX_ADDED)));
        earlier += n;
    }
    op.target_indexes = indexes
        .into_iter()
        .rev()
        .map(|i| i.min(i32::MAX as usize) as i32)
        .collect();
    items.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
    op.target_items = items.iter().map(|(_, o)| *o).collect();
}

/// Rows of another kind are about to come or go while a change of rows
/// waits for the next pass: that one is laid out first, while the model
/// still has the rows it was about, so that each runs its transitions.
fn lay_out_other_change(e: &Engine, view: ObjectId, st: &ViewState, coming: OpKind) {
    let other = st
        .pending
        .borrow()
        .as_ref()
        .is_some_and(|op| op.kind != coming);
    if other && st.due.get() {
        pass::refresh(e, view);
    }
}

/// Rows from `first` on are not where the last pass placed them.
fn stale_from(st: &ViewState, first: usize) {
    if st.slots.borrow().last().is_some_and(|s| s.line >= first) {
        st.slots_stale.set(true);
    }
}

/// `count` is the model's at once, as qml6 reads it, but says it changed
/// only when the rows are laid out: then the delegates are there, and
/// `countChanged` comes after their `Component.onCompleted`, as in qml6.
fn untold_count(e: &Engine, view: ObjectId, st: &ViewState, rows: i64) {
    let n = rows.clamp(0, i64::from(i32::MAX)) as i32;
    let g = e.graph();
    // A binding that read it keeps what it read until it is told; the
    // field is what a read sees in the meantime.
    let quiet = e
        .property_node(view, ItemView::COUNT.index)
        .is_ok_and(|node| g.replace_quietly(node, Value::Int(n)).unwrap_or(false));
    if quiet {
        let _ = e.with_mut(view, |v: &mut ItemView| v.count = n);
        st.count_untold.set(true);
    } else {
        put(e, view, ItemView::COUNT.index, Value::Int(n));
    }
}

/// The current row is `index` now, and `currentIndex` says so when the
/// rows are next laid out.
fn move_current_later(st: &ViewState, index: i64) {
    st.current_due
        .set(Some(index.clamp(-1, i64::from(i32::MAX)) as i32));
}

/// Writes `currentIndex` as the view's own doing: the row is the same,
/// its number is another, and the view does not go anywhere for it.
fn set_current(e: &Engine, view: ObjectId, st: &ViewState, index: i64) {
    let index = index.clamp(-1, i64::from(i32::MAX)) as i32;
    st.current_due.set(None);
    st.seen_index.set(Some(index));
    put(e, view, ItemView::CURRENT_INDEX.index, Value::Int(index));
}

/// A new model or delegate: nothing that was known about the rows holds.
pub(crate) fn forget(e: &Engine, view: ObjectId, st: &ViewState) {
    let _ = (e, view);
    st.forget_rows();
    st.positioned.set(false);
    st.populate.set(true);
}

/// The row of `object` left the model: the delegate is told, and goes
/// when nothing holds it back.
fn leave(e: &Engine, view: ObjectId, st: &ViewState, object: ObjectId, index: i32) {
    let att = attached_of(e, view, object);
    if let Some(att) = att {
        warn(e, "remove", e.emit_by_name(att, "remove", &[]));
    }
    if !e.is_alive(object) {
        return;
    }
    st.leaving.borrow_mut().push(object);
    let held = att
        .filter(|a| e.is_alive(*a))
        .filter(|a| e.untrack(|| e.read(*a, ViewAttached::DELAY_REMOVE.index).to_boolean()));
    match held {
        Some(att) => {
            // `ListView.delayRemove`: the delegate stays until it is false.
            let watching = e.watch(att, ViewAttached::DELAY_REMOVE.index, move |e, v| {
                if !v.to_boolean() {
                    finish(e, view, object, index, None);
                }
            });
            if let Err(err) = watching {
                warn(e, "delayRemove", Err(err));
                finish(e, view, object, index, None);
            }
        }
        // In the layout pass, once for every row that left with it, as
        // qml6 runs it ([`run_removals`]).
        None => st.removing.borrow_mut().push((object, index)),
    }
}

/// The pass has taken the removal `op`, told as `targets`: each delegate
/// that left in it is told the index the change gives its row.
pub(crate) fn told_removal(st: &ViewState, op: &PendingOp, targets: &SharedTargets) {
    let mut told = Vec::new();
    st.removing.borrow_mut().retain(|&(object, _)| {
        let Some(&(row, _)) = op.gone_items.iter().find(|g| g.1 == object) else {
            return true;
        };
        // A run of rows counts on from where it starts, once the runs
        // before it are gone.
        let earlier: usize = (op.gone_runs.iter())
            .take_while(|r| r.0 + r.1 <= row)
            .map(|r| r.1)
            .sum();
        told.push((object, (row - earlier).min(i32::MAX as usize) as i32));
        false
    });
    if !told.is_empty() {
        *st.removed.borrow_mut() = Some((Rc::clone(targets), told));
    }
}

/// Runs `remove` on the delegates whose rows left before the pass that
/// just ended, then gives them back. One whose removal no pass will take
/// (a reset or a move took its place) is told its own row alone.
pub(crate) fn run_removals(e: &Engine, view: ObjectId, st: &ViewState) {
    let told = st.removed.borrow_mut().take();
    if let Some((targets, told)) = told {
        for (object, index) in told {
            finish(e, view, object, index, Some(Rc::clone(&targets)));
        }
    }
    let waits = (st.pending.borrow().as_ref()).is_some_and(|op| op.kind == OpKind::Remove);
    if !waits {
        let untold = std::mem::take(&mut *st.removing.borrow_mut());
        for (object, index) in untold {
            finish(e, view, object, index, None);
        }
    }
}

/// Shows a delegate out the way `remove` says, then gives it back.
/// `index` is the row it had, which the transition is told, with the
/// change's `targets` (that row alone without them).
fn finish(
    e: &Engine,
    view: ObjectId,
    object: ObjectId,
    index: i32,
    targets: Option<SharedTargets>,
) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    {
        // Once: `delayRemove` may go false, true and false again.
        let mut leaving = st.leaving.borrow_mut();
        let Some(i) = leaving.iter().position(|o| *o == object) else {
            return;
        };
        leaving.swap_remove(i);
    }
    let transition = e.untrack(|| e.get(view, ItemView::REMOVE)).ok().flatten();
    let animated = animate_at(e, view, transition, object, index, targets, move |e| {
        discard(e, view, object);
        // At the next polish, not now: a transition that ends at once
        // ends while the model is still telling the view of its rows,
        // and the rows that go together are one change to the pass.
        if let Some(st) = state_of(e, view) {
            lay_out_later(e, view, &st);
        }
    });
    if !animated {
        discard(e, view, object);
    }
}

fn discard(e: &Engine, view: ObjectId, object: ObjectId) {
    if let Some(d) = state_of(e, view).and_then(|st| st.delegates()) {
        d.release(e, object, false);
    }
}

/// Runs a view's transition on one of its items and calls `done` at its
/// end. False, and `done` is not called, when there is no transition to
/// run: none was given, or it is running on another item (a `Transition`
/// runs on one target at a time).
pub(crate) fn animate(
    e: &Engine,
    view: ObjectId,
    transition: Option<ObjectId>,
    item: ObjectId,
    done: impl Fn(&Engine) + 'static,
) -> bool {
    let index = state_of(e, view)
        .and_then(|st| st.delegates())
        .and_then(|d| d.index_of(item))
        .map_or(-1, |i| i as i32);
    animate_at(e, view, transition, item, index, None, done)
}

/// [`animate`] on an item the view numbers `index`, which a delegate
/// whose row is gone no longer tells.
fn animate_at(
    e: &Engine,
    view: ObjectId,
    transition: Option<ObjectId>,
    item: ObjectId,
    index: i32,
    targets: Option<SharedTargets>,
    done: impl Fn(&Engine) + 'static,
) -> bool {
    let Some(t) = transition.filter(|t| e.is_alive(*t) && is_a::<Transition>(e, *t)) else {
        return false;
    };
    let Some(st) = state_of(e, view) else {
        return false;
    };
    if st.running.borrow().iter().any(|r| r.0 == t) {
        return false;
    }
    // This run, too, is a view transition: it is told the item and the
    // place the view has put it in, so that a transition may read its
    // `ViewTransition`, with the rows of the change it runs for.
    {
        let at = [
            pass::rd(e, item, crate::item::Item::X.index),
            pass::rd(e, item, crate::item::Item::Y.index),
        ];
        super::view_transition::tell(
            e,
            t,
            item,
            index,
            at,
            &targets.unwrap_or_else(|| {
                std::rc::Rc::new(Targets {
                    indexes: vec![index],
                    items: vec![item],
                })
            }),
        );
    }
    st.running.borrow_mut().push((t, item));
    run_transition(e, t, item, move |e| {
        if let Some(st) = state_of(e, view) {
            st.running.borrow_mut().retain(|r| r.0 != t);
        }
        done(e);
    });
    true
}
