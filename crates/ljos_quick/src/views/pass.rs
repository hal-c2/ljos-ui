//! The layout pass: which rows have a delegate, and where each one is.
//!
//! A pass starts from the anchor (the first row in sight after the last
//! pass, and where it was), lays rows out from there in both directions
//! until the window around what is shown is full, gives back every
//! delegate outside it, and writes the positions, the origin and the
//! length of the content. Rows that were never measured count as long as
//! the average of those that were, which is how Qt estimates too
//! (observed on qml6): `contentHeight` of a long list is a guess that
//! gets better as the view moves through it. A row just made whose
//! length is still to come from what is in it is laid out by a polish
//! before the next one is made, as Qt does (observed on qml6), so the
//! window holds as many rows as fit and none is made for a guess.
//!
//! The pass runs inside one effect ([`super::start`]) that reads what a
//! layout depends on, and directly whenever the engine tells the view
//! something (a model change, a delegate that is ready, a method).

use std::collections::VecDeque;

use ljos_engine::{Atom, Engine, Made, ObjectId, PropIndex, RowKey, Urgency, Value, ViewDelegates};
use rustc_hash::{FxHashMap, FxHashSet};

use super::state::{Extra, Geo, Slot, Steady, ViewState, average};
use super::{
    GridView, ItemView, ListView, Part, ViewAttached, attached_of, changes, current, enrol,
    grid_flow, orientation, put, section_criteria, stack, state_of, strike,
    vertical_layout_direction, warn,
};
use crate::enums::layout_direction;
use crate::flickable::{self, X, Y};
use crate::item::{Item, is_item, is_unset, size_follows_implicit, write_computed, write_placed};
use crate::tree;

/// How many rows either side of what is shown have delegates while an
/// assistive technology listens (ARCHITECTURE.md §14): it walks to the
/// next row, and that row has to be there.
const READER_ROWS: f64 = 20.0;

/// Lines laid out in one pass at most: rows that are 0 long would never
/// fill a window.
const MAX_LINES: usize = 10_000;
const MAX_EMPTY_LINES: usize = 64;
/// How far a pass walks from the anchor over rows it knows the length of
/// before it estimates instead.
const MAX_WALK: usize = 4096;
/// How many row lengths are remembered before those out of sight go.
const MAX_SIZES: usize = 16_384;

pub(crate) fn pos_prop(axis: usize) -> PropIndex {
    if axis == X {
        Item::X.index
    } else {
        Item::Y.index
    }
}

pub(crate) fn size_prop(axis: usize) -> PropIndex {
    if axis == X {
        Item::WIDTH.index
    } else {
        Item::HEIGHT.index
    }
}

pub(crate) fn rd(e: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    e.read(obj, p).to_number()
}

/// Makes the layout effect run again.
pub(crate) fn bump(e: &Engine, st: &ViewState) {
    if let Some(n) = st.trigger.get() {
        let g = e.graph();
        let now = g.get_untracked(n).to_number() as i32;
        g.set(n, Value::Int(now.wrapping_add(1)));
    }
}

/// Lays the rows out now: for whoever changed something the layout
/// effect does not read.
pub(crate) fn refresh(e: &Engine, view: ObjectId) {
    let Some(st) = state_of(e, view) else {
        return;
    };
    // The layout effect lays the rows out and reads the sizes of the
    // delegates there are now. Where it cannot run at once (in a batch,
    // or behind the effect that is running) the rows are laid out here.
    let before = st.passes.get();
    bump(e, &st);
    // In a batch the effect runs when the batch ends, before the caller
    // that began it goes on: laying the rows out here too would do it
    // twice.
    if st.passes.get() == before && !e.graph().batching() {
        e.untrack(|| pass(e, view, &st, None));
    }
}

/// Whether the rows of `view` run from right to left.
pub(crate) fn effective_rtl(e: &Engine, view: ObjectId) -> bool {
    let asked = rd(e, view, ItemView::LAYOUT_DIRECTION.index) as i32;
    (asked == layout_direction::RIGHT_TO_LEFT) != crate::mirroring::mirrored(e, view)
}

/// How the view is laid out now. Reads are tracked when an effect asks.
pub(crate) fn read_geo(e: &Engine, view: ObjectId, st: &ViewState) -> Geo {
    let n = |p: PropIndex| rd(e, view, p);
    let rtl = effective_rtl(e, view);
    let bottom_up = n(ItemView::VERTICAL_LAYOUT_DIRECTION.index) as i32
        == vertical_layout_direction::BOTTOM_TO_TOP;
    let (w, h) = (n(Item::WIDTH.index), n(Item::HEIGHT.index));
    let buffer = n(ItemView::CACHE_BUFFER.index).max(0.0);
    let margin_begin = n(ItemView::DISPLAY_MARGIN_BEGINNING.index);
    let margin_end = n(ItemView::DISPLAY_MARGIN_END.index);
    let mut g = if st.grid.get() {
        let cw = n(GridView::CELL_WIDTH.index).max(0.0);
        let ch = n(GridView::CELL_HEIGHT.index).max(0.0);
        let fit = |space: f64, cell: f64| {
            if cell > 0.0 && space.is_finite() {
                ((space / cell).floor() as usize).max(1)
            } else {
                1
            }
        };
        if n(GridView::FLOW.index) as i32 == grid_flow::TOP_TO_BOTTOM {
            Geo {
                grid: true,
                main: X,
                reversed: rtl,
                cross_reversed: bottom_up,
                spacing: 0.0,
                per_line: fit(h, ch),
                cell_main: cw,
                cell_cross: ch,
                view_main: w,
                view_cross: h,
                buffer,
                margin_begin,
                margin_end,
            }
        } else {
            Geo {
                grid: true,
                main: Y,
                reversed: bottom_up,
                cross_reversed: rtl,
                spacing: 0.0,
                per_line: fit(w, cw),
                cell_main: ch,
                cell_cross: cw,
                view_main: h,
                view_cross: w,
                buffer,
                margin_begin,
                margin_end,
            }
        }
    } else {
        let horizontal = n(ListView::ORIENTATION.index) as i32 == orientation::HORIZONTAL;
        Geo {
            grid: false,
            main: if horizontal { X } else { Y },
            reversed: if horizontal { rtl } else { bottom_up },
            cross_reversed: false,
            spacing: n(ListView::SPACING.index),
            per_line: 1,
            cell_main: 0.0,
            cell_cross: 0.0,
            view_main: if horizontal { w } else { h },
            view_cross: if horizontal { h } else { w },
            buffer,
            margin_begin,
            margin_end,
        }
    };
    if crate::system_prefs::screen_reader_active(e) {
        let per = if g.grid {
            g.cell_main
        } else {
            st.avg.get() + g.spacing
        };
        g.buffer = g.buffer.max(READER_ROWS * per.max(0.0));
    }
    g
}

/// The three properties of the `section` group.
fn section_props(st: &ViewState) -> Option<[PropIndex; 3]> {
    if let Some(p) = st.section_props.get() {
        return Some(p);
    }
    use ljos_engine::QmlType;
    let meta = ListView::static_meta();
    let p = [
        meta.property_index("section.property")?,
        meta.property_index("section.criteria")?,
        meta.property_index("section.delegate")?,
    ];
    st.section_props.set(Some(p));
    Some(p)
}

/// Reads, tracked, what a layout depends on besides the delegates.
pub(crate) fn read_inputs(e: &Engine, view: ObjectId, st: &ViewState) {
    let g = read_geo(e, view, st);
    let d = flickable::dir(g.main);
    for p in [
        d.lead,
        d.trail,
        ItemView::HEADER.index,
        ItemView::FOOTER.index,
        ItemView::HIGHLIGHT.index,
        ItemView::CURRENT_INDEX.index,
        ItemView::HIGHLIGHT_FOLLOWS_CURRENT_ITEM.index,
        ItemView::HIGHLIGHT_MOVE_DURATION.index,
        ItemView::REUSE_ITEMS.index,
    ] {
        let _ = e.read(view, p);
    }
    if !g.grid {
        let _ = e.read(view, ListView::HIGHLIGHT_MOVE_VELOCITY.index);
        if let Some(props) = section_props(st) {
            for p in props {
                let _ = e.read(view, p);
            }
        }
    }
    st.moves_rows
        .set(super::view_transition::moves_rows(e, view));
    if let Some(n) = st.trigger.get() {
        let _ = e.graph().get(n);
    }
}

/// Reads, tracked, the sizes the last pass laid the rows out by: a row
/// that grows moves the rows after it.
pub(crate) fn read_sizes(e: &Engine, view: ObjectId, st: &ViewState) {
    let g = e.untrack(|| read_geo(e, view, st));
    let size = size_prop(g.main);
    let items = sized_items(st, g.grid);
    for &o in &items {
        if e.is_alive(o) {
            let _ = e.read(o, size);
        }
    }
    // The highlight is as large as the current row.
    if let Some(c) = st.current.get().filter(|c| e.is_alive(*c)) {
        let _ = e.read(c, Item::WIDTH.index);
        let _ = e.read(c, Item::HEIGHT.index);
    }
    *st.sized.borrow_mut() = items;
}

/// The items whose lengths [`read_sizes`] reads, the current row last.
pub(crate) fn sized_items(st: &ViewState, grid: bool) -> Vec<ObjectId> {
    let mut items: Vec<ObjectId> = Vec::new();
    if !grid {
        items.extend(st.placed.borrow().iter().map(|p| p.1));
        items.extend(st.sections.borrow().values().map(|s| s.1));
    }
    for extra in [st.header.get(), st.footer.get()] {
        items.extend(extra.map(|x| x.1));
    }
    items.extend(st.current.get());
    items
}

/// Makes an item of `component` that is the view's and not a row.
pub(crate) fn make_extra(
    e: &Engine,
    view: ObjectId,
    content: ObjectId,
    component: ObjectId,
    values: &[(&str, Value)],
    z: Option<f64>,
) -> Option<ObjectId> {
    let made = e.create_in_context(component, Some(view), values, |e, root| {
        if !is_item(e, root) {
            return;
        }
        enrol(e, root, view, Part::Extra);
        e.batch(|| {
            warn(e, "parent", tree::set_parent_item(e, root, Some(content)));
            if let Some(z) = z {
                stack(e, root, z);
            }
        });
    });
    match made {
        Ok(o) if is_item(e, o) => Some(o),
        Ok(o) => {
            e.destroy(o);
            None
        }
        Err(err) => {
            warn(e, "create", Err(err));
            None
        }
    }
}

pub(crate) fn drop_extra(e: &Engine, item: ObjectId) {
    strike(e, item);
    if e.is_alive(item) {
        // Told while it is still in the tree, as a delegate is.
        e.announce_destruction(item);
        let _ = tree::set_parent_item(e, item, None);
        e.destroy(item);
    }
}

/// Writes a property that holds an object or nothing.
pub(crate) fn put_object(
    e: &Engine,
    view: ObjectId,
    p: ljos_engine::Prop<Option<ObjectId>>,
    v: Option<ObjectId>,
) {
    if e.untrack(|| e.get(view, p)).ok() != Some(v) {
        warn(e, "write", e.set_internal(view, p, v));
    }
}

/// Keeps the header or the footer in step with its component.
fn ensure_extra(
    e: &Engine,
    view: ObjectId,
    content: ObjectId,
    slot: &std::cell::Cell<Extra>,
    component: Option<ObjectId>,
    out: ljos_engine::Prop<Option<ObjectId>>,
) -> Option<ObjectId> {
    let keep = matches!(slot.get(), Some((c, item)) if c == component && component.is_some() && e.is_alive(item));
    if !keep {
        if let Some((_, old)) = slot.take() {
            drop_extra(e, old);
        }
        if let Some(c) = component
            && let Some(item) = make_extra(e, view, content, c, &[], Some(1.0))
        {
            slot.set(Some((component, item)));
        }
    }
    let item = slot.get().map(|x| x.1);
    put_object(e, view, out, item);
    item
}

/// Gives a delegate back: to the pool with `reuse`, not shown there.
pub(crate) fn release(
    e: &Engine,
    view: ObjectId,
    st: &ViewState,
    d: &ViewDelegates,
    obj: ObjectId,
    reuse: bool,
) {
    if !d.release(e, obj, reuse) {
        return;
    }
    if let Some(att) = attached_of(e, view, obj) {
        warn(e, "pooled", e.emit_by_name(att, "pooled", &[]));
    }
    if e.is_alive(obj) && st.hidden.borrow_mut().insert(obj) {
        super::cull::pool(e, obj, true);
    }
}

/// A delegate came out of the pool: it is shown again and told.
fn revive(e: &Engine, view: ObjectId, st: &ViewState, obj: ObjectId) {
    if st.hidden.borrow_mut().remove(&obj) {
        super::cull::pool(e, obj, false);
    }
    if let Some(att) = attached_of(e, view, obj) {
        warn(e, "reused", e.emit_by_name(att, "reused", &[]));
    }
}

/// The sections of a list: which property names them and how.
struct Sections {
    property: String,
    first_character: bool,
    delegate: Option<ObjectId>,
    /// Asked once a pass per row.
    seen: FxHashMap<usize, String>,
}

impl Sections {
    fn read(e: &Engine, view: ObjectId, st: &ViewState) -> Option<Sections> {
        if st.grid.get() {
            return None;
        }
        let [property, criteria, delegate] = section_props(st)?;
        let property = e.read(view, property).to_js_string().to_string();
        if property.is_empty() {
            return None;
        }
        Some(Sections {
            property,
            first_character: rd(e, view, criteria) as i32 == section_criteria::FIRST_CHARACTER,
            delegate: e.read(view, delegate).as_object(),
            seen: FxHashMap::default(),
        })
    }

    /// The section row `row` is in; the empty string before the first
    /// row and after the last.
    fn of(&mut self, e: &Engine, d: &ViewDelegates, row: Option<usize>) -> String {
        let Some(row) = row.filter(|r| *r < d.count()) else {
            return String::new();
        };
        if let Some(s) = self.seen.get(&row) {
            return s.clone();
        }
        let v = d.role(e, row, &self.property);
        let mut s = if v.is_nullish() {
            String::new()
        } else {
            v.to_js_string().to_string()
        };
        if self.first_character {
            s = s.chars().next().map(String::from).unwrap_or_default();
        }
        self.seen.insert(row, s.clone());
        s
    }
}

/// Whether the rows of `view` are in sections.
pub(crate) fn has_sections(e: &Engine, view: ObjectId, st: &ViewState) -> bool {
    e.untrack(|| Sections::read(e, view, st)).is_some()
}

/// Where a line is put: from its start on, or back from its end.
#[derive(Clone, Copy)]
enum At {
    Top(f64),
    End(f64),
}

/// What a pass gathers while it lays lines out.
struct Fill<'a> {
    e: &'a Engine,
    view: ObjectId,
    content: ObjectId,
    st: &'a ViewState,
    d: &'a ViewDelegates,
    g: Geo,
    rows: usize,
    /// What is shown: rows here have their delegate before the pass ends.
    zone: (f64, f64),
    current: Option<usize>,
    sections: Option<Sections>,
    /// How long a row that was never measured is taken to be.
    guess: f64,
    /// A row just made has its length still to come from what is in it
    /// (a text not laid out yet): whether another row fits is not known
    /// until it has one. The pass stops; the one that runs once the scene
    /// laid the row out fills on with the length it has then. Qt measures
    /// each row it makes before it makes the next (observed on qml6).
    waiting: bool,
    /// The row waited for is 0 long still after a polish: the rows are,
    /// and the view fills as for any rows of no length.
    no_wait: bool,
    /// A row after those laid out is still being made.
    held: bool,
    /// The shortest and the longest row measured in this pass.
    spread: (f64, f64),
    /// The delegates there are, with the logical span of each.
    items: Vec<(usize, ObjectId, f64, f64)>,
    /// Section headings, with where each starts and how long it is.
    heads: Vec<(ObjectId, f64, f64)>,
    used_heads: FxHashSet<RowKey>,
}

impl Fill<'_> {
    fn lay(&mut self, line: usize, at: At) -> Slot {
        if self.g.grid {
            self.lay_cells(line)
        } else {
            self.lay_row(line, at)
        }
    }

    fn lay_cells(&mut self, line: usize) -> Slot {
        let (e, g) = (self.e, self.g);
        let pos = line as f64 * g.cell_main;
        let shown = pos < self.zone.1 && pos + g.cell_main.max(1.0) > self.zone.0;
        let first = line * g.per_line;
        for row in first..(first + g.per_line).min(self.rows) {
            let urgency = if shown || Some(row) == self.current {
                Urgency::Now
            } else {
                Urgency::Later
            };
            if let Some((o, made)) = self.d.request(e, row, urgency) {
                if made == Made::Reused {
                    revive(e, self.view, self.st, o);
                }
                if is_item(e, o) {
                    self.items.push((row, o, pos, g.cell_main));
                }
            }
        }
        Slot {
            line,
            pos,
            size: g.cell_main,
            lead: 0.0,
            measured: true,
        }
    }

    /// The heading before `row`, made if it is not there: how long it is.
    fn heading(&mut self, row: usize, key: Option<RowKey>) -> Option<(ObjectId, f64)> {
        let (e, d) = (self.e, self.d);
        let sections = self.sections.as_mut()?;
        let name = sections.of(e, d, Some(row));
        let before = sections.of(e, d, row.checked_sub(1));
        if name == before {
            return None;
        }
        let (component, key) = (sections.delegate?, key?);
        let have = self.st.sections.borrow().get(&key).cloned();
        let item = match have {
            Some((old, item)) if old == name && e.is_alive(item) => item,
            other => {
                if let Some((_, item)) = other {
                    self.st.sections.borrow_mut().remove(&key);
                    drop_extra(e, item);
                }
                let values = [("section", Value::string(name.clone()))];
                let item = make_extra(e, self.view, self.content, component, &values, Some(2.0))?;
                self.st.sections.borrow_mut().insert(key, (name, item));
                item
            }
        };
        self.used_heads.insert(key);
        Some((item, rd(e, item, size_prop(self.g.main))))
    }

    fn wait_for(&mut self, key: RowKey) {
        let now = crate::scene::polishes();
        match self.st.waited.get() {
            Some((k, then)) if k == key && then < now => self.no_wait = true,
            Some((k, _)) if k == key => self.waiting = true,
            _ => {
                self.st.waited.set(Some((key, now)));
                self.waiting = true;
            }
        }
    }

    fn lay_row(&mut self, row: usize, at: At) -> Slot {
        let (e, g) = (self.e, self.g);
        let key = self.d.key_at(row);
        let head = self.heading(row, key);
        let lead = head.map_or(0.0, |h| h.1);
        let known = key.and_then(|k| self.st.sizes.borrow().get(&k).copied());
        let hint = known.unwrap_or(self.guess);
        // A row not measured yet is taken to be there at all.
        let block = lead + hint.max(1.0);
        let (top, end) = match at {
            At::Top(t) => (t, t + block),
            At::End(end) => (end - block, end),
        };
        let shown = top < self.zone.1 && end > self.zone.0;
        let urgency = if shown || Some(row) == self.current {
            Urgency::Now
        } else {
            Urgency::Later
        };
        let mut item = None;
        let got = self.d.request(e, row, urgency);
        let (size, measured) = match got {
            Some((o, made)) => {
                if made == Made::Reused {
                    revive(e, self.view, self.st, o);
                }
                if is_item(e, o) {
                    item = Some(o);
                    let size = rd(e, o, size_prop(g.main));
                    let follows = size_follows_implicit(e, o, g.main == X);
                    // A row whose length is still to come from what is in
                    // it (a text not shaped yet) is not 0 long. Nor is it
                    // as long as a positioner made it on completion,
                    // before the text in it had a length, or as long as
                    // the items in it laid out so far while the scene lays
                    // them out: it is measured once a polish has run since
                    // it was made and none is running, and the view lays
                    // its rows out again then.
                    // Nor is a row just made 0 long for good: its height
                    // may be bound to what is in it (a text's implicit
                    // height), which has none until the scene lays it out.
                    // Once a polish has run it is as long as it is then,
                    // 0 or not.
                    let empty = !follows && size <= 0.0;
                    let follows = follows || empty;
                    let polishing = follows && crate::scene::polishing();
                    if !follows || (!polishing && (size > 0.0 || empty) && self.st.settled(o, made))
                    {
                        (size, true)
                    } else {
                        super::changes::polished_later(e, self.view);
                        // Nor is it as long as the first row was, or as
                        // long as a positioner made it before its text had
                        // a length: a column of a title and a picture would
                        // be taken as the picture's height, and a new view
                        // would make rows that the polish then lets go.
                        // A row made in this pass has not been laid out
                        // at all, polishing or not.
                        let laid = polishing && made == Made::Existing;
                        // A row measured before is as long as it was then.
                        // Rows made before this pass and not waited for (a
                        // method laid them out) are laid on: stopping at
                        // one would let go of those after it.
                        let new = made != Made::Existing
                            || matches!(self.st.waited.get(), Some((w, _)) if Some(w) == key);
                        if let Some(k) = key
                            && new
                            && known.is_none()
                            && !laid
                            && !self.no_wait
                            && !self.waiting
                        {
                            self.wait_for(k);
                        }
                        if size <= 0.0 {
                            (hint, false)
                        } else {
                            if self.guess <= 0.0 && !self.waiting {
                                // The first row stands for the rest until then.
                                self.guess = size;
                            }
                            let other = if laid || self.guess <= 0.0 {
                                size
                            } else {
                                self.guess
                            };
                            (known.unwrap_or(other), false)
                        }
                    }
                } else {
                    (0.0, true)
                }
            }
            None => {
                // A row ahead still being made has no length either: where
                // the rows differ in length, the pass that its being ready
                // runs fills on from it. Where they are all as long, the
                // guess is right and the buffer fills as fast as the
                // incubator makes rows. Behind, the rows laid out go on to
                // where the view's origin is worked out from, and that
                // stays where a method put it.
                if !self.no_wait
                    && matches!(at, At::Top(_))
                    && self.spread.1 - self.spread.0 > 0.5
                    && self.d.pending().contains(&row)
                {
                    self.held = true;
                }
                (hint, false)
            }
        };
        if measured && let Some(k) = key {
            self.st.sizes.borrow_mut().insert(k, size);
        }
        if measured || known.is_some() {
            self.spread = (self.spread.0.min(size), self.spread.1.max(size));
        }
        if measured && self.guess <= 0.0 {
            // The first row measured says how long the ones still to be
            // made are taken to be, so that a view that is new does not
            // ask for every row it has.
            self.guess = size;
            self.waiting &= size <= 0.0;
        }
        let pos = match at {
            At::Top(t) => t + lead,
            At::End(end) => end - size,
        };
        let slot = Slot {
            line: row,
            pos,
            size,
            lead,
            measured,
        };
        if let Some(o) = item {
            self.items.push((row, o, pos, size));
        }
        if let Some((h, lead)) = head {
            self.heads.push((h, slot.top(), lead));
        }
        slot
    }

    /// How long row `row` and the heading before it are, if the row was
    /// measured before.
    fn known_block(&mut self, row: usize) -> Option<f64> {
        let key = self.d.key_at(row)?;
        let size = self.st.sizes.borrow().get(&key).copied()?;
        let lead = match self.sections.as_mut() {
            Some(s) => {
                let (name, before) = (
                    s.of(self.e, self.d, Some(row)),
                    s.of(self.e, self.d, row.checked_sub(1)),
                );
                if name != before && s.delegate.is_some() {
                    let have = self.st.sections.borrow().get(&key).map(|x| x.1);
                    match have.filter(|h| self.e.is_alive(*h)) {
                        Some(h) => rd(self.e, h, size_prop(self.g.main)),
                        None => self.st.heading.get(),
                    }
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        Some(lead + size)
    }

    /// The row to start from and where its block starts, near `v0`.
    fn anchor(&mut self, lines: usize, v0: f64, window: (f64, f64)) -> (usize, f64) {
        let g = self.g;
        if g.grid {
            let line = if g.cell_main > 0.0 {
                (v0 / g.cell_main).floor().max(0.0) as usize
            } else {
                0
            };
            let line = line.min(lines - 1);
            return (line, line as f64 * g.cell_main);
        }
        let (mut a, mut t) = self.st.anchor.get();
        if a >= lines {
            // The rows it stood on are gone: the last one is where it was.
            a = lines - 1;
        }
        for round in 0..2 {
            let mut steps = 0;
            // Forward over rows that end before what is shown.
            while a + 1 < lines && steps < MAX_WALK {
                let Some(block) = self.known_block(a) else {
                    break;
                };
                if t + block + g.spacing > v0 {
                    break;
                }
                t += block + g.spacing;
                a += 1;
                steps += 1;
            }
            // Back over rows that start after it.
            while a > 0 && t > v0 && steps < MAX_WALK {
                let Some(block) = self.known_block(a - 1) else {
                    break;
                };
                a -= 1;
                t -= block + g.spacing;
                steps += 1;
            }
            if round == 1 {
                break;
            }
            let mut per = self.st.avg.get() + g.spacing;
            let block = self.known_block(a).unwrap_or(per.max(1.0));
            if t <= window.1 && t + block >= window.0 {
                break;
            }
            // Far from what is shown and nothing known in between: rows
            // of the average length are taken to be there.
            if per <= 0.0 {
                // Nothing was ever measured: one row says how long.
                let probe = self.lay_row(a, At::Top(t));
                per = probe.size + probe.lead + g.spacing;
                self.items.clear();
                self.heads.clear();
                if per <= 0.0 {
                    break;
                }
            }
            let by = ((v0 - t) / per).floor();
            let to = (a as f64 + by).clamp(0.0, (lines - 1) as f64) as usize;
            t += (to as f64 - a as f64) * per;
            a = to;
            // A row the view was at before is where it was then.
            if let Some(k) = self.d.key_at(a)
                && let Some(place) = self.st.places.borrow().get(&k)
            {
                t = *place;
            }
        }
        (a, t)
    }
}

/// The rows the last pass laid out that adjoin `slots`, before its first
/// and after its last, as long as each was measured at some time. A row
/// laid out at its known length while its delegate is still to be made
/// counts as that length, not as the average, in the pass that laid it.
fn beside(st: &ViewState, d: &ViewDelegates, slots: &[Slot]) -> (Vec<Slot>, Vec<Slot>) {
    let (Some(first), Some(last)) = (slots.first(), slots.last()) else {
        return (Vec::new(), Vec::new());
    };
    let last_pass = st.slots.borrow();
    let sizes = st.sizes.borrow();
    let known = |s: &Slot| known_size(&sizes, d, s);
    let mut before = Vec::new();
    let mut want = first.line;
    for s in last_pass.iter().rev().filter(|s| s.line < first.line) {
        match known(s) {
            Some(k) if want == s.line + 1 => {
                before.push(k);
                want = s.line;
            }
            _ => break,
        }
    }
    let mut after = Vec::new();
    let mut want = last.line + 1;
    for s in last_pass.iter().filter(|s| s.line > last.line) {
        match known(s) {
            Some(k) if s.line == want => {
                after.push(k);
                want += 1;
            }
            _ => break,
        }
    }
    (before, after)
}

/// `s` at the length its row was last measured at, if it ever was.
fn known_size(sizes: &FxHashMap<RowKey, f64>, d: &ViewDelegates, s: &Slot) -> Option<Slot> {
    let size = *sizes.get(&d.key_at(s.line)?)?;
    Some(Slot {
        size,
        measured: true,
        ..*s
    })
}

/// Where the row of `line` starts when it is outside what was laid out.
fn place_outside(slots: &VecDeque<Slot>, line: usize, per: f64, spacing: f64) -> f64 {
    let (Some(first), Some(last)) = (slots.front(), slots.back()) else {
        return line as f64 * per;
    };
    if let Some(s) = slots.iter().find(|s| s.line == line) {
        s.pos
    } else if line < first.line {
        first.top() - (first.line - line) as f64 * per
    } else {
        last.end() + spacing + (line - last.line - 1) as f64 * per
    }
}

/// One layout pass. With `over` the rows are laid out as if the view
/// started there, and neither the view nor its content is changed: for
/// the methods that have to measure a row before they can go to it. What
/// the content would be is left in [`ViewState::laid`].
pub(crate) fn pass(e: &Engine, view: ObjectId, st: &ViewState, over: Option<f64>) {
    if st.in_pass.replace(true) {
        st.dirty.set(true);
        return;
    }
    st.passes.set(st.passes.get() + 1);
    run(e, view, st, over);
    st.in_pass.set(false);
    // What changed during a pass for a method is laid out once the method
    // has gone where it measured: laid out now, it would be laid out where
    // the view still is, and the row measured would be let go.
    if over.is_none() && st.dirty.replace(false) {
        bump(e, st);
    }
    fetch_at_end(e, view, st);
}

/// When the last row has a delegate (or there are none), the model may
/// have more to read.
fn fetch_at_end(e: &Engine, view: ObjectId, st: &ViewState) {
    let Some(model) = st.delegates().and_then(|d| d.model()) else {
        return;
    };
    let rows = model.row_count();
    let at_end = rows == 0 || st.placed.borrow().iter().any(|p| p.0 + 1 == rows);
    if at_end {
        let current = |e: &Engine, view| state_of(e, view)?.delegates()?.model();
        super::fetch::soon(e, view, model, current);
    }
}

fn run(e: &Engine, view: ObjectId, st: &ViewState, over: Option<f64>) {
    if !e.is_alive(view) {
        return;
    }
    let (Some(d), Some(content)) = (st.delegates(), flickable::content_item(e, view)) else {
        return;
    };
    st.steady.set(None);
    let g = read_geo(e, view, st);
    let dir = flickable::dir(g.main);
    let first_pass = st.seen_index.get().is_none();
    let rows = d.count();
    put(e, view, ItemView::COUNT.index, Value::Int(rows as i32));
    let current = current::normalise(e, view, st, rows);

    // The direction the rows run in.
    let axis = (g.main, g.reversed);
    if st.last_main.get() != Some(axis) {
        if let Some((old, _)) = st.last_main.get() {
            if old != g.main {
                let od = flickable::dir(old);
                flickable::set_reversed(e, view, old, false);
                // Nothing is anywhere along the way the rows ran before.
                let mut theirs: Vec<ObjectId> = d.live().into_iter().map(|l| l.1).collect();
                theirs.extend(st.header.get().map(|x| x.1));
                theirs.extend(st.footer.get().map(|x| x.1));
                theirs.extend(st.sections.borrow().values().map(|x| x.1));
                let was = pos_prop(old);
                e.batch(|| {
                    for o in theirs {
                        if e.is_alive(o) && is_item(e, o) {
                            warn(e, "place", write_placed(e, o, was, Value::Real(0.0)));
                        }
                    }
                    put(e, view, od.origin, Value::Real(0.0));
                    put(e, view, od.content, Value::Real(-1.0));
                    flickable::scroll_to(e, view, old, 0.0);
                });
            }
            st.forget_rows();
            st.positioned.set(false);
        }
        flickable::set_reversed(e, view, g.main, g.reversed);
        st.last_main.set(Some(axis));
    }
    if g.grid {
        // The cells of a grid that fills its lines from the far side end
        // at the far side of the view.
        let cross = 1 - g.main;
        let at = if g.cross_reversed {
            g.per_line as f64 * g.cell_cross - g.view_cross
        } else {
            0.0
        };
        e.batch(|| {
            put(e, view, flickable::dir(cross).origin, Value::Real(at));
            flickable::scroll_to(e, view, cross, at);
        });
    }

    let header = e.get(view, ItemView::HEADER).ok().flatten();
    let footer = e.get(view, ItemView::FOOTER).ok().flatten();
    let head_item = ensure_extra(e, view, content, &st.header, header, ItemView::HEADER_ITEM);
    let foot_item = ensure_extra(e, view, content, &st.footer, footer, ItemView::FOOTER_ITEM);
    let size_p = size_prop(g.main);
    let head = head_item.map_or(0.0, |h| rd(e, h, size_p));
    let foot = foot_item.map_or(0.0, |f| rd(e, f, size_p));

    // Where the view starts.
    let margin = rd(e, view, if g.reversed { dir.trail } else { dir.lead });
    let position = rd(e, view, dir.position);
    if first_pass && !is_unset(e, view, dir.position) && position != -margin {
        // A place that was declared is where the view starts.
        st.positioned.set(true);
    }
    if let Some(c) = st.carried_anchor.take()
        && first_pass
        && c.main == g.main
        && st.positioned.get()
    {
        st.anchor.set(c.anchor);
        // The first pass may come before the view has its length, and
        // then only the current row is made.
        if st.avg.get() <= 0.0 {
            st.avg.set(c.avg);
        }
    }
    let to_begin = !st.positioned.get() && over.is_none();
    let v0 = match over {
        Some(v) => v,
        None if to_begin => st.anchor.get().1 - head - margin,
        None => g.viewport(position),
    };
    let zone = (v0 - g.margin_begin, v0 + g.view_main + g.margin_end);
    let window = if d.rows_show_themselves() {
        // Rows that are items of their own are all there already: each
        // has its place, however far from what is shown, so that none is
        // left lying where it was last.
        (f64::NEG_INFINITY, f64::INFINITY)
    } else {
        (zone.0 - g.buffer, zone.1 + g.buffer)
    };

    let reuse = e.get(view, ItemView::REUSE_ITEMS).unwrap_or(false);
    let lines = g.lines(rows);
    if reuse {
        // What left the window goes to the pool first, so that the rows
        // that enter it find a delegate there.
        let gone: Vec<ObjectId> = st
            .placed
            .borrow()
            .iter()
            .filter(|p| (p.2 + p.3 <= window.0 || p.2 > window.1) && Some(p.0) != current)
            .map(|p| p.1)
            .collect();
        for o in gone {
            if d.index_of(o).is_some_and(|row| Some(row) != current) {
                release(e, view, st, &d, o, true);
            }
        }
    }

    let section_delegate = Sections::read(e, view, st).and_then(|s| s.delegate);
    if st.section_delegate.replace(section_delegate) != section_delegate {
        // Headings of another component: none of the old ones stays.
        let old: Vec<ObjectId> = st.sections.borrow_mut().drain().map(|(_, s)| s.1).collect();
        for h in old {
            drop_extra(e, h);
        }
    }

    let mut fill = Fill {
        e,
        view,
        content,
        st,
        d: &d,
        g,
        rows,
        zone,
        current,
        sections: Sections::read(e, view, st),
        guess: st.avg.get(),
        waiting: false,
        // Rows that are items of their own are all laid out anyway: none
        // is made for a guess, so none is waited for.
        no_wait: d.rows_show_themselves(),
        held: false,
        spread: (f64::INFINITY, f64::NEG_INFINITY),
        items: Vec::new(),
        heads: Vec::new(),
        used_heads: FxHashSet::default(),
    };
    let avg_before = st.avg.get();
    let mut slots: VecDeque<Slot> = VecDeque::new();
    if lines > 0 {
        let (a, t) = fill.anchor(lines, v0, window);
        slots.push_back(fill.lay(a, At::Top(t)));
        let mut budget = MAX_LINES;
        let mut empty = 0;
        // Qt fills while the next row starts inside the window, its far
        // edge included (observed on qml6).
        while let Some(last) = slots.back().copied() {
            if last.line + 1 >= lines
                || last.end() + g.spacing > window.1
                || budget == 0
                || fill.waiting
                || fill.held
            {
                break;
            }
            let s = fill.lay(last.line + 1, At::Top(last.end() + g.spacing));
            empty = if s.size + s.lead + g.spacing <= 0.0 {
                empty + 1
            } else {
                0
            };
            slots.push_back(s);
            budget -= 1;
            if empty > MAX_EMPTY_LINES {
                break;
            }
        }
        empty = 0;
        while let Some(first) = slots.front().copied() {
            if first.line == 0 || first.top() <= window.0 || budget == 0 || fill.waiting {
                break;
            }
            let s = fill.lay(first.line - 1, At::End(first.top() - g.spacing));
            empty = if s.size + s.lead + g.spacing <= 0.0 {
                empty + 1
            } else {
                0
            };
            slots.push_front(s);
            budget -= 1;
            if empty > MAX_EMPTY_LINES {
                break;
            }
        }
    }

    // How long everything is.
    // Rows still being made are not measured, but the row the anchor
    // measured to find its way says how long they are: without it the
    // rows before the first one laid out would take no room, and a view
    // made at a place (one a reload carried) would start there.
    // Rows the last pass laid out just beyond these count as long as they
    // were measured, not as the average: two passes a row apart (a method
    // that lays out where the view goes, then the view where it ended up)
    // then give the same length, and a list sent to its end again keeps it.
    let (before, after) = if g.grid {
        (Vec::new(), Vec::new())
    } else {
        beside(st, &d, slots.make_contiguous())
    };
    let guessed = (fill.guess > 0.0).then_some(fill.guess);
    let avg = {
        let sizes = st.sizes.borrow();
        let all: Vec<Slot> = before
            .iter()
            .copied()
            .chain(
                slots
                    .iter()
                    .map(|s| known_size(&sizes, &d, s).unwrap_or(*s)),
            )
            .chain(after.iter().copied())
            .collect();
        average(&all)
    }
    .or(guessed)
    .unwrap_or(avg_before);
    let per = avg + g.spacing;
    let block = |s: &Slot| s.lead + s.size + g.spacing;
    let (start0, end) = match (slots.front(), slots.back()) {
        (Some(first), Some(last)) if g.grid => {
            let _ = (first, last);
            (0.0, lines as f64 * g.cell_main)
        }
        (Some(first), Some(last)) => (
            first.top()
                - before.iter().map(block).sum::<f64>()
                - (first.line - before.len()) as f64 * per,
            last.end()
                + after.iter().map(block).sum::<f64>()
                + (lines - 1 - last.line - after.len()) as f64 * per,
        ),
        _ => (0.0, 0.0),
    };

    // The current row has a delegate wherever it is: `currentItem` is
    // never a row that was scrolled away.
    let mut current_span = None;
    if let Some(ci) = current {
        if let Some(found) = fill.items.iter().find(|i| i.0 == ci) {
            current_span = Some((found.2, found.3));
        } else if let Some((o, made)) = d.request(e, ci, Urgency::Now) {
            if made == Made::Reused {
                revive(e, view, st, o);
            }
            if is_item(e, o) {
                let line = g.line_of(ci);
                let (pos, size) = if g.grid {
                    (line as f64 * g.cell_main, g.cell_main)
                } else {
                    (
                        place_outside(&slots, line, per, g.spacing),
                        rd(e, o, size_p),
                    )
                };
                fill.items.push((ci, o, pos, size));
                current_span = Some((pos, size));
            }
        }
    }

    // Everything else outside the window goes.
    let in_run = |row: usize| {
        let line = g.line_of(row);
        matches!((slots.front(), slots.back()), (Some(f), Some(l)) if line >= f.line && line <= l.line)
    };
    for (row, o) in d.live() {
        if !in_run(row) && Some(row) != current {
            release(e, view, st, &d, o, reuse);
        }
    }
    for row in d.pending() {
        if !in_run(row) && Some(row) != current {
            d.cancel(e, row);
        }
    }
    d.trim_pool(
        e,
        if reuse {
            2 * slots.len() * g.per_line + 4
        } else {
            0
        },
    );
    let Fill {
        items,
        heads,
        used_heads,
        mut sections,
        ..
    } = fill;
    let unused: Vec<ObjectId> = {
        let mut all = st.sections.borrow_mut();
        let keys: Vec<RowKey> = all
            .keys()
            .filter(|k| !used_heads.contains(k))
            .copied()
            .collect();
        keys.iter()
            .filter_map(|k| all.remove(k))
            .map(|s| s.1)
            .collect()
    };
    for h in unused {
        drop_extra(e, h);
    }
    if let Some(longest) = heads.iter().map(|h| h.2).reduce(f64::max) {
        st.heading.set(longest);
    }

    // Where everything is. A row that another place was put at by the
    // last pass and that the model's last word moved or pushed aside is
    // taken there by its transition instead, which is started below
    // while the item is still where it was.
    let pos_p = pos_prop(g.main);
    let cross_p = pos_prop(1 - g.main);
    let dest_of = |row: usize, o: ObjectId, pos: f64, size: f64| -> [f64; 2] {
        let mut dest = [0.0; 2];
        dest[g.main] = g.in_content(pos, size);
        dest[1 - g.main] = if g.grid {
            g.cross_of(row % g.per_line)
        } else {
            rd(e, o, cross_p)
        };
        dest
    };
    let mut vt_start: Vec<(ObjectId, ObjectId, i32, [f64; 2])> = Vec::new();
    if over.is_none()
        && let Some(op) = st.pending.borrow_mut().take()
    {
        let mut targets = super::state::Targets {
            indexes: op.target_indexes.clone(),
            items: op.target_items.clone(),
        };
        if targets.items.is_empty() {
            targets.items = items
                .iter()
                .filter(|(row, ..)| d.key_at(*row).is_some_and(|k| op.target_keys.contains(&k)))
                .map(|(_, o, ..)| *o)
                .collect();
        }
        let targets = std::rc::Rc::new(targets);
        for &(row, o, pos, size) in &items {
            let Some(key) = d.key_at(row) else { continue };
            let was = st
                .last_places
                .borrow()
                .get(&key)
                .filter(|(of, _)| *of == o)
                .map(|(_, at)| *at);
            let Some(was) = was else { continue };
            let dest = dest_of(row, o, pos, size);
            if dest == was {
                continue;
            }
            let target = op.target_keys.contains(&key);
            if let Some(t) = super::view_transition::transition_for(e, view, op.kind, target) {
                vt_start.push((t, o, row as i32, dest));
            }
        }
        if !vt_start.is_empty() {
            // Kept for the writes below: which items are not to be put.
            st.vt_places.borrow_mut().extend(
                vt_start
                    .iter()
                    .map(|(_, o, _, dest)| (*o, [Some(dest[X]), Some(dest[Y])])),
            );
            // The same targets for every run the change starts.
            for (t, o, index, dest) in &vt_start {
                super::view_transition::start(
                    e,
                    view,
                    Some(*t),
                    *o,
                    *index,
                    *dest,
                    targets.clone(),
                );
            }
        }
    }
    let running_to: FxHashMap<ObjectId, [Option<f64>; 2]> = st.vt_places.borrow().clone();
    e.batch(|| {
        for &(row, o, pos, size) in &items {
            // An item on its way to this very place is left to its
            // transition; one on its way somewhere else is put here.
            let on_way = running_to.get(&o);
            // A row of a list is put along the view only, so where it is
            // across matters only to a transition.
            let dest = if g.grid || on_way.is_some() {
                dest_of(row, o, pos, size)
            } else {
                let mut dest = [0.0; 2];
                dest[g.main] = g.in_content(pos, size);
                dest
            };
            let holds =
                |axis: usize| on_way.is_some_and(|to| to[axis].is_some_and(|to| to == dest[axis]));
            if on_way.is_some() && ![X, Y].iter().any(|a| holds(*a)) {
                super::view_transition::forget(e, view, o);
            }
            if !holds(g.main) {
                let at = Value::Real(dest[g.main]);
                warn(e, "place", write_placed(e, o, pos_p, at));
            }
            if g.grid && !holds(1 - g.main) {
                let across = Value::Real(dest[1 - g.main]);
                warn(e, "place", write_placed(e, o, cross_p, across));
            }
        }
        for &(h, top, lead) in &heads {
            let at = Value::Real(g.in_content(top, lead));
            warn(e, "place", write_placed(e, h, pos_p, at));
        }
        if let Some(h) = head_item {
            let at = Value::Real(g.in_content(start0 - head, head));
            warn(e, "place", write_placed(e, h, pos_p, at));
        }
        if let Some(f) = foot_item {
            let at = Value::Real(g.in_content(end, foot));
            warn(e, "place", write_placed(e, f, pos_p, at));
        }
        let (lo, hi) = (start0 - head, end + foot);
        let origin = if g.reversed { -hi } else { lo };
        if over.is_some() {
            // Content that got shorter here would bring the view inside
            // it, and lay rows out at every place it passes on the way.
            st.laid.set(Some((origin, hi - lo)));
        } else {
            put(e, view, dir.origin, Value::Real(origin));
            put(e, view, dir.content, Value::Real(hi - lo));
        }
    });

    // Where the view is.
    let b = flickable::bounds(e, view, g.main);
    if to_begin {
        let begin = if g.reversed { b.max } else { b.min };
        flickable::scroll_to(e, view, g.main, begin);
        st.positioned.set(true);
        st.bounds.set((b.min, b.max, 0.0));
    } else if over.is_none() && st.bounds.get() != (b.min, b.max, 0.0) {
        // The content got shorter, or starts elsewhere: what is past its
        // ends now comes back inside.
        st.bounds.set((b.min, b.max, 0.0));
        let inside = b.position.clamp(b.min.min(b.max), b.max);
        if inside != b.position {
            flickable::scroll_to(e, view, g.main, inside);
        }
    }
    if over.is_none()
        && let Some((end, at)) = st.at_edge.get()
    {
        // Rows a method made at an edge take their length after it put
        // the view there: the edge moves, and the view goes with it until
        // they all have one. Moved by anything else, it stays.
        let b = flickable::bounds(e, view, g.main);
        let settled = !fill.waiting && !fill.held && slots.iter().all(|s| s.measured);
        let edge = if end != g.reversed { b.max } else { b.min };
        let kept = (b.position - at).abs() < 0.5;
        if kept && edge != b.position {
            flickable::scroll_to(e, view, g.main, edge);
            st.bounds.set((b.min, b.max, 0.0));
        }
        st.at_edge.set((kept && !settled).then_some((end, edge)));
    }
    let v_now = match over {
        Some(v) => v,
        None => g.viewport(rd(e, view, dir.position)),
    };
    if over.is_none() {
        // What is made for the cacheBuffer is not drawn (`cull`). A row
        // that touches what is shown is drawn (observed on qml6).
        let shown = (v_now - g.margin_begin, v_now + g.view_main + g.margin_end);
        let unshown = items
            .iter()
            .filter(|(_, _, pos, size)| pos + size < shown.0 || *pos > shown.1)
            .map(|(_, o, ..)| *o)
            .collect();
        super::cull::set(e, view, unshown);
    }

    // What the next pass starts from.
    // A row with no length yet (none of a view that has no length either
    // was made) ends where it starts: the first of those at the start of
    // what is shown is the one, not the last, which the rows before it
    // would then be laid out behind.
    let anchor = slots
        .iter()
        .find(|s| s.end() > v_now)
        .or_else(|| slots.iter().find(|s| s.top() >= v_now))
        .or(slots.back())
        .map_or((0, 0.0), |s| (s.line, s.top()));
    st.anchor.set(anchor);
    st.start.set(start0);
    st.end.set(end);
    st.avg.set(avg);
    if !g.grid {
        let mut places = st.places.borrow_mut();
        if places.len() > MAX_SIZES {
            places.clear();
        }
        for s in &slots {
            // Only what was measured: a row that is still to be made is
            // where the rows before it leave it, and that moves.
            if s.measured
                && let Some(k) = d.key_at(s.line)
            {
                places.insert(k, s.top());
            }
        }
        let mut sizes = st.sizes.borrow_mut();
        if sizes.len() > MAX_SIZES {
            let keep: FxHashSet<RowKey> = slots.iter().filter_map(|s| d.key_at(s.line)).collect();
            sizes.retain(|k, _| keep.contains(k));
        }
    }
    let first_shown = slots.iter().find(|s| s.end() > v_now).map(|s| s.line);
    // Every line laid out holds all its rows: a grid's line all its cells.
    let whole = |s: &Slot| {
        let cells = (rows - s.line * g.per_line).min(g.per_line);
        items.iter().filter(|i| g.line_of(i.0) == s.line).count() == cells
    };
    let keeps = over.is_none()
        && !fill.waiting
        && !fill.held
        && slots.len() < MAX_LINES
        && slots.iter().all(|s| s.measured && s.size > 0.0 && whole(s))
        && vt_start.is_empty()
        && running_to.is_empty();
    *st.slots.borrow_mut() = slots.into_iter().collect();
    *st.placed.borrow_mut() = items.clone();
    // A transition set later runs the pass again (`read_inputs`), which
    // keeps the places before a change can need them.
    let mut last_places = st.last_places.borrow_mut();
    if st.moves_rows.get() {
        *last_places = items
            .iter()
            .filter_map(|(row, o, pos, size)| {
                d.key_at(*row)
                    .map(|k| (k, (*o, dest_of(*row, *o, *pos, *size))))
            })
            .collect();
    } else {
        last_places.clear();
    }
    drop(last_places);
    if avg_before == 0.0 && avg > 0.0 && crate::system_prefs::screen_reader_active(e) {
        // The larger cache is so many rows, and now a row has a length.
        st.dirty.set(true);
    }

    // What the view says about its rows.
    let span = current.and(current_span);
    current::sync(e, view, st, &g, content, span);
    let current = current_index(e, view);
    for &(row, o, ..) in &items {
        if let Some(att) = attached_of(e, view, o) {
            tell(e, st, &d, sections.as_mut(), o, att, current);
        }
        // A delegate of a DelegateModel: the entry it stands for is the
        // row it was placed at, also after a use again for another row.
        super::delegate_groups::retell_for_row(e, o, row);
    }
    if !g.grid {
        let name = match sections.as_mut() {
            Some(s) => s.of(e, &d, first_shown),
            None => String::new(),
        };
        put(
            e,
            view,
            ListView::CURRENT_SECTION.index,
            Value::string(name),
        );
    }
    put(
        e,
        view,
        ItemView::EFFECTIVE_LAYOUT_DIRECTION.index,
        Value::Int(i32::from(effective_rtl(e, view))),
    );
    announce(e, view, st, &d, &items);
    let keeps = keeps
        && st.positioned.get()
        && st.at_edge.get().is_none()
        && st.pending.borrow().is_none()
        && st.added.borrow().is_empty()
        && !st.populate.get()
        && d.pending().is_empty();
    if keeps && let Some(trigger) = st.trigger.get() {
        st.steady.set(Some(Steady {
            g,
            rows,
            trigger: e.graph().get_untracked(trigger).to_number() as i32,
            reuse,
            span,
        }));
    }
}

/// The view moved along its rows. Where the rows the last pass laid out
/// are the ones a pass would lay out again, at the places they have, only
/// what follows from where the view is changes: which of them are drawn,
/// the row the next pass starts from, the current section and the
/// highlight. Anything else is a pass.
pub(crate) fn scrolled(e: &Engine, view: ObjectId, st: &ViewState) {
    if st.in_pass.replace(true) {
        st.dirty.set(true);
        return;
    }
    let kept = keep_rows(e, view, st);
    st.in_pass.set(false);
    if !kept || st.dirty.replace(false) {
        bump(e, st);
        return;
    }
    fetch_at_end(e, view, st);
}

/// What [`scrolled`] does where the rows stay: false where they do not,
/// and nothing changed then.
fn keep_rows(e: &Engine, view: ObjectId, st: &ViewState) -> bool {
    let (Some(s), Some(d)) = (st.steady.get(), st.delegates()) else {
        return false;
    };
    if !e.is_alive(view) {
        return false;
    }
    let Some(content) = flickable::content_item(e, view) else {
        return false;
    };
    let g = read_geo(e, view, st);
    let trigger = st
        .trigger
        .get()
        .map(|n| e.graph().get_untracked(n).to_number() as i32);
    if g != s.g
        || d.count() != s.rows
        || trigger != Some(s.trigger)
        || st.at_edge.get().is_some()
        || !d.pending().is_empty()
    {
        return false;
    }
    let dir = flickable::dir(g.main);
    let v = g.viewport(rd(e, view, dir.position));
    let shown = (v - g.margin_begin, v + g.view_main + g.margin_end);
    let window = if d.rows_show_themselves() {
        (f64::NEG_INFINITY, f64::INFINITY)
    } else {
        (shown.0 - g.buffer, shown.1 + g.buffer)
    };
    let lines = g.lines(s.rows);
    let slots = st.slots.borrow();
    let (Some(first), Some(last)) = (slots.first(), slots.last()) else {
        return false;
    };
    // A pass starts at the row at `v`, lays rows out after it until the
    // next would start past the window and before it until one starts
    // before it: the same rows, where none of those inside stops it.
    let ends = |s: &Slot| s.line + 1 >= lines || s.end() + g.spacing > window.1;
    let begins = |s: &Slot| s.line == 0 || s.top() <= window.0;
    let n = slots.len();
    if v < first.top()
        || v > last.end()
        || !ends(last)
        || !begins(first)
        || (n > 1 && (ends(&slots[n - 2]) || begins(&slots[1])))
    {
        return false;
    }
    let current = st.seen_index.get().and_then(|i| usize::try_from(i).ok());
    let placed = st.placed.borrow();
    let outside = |p: &(usize, ObjectId, f64, f64)| p.2 + p.3 <= window.0 || p.2 > window.1;
    if s.reuse && placed.iter().any(|p| outside(p) && Some(p.0) != current) {
        // The pass would give it back to the pool first.
        return false;
    }
    let unshown = placed
        .iter()
        .filter(|(_, _, pos, size)| pos + size < shown.0 || *pos > shown.1)
        .map(|(_, o, ..)| *o)
        .collect();
    drop(placed);
    super::cull::set(e, view, unshown);
    let anchor = slots
        .iter()
        .find(|s| s.end() > v)
        .or_else(|| slots.iter().find(|s| s.top() >= v))
        .or(slots.last())
        .map_or((0, 0.0), |s| (s.line, s.top()));
    st.anchor.set(anchor);
    let first_shown = slots.iter().find(|s| s.end() > v).map(|s| s.line);
    drop(slots);
    current::sync(e, view, st, &g, content, s.span);
    if !g.grid {
        let name = match Sections::read(e, view, st).as_mut() {
            Some(sections) => sections.of(e, &d, first_shown),
            None => String::new(),
        };
        put(
            e,
            view,
            ListView::CURRENT_SECTION.index,
            Value::string(name),
        );
    }
    true
}

/// Tells the rows that arrived, and shows the first rows of a model the
/// way `populate` says.
fn announce(
    e: &Engine,
    view: ObjectId,
    st: &ViewState,
    d: &ViewDelegates,
    items: &[(usize, ObjectId, f64, f64)],
) {
    if st.populate.get() && d.count() == 0 {
        // A model that starts with no rows has shown all it started
        // with: the rows that come arrive.
        st.populate.set(false);
    }
    if items.is_empty() && d.pending().is_empty() {
        st.added.borrow_mut().clear();
        return;
    }
    if st.populate.replace(false) {
        // The rows a model starts with did not arrive.
        st.added.borrow_mut().clear();
        if let Some(t) = e.get(view, ItemView::POPULATE).ok().flatten() {
            for &(_, o, ..) in items {
                changes::animate(e, view, Some(t), o, |_| {});
            }
        }
        return;
    }
    if st.added.borrow().is_empty() {
        return;
    }
    let added = std::mem::take(&mut *st.added.borrow_mut());
    let transition = e.get(view, ItemView::ADD).ok().flatten();
    for &(row, o, ..) in items {
        if d.key_at(row).is_some_and(|k| added.contains(&k)) {
            if let Some(att) = attached_of(e, view, o) {
                warn(e, "add", e.emit_by_name(att, "add", &[]));
            }
            changes::animate(e, view, transition, o, |_| {});
        }
    }
    // A row whose delegate is still being made is told when it is there.
    let waiting: FxHashSet<RowKey> = d
        .pending()
        .into_iter()
        .filter_map(|row| d.key_at(row))
        .filter(|k| added.contains(k))
        .collect();
    st.added.borrow_mut().extend(waiting);
}

/// Writes what the attached object of a delegate says about its row.
fn tell(
    e: &Engine,
    st: &ViewState,
    d: &ViewDelegates,
    sections: Option<&mut Sections>,
    item: ObjectId,
    att: ObjectId,
    current: i64,
) {
    let Some(row) = d.index_of(item) else {
        return;
    };
    let is_current = current == row as i64 && st.seen_index.get().is_some();
    let names = match sections {
        Some(s) => [
            s.of(e, d, Some(row)),
            s.of(e, d, row.checked_sub(1)),
            s.of(e, d, Some(row + 1)),
        ],
        None => [String::new(), String::new(), String::new()],
    };
    let told = (is_current, names);
    // Every pass tells every placed delegate, and nearly always the same.
    if e.with(att, |a: &ViewAttached| a.told == told)
        .unwrap_or(false)
    {
        return;
    }
    e.batch(|| {
        let write = |p: PropIndex, v: Value| warn(e, "attached", write_computed(e, att, p, v));
        write(ViewAttached::IS_CURRENT_ITEM.index, Value::Bool(told.0));
        let [name, before, after] = &told.1;
        write(
            ViewAttached::SECTION.index,
            Value::string(Atom::from(name.as_str())),
        );
        write(
            ViewAttached::PREVIOUS_SECTION.index,
            Value::string(Atom::from(before.as_str())),
        );
        write(
            ViewAttached::NEXT_SECTION.index,
            Value::string(Atom::from(after.as_str())),
        );
    });
    let _ = e.with_mut(att, |a: &mut ViewAttached| a.told = told);
}

/// The view's `currentIndex`, untracked, as [`tell`] compares it.
fn current_index(e: &Engine, view: ObjectId) -> i64 {
    e.untrack(|| rd(e, view, ItemView::CURRENT_INDEX.index)) as i64
}

/// What the attached object of one delegate says, for a delegate that
/// asks after the view placed it.
pub(crate) fn tell_row(e: &Engine, view: ObjectId, st: &ViewState, item: ObjectId, att: ObjectId) {
    let Some(d) = st.delegates() else {
        return;
    };
    let mut sections = Sections::read(e, view, st);
    let current = current_index(e, view);
    tell(e, st, &d, sections.as_mut(), item, att, current);
}
