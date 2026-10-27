//! What the loop reads of a window's input before the scene does.

use ljos_host::{ButtonState, PointerKind, WindowEvent};

/// What a modal window keeps from the windows behind it: everything a
/// user starts. Releases and the pointer leaving still arrive, so that
/// nothing stays held.
pub(super) fn starts_input(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Text(_) | WindowEvent::Touch(_) => true,
        WindowEvent::Key(key) => key.state == ButtonState::Pressed,
        WindowEvent::Pointer(pointer) => matches!(
            pointer.kind,
            PointerKind::Moved
                | PointerKind::Entered
                | PointerKind::Wheel(_)
                | PointerKind::Button(_, ButtonState::Pressed)
        ),
        _ => false,
    }
}

/// Whether a press asks for a paste, by the chords of Qt's
/// `QKeySequence::Paste` that egui-winit knows too: Command+V (Control+V
/// off macOS), with Shift or without, Shift+Insert, and the Paste key. The
/// V is the key that types it, or the V's place on a keyboard that types
/// no Latin letter there.
pub(super) fn asks_paste(key: &ljos_host::KeyEvent, held: ljos_host::Modifiers, mac: bool) -> bool {
    if key.state != ButtonState::Pressed {
        return false;
    }
    let command = if mac { held.meta } else { held.control };
    let other = if mac { held.control } else { held.meta };
    match &key.key {
        ljos_host::Key::Named(name) => {
            name == "Paste"
                || (!mac
                    && name == "Insert"
                    && held.shift
                    && !held.control
                    && !held.alt
                    && !held.meta)
        }
        ljos_host::Key::Character(c) if command && !held.alt && !other => {
            let latin = c.chars().all(|c| c.is_ascii_alphabetic());
            c.eq_ignore_ascii_case("v") || (!latin && key.code.as_deref() == Some("KeyV"))
        }
        _ => false,
    }
}

/// Keeps the modifiers a window's keyboard holds. One that lost the
/// keyboard holds none: the release goes to the window that has it.
pub(super) fn follow_modifiers(held: &mut ljos_host::Modifiers, event: &WindowEvent) {
    match event {
        WindowEvent::Modifiers(m) => *held = *m,
        WindowEvent::Focus(false) => *held = ljos_host::Modifiers::default(),
        _ => {}
    }
}
