//! The item that draws a popup.
//!
//! It is a `Page` underneath: a page already places a background, a
//! content item inside the padding and a header and a footer, and already
//! takes its font and palette from what it is inside. The popup hands it
//! its properties with bindings and reads back what the page works out.
//! QML cannot name the type.

use std::sync::OnceLock;

use ljos_a11y::Role;
use ljos_engine::{
    Engine, EngineError, MetaRef, ObjectId, PropIndex, Provenance, QmlObject, QmlType, Reset,
    Value, WriteKind,
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

use super::{Popup, close_policy, dismiss, popup_of, put};
use crate::containers::{Page, Pane};
use crate::control::Control;
use crate::palette::{ROLE_COUNT, role_props};
use crate::util::{check, flag};

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
        (Control::LOCALE.index, Popup::LOCALE.index),
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
    put(
        engine,
        att,
        AccessibleAttached::MODAL.index,
        Value::Bool(modal),
    )
}

fn author_set(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(Provenance::Assigned | Provenance::Bound)
    )
}

fn popup_roles() -> &'static [PropIndex; ROLE_COUNT] {
    static ROLES: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    ROLES.get_or_init(|| role_props(Popup::static_meta()))
}

fn item_roles() -> &'static [PropIndex; ROLE_COUNT] {
    static ROLES: OnceLock<[PropIndex; ROLE_COUNT]> = OnceLock::new();
    ROLES.get_or_init(|| role_props(Control::static_meta()))
}

/// Keeps the popup's `font` and `palette` and its item's the same: what
/// the author sets on the popup goes to the item, and what the item takes
/// from the window is what the popup reads.
pub(super) fn mirror_look(
    engine: &Engine,
    popup: ObjectId,
    item: ObjectId,
) -> Result<(), EngineError> {
    let (mine, theirs) = (Popup::FONT.index, Control::FONT.index);
    let hand_down = move |e: &Engine, from: PropIndex, to: PropIndex, v: &Value| {
        if author_set(e, popup, from) && !e.untrack(|| e.read(item, to)).change_equals(v) {
            check(
                e,
                "popup",
                e.write(item, to, v.clone(), WriteKind::Imperative),
            );
        }
    };
    hand_down(
        engine,
        mine,
        theirs,
        &engine.untrack(|| engine.read(popup, mine)),
    );
    engine.watch(popup, mine, move |e, v| hand_down(e, mine, theirs, v))?;
    let (my_locale, their_locale) = (Popup::LOCALE.index, Control::LOCALE.index);
    hand_down(
        engine,
        my_locale,
        their_locale,
        &engine.untrack(|| engine.read(popup, my_locale)),
    );
    engine.watch(popup, my_locale, move |e, v| {
        hand_down(e, my_locale, their_locale, v);
    })?;
    engine.effect_in(popup, move |e| {
        let font = e.read(item, theirs);
        let locale = e.read(item, their_locale);
        e.untrack(|| {
            check(e, "font", put(e, popup, mine, font));
            if !author_set(e, popup, my_locale) {
                check(e, "locale", put(e, popup, my_locale, locale));
            }
        });
    })?;

    let (mine, theirs) = (popup_roles(), item_roles());
    for i in 0..ROLE_COUNT {
        let (from, to) = (mine[i], theirs[i]);
        hand_down(
            engine,
            from,
            to,
            &engine.untrack(|| engine.read(popup, from)),
        );
        engine.watch(popup, from, move |e, v| hand_down(e, from, to, v))?;
    }
    engine.effect_in(popup, move |e| {
        for i in 0..ROLE_COUNT {
            let colour = e.read(item, theirs[i]);
            e.untrack(|| {
                if !author_set(e, popup, mine[i]) {
                    check(e, "palette", put(e, popup, mine[i], colour));
                }
            });
        }
    })?;
    Ok(())
}

/// `font = undefined` and `locale = undefined` on a popup: its item hands
/// its own back, and the popup has what the item then has.
pub(super) const FONT_RESET: Reset = Reset::Custom {
    apply: take_back,
    value: from_above,
};
/// A popup has a locale from the start, before it has an item to take
/// one from: the application's.
pub(super) const LOCALE_RESET: Reset = Reset::CustomFromCreation {
    apply: take_back,
    value: locale_from_above,
};

/// The property of the popup's item that `prop` of the popup is kept the
/// same as.
fn of_item(popup_prop: PropIndex) -> Option<PropIndex> {
    if popup_prop == Popup::FONT.index {
        Some(Control::FONT.index)
    } else if popup_prop == Popup::LOCALE.index {
        Some(Control::LOCALE.index)
    } else {
        let i = popup_roles().iter().position(|p| *p == popup_prop)?;
        Some(item_roles()[i])
    }
}

fn from_above(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Value {
    look_from_above(engine, popup, prop).unwrap_or_else(|| engine.default_value(popup, prop))
}

fn locale_from_above(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Value {
    look_from_above(engine, popup, prop)
        .unwrap_or_else(|| crate::inherit::application_locale(engine))
}

/// What the popup's item would have in the property of a popup's look if
/// nobody gave it one (tracked). None for an object that is no popup, a
/// popup without an item, or another property.
pub(crate) fn look_from_above(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Option<Value> {
    if !crate::util::is_a::<Popup>(engine, popup) {
        return None;
    }
    let item = super::item_of(engine, popup)?;
    let theirs = of_item(prop)?;
    let reset = engine.reset_of(item, theirs)?;
    Some(match reset {
        Reset::Default => engine.default_value(item, theirs),
        Reset::Follow(value)
        | Reset::FollowFromCreation(value)
        | Reset::Custom { value, .. }
        | Reset::CustomFromCreation { value, .. }
        | Reset::Absent { value, .. } => value(engine, item, theirs),
    })
}

fn take_back(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
    match take_look_back(engine, popup, prop) {
        Some(done) => done,
        None => engine.reset_to_default(popup, prop),
    }
}

/// Hands a property of a popup's look back; None as [`look_from_above`].
pub(crate) fn take_look_back(
    engine: &Engine,
    popup: ObjectId,
    prop: PropIndex,
) -> Option<Result<(), EngineError>> {
    if !crate::util::is_a::<Popup>(engine, popup) {
        return None;
    }
    let item = super::item_of(engine, popup)?;
    let theirs = of_item(prop)?;
    Some(engine.reset(item, theirs).and_then(|()| {
        let now = engine.untrack(|| engine.read(item, theirs));
        engine.reset_to(popup, prop, now)
    }))
}

// ------------------------------------------------------------------ input

fn press(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) -> bool {
    // A popup is opaque to the pointer: neither the content nor a popup
    // beneath gets a press that lands on it (checked on qml6).
    true
}

fn ignore(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) {}

/// A drawer that has the pointer follows it from the overlay, which sees
/// each move first; what is left here is to let go.
fn release(engine: &Engine, item: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
    super::drawer_drag::released(engine, item, ev);
}

fn cancel(engine: &Engine, item: ObjectId) {
    super::drawer_drag::canceled(engine, item);
}

fn no_key(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

fn key_press(engine: &Engine, item: ObjectId, ev: &KeyEvent) -> bool {
    let Some(popup) = popup_of(engine, item) else {
        return false;
    };
    // A menu's keys go by its items, not by the focus chain.
    if crate::menu::key_press(engine, popup, ev) {
        return true;
    }
    match ev.key {
        key::ESCAPE => {
            let policy = engine.untrack(|| engine.read(popup, Popup::CLOSE_POLICY.index));
            if policy.to_number() as i32 & close_policy::ON_ESCAPE == 0 {
                return false;
            }
            check(engine, "close", dismiss(engine, popup));
            true
        }
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
        for _ in 0..4096 {
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
