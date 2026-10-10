//! Key sequences: what a `Shortcut` waits for, written as Qt writes it
//! (`"Ctrl+Shift+S"`, or several chords one after the other,
//! `"Ctrl+K, Ctrl+C"`). See doc.qt.io/qt-6/qkeysequence.html; the text
//! forms were checked against Qt 6.11.

use crate::input::Modifiers;
use crate::keys::key;

/// The platform whose key bindings and key names are used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Linux, the BSDs and anything else.
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    /// The platform the program was built for.
    pub fn current() -> Platform {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// A sequence has at most this many chords, as in Qt.
pub const MAX_CHORDS: usize = 4;

/// One key with the modifiers held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// A `Qt.Key` number.
    pub key: i32,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn new(key: i32, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    fn parse(text: &str) -> Option<KeyChord> {
        let text = text.trim();
        // The last `+` ends the modifiers, unless the key is `+` itself.
        let (mods, name) = if text == "+" {
            ("", "+")
        } else if let Some(mods) = text.strip_suffix("++") {
            (mods, "+")
        } else {
            match text.rfind('+') {
                Some(at) => (&text[..at], &text[at + 1..]),
                None => ("", text),
            }
        };
        let mut modifiers = Modifiers::empty();
        if !mods.is_empty() {
            for m in mods.split('+') {
                modifiers |= match m.trim().to_ascii_lowercase().as_str() {
                    "ctrl" => Modifiers::CONTROL,
                    "shift" => Modifiers::SHIFT,
                    "alt" => Modifiers::ALT,
                    "meta" => Modifiers::META,
                    "num" => Modifiers::KEYPAD,
                    _ => return None,
                };
            }
        }
        Some(KeyChord {
            key: key::from_name(name.trim())?,
            modifiers,
        })
    }

    fn portable(self, out: &mut String) {
        // Qt's order.
        for (m, name) in [
            (Modifiers::META, "Meta+"),
            (Modifiers::CONTROL, "Ctrl+"),
            (Modifiers::ALT, "Alt+"),
            (Modifiers::SHIFT, "Shift+"),
            (Modifiers::KEYPAD, "Num+"),
        ] {
            if self.modifiers.contains(m) {
                out.push_str(name);
            }
        }
        out.push_str(&key::name(self.key).unwrap_or_default());
    }

    fn native(self, platform: Platform, out: &mut String) {
        if platform != Platform::MacOs {
            return self.portable(out);
        }
        // macOS writes symbols and no separators. `CONTROL` is the
        // Command key there and `META` the Control key, as in Qt.
        for (m, symbol) in [
            (Modifiers::META, '\u{2303}'),
            (Modifiers::ALT, '\u{2325}'),
            (Modifiers::SHIFT, '\u{21e7}'),
            (Modifiers::CONTROL, '\u{2318}'),
        ] {
            if self.modifiers.contains(m) {
                out.push(symbol);
            }
        }
        let symbol = match self.key {
            key::ESCAPE => Some('\u{238b}'),
            key::TAB => Some('\u{21e5}'),
            key::BACKTAB => Some('\u{21e4}'),
            key::BACKSPACE => Some('\u{232b}'),
            key::RETURN => Some('\u{21b5}'),
            key::ENTER => Some('\u{2324}'),
            key::DELETE => Some('\u{2326}'),
            key::HOME => Some('\u{2196}'),
            key::END => Some('\u{2198}'),
            key::LEFT => Some('\u{2190}'),
            key::UP => Some('\u{2191}'),
            key::RIGHT => Some('\u{2192}'),
            key::DOWN => Some('\u{2193}'),
            key::PAGE_UP => Some('\u{21de}'),
            key::PAGE_DOWN => Some('\u{21df}'),
            _ => None,
        };
        match symbol {
            Some(c) => out.push(c),
            None => out.push_str(&key::name(self.key).unwrap_or_default()),
        }
    }
}

/// One to four chords pressed one after the other. Empty: no sequence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeySequence(Vec<KeyChord>);

impl KeySequence {
    /// A sequence of the given chords; those after the fourth are dropped.
    pub fn new(chords: impl IntoIterator<Item = KeyChord>) -> Self {
        KeySequence(chords.into_iter().take(MAX_CHORDS).collect())
    }

    /// Reads Qt's portable text: chords separated by commas, each a key
    /// name with `Ctrl+`, `Shift+`, `Alt+`, `Meta+` and `Num+` before it,
    /// in any case and with spaces allowed around the parts. `None` when
    /// a chord cannot be read (Qt then has an empty sequence); chords
    /// after the fourth are dropped.
    pub fn parse(text: &str) -> Option<KeySequence> {
        let mut chords = Vec::new();
        let mut rest = text;
        while !rest.is_empty() && chords.len() < MAX_CHORDS {
            // A comma ends a chord, unless it is the chord's key: the
            // last character of the text, or a comma before another.
            let (chord, after) = match rest.find(',') {
                None => (rest, ""),
                Some(at) if at + 1 == rest.len() => (rest, ""),
                Some(at) if rest[at + 1..].starts_with(',') => (&rest[..=at], &rest[at + 2..]),
                Some(at) => (&rest[..at], &rest[at + 1..]),
            };
            chords.push(KeyChord::parse(chord)?);
            rest = after;
        }
        Some(KeySequence(chords))
    }

    pub fn chords(&self) -> &[KeyChord] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The text that reads the same on every platform: `"Ctrl+K, Ctrl+C"`.
    pub fn portable_text(&self) -> String {
        self.text(|c, out| c.portable(out))
    }

    /// The text the platform shows its users.
    pub fn native_text(&self, platform: Platform) -> String {
        self.text(|c, out| c.native(platform, out))
    }

    fn text(&self, one: impl Fn(KeyChord, &mut String)) -> String {
        let mut out = String::new();
        for (i, c) in self.0.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            one(*c, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portable(text: &str) -> String {
        KeySequence::parse(text).map_or_else(String::new, |s| s.portable_text())
    }

    #[test]
    fn a_chord_is_modifiers_and_a_key() {
        let s = KeySequence::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(
            s.chords(),
            [KeyChord::new(0x53, Modifiers::CONTROL | Modifiers::SHIFT)]
        );
        let s = KeySequence::parse("Ctrl+K, Ctrl+C").unwrap();
        assert_eq!(
            s.chords(),
            [
                KeyChord::new(0x4b, Modifiers::CONTROL),
                KeyChord::new(0x43, Modifiers::CONTROL)
            ]
        );
    }

    // Each line is what Qt 6.11 makes of the text.
    #[test]
    fn text_is_normalised_as_qt_does() {
        for (text, want) in [
            ("Shift+Alt+Ctrl+Meta+A", "Meta+Ctrl+Alt+Shift+A"),
            ("ctrl+shift+del", "Ctrl+Shift+Del"),
            ("Ctrl++", "Ctrl++"),
            ("+", "+"),
            ("Ctrl+,, Ctrl+A", "Ctrl+,, Ctrl+A"),
            (",", ","),
            ("Shift+,", "Shift+,"),
            ("a,b,c,d,e", "A, B, C, D"),
            ("A ,B", "A, B"),
            ("Ctrl + S", "Ctrl+S"),
            (" A ", "A"),
            ("Ctrl+Escape", "Ctrl+Esc"),
            ("Shift+Tab", "Shift+Tab"),
            ("Ctrl+Space", "Ctrl+Space"),
            ("Ctrl+Ctrl+A", "Ctrl+A"),
            ("Ctrl+Volume Up", "Ctrl+Volume Up"),
            ("Num+Enter", "Num+Enter"),
            ("Ctrl+Shift", "Ctrl+Shift"),
            ("Ctrl+é", "Ctrl+É"),
            ("", ""),
            // Not sequences.
            ("Ctrl+Nonsense", ""),
            ("PageUp", ""),
            ("Ctrl+Plus", ""),
            ("Shift+", ""),
            ("Ctrl+", ""),
            ("A+B", ""),
            ("Ctrl+A+B", ""),
            ("A,", ""),
        ] {
            assert_eq!(portable(text), want, "{text:?}");
        }
    }

    #[test]
    fn native_text_is_portable_text_off_the_mac() {
        let s = KeySequence::parse("Ctrl+Shift+Del").unwrap();
        assert_eq!(s.native_text(Platform::Linux), "Ctrl+Shift+Del");
        assert_eq!(s.native_text(Platform::Windows), "Ctrl+Shift+Del");
        assert_eq!(s.native_text(Platform::MacOs), "\u{21e7}\u{2318}\u{2326}");
    }
}
