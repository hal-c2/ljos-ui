//! What a view remembers between layout passes, and the arithmetic that
//! needs no engine.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Made, NodeId, ObjectId, OverrideId, PropIndex, RowKey, ViewDelegates};
use rustc_hash::{FxHashMap, FxHashSet};

/// One line of what a view laid out: a row of a list, or the cells of a
/// grid that share a place along the direction it scrolls in. Positions
/// are logical: they grow in the direction the rows run, whichever way
/// that is on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub line: usize,
    /// Where the row starts, after the section heading before it.
    pub pos: f64,
    pub size: f64,
    /// How long the section heading before the row is; 0 without one.
    pub lead: f64,
    /// False while the size is a guess: the delegate is still being made
    /// or has not been measured yet.
    pub measured: bool,
}

impl Slot {
    pub fn top(&self) -> f64 {
        self.pos - self.lead
    }

    pub fn end(&self) -> f64 {
        self.pos + self.size
    }
}

/// How a view is laid out, read once a pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geo {
    pub grid: bool,
    /// The direction it scrolls in: an index into the Flickable's two.
    pub main: usize,
    /// The rows run against the coordinate (bottom to top, right to left).
    pub reversed: bool,
    /// A grid whose cells run against the coordinate across the lines.
    pub cross_reversed: bool,
    pub spacing: f64,
    /// Cells in a line; 1 for a list.
    pub per_line: usize,
    pub cell_main: f64,
    pub cell_cross: f64,
    pub view_main: f64,
    pub view_cross: f64,
    /// How far outside what is shown delegates are kept.
    pub buffer: f64,
    pub margin_begin: f64,
    pub margin_end: f64,
}

impl Geo {
    pub fn lines(&self, rows: usize) -> usize {
        rows.div_ceil(self.per_line.max(1))
    }

    pub fn line_of(&self, row: usize) -> usize {
        row / self.per_line.max(1)
    }

    /// The content coordinate of a logical span.
    pub fn in_content(&self, pos: f64, size: f64) -> f64 {
        if self.reversed { -(pos + size) } else { pos }
    }

    /// The logical start of a span given in content coordinates.
    pub fn in_logical(&self, at: f64, size: f64) -> f64 {
        if self.reversed { -(at + size) } else { at }
    }

    /// Where the view starts, logically, when the content is at `at`.
    pub fn viewport(&self, at: f64) -> f64 {
        self.in_logical(at, self.view_main)
    }

    /// The content position that puts the logical place `v0` at the
    /// start of the view.
    pub fn content_position(&self, v0: f64) -> f64 {
        self.in_content(v0, self.view_main)
    }

    /// Where a cell is across the lines, in content coordinates.
    pub fn cross_of(&self, column: usize) -> f64 {
        let column = if self.cross_reversed {
            self.per_line.saturating_sub(1).saturating_sub(column)
        } else {
            column
        };
        column as f64 * self.cell_cross
    }
}

/// `positionViewAtIndex` modes.
pub mod position_mode {
    pub const BEGINNING: i32 = 0;
    pub const CENTER: i32 = 1;
    pub const END: i32 = 2;
    pub const VISIBLE: i32 = 3;
    pub const CONTAIN: i32 = 4;
    pub const SNAP_POSITION: i32 = 5;
}

/// Where the view is to start so that the span `pos..pos + size` is where
/// `mode` wants it; `None` when the mode asks for nothing (the row is in
/// sight already). `v0` is where the view starts now.
pub(crate) fn position_target(mode: i32, pos: f64, size: f64, v0: f64, view: f64) -> Option<f64> {
    use position_mode as m;
    let end = pos + size;
    match mode {
        m::CENTER => Some(pos + size / 2.0 - view / 2.0),
        m::END => Some(end - view),
        m::VISIBLE => {
            if end <= v0 {
                Some(pos)
            } else if pos >= v0 + view {
                Some(end - view)
            } else {
                None
            }
        }
        m::CONTAIN => {
            // A row longer than the view shows its start.
            if pos < v0 || size > view {
                Some(pos)
            } else if end > v0 + view {
                Some(end - view)
            } else {
                None
            }
        }
        _ => Some(pos),
    }
}

/// Where a view that follows its highlight has to start. The highlight is
/// on its way to the current row: the view goes with it, but never past
/// the row it is heading for.
pub(crate) fn follow_target(highlight: (f64, f64), current: (f64, f64), v0: f64, view: f64) -> f64 {
    let (hp, he) = (highlight.0, highlight.0 + highlight.1);
    let (cp, ce) = (current.0, current.0 + current.1);
    if hp < v0 {
        if cp > hp { cp.min(v0) } else { hp }
    } else if he > v0 + view {
        (if ce < he { ce.max(v0 + view) } else { he }) - view
    } else {
        v0
    }
}

/// The start of a row nearest to where the view starts: where `snapMode`
/// brings the view to rest. `slots` are in order.
pub(crate) fn snap_target(slots: &[Slot], v0: f64) -> Option<f64> {
    slots
        .iter()
        .map(|s| s.top())
        .min_by(|a, b| (a - v0).abs().total_cmp(&(b - v0).abs()))
}

/// The whole number rows are taken to be long where none was measured:
/// Qt estimates with a rounded average too (observed on qml6).
pub(crate) fn average(slots: &[Slot]) -> Option<f64> {
    let measured: Vec<f64> = slots
        .iter()
        .filter(|s| s.measured)
        .map(|s| s.size)
        .collect();
    if measured.is_empty() {
        return None;
    }
    Some((measured.iter().sum::<f64>() / measured.len() as f64).round())
}

/// An item a view made that is not a delegate, and the component it is
/// of (`None`: the view's own plain item).
pub(crate) type Extra = Option<(Option<ObjectId>, ObjectId)>;

/// What changed in the model since the last pass, for the transitions a
/// pass starts because of it: which rows the change is about, which the
/// rows pushed aside by it are not.
#[derive(Clone, Debug)]
pub(crate) struct PendingOp {
    pub kind: OpKind,
    /// The rows the change is about, by key: the ones that arrived or
    /// moved. Of a removal there are none left to key.
    pub target_keys: FxHashSet<RowKey>,
    /// Their indexes as `ViewTransition.targetIndexes` reads them: where
    /// they are after an insertion or a move, where they were before a
    /// removal.
    pub target_indexes: Vec<i32>,
    /// Their delegates, when there are any: of a removal, the ones that
    /// are leaving; of the rest, the pass fills this in.
    pub target_items: Vec<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpKind {
    Add,
    Remove,
    Move,
}

/// The target rows of the change the transitions of one pass are run
/// for, in the form every one of them is told the same way.
#[derive(Clone, Debug, Default)]
pub(crate) struct Targets {
    pub indexes: Vec<i32>,
    pub items: Vec<ObjectId>,
}

pub(crate) type SharedTargets = Rc<Targets>;

pub(crate) struct ViewState {
    pub grid: Cell<bool>,
    pub delegates: RefCell<Option<ViewDelegates>>,
    /// Bumped to make the layout effect run again.
    pub trigger: Cell<Option<NodeId>>,
    pub in_pass: Cell<bool>,
    /// Something asked for a pass while one was running.
    pub dirty: Cell<bool>,
    /// How many times the rows were laid out.
    pub passes: Cell<u64>,
    /// The first row in sight after the last pass and where it starts:
    /// what stays put when rows come and go before it.
    pub anchor: Cell<(usize, f64)>,
    /// How long each row measured, by model key.
    pub sizes: RefCell<FxHashMap<RowKey, f64>>,
    /// Where each row was last laid out, until the model changes: a row
    /// the view comes back to is where it was.
    pub places: RefCell<FxHashMap<RowKey, f64>>,
    pub avg: Cell<f64>,
    /// The row a pass stopped at to wait for its length, and the polish
    /// count then (see `Fill::wait`).
    pub waited: Cell<Option<(RowKey, u64)>>,
    /// The delegates made or reused since the last polish, with the
    /// polish count then (see [`ViewState::settled`]).
    pub fresh: RefCell<FxHashMap<ObjectId, u64>>,
    pub heading: Cell<f64>,
    pub slots: RefCell<Vec<Slot>>,
    /// The delegates of the last pass with the logical span of each.
    pub placed: RefCell<Vec<(usize, ObjectId, f64, f64)>>,
    /// Where row 0 starts and the last row ends, measured or estimated.
    pub start: Cell<f64>,
    pub end: Cell<f64>,
    pub header: Cell<Extra>,
    pub footer: Cell<Extra>,
    pub highlight: Cell<Extra>,
    /// Section headings by the key of the row each is before.
    pub sections: RefCell<FxHashMap<RowKey, (String, ObjectId)>>,
    pub section_delegate: Cell<Option<ObjectId>>,
    pub section_props: Cell<Option<[PropIndex; 3]>>,
    pub current: Cell<Option<ObjectId>>,
    /// The `currentIndex` the view last wrote or saw: another value is
    /// somebody's assignment.
    pub seen_index: Cell<Option<i32>>,
    /// `currentIndex` was given as -1: rows that arrive select nothing.
    pub cleared: Cell<bool>,
    /// The current row is the view's own choice (the first row, when
    /// nothing was said): it is no row again when the rows are gone.
    pub auto_index: Cell<bool>,
    /// Where the current row is, logically: where the highlight goes.
    pub current_span: Cell<Option<(f64, f64)>>,
    /// The current row changed: the view goes where the highlight goes.
    pub follow: Cell<bool>,
    /// The view has been put at its beginning.
    pub positioned: Cell<bool>,
    pub last_main: Cell<Option<(usize, bool)>>,
    /// The bounds the content last had: a pass that changes them brings
    /// the content inside.
    pub bounds: Cell<(f64, f64, f64)>,
    /// Where the content starts and how long it is by the last pass that
    /// laid rows out at a place the view is not at: whoever moves the
    /// view there writes them with its place.
    pub laid: Cell<Option<(f64, f64)>>,
    /// Rows that arrived since the last pass.
    pub added: RefCell<FxHashSet<RowKey>>,
    /// The first rows of a model are still to be shown.
    pub populate: Cell<bool>,
    /// Delegates of rows that left and are still shown.
    pub leaving: RefCell<Vec<ObjectId>>,
    /// Pooled delegates and what hides each.
    pub hidden: RefCell<FxHashMap<ObjectId, OverrideId>>,
    /// Transitions that are running, and on which item.
    pub running: RefCell<Vec<(ObjectId, ObjectId)>>,
    /// The change of rows the last word from the model was about, until
    /// the pass that follows it has run its transitions.
    pub pending: RefCell<Option<PendingOp>>,
    /// Where each row's delegate was put by the last pass, in content
    /// coordinates by the row's key: what a row pushed aside moves from.
    pub last_places: RefCell<FxHashMap<RowKey, (ObjectId, [f64; 2])>>,
    /// The delegates a move or displaced transition is on the way to
    /// their place with, and where that place is per axis: a pass does
    /// not put them there before the transition does.
    pub vt_places: RefCell<FxHashMap<ObjectId, [Option<f64>; 2]>>,
    /// How many items each such transition is running on, for its
    /// `running`.
    pub vt_counts: RefCell<FxHashMap<ObjectId, usize>>,
    /// The animations that move the highlight, and where each is going.
    pub movers: [Cell<Option<ObjectId>>; 2],
    pub mover_to: Cell<[Option<f64>; 2]>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            grid: Cell::new(false),
            delegates: RefCell::new(None),
            trigger: Cell::new(None),
            in_pass: Cell::new(false),
            dirty: Cell::new(false),
            passes: Cell::new(0),
            anchor: Cell::new((0, 0.0)),
            sizes: RefCell::default(),
            places: RefCell::default(),
            avg: Cell::new(0.0),
            waited: Cell::new(None),
            fresh: RefCell::default(),
            heading: Cell::new(0.0),
            slots: RefCell::default(),
            placed: RefCell::default(),
            start: Cell::new(0.0),
            end: Cell::new(0.0),
            header: Cell::new(None),
            footer: Cell::new(None),
            highlight: Cell::new(None),
            sections: RefCell::default(),
            section_delegate: Cell::new(None),
            section_props: Cell::new(None),
            current: Cell::new(None),
            seen_index: Cell::new(None),
            cleared: Cell::new(false),
            auto_index: Cell::new(false),
            current_span: Cell::new(None),
            follow: Cell::new(false),
            positioned: Cell::new(false),
            last_main: Cell::new(None),
            bounds: Cell::new((0.0, 0.0, 0.0)),
            laid: Cell::new(None),
            added: RefCell::default(),
            populate: Cell::new(false),
            leaving: RefCell::default(),
            hidden: RefCell::default(),
            running: RefCell::default(),
            pending: RefCell::default(),
            last_places: RefCell::default(),
            vt_places: RefCell::default(),
            vt_counts: RefCell::default(),
            movers: [Cell::new(None), Cell::new(None)],
            mover_to: Cell::new([None; 2]),
        }
    }
}

impl std::fmt::Debug for ViewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewState")
            .field("grid", &self.grid.get())
            .field("anchor", &self.anchor.get())
            .field("slots", &self.slots.borrow().len())
            .finish_non_exhaustive()
    }
}

impl ViewState {
    /// Whether delegate `o`, which a request gave as `made`, has had its
    /// length from what is in it since it was made or took another row:
    /// two polishes ran, as the first may have run before it joined the
    /// scene (see `Fill::wait_for`).
    pub fn settled(&self, o: ObjectId, made: Made) -> bool {
        let now = crate::scene::polishes();
        let mut fresh = self.fresh.borrow_mut();
        if made != Made::Existing {
            fresh.retain(|_, then| *then >= now);
            fresh.insert(o, now);
            return false;
        }
        fresh.get(&o).is_none_or(|then| *then < now)
    }

    pub fn delegates(&self) -> Option<ViewDelegates> {
        self.delegates.borrow().clone()
    }

    /// Forgets everything that was said about the rows of a model.
    pub fn forget_rows(&self) {
        self.anchor.set((0, 0.0));
        self.sizes.borrow_mut().clear();
        self.places.borrow_mut().clear();
        self.slots.borrow_mut().clear();
        self.placed.borrow_mut().clear();
        self.added.borrow_mut().clear();
        self.last_places.borrow_mut().clear();
        *self.pending.borrow_mut() = None;
        self.avg.set(0.0);
    }

    /// Where the row of `line` starts as far as the last pass knows: where
    /// it is, or where rows of the average length put it.
    pub fn place_of(&self, line: usize, spacing: f64) -> f64 {
        let slots = self.slots.borrow();
        let per = self.avg.get() + spacing;
        let (Some(first), Some(last)) = (slots.first(), slots.last()) else {
            return self.start.get() + line as f64 * per;
        };
        if let Some(s) = slots.iter().find(|s| s.line == line) {
            s.pos
        } else if line < first.line {
            first.top() - (first.line - line) as f64 * per
        } else {
            last.end() + spacing + (line - last.line - 1) as f64 * per
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo(reversed: bool) -> Geo {
        Geo {
            grid: false,
            main: 1,
            reversed,
            cross_reversed: false,
            spacing: 0.0,
            per_line: 1,
            cell_main: 0.0,
            cell_cross: 0.0,
            view_main: 300.0,
            view_cross: 200.0,
            buffer: 320.0,
            margin_begin: 0.0,
            margin_end: 0.0,
        }
    }

    #[test]
    fn a_reversed_view_maps_rows_before_the_origin() {
        let g = geo(true);
        // Row 0 of 20 is right before 0, and a view of 300 that shows the
        // beginning sits at -300: what qml6 reports for BottomToTop.
        assert_eq!(g.in_content(0.0, 20.0), -20.0);
        assert_eq!(g.in_content(60.0, 20.0), -80.0);
        assert_eq!(g.content_position(0.0), -300.0);
        assert_eq!(g.viewport(-300.0), 0.0);
        let f = geo(false);
        assert_eq!(f.in_content(60.0, 20.0), 60.0);
        assert_eq!(f.viewport(40.0), 40.0);
    }

    #[test]
    fn grid_lines_and_cells() {
        let g = Geo {
            grid: true,
            per_line: 3,
            cell_main: 60.0,
            cell_cross: 80.0,
            ..geo(false)
        };
        assert_eq!(g.lines(100), 34);
        assert_eq!(g.lines(0), 0);
        assert_eq!(g.line_of(5), 1);
        assert_eq!(g.cross_of(2), 160.0);
        let r = Geo {
            cross_reversed: true,
            ..g
        };
        assert_eq!(r.cross_of(0), 160.0);
        assert_eq!(r.cross_of(2), 0.0);
    }

    #[test]
    fn positioning_modes() {
        use position_mode as m;
        // 100 rows of 20 in a view of 300: the numbers qml6 gives.
        let at = |mode, index: f64, from| position_target(mode, index * 20.0, 20.0, from, 300.0);
        assert_eq!(at(m::BEGINNING, 50.0, 0.0), Some(1000.0));
        assert_eq!(at(m::CENTER, 50.0, 0.0), Some(860.0));
        assert_eq!(at(m::END, 50.0, 0.0), Some(720.0));
        assert_eq!(at(m::VISIBLE, 50.0, 0.0), Some(720.0));
        assert_eq!(at(m::VISIBLE, 10.0, 1500.0), Some(200.0));
        assert_eq!(at(m::VISIBLE, 25.0, 400.0), None);
        assert_eq!(at(m::CONTAIN, 30.0, 0.0), Some(320.0));
        assert_eq!(at(m::SNAP_POSITION, 40.0, 0.0), Some(800.0));
    }

    #[test]
    fn the_view_goes_with_the_highlight_but_not_past_the_row() {
        // The highlight is far above and heads for a row above the view:
        // the view is at the row at once.
        assert_eq!(
            follow_target((3.7, 20.0), (800.0, 20.0), 1000.0, 300.0),
            800.0
        );
        // In sight: nothing moves.
        assert_eq!(follow_target((64.0, 20.0), (600.0, 20.0), 0.0, 300.0), 0.0);
        // Leaving at the end: the view keeps its end at the highlight's.
        assert_eq!(
            follow_target((290.0, 20.0), (600.0, 20.0), 0.0, 300.0),
            10.0
        );
        // The highlight is far below and heads for a row below the view.
        assert_eq!(
            follow_target((1600.0, 20.0), (600.0, 20.0), 0.0, 300.0),
            320.0
        );
        // Coming back up past the start of the view.
        assert_eq!(
            follow_target((500.0, 20.0), (100.0, 20.0), 1000.0, 300.0),
            500.0
        );
    }

    #[test]
    fn snapping_and_the_average() {
        let slot = |line: usize, pos: f64, size: f64, measured| Slot {
            line,
            pos,
            size,
            lead: 0.0,
            measured,
        };
        let slots = [
            slot(2, 80.0, 40.0, true),
            slot(3, 120.0, 40.0, true),
            slot(4, 160.0, 30.0, false),
        ];
        assert_eq!(snap_target(&slots, 105.0), Some(120.0));
        assert_eq!(snap_target(&slots, 95.0), Some(80.0));
        assert_eq!(snap_target(&[], 95.0), None);
        assert_eq!(average(&slots), Some(40.0));
        assert_eq!(average(&slots[2..]), None);
    }
}
