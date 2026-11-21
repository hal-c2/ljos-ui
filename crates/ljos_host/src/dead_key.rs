//! A dead key on a hardware keyboard: the accent it types waits for the
//! next key and goes onto it (´ then e is é). Only the Android host uses
//! it; it builds everywhere so that its tests run on every host.

/// What a pressed key types, by the keyboard's character map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Typed {
    /// A character.
    Char(char),
    /// An accent that waits for the next key (a dead key).
    Accent(char),
}

/// The accent a dead key left waiting, if any.
#[derive(Debug, Default)]
pub(crate) struct DeadKey {
    accent: Option<char>,
}

impl DeadKey {
    /// The text a press types, if it types any now. `compose` is the
    /// keyboard's own composition of an accent with the character after
    /// it, which also says what an accent typed twice or before a space
    /// is. What it does not compose types the accent, then the key.
    pub(crate) fn press(
        &mut self,
        typed: Typed,
        compose: impl FnOnce(char, char) -> Option<char>,
    ) -> Option<String> {
        let c = match typed {
            Typed::Char(c) | Typed::Accent(c) => c,
        };
        let Some(accent) = self.accent.take() else {
            return match typed {
                Typed::Char(c) => Some(c.to_string()),
                Typed::Accent(a) => {
                    self.accent = Some(a);
                    None
                }
            };
        };
        if let Some(both) = compose(accent, c) {
            return Some(both.to_string());
        }
        match typed {
            Typed::Char(c) => Some(format!("{accent}{c}")),
            Typed::Accent(a) => {
                self.accent = Some(a);
                Some(accent.to_string())
            }
        }
    }

    /// A key that types nothing (Backspace, an arrow, Enter) drops the
    /// accent waiting: it was not meant for whatever comes after.
    pub(crate) fn other(&mut self) {
        self.accent = None;
    }

    /// A key the keyboard has no character for (F1, a media key) drops the
    /// accent too, except a modifier: Shift is held for the letter the
    /// accent goes on, so the accent then Shift+E is É.
    pub(crate) fn untyped(&mut self, modifier: bool) {
        if !modifier {
            self.other();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeadKey, Typed};

    const ACUTE: char = '\u{b4}';
    const GRAVE: char = '`';

    /// Android's composition for the accents the tests use: the accent
    /// twice or before a space is the accent itself.
    fn compose(accent: char, c: char) -> Option<char> {
        match (accent, c) {
            (a, b) if a == b || b == ' ' => Some(a),
            (ACUTE, 'e') => Some('é'),
            (ACUTE, 'E') => Some('É'),
            (GRAVE, 'a') => Some('à'),
            _ => None,
        }
    }

    fn typing(keys: &[Typed]) -> Vec<Option<String>> {
        let mut dead = DeadKey::default();
        keys.iter().map(|&k| dead.press(k, compose)).collect()
    }

    #[test]
    fn an_accent_then_a_letter_types_the_accented_letter() {
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Char('e')]),
            [None, Some("é".into())]
        );
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Char('E'), Typed::Char('e')]),
            [None, Some("É".into()), Some("e".into())]
        );
    }

    #[test]
    fn a_modifier_keeps_the_accent_and_a_key_without_a_character_drops_it() {
        let mut dead = DeadKey::default();
        assert_eq!(dead.press(Typed::Accent(ACUTE), compose), None);
        dead.untyped(true);
        assert_eq!(dead.press(Typed::Char('E'), compose), Some("É".into()));
        assert_eq!(dead.press(Typed::Accent(ACUTE), compose), None);
        dead.untyped(false);
        assert_eq!(dead.press(Typed::Char('e'), compose), Some("e".into()));
    }

    #[test]
    fn an_accent_twice_or_before_a_space_types_the_accent() {
        let accent = Some(ACUTE.to_string());
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Accent(ACUTE)]),
            [None, accent.clone()]
        );
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Char(' ')]),
            [None, accent]
        );
    }

    #[test]
    fn an_accent_that_does_not_go_on_the_letter_is_typed_before_it() {
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Char('q')]),
            [None, Some(format!("{ACUTE}q"))]
        );
    }

    #[test]
    fn a_second_accent_types_the_first_and_waits_itself() {
        assert_eq!(
            typing(&[Typed::Accent(ACUTE), Typed::Accent(GRAVE), Typed::Char('a')]),
            [None, Some(ACUTE.to_string()), Some("à".into())]
        );
    }

    #[test]
    fn a_key_that_types_nothing_drops_the_accent() {
        let mut dead = DeadKey::default();
        assert_eq!(dead.press(Typed::Accent(ACUTE), compose), None);
        dead.other();
        assert_eq!(dead.press(Typed::Char('e'), compose), Some("e".into()));
    }
}
