//! `StandardKey` (QtQuick): the names of the common shortcuts, and the key
//! sequences each one is on a platform
//! (doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts).
//!
//! The Linux column is what Qt 6.11 answers there; the Windows and Apple
//! columns are the documentation's table.

use std::cell::Cell;

use ljos_engine::{Engine, QmlObject};

use super::sequence::{KeyChord, KeySequence, Platform};
use crate::enums::EnumTable;
use crate::input::Modifiers;

/// `StandardKey` (QtQuick): only its enum is used, `StandardKey.Copy`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "StandardKey",
    uncreatable,
    enums = standard_key_enums
)]
pub struct StandardKey {}

/// One standard key: its name, and its sequences on Linux, Windows and
/// the Apple platforms, the preferred one first, separated by `|`.
type Row = (&'static str, &'static str, &'static str, &'static str);

/// In the order of Qt's enum: a row's place is its value.
const TABLE: &[Row] = &[
    ("UnknownKey", "", "", ""),
    ("HelpContents", "F1", "F1", "Ctrl+?"),
    ("WhatsThis", "Shift+F1", "Shift+F1", "Shift+F1"),
    ("Open", "Ctrl+O", "Ctrl+O", "Ctrl+O"),
    ("Close", "Ctrl+W", "Ctrl+F4|Ctrl+W", "Ctrl+W|Ctrl+F4"),
    ("Save", "Ctrl+S", "Ctrl+S", "Ctrl+S"),
    ("New", "Ctrl+N", "Ctrl+N", "Ctrl+N"),
    ("Delete", "Del|Ctrl+D", "Del", "Del|Meta+D"),
    (
        "Cut",
        "Ctrl+X|F20|Shift+Del",
        "Ctrl+X|Shift+Del",
        "Ctrl+X|Meta+K",
    ),
    ("Copy", "Ctrl+C|F16|Ctrl+Ins", "Ctrl+C|Ctrl+Ins", "Ctrl+C"),
    (
        "Paste",
        "Ctrl+V|F18|Shift+Ins",
        "Ctrl+V|Shift+Ins",
        "Ctrl+V|Meta+Y",
    ),
    ("Undo", "Ctrl+Z|F14", "Ctrl+Z|Alt+Backspace", "Ctrl+Z"),
    (
        "Redo",
        "Ctrl+Shift+Z",
        "Ctrl+Y|Ctrl+Shift+Z|Alt+Shift+Backspace",
        "Ctrl+Shift+Z",
    ),
    ("Back", "Alt+Left", "Alt+Left|Backspace", "Ctrl+["),
    (
        "Forward",
        "Alt+Right",
        "Alt+Right|Shift+Backspace",
        "Ctrl+]",
    ),
    ("Refresh", "Ctrl+R|F5", "F5", "F5"),
    ("ZoomIn", "Ctrl++", "Ctrl++", "Ctrl++"),
    ("ZoomOut", "Ctrl+-", "Ctrl+-", "Ctrl+-"),
    ("Print", "Ctrl+P", "Ctrl+P", "Ctrl+P"),
    ("AddTab", "Ctrl+T", "Ctrl+T", "Ctrl+T"),
    (
        "NextChild",
        "Ctrl+Tab|Forward",
        "Ctrl+Tab|Forward|Ctrl+F6",
        "Ctrl+}|Forward|Ctrl+Tab",
    ),
    (
        "PreviousChild",
        "Ctrl+Shift+Backtab|Back",
        "Ctrl+Shift+Backtab|Back|Ctrl+Shift+F6",
        "Ctrl+{|Back|Ctrl+Shift+Backtab",
    ),
    ("Find", "Ctrl+F", "Ctrl+F", "Ctrl+F"),
    ("FindNext", "Ctrl+G|F3", "F3|Ctrl+G", "Ctrl+G"),
    (
        "FindPrevious",
        "Ctrl+Shift+G|Shift+F3",
        "Shift+F3|Ctrl+Shift+G",
        "Ctrl+Shift+G",
    ),
    ("Replace", "Ctrl+H", "Ctrl+H", ""),
    ("SelectAll", "Ctrl+A", "Ctrl+A", "Ctrl+A"),
    ("Bold", "Ctrl+B", "Ctrl+B", "Ctrl+B"),
    ("Italic", "Ctrl+I", "Ctrl+I", "Ctrl+I"),
    ("Underline", "Ctrl+U", "Ctrl+U", "Ctrl+U"),
    ("MoveToNextChar", "Right", "Right", "Right|Meta+F"),
    ("MoveToPreviousChar", "Left", "Left", "Left|Meta+B"),
    ("MoveToNextWord", "Ctrl+Right", "Ctrl+Right", "Alt+Right"),
    ("MoveToPreviousWord", "Ctrl+Left", "Ctrl+Left", "Alt+Left"),
    ("MoveToNextLine", "Down", "Down", "Down|Meta+N"),
    ("MoveToPreviousLine", "Up", "Up", "Up|Meta+P"),
    (
        "MoveToNextPage",
        "PgDown",
        "PgDown",
        "PgDown|Alt+PgDown|Meta+Down|Meta+PgDown|Meta+V",
    ),
    (
        "MoveToPreviousPage",
        "PgUp",
        "PgUp",
        "PgUp|Alt+PgUp|Meta+Up|Meta+PgUp",
    ),
    ("MoveToStartOfLine", "Home", "Home", "Ctrl+Left|Meta+Left"),
    (
        "MoveToEndOfLine",
        "End|Ctrl+E",
        "End",
        "Ctrl+Right|Meta+Right",
    ),
    ("MoveToStartOfBlock", "", "", "Alt+Up|Meta+A"),
    ("MoveToEndOfBlock", "", "", "Alt+Down|Meta+E"),
    (
        "MoveToStartOfDocument",
        "Ctrl+Home",
        "Ctrl+Home",
        "Ctrl+Up|Home",
    ),
    (
        "MoveToEndOfDocument",
        "Ctrl+End",
        "Ctrl+End",
        "Ctrl+Down|End",
    ),
    (
        "SelectNextChar",
        "Shift+Right",
        "Shift+Right",
        "Shift+Right",
    ),
    (
        "SelectPreviousChar",
        "Shift+Left",
        "Shift+Left",
        "Shift+Left",
    ),
    (
        "SelectNextWord",
        "Ctrl+Shift+Right",
        "Ctrl+Shift+Right",
        "Alt+Shift+Right",
    ),
    (
        "SelectPreviousWord",
        "Ctrl+Shift+Left",
        "Ctrl+Shift+Left",
        "Alt+Shift+Left",
    ),
    ("SelectNextLine", "Shift+Down", "Shift+Down", "Shift+Down"),
    ("SelectPreviousLine", "Shift+Up", "Shift+Up", "Shift+Up"),
    (
        "SelectNextPage",
        "Shift+PgDown",
        "Shift+PgDown",
        "Shift+PgDown",
    ),
    (
        "SelectPreviousPage",
        "Shift+PgUp",
        "Shift+PgUp",
        "Shift+PgUp",
    ),
    (
        "SelectStartOfLine",
        "Shift+Home",
        "Shift+Home",
        "Ctrl+Shift+Left|Meta+Shift+Left",
    ),
    (
        "SelectEndOfLine",
        "Shift+End",
        "Shift+End",
        "Ctrl+Shift+Right|Meta+Shift+Right",
    ),
    ("SelectStartOfBlock", "", "", "Alt+Shift+Up|Meta+Shift+A"),
    ("SelectEndOfBlock", "", "", "Alt+Shift+Down|Meta+Shift+E"),
    (
        "SelectStartOfDocument",
        "Ctrl+Shift+Home",
        "Ctrl+Shift+Home",
        "Ctrl+Shift+Up|Shift+Home",
    ),
    (
        "SelectEndOfDocument",
        "Ctrl+Shift+End",
        "Ctrl+Shift+End",
        "Ctrl+Shift+Down|Shift+End",
    ),
    (
        "DeleteStartOfWord",
        "Ctrl+Backspace",
        "Ctrl+Backspace",
        "Alt+Backspace",
    ),
    ("DeleteEndOfWord", "Ctrl+Del", "Ctrl+Del", ""),
    ("DeleteEndOfLine", "Ctrl+K", "", ""),
    // The key people call Enter is Return; the keypad's is Enter. Qt
    // names the keypad's first and takes both.
    (
        "InsertParagraphSeparator",
        "Enter|Return",
        "Enter|Return",
        "Enter|Return",
    ),
    (
        "InsertLineSeparator",
        "Shift+Enter|Shift+Return",
        "Shift+Enter|Shift+Return",
        "Meta+Enter|Meta+Return|Meta+O",
    ),
    ("SaveAs", "Ctrl+Shift+S", "Ctrl+Shift+S", "Ctrl+Shift+S"),
    ("Preferences", "", "", "Ctrl+,"),
    ("Quit", "Ctrl+Q", "", "Ctrl+Q"),
    ("FullScreen", "F11", "F11|Alt+Enter", "Ctrl+Meta+F"),
    ("Deselect", "Ctrl+Shift+A", "", ""),
    ("DeleteCompleteLine", "Ctrl+U", "", ""),
    ("Backspace", "", "", "Meta+H"),
    ("Cancel", "Esc", "Esc", "Esc|Ctrl+."),
];

pub(crate) fn standard_key_enums() -> EnumTable {
    vec![(
        "StandardKey",
        TABLE
            .iter()
            .enumerate()
            .map(|(i, row)| (row.0, i as i64))
            .collect(),
    )]
}

/// The value of a standard key by its name: `"Copy"` is 9.
pub fn standard_key(name: &str) -> Option<i32> {
    TABLE.iter().position(|row| row.0 == name).map(|i| i as i32)
}

/// The sequences a standard key is on `platform`, the preferred one
/// first. Empty for a key the platform has no sequence for, and for a
/// number that is no standard key.
pub fn standard_key_sequences(key: i32, platform: Platform) -> Vec<KeySequence> {
    let Some(row) = usize::try_from(key).ok().and_then(|i| TABLE.get(i)) else {
        return Vec::new();
    };
    let list = match platform {
        Platform::Linux => row.1,
        Platform::Windows => row.2,
        Platform::MacOs => row.3,
    };
    list.split('|')
        .filter(|s| !s.is_empty())
        .filter_map(KeySequence::parse)
        .collect()
}

/// The modifiers that tell one key sequence from another.
pub(crate) const CHORD_MODIFIERS: Modifiers = Modifiers::SHIFT
    .union(Modifiers::CONTROL)
    .union(Modifiers::ALT)
    .union(Modifiers::META);

/// `KeyEvent.matches(StandardKey)`: whether the key with the modifiers
/// held is one of the standard key's sequences on `platform`.
pub fn matches_standard_key(
    key: i32,
    modifiers: Modifiers,
    standard: i32,
    platform: Platform,
) -> bool {
    let chord = KeyChord::new(key, modifiers & CHORD_MODIFIERS);
    standard_key_sequences(standard, platform)
        .iter()
        .any(|s| s.chords() == [chord])
}

/// The platform an engine takes its key bindings from.
#[derive(Default)]
struct PlatformSlot(Cell<Option<Platform>>);

/// The platform whose key bindings `engine` uses: the one the program was
/// built for, unless [`set_platform`] said otherwise.
pub fn platform(engine: &Engine) -> Platform {
    engine
        .extension::<PlatformSlot>()
        .0
        .get()
        .unwrap_or_else(Platform::current)
}

/// Makes `engine` use the key bindings of `platform`: for a host that
/// knows better than the build target (a browser), and for tests.
pub fn set_platform(engine: &Engine, platform: Platform) {
    engine.extension::<PlatformSlot>().0.set(Some(platform));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::key;

    #[test]
    fn the_table_is_in_the_order_of_qts_enum() {
        assert_eq!(TABLE.len(), 71);
        for (name, value) in [
            ("UnknownKey", 0),
            ("Copy", 9),
            ("Find", 22),
            ("MoveToNextChar", 30),
            ("SelectNextChar", 44),
            ("SelectEndOfDocument", 57),
            ("DeleteStartOfWord", 58),
            ("InsertParagraphSeparator", 61),
            ("SaveAs", 63),
            ("Cancel", 70),
        ] {
            assert_eq!(standard_key(name), Some(value), "{name}");
        }
    }

    #[test]
    fn every_sequence_in_the_table_reads() {
        for row in TABLE {
            for list in [row.1, row.2, row.3] {
                for s in list.split('|').filter(|s| !s.is_empty()) {
                    let seq = KeySequence::parse(s);
                    assert!(seq.is_some_and(|q| !q.is_empty()), "{}: {s:?}", row.0);
                }
            }
        }
    }

    #[test]
    fn the_first_sequence_is_the_preferred_one() {
        let first = |name: &str, p| {
            standard_key_sequences(standard_key(name).unwrap(), p)
                .first()
                .map(KeySequence::portable_text)
                .unwrap_or_default()
        };
        assert_eq!(first("Copy", Platform::Linux), "Ctrl+C");
        assert_eq!(first("Redo", Platform::Linux), "Ctrl+Shift+Z");
        assert_eq!(first("Redo", Platform::Windows), "Ctrl+Y");
        assert_eq!(first("ZoomIn", Platform::Linux), "Ctrl++");
        assert_eq!(first("Preferences", Platform::MacOs), "Ctrl+,");
        assert_eq!(first("Preferences", Platform::Linux), "");
        assert_eq!(first("Cancel", Platform::Linux), "Esc");
        assert!(standard_key_sequences(999, Platform::Linux).is_empty());
        assert!(standard_key_sequences(-1, Platform::Linux).is_empty());
    }

    #[test]
    fn matches_looks_at_every_sequence_of_the_key() {
        let copy = standard_key("Copy").unwrap();
        let c = 0x43;
        let l = Platform::Linux;
        assert!(matches_standard_key(c, Modifiers::CONTROL, copy, l));
        assert!(matches_standard_key(
            key::INSERT,
            Modifiers::CONTROL,
            copy,
            l
        ));
        assert!(!matches_standard_key(c, Modifiers::empty(), copy, l));
        assert!(!matches_standard_key(
            c,
            Modifiers::CONTROL | Modifiers::SHIFT,
            copy,
            l
        ));
        // The keypad does not make it another key.
        assert!(matches_standard_key(
            c,
            Modifiers::CONTROL | Modifiers::KEYPAD,
            copy,
            l
        ));
        assert!(!matches_standard_key(
            key::INSERT,
            Modifiers::CONTROL,
            copy,
            Platform::MacOs
        ));
    }
}
