//! The keyboard modifiers the application sees as held, for native code
//! that acts on a click or a key and has no event at hand: an action run
//! from a QML handler that does something else with Shift held.
//!
//! Every key, pointer and wheel event the scene is given says which
//! modifiers are held (`QuickScene::key_event`, `pointer_event`,
//! `wheel_event`), and the last one is what is seen, so a click with Shift
//! held is Shift though no key event for Shift came. Events that say
//! nothing of the keyboard leave it as it was: touches, a press by an
//! assistive technology, a pointer cancelled or gone from the window, and
//! the keys a host makes of typed text (`QuickScene::key_event_from_text`).
//! See `features/input/keyboard-modifiers.feature`.

use std::cell::Cell;

use ljos_engine::Engine;

use crate::input::{KeyEvent, KeyKind, Modifiers, PointerEvent, PointerKind};

use super::key;

#[derive(Default)]
struct Held(Cell<Modifiers>);

/// The modifiers held, as the last key, pointer or wheel event the
/// application was given said: Shift, Ctrl, Alt and Meta, never the
/// keypad's flag, which is of a key and not held.
pub fn keyboard_modifiers(engine: &Engine) -> Modifiers {
    engine.extension::<Held>().0.get()
}

/// Says which modifiers are held, for a host that learns of them without
/// an event to give the scene: a window that gets the keyboard back with
/// Shift already down, or a pass whose input changed only the modifiers.
pub fn set_keyboard_modifiers(engine: &Engine, held: Modifiers) {
    engine.extension::<Held>().0.set(held - Modifiers::KEYPAD);
}

/// A key event lists the modifiers held besides its own key, so a
/// modifier key's own goes on with its press and off with its release.
pub(crate) fn note_key(engine: &Engine, ev: &KeyEvent) {
    let own = match ev.key {
        key::SHIFT => Modifiers::SHIFT,
        key::CONTROL => Modifiers::CONTROL,
        key::ALT => Modifiers::ALT,
        key::META => Modifiers::META,
        _ => Modifiers::empty(),
    };
    let held = match ev.kind {
        KeyKind::Press => ev.modifiers | own,
        KeyKind::Release => ev.modifiers - own,
    };
    set_keyboard_modifiers(engine, held);
}

/// A cancel and a leave are made by the platform or the host with no
/// keyboard behind them.
pub(crate) fn note_pointer(engine: &Engine, ev: &PointerEvent) {
    match ev.kind {
        PointerKind::Press | PointerKind::Release | PointerKind::Move => {
            set_keyboard_modifiers(engine, ev.modifiers);
        }
        PointerKind::Cancel | PointerKind::Leave => {}
    }
}
