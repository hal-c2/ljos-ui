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
    /// The application hid the keyboard since the last sync: an editor
    /// focused meanwhile does not show it.
    hidden: bool,
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
        self.hidden = !wanted;
        if !wanted {
            self.asked.push(Request::Keyboard(false));
        } else if self.hints.is_some() {
            // Shown by `sync`, after the editor focused meanwhile is set up
            // and holds its text: the hints held now may be of the editor
            // focus left.
            self.shown = false;
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

    /// The window has a new surface, and the platform's method a new
    /// editor: the editor with focus is set up, told what it holds and
    /// shown again.
    pub(crate) fn surface_changed(&mut self) {
        if let Some(hints) = self.hints {
            self.asked.push(Request::Editor(editor_kind(hints)));
            self.told = None;
            self.shown = false;
        }
    }

    /// What the platform is to be asked after the scene handled input or
    /// made a frame: the keyboard for the editor with focus, what it is to
    /// hold, and to show itself for an editor newly focused.
    pub(crate) fn sync(&mut self, scene: &QuickScene, engine: &Engine) -> Vec<Request> {
        let mut state = std::mem::take(&mut self.state);
        state.sync(scene, engine, self);
        self.state = state;
        let hidden = std::mem::take(&mut self.hidden);
        if self.hints.is_some()
            && let Some(now) = scene.ime_text(engine).map(held)
        {
            let shown = self.shown;
            self.shown = true;
            if differs(self.told.as_ref(), &now) {
                let over = self.told.replace(now.clone());
                self.asked.push(Request::Held { now, over });
            }
            // What it holds is set before the keyboard shows.
            if !shown && !hidden {
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

    use ljos_engine::{ObjectId, Value};
    use ljos_quick::{Item, TextInput};
    use ljos_runtime::Time;

    /// A scene of one focused `TextInput` holding `start`, with `set`
    /// written to it first.
    fn editor(start: &str, set: &[(&str, Value)]) -> (Engine, QuickScene, ObjectId) {
        let e = Engine::new();
        ljos_quick::register(&e);
        let root = e.create_native(Item::default(), None).unwrap();
        e.complete(root).unwrap();
        let input = e.create_native(TextInput::default(), Some(root)).unwrap();
        ljos_quick::add_data(&e, root, input).unwrap();
        e.complete(input).unwrap();
        for (name, value) in set {
            e.write_by_name(input, name, value.clone()).unwrap();
        }
        e.write_by_name(input, "text", Value::from(start)).unwrap();
        let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
        s.set_root(&e, root).unwrap();
        s.frame(&e, Time::ZERO);
        e.invoke_by_name(input, "forceActiveFocus", &[]).unwrap();
        s.frame(&e, Time::ZERO);
        (e, s, input)
    }

    fn helds(asked: &[Request]) -> Vec<String> {
        asked
            .iter()
            .filter_map(|r| match r {
                Request::Held { now, .. } => Some(now.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_focused_editor_is_set_up_held_and_then_shown() {
        let (e, s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        let asked = keyboard.sync(&s, &e);
        let at = |want: fn(&Request) -> bool| asked.iter().position(want);
        let editor = at(|r| matches!(r, Request::Editor(_))).expect("the editor is told");
        let held = at(|r| matches!(r, Request::Held { .. })).expect("its text is told");
        let shown = asked
            .iter()
            .rposition(|r| *r == Request::Keyboard(true))
            .expect("the keyboard is shown");
        assert!(editor < held && held < shown, "{asked:?}");
        assert_eq!(helds(&asked), ["ab"]);
        // Nothing changed: nothing more is told.
        assert_eq!(keyboard.sync(&s, &e), Vec::<Request>::new());
    }

    #[test]
    fn what_the_editor_refused_is_told_back() {
        let (e, mut s, _) = editor("", &[("maximumLength", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("abx", 3, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), ["ab"]);
    }

    #[test]
    fn what_the_application_set_is_told() {
        let (e, mut s, input) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("abc", 3, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        e.write_by_name(input, "text", Value::from("zz")).unwrap();
        s.frame(&e, Time::ZERO);
        let asked = keyboard.sync(&s, &e);
        assert_eq!(helds(&asked), ["zz"]);
        // With what the method held, so that the platform drops it if the
        // method typed before it got to it.
        let over = asked.iter().find_map(|r| match r {
            Request::Held { over, .. } => over.as_ref().map(|o| o.text.clone()),
            _ => None,
        });
        assert_eq!(over.as_deref(), Some("abc"));
    }

    #[test]
    fn a_hidden_field_leaves_the_method_its_own_typing() {
        // `TextInput.Password`.
        let (e, mut s, input) = editor("12", &[("echoMode", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        // The method holds none of a hidden field's text.
        assert_eq!(helds(&keyboard.sync(&s, &e)), [""]);
        keyboard.held(&s, &e, text("3", 1, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        keyboard.held(&s, &e, text("34", 2, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "1234"
        );
    }

    #[test]
    fn typing_into_a_hidden_field_with_all_selected_types_over_it() {
        let (e, mut s, input) = editor("1234", &[("echoMode", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        // Selected on focus, as a password field often is.
        e.invoke_by_name(input, "selectAll", &[]).unwrap();
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("5", 1, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "5"
        );
    }

    fn position(asked: &[Request], want: fn(&Request) -> bool) -> Option<usize> {
        asked.iter().position(want)
    }

    #[test]
    fn focus_moved_by_a_tap_shows_the_keyboard_once_the_new_editor_is_set_up() {
        let (e, s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        // The tap asks for the keyboard while the editor it left is the
        // one the method knows; the editor it focused is told after.
        keyboard.set_keyboard_wanted(true);
        keyboard.enable(hint::DIGITS_ONLY);
        let asked = keyboard.sync(&s, &e);
        let editor = position(&asked, |r| matches!(r, Request::Editor(_))).expect("told");
        let held = position(&asked, |r| matches!(r, Request::Held { .. })).expect("held");
        let shown = position(&asked, |r| *r == Request::Keyboard(true)).expect("shown");
        assert!(editor < held && held < shown, "{asked:?}");
    }

    #[test]
    fn a_keyboard_hidden_as_an_editor_takes_focus_stays_hidden() {
        let (e, s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        // `Qt.inputMethod.hide()` before the editor's first sync.
        keyboard.set_keyboard_wanted(false);
        let asked = keyboard.sync(&s, &e);
        assert!(position(&asked, |r| matches!(r, Request::Editor(_))).is_some());
        assert!(!asked.contains(&Request::Keyboard(true)), "{asked:?}");
        // Asked for again, it shows.
        keyboard.set_keyboard_wanted(true);
        assert!(keyboard.sync(&s, &e).contains(&Request::Keyboard(true)));
    }

    #[test]
    fn a_new_surface_tells_the_focused_editor_again() {
        let (e, s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.surface_changed();
        let asked = keyboard.sync(&s, &e);
        let editor = position(&asked, |r| matches!(r, Request::Editor(_))).expect("told");
        let held = position(&asked, |r| matches!(r, Request::Held { .. })).expect("held");
        let shown = position(&asked, |r| *r == Request::Keyboard(true)).expect("shown");
        assert!(editor < held && held < shown, "{asked:?}");
        assert_eq!(helds(&asked), ["ab"]);
    }

    #[test]
    fn a_hidden_field_composing_keeps_what_the_method_typed_before() {
        let (e, mut s, input) = editor("", &[("echoMode", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("a", 1, None));
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("ab", 2, Some(1..2)));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "a"
        );
    }

    #[test]
    fn a_hidden_replacement_the_validator_refuses_leaves_the_text_whole() {
        let (e, mut s, input) = editor("", &[("echoMode", Value::from(2))]);
        let validator = e
            .create_native(
                ljos_quick::IntValidator {
                    bottom: 0,
                    top: 999,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        e.complete(validator).unwrap();
        e.write_by_name(input, "validator", Value::Object(validator))
            .unwrap();
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("123", 3, None));
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("1x3", 3, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "123"
        );
    }

    #[test]
    fn a_hidden_field_that_took_part_of_the_typing_starts_the_method_anew() {
        let (e, mut s, input) = editor(
            "",
            &[
                ("echoMode", Value::from(2)),
                ("maximumLength", Value::from(2)),
            ],
        );
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("abc", 3, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "ab"
        );
        // The method is not left holding the "c" the field cut.
        assert_eq!(helds(&keyboard.sync(&s, &e)), [""]);
        keyboard.held(&s, &e, text("", 0, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "ab"
        );
    }
}
