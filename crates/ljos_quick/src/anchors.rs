//! Anchor layout: eager geometry effects (ARCHITECTURE §7.4).
//!
//! Each item in a scene has one anchors effect. It reads the item's
//! `anchors.*` and, when any are set, the geometry of the items it is
//! anchored to; it writes the anchored `x`/`y`/`width`/`height` through
//! an override layer. The layer sits above whatever the item was given,
//! so removing an anchor leaves the item where it is (Qt keeps the
//! geometry too) and the item's own binding is not destroyed.

use std::cell::RefCell;

use ljos_engine::{
    Engine, EngineError, LogLevel, ObjectId, OverrideContent, OverrideId, PropIndex, RestoreMode,
    Value, WriteKind,
};

use crate::item::{AnchorLine, AnchorProps, Edge, Item, effective_margin, num};
use crate::mirroring::mirrored;

/// Log category for anchor warnings.
pub const CATEGORY: &str = "qt.quick.anchors";

/// Per-item anchor bookkeeping, owned by the effect.
#[derive(Debug, Default)]
pub(crate) struct AnchorState {
    x: Option<OverrideId>,
    y: Option<OverrideId>,
    w: Option<OverrideId>,
    h: Option<OverrideId>,
    /// Last warnings, so an effect re-run does not repeat them.
    warned: Vec<String>,
}

impl AnchorState {
    /// Drops every override, keeping the current geometry.
    pub(crate) fn release(&mut self, engine: &Engine, obj: ObjectId) {
        for (p, slot) in [
            (Item::X.index, &mut self.x),
            (Item::Y.index, &mut self.y),
            (Item::WIDTH.index, &mut self.w),
            (Item::HEIGHT.index, &mut self.h),
        ] {
            if let Some(id) = slot.take() {
                let _ = engine.remove_override(obj, p, id, RestoreMode::None);
            }
        }
    }
}

struct Ctx<'a> {
    engine: &'a Engine,
    obj: ObjectId,
    parent: Option<ObjectId>,
    warnings: Vec<String>,
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
    fn line(&mut self, p: PropIndex, horizontal: bool) -> Option<AnchorLine> {
        let v = self.engine.read(self.obj, p);
        if v.is_nullish() {
            return None;
        }
        let Some(l) = AnchorLine::from_value(&v) else {
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
pub(crate) fn run(engine: &Engine, obj: ObjectId, state: &RefCell<AnchorState>) {
    if !engine.is_alive(obj) {
        return;
    }
    let a = AnchorProps::get();
    let mut cx = Ctx {
        engine,
        obj,
        parent: engine.visual_parent(obj),
        warnings: Vec::new(),
    };
    let fill = engine.read(obj, a.fill).as_object();
    let center_in = engine.read(obj, a.center_in).as_object();
    let h = horizontal(&mut cx, fill, center_in);
    let v = vertical(&mut cx, fill, center_in);

    let mut st = state.borrow_mut();
    let result = engine.batch(|| -> Result<(), EngineError> {
        apply(engine, obj, Item::X.index, &mut st.x, h.pos)?;
        apply(engine, obj, Item::WIDTH.index, &mut st.w, h.len)?;
        apply(engine, obj, Item::Y.index, &mut st.y, v.pos)?;
        apply(engine, obj, Item::HEIGHT.index, &mut st.h, v.len)
    });
    if let Err(e) = result {
        cx.warnings.push(format!("anchors: {e}"));
    }
    if cx.warnings != st.warned {
        for w in &cx.warnings {
            if !st.warned.contains(w) {
                engine.log(LogLevel::Warning, CATEGORY, w.clone());
            }
        }
        st.warned = cx.warnings;
    }
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
    let mut left = cx.line(a.left, true);
    let mut right = cx.line(a.right, true);
    let mut hcenter = cx.line(a.horizontal_center, true);
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
    span(e, obj, Item::WIDTH.index, l, r, c, align)
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
    let top = cx.line(a.top, false);
    let bottom = cx.line(a.bottom, false);
    let mut vcenter = cx.line(a.vertical_center, false);
    let baseline = cx.line(a.baseline, false);
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
    span(e, obj, Item::HEIGHT.index, t, b, c, align)
}

/// Resolves one axis from its start, end and centre lines (already
/// offset by margins).
fn span(
    e: &Engine,
    obj: ObjectId,
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
            pos: Some(t - num(e, obj, len_prop)),
            len: None,
        },
        (None, None, Some(c)) => {
            // `alignWhenCentered` rounds the half size, not the position:
            // a 51-wide item centred on 100 lands on 74 (qml6).
            let half = num(e, obj, len_prop) / 2.0;
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
    slot: &mut Option<OverrideId>,
    v: Option<f64>,
) -> Result<(), EngineError> {
    match (v, *slot) {
        (None, None) => Ok(()),
        (None, Some(id)) => {
            *slot = None;
            engine.remove_override(obj, prop, id, RestoreMode::None)
        }
        (Some(v), None) => {
            *slot =
                Some(engine.push_override(obj, prop, OverrideContent::Value(Value::Real(v)))?);
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
                *slot = Some(engine.push_override(
                    obj,
                    prop,
                    OverrideContent::Value(Value::Real(v)),
                )?);
            }
            Ok(())
        }
    }
}
