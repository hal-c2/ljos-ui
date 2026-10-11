//! Text from the platform, and the platform's input method told where the
//! text is (ARCHITECTURE §11).
//!
//! Inwards: what an input method composes and commits (`Event::Ime`), and
//! text that arrives with no key egui can name (`Event::Text` alone: a key
//! of a layout egui has no name for, a compose sequence). Outwards: while
//! an editor has active focus egui's platform output says so and where the
//! cursor is, so the integration turns the input method on and its
//! candidate window sits at the cursor.
//!
//! This is the desktop [`PlatformTextInput`]; the trait knows no egui, a
//! phone implements it over its keyboard service.

use egui::output::IMEOutput;
use egui::{Event, IMEPurpose, ImeEvent, Pos2, Rect, Ui};
use ljos_engine::Engine;
use ljos_quick::keys::key;
use ljos_quick::text_input::input_method_hint as hint;
use ljos_quick::{ImeState, KeyEvent, PlatformTextInput, QuickScene};
use ljos_render::kurbo;

use crate::keys::Translated;

/// Text the platform hands over that is not a key press.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlatformText {
    /// Text typed with no key egui names.
    Typed(String),
    /// What an input method is composing; its cursor counts UTF-16 units.
    Preedit {
        text: String,
        cursor: Option<usize>,
    },
    Commit(String),
    /// The input method takes back characters around the cursor.
    Delete {
        before: usize,
        after: usize,
    },
}

/// The UTF-16 units of the first `chars` characters of `text`.
fn units(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

/// The text events of one pass with their place among its events, in
/// order. `keys` are the key events of the same pass: a text one of them
/// took as what its key typed is not listed again.
pub(crate) fn collect(events: &[Event], keys: &[Translated]) -> Vec<(usize, PlatformText)> {
    let paired = |i: usize| keys.iter().any(|k| k.sources[1] == Some(i));
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let what = match e {
            Event::Text(t) if !t.is_empty() && !paired(i) => PlatformText::Typed(t.clone()),
            Event::Ime(ImeEvent::Preedit {
                text,
                active_range_chars,
            }) => PlatformText::Preedit {
                text: text.clone(),
                // The end of what the method marks as active is where the
                // next character goes.
                cursor: active_range_chars.as_ref().map(|r| units(text, r.end)),
            },
            Event::Ime(ImeEvent::Commit(text)) => PlatformText::Commit(text.clone()),
            Event::Ime(ImeEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            }) => PlatformText::Delete {
                before: *before_chars,
                after: *after_chars,
            },
            _ => continue,
        };
        out.push((i, what));
    }
    out
}

/// Hands one text event to the scene. Returns whether an item took it;
/// what nobody took stays in egui's input.
pub(crate) fn deliver(scene: &mut QuickScene, engine: &Engine, what: &PlatformText) -> bool {
    match what {
        PlatformText::Typed(text) => {
            // A key event like any other, so that `Keys` handlers and
            // shortcuts see it before an editor types it. The character
            // names the key where Qt has one for it.
            let mut chars = text.chars();
            let code = match (chars.next(), chars.next()) {
                (Some(c), None) => key::from_char(c),
                _ => None,
            }
            .unwrap_or(key::UNKNOWN);
            let taken = scene.key_event(engine, &KeyEvent::press(code).with_text(text.as_str()));
            // No release will come for a key egui does not report.
            scene.key_event(engine, &KeyEvent::release(code).with_text(text.as_str()));
            taken
        }
        PlatformText::Preedit { text, cursor } => scene.ime_preedit(engine, text, *cursor),
        PlatformText::Commit(text) => scene.ime_commit(engine, text),
        PlatformText::Delete { before, after } => {
            let Some((item, client)) = scene.text_input_target(engine) else {
                return false;
            };
            if !client.enabled(engine, item) {
                return false;
            }
            let around = client.surrounding(engine, item);
            // The method counts characters, the editor UTF-16 units.
            let mut starts: Vec<usize> = Vec::new();
            let mut at = 0;
            for c in around.text.chars() {
                starts.push(at);
                at += c.len_utf16();
            }
            starts.push(at);
            let here = starts.partition_point(|s| *s < around.cursor);
            let start = starts[here.saturating_sub(*before)];
            let end = starts[(here + after).min(starts.len() - 1)];
            if start < end {
                client.replace(engine, item, start..end, "");
            }
            true
        }
    }
}

/// What egui's platform output is told about the editor with focus.
#[derive(Debug, Default)]
struct Desktop {
    /// The `inputMethodHints` of the editor with focus; `None` without
    /// one.
    hints: Option<i32>,
    /// Its cursor, in the scene's coordinates.
    area: kurbo::Rect,
    /// Focus went from one editor straight to another: what the method
    /// was composing for the first is not for the second.
    interrupt: bool,
}

impl PlatformTextInput for Desktop {
    fn enable(&mut self, hints: i32) {
        self.interrupt = self.hints.is_some();
        self.hints = Some(hints);
    }

    fn disable(&mut self) {
        self.hints = None;
        self.interrupt = false;
    }

    fn set_cursor_area(&mut self, rect: kurbo::Rect) {
        self.area = rect;
    }
}

/// The view's side of the input method.
#[derive(Debug, Default)]
pub(crate) struct TextBridge {
    state: ImeState,
    desktop: Desktop,
}

impl TextBridge {
    /// Tells egui whether an editor has focus and where its cursor is.
    /// `rect` is where the scene is painted.
    ///
    /// egui forgets its platform output after every pass, so this is said
    /// in every pass; the scene is only asked what changed.
    pub fn sync(&mut self, scene: &QuickScene, engine: &Engine, ui: &Ui, rect: Rect) {
        self.state.sync(scene, engine, &mut self.desktop);
        let Some(hints) = self.desktop.hints else {
            return;
        };
        let a = self.desktop.area;
        let at = |x: f64, y: f64| Pos2::new(rect.min.x + x as f32, rect.min.y + y as f32);
        let ime = IMEOutput {
            // The platform's method may keep what it sees: it is told not
            // to for a field that hides its text.
            purpose: if hints & hint::HIDDEN_TEXT != 0 {
                IMEPurpose::Password
            } else {
                IMEPurpose::Normal
            },
            rect,
            cursor_rect: Rect::from_min_max(at(a.x0, a.y0), at(a.x1, a.y1)),
            should_interrupt_composition: std::mem::take(&mut self.desktop.interrupt),
        };
        ui.ctx().output_mut(|o| {
            o.ime = Some(ime);
            o.mutable_text_under_cursor = true;
        });
    }

    /// Whether the platform was last told an editor has focus.
    pub fn enabled(&self) -> bool {
        self.desktop.hints.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyTrack;
    use egui::os::OperatingSystem;
    use egui::{Key, Modifiers};

    fn down(k: Key) -> Event {
        Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn text_a_key_typed_is_not_text_of_its_own() {
        let events = [
            down(Key::A),
            Event::Text("a".into()),
            Event::Text("é".into()),
        ];
        let keys = KeyTrack::default().translate(&events, OperatingSystem::Nix);
        assert_eq!(
            collect(&events, &keys),
            [(2, PlatformText::Typed("é".into()))]
        );
    }

    #[test]
    fn a_composition_counts_its_cursor_in_utf16() {
        let events = [
            Event::Ime(ImeEvent::Preedit {
                text: "𝄞に".into(),
                active_range_chars: Some(1..2),
            }),
            Event::Ime(ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            Event::Ime(ImeEvent::Commit("日本".into())),
        ];
        assert_eq!(
            collect(&events, &[]),
            [
                (
                    0,
                    PlatformText::Preedit {
                        text: "𝄞に".into(),
                        cursor: Some(3)
                    }
                ),
                (
                    1,
                    PlatformText::Preedit {
                        text: String::new(),
                        cursor: None
                    }
                ),
                (2, PlatformText::Commit("日本".into())),
            ]
        );
    }

    #[test]
    fn a_second_editor_interrupts_the_composition_of_the_first() {
        let mut d = Desktop::default();
        d.enable(0);
        assert!(!d.interrupt);
        d.enable(hint::HIDDEN_TEXT);
        assert!(d.interrupt);
        d.disable();
        d.enable(0);
        assert!(!d.interrupt);
    }
}
