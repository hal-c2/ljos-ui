//! `SafeArea` (QtQuick 6.9): how far an item reaches into what the
//! platform covers (a notch, a status bar, a home indicator) or into
//! space an ancestor asked to keep free.
//!
//! `SafeArea.margins` is read-only and says, per edge, how much of the
//! item's own edge is unsafe. `SafeArea.additionalMargins` is what an
//! item adds for itself. Without a host
//! every margin is 0; a host that knows the platform's insets tells them
//! with [`set_safe_area_insets`] (a window's, in logical pixels, from the
//! scene's edges).
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-safearea.html and is
//! pinned by `features/items/safe-area.feature`.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, NodeId, ObjectId, PropIndex, QmlObject, Value, WriteKind};

use crate::item::Item;
use crate::scene::SCENE_CATEGORY;

/// `SafeArea` (QtQuick): only ever used attached.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "SafeArea",
    attached = SafeAreaAttached,
    uncreatable
)]
pub struct SafeArea {}

/// What `SafeArea.` attaches to an item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SafeAreaAttached")]
pub struct SafeAreaAttached {
    #[qml(group)]
    pub margins: SafeMargins,
    #[qml(group)]
    pub additional_margins: SafeAdditionalMargins,
}

/// `SafeArea.margins`, read-only.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SafeMargins", uncreatable)]
pub struct SafeMargins {
    #[qml(notify, readonly)]
    pub left: f64,
    #[qml(notify, readonly)]
    pub top: f64,
    #[qml(notify, readonly)]
    pub right: f64,
    #[qml(notify, readonly)]
    pub bottom: f64,
}

/// `SafeArea.additionalMargins`, written by the application.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SafeAdditionalMargins", uncreatable)]
pub struct SafeAdditionalMargins {
    #[qml(notify)]
    pub left: f64,
    #[qml(notify)]
    pub top: f64,
    #[qml(notify)]
    pub right: f64,
    #[qml(notify)]
    pub bottom: f64,
}

/// Per engine: what the host covers, and a counter that tells every
/// asker to ask again when it, or the set of attached objects, changed.
#[derive(Default)]
struct State {
    /// Left, top, right, bottom.
    insets: Cell<[f64; 4]>,
    epoch: Cell<Option<NodeId>>,
}

fn state(engine: &Engine) -> Rc<State> {
    engine.extension::<State>()
}

fn bump(engine: &Engine, s: &State) {
    let g = engine.graph();
    let node = match s.epoch.get() {
        Some(n) => n,
        None => {
            let Ok(n) = g.property(g.root(), Value::Int(0)) else {
                return;
            };
            s.epoch.set(Some(n));
            n
        }
    };
    crate::item::bump_node(engine, node);
}

fn watch_epoch(engine: &Engine, s: &State) {
    if s.epoch.get().is_none() {
        let g = engine.graph();
        s.epoch.set(g.property(g.root(), Value::Int(0)).ok());
    }
    if let Some(n) = s.epoch.get() {
        engine.graph().get(n);
    }
}

/// Tells the application what the platform covers at the edges of its
/// windows, in logical pixels. A host calls this when it starts and when
/// the platform says the insets changed (rotation, a keyboard). Telling
/// the same again costs nothing.
pub fn set_safe_area_insets(engine: &Engine, left: f64, top: f64, right: f64, bottom: f64) {
    let s = state(engine);
    let v = [left, top, right, bottom].map(|n| if n.is_finite() { n.max(0.0) } else { 0.0 });
    if s.insets.replace(v) == v {
        return;
    }
    bump(engine, &s);
    engine.request_wake();
}

/// What the host said last.
pub fn safe_area_insets(engine: &Engine) -> [f64; 4] {
    state(engine).insets.get()
}

/// `[left, top, right, bottom]` an item adds: tracked.
fn additional(engine: &Engine, att: ObjectId) -> [f64; 4] {
    ["left", "top", "right", "bottom"].map(|n| {
        engine
            .property_index(att, &format!("additionalMargins.{n}"))
            .map_or(0.0, |i| engine.read(att, i).to_number())
    })
}

/// The item's origin and size in the scene's coordinates. Tracked.
fn scene_rect(engine: &Engine, item: ObjectId) -> [f64; 4] {
    use crate::transform::scene_map;
    let o = scene_map(engine, item)
        .apply(ljos_render::kurbo::Point::ORIGIN)
        .unwrap_or(ljos_render::kurbo::Point::ORIGIN);
    let w = engine.read(item, Item::WIDTH.index).to_number();
    let h = engine.read(item, Item::HEIGHT.index).to_number();
    [o.x, o.y, w, h]
}

/// How far into the unsafe parts the item reaches, per edge.
fn compute(engine: &Engine, item: ObjectId, own: ObjectId) -> [f64; 4] {
    let s = state(engine);
    watch_epoch(engine, &s);
    let me = scene_rect(engine, item);
    let [x, y, w, h] = me;
    let root = crate::focus::tree::top_of(engine, item);
    let root_rect = if root == item {
        me
    } else {
        scene_rect(engine, root)
    };
    let ins = s.insets.get();
    // The unsafe region's inner edge on each side, in scene coordinates.
    let mut left = root_rect[0] + ins[0];
    let mut top = root_rect[1] + ins[1];
    let mut right = root_rect[0] + root_rect[2] - ins[2];
    let mut bottom = root_rect[1] + root_rect[3] - ins[3];
    // Only the item's own `additionalMargins` count: what an ancestor
    // keeps free does not reach into its children
    // (features/items/safe-area.feature).
    {
        let add = additional(engine, own);
        left = left.max(x + add[0]);
        top = top.max(y + add[1]);
        right = right.min(x + w - add[2]);
        bottom = bottom.min(y + h - add[3]);
    }
    [
        (left - x).clamp(0.0, w.max(0.0)),
        (top - y).clamp(0.0, h.max(0.0)),
        (x + w - right).clamp(0.0, w.max(0.0)),
        (y + h - bottom).clamp(0.0, h.max(0.0)),
    ]
}

/// Makes `margins` follow the item's place and what is asked of the
/// area.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created_of::<SafeAreaAttached>(|e, item, att| {
        let s = state(e);
        let leaves: Vec<PropIndex> = ["left", "top", "right", "bottom"]
            .iter()
            .filter_map(|n| e.property_index(att, &format!("margins.{n}")).ok())
            .collect();
        if leaves.len() == 4 {
            let followed = e.effect_in(att, move |e| {
                let m = compute(e, item, att);
                e.untrack(|| {
                    for (p, v) in leaves.iter().zip(m) {
                        let _ = e.write_internal(att, *p, Value::Real(v), WriteKind::Imperative);
                    }
                });
            });
            if let Err(err) = followed {
                e.log(
                    LogLevel::Warning,
                    SCENE_CATEGORY,
                    format!("SafeArea: margins do not follow the item: {err}"),
                );
            }
        }
        // Items that asked before this one existed look again.
        bump(e, &s);
        true
    });
}
