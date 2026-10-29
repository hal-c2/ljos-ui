//! Anchor layout: eager geometry effects (ARCHITECTURE §7.4).
//!
//! An item a scene shows is followed ([`follow`]) and, from the first
//! anchor line it is given, has one anchors effect. Most items are
//! anchored by nothing: they cost the scene no effect, and the line that
//! is given late is told ([`line_given`]) and starts one then. The effect
//! reads the item's `anchors.*` and, when any are set, the geometry of the
//! items it is anchored to; it writes the anchored `x`/`y`/`width`/`height`
//! through an override layer. The layer sits above whatever the item was
//! given, so removing an anchor leaves the item where it is (Qt keeps the
//! geometry too) and the item's own binding is not destroyed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, NodeId, ObjectId, OverrideContent, OverrideId, PropIndex,
    RestoreMode, Value, WriteKind,
};
use slotmap::SecondaryMap;

use crate::item::{AnchorLine, AnchorProps, Edge, Item, effective_margin, num};
use crate::mirroring::mirrored;

/// Log category for anchor warnings.
pub const CATEGORY: &str = "qt.quick.anchors";

/// Per-item anchor bookkeeping, owned by the effect.
#[derive(Debug, Default)]
pub(crate) struct AnchorState {
    x: Cell<Option<OverrideId>>,
    y: Cell<Option<OverrideId>>,
    w: Cell<Option<OverrideId>>,
    h: Cell<Option<OverrideId>>,
    flags: Cell<u8>,
    /// Last warnings, so an effect re-run does not repeat them.
    warned: RefCell<Box<[String]>>,
}

const FILL_FLAG: u8 = 1 << 0;
const CENTER_IN_FLAG: u8 = 1 << 1;
const RUNNING_FLAG: u8 = 1 << 2;
const REENTERED_FLAG: u8 = 1 << 3;
const OWN_WIDTH_FLAG: u8 = 1 << 4;
const OWN_HEIGHT_FLAG: u8 = 1 << 5;

impl AnchorState {
    fn flag(&self, flag: u8) -> bool {
        self.flags.get() & flag != 0
    }

    fn set_flag(&self, flag: u8, value: bool) {
        let flags = self.flags.get();
        self.flags
            .set(if value { flags | flag } else { flags & !flag });
    }

    /// Drops every override, keeping the current geometry.
    pub(crate) fn release(&self, engine: &Engine, obj: ObjectId) {
        for (p, slot) in [
            (Item::X.index, &self.x),
            (Item::Y.index, &self.y),
            (Item::WIDTH.index, &self.w),
            (Item::HEIGHT.index, &self.h),
        ] {
            if let Some(id) = slot.take() {
                let _ = engine.remove_override(obj, p, id, RestoreMode::None);
            }
        }
    }
}

/// The items scenes show, each with the effect that follows its anchors
/// once it has one. Per engine: a line is told of on its object, which
/// does not say which scene shows it.
#[derive(Default)]
struct Followed {
    items: RefCell<SecondaryMap<ObjectId, Shown>>,
}

struct Shown {
    /// How many scenes show it: the last to stop showing it stops it.
    scenes: u32,
    following: Option<Following>,
}

struct Following {
    effect: NodeId,
    state: Rc<AnchorState>,
}

/// Follows the anchors of `obj`, which a scene has started to show: now if
/// it has a line, otherwise from when it is given one.
pub(crate) fn follow(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let followed = engine.extension::<Followed>();
    {
        let mut items = followed.items.borrow_mut();
        if let Some(shown) = items.get_mut(obj) {
            shown.scenes += 1;
            return Ok(());
        }
        items.insert(
            obj,
            Shown {
                scenes: 1,
                following: None,
            },
        );
    }
    let a = AnchorProps::get();
    let lines = [
        a.fill,
        a.center_in,
        a.left,
        a.right,
        a.horizontal_center,
        a.top,
        a.bottom,
        a.vertical_center,
        a.baseline,
    ];
    if engine.authored(obj, &lines, (1 << lines.len()) - 1) != 0 {
        start(engine, obj, &followed)?;
    }
    Ok(())
}

/// Stops following `obj`'s anchors for a scene that no longer shows it;
/// the geometry they gave it stays.
pub(crate) fn unfollow(engine: &Engine, obj: ObjectId) {
    let followed = engine.extension::<Followed>();
    let following = {
        let mut items = followed.items.borrow_mut();
        let Some(shown) = items.get_mut(obj) else {
            return;
        };
        shown.scenes -= 1;
        if shown.scenes > 0 {
            return;
        }
        items.remove(obj).and_then(|s| s.following)
    };
    let Some(f) = following else {
        return;
    };
    engine.stop_effect(f.effect);
    if engine.is_alive(obj) {
        f.state.release(engine, obj);
    }
}

fn start(engine: &Engine, obj: ObjectId, followed: &Followed) -> Result<(), EngineError> {
    let state = Rc::new(AnchorState::default());
    let runs = Rc::clone(&state);
    // Made with nothing borrowed: its first run may be now, and what it
    // writes may tell of more.
    let effect = engine.effect_in(obj, move |e| run(e, obj, &runs))?;
    let kept = match followed.items.borrow_mut().get_mut(obj) {
        Some(shown) => {
            shown.following = Some(Following { effect, state });
            true
        }
        None => false,
    };
    if !kept {
        // Let go of while it was being made.
        engine.stop_effect(effect);
    }
    Ok(())
}

/// A followed item's own size changed where its anchors read it: finish its
/// pending effect before the change signal runs.
pub(crate) fn size_changed(engine: &Engine, obj: ObjectId, prop: PropIndex) {
    let flag = if prop == Item::WIDTH.index {
        OWN_WIDTH_FLAG
    } else if prop == Item::HEIGHT.index {
        OWN_HEIGHT_FLAG
    } else {
        return;
    };
    let followed = engine.extension::<Followed>();
    let following = followed.items.borrow().get(obj).and_then(|s| {
        s.following
            .as_ref()
            .map(|f| (f.effect, Rc::clone(&f.state)))
    });
    if let Some((effect, state)) = following.filter(|(_, st)| st.flag(flag)) {
        if state.flag(RUNNING_FLAG) {
            run(engine, obj, &state);
        } else {
            engine.graph().run_effect_now(effect);
        }
    }
}

/// An anchor line of `cx.this` was given: an item shown and not yet
/// followed starts to be.
pub(crate) fn line_given(cx: &Cx<'_>, _line: PropIndex) {
    let (engine, obj) = (cx.engine, cx.this);
    let followed = engine.extension::<Followed>();
    let waits = followed
        .items
        .borrow()
        .get(obj)
        .is_some_and(|s| s.following.is_none());
    if !waits {
        return;
    }
    if let Err(e) = start(engine, obj, &followed) {
        engine.log(LogLevel::Warning, CATEGORY, format!("anchors: {e}"));
    }
}

struct Ctx<'a> {
    engine: &'a Engine,
    obj: ObjectId,
    parent: Option<ObjectId>,
    /// What [`read_lines`] read.
    lines: [Value; 9],
    warnings: Vec<String>,
    own_width: bool,
    own_height: bool,
}

// Where each line is in what [`read_lines`] gives.
const FILL: usize = 0;
const CENTER_IN: usize = 1;
const LEFT: usize = 2;
const RIGHT: usize = 3;
const H_CENTER: usize = 4;
const TOP: usize = 5;
const BOTTOM: usize = 6;
const V_CENTER: usize = 7;
const BASELINE: usize = 8;

/// `fill`, `centerIn` and the seven lines of `obj`'s anchors.
fn read_lines(engine: &Engine, obj: ObjectId) -> [Value; 9] {
    let a = AnchorProps::get();
    engine.read_values(
        obj,
        [
            a.fill,
            a.center_in,
            a.left,
            a.right,
            a.horizontal_center,
            a.top,
            a.bottom,
            a.vertical_center,
            a.baseline,
        ],
    )
}

impl Ctx<'_> {
    fn warn(&mut self, msg: &str) {
        let ty = self
            .engine
            .meta_of(self.obj)
            .map_or_else(|| "Item".to_owned(), |m| m.name.to_string());
        self.warnings.push(format!("QML {ty}: {msg}"));
    }

    /// The position of `edge` of `target`, in the anchored item's parent's
    /// coordinates. Only the parent and siblings can be anchored to.
    fn edge(&mut self, target: ObjectId, edge: Edge) -> Option<f64> {
        let e = self.engine;
        if target == self.obj {
            self.warn("Cannot anchor item to self.");
            return None;
        }
        if !e.is_alive(target) {
            self.warn("Cannot anchor to a null item.");
            return None;
        }
        let (bx, by) = if Some(target) == self.parent {
            (0.0, 0.0)
        } else if self.parent.is_some() && e.visual_parent(target) == self.parent {
            (num(e, target, Item::X.index), num(e, target, Item::Y.index))
        } else {
            self.warn("Cannot anchor to an item that isn't a parent or sibling.");
            return None;
        };
        Some(match edge {
            Edge::Left => bx,
            Edge::Right => bx + num(e, target, Item::WIDTH.index),
            // Qt rounds the half size of the target (observed with qml6:
            // a 41-wide sibling at x 3 has its centre line at 24).
            Edge::HorizontalCenter => bx + (num(e, target, Item::WIDTH.index) / 2.0).round(),
            Edge::Top => by,
            Edge::Bottom => by + num(e, target, Item::HEIGHT.index),
            Edge::VerticalCenter => by + (num(e, target, Item::HEIGHT.index) / 2.0).round(),
            Edge::Baseline => by + num(e, target, Item::BASELINE_OFFSET.index),
        })
    }

    /// An `anchors.<side>` line, checked for the right axis.
    fn line(&mut self, i: usize, horizontal: bool) -> Option<AnchorLine> {
        let v = &self.lines[i];
        if v.is_nullish() {
            return None;
        }
        let Some(l) = AnchorLine::from_value(v) else {
            self.warn("Unable to assign a non-anchor value to an anchor.");
            return None;
        };
        if l.edge.is_horizontal() != horizontal {
            self.warn(if horizontal {
                "Cannot anchor a horizontal edge to a vertical edge."
            } else {
                "Cannot anchor a vertical edge to a horizontal edge."
            });
            return None;
        }
        Some(l)
    }
}

/// The anchored geometry along one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Span {
    pos: Option<f64>,
    len: Option<f64>,
}

/// The body of an item's anchors effect.
pub(crate) fn run(engine: &Engine, obj: ObjectId, state: &AnchorState) {
    if !engine.is_alive(obj) {
        return;
    }
    if state.flag(RUNNING_FLAG) {
        state.set_flag(REENTERED_FLAG, true);
        // A nested effect still follows what the new values will depend on;
        // the active pass applies the settled geometry.
        compute(engine, obj, state);
        return;
    }

    state.set_flag(RUNNING_FLAG, true);
    let mut settled = false;
    for _ in 0..32 {
        state.set_flag(REENTERED_FLAG, false);
        if let Some((mut cx, h, v)) = compute(engine, obj, state) {
            let apply_all = || -> Result<(), EngineError> {
                apply(engine, obj, Item::WIDTH.index, &state.w, h.len)?;
                apply(engine, obj, Item::X.index, &state.x, h.pos)?;
                apply(engine, obj, Item::HEIGHT.index, &state.h, v.len)?;
                apply(engine, obj, Item::Y.index, &state.y, v.pos)
            };
            // Length before position is visible to geometry handlers; with
            // no observer, one notification round has the same result.
            let observed = [
                Item::WIDTH.index,
                Item::X.index,
                Item::HEIGHT.index,
                Item::Y.index,
            ]
            .into_iter()
            .any(|p| engine.is_observed(obj, p));
            let result = if observed {
                apply_all()
            } else {
                engine.batch(apply_all)
            };
            if let Err(e) = result {
                cx.warnings.push(format!("anchors: {e}"));
            }
            let previous = state.warned.borrow();
            if cx.warnings.as_slice() != previous.as_ref() {
                let new: Vec<String> = cx
                    .warnings
                    .iter()
                    .filter(|w| !previous.contains(*w))
                    .cloned()
                    .collect();
                drop(previous);
                for w in new {
                    engine.log(LogLevel::Warning, CATEGORY, w);
                }
                *state.warned.borrow_mut() = cx.warnings.into_boxed_slice();
            }
        } else {
            if [state.x.get(), state.y.get(), state.w.get(), state.h.get()]
                .iter()
                .any(Option::is_some)
            {
                engine.batch(|| state.release(engine, obj));
            }
            *state.warned.borrow_mut() = Box::default();
        }
        if !state.flag(REENTERED_FLAG) {
            settled = true;
            break;
        }
    }
    state.set_flag(RUNNING_FLAG, false);
    if !settled {
        engine.warn_binding_loop(obj, Item::WIDTH.index);
    }
}

fn compute<'a>(
    engine: &'a Engine,
    obj: ObjectId,
    state: &AnchorState,
) -> Option<(Ctx<'a>, Span, Span)> {
    // Most items are anchored by nothing, and each new one runs this: the
    // lines are read in one go, and an item none is set on asks no more.
    let lines = read_lines(engine, obj);
    let (fill, center_in) = (lines[FILL].as_object(), lines[CENTER_IN].as_object());
    if lines.iter().all(Value::is_nullish) {
        state.set_flag(FILL_FLAG, false);
        state.set_flag(CENTER_IN_FLAG, false);
        state.set_flag(OWN_WIDTH_FLAG, false);
        state.set_flag(OWN_HEIGHT_FLAG, false);
        return None;
    }
    let keep = fill.is_none()
        && center_in.is_none()
        && (state.flag(FILL_FLAG) || state.flag(CENTER_IN_FLAG));
    state.set_flag(FILL_FLAG, fill.is_some());
    state.set_flag(CENTER_IN_FLAG, center_in.is_some());
    let mut cx = Ctx {
        engine,
        obj,
        parent: engine.visual_parent(obj),
        lines,
        warnings: Vec::new(),
        own_width: false,
        own_height: false,
    };
    let (h, v) = if keep {
        (Span::default(), Span::default())
    } else {
        (
            horizontal(&mut cx, fill, center_in),
            vertical(&mut cx, fill, center_in),
        )
    };
    state.set_flag(OWN_WIDTH_FLAG, cx.own_width);
    state.set_flag(OWN_HEIGHT_FLAG, cx.own_height);
    Some((cx, h, v))
}

/// Where the anchors of `obj` put it as they are at this moment: `x`,
/// `width`, `y` and `height`, `None` for what they leave alone. For who
/// has to know before the item's effect has run (a transition between two
/// sets of anchors). Follows nothing and warns of nothing.
pub(crate) fn resolve(engine: &Engine, obj: ObjectId) -> [Option<f64>; 4] {
    engine.untrack(|| {
        let lines = read_lines(engine, obj);
        let (fill, center_in) = (lines[FILL].as_object(), lines[CENTER_IN].as_object());
        let mut cx = Ctx {
            engine,
            obj,
            parent: engine.visual_parent(obj),
            lines,
            warnings: Vec::new(),
            own_width: false,
            own_height: false,
        };
        let h = horizontal(&mut cx, fill, center_in);
        let v = vertical(&mut cx, fill, center_in);
        [h.pos, h.len, v.pos, v.len]
    })
}

fn horizontal(cx: &mut Ctx<'_>, fill: Option<ObjectId>, center_in: Option<ObjectId>) -> Span {
    let a = AnchorProps::get();
    let e = cx.engine;
    let obj = cx.obj;
    if let Some(t) = fill {
        let l = cx.edge(t, Edge::Left);
        let r = cx.edge(t, Edge::Right);
        // `LayoutMirroring` swaps the two margins; the edges are the same.
        let (near, far) = if mirrored(e, obj) {
            (a.right_margin, a.left_margin)
        } else {
            (a.left_margin, a.right_margin)
        };
        return match (l, r) {
            (Some(l), Some(r)) => {
                let l = l + effective_margin(e, obj, near);
                let r = r - effective_margin(e, obj, far);
                Span {
                    pos: Some(l),
                    len: Some(r - l),
                }
            }
            _ => Span::default(),
        };
    }
    let (mut left, mut right) = if center_in.is_some() {
        (None, None)
    } else {
        (cx.line(LEFT, true), cx.line(RIGHT, true))
    };
    let mut hcenter = cx.line(H_CENTER, true);
    if hcenter.is_none()
        && let Some(t) = center_in
    {
        hcenter = Some(AnchorLine {
            item: t,
            edge: Edge::HorizontalCenter,
        });
    }
    // Under `LayoutMirroring` an item's left anchor holds its right edge,
    // every line points at the other side of its target, the two margins
    // change places and the centre offset counts the other way. Asked only
    // by items that are anchored: the answer walks up the tree.
    let flip = (left.is_some() || right.is_some() || hcenter.is_some()) && mirrored(e, obj);
    let (left_margin, right_margin, offset_sign) = if flip {
        (left, right) = (right.map(other_side), left.map(other_side));
        hcenter = hcenter.map(other_side);
        (a.right_margin, a.left_margin, -1.0)
    } else {
        (a.left_margin, a.right_margin, 1.0)
    };
    if left.is_some() && right.is_some() && hcenter.is_some() {
        // Qt warns and goes on with left and right.
        cx.warn("Cannot specify left, right, and horizontalCenter anchors at the same time.");
        hcenter = None;
    }
    let l = left
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p + effective_margin(e, obj, left_margin));
    let r = right
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p - effective_margin(e, obj, right_margin));
    let c = hcenter
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p + offset_sign * num(e, obj, a.horizontal_center_offset));
    let align = e.read(obj, a.align_when_centered).to_boolean();
    span(cx, Item::WIDTH.index, l, r, c, align)
}

/// The same line on the other side of its target: left for right.
fn other_side(l: AnchorLine) -> AnchorLine {
    AnchorLine {
        item: l.item,
        edge: match l.edge {
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
            other => other,
        },
    }
}

fn vertical(cx: &mut Ctx<'_>, fill: Option<ObjectId>, center_in: Option<ObjectId>) -> Span {
    let a = AnchorProps::get();
    let e = cx.engine;
    let obj = cx.obj;
    if let Some(t) = fill {
        let top = cx.edge(t, Edge::Top);
        let bottom = cx.edge(t, Edge::Bottom);
        return match (top, bottom) {
            (Some(t), Some(b)) => {
                let t = t + effective_margin(e, obj, a.top_margin);
                let b = b - effective_margin(e, obj, a.bottom_margin);
                Span {
                    pos: Some(t),
                    len: Some(b - t),
                }
            }
            _ => Span::default(),
        };
    }
    let (top, bottom) = if center_in.is_some() {
        (None, None)
    } else {
        (cx.line(TOP, false), cx.line(BOTTOM, false))
    };
    let mut vcenter = cx.line(V_CENTER, false);
    let baseline = cx.line(BASELINE, false);
    if vcenter.is_none()
        && let Some(t) = center_in
    {
        vcenter = Some(AnchorLine {
            item: t,
            edge: Edge::VerticalCenter,
        });
    }
    if baseline.is_some() && (top.is_some() || bottom.is_some() || vcenter.is_some()) {
        // Qt rejects the anchor set last; Ljós cannot tell which that was
        // and drops the baseline.
        cx.warn(
            "Baseline anchor cannot be used in conjunction with top, bottom, or verticalCenter anchors.",
        );
    } else if baseline.is_some() {
        let pos = baseline
            .and_then(|l| cx.edge(l.item, l.edge))
            .map(|p| p + num(e, obj, a.baseline_offset) - num(e, obj, Item::BASELINE_OFFSET.index));
        return Span { pos, len: None };
    }
    if top.is_some() && bottom.is_some() && vcenter.is_some() {
        cx.warn("Cannot specify top, bottom, and verticalCenter anchors at the same time.");
        vcenter = None;
    }
    let t = top
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p + effective_margin(e, obj, a.top_margin));
    let b = bottom
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p - effective_margin(e, obj, a.bottom_margin));
    let c = vcenter
        .and_then(|l| cx.edge(l.item, l.edge))
        .map(|p| p + num(e, obj, a.vertical_center_offset));
    let align = e.read(obj, a.align_when_centered).to_boolean();
    span(cx, Item::HEIGHT.index, t, b, c, align)
}

/// Reads the item's own length, noting that the anchors depend on it.
fn own_len(cx: &mut Ctx<'_>, prop: PropIndex) -> f64 {
    if prop == Item::WIDTH.index {
        cx.own_width = true;
    } else {
        cx.own_height = true;
    }
    let e = cx.engine;
    num(e, cx.obj, prop)
}

/// Resolves one axis from its start, end and centre lines (already
/// offset by margins).
fn span(
    cx: &mut Ctx<'_>,
    len_prop: PropIndex,
    start: Option<f64>,
    end: Option<f64>,
    center: Option<f64>,
    align_centered: bool,
) -> Span {
    match (start, end, center) {
        (Some(s), Some(t), _) => Span {
            pos: Some(s),
            len: Some(t - s),
        },
        (Some(s), None, Some(c)) => Span {
            pos: Some(s),
            len: Some((c - s) * 2.0),
        },
        (None, Some(t), Some(c)) => {
            let len = (t - c) * 2.0;
            Span {
                pos: Some(t - len),
                len: Some(len),
            }
        }
        (Some(s), None, None) => Span {
            pos: Some(s),
            len: None,
        },
        (None, Some(t), None) => Span {
            pos: Some(t - own_len(cx, len_prop)),
            len: None,
        },
        (None, None, Some(c)) => {
            // `alignWhenCentered` rounds the half size, not the position:
            // a 51-wide item centred on 100 lands on 74 (qml6).
            let half = own_len(cx, len_prop) / 2.0;
            Span {
                pos: Some(c - if align_centered { half.round() } else { half }),
                len: None,
            }
        }
        (None, None, None) => Span::default(),
    }
}

fn apply(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    slot: &Cell<Option<OverrideId>>,
    v: Option<f64>,
) -> Result<(), EngineError> {
    match (v, slot.get()) {
        (None, None) => Ok(()),
        (None, Some(id)) => {
            slot.set(None);
            engine.remove_override(obj, prop, id, RestoreMode::None)
        }
        (Some(v), None) => {
            slot.set(Some(engine.push_override(
                obj,
                prop,
                OverrideContent::Value(Value::Real(v)),
            )?));
            Ok(())
        }
        (Some(v), Some(id)) => {
            let cur = engine.untrack(|| engine.read(obj, prop));
            if cur.to_number() == v {
                return Ok(());
            }
            if engine
                .write_internal(obj, prop, Value::Real(v), WriteKind::Override(id))
                .is_err()
            {
                // Someone removed the layer; start a new one.
                slot.set(Some(engine.push_override(
                    obj,
                    prop,
                    OverrideContent::Value(Value::Real(v)),
                )?));
            }
            Ok(())
        }
    }
}
