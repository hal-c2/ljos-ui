//! Selecting cells with the pointer for a [`CellSelector`], which is how
//! a `SelectionRectangle` (QtQuick.Controls) selects in a table view, as
//! qml6 does.
//!
//! By drag, the press is the view's alone (it does not flick), and the
//! first move beyond the drag threshold selects the block from the cell
//! pressed to the cell under the pointer, and every move after it the
//! block to where the pointer is; a press that does not move is a tap.
//! By press and hold, the view flicks as it would, and a press held for
//! the system's press-and-hold interval without moving selects the cell
//! pressed. A press on one of the selector's handles drags that corner
//! of the block, the other corner staying. The cell the pointer selects
//! to is current. The selector is told every block it gets, and whether
//! the pointer is dragging.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ObjectId, Timer, Value};
use ljos_render::kurbo;

use super::{TableState, TableView, index_of_cell, layout, nav, selection_behavior, state_of};
use crate::DRAG_THRESHOLD;
use crate::flickable::{Flickable, keep_pointer_grab};
use crate::item::Item;

/// `SelectionRectangle.SelectionMode`.
pub mod select_by {
    pub const DRAG: i32 = 0;
    pub const PRESS_AND_HOLD: i32 = 1;
    /// Press and hold on a view that flicks, drag on one that does not.
    pub const AUTO: i32 = 2;
}

/// Cells from `top`, `left` to `bottom`, `right`, all included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellBlock {
    pub top: usize,
    pub left: usize,
    pub bottom: usize,
    pub right: usize,
}

impl CellBlock {
    fn spanning(a: (usize, usize), b: (usize, usize)) -> Self {
        Self {
            top: a.0.min(b.0),
            left: a.1.min(b.1),
            bottom: a.0.max(b.0),
            right: a.1.max(b.1),
        }
    }
}

/// What selects the cells of a table view with the pointer.
pub trait CellSelector {
    /// How it selects: one of [`select_by`].
    fn mode(&self, e: &Engine) -> i32;
    /// Its handles as far as it made them: the one at the top-left corner
    /// of its block and the one at the bottom-right, items of the view's
    /// content. A visible one can be dragged.
    fn handles(&self, e: &Engine) -> [Option<ObjectId>; 2];
    /// The view selected `block` for it; `dragging` while the pointer
    /// drags, by `handle` (0 top-left, 1 bottom-right) when it drags one.
    fn selected(&self, e: &Engine, block: CellBlock, dragging: bool, handle: Option<usize>);
    /// The pointer stopped dragging.
    fn dropped(&self, e: &Engine);
}

/// What the press the view holds is to its selector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Grab {
    /// By drag, not yet moved beyond the threshold: pressed at `at` in the
    /// view, on `cell` if any.
    Pressed {
        at: kurbo::Point,
        cell: Option<(usize, usize)>,
    },
    /// Dragging from `anchor`, the corner that stays; by `handle` if one.
    Dragging {
        anchor: (usize, usize),
        handle: Option<usize>,
    },
    /// Waiting for the press to be held long enough, on `cell`.
    Holding {
        at: kurbo::Point,
        cell: (usize, usize),
    },
    /// The drag could not select; it does nothing more.
    Refused,
}

/// What a view keeps for its selector.
#[derive(Default)]
pub(crate) struct SelectState {
    pub selector: std::cell::RefCell<Option<Rc<dyn CellSelector>>>,
    pub grab: Cell<Option<Grab>>,
    /// The block last selected for the selector.
    pub block: Cell<Option<CellBlock>>,
    /// The timer of a press and hold, made when first needed.
    hold: Cell<Option<ObjectId>>,
}

/// Makes `selector` select the cells of `view` with the pointer, in
/// place of the one it had; none: the view has none.
pub fn set_cell_selector(e: &Engine, view: ObjectId, selector: Option<Rc<dyn CellSelector>>) {
    if let Some(st) = state_of(e, view) {
        st.select.grab.set(None);
        st.select.block.set(None);
        *st.select.selector.borrow_mut() = selector;
    }
}

/// Where the cells of `block` are in the content of `view`, as last laid
/// out; tracked, so a binding that asks moves with the cells.
pub fn cell_block_rect(e: &Engine, view: ObjectId, block: CellBlock) -> Option<kurbo::Rect> {
    let st = state_of(e, view)?;
    if let Some(n) = st.laid.get() {
        let _ = e.graph().get(n);
    }
    let rows = st.row_y.borrow();
    let heights = st.heights.borrow();
    let columns = st.column_x.borrow();
    let y0 = *rows.get(block.top)?;
    let y1 = rows.get(block.bottom)? + heights.get(block.bottom)?;
    let x0 = columns.get(block.left)?.0;
    let (x, w) = *columns.get(block.right)?;
    Some(kurbo::Rect::new(x0, y0, x + w, y1))
}

fn selector(st: &TableState) -> Option<Rc<dyn CellSelector>> {
    st.select.selector.borrow().clone()
}

fn content_at(e: &Engine, view: ObjectId, at: kurbo::Point) -> kurbo::Point {
    let (x, y) = e.untrack(|| {
        (
            e.read(view, Flickable::CONTENT_X.index).to_number(),
            e.read(view, Flickable::CONTENT_Y.index).to_number(),
        )
    });
    kurbo::Point::new(at.x + x, at.y + y)
}

/// The cell nearest a point of the content: the one under it, or the
/// first or last row or column it is beyond.
fn nearest_cell(st: &TableState, p: kurbo::Point) -> Option<(usize, usize)> {
    let rows = st.row_y.borrow();
    let heights = st.heights.borrow();
    let columns = st.column_x.borrow();
    if rows.is_empty() || columns.is_empty() {
        return None;
    }
    let row = rows
        .iter()
        .enumerate()
        .position(|(r, top)| p.y < top + heights.get(r).copied().unwrap_or(0.0))
        .unwrap_or(rows.len() - 1);
    let column = columns
        .iter()
        .position(|(left, wide)| p.x < left + wide)
        .unwrap_or(columns.len() - 1);
    Some((row, column))
}

/// The selector's handle under a point of the content.
fn handle_at(e: &Engine, s: &dyn CellSelector, p: kurbo::Point) -> Option<usize> {
    let handles = s.handles(e);
    e.untrack(|| {
        // The bottom-right one is above the other where they meet.
        (0..2).rev().find(|i| {
            handles[*i].is_some_and(|h| {
                let n = |p| e.read(h, p).to_number();
                e.is_alive(h)
                    && e.read(h, Item::VISIBLE.index).to_boolean()
                    && p.x >= n(Item::X.index)
                    && p.x < n(Item::X.index) + n(Item::WIDTH.index)
                    && p.y >= n(Item::Y.index)
                    && p.y < n(Item::Y.index) + n(Item::HEIGHT.index)
            })
        })
    })
}

fn by_drag(e: &Engine, view: ObjectId, s: &dyn CellSelector) -> bool {
    match s.mode(e) {
        select_by::DRAG => true,
        select_by::PRESS_AND_HOLD => false,
        _ => !e.untrack(|| e.read(view, Flickable::INTERACTIVE.index).to_boolean()),
    }
}

/// A left press at `at` in the view. Some: the selector has it, and
/// whether the view takes it; none: the view handles it as its own,
/// which by press and hold it also does.
pub(crate) fn press(e: &Engine, view: ObjectId, st: &TableState, at: kurbo::Point) -> Option<bool> {
    let s = selector(st)?;
    st.select.grab.set(None);
    let p = content_at(e, view, at);
    if let Some(handle) = handle_at(e, &*s, p)
        && let Some(block) = st.select.block.get()
    {
        let anchor = if handle == 0 {
            (block.bottom, block.right)
        } else {
            (block.top, block.left)
        };
        st.select.grab.set(Some(Grab::Dragging {
            anchor,
            handle: Some(handle),
        }));
        keep_pointer_grab(e, view, true);
        s.selected(e, block, true, Some(handle));
        return Some(true);
    }
    if by_drag(e, view, &*s) {
        st.select.grab.set(Some(Grab::Pressed {
            at,
            cell: layout::cell_at(st, p.x, p.y),
        }));
        // The view's own: no Flickable around it drags it.
        keep_pointer_grab(e, view, true);
        return None;
    }
    if let Some(cell) = layout::cell_at(st, p.x, p.y) {
        st.select.grab.set(Some(Grab::Holding { at, cell }));
        arm(e, view, st);
    }
    None
}

/// Whether the press the view holds is the selector's alone, which the
/// content does not follow.
pub(crate) fn holds(st: &TableState) -> bool {
    matches!(
        st.select.grab.get(),
        Some(Grab::Pressed { .. } | Grab::Dragging { .. } | Grab::Refused)
    )
}

/// The pointer moved to `at` in the view, pressed.
pub(crate) fn moved(e: &Engine, view: ObjectId, st: &TableState, at: kurbo::Point) {
    let Some(grab) = st.select.grab.get() else {
        return;
    };
    match grab {
        Grab::Pressed { at: from, cell } => {
            if (at - from).hypot() <= DRAG_THRESHOLD {
                return;
            }
            // A drag is no tap.
            st.press.set(None);
            let Some(cell) = cell else {
                st.select.grab.set(Some(Grab::Refused));
                return;
            };
            if !can_start(e, view) {
                st.select.grab.set(Some(Grab::Refused));
                return;
            }
            st.select.grab.set(Some(Grab::Dragging {
                anchor: cell,
                handle: None,
            }));
            drag_to(e, view, st, cell, None, at);
        }
        Grab::Dragging { anchor, handle } => drag_to(e, view, st, anchor, handle, at),
        Grab::Holding { at: from, .. } => {
            if (at - from).hypot() > DRAG_THRESHOLD {
                st.select.grab.set(None);
                stop(e, st);
            }
        }
        Grab::Refused => {}
    }
}

fn drag_to(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    anchor: (usize, usize),
    handle: Option<usize>,
    at: kurbo::Point,
) {
    let p = content_at(e, view, at);
    if let Some(to) = nearest_cell(st, p) {
        select(e, view, st, anchor, to, true, handle);
    }
}

/// The press ended. Returns whether it was the selector's, which is no
/// tap.
pub(crate) fn release(e: &Engine, view: ObjectId, st: &TableState) -> bool {
    stop(e, st);
    keep_pointer_grab(e, view, false);
    let grab = st.select.grab.take();
    if let Some(Grab::Dragging { .. }) = grab
        && let Some(s) = selector(st)
    {
        s.dropped(e);
    }
    matches!(grab, Some(Grab::Dragging { .. } | Grab::Refused))
}

/// Whether a selection can start, saying why not as qml6 does.
fn can_start(e: &Engine, view: ObjectId) -> bool {
    let (model, behaviour) = e.untrack(|| {
        (
            e.read(view, TableView::SELECTION_MODEL.index).as_object(),
            e.read(view, TableView::SELECTION_BEHAVIOR.index)
                .to_number() as i32,
        )
    });
    let why = if model.is_none() {
        "no SelectionModel assigned!"
    } else if behaviour == selection_behavior::SELECTION_DISABLED {
        "TableView.selectionBehavior == TableView.SelectionDisabled"
    } else {
        return true;
    };
    e.log(
        LogLevel::Warning,
        "qml",
        format!("QML TableView: Cannot start selection: {why}"),
    );
    false
}

/// Selects from `anchor` to `to` for the selector and makes the cell
/// it ends on current.
fn select(
    e: &Engine,
    view: ObjectId,
    st: &TableState,
    anchor: (usize, usize),
    to: (usize, usize),
    dragging: bool,
    handle: Option<usize>,
) {
    let Some(s) = selector(st) else {
        return;
    };
    let Some(end) = nav::select_block(e, view, st, anchor, to) else {
        return;
    };
    let index = index_of_cell(e, st, end.0, end.1);
    if let Some(sm) = e.untrack(|| e.read(view, TableView::SELECTION_MODEL.index).as_object()) {
        let r = e.invoke_by_name(sm, "setCurrentIndex", &[index, Value::Int(0)]);
        super::warn(e, "setCurrentIndex", r.map(drop));
    }
    let behaviour = e.untrack(|| {
        e.read(view, TableView::SELECTION_BEHAVIOR.index)
            .to_number() as i32
    });
    let mut block = CellBlock::spanning(anchor, end);
    let last_row = st.cells.line_count().saturating_sub(1);
    let last_column = st.cells.columns().saturating_sub(1);
    match behaviour {
        selection_behavior::SELECT_ROWS => (block.left, block.right) = (0, last_column),
        selection_behavior::SELECT_COLUMNS => (block.top, block.bottom) = (0, last_row),
        _ => {}
    }
    st.select.block.set(Some(block));
    s.selected(e, block, dragging, handle);
}

// ------------------------------------------------------- press and hold

fn arm(e: &Engine, view: ObjectId, st: &TableState) {
    let ms = crate::system_prefs::press_and_hold_interval(e);
    let timer = match st.select.hold.get().filter(|t| e.is_alive(*t)) {
        Some(t) => Ok(t),
        None => e
            .create_native_incomplete(Timer::default(), Some(view))
            .and_then(|t| {
                e.complete(t)?;
                e.connect_by_name(t, "triggered", move |e, _| held(e, view))?;
                st.select.hold.set(Some(t));
                Ok(t)
            }),
    };
    let r = timer.and_then(|t| {
        e.set(t, Timer::INTERVAL, ms.max(1))?;
        e.invoke_by_name(t, "restart", &[]).map(drop)
    });
    super::warn(e, "press and hold", r);
}

fn stop(e: &Engine, st: &TableState) {
    if let Some(t) = st.select.hold.get().filter(|t| e.is_alive(*t)) {
        let _ = e.set(t, Timer::RUNNING, false);
    }
}

/// The press was held long enough: its cell is selected, and it is no
/// tap.
fn held(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    let Some(Grab::Holding { cell, .. }) = st.select.grab.get() else {
        return;
    };
    st.select.grab.set(None);
    st.press.set(None);
    if can_start(e, view) {
        select(e, view, &st, cell, cell, false, None);
    }
}
