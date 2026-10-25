//! egui key and text events to `ljos_quick`'s key events.
//!
//! egui reports a key twice: `Event::Key` says which key, and for a key
//! that types something an `Event::Text` follows with what it typed. A
//! `Qt.Key` event carries both, so the two are paired here. Text that
//! comes with no key (a key egui has no name for) is not a key here: the
//! `text` module hands it to the scene.

use egui::os::OperatingSystem;
use egui::{Event, FocusDirection, Key};
use ljos_quick::keys::key;
use ljos_quick::{KeyEvent, Modifiers};

use crate::input::map_modifiers;

/// The `Qt.Key` of an egui key.
///
/// On macOS Qt calls the Command key `Key_Control` and the Control key
/// `Key_Meta`, the same swap as for the modifiers.
pub fn map_key(k: Key, os: OperatingSystem) -> i32 {
    let mac = os == OperatingSystem::Mac;
    match k {
        Key::ArrowDown => key::DOWN,
        Key::ArrowLeft => key::LEFT,
        Key::ArrowRight => key::RIGHT,
        Key::ArrowUp => key::UP,
        Key::Escape => key::ESCAPE,
        Key::Tab => key::TAB,
        Key::Backspace => key::BACKSPACE,
        // egui has one Enter for the main key and the keypad's; the main
        // one is by far the commoner, and QML that cares handles both.
        Key::Enter => key::RETURN,
        Key::Space => key::SPACE,
        Key::Insert => key::INSERT,
        Key::Delete => key::DELETE,
        Key::Home => key::HOME,
        Key::End => key::END,
        Key::PageUp => key::PAGE_UP,
        Key::PageDown => key::PAGE_DOWN,
        Key::Copy => key::COPY,
        Key::Cut => key::CUT,
        Key::Paste => key::PASTE,
        Key::BrowserBack => key::BACK,
        Key::BrowserForward => key::FORWARD,
        Key::BrowserRefresh => key::REFRESH,
        Key::BrowserSearch => key::SEARCH,
        Key::BrowserHome => key::HOME_PAGE,
        Key::BrowserFavorites => key::FAVORITES,
        Key::BrowserStop => key::STOP,
        Key::MediaPlayPause => key::MEDIA_TOGGLE_PLAY_PAUSE,
        Key::MediaTrackNext => key::MEDIA_NEXT,
        Key::MediaTrackPrevious => key::MEDIA_PREVIOUS,
        Key::MediaStop => key::MEDIA_STOP,
        Key::AudioVolumeMute => key::VOLUME_MUTE,
        Key::AudioVolumeDown => key::VOLUME_DOWN,
        Key::AudioVolumeUp => key::VOLUME_UP,
        Key::LaunchMail => key::LAUNCH_MAIL,
        Key::LaunchApp1 => key::LAUNCH_0,
        Key::LaunchApp2 => key::LAUNCH_1,
        Key::CapsLock => key::CAPS_LOCK,
        Key::NumLock => key::NUM_LOCK,
        Key::ScrollLock => key::SCROLL_LOCK,
        Key::PrintScreen => key::PRINT,
        Key::Pause => key::PAUSE,
        Key::Menu => key::MENU,
        Key::Eject => key::EJECT,
        Key::Help => key::HELP,
        Key::Power => key::POWER_OFF,
        Key::Sleep => key::SLEEP,
        Key::Clear => key::CLEAR,
        // Qt has no code for it: the keyboard keeps it to itself.
        Key::Fn => key::UNKNOWN,
        Key::ShiftLeft | Key::ShiftRight => key::SHIFT,
        Key::AltLeft | Key::AltRight => key::ALT,
        Key::ControlLeft | Key::ControlRight if mac => key::META,
        Key::ControlLeft | Key::ControlRight => key::CONTROL,
        Key::SuperLeft | Key::SuperRight if mac => key::CONTROL,
        Key::SuperLeft | Key::SuperRight => key::META,
        k => {
            let name = k.name();
            if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<i32>().ok())
                && (1..=35).contains(&n)
            {
                return key::F1 + n - 1;
            }
            // Every other key prints one ASCII character, and that
            // character's code is its `Qt.Key`. Letters and digits are
            // named by theirs.
            let c = match k {
                Key::Colon => ':',
                Key::Comma => ',',
                Key::Backslash => '\\',
                Key::Slash => '/',
                Key::Pipe => '|',
                Key::Questionmark => '?',
                Key::Exclamationmark => '!',
                Key::OpenBracket => '[',
                Key::CloseBracket => ']',
                Key::OpenCurlyBracket => '{',
                Key::CloseCurlyBracket => '}',
                Key::Backtick => '`',
                Key::Minus => '-',
                Key::Period => '.',
                Key::Plus => '+',
                Key::Equals => '=',
                Key::Semicolon => ';',
                Key::Quote => '\'',
                _ => {
                    let mut chars = name.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => c,
                        // A key egui has learnt since, or one that is a
                        // place on the keyboard and not a character
                        // (IntlBackslash): the text that comes with the
                        // press says what it is.
                        _ => return key::UNKNOWN,
                    }
                }
            };
            key::from_char(c).unwrap_or(key::UNKNOWN)
        }
    }
}

/// The modifier a key is by itself: a `KeyEvent` lists the modifiers held
/// besides its own key.
fn own_modifier(qt: i32) -> Modifiers {
    match qt {
        key::SHIFT => Modifiers::SHIFT,
        key::CONTROL => Modifiers::CONTROL,
        key::ALT => Modifiers::ALT,
        key::META => Modifiers::META,
        _ => Modifiers::empty(),
    }
}

/// What egui itself does with a key that nobody took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fallback {
    /// Tab, Shift+Tab and the bare arrows move egui's focus.
    Move(FocusDirection),
    /// Escape gives up the keyboard.
    Release,
}

fn fallback(k: Key, m: egui::Modifiers) -> Option<Fallback> {
    Some(match k {
        Key::Tab if !m.any() => Fallback::Move(FocusDirection::Next),
        Key::Tab if m.shift_only() => Fallback::Move(FocusDirection::Previous),
        Key::ArrowUp if !m.any() => Fallback::Move(FocusDirection::Up),
        Key::ArrowDown if !m.any() => Fallback::Move(FocusDirection::Down),
        Key::ArrowLeft if !m.any() => Fallback::Move(FocusDirection::Left),
        Key::ArrowRight if !m.any() => Fallback::Move(FocusDirection::Right),
        Key::Escape if !m.any() => Fallback::Release,
        _ => return None,
    })
}

/// One key event for the scene, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Translated {
    pub event: KeyEvent,
    /// Indexes into the pass's events: the ones to drop from egui's input
    /// when the scene accepts the event.
    pub sources: [Option<usize>; 2],
    /// The egui key to forget as held when a press is accepted.
    pub held: Option<Key>,
    /// What egui does with the press when the scene leaves it.
    pub fallback: Option<Fallback>,
}

/// Keys that are down, with what their press was reported as. A release
/// repeats it: egui's release carries no text, and with Shift let go first
/// it may not even name the same key.
#[derive(Debug, Default)]
pub(crate) struct KeyTrack {
    down: Vec<(Key, i32, String)>,
}

impl KeyTrack {
    /// Forgets the held keys: their releases go to whoever has the
    /// keyboard then.
    pub fn clear(&mut self) {
        self.down.clear();
    }

    fn pressed(&mut self, k: Key, qt: i32, text: &str) {
        self.down.retain(|(held, ..)| *held != k);
        self.down.push((k, qt, text.to_owned()));
    }

    fn released(&mut self, k: Key) -> Option<(i32, String)> {
        let at = self.down.iter().position(|(held, ..)| *held == k)?;
        let (_, qt, text) = self.down.remove(at);
        Some((qt, text))
    }

    /// The key events in one pass's events, in order.
    pub fn translate(&mut self, events: &[Event], os: OperatingSystem) -> Vec<Translated> {
        let mut out = Vec::new();
        for (i, e) in events.iter().enumerate() {
            match e {
                Event::Key {
                    key: k,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => {
                    let (text, text_at) = match events.get(i + 1) {
                        Some(Event::Text(t)) => (t.as_str(), Some(i + 1)),
                        // egui keeps control characters to itself; a Qt
                        // key event carries the one its key types (a
                        // `TextEdit` types the tab).
                        _ if !(*k == Key::Tab && modifiers.shift) => {
                            (key::control_text(map_key(*k, os)), None)
                        }
                        _ => ("", None),
                    };
                    // The character says more than egui's key does: it
                    // follows the keyboard layout, and egui has no key for
                    // most of Latin-1.
                    let mut chars = text.chars();
                    let typed = match (chars.next(), chars.next()) {
                        (Some(c), None) => key::from_char(c),
                        _ => None,
                    };
                    let mut qt = typed.unwrap_or_else(|| map_key(*k, os));
                    if qt == key::TAB && modifiers.shift {
                        qt = key::BACKTAB;
                    }
                    let mut event = KeyEvent::press(qt)
                        .with_modifiers(map_modifiers(*modifiers, os) - own_modifier(qt))
                        .with_text(text);
                    if *repeat {
                        event = event.repeated();
                    }
                    self.pressed(*k, qt, text);
                    out.push(Translated {
                        event,
                        sources: [Some(i), text_at],
                        held: Some(*k),
                        fallback: fallback(*k, *modifiers),
                    });
                }
                Event::Key {
                    key: k,
                    pressed: false,
                    modifiers,
                    ..
                } => {
                    let (qt, text) = self
                        .released(*k)
                        .unwrap_or_else(|| (map_key(*k, os), String::new()));
                    let event = KeyEvent::release(qt)
                        .with_modifiers(map_modifiers(*modifiers, os) - own_modifier(qt))
                        .with_text(text);
                    out.push(Translated {
                        event,
                        sources: [Some(i), None],
                        held: None,
                        fallback: None,
                    });
                }
                // The platform layer turns the press of the clipboard
                // chords into these and sends no key. The scene's
                // shortcuts and Keys handlers still get to see the chord;
                // the release arrives as an ordinary key.
                Event::Copy | Event::Cut | Event::Paste(_) => {
                    let (k, c) = match e {
                        Event::Copy => (Key::C, 'C'),
                        Event::Cut => (Key::X, 'X'),
                        _ => (Key::V, 'V'),
                    };
                    let qt = key::from_char(c).unwrap_or(key::UNKNOWN);
                    self.pressed(k, qt, "");
                    out.push(Translated {
                        event: KeyEvent::press(qt).with_modifiers(Modifiers::CONTROL),
                        sources: [Some(i), None],
                        held: None,
                        fallback: None,
                    });
                }
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_quick::KeyKind;

    const NIX: OperatingSystem = OperatingSystem::Nix;
    const MAC: OperatingSystem = OperatingSystem::Mac;

    fn down(k: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn up(k: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key: k,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }
    }

    fn events(track: &mut KeyTrack, events: &[Event]) -> Vec<KeyEvent> {
        track
            .translate(events, NIX)
            .into_iter()
            .map(|t| t.event)
            .collect()
    }

    #[test]
    fn named_keys_have_their_qt_codes() {
        for (k, qt) in [
            (Key::Escape, key::ESCAPE),
            (Key::Tab, key::TAB),
            (Key::Enter, key::RETURN),
            (Key::Space, key::SPACE),
            (Key::ArrowLeft, key::LEFT),
            (Key::PageDown, key::PAGE_DOWN),
            (Key::Delete, key::DELETE),
            (Key::F1, key::F1),
            (Key::F12, key::F1 + 11),
            (Key::F35, key::F35),
            (Key::A, key::A),
            (Key::Z, key::Z),
            (Key::Num0, key::DIGIT_0),
            (Key::Num9, key::DIGIT_9),
            (Key::Minus, key::MINUS),
            (Key::Plus, key::PLUS),
            (Key::Comma, key::COMMA),
            (Key::OpenBracket, 0x5b),
            (Key::Questionmark, 0x3f),
            (Key::Quote, 0x27),
            (Key::Backslash, 0x5c),
            (Key::ShiftLeft, key::SHIFT),
            (Key::ControlRight, key::CONTROL),
            (Key::SuperLeft, key::META),
            (Key::BrowserBack, key::BACK),
            (Key::BrowserForward, key::FORWARD),
            (Key::MediaPlayPause, key::MEDIA_TOGGLE_PLAY_PAUSE),
            (Key::AudioVolumeUp, key::VOLUME_UP),
            (Key::CapsLock, key::CAPS_LOCK),
            (Key::PrintScreen, key::PRINT),
            (Key::Menu, key::MENU),
        ] {
            assert_eq!(map_key(k, NIX), qt, "{k:?}");
        }
    }

    #[test]
    fn every_egui_key_has_a_code() {
        for k in Key::ALL {
            let qt = map_key(*k, NIX);
            if !matches!(k, Key::IntlBackslash | Key::Fn) {
                assert_ne!(qt, key::UNKNOWN, "{k:?}");
            }
        }
    }

    #[test]
    fn mac_swaps_the_command_and_control_keys() {
        assert_eq!(map_key(Key::SuperLeft, MAC), key::CONTROL);
        assert_eq!(map_key(Key::ControlLeft, MAC), key::META);
    }

    #[test]
    fn return_carries_the_carriage_return_egui_keeps_to_itself() {
        let mut track = KeyTrack::default();
        let out = track.translate(
            &[
                down(Key::Enter, egui::Modifiers::CTRL),
                up(Key::Enter, egui::Modifiers::CTRL),
            ],
            NIX,
        );
        assert_eq!(out[0].event.text, "\r");
        assert_eq!(out[1].event.text, "\r");
    }

    #[test]
    fn a_press_takes_the_text_that_follows_it() {
        let mut track = KeyTrack::default();
        let out = track.translate(
            &[
                down(Key::A, egui::Modifiers::NONE),
                Event::Text("a".into()),
                up(Key::A, egui::Modifiers::NONE),
            ],
            NIX,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].event, KeyEvent::press(key::A).with_text("a"));
        assert_eq!(out[0].sources, [Some(0), Some(1)]);
        // The release says what the press said.
        assert_eq!(out[1].event, KeyEvent::release(key::A).with_text("a"));
        assert_eq!(out[1].sources, [Some(2), None]);
    }

    #[test]
    fn the_typed_character_names_the_key() {
        let mut track = KeyTrack::default();
        // A layout where the key egui calls Num1 types "é" with Shift held.
        let out = events(
            &mut track,
            &[
                down(Key::Num1, egui::Modifiers::SHIFT),
                Event::Text("é".into()),
            ],
        );
        assert_eq!(out[0].key, 0xc9);
        assert_eq!(out[0].text, "é");
        assert_eq!(out[0].modifiers, Modifiers::SHIFT);
        // A character no Qt key stands for leaves egui's key.
        let out = events(
            &mut track,
            &[down(Key::A, egui::Modifiers::NONE), Event::Text("ф".into())],
        );
        assert_eq!(out[0].key, key::A);
        assert_eq!(out[0].text, "ф");
    }

    #[test]
    fn a_release_after_shift_went_up_is_still_the_pressed_key() {
        let mut track = KeyTrack::default();
        let out = events(
            &mut track,
            &[
                down(Key::Exclamationmark, egui::Modifiers::SHIFT),
                Event::Text("!".into()),
            ],
        );
        assert_eq!(out[0].key, 0x21);
        let out = events(
            &mut track,
            &[up(Key::Exclamationmark, egui::Modifiers::NONE)],
        );
        assert_eq!(out[0].kind, KeyKind::Release);
        assert_eq!((out[0].key, out[0].text.as_str()), (0x21, "!"));
    }

    #[test]
    fn shift_tab_is_backtab() {
        let mut track = KeyTrack::default();
        let out = track.translate(&[down(Key::Tab, egui::Modifiers::SHIFT)], NIX);
        assert_eq!(out[0].event.key, key::BACKTAB);
        assert_eq!(out[0].event.modifiers, Modifiers::SHIFT);
        assert_eq!(
            out[0].fallback,
            Some(Fallback::Move(FocusDirection::Previous))
        );
        let out = track.translate(&[down(Key::Tab, egui::Modifiers::NONE)], NIX);
        assert_eq!(out[0].event.key, key::TAB);
        assert_eq!(out[0].fallback, Some(Fallback::Move(FocusDirection::Next)));
        // With Ctrl held egui does nothing with Tab either.
        let out = track.translate(&[down(Key::Tab, egui::Modifiers::CTRL)], NIX);
        assert_eq!(out[0].fallback, None);
    }

    #[test]
    fn a_held_key_repeats() {
        let mut track = KeyTrack::default();
        let repeat = Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::NONE,
        };
        let out = events(&mut track, &[repeat]);
        assert!(out[0].auto_repeat);
        assert_eq!(out[0].key, key::DOWN);
    }

    #[test]
    fn a_modifier_key_does_not_list_itself() {
        let mut track = KeyTrack::default();
        let out = events(
            &mut track,
            &[
                down(Key::ShiftLeft, egui::Modifiers::SHIFT),
                down(
                    Key::ControlLeft,
                    egui::Modifiers::SHIFT | egui::Modifiers::CTRL,
                ),
                up(Key::ControlLeft, egui::Modifiers::SHIFT),
            ],
        );
        assert_eq!(
            (out[0].key, out[0].modifiers),
            (key::SHIFT, Modifiers::empty())
        );
        assert_eq!(
            (out[1].key, out[1].modifiers),
            (key::CONTROL, Modifiers::SHIFT)
        );
        assert_eq!(
            (out[2].key, out[2].modifiers),
            (key::CONTROL, Modifiers::SHIFT)
        );
    }

    #[test]
    fn the_clipboard_events_are_presses_of_their_chords() {
        let mut track = KeyTrack::default();
        let out = events(
            &mut track,
            &[Event::Copy, Event::Cut, Event::Paste("x".into())],
        );
        let chords: Vec<(i32, Modifiers)> = out.iter().map(|e| (e.key, e.modifiers)).collect();
        assert_eq!(
            chords,
            [
                (0x43, Modifiers::CONTROL),
                (0x58, Modifiers::CONTROL),
                (0x56, Modifiers::CONTROL)
            ]
        );
        assert!(out.iter().all(|e| e.kind == KeyKind::Press));
    }

    #[test]
    fn text_with_no_key_is_left_alone() {
        let mut track = KeyTrack::default();
        let out = track.translate(
            &[
                Event::Text("né".into()),
                Event::PointerGone,
                Event::Text("x".into()),
            ],
            NIX,
        );
        assert!(out.is_empty());
    }
}
