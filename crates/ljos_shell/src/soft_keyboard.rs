//! A phone's on-screen keyboard and its input method, for an editor of the
//! scene (ARCHITECTURE §11): the phone's [`PlatformTextInput`].
//!
//! The input method there holds the editor's text itself and tells the
//! whole of it after every change ([`TextEvent::Held`]); the editor takes
//! what changed ([`QuickScene::ime_set_text`]). What the editor holds is
//! told back whenever it differs from what the method holds: the
//! application changed the text, the user moved the cursor by touch, a
//! validator refused what was typed, or a field hides its text and the
//! method is to hold none of it.
//!
//! [`TextEvent::Held`]: ljos_host::TextEvent::Held

#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use ljos_engine::Engine;
use ljos_host::relay::Request;
use ljos_host::{EditorKind, HeldText, TextKind};
use ljos_quick::text_input::input_method_hint as hint;
use ljos_quick::{ImeState, ImeText, PlatformTextInput, QuickScene};
use ljos_render::kurbo;

// `Qt.InputMethodHint` values ljos_quick does not name.
const PREFER_NUMBERS: i32 = 0x8;
const DIALABLE_CHARACTERS_ONLY: i32 = 0x10_0000;
const EMAIL_CHARACTERS_ONLY: i32 = 0x20_0000;
const URL_CHARACTERS_ONLY: i32 = 0x40_0000;

/// The keyboard an editor with `hints` (`Qt.InputMethodHint`) asks for.
pub(crate) fn editor_kind(hints: i32) -> EditorKind {
    let has = |h: i32| hints & h != 0;
    let kind = if has(hint::DIGITS_ONLY) {
        TextKind::Digits
    } else if has(hint::FORMATTED_NUMBERS_ONLY) || has(PREFER_NUMBERS) {
        TextKind::Number
    } else if has(DIALABLE_CHARACTERS_ONLY) {
        TextKind::Phone
    } else if has(EMAIL_CHARACTERS_ONLY) {
        TextKind::Email
    } else if has(URL_CHARACTERS_ONLY) {
        TextKind::Url
    } else {
        TextKind::Text
    };
    EditorKind {
        kind,
        hidden: has(hint::HIDDEN_TEXT),
        private: has(hint::SENSITIVE_DATA),
        multi_line: has(hint::MULTI_LINE),
        no_suggestions: has(hint::NO_PREDICTIVE_TEXT),
        no_capitals: has(hint::NO_AUTO_UPPERCASE),
    }
}

fn held(text: ImeText) -> HeldText {
    HeldText {
        text: text.text,
        anchor: text.anchor,
        cursor: text.cursor,
        compose: text.compose,
    }
}

/// Whether the method holding `told` must be told `now`. While it is
/// composing, its cursor inside the composition is its own.
fn differs(told: Option<&HeldText>, now: &HeldText) -> bool {
    let Some(told) = told else {
        return true;
    };
    told.text != now.text
        || told.compose != now.compose
        || (now.compose.is_none() && (told.anchor, told.cursor) != (now.anchor, now.cursor))
}

/// The phone's side of the input method.
#[derive(Debug, Default)]
pub(crate) struct SoftKeyboard {
    state: ImeState,
    /// The hints of the editor with focus; `None` without one.
    hints: Option<i32>,
    /// What the method holds, as it last told it or was told.
    told: Option<HeldText>,
    asked: Vec<Request>,
    /// The keyboard was asked to show for the editor with focus.
    shown: bool,
}

impl PlatformTextInput for SoftKeyboard {
    fn enable(&mut self, hints: i32) {
        self.hints = Some(hints);
        self.told = None;
        self.shown = false;
        self.asked.push(Request::Editor(editor_kind(hints)));
    }

    fn disable(&mut self) {
        self.hints = None;
        self.told = None;
        self.asked.push(Request::Keyboard(false));
    }

    fn set_cursor_area(&mut self, _rect: kurbo::Rect) {
        // The keyboard is told nothing of where the cursor is: the view
        // gives up the keyboard's part of the screen instead.
    }

    fn interrupt(&mut self) {
        // The method's composition was dropped by the application: it is
        // told what the editor holds now.
        self.told = None;
    }

    fn set_keyboard_wanted(&mut self, wanted: bool) {
        if self.hints.is_some() || !wanted {
            self.asked.push(Request::Keyboard(wanted));
        }
    }
}

impl SoftKeyboard {
    /// The input method holds `now`: the editor with focus takes what
    /// changed.
    pub(crate) fn held(&mut self, scene: &QuickScene, engine: &Engine, now: HeldText) {
        let text = ImeText {
            text: now.text.clone(),
            anchor: now.anchor,
            cursor: now.cursor,
            compose: now.compose.clone(),
        };
        scene.ime_set_text(engine, &text);
        self.told = Some(now);
    }

    /// What the platform is to be asked after the scene handled input or
    /// made a frame: the keyboard for the editor with focus, what it is to
    /// hold, and to show itself for an editor newly focused.
    pub(crate) fn sync(&mut self, scene: &QuickScene, engine: &Engine) -> Vec<Request> {
        let mut state = std::mem::take(&mut self.state);
        state.sync(scene, engine, self);
        self.state = state;
        if self.hints.is_some()
            && let Some(now) = scene.ime_text(engine).map(held)
        {
            let shown = self.shown;
            self.shown = true;
            if differs(self.told.as_ref(), &now) {
                self.told = Some(now.clone());
                self.asked.push(Request::Held(now));
            }
            // What it holds is set before the keyboard shows.
            if !shown {
                self.asked.push(Request::Keyboard(true));
            }
        }
        std::mem::take(&mut self.asked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_pick_the_keyboard() {
        assert_eq!(editor_kind(0), EditorKind::default());
        assert_eq!(editor_kind(hint::DIGITS_ONLY).kind, TextKind::Digits);
        assert_eq!(
            editor_kind(hint::FORMATTED_NUMBERS_ONLY).kind,
            TextKind::Number
        );
        assert_eq!(editor_kind(DIALABLE_CHARACTERS_ONLY).kind, TextKind::Phone);
        assert_eq!(editor_kind(EMAIL_CHARACTERS_ONLY).kind, TextKind::Email);
        assert_eq!(editor_kind(URL_CHARACTERS_ONLY).kind, TextKind::Url);
        let password =
            editor_kind(hint::HIDDEN_TEXT | hint::SENSITIVE_DATA | hint::NO_PREDICTIVE_TEXT);
        assert!(password.hidden && password.private && password.no_suggestions);
        assert!(editor_kind(hint::MULTI_LINE).multi_line);
    }

    fn text(text: &str, cursor: usize, compose: Option<std::ops::Range<usize>>) -> HeldText {
        HeldText {
            text: text.into(),
            anchor: cursor,
            cursor,
            compose,
        }
    }

    #[test]
    fn the_method_is_told_only_what_it_does_not_hold() {
        let ab = text("ab", 2, None);
        assert!(differs(None, &ab));
        assert!(!differs(Some(&ab), &ab));
        assert!(differs(Some(&ab), &text("ab", 1, None)));
        assert!(differs(Some(&ab), &text("abc", 3, None)));
        // Its cursor inside what it composes is its own.
        let composing = text("abni", 3, Some(2..4));
        assert!(!differs(Some(&composing), &text("abni", 4, Some(2..4))));
        assert!(differs(Some(&composing), &text("abni", 4, None)));
    }
}
