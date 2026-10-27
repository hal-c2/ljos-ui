//! `LayoutItemProxy` (`QtQuick.Layouts`): stands in for a `target` item
//! inside a layout so the same item can live in alternative layouts that
//! switch by visibility.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject, Value, WriteKind, qml_impl,
};

use super::{Attached, H, LayoutAttached, MAX_CELLS, V, attacher};
use crate::item::{Item, is_a, is_unset, is_unset_tracked, num, write_placed};
use crate::tree::set_parent_item;

/// `LayoutItemProxy` (`QtQuick.Layouts`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Layouts", name = "LayoutItemProxy", extends = Item)]
pub struct LayoutItemProxy {
    #[qml(base)]
    pub item: Item,
    /// The item this proxy stands in for.
    #[qml(notify, object = "Item")]
    pub target: Option<ObjectId>,
    #[qml(skip)]
    state: ProxyState,
}

#[derive(Debug, Default)]
struct ProxyState {
    registered_target: Option<ObjectId>,
    /// Effective visibility at the last run, `None` before the first.
    was_visible: Option<bool>,
    initialized: bool,
}

/// Which properties on a proxy's `LayoutAttached` still hold their
/// forwarding binding (`strong_count() > 0`).
#[derive(Debug, Default)]
pub(crate) struct ProxyAttachedState {
    /// `LayoutItemProxy` items currently targeting the owner of this
    /// `LayoutAttached`, in registration order.
    pub(crate) proxies: Vec<ObjectId>,
    forwarded: HashMap<PropIndex, Weak<()>>,
}

#[qml_impl]
impl LayoutItemProxy {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_proxy(cx.engine, cx.this)
    }
}

/// Forwarded `LayoutAttached` properties (all except `row`, `column`,
/// `rowSpan`, `columnSpan`, which belong to the proxy's own cell in a
/// `GridLayout`).
fn forwarded_props() -> [PropIndex; 16] {
    [
        LayoutAttached::FILL_WIDTH.index,
        LayoutAttached::FILL_HEIGHT.index,
        LayoutAttached::PREFERRED_WIDTH.index,
        LayoutAttached::PREFERRED_HEIGHT.index,
        LayoutAttached::MINIMUM_WIDTH.index,
        LayoutAttached::MINIMUM_HEIGHT.index,
        LayoutAttached::MAXIMUM_WIDTH.index,
        LayoutAttached::MAXIMUM_HEIGHT.index,
        LayoutAttached::ALIGNMENT.index,
        LayoutAttached::MARGINS.index,
        LayoutAttached::LEFT_MARGIN.index,
        LayoutAttached::TOP_MARGIN.index,
        LayoutAttached::RIGHT_MARGIN.index,
        LayoutAttached::BOTTOM_MARGIN.index,
        LayoutAttached::HORIZONTAL_STRETCH_FACTOR.index,
        LayoutAttached::VERTICAL_STRETCH_FACTOR.index,
    ]
}

fn is_side_margin(p: PropIndex) -> bool {
    p == LayoutAttached::LEFT_MARGIN.index
        || p == LayoutAttached::TOP_MARGIN.index
        || p == LayoutAttached::RIGHT_MARGIN.index
        || p == LayoutAttached::BOTTOM_MARGIN.index
}

/// Whether `p` on `att` has not been given an explicit value by the author
/// (either still at its default provenance, or still holding the proxy's
/// computed forwarding binding).
pub(crate) fn own_unset_tracked(e: &Engine, att: ObjectId, p: PropIndex) -> bool {
    if is_unset_tracked(e, att, p) {
        return true;
    }
    // Track the property's provenance so an effect re-runs when the author
    // assigns or binds over the forwarding binding.
    let _ = e.provenance_tracked(att, p);
    e.with(att, |a: &LayoutAttached| {
        a.proxy
            .forwarded
            .get(&p)
            .is_some_and(|w| w.strong_count() > 0)
    })
    .unwrap_or(false)
}

/// Installs forwarding bindings on `att` when `att` is the `LayoutAttached`
/// of a `LayoutItemProxy`.
pub(crate) fn setup_proxy_attached(e: &Engine, proxy: ObjectId, att: ObjectId) {
    if !is_a::<LayoutItemProxy>(e, proxy) {
        return;
    }
    for p in forwarded_props() {
        if !e.untrack(|| is_unset(e, att, p)) {
            continue;
        }
        let token = Rc::new(());
        let weak = Rc::downgrade(&token);
        let _ = e.with_mut(att, |a: &mut LayoutAttached| {
            a.proxy.forwarded.insert(p, weak);
        });
        let _ = e.bind_computed(att, p, move |e| {
            let _alive = &token;
            read_forwarded_prop(e, proxy, att, p)
        });
    }
}

fn read_forwarded_prop(e: &Engine, proxy: ObjectId, att: ObjectId, p: PropIndex) -> Value {
    if is_side_margin(p) && !own_unset_tracked(e, att, LayoutAttached::MARGINS.index) {
        return e.read(att, LayoutAttached::MARGINS.index);
    }
    let Some(target) = e.read(proxy, LayoutItemProxy::TARGET.index).as_object() else {
        return if is_side_margin(p) {
            e.read(att, LayoutAttached::MARGINS.index)
        } else {
            e.default_value(att, p)
        };
    };
    if !e.is_alive(target) {
        return e.default_value(att, p);
    }
    let Ok(ta) = e.attached_object(target, &attacher()) else {
        return e.default_value(att, p);
    };
    if is_side_margin(p) {
        let v = e.read(ta, p);
        if !own_unset_tracked(e, ta, p) {
            v
        } else {
            e.read(ta, LayoutAttached::MARGINS.index)
        }
    } else {
        let v = e.read(ta, p);
        let _ = own_unset_tracked(e, ta, p);
        v
    }
}

/// Reads `Attached` for `proxy`, merging properties set on `proxy.Layout`
/// over properties set on `target.Layout` (except `row`, `column`,
/// `rowSpan`, `columnSpan`, which come from `proxy.Layout` only).
pub(crate) fn read_proxy_attached(e: &Engine, proxy: ObjectId) -> Attached {
    let own = RawAttached::read(e, proxy);
    let Some(target) = e.read(proxy, LayoutItemProxy::TARGET.index).as_object() else {
        return own.into_attached(None);
    };
    if !e.is_alive(target) {
        return own.into_attached(None);
    }
    let target_raw = RawAttached::read(e, target);
    own.into_attached(Some(&target_raw))
}

#[derive(Clone, Copy, Debug, Default)]
struct RawAttached {
    fill: [Option<bool>; 2],
    pref: [Option<f64>; 2],
    min: [Option<f64>; 2],
    max: [Option<f64>; 2],
    stretch: [Option<i32>; 2],
    align: Option<i32>,
    row: Option<usize>,
    column: Option<usize>,
    row_span: usize,
    column_span: usize,
    all_margins: Option<f64>,
    side_margins: [Option<f64>; 4],
}

impl RawAttached {
    fn read(e: &Engine, item: ObjectId) -> Self {
        let Some(a) = e.existing_attached_object(item, &attacher()) else {
            return Self {
                row_span: 1,
                column_span: 1,
                ..Self::default()
            };
        };
        let set = |p: PropIndex| {
            let v = e.read(a, p);
            (!own_unset_tracked(e, a, p)).then_some(v)
        };
        let size = |p: PropIndex| set(p).map(|v| v.to_number()).filter(|v| *v >= 0.0);
        let flag = |p: PropIndex| set(p).map(|v| v.to_boolean());
        let int = |p: PropIndex| set(p).map(|v| v.to_number() as i32);
        let cell = |p: PropIndex| size(p).map(|v| v.min(MAX_CELLS) as usize);
        let span = |p: PropIndex| num(e, a, p).clamp(1.0, MAX_CELLS) as usize;
        let margin = |p: PropIndex| set(p).map(|v| v.to_number());
        Self {
            fill: [
                flag(LayoutAttached::FILL_WIDTH.index),
                flag(LayoutAttached::FILL_HEIGHT.index),
            ],
            pref: [
                size(LayoutAttached::PREFERRED_WIDTH.index),
                size(LayoutAttached::PREFERRED_HEIGHT.index),
            ],
            min: [
                size(LayoutAttached::MINIMUM_WIDTH.index),
                size(LayoutAttached::MINIMUM_HEIGHT.index),
            ],
            max: [
                size(LayoutAttached::MAXIMUM_WIDTH.index),
                size(LayoutAttached::MAXIMUM_HEIGHT.index),
            ],
            stretch: [
                int(LayoutAttached::HORIZONTAL_STRETCH_FACTOR.index),
                int(LayoutAttached::VERTICAL_STRETCH_FACTOR.index),
            ],
            align: int(LayoutAttached::ALIGNMENT.index),
            row: cell(LayoutAttached::ROW.index),
            column: cell(LayoutAttached::COLUMN.index),
            row_span: span(LayoutAttached::ROW_SPAN.index),
            column_span: span(LayoutAttached::COLUMN_SPAN.index),
            all_margins: margin(LayoutAttached::MARGINS.index),
            side_margins: [
                margin(LayoutAttached::LEFT_MARGIN.index),
                margin(LayoutAttached::TOP_MARGIN.index),
                margin(LayoutAttached::RIGHT_MARGIN.index),
                margin(LayoutAttached::BOTTOM_MARGIN.index),
            ],
        }
    }

    fn into_attached(self, fallback: Option<&Self>) -> Attached {
        let fb = fallback.copied().unwrap_or_default();
        let side = |i: usize| {
            self.side_margins[i]
                .or(self.all_margins)
                .or(fb.side_margins[i])
                .or(fb.all_margins)
                .unwrap_or(0.0)
        };
        Attached {
            fill: [self.fill[H].or(fb.fill[H]), self.fill[V].or(fb.fill[V])],
            pref: [self.pref[H].or(fb.pref[H]), self.pref[V].or(fb.pref[V])],
            min: [self.min[H].or(fb.min[H]), self.min[V].or(fb.min[V])],
            max: [self.max[H].or(fb.max[H]), self.max[V].or(fb.max[V])],
            stretch: [
                self.stretch[H].or(fb.stretch[H]).unwrap_or(-1),
                self.stretch[V].or(fb.stretch[V]).unwrap_or(-1),
            ],
            align: self.align.or(fb.align).unwrap_or(0),
            row: self.row,
            column: self.column,
            row_span: self.row_span.max(1),
            column_span: self.column_span.max(1),
            margins: [side(0), side(1), side(2), side(3)],
        }
    }
}

pub(crate) fn init_proxy(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let already = e.with_mut(me, |p: &mut LayoutItemProxy| {
        std::mem::replace(&mut p.state.initialized, true)
    })?;
    if already {
        return Ok(());
    }
    if let Some(att) = e.existing_attached_object(me, &attacher()) {
        setup_proxy_attached(e, me, att);
    }
    for (imp, target_imp) in [
        (Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_WIDTH.index),
        (Item::IMPLICIT_HEIGHT.index, Item::IMPLICIT_HEIGHT.index),
    ] {
        if e.untrack(|| is_unset(e, me, imp)) {
            // Losing the target keeps the last size (probed on 6.11).
            let last = Cell::new(0.0);
            e.bind_computed(me, imp, move |e| {
                match e.read(me, LayoutItemProxy::TARGET.index).as_object() {
                    Some(t) if e.is_alive(t) => {
                        let v = e.read(t, target_imp).to_number();
                        last.set(v);
                        Value::Real(v)
                    }
                    _ => Value::Real(last.get()),
                }
            })?;
        }
    }
    e.effect_in(me, move |e| {
        if !e.is_alive(me) {
            return;
        }
        // Only these are tracked. Qt reacts to the proxy's own target,
        // visibility and size; it never pulls the target back when
        // something else reparents or moves it (probed on 6.11).
        let target = e
            .read(me, LayoutItemProxy::TARGET.index)
            .as_object()
            .filter(|t| e.is_alive(*t));
        let vis = effective_visible(e, me);
        let w = num(e, me, Item::WIDTH.index);
        let h = num(e, me, Item::HEIGHT.index);
        e.untrack(|| {
            if let Err(err) = update_proxy(e, me, target, vis, w, h) {
                e.log(
                    LogLevel::Warning,
                    "qml.layouts",
                    format!("LayoutItemProxy update failed: {err}"),
                );
            }
        });
    })?;
    Ok(())
}

/// Whether `proxy` and all its visual ancestors have `visible: true`.
/// Reads `VISIBLE` and `PARENT` along the chain so calling this inside an
/// effect subscribes to visibility and reparenting changes up the tree.
fn effective_visible(e: &Engine, mut cur: ObjectId) -> bool {
    let mut vis = true;
    loop {
        if !e.is_alive(cur) {
            return false;
        }
        if !e.read(cur, Item::VISIBLE.index).to_boolean() {
            vis = false;
        }
        let _ = e.read(cur, Item::PARENT.index);
        match e.visual_parent(cur) {
            Some(p) => cur = p,
            None => return vis,
        }
    }
}

/// Untracked [`effective_visible`], for decisions about other proxies.
fn shown(e: &Engine, p: ObjectId) -> bool {
    e.untrack(|| effective_visible(e, p))
}

/// Reacts to what changed since the last run. Qt's proxy is event driven:
/// a proxy takes its target when it gets one or becomes visible, unless
/// another visible proxy of that target holds it; it gives the target away
/// only when it is hidden; and it moves the target only while holding it.
fn update_proxy(
    e: &Engine,
    me: ObjectId,
    target: Option<ObjectId>,
    visible: bool,
    w: f64,
    h: f64,
) -> Result<(), EngineError> {
    let (old_target, was_visible) = e.with_mut(me, |p: &mut LayoutItemProxy| {
        (
            std::mem::replace(&mut p.state.registered_target, target),
            p.state.was_visible.replace(visible),
        )
    })?;
    if old_target != target {
        if let Some(old) = old_target.filter(|o| e.is_alive(*o)) {
            unregister_proxy_from_target(e, me, old)?;
            // A retargeted proxy keeps its old target as a child unless
            // another visible proxy of it can take it.
            if e.visual_parent(old) == Some(me)
                && let Some(next) = visible_proxy_of(e, old, me)
            {
                take(e, next, old)?;
            }
        }
        if let Some(t) = target {
            register_proxy_on_target(e, me, t)?;
            if visible {
                grab(e, me, t)?;
            } else if held_by_visible_proxy(e, t).is_none() {
                // Nothing shows it: Qt hides it where it is.
                write_placed(e, t, Item::VISIBLE.index, Value::Bool(false))?;
            }
        }
        if let Some(parent) = e.visual_parent(me)
            && super::kind(e, parent).is_some()
        {
            let _ = e.notify(parent, Item::CHILDREN.index);
        }
    } else if let Some(t) = target
        && was_visible != Some(visible)
    {
        if visible {
            grab(e, me, t)?;
        } else if e.visual_parent(t) == Some(me) {
            match visible_proxy_of(e, t, me) {
                Some(next) => take(e, next, t)?,
                None => {
                    set_parent_item(e, t, None)?;
                    // Unparenting runs the other proxies' effects; one
                    // that took the target meanwhile has shown it.
                    if e.visual_parent(t).is_none() {
                        hide_own(e, t)?;
                    }
                }
            }
        }
    }
    if let Some(t) = target
        && e.visual_parent(t) == Some(me)
    {
        place_target(e, t, w, h)?;
    }
    Ok(())
}

/// Hides `target`, which no proxy shows, in its own `visible`. Taken out
/// from under a hidden proxy it may still read the false the proxy
/// passed down, until the scene has it inherit again: an assignment is
/// then its own value (scene.rs, `inherit_one`), where a placing write
/// would go under what it reads and be lost.
fn hide_own(e: &Engine, target: ObjectId) -> Result<(), EngineError> {
    if e.untrack(|| e.read(target, Item::VISIBLE.index).to_boolean()) {
        write_placed(e, target, Item::VISIBLE.index, Value::Bool(false))
    } else {
        e.write_internal(
            target,
            Item::VISIBLE.index,
            Value::Bool(false),
            WriteKind::Imperative,
        )
    }
}

/// The visible proxy that currently holds `target`, if any.
fn held_by_visible_proxy(e: &Engine, target: ObjectId) -> Option<ObjectId> {
    e.visual_parent(target).filter(|p| {
        is_a::<LayoutItemProxy>(e, *p)
            && e.read(*p, LayoutItemProxy::TARGET.index).as_object() == Some(target)
            && shown(e, *p)
    })
}

/// The first visible proxy of `target` other than `except`, in the order
/// the proxies took `target` on.
fn visible_proxy_of(e: &Engine, target: ObjectId, except: ObjectId) -> Option<ObjectId> {
    live_proxies(e, target)
        .into_iter()
        .find(|p| *p != except && shown(e, *p))
}

/// `me` takes `target` unless another visible proxy of it holds it.
fn grab(e: &Engine, me: ObjectId, target: ObjectId) -> Result<(), EngineError> {
    match held_by_visible_proxy(e, target) {
        Some(holder) if holder != me => Ok(()),
        _ => take(e, me, target),
    }
}

/// Reparents `target` to `proxy`, shows it and gives it the proxy's
/// geometry.
fn take(e: &Engine, proxy: ObjectId, target: ObjectId) -> Result<(), EngineError> {
    if e.visual_parent(target) != Some(proxy) {
        set_parent_item(e, target, Some(proxy))?;
    }
    write_placed(e, target, Item::VISIBLE.index, Value::Bool(true))?;
    let w = num(e, proxy, Item::WIDTH.index);
    let h = num(e, proxy, Item::HEIGHT.index);
    place_target(e, target, w, h)
}

fn register_proxy_on_target(
    e: &Engine,
    proxy: ObjectId,
    target: ObjectId,
) -> Result<(), EngineError> {
    let ta = e.attached_object(target, &attacher())?;
    e.with_mut(ta, |a: &mut LayoutAttached| {
        if !a.proxy.proxies.contains(&proxy) {
            a.proxy.proxies.push(proxy);
        }
    })
}

fn unregister_proxy_from_target(
    e: &Engine,
    proxy: ObjectId,
    target: ObjectId,
) -> Result<(), EngineError> {
    if let Some(ta) = e.existing_attached_object(target, &attacher()) {
        e.with_mut(ta, |a: &mut LayoutAttached| {
            a.proxy.proxies.retain(|p| *p != proxy);
        })?;
    }
    Ok(())
}

/// The proxies registered on `target` that are alive and still target it.
fn live_proxies(e: &Engine, target: ObjectId) -> Vec<ObjectId> {
    let Some(ta) = e.existing_attached_object(target, &attacher()) else {
        return Vec::new();
    };
    // The store is borrowed inside `with`, so the liveness and target
    // checks happen outside it.
    let mut proxies = e
        .with(ta, |a: &LayoutAttached| a.proxy.proxies.clone())
        .unwrap_or_default();
    proxies.retain(|p| {
        e.is_alive(*p) && e.read(*p, LayoutItemProxy::TARGET.index).as_object() == Some(target)
    });
    proxies
}

fn place_target(e: &Engine, target: ObjectId, w: f64, h: f64) -> Result<(), EngineError> {
    write_placed(e, target, Item::X.index, Value::Real(0.0))?;
    write_placed(e, target, Item::Y.index, Value::Real(0.0))?;
    write_placed(e, target, Item::WIDTH.index, Value::Real(w))?;
    write_placed(e, target, Item::HEIGHT.index, Value::Real(h))?;
    Ok(())
}

/// Called by `arrange` in `layouts/mod.rs` right after a layout places a
/// child `c` at width `w` and height `h`: Qt resizes the held target in
/// the same turn as the proxy.
pub(crate) fn sync_placed_proxy(
    e: &Engine,
    c: ObjectId,
    w: f64,
    h: f64,
) -> Result<(), EngineError> {
    if !is_a::<LayoutItemProxy>(e, c) {
        return Ok(());
    }
    let Some(target) = e.read(c, LayoutItemProxy::TARGET.index).as_object() else {
        return Ok(());
    };
    if e.is_alive(target) && e.visual_parent(target) == Some(c) {
        place_target(e, target, w, h)?;
    }
    Ok(())
}
