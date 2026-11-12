//! The enums of Qt's `Qt` namespace as types of `Ljos.Ui`
//! (docs/DIALECT.md §4.3.1): `Qt.AlignLeft` is `Align.Left`, `Qt.Key_0` is
//! `Key.Digit0`, `Qt.ControlModifier` is `Modifier.Control`.
//!
//! A key of `Qt` never says which enum it belongs to, so in Ljós's API each
//! enum is a type of its own, named for what it is, and its keys drop the
//! part they share with it. The names follow a rule, and the few the rule
//! reads badly are written by hand ([`TYPE_NAMES`], [`KEY_NAMES`]). A key
//! keeps Qt's number, so a value passes between a Ljós file and a kept Qt
//! file unchanged.
//!
//! The table is generated from here into `crates/qt_types/src/ljos_enums.txt`
//! (by the `own_types` test of `ljos_cli`), which is what the compiler, the
//! checker and the lowering pass read: they do not link the engine.

use std::sync::OnceLock;

use crate::qt_enums::QT_ENUMS;

/// One enum of `Qt` as a type of `Ljos.Ui`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LjosEnum {
    /// The type a file names: `Align`, or `Permission.Status` for an enum
    /// that is a member of a type.
    pub name: &'static str,
    /// The enum's name in Qt's namespace, as a property is declared with
    /// it: `Alignment`, `CursorShape`, `Key`.
    pub qt: &'static str,
    /// Its keys are combined with `|` (`Align.Left | Align.Top`). A plain
    /// enum is a union of its keys to the checker, and a `switch` on one is
    /// held to cover them.
    pub flags: bool,
    pub keys: Vec<LjosKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LjosKey {
    /// `Left` of `Align.Left`.
    pub name: String,
    /// `AlignLeft` of `Qt.AlignLeft`.
    pub qt: &'static str,
    pub value: i64,
}

impl LjosEnum {
    /// The type that holds the enum: `Permission` of `Permission.Status`.
    pub fn type_name(&self) -> &'static str {
        self.name.split_once('.').map_or(self.name, |(ty, _)| ty)
    }

    /// The enum's own name inside its type: `Status` of
    /// `Permission.Status`, and the type's name otherwise.
    pub fn enum_name(&self) -> &'static str {
        self.name
            .split_once('.')
            .map_or(self.name, |(_, name)| name)
    }
}

/// Why the table cannot be made: what the rule leaves of a key cannot be
/// written after a `.`, or two keys of a type would have one name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error(
        "`Qt.{key}` of `{ty}` would be `{ty}.{name}`, which is no name: write one in KEY_NAMES"
    )]
    NoName {
        ty: &'static str,
        key: &'static str,
        name: String,
    },
    #[error("`Qt.{key}` and `Qt.{other}` would both be `{ty}.{name}`: write one in KEY_NAMES")]
    Collides {
        ty: &'static str,
        key: &'static str,
        other: &'static str,
        name: String,
    },
}

/// The enums of `Qt` whose Ljós names are written by hand: the short ones
/// docs/DIALECT.md §4.3.1 names, and one the rule would give twice.
/// Every other enum is named by its singular name (a flags' one key's
/// enum, `MouseButton` of `MouseButtons`), or its name.
const TYPE_NAMES: &[(&str, &str)] = &[
    ("Alignment", "Align"),
    ("KeyboardModifiers", "Modifier"),
    ("TextElideMode", "Elide"),
    ("CursorShape", "Cursor"),
    ("LayoutDirection", "Direction"),
    ("PermissionStatus", "Permission.Status"),
    // Qt's `Modifier` is the modifiers of a key sequence (`META`, `CTRL`),
    // which the rule would name `Modifier`, taken by `KeyboardModifiers`.
    ("Modifiers", "KeyModifier"),
    // Its one key's enum is `SplitBehaviorFlags`.
    ("SplitBehavior", "SplitBehavior"),
];

/// Flags in Qt (an enum with a name for one key's enum) whose keys are
/// never combined: Qt's `Orientations` holds one orientation where an item
/// has one.
const PLAIN: &[&str] = &["Orientations"];

/// Keys the rule would leave starting with a digit or with nothing, or
/// would read as something else, by the Ljós type and Qt's key.
const KEY_NAMES: &[(&str, &str, &str)] = &[
    ("Key", "Key_0", "Digit0"),
    ("Key", "Key_1", "Digit1"),
    ("Key", "Key_2", "Digit2"),
    ("Key", "Key_3", "Digit3"),
    ("Key", "Key_4", "Digit4"),
    ("Key", "Key_5", "Digit5"),
    ("Key", "Key_6", "Digit6"),
    ("Key", "Key_7", "Digit7"),
    ("Key", "Key_8", "Digit8"),
    ("Key", "Key_9", "Digit9"),
    // The rule would leave `M` of Qt's masks of the pen's styles.
    ("PenCapStyle", "MPenCapStyle", "Mask"),
    ("PenJoinStyle", "MPenJoinStyle", "Mask"),
];

/// Qt's colours are no enum in Ljós's API: a colour is its name
/// (`"red"`), where it goes to a `color`.
const NOT_ENUMS: &[&str] = &["GlobalColor"];

/// The table, made once. Panics when the table is refused: that is a
/// mistake in this file, which the `own_types` test of `ljos_cli` and the
/// tests here say first.
pub fn ljos_enums() -> &'static [LjosEnum] {
    static TABLE: OnceLock<Vec<LjosEnum>> = OnceLock::new();
    TABLE.get_or_init(|| match table() {
        Ok(table) => table,
        Err(error) => panic!("the table of Ljós's enums is refused: {error}"),
    })
}

/// The table as `crates/qt_types/src/ljos_enums.txt` has it (see that
/// crate's `ljos_enums`): what the crates that read QML know of it.
pub fn written() -> String {
    let mut out = String::from(
        "# The enums of Qt's `Qt` namespace as types of `Ljos.Ui` (docs/DIALECT.md\n\
         # §4.3.1): a line per enum, `<type> flags|enum <Qt's enum>`, then a line per\n\
         # key, `<Qt's key> <Ljós's key> <number>`.\n\
         #\n\
         # Do not edit: `crates/ljos_cli/tests/own_types.rs` writes this file from\n\
         # the engine's table (`crates/ljos_engine/src/ljos_enums.rs`).\n",
    );
    for e in ljos_enums() {
        let kind = if e.flags { "flags" } else { "enum" };
        out.push_str(&format!("\n{} {kind} {}\n", e.name, e.qt));
        for key in &e.keys {
            out.push_str(&format!("\t{} {} {}\n", key.qt, key.name, key.value));
        }
    }
    out
}

/// Makes the table, or says why it cannot be made.
pub fn table() -> Result<Vec<LjosEnum>, TableError> {
    let keys = QT_ENUMS
        .iter()
        .filter(|e| !NOT_ENUMS.contains(&e.name))
        .map(|e| (e.name, e.alias, e.keys.to_vec()))
        .chain(std::iter::once((
            "Key",
            "",
            crate::qt_keys::QT_KEY_ENUMS
                .iter()
                .filter(|(key, _)| key.starts_with("Key_"))
                .copied()
                .collect(),
        )));
    keys.map(|(qt, alias, keys)| made(qt, alias, &keys))
        .collect()
}

fn made(
    qt: &'static str,
    alias: &'static str,
    keys: &[(&'static str, i64)],
) -> Result<LjosEnum, TableError> {
    let single = if alias.is_empty() { qt } else { alias };
    let name = TYPE_NAMES
        .iter()
        .find(|(from, _)| *from == qt)
        .map_or(single, |(_, to)| *to);
    let short = name.rsplit('.').next().unwrap_or(name);
    let affixes = [short, single, qt];
    let mut made: Vec<LjosKey> = Vec::with_capacity(keys.len());
    for &(key, value) in keys {
        let hand = KEY_NAMES
            .iter()
            .find(|(ty, from, _)| *ty == name && *from == key)
            .map(|(_, _, to)| (*to).to_owned());
        let ljos = hand.unwrap_or_else(|| ruled(key, &affixes));
        if !ljos.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(TableError::NoName {
                ty: name,
                key,
                name: ljos,
            });
        }
        if let Some(other) = made.iter().find(|k| k.name == ljos && k.value != value) {
            return Err(TableError::Collides {
                ty: name,
                key,
                other: other.qt,
                name: ljos,
            });
        }
        made.push(LjosKey {
            name: ljos,
            qt: key,
            value,
        });
    }
    Ok(LjosEnum {
        name,
        qt,
        flags: !alias.is_empty() && !PLAIN.contains(&qt),
        keys: made,
    })
}

/// The words of a name: `AlignHCenter` is `Align`, `HCenter`;
/// `Key_Super_L` is `Key`, `Super`, `L`; `IBeamCursor` is `IBeam`,
/// `Cursor`; `Dense1Pattern` is `Dense1`, `Pattern`.
fn words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    for part in name.split('_').filter(|part| !part.is_empty()) {
        let chars: Vec<(usize, char)> = part.char_indices().collect();
        let mut start = 0;
        for i in 1..chars.len() {
            let (at, c) = chars[i];
            let before = chars[i - 1].1;
            let after = chars.get(i + 1).map(|(_, c)| *c);
            // A capital starts a word after a small letter or a digit, and
            // the last capital of a run starts one when a small letter
            // follows it (`IBeam` + `Cursor`, `HCenter`).
            let starts = c.is_ascii_uppercase()
                && (before.is_ascii_lowercase()
                    || before.is_ascii_digit()
                    || (before.is_ascii_uppercase()
                        && after.is_some_and(|a| a.is_ascii_lowercase())));
            if starts {
                words.push(&part[start..at]);
                start = at;
            }
        }
        words.push(&part[start..]);
    }
    words
}

/// What the rule makes of `key`: the words it shares with the enum's name
/// at its start or its end are dropped, the most that can be (one word is
/// kept), `No…` of a whole enum is `None` and `All…s` is `All`, and the
/// first letter is a capital.
fn ruled(key: &str, affixes: &[&str]) -> String {
    let key_words = words(key);
    let mut best: Option<(Vec<&str>, bool)> = None;
    for affix in affixes {
        let affix_words = words(affix);
        // Every run of the affix's words from its start (`Window` and
        // `WindowState` of `WindowState`), and its last word (`State`).
        let mut runs: Vec<Vec<&str>> = (1..=affix_words.len())
            .map(|n| affix_words[..n].to_vec())
            .collect();
        if let Some(last) = affix_words.last() {
            runs.push(vec![*last]);
        }
        for run in runs {
            let n = run.len();
            if key_words.len() <= n {
                continue;
            }
            let (rest, at_end) = if key_words[..n] == run[..] {
                (key_words[n..].to_vec(), false)
            } else if key_words[key_words.len() - n..] == run[..] || plural_end(&key_words, &run) {
                (key_words[..key_words.len() - n].to_vec(), true)
            } else {
                continue;
            };
            if best.as_ref().is_none_or(|(b, _)| rest.len() < b.len()) {
                best = Some((rest, at_end));
            }
        }
    }
    let (rest, at_end) = best.unwrap_or((key_words, false));
    if at_end && rest == ["No"] {
        return "None".to_owned();
    }
    if at_end && rest == ["All"] {
        return "All".to_owned();
    }
    let mut name = rest.concat();
    if let Some(first) = name.get(..1) {
        let upper = first.to_ascii_uppercase();
        name.replace_range(..1, &upper);
    }
    name
}

/// `AllButtons`, `NoItemFlags`: the key ends with the run's words, its last
/// one plural.
fn plural_end(key: &[&str], run: &[&str]) -> bool {
    let n = run.len();
    let tail = &key[key.len() - n..];
    tail[..n - 1] == run[..n - 1] && tail[n - 1].strip_suffix('s') == Some(run[n - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(ty: &str, qt: &str) -> String {
        let table = table().expect("the table is made");
        let e = table.iter().find(|e| e.name == ty).expect(ty);
        e.keys
            .iter()
            .find(|k| k.qt == qt)
            .unwrap_or_else(|| panic!("{qt} of {ty}"))
            .name
            .clone()
    }

    #[test]
    fn the_names_docs_dialect_gives_are_the_tables() {
        assert_eq!(key("Align", "AlignLeft"), "Left");
        assert_eq!(key("Align", "AlignHCenter"), "HCenter");
        assert_eq!(key("Key", "Key_Return"), "Return");
        assert_eq!(key("Key", "Key_A"), "A");
        assert_eq!(key("Key", "Key_0"), "Digit0");
        assert_eq!(key("Key", "Key_No"), "No");
        assert_eq!(key("MouseButton", "LeftButton"), "Left");
        assert_eq!(key("MouseButton", "AllButtons"), "All");
        assert_eq!(key("MouseButton", "NoButton"), "None");
        assert_eq!(key("Modifier", "ControlModifier"), "Control");
        assert_eq!(key("Modifier", "NoModifier"), "None");
        assert_eq!(key("Orientation", "Horizontal"), "Horizontal");
        assert_eq!(key("Elide", "ElideRight"), "Right");
        assert_eq!(key("Cursor", "PointingHandCursor"), "PointingHand");
        assert_eq!(key("CheckState", "Checked"), "Checked");
        assert_eq!(key("Direction", "RightToLeft"), "RightToLeft");
        assert_eq!(key("Permission.Status", "Granted"), "Granted");
        assert_eq!(key("ItemFlag", "NoItemFlags"), "None");
    }

    #[test]
    fn the_table_says_which_enums_are_flags() {
        let table = table().expect("the table is made");
        let flags = |name: &str| table.iter().find(|e| e.name == name).expect(name).flags;
        assert!(flags("Align") && flags("Modifier") && flags("MouseButton"));
        assert!(!flags("Permission.Status") && !flags("Orientation") && !flags("CheckState"));
        assert!(!flags("Key") && !flags("Cursor"));
    }

    #[test]
    fn every_key_of_qt_has_a_home_with_its_number() {
        let table = table().expect("the table is made");
        let home = |qt: &str| {
            table
                .iter()
                .flat_map(|e| e.keys.iter())
                .find(|k| k.qt == qt)
                .map(|k| k.value)
        };
        for e in QT_ENUMS.iter().filter(|e| !NOT_ENUMS.contains(&e.name)) {
            for (key, value) in e.keys {
                assert_eq!(home(key), Some(*value), "Qt.{key}");
            }
        }
        for (key, value) in crate::qt_keys::QT_KEY_ENUMS
            .iter()
            .filter(|(k, _)| k.starts_with("Key_"))
        {
            assert_eq!(home(key), Some(*value), "Qt.{key}");
        }
    }

    #[test]
    fn a_key_the_rule_leaves_as_a_digit_is_refused() {
        assert_eq!(
            made("Key", "", &[("Key_7x", 1)]).unwrap_err(),
            TableError::NoName {
                ty: "Key",
                key: "Key_7x",
                name: "7x".into()
            }
        );
        assert!(matches!(
            made("Thing", "", &[("ThingLeft", 1), ("LeftThing", 2)]),
            Err(TableError::Collides { .. })
        ));
    }

    #[test]
    fn every_type_has_one_name() {
        let table = table().expect("the table is made");
        let mut names: Vec<&str> = table.iter().map(|e| e.name).collect();
        names.sort_unstable();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len, "two enums of Qt would be one type");
    }
}
