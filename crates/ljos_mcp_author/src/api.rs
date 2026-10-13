//! What Ljós has of a Qt Quick type or member: the one answer `api_status`
//! gives and `ljos check --port` reports from (ARCHITECTURE.md §17, §19.3).
//!
//! The status of `Module.Type[.member]` is the ledger's row when there is one.
//! The ledger is the curated record (it can say `dropped` or `partial`, and
//! which scenario proves a row), but it is a seed that grows by hand, so
//! without a row the answer is derived from the two tables of types
//! (`qt_types`):
//!
//! - **supported** when Ljós's own table (`ljos.txt`) has it;
//! - **unsupported** when it has not and Qt's table does. That needs Qt's
//!   table, which only some machines have;
//! - no answer when neither table has it: a slip, or (in a build without
//!   Qt's table) something only Qt could confirm.
//!
//! Ljós's own table is read here on its own. `qt_types`' lookups answer from
//! Qt's description wherever there is one, which is right for reading QML
//! but cannot say what Ljós implements.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use crate::{ApiStatus, Ledger, LedgerRow, MemberKind};

/// The reason of a row derived from the table of types.
pub const NOT_IN_TABLE: &str = "not in Ljós's type table";
const IN_TABLE: &str = "in Ljós's type table (no ledger row)";

/// The status of `module.type_name[.member]`, or None when neither the
/// ledger nor the tables know it. `member` may be a path (`font.pixelSize`)
/// or a handler (`onClicked`).
pub fn status(
    ledger: Option<&Ledger>,
    module: &str,
    type_name: &str,
    member: Option<&str>,
) -> Option<LedgerRow> {
    if let Some(row) = ledger.and_then(|l| l.row(module, type_name, member)) {
        return Some(row.clone());
    }
    derive(module, type_name, member)
}

/// The status of a type or member from the tables alone.
pub fn derive(module: &str, type_name: &str, member: Option<&str>) -> Option<LedgerRow> {
    let row = |kind, status, reason: &str| LedgerRow {
        module: module.to_owned(),
        type_name: type_name.to_owned(),
        member: member.map(str::to_owned),
        kind,
        status,
        reason: Some(reason.to_owned()),
        scenarios: Vec::new(),
    };
    let own = own();
    let ours = own.type_named(module, type_name);
    let Some(member) = member else {
        return if ours.is_some() {
            Some(row(MemberKind::Type, ApiStatus::Supported, IN_TABLE))
        } else if qt_has_type(module, type_name) {
            Some(row(MemberKind::Type, ApiStatus::Unsupported, NOT_IN_TABLE))
        } else {
            None
        };
    };
    if let Some(kind) = ours.and_then(|ty| own.member_path(ty, member)) {
        return Some(row(kind, ApiStatus::Supported, IN_TABLE));
    }
    let kind = qt_member(module, type_name, member)?;
    let reason = if ours.is_some() {
        NOT_IN_TABLE.to_owned()
    } else {
        format!("{NOT_IN_TABLE}: `{module}.{type_name}` is not either")
    };
    Some(row(kind, ApiStatus::Unsupported, &reason))
}

/// Splits `Module.Type[.member]`. Modules contain dots (`QtQuick.Controls`),
/// so the longest prefix that is a module of either table is the module.
pub fn split_query(query: &str) -> Option<(String, String, Option<String>)> {
    let parts: Vec<&str> = query.split('.').collect();
    for k in (1..parts.len()).rev() {
        let module = parts[..k].join(".");
        if !(own().modules.contains_key(module.as_str()) || qt_types::module(&module).is_some()) {
            continue;
        }
        let member = (k + 1 < parts.len()).then(|| parts[k + 1..].join("."));
        return Some((module, parts[k].to_owned(), member));
    }
    None
}

/// The modules either table has: what an answer to an unknown query may
/// suggest.
pub fn modules() -> Vec<String> {
    let mut all: Vec<String> = own().modules.keys().map(|m| (*m).to_owned()).collect();
    all.extend(qt_types::modules().into_iter().map(str::to_owned));
    all.sort();
    all.dedup();
    all
}

/// Whether this build can tell what Qt has that Ljós has not. Without Qt's
/// table [`derive`] has no answer for anything Ljós lacks, and a caller has to
/// decide what that silence means.
pub fn has_qt_table() -> bool {
    qt_types::has_qt_table()
}

/// Whether Ljós's own table has the module.
pub fn has_module(module: &str) -> bool {
    own().modules.contains_key(module)
}

fn qt_type(module: &str, name: &str) -> Option<&'static qt_types::Type> {
    if !qt_types::has_qt_table() {
        return None;
    }
    let module = qt_types::module(module).filter(|m| !m.is_own)?;
    module.type_named(name).filter(|ty| !ty.is_own)
}

/// Whether Qt's table, when this build has one, has the type.
fn qt_has_type(module: &str, name: &str) -> bool {
    qt_type(module, name).is_some()
}

/// What Qt's table says `member` is on the type, when it has it.
fn qt_member(module: &str, type_name: &str, member: &str) -> Option<MemberKind> {
    let ty = qt_type(module, type_name)?;
    let mut path = member.split('.');
    let first = path.next()?;
    let kind = qt_kind(ty, first)?;
    // The rest of a path are members of the value the first holds.
    let mut value = ty.property(first).and_then(qt_types::Property::value_type);
    for name in path {
        let next = value?;
        qt_kind(next, name)?;
        value = next.property(name).and_then(qt_types::Property::value_type);
    }
    Some(kind)
}

pub(crate) fn qt_kind(ty: &'static qt_types::Type, name: &str) -> Option<MemberKind> {
    if let Some(signal) = qml_front::scope::handled(name)
        && ty.property(name).is_none()
    {
        let changed = signal
            .strip_suffix("Changed")
            .is_some_and(|p| ty.property(p).is_some());
        if ty.signal(&signal).is_some() || changed || ty.attaches(&signal) {
            return Some(MemberKind::Signal);
        }
        return None;
    }
    if ty.property(name).is_some() {
        Some(MemberKind::Property)
    } else if ty.signal(name).is_some() {
        Some(MemberKind::Signal)
    } else if ty.has_method(name) {
        Some(MemberKind::Method)
    } else if ty.enum_value(name).is_some() {
        Some(MemberKind::Enum)
    } else if ty.attaches(name) {
        Some(MemberKind::Property)
    } else {
        None
    }
}

// ---- Ljós's own table ----------------------------------------------------

/// How a type refers to another: by class, or `named` as a file that
/// imports the module would (`QtQuick.Item`).
#[derive(Clone, Copy)]
enum Link {
    Class(&'static str),
    Named(&'static str),
}

struct Prop {
    name: &'static str,
    type_name: &'static str,
    named: bool,
}

#[derive(Default)]
struct Ty {
    prototype: Option<Link>,
    extension: Option<(Link, bool)>,
    attached: Option<Link>,
    props: Vec<Prop>,
    signals: HashSet<&'static str>,
    methods: HashSet<&'static str>,
    /// Enum names, aliases and keys alike: each is a name `Type.name`.
    enums: HashSet<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportKind {
    Always,
    Default,
    Optional,
}

#[derive(Default)]
struct Module {
    imports: Vec<(&'static str, ImportKind)>,
    exports: HashMap<&'static str, &'static str>,
}

struct Own {
    modules: HashMap<&'static str, Module>,
    types: HashMap<&'static str, Ty>,
}

/// How many links of a chain are followed before it is taken to loop.
const DEPTH: usize = 64;

fn own() -> &'static Own {
    static OWN: OnceLock<Own> = OnceLock::new();
    OWN.get_or_init(|| Own::read(qt_types::own_text()))
}

impl Own {
    /// The format is `qt_types`'; only what the lookups here need is kept.
    fn read(text: &'static str) -> Self {
        let mut own = Own {
            modules: HashMap::new(),
            types: HashMap::new(),
        };
        let mut module = "";
        let mut class = "";
        for line in text.lines() {
            let depth = line.len() - line.trim_start_matches('\t').len();
            let mut words = line.split_ascii_whitespace();
            let mut word = || words.next().unwrap_or("");
            match (depth, word()) {
                (0, "module") => {
                    module = word();
                    own.modules.entry(module).or_default();
                }
                (0, _) => {}
                (1, "import") => {
                    let uri = word();
                    let kind = match word() {
                        "default" => ImportKind::Default,
                        "optional" => ImportKind::Optional,
                        _ => ImportKind::Always,
                    };
                    if let Some(m) = own.modules.get_mut(module) {
                        m.imports.push((uri, kind));
                    }
                }
                (1, "type") => {
                    class = word();
                    own.types.entry(class).or_default();
                }
                (1, name) => {
                    let exported = word();
                    if let Some(m) = own.modules.get_mut(module) {
                        m.exports.insert(name, exported);
                    }
                }
                (_, member) => {
                    let Some(ty) = own.types.get_mut(class) else {
                        continue;
                    };
                    let link = |to, named| {
                        if named {
                            Link::Named(to)
                        } else {
                            Link::Class(to)
                        }
                    };
                    match member {
                        "prototype" => {
                            let to = word();
                            ty.prototype = Some(link(to, word() == "named"));
                        }
                        "attached" => {
                            let to = word();
                            ty.attached = Some(link(to, word() == "named"));
                        }
                        "extension" => {
                            let to = word();
                            ty.extension = Some((Link::Class(to), word() == "namespace"));
                        }
                        "prop" => {
                            let (name, type_name) = (word(), word());
                            ty.props.push(Prop {
                                name,
                                type_name,
                                named: words.any(|w| w == "named"),
                            });
                        }
                        "signal" => {
                            ty.signals.insert(word());
                        }
                        "method" => {
                            ty.methods.insert(word());
                        }
                        "enum" | "flags" => {
                            let name = word();
                            for part in name.split('=') {
                                ty.enums.insert(part);
                            }
                            for key in words {
                                ty.enums.insert(key.split('=').next().unwrap_or(key));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        own
    }

    /// The type `name` is in a file that imports `module`: one of its own,
    /// or of a module that comes with it.
    fn type_named(&self, module: &str, name: &str) -> Option<&Ty> {
        let class = self.class_named(module, name, &mut Vec::new())?;
        self.types.get(class)
    }

    fn class_named(
        &self,
        uri: &str,
        name: &str,
        seen: &mut Vec<&'static str>,
    ) -> Option<&'static str> {
        let (key, module) = self.modules.get_key_value(uri)?;
        if seen.contains(key) {
            return None;
        }
        seen.push(key);
        if let Some(class) = module.exports.get(name) {
            return Some(class);
        }
        let chooses = module
            .imports
            .iter()
            .any(|(_, kind)| *kind == ImportKind::Optional);
        module
            .imports
            .iter()
            .filter(|(_, kind)| match kind {
                ImportKind::Always => true,
                ImportKind::Default => chooses,
                ImportKind::Optional => false,
            })
            .find_map(|(uri, _)| self.class_named(uri, name, seen))
    }

    fn resolve(&self, link: Link) -> Option<&Ty> {
        match link {
            Link::Class(class) => self.types.get(class),
            Link::Named(named) => {
                let (module, name) = named.rsplit_once('.')?;
                self.type_named(module, name)
            }
        }
    }

    /// The type, then what it inherits, nearest first.
    fn chain<'s>(&'s self, ty: &'s Ty) -> impl Iterator<Item = &'s Ty> {
        let mut next = Some(ty);
        let mut left = DEPTH;
        std::iter::from_fn(move || {
            let current = next?;
            left = left.checked_sub(1)?;
            next = current.prototype.and_then(|link| self.resolve(link));
            Some(current)
        })
    }

    fn kind_in(&self, ty: &Ty, name: &str) -> Option<MemberKind> {
        for t in self.chain(ty) {
            if t.props.iter().any(|p| p.name == name) {
                return Some(MemberKind::Property);
            }
            if t.signals.contains(name) {
                return Some(MemberKind::Signal);
            }
            if t.methods.contains(name) {
                return Some(MemberKind::Method);
            }
            if t.enums.contains(name) {
                return Some(MemberKind::Enum);
            }
            // A namespace extension is there for its enums alone.
            if let Some((link, namespace)) = t.extension
                && let Some(ext) = self.resolve(link)
                && (!namespace || ext.enums.contains(name))
                && let Some(kind) = self.kind_in(ext, name)
            {
                return Some(kind);
            }
        }
        None
    }

    /// `name` on the type or what it inherits; a handler (`onClicked`) is its
    /// signal, and a property's `Changed` signal needs no line of its own.
    fn member(&self, ty: &Ty, name: &str) -> Option<MemberKind> {
        if let Some(signal) = qml_front::scope::handled(name)
            && self.kind_in(ty, name).is_none()
        {
            let changed = signal
                .strip_suffix("Changed")
                .is_some_and(|p| self.kind_in(ty, p) == Some(MemberKind::Property));
            return (self.kind_in(ty, &signal) == Some(MemberKind::Signal) || changed)
                .then_some(MemberKind::Signal);
        }
        if let Some(kind) = self.kind_in(ty, name) {
            return Some(kind);
        }
        if let Some(property) = name.strip_suffix("Changed")
            && self.kind_in(ty, property) == Some(MemberKind::Property)
        {
            return Some(MemberKind::Signal);
        }
        None
    }

    /// As [`member`](Self::member), then among what the type attaches to
    /// other objects: `Layout.fillWidth`, `Keys.onPressed`.
    fn member_or_attached(&self, ty: &Ty, name: &str) -> Option<MemberKind> {
        if let Some(kind) = self.member(ty, name) {
            return Some(kind);
        }
        let attached = self
            .chain(ty)
            .find_map(|t| t.attached)
            .and_then(|link| self.resolve(link))?;
        self.member(attached, name)
    }

    /// A member that may be a path through value types: `font.pixelSize`.
    fn member_path(&self, ty: &Ty, path: &str) -> Option<MemberKind> {
        let mut names = path.split('.');
        let first = names.next()?;
        let kind = self.member_or_attached(ty, first)?;
        let mut value = self.value_type(ty, first);
        for name in names {
            // A value whose type this table does not describe cannot be
            // told to lack the name.
            let Some(next) = value else {
                return Some(kind);
            };
            self.member(next, name)?;
            value = self.value_type(next, name);
        }
        Some(kind)
    }

    /// The type a property holds, when this table describes it.
    fn value_type(&self, ty: &Ty, property: &str) -> Option<&Ty> {
        let prop = self
            .chain(ty)
            .find_map(|t| t.props.iter().find(|p| p.name == property))?;
        self.resolve(if prop.named {
            Link::Named(prop.type_name)
        } else {
            Link::Class(prop.type_name)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_modules_that_contain_dots() {
        let (module, ty, member) = split_query("QtQuick.Controls.Button.checked").unwrap();
        assert_eq!(
            (module.as_str(), ty.as_str()),
            ("QtQuick.Controls", "Button")
        );
        assert_eq!(member.as_deref(), Some("checked"));
        let (module, ty, member) = split_query("QtQuick.Item").unwrap();
        assert_eq!(
            (module.as_str(), ty.as_str(), member),
            ("QtQuick", "Item", None)
        );
        assert!(split_query("Nowhere.Item").is_none());
        assert!(split_query("QtQuick").is_none());
    }

    #[test]
    fn a_type_of_ljos_is_supported_and_members_come_along_the_base_chain() {
        let row = derive("QtQuick", "Rectangle", None).unwrap();
        assert_eq!(row.status, ApiStatus::Supported);
        // `width` is Item's, `color` Rectangle's.
        for (member, kind) in [
            ("width", MemberKind::Property),
            ("color", MemberKind::Property),
            ("widthChanged", MemberKind::Signal),
            ("onWidthChanged", MemberKind::Signal),
            ("border.width", MemberKind::Property),
            ("forceActiveFocus", MemberKind::Method),
        ] {
            let row = derive("QtQuick", "Rectangle", Some(member)).unwrap();
            assert_eq!(
                (row.status, row.kind),
                (ApiStatus::Supported, kind),
                "{member}"
            );
        }
    }

    #[test]
    fn attached_members_are_members_of_the_type_that_attaches() {
        let row = derive("QtQuick", "Keys", Some("onPressed")).unwrap();
        assert_eq!(row.status, ApiStatus::Supported);
        let row = derive("QtQuick.Layouts", "Layout", Some("fillWidth")).unwrap();
        assert_eq!(row.status, ApiStatus::Supported);
    }

    #[test]
    fn what_no_table_has_has_no_derived_status() {
        assert!(derive("QtQuick", "Nope", None).is_none());
        assert!(derive("QtQuick", "Nope", Some("width")).is_none());
        assert!(derive("QtQuick", "Rectangle", Some("colr")).is_none());
        assert!(derive("Nowhere", "Item", None).is_none());
    }

    /// Only with Qt's table can a type or member Ljós lacks be told from a
    /// slip.
    #[test]
    fn what_qt_has_and_ljos_lacks_is_unsupported() {
        if !qt_types::test_has_qt_table("what_qt_has_and_ljos_lacks_is_unsupported") {
            return;
        }
        let missing = qt_types::modules()
            .into_iter()
            .filter(|m| !qt_types::module(m).is_some_and(|m| m.is_own))
            .flat_map(|m| {
                qt_types::native_types(m)
                    .into_iter()
                    .flatten()
                    .map(move |t| (m, t))
            })
            .find(|(m, t)| derive(m, t, None).is_some_and(|r| r.status == ApiStatus::Unsupported));
        let Some((module, name)) = missing else {
            return; // Ljós has everything of Qt's: nothing to find.
        };
        let row = derive(module, name, None).unwrap();
        assert_eq!(row.reason.as_deref(), Some(NOT_IN_TABLE));
    }

    #[test]
    fn the_ledger_wins_over_the_tables() {
        let ledger = Ledger::parse(
            "[[row]]\nmodule='QtQuick'\ntype='Rectangle'\nmember='color'\nkind='property'\n\
             status='dropped'\nreason='use palette'\n",
        )
        .unwrap();
        let row = status(Some(&ledger), "QtQuick", "Rectangle", Some("color")).unwrap();
        assert_eq!(row.status, ApiStatus::Dropped);
        // A row for a member says nothing of the type or of its other members.
        let row = status(Some(&ledger), "QtQuick", "Rectangle", None).unwrap();
        assert_eq!(row.status, ApiStatus::Supported);
        let row = status(Some(&ledger), "QtQuick", "Rectangle", Some("radius")).unwrap();
        assert_eq!(row.status, ApiStatus::Supported);
    }
}
