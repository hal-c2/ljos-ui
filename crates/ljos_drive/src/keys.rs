//! Keys as they are named in a step, a tool call or a test: `Ctrl+Shift+S`,
//! `Tab`, `Ctrl++`.

use ljos_quick::Modifiers;
use ljos_quick::keys::key;

/// The modifier keys in the order a chord presses them.
pub const MODIFIER_KEYS: [(Modifiers, i32); 4] = [
    (Modifiers::CONTROL, key::CONTROL),
    (Modifiers::SHIFT, key::SHIFT),
    (Modifiers::ALT, key::ALT),
    (Modifiers::META, key::META),
];

/// One key with the modifiers held around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: i32,
    /// What the key types with those modifiers, as a Qt key event's `text`
    /// carries it.
    pub text: String,
}

impl Chord {
    /// Reads a chord: modifiers (`Ctrl`, `Shift`, `Alt`, `Meta`, and the
    /// other names people give them) joined to a key by `+`. Keys have Qt's
    /// names: `Return`, and `Enter`, which is the keypad's.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (mods, name) = match text.strip_suffix("++") {
            Some(mods) => (mods, "+"),
            None => match text.rsplit_once('+') {
                Some((mods, name)) if !name.is_empty() => (mods, name),
                _ => ("", text),
            },
        };
        let mut modifiers = Modifiers::empty();
        for m in mods.split('+').filter(|m| !m.is_empty()) {
            modifiers |= match m.to_ascii_lowercase().as_str() {
                "shift" => Modifiers::SHIFT,
                "ctrl" | "control" => Modifiers::CONTROL,
                "alt" => Modifiers::ALT,
                "meta" | "super" | "cmd" => Modifiers::META,
                _ => {
                    return Err(format!(
                        "unknown modifier {m:?} in the chord {text:?}; the modifiers are Ctrl, \
                         Shift, Alt and Meta"
                    ));
                }
            };
        }
        // The spellings the Qt oracle's keyboard also takes.
        let name = match name {
            n if n.eq_ignore_ascii_case("PageUp") => "PgUp",
            n if n.eq_ignore_ascii_case("PageDown") => "PgDown",
            n => n,
        };
        let code = key::from_name(name).ok_or_else(|| {
            format!(
                "unknown key {name:?} in the chord {text:?}; keys are named as `Return`, `Tab`, \
                 `Escape`, `Backspace`, `Left`, `F5`, a letter or digit, with modifiers before \
                 them such as `Ctrl+Shift+S`"
            )
        })?;
        let shift = modifiers.contains(Modifiers::SHIFT);
        // A desktop sends Shift+Tab as the Backtab key.
        let code = if code == key::TAB && shift {
            key::BACKTAB
        } else {
            code
        };
        let commands = Modifiers::CONTROL | Modifiers::ALT | Modifiers::META;
        let mut chars = name.chars();
        let typed = match (chars.next(), chars.next()) {
            (Some(_), None) if modifiers.intersects(commands) => String::new(),
            _ if modifiers.intersects(commands) => key::control_text(code).to_owned(),
            (Some(c), None) if shift => c.to_uppercase().collect(),
            (Some(c), None) => c.to_lowercase().collect(),
            _ if code == key::SPACE => " ".to_owned(),
            // A Qt key event carries the control character its key types:
            // Tab's, which a `TextEdit` types, and Return's.
            _ => key::control_text(code).to_owned(),
        };
        Ok(Self {
            modifiers,
            key: code,
            text: typed,
        })
    }

    /// The key a keyboard with a key for each character presses to type
    /// `c`: a line break is Return and a tab is Tab, with the control
    /// character Qt's key event carries for them.
    pub fn of_char(c: char) -> Self {
        let (code, text) = match c {
            '\n' => (key::RETURN, key::control_text(key::RETURN).to_owned()),
            '\t' => (key::TAB, key::control_text(key::TAB).to_owned()),
            c => (key::from_char(c).unwrap_or(0), c.to_string()),
        };
        Self {
            modifiers: if c.is_uppercase() {
                Modifiers::SHIFT
            } else {
                Modifiers::empty()
            },
            key: code,
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_name_modifiers_then_a_key() {
        let c = Chord::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(c.modifiers, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!((c.key, c.text.as_str()), (0x53, ""));
        let c = Chord::parse("a").unwrap();
        assert_eq!((c.key, c.text.as_str()), (key::A, "a"));
        let c = Chord::parse("Shift+A").unwrap();
        assert_eq!((c.key, c.text.as_str()), (key::A, "A"));
        assert_eq!(Chord::parse("+").unwrap().key, key::PLUS);
        assert_eq!(Chord::parse("PageUp").unwrap().key, key::PAGE_UP);
        assert_eq!(Chord::parse("Space").unwrap().text, " ");
        assert_eq!(Chord::parse("F12").unwrap().text, "");
    }

    #[test]
    fn a_letter_with_a_command_types_nothing() {
        let c = Chord::parse("Ctrl+A").unwrap();
        assert_eq!(c.modifiers, Modifiers::CONTROL);
        assert_eq!(c.key, i32::from(b'A'));
        assert_eq!(c.text, "");
    }

    #[test]
    fn named_keys_carry_their_control_character() {
        assert_eq!(Chord::parse("Return").unwrap().text, "\r");
        assert_eq!(Chord::parse("Ctrl+Return").unwrap().text, "\r");
        assert_eq!(Chord::parse("Backspace").unwrap().text, "\u{8}");
        assert_eq!(Chord::parse("Enter").unwrap().key, key::ENTER);
    }

    #[test]
    fn shift_tab_is_backtab_and_plus_is_a_key() {
        assert_eq!(Chord::parse("Shift+Tab").unwrap().key, key::BACKTAB);
        let plus = Chord::parse("Ctrl++").unwrap();
        assert_eq!(plus.key, i32::from(b'+'));
        assert_eq!(plus.modifiers, Modifiers::CONTROL);
    }

    #[test]
    fn other_names_of_modifiers_are_taken() {
        let c = Chord::parse("Control+Super+X").unwrap();
        assert_eq!(c.modifiers, Modifiers::CONTROL | Modifiers::META);
    }

    #[test]
    fn an_unknown_key_is_named_in_the_error() {
        let e = Chord::parse("Hyper+Banana").unwrap_err();
        assert!(e.contains("Banana"), "{e}");
        let e = Chord::parse("Banana").unwrap_err();
        assert!(e.contains("Banana"), "{e}");
    }

    #[test]
    fn a_line_break_is_typed_with_return() {
        let c = Chord::of_char('\n');
        assert_eq!((c.key, c.text.as_str()), (key::RETURN, "\r"));
        let c = Chord::of_char('Q');
        assert_eq!((c.key, c.modifiers), (i32::from(b'Q'), Modifiers::SHIFT));
    }
}
