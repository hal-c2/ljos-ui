//! The overlay of a window: the item above its content that the popups
//! are in, and `Overlay`, the attached type that names it.
//!
//! There is one per window, made when the first popup or the first
//! `Overlay.overlay` needs it: for a `Window`, that can be while its tree
//! is still made, before a scene shows it. It is
//! the layer of its scene (`ljos_quick::set_layer`): drawn over the root
//! item, asked first by the pointer, and not among the root's children,
//! as the root may be a positioner; its `parent` reads the root, as in Qt.
//! Its children are the items of the popups in the
//! order they were opened, each shade right before the popup it is for.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-overlay.html and
//! the `closePolicy` of doc.qt.io/qt-6/qml-qtquick-controls-popup.html;
//! pinned by `features/controls/overlay.feature`, `overlay-window.feature`,
//! `popup-close-policy.feature` and `popup-pointer.feature`.

use ljos_engine::{
    Color, Engine, EngineError, LogLevel, MetaRef, ObjectId, QmlObject, QmlType, Value, WriteKind,
};
use ljos_quick::{
    Item, LayerHooks, PointerEvent, Rectangle, child_items, contains_point, is_item,
    scene_transform, set_parent_item,
};
use ljos_render::kurbo;

use super::item::is_inside;
use super::{Popup, PopupState, close_policy, dismiss, item_of, popup_of, state_of};
use crate::util::{CATEGORY, check, flag, is_a, num, object, set_computed};

/// The overlay is above everything a window's content can be (the number
/// is the one Qt's reads).
const OVERLAY_Z: f64 = 1_000_001.0;
/// The shade behind a modal popup, and behind one that only dims.
const MODAL_SHADE: Color = Color::from_rgba8(0x28, 0x28, 0x2a, 0x7f);
const MODELESS_SHADE: Color = Color::from_rgba8(0x28, 0x28, 0x2a, 0x1e);

/// `Overlay` (QtQuick.Controls): only ever used attached, as
/// `Overlay.overlay` on an item and `Overlay.modal` on a popup.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Overlay",
    attached = OverlayAttached,
    uncreatable
)]
pub struct Overlay {}

/// What `Overlay.` attaches to an item or a popup.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "OverlayAttached")]
pub struct OverlayAttached {
    /// The overlay of the window the item is in.
    #[qml(notify, readonly)]
    pub overlay: Option<ObjectId>,
    /// The component of the shade behind the popup while it is modal.
    #[qml(notify, object = "Component")]
    pub modal: Option<ObjectId>,
    /// The same for a popup that dims without being modal.
    #[qml(notify, object = "Component")]
    pub modeless: Option<ObjectId>,
}

/// The overlay itself. QML reaches it as `Overlay.overlay`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "OverlayItem", extends = Item)]
pub struct OverlayItem {
    #[qml(base)]
    pub item: Item,
    /// Where the pointer was last pressed or released, in the window.
    #[qml(skip)]
    pointer: std::cell::Cell<Option<(f64, f64)>>,
}

fn remember_pointer(engine: &Engine, overlay: ObjectId, at: kurbo::Point) {
    let _ = engine.with::<OverlayItem, _>(overlay, |o| o.pointer.set(Some((at.x, at.y))));
}

/// Where the pointer was last pressed or released in the window `popup`
/// is in: what a menu that is asked to open "here" goes by.
pub(crate) fn last_pointer(engine: &Engine, popup: ObjectId) -> Option<kurbo::Point> {
    let overlay = engine.visual_parent(super::item_of(engine, popup)?)?;
    engine
        .with::<OverlayItem, _>(overlay, |o| o.pointer.get())
        .ok()
        .flatten()
        .map(|(x, y)| kurbo::Point::new(x, y))
}

/// The overlay of the window whose top item is `top`, made if there is
/// none.
pub(super) fn ensure(engine: &Engine, top: ObjectId) -> Result<ObjectId, EngineError> {
    engine.untrack(|| {
        if let Some(layer) = ljos_quick::layer_of(engine, top) {
            return Ok(layer);
        }
        let o = engine.create_native_incomplete(OverlayItem::default(), Some(top))?;
        // Before anything is written: a write runs the effects of other
        // popups, and one that looked for the overlay now would make a
        // second, which nothing would ever show.
        ljos_quick::set_layer(engine, top, o);
        for (prop, v) in [
            (Item::Z.index, Value::Real(OVERLAY_Z)),
            (Item::VISIBLE.index, Value::Bool(false)),
            // The scene keeps it as large as the view; until its first
            // frame the window's own size is the best guess.
            (
                Item::WIDTH.index,
                Value::Real(num(engine, top, Item::WIDTH.index)),
            ),
            (
                Item::HEIGHT.index,
                Value::Real(num(engine, top, Item::HEIGHT.index)),
            ),
        ] {
            engine.write(o, prop, v, WriteKind::Imperative)?;
        }
        engine.complete(o)?;
        // QML reads the window's item as its parent, as in Qt, where it
        // is that item's last child; here it stays out of the item's
        // `children`, which a positioner or a layout would place it by.
        // After completion, which would make the declared parent the
        // item's parent in the tree too.
        engine.write_internal(
            o,
            Item::PARENT.index,
            Value::Object(top),
            WriteKind::Imperative,
        )?;
        // A popup that is destroyed while open leaves without a word.
        engine.effect_in(o, move |e| {
            let _ = e.read(o, Item::CHILDREN.index);
            e.untrack(|| changed(e, o));
        })?;
        Ok(o)
    })
}

/// The shown popups of an overlay, the lowest first: by `z`, and in the
/// order they were opened among equals.
pub(super) fn stack(engine: &Engine, overlay: ObjectId) -> Vec<(ObjectId, ObjectId)> {
    engine.untrack(|| {
        let mut shown: Vec<(f64, ObjectId, ObjectId)> = child_items(engine, overlay)
            .into_iter()
            .filter_map(|item| {
                let popup = popup_of(engine, item)?;
                super::is_shown(engine, popup)
                    .then(|| (num(engine, item, Item::Z.index), item, popup))
            })
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        shown.into_iter().map(|(_, i, p)| (i, p)).collect()
    })
}

/// A popup was shown or hidden: the overlay is there while one is shown,
/// and while the window has a drawer, which can be dragged in at any time
/// (measured on qml6: so it is with one that is not interactive too).
pub(super) fn changed(engine: &Engine, overlay: ObjectId) {
    if !engine.is_alive(overlay) {
        return;
    }
    let any = !stack(engine, overlay).is_empty() || has_drawer(engine, overlay);
    check(
        engine,
        "overlay",
        set_computed(engine, overlay, Item::VISIBLE.index, Value::Bool(any)),
    );
}

fn has_drawer(engine: &Engine, overlay: ObjectId) -> bool {
    engine.untrack(|| {
        child_items(engine, overlay)
            .into_iter()
            .filter_map(|item| popup_of(engine, item))
            .any(|popup| super::drawer::is_drawer(engine, popup))
    })
}

/// Makes the shade of `popup`, if it has one, as a child of `overlay`.
fn make_shade(
    engine: &Engine,
    popup: ObjectId,
    overlay: ObjectId,
) -> Result<Option<ObjectId>, EngineError> {
    if !flag(engine, popup, Popup::DIM.index) {
        return Ok(None);
    }
    let modal = flag(engine, popup, Popup::MODAL.index);
    let component = engine
        .existing_attached_object(popup, &MetaRef::Static(Overlay::static_meta()))
        .and_then(|att| {
            let prop = if modal {
                OverlayAttached::MODAL.index
            } else {
                OverlayAttached::MODELESS.index
            };
            object(engine, att, prop)
        });
    let shade = match component {
        Some(c) => engine.component_create(c, Some(popup), Vec::new())?,
        None => {
            let r = engine.create_native_incomplete(Rectangle::default(), Some(popup))?;
            let colour = if modal { MODAL_SHADE } else { MODELESS_SHADE };
            engine.set(r, Rectangle::COLOR, colour)?;
            engine.complete(r)?;
            r
        }
    };
    if !is_item(engine, shade) {
        engine.destroy(shade);
        return Ok(None);
    }
    for size in [Item::WIDTH.index, Item::HEIGHT.index] {
        engine.bind(shade, size, move |e| e.read(overlay, size))?;
    }
    engine.bind(shade, Item::Z.index, move |e| e.read(popup, Popup::Z.index))?;
    if super::drawer::is_drawer(engine, popup) {
        // The shade fades in as the drawer comes in.
        engine.bind(shade, Item::OPACITY.index, move |e| {
            super::drawer::shade_opacity(e, popup)
        })?;
    }
    set_parent_item(engine, shade, Some(overlay))?;
    Ok(Some(shade))
}

fn drop_shade(engine: &Engine, state: &PopupState) -> Result<(), EngineError> {
    if let Some(old) = state.dimmer.take().filter(|d| engine.is_alive(*d)) {
        // Out of the overlay now; the object goes at the next pass.
        set_parent_item(engine, old, None)?;
        engine.destroy(old);
    }
    Ok(())
}

/// Puts a popup that is being shown above the others, behind a new shade
/// if it has one.
pub(super) fn raise(
    engine: &Engine,
    popup: ObjectId,
    state: &PopupState,
    item: ObjectId,
    overlay: ObjectId,
) -> Result<(), EngineError> {
    drop_shade(engine, state)?;
    let shade = make_shade(engine, popup, overlay)?;
    state.dimmer.set(shade);
    let last = child_items(engine, overlay).last().copied();
    if last != Some(item) {
        // To the end of the overlay's children: there is no other way to
        // move an item among its siblings.
        engine.batch(|| {
            set_parent_item(engine, item, None)?;
            set_parent_item(engine, item, Some(overlay))
        })?;
    }
    Ok(())
}

/// `modal` or `dim` changed: a shown popup gets the shade that goes with
/// what it is now.
pub(super) fn reshade(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    if !state.shown.get() {
        return Ok(());
    }
    engine.untrack(|| {
        let Some(overlay) = engine.visual_parent(item) else {
            return Ok(());
        };
        drop_shade(engine, &state)?;
        if let Some(shade) = make_shade(engine, popup, overlay)? {
            state.dimmer.set(Some(shade));
            // The shade goes right before the popup it is for.
            let children = child_items(engine, overlay);
            let after: Vec<ObjectId> = children
                .iter()
                .copied()
                .skip_while(|c| *c != item)
                .filter(|c| *c != shade)
                .collect();
            if after.first() == Some(&item) && after.len() == 1 {
                // The popup is the top one, where a shade made later is
                // above it: the popup goes over it again. Its focus goes
                // along (`ljos_quick` moves a focus item with its scope).
                engine.batch(|| {
                    set_parent_item(engine, item, None)?;
                    set_parent_item(engine, item, Some(overlay))
                })?;
            } else {
                for c in after {
                    engine.batch(|| {
                        set_parent_item(engine, c, None)?;
                        set_parent_item(engine, c, Some(overlay))
                    })?;
                }
            }
        }
        super::item::set_modal(engine, popup, item)
    })
}

// ---------------------------------------------------------------- pointer

pub(super) fn point_in(engine: &Engine, item: ObjectId, scene: kurbo::Point) -> bool {
    let local = engine.untrack(|| scene_transform(engine, item)).inverse() * scene;
    contains_point(engine, item, local)
}

fn policy(engine: &Engine, popup: ObjectId) -> i32 {
    engine
        .untrack(|| engine.read(popup, Popup::CLOSE_POLICY.index))
        .to_number() as i32
}

pub(super) fn is_modal(engine: &Engine, popup: ObjectId) -> bool {
    engine.untrack(|| flag(engine, popup, Popup::MODAL.index))
}

fn outside_parent(engine: &Engine, popup: ObjectId, at: kurbo::Point) -> bool {
    let parent = engine.untrack(|| object(engine, popup, Popup::PARENT.index));
    parent
        .filter(|p| is_item(engine, *p))
        .is_none_or(|p| !point_in(engine, p, at))
}

/// What is beneath the topmost modal popup does not get the pointer.
fn filter(engine: &Engine, overlay: ObjectId, hits: &mut Vec<ObjectId>) {
    let stack = stack(engine, overlay);
    let Some(first) = stack.iter().rposition(|(_, p)| is_modal(engine, *p)) else {
        return;
    };
    let reachable = &stack[first..];
    hits.retain(|h| {
        reachable
            .iter()
            .any(|(item, _)| is_inside(engine, *h, *item))
    });
}

/// Walks the shown popups from the top one down to the first the point
/// is in, or the first modal one: those are the ones a press or release
/// beside them can close.
fn beside(engine: &Engine, overlay: ObjectId, at: kurbo::Point, mut f: impl FnMut(ObjectId)) {
    for (item, popup) in stack(engine, overlay).into_iter().rev() {
        if !engine.is_alive(popup) || !super::is_shown(engine, popup) {
            // Closed by a handler of the one above.
            continue;
        }
        let inside = point_in(engine, item, at);
        let modal = is_modal(engine, popup);
        if !inside {
            f(popup);
        } else if let Some(s) = state_of(engine, popup) {
            s.pressed_outside.set(false);
        }
        if inside || modal {
            break;
        }
    }
}

fn press(engine: &Engine, overlay: ObjectId, ev: &PointerEvent) {
    let at = ev.pos;
    remember_pointer(engine, overlay, at);
    beside(engine, overlay, at, |popup| {
        if let Some(s) = state_of(engine, popup) {
            s.pressed_outside.set(true);
        }
        let policy = policy(engine, popup);
        if policy & close_policy::ON_PRESS_OUTSIDE != 0
            || (policy & close_policy::ON_PRESS_OUTSIDE_PARENT != 0
                && outside_parent(engine, popup, at))
        {
            check(engine, "close", dismiss(engine, popup));
        }
    });
    super::drawer_drag::pressed(engine, overlay, ev);
}

fn release(engine: &Engine, overlay: ObjectId, ev: &PointerEvent) {
    let at = ev.pos;
    remember_pointer(engine, overlay, at);
    super::drawer_drag::let_go(engine);
    beside(engine, overlay, at, |popup| {
        // Only what was pressed beside is released beside: a drag that
        // began on the popup and ends off it closes nothing.
        let began_outside =
            state_of(engine, popup).is_some_and(|s| s.pressed_outside.replace(false));
        if !began_outside {
            return;
        }
        let policy = policy(engine, popup);
        if policy & close_policy::ON_RELEASE_OUTSIDE != 0
            || (policy & close_policy::ON_RELEASE_OUTSIDE_PARENT != 0
                && outside_parent(engine, popup, at))
        {
            check(engine, "close", dismiss(engine, popup));
        }
    });
}

/// The top item of the window whose overlay `item` sees: the tree a scene
/// shows it in, or else, while none does yet, the top of its tree when
/// that is a `Window`. Qt makes a window's items in the window, so its
/// overlay is there before their bindings first run; here a scene or a
/// host takes the window only once its tree is made. An item tree with no
/// `Window` above it has no window until a scene shows it, as in Qt.
///
/// An item in a popup, open or not, is in the window of the popup's
/// parent. The walk goes there rather than up from the popup's item: that
/// item is moved into the overlay by the popup's own effect once the
/// parent has a window, and following it would run this again in the same
/// update. Tracks the parents, so a window moved later (one made loose and
/// put in a shown tree) is followed.
fn window_top(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    let mut cur = item;
    for _ in 0..1024 {
        if let Some(popup) = popup_of(engine, cur) {
            cur = object(engine, popup, Popup::PARENT.index)
                .or_else(|| super::default_parent(engine, popup))
                .filter(|p| is_item(engine, *p))?;
            continue;
        }
        match engine.visual_parent(cur) {
            Some(p) => cur = p,
            None => {
                return ljos_quick::window_of(engine, cur)
                    .or_else(|| is_a::<crate::Window>(engine, cur).then_some(cur));
            }
        }
    }
    None
}

pub(super) fn register(engine: &Engine) {
    ljos_quick::set_layer_hooks(
        engine,
        LayerHooks {
            filter,
            press,
            release,
        },
    );
    ljos_quick::set_layer_take(engine, super::drawer_drag::take);
    engine.on_attached_created_of::<OverlayAttached>(|e, owner, att| {
        let bound = e.bind_initial(att, OverlayAttached::OVERLAY.index, move |e| {
            // On a popup it is the overlay the popup is shown in. Its
            // parent is peeked, not read: `parent: Overlay.overlay` is
            // this binding's reader. A popup with no parent yet (before
            // there is a window) goes by where it is declared, so the
            // window that appears there is what gives it one. So does one
            // whose parent is the overlay this gave it: that says nothing
            // of where it is now, and a window taken out of the tree it
            // was declared in (by a host) has an overlay of its own.
            let from = if is_a::<Popup>(e, owner) {
                let given = e.peek(att, OverlayAttached::OVERLAY.index).as_object();
                e.peek(owner, Popup::PARENT.index)
                    .as_object()
                    .filter(|p| e.is_alive(*p) && Some(*p) != given)
                    .or_else(|| super::default_parent(e, owner))
            } else {
                Some(owner)
            };
            from.filter(|f| is_item(e, *f))
                .and_then(|f| window_top(e, f))
                .and_then(|top| ensure(e, top).ok())
                .map_or(Value::Null, Value::Object)
        });
        if let Err(err) = bound {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("Overlay: cannot bind the attached properties: {err}"),
            );
        }
        true
    });
}
