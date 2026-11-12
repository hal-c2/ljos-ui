//! The enums of Qt's `Qt` namespace as types of `Ljos.Ui`, as
//! `src/ljos_enums.txt` lists them (docs/DIALECT.md §4.3.1): `Qt.AlignLeft`
//! is `Align.Left`, `Qt.Key_0` is `Key.Digit0`.
//!
//! The engine makes the table and registers the types; the text is what the
//! crates that read QML, which do not link the engine, know of it. It is
//! committed, and `crates/ljos_cli/tests/own_types.rs` writes it beside
//! `ljos.txt` and fails when it is not what the engine has.
//!
//! The format: a line per enum, `<type> flags|enum <Qt's enum>`, then a line
//! per key, indented by a tab, `<Qt's key> <Ljós's key> <number>`.

use std::{collections::HashMap, sync::OnceLock};

const TEXT: &str = include_str!("ljos_enums.txt");

/// One enum of `Qt` as a type of `Ljos.Ui`.
#[derive(Debug)]
pub struct LjosEnum {
    /// `Align`, or `Permission.Status` for an enum that is a member of a
    /// type.
    pub name: &'static str,
    /// Qt's name of the enum: `Alignment`.
    pub qt: &'static str,
    /// Its keys are combined with `|`.
    pub flags: bool,
    /// Qt's key, Ljós's key and the number.
    pub keys: Vec<(&'static str, &'static str, i64)>,
}

impl LjosEnum {
    /// The type that holds the enum: `Permission` of `Permission.Status`.
    pub fn type_name(&self) -> &'static str {
        self.name.split_once('.').map_or(self.name, |(ty, _)| ty)
    }
}

/// Where a key of `Qt` is in Ljós's API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Home {
    /// `Align`, `Permission.Status`.
    pub ty: &'static str,
    /// `Left` of `Align.Left`.
    pub key: &'static str,
    pub value: i64,
}

impl std::fmt::Display for Home {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.ty, self.key)
    }
}

struct Table {
    enums: Vec<LjosEnum>,
    by_qt_key: HashMap<&'static str, Home>,
    by_qt_enum: HashMap<&'static str, usize>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut enums: Vec<LjosEnum> = Vec::new();
        for line in TEXT
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            if let Some(key) = line.strip_prefix('\t') {
                let mut words = key.split(' ');
                let (Some(qt), Some(ljos), Some(value), Some(last)) =
                    (words.next(), words.next(), words.next(), enums.last_mut())
                else {
                    continue;
                };
                last.keys.push((qt, ljos, value.parse().unwrap_or(0)));
            } else {
                let mut words = line.split(' ');
                let (Some(name), Some(kind), Some(qt)) = (words.next(), words.next(), words.next())
                else {
                    continue;
                };
                enums.push(LjosEnum {
                    name,
                    qt,
                    flags: kind == "flags",
                    keys: Vec::new(),
                });
            }
        }
        let mut by_qt_key = HashMap::new();
        let mut by_qt_enum = HashMap::new();
        for (at, e) in enums.iter().enumerate() {
            by_qt_enum.insert(e.qt, at);
            for &(qt, key, value) in &e.keys {
                by_qt_key.entry(qt).or_insert(Home {
                    ty: e.name,
                    key,
                    value,
                });
            }
        }
        Table {
            enums,
            by_qt_key,
            by_qt_enum,
        }
    })
}

/// Every enum, in the order of the text.
pub fn enums() -> &'static [LjosEnum] {
    &table().enums
}

/// Where `Qt.<key>` is: `Align.Left` of `AlignLeft`.
pub fn home(key: &str) -> Option<Home> {
    table().by_qt_key.get(key).copied()
}

/// The enum Qt names `name` (`Alignment`, `PermissionStatus`).
pub fn of_qt_enum(name: &str) -> Option<&'static LjosEnum> {
    table().by_qt_enum.get(name).map(|&at| &table().enums[at])
}

/// The text, as committed.
pub fn text() -> &'static str {
    TEXT
}
