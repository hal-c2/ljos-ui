//! The item that draws a popup.
//!
//! It is a `Page` underneath: a page already places a background, a
//! content item inside the padding and a header and a footer. The popup
//! hands it its properties with bindings and reads back what the page
//! works out. It takes its font, palette and locale from the popup, as a
//! control does from the item it is in ([`crate::inherit`]). QML cannot
//! name the type.

use ljos_a11y::Role;
use ljos_engine::{
    Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlObject, QmlType, Value, WriteKind,
};
use ljos_quick::focus::{
    FocusReason, active_focus_item, force_active_focus, next_item_in_focus_chain,
    register_focus_scope,
};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{
    Accessible, AccessibleAttached, Item, KeyEvent, PointerEvent, PointerItem,
    register_pointer_item,
};
use ljos_render::kurbo;

use super::{Popup, popup_of};
use crate::containers::{Page, Pane};
use crate::control::{Control, ignore, no_key};
use crate::util::{flag, set_derived};

/// The item of a popup.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "PopupItem", extends = Page)]
pub struct PopupItem {
    #[qml(base)]
    pub page: Page,
}

/// Makes `to` of the popup's item follow `from` of the popup.
pub(crate) fn follow(
    engine: &Engine,
    item: ObjectId,
    to: PropIndex,
    popup: ObjectId,
    from: PropIndex,
) -> Result<(), EngineError> {
    engine.bind(item, to, move |e| e.read(popup, from))
}

/// Makes the item of `popup`, hidden and outside every tree.
pub(super) fn create(engine: &Engine, popup: ObjectId) -> Result<ObjectId, EngineError> {
    let item = engine.create_native_incomplete(PopupItem::default(), Some(popup))?;
    engine.write(
        item,
        Item::VISIBLE.index,
        Value::Bool(false),
        WriteKind::Imperative,
    )?;
    for (to, from) in [
        (Control::CONTENT_ITEM.index, Popup::CONTENT_ITEM.index),
        (Control::BACKGROUND.index, Popup::BACKGROUND.index),
        (Control::PADDING.index, Popup::PADDING.index),
        (Control::TOP_PADDING.index, Popup::TOP_PADDING.index),
        (Control::LEFT_PADDING.index, Popup::LEFT_PADDING.index),
        (Control::RIGHT_PADDING.index, Popup::RIGHT_PADDING.index),
        (Control::BOTTOM_PADDING.index, Popup::BOTTOM_PADDING.index),
        (
            Control::HORIZONTAL_PADDING.index,
            Popup::HORIZONTAL_PADDING.index,
        ),
        (
            Control::VERTICAL_PADDING.index,
            Popup::VERTICAL_PADDING.index,
        ),
        (Control::TOP_INSET.index, Popup::TOP_INSET.index),
        (Control::LEFT_INSET.index, Popup::LEFT_INSET.index),
        (Control::RIGHT_INSET.index, Popup::RIGHT_INSET.index),
        (Control::BOTTOM_INSET.index, Popup::BOTTOM_INSET.index),
        (Control::SPACING.index, Popup::SPACING.index),
        (Pane::CONTENT_WIDTH.index, Popup::CONTENT_WIDTH.index),
        (Pane::CONTENT_HEIGHT.index, Popup::CONTENT_HEIGHT.index),
        (Item::WIDTH.index, Popup::WIDTH.index),
        (Item::HEIGHT.index, Popup::HEIGHT.index),
        (Item::IMPLICIT_WIDTH.index, Popup::IMPLICIT_WIDTH.index),
        (Item::IMPLICIT_HEIGHT.index, Popup::IMPLICIT_HEIGHT.index),
        (Item::Z.index, Popup::Z.index),
        (Item::OPACITY.index, Popup::OPACITY.index),
        (Item::SCALE.index, Popup::SCALE.index),
        (Item::TRANSFORM_ORIGIN.index, Popup::TRANSFORM_ORIGIN.index),
        (Item::ENABLED.index, Popup::ENABLED.index),
        (Item::CLIP.index, Popup::CLIP.index),
    ] {
        follow(engine, item, to, popup, from)?;
    }
    engine.complete(item)?;
    role_of_item(engine, item, Role::Dialog)?;
    Ok(item)
}

/// Says what the popup is to an assistive technology. A page calls itself
/// a tab, which a popup is not.
pub(crate) fn set_role(engine: &Engine, popup: ObjectId, role: Role) -> Result<(), EngineError> {
    match super::item_of(engine, popup) {
        Some(item) => role_of_item(engine, item, role),
        None => Ok(()),
    }
}

fn role_of_item(engine: &Engine, item: ObjectId, role: Role) -> Result<(), EngineError> {
    let att = engine.attached_object(item, &MetaRef::Static(Accessible::static_meta()))?;
    engine.write_derived(
        att,
        AccessibleAttached::ROLE.index,
        Value::Int(role.qt_value()),
    )
}

/// Tells an assistive technology that nothing outside the popup can be
/// reached while it is shown and modal.
pub(super) fn set_modal(
    engine: &Engine,
    popup: ObjectId,
    item: ObjectId,
) -> Result<(), EngineError> {
    let modal = super::is_shown(engine, popup) && flag(engine, popup, Popup::MODAL.index);
    let att = engine.attached_object(item, &MetaRef::Static(Accessible::static_meta()))?;
    set_derived(
        engine,
        att,
        AccessibleAttached::MODAL.index,
        Value::Bool(modal),
    )
}

// ------------------------------------------------------------------ input

fn press(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) -> bool {
    // A popup is opaque to the pointer: neither the content nor a popup
    // beneath gets a press that lands on it (checked on qml6).
    true
}

/// A drawer that has the pointer follows it from the overlay, which sees
/// each move first; what is left here is to let go.
fn release(engine: &Engine, item: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
    super::drawer_drag::released(engine, item, ev);
}

fn cancel(engine: &Engine, item: ObjectId) {
    super::drawer_drag::canceled(engine, item);
}

fn key_press(engine: &Engine, item: ObjectId, ev: &KeyEvent) -> bool {
    let Some(popup) = popup_of(engine, item) else {
        return false;
    };
    // A menu's keys go by its items, not by the focus chain.
    if super::kind_of(engine, popup).key_press(engine, popup, ev) {
        return true;
    }
    match ev.key {
        key::ESCAPE => super::escape(engine, popup),
        key::TAB | key::BACKTAB => {
            // Tab stays in the popup that has the keyboard. Nothing in it
            // stopping Tab is no reason to hand the key to the content.
            let forward = ev.key == key::TAB;
            let top = ljos_quick::window_item(engine, item);
            let from = active_focus_item(engine, top).unwrap_or(item);
            let next = next_item_in_focus_chain(engine, from, forward);
            if next != from && is_inside(engine, next, item) {
                let reason = if forward {
                    FocusReason::Tab
                } else {
                    FocusReason::Backtab
                };
                force_active_focus(engine, next, reason);
            }
            true
        }
        _ => false,
    }
}

/// Whether `obj` is `ancestor` or inside it.
pub(crate) fn is_inside(engine: &Engine, obj: ObjectId, ancestor: ObjectId) -> bool {
    engine.untrack(|| {
        let mut cur = Some(obj);
        for _ in 0..crate::util::DEPTH {
            match cur {
                Some(o) if o == ancestor => return true,
                Some(o) => cur = engine.visual_parent(o),
                None => return false,
            }
        }
        false
    })
}

pub(super) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(PopupItem::static_meta());
    register_focus_scope(engine, meta());
    register_pointer_item(
        engine,
        meta(),
        PointerItem {
            press,
            moved: ignore,
            release,
            cancel,
            hover: crate::control::hover,
        },
    );
    register_key_item(
        engine,
        meta(),
        KeyItem {
            shortcut_override: no_key,
            press: key_press,
            release: no_key,
        },
    );
}
