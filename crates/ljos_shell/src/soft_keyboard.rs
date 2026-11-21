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
use ljos_host::{Composing, CursorAnchor, EditorKind, HeldText, TextKind};
use ljos_quick::text_input::input_method_hint as hint;
use ljos_quick::{ImeGeometry, ImeState, ImeText, PlatformTextInput, QuickScene};
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

/// The part of `text` from UTF-16 unit `start` to `end`; a unit inside a
/// character takes the whole character.
fn units(text: &str, start: usize, end: usize) -> &str {
    // The byte of the character `unit` is in (`inside`), or after it.
    let byte = |unit: usize, inside: bool| {
        let mut at = 0;
        for (byte, c) in text.char_indices() {
            let next = at + c.len_utf16();
            if at >= unit || (inside && next > unit) {
                return byte;
            }
            at = next;
        }
        text.len()
    };
    let start = byte(start, true);
    &text[start..byte(end, false).max(start)]
}

/// Where `geometry`, in the scene's points, puts the editor holding
/// `text`, in the surface's pixels at `scale` pixels a point. The method
/// that holds `told` and composes is told its own cursor inside the
/// composition, which the editor does not keep (see [`differs`]).
fn anchor(
    geometry: &ImeGeometry,
    text: &HeldText,
    told: Option<&HeldText>,
    scale: f64,
) -> CursorAnchor {
    #[allow(clippy::cast_possible_truncation)]
    let px = |r: &kurbo::Rect| [r.x0, r.y0, r.x1, r.y1].map(|v| (v * scale) as f32);
    let composing = text
        .compose
        .as_ref()
        .filter(|c| c.start < c.end)
        .map(|c| Composing {
            start: c.start,
            text: units(&text.text, c.start, c.end).to_owned(),
            units: geometry.composing.iter().map(px).collect(),
        });
    CursorAnchor {
        editor: px(&geometry.editor),
        cursor: px(&geometry.cursor),
        selection: told
            .filter(|t| t.compose.is_some() && !differs(Some(t), text))
            .map_or([text.anchor, text.cursor], |t| [t.anchor, t.cursor]),
        composing,
    }
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
    /// Where the editor with focus and its text are, in the scene.
    geometry: Option<ImeGeometry>,
    /// The surface's pixels a point; 0 before the first size.
    scale: f64,
    /// Where the method was last told the text is.
    anchored: Option<CursorAnchor>,
}

impl PlatformTextInput for SoftKeyboard {
    fn enable(&mut self, hints: i32) {
        self.hints = Some(hints);
        self.told = None;
        self.unanchor();
        self.shown = false;
        self.asked.push(Request::Editor(editor_kind(hints)));
    }

    fn disable(&mut self) {
        self.hints = None;
        self.told = None;
        self.geometry = None;
        self.unanchor();
        self.asked.push(Request::Keyboard(false));
    }

    fn set_cursor_area(&mut self, _rect: kurbo::Rect) {
        // The keyboard is told nothing of where the cursor is: the view
        // gives up the keyboard's part of the screen instead.
    }

    fn wants_geometry(&self) -> bool {
        self.hints.is_some()
    }

    fn set_geometry(&mut self, geometry: &ImeGeometry) {
        self.geometry = Some(geometry.clone());
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
    /// The method is no longer told where the last editor's text was.
    fn unanchor(&mut self) {
        if self.anchored.take().is_some() {
            self.asked.push(Request::Anchor(None));
        }
    }

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

    /// The surface now has `scale` pixels a point.
    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// The window has a new surface, and the platform's method a new
    /// editor: the editor with focus is set up, told what it holds and
    /// shown again.
    pub(crate) fn surface_changed(&mut self) {
        if let Some(hints) = self.hints {
            self.asked.push(Request::Editor(editor_kind(hints)));
            self.told = None;
            self.anchored = None;
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
            // Told where the text is with what it is to hold, for an
            // input method that asked: the platform passes it on only then.
            let anchored = self
                .geometry
                .as_ref()
                .filter(|_| self.scale > 0.0)
                .map(|g| anchor(g, &now, self.told.as_ref(), self.scale));
            if differs(self.told.as_ref(), &now) {
                let over = self.told.replace(now.clone());
                self.asked.push(Request::Held { now, over });
            }
            if let Some(anchored) = anchored
                && self.anchored.as_ref() != Some(&anchored)
            {
                self.anchored = Some(anchored.clone());
                self.asked.push(Request::Anchor(Some(anchored)));
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

    fn anchors(asked: &[Request]) -> Vec<CursorAnchor> {
        asked
            .iter()
            .filter_map(|r| match r {
                Request::Anchor(Some(a)) => Some(a.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_method_composing_is_told_its_own_cursor() {
        let (e, mut s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.set_scale(1.0);
        keyboard.sync(&s, &e);
        // Its cursor between the composed n and i.
        keyboard.held(&s, &e, text("abni", 3, Some(2..4)));
        s.frame(&e, Time::ZERO);
        let told = anchors(&keyboard.sync(&s, &e));
        let last = told.last().expect("the composition is told");
        assert_eq!(last.selection, [3, 3]);
        assert_eq!(last.composing.as_ref().map(|c| c.start), Some(2));
    }

    #[test]
    fn an_editor_losing_focus_leaves_nothing_anchored() {
        let (e, s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.set_scale(1.0);
        assert_eq!(anchors(&keyboard.sync(&s, &e)).len(), 1);
        // Another editor takes focus: the first is forgotten before the
        // method is told where the new one is.
        keyboard.enable(0);
        let asked = keyboard.sync(&s, &e);
        let cleared = asked
            .iter()
            .position(|r| matches!(r, Request::Anchor(None)));
        let told = asked
            .iter()
            .position(|r| matches!(r, Request::Anchor(Some(_))));
        assert!(cleared.is_some() && cleared < told, "{asked:?}");
        keyboard.disable();
        let asked = keyboard.sync(&s, &e);
        assert!(
            asked.iter().any(|r| matches!(r, Request::Anchor(None))),
            "{asked:?}"
        );
        // Nothing was anchored: nothing to clear.
        keyboard.disable();
        let asked = keyboard.sync(&s, &e);
        assert!(
            !asked.iter().any(|r| matches!(r, Request::Anchor(_))),
            "{asked:?}"
        );
    }

    #[test]
    fn a_unit_range_takes_whole_characters() {
        assert_eq!(units("a😀b", 1, 3), "😀");
        assert_eq!(units("a😀b", 2, 4), "😀b");
        assert_eq!(units("ab", 1, 9), "b");
        assert_eq!(units("ab", 2, 1), "");
    }

    #[test]
    fn the_method_is_told_where_the_editor_is_in_pixels_when_it_moves() {
        let (e, mut s, input) = editor("ab", &[("x", Value::from(10))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.set_scale(2.0);
        let told = anchors(&keyboard.sync(&s, &e));
        let [at] = &told[..] else {
            panic!("told {told:?}");
        };
        assert_eq!(at.editor[0], 20.0);
        assert_eq!(at.selection, [2, 2]);
        assert!(at.cursor[0] > at.editor[0] && at.cursor[1] < at.cursor[3]);
        assert_eq!(at.composing, None);
        // Nothing moved: nothing more is told.
        assert_eq!(anchors(&keyboard.sync(&s, &e)), []);
        e.write_by_name(input, "x", Value::from(30)).unwrap();
        s.frame(&e, Time::ZERO);
        let moved = anchors(&keyboard.sync(&s, &e));
        assert_eq!(
            moved.iter().map(|a| a.editor[0]).collect::<Vec<_>>(),
            [60.0]
        );
        assert_eq!(moved[0].cursor[0] - at.cursor[0], 40.0);
    }

    #[test]
    fn the_method_is_told_where_each_composed_unit_stands() {
        let (e, mut s, _) = editor("ab", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.set_scale(1.0);
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("ab😀n", 5, Some(2..5)));
        s.frame(&e, Time::ZERO);
        let told = anchors(&keyboard.sync(&s, &e));
        let composing = told
            .last()
            .and_then(|a| a.composing.clone())
            .expect("what is composed is told");
        assert_eq!((composing.start, composing.text.as_str()), (2, "😀n"));
        let [smile, also, n] = composing.units[..] else {
            panic!("a box a unit: {composing:?}");
        };
        // Both units of a character outside the BMP share its box.
        assert_eq!(smile, also);
        assert_eq!(smile[2], n[0]);
        assert!(n[2] > n[0]);
        // Once it ends, nothing is composed.
        keyboard.held(&s, &e, text("ab😀n", 5, None));
        s.frame(&e, Time::ZERO);
        let told = anchors(&keyboard.sync(&s, &e));
        assert_eq!(told.last().map(|a| a.composing.clone()), Some(None));
    }

    /// What GameTextInput held, in order, while a Japanese input method
    /// typed にほん, turned it into 日本, typed ご and chose 語, recorded on
    /// an Android 16 emulator (API 36, x86_64) through a test input method
    /// calling setComposingText and commitText.
    #[test]
    fn a_japanese_composition_recorded_on_android_ends_as_its_text() {
        let (e, mut s, input) = editor("", &[]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        let field = |name| e.read_by_name(input, name).to_js_string().to_string();
        let recorded = [
            (text("に", 1, Some(0..1)), "", "に"),
            (text("にほ", 2, Some(0..2)), "", "にほ"),
            (text("にほん", 3, Some(0..3)), "", "にほん"),
            (text("日本", 2, Some(0..2)), "", "日本"),
            (text("日本", 2, None), "日本", ""),
            (text("日本ご", 3, Some(2..3)), "日本", "ご"),
            (text("日本語", 3, None), "日本語", ""),
        ];
        for (held, typed, composing) in recorded {
            let told = held.text.clone();
            keyboard.held(&s, &e, held);
            s.frame(&e, Time::ZERO);
            assert_eq!(
                (field("text"), field("preeditText")),
                (typed.into(), composing.into()),
                "{told}"
            );
            // The field took it as held: nothing is told back.
            assert_eq!(
                helds(&keyboard.sync(&s, &e)),
                Vec::<String>::new(),
                "{told}"
            );
        }
        assert_eq!(field("cursorPosition"), "3");
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

    #[test]
    fn a_hidden_field_keeps_the_order_of_a_composition_text_follows() {
        let (e, mut s, input) = editor("12", &[("echoMode", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        // Composing "b", with "c" after it.
        keyboard.held(&s, &e, text("abc", 3, Some(1..2)));
        s.frame(&e, Time::ZERO);
        // Taken whole, in its order: the method is told it with its
        // composition ended, never a text in another order.
        assert_eq!(helds(&keyboard.sync(&s, &e)), ["abc"]);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "12abc"
        );
    }

    #[test]
    fn a_hidden_field_leaves_the_method_the_cursor_it_moved() {
        let (e, mut s, input) = editor("12", &[("echoMode", Value::from(2))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("abc", 3, None));
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        // A swipe on the space bar.
        keyboard.held(&s, &e, text("abc", 1, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        keyboard.held(&s, &e, text("axbc", 2, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "12axbc"
        );
        assert_eq!(e.read_by_name(input, "cursorPosition").to_number(), 4.0);
    }

    #[test]
    fn a_field_that_clears_on_edit_takes_the_method_typing() {
        // `TextInput.PasswordEchoOnEdit`: the first edit starts over.
        let (e, mut s, input) = editor("secret", &[("echoMode", Value::from(3))]);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("a", 1, None));
        s.frame(&e, Time::ZERO);
        // The method keeps what it typed: it is not told to start anew.
        assert_eq!(helds(&keyboard.sync(&s, &e)), Vec::<String>::new());
        keyboard.held(&s, &e, text("ab", 2, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(
            e.read_by_name(input, "text").to_js_string().to_string(),
            "ab"
        );
    }

    #[test]
    fn a_field_that_clears_on_edit_refocused_has_the_method_start_over() {
        // `TextInput.PasswordEchoOnEdit`: focused again, the field starts
        // over on its next edit, so what the method typed before is not
        // in it.
        let e = Engine::new();
        ljos_quick::register(&e);
        let root = e.create_native(Item::default(), None).unwrap();
        e.complete(root).unwrap();
        let [input, other] = [3, 0].map(|echo| {
            let input = e.create_native(TextInput::default(), Some(root)).unwrap();
            ljos_quick::add_data(&e, root, input).unwrap();
            e.complete(input).unwrap();
            e.write_by_name(input, "echoMode", Value::from(echo))
                .unwrap();
            input
        });
        let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
        s.set_root(&e, root).unwrap();
        s.frame(&e, Time::ZERO);
        e.invoke_by_name(input, "forceActiveFocus", &[]).unwrap();
        s.frame(&e, Time::ZERO);
        let mut keyboard = SoftKeyboard::default();
        keyboard.sync(&s, &e);
        keyboard.held(&s, &e, text("abc", 3, None));
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        e.invoke_by_name(other, "forceActiveFocus", &[]).unwrap();
        s.frame(&e, Time::ZERO);
        keyboard.sync(&s, &e);
        e.invoke_by_name(input, "forceActiveFocus", &[]).unwrap();
        s.frame(&e, Time::ZERO);
        assert_eq!(helds(&keyboard.sync(&s, &e)), [""]);
        keyboard.held(&s, &e, text("d", 1, None));
        s.frame(&e, Time::ZERO);
        let field = || e.read_by_name(input, "text").to_js_string().to_string();
        assert_eq!(field(), "d");
        // Backspace takes back what the method typed since.
        keyboard.held(&s, &e, text("", 0, None));
        s.frame(&e, Time::ZERO);
        assert_eq!(field(), "");
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
