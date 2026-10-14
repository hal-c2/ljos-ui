//! `ljos check --port`: which Qt Quick types and members an app uses that
//! Ljós lacks (ARCHITECTURE.md §17, §19.3).
//!
//! What a project uses is read by the authoring server's front end, and what
//! Ljós has of it is answered by the same function that answers the MCP tool
//! `api_status`, so the two never disagree. Only the weighing and the
//! report are here.
//!
//! The ledger is not read unless `--ledger` names one: it is a seed that grows
//! by hand, and a row in it that says a type is still in progress would flag
//! every use of the type itself (a row names one type or one member exactly;
//! it says nothing about the other). The tables of types are the default
//! source of truth.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::Path,
};

use anyhow::{Context as _, bail};
use ljos_compiler::baseline;
use ljos_mcp_author::{ApiStatus, Ledger, MemberKind, QmlFrontEnd, Resolution, api};
use serde_json::{Value, json};

use super::Check;

/// The module column of a use the front end could not place.
const UNPLACED: &str = "(unplaced)";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Verdict {
    /// Ljós has it, whole or in part.
    Have,
    /// Qt has it and Ljós has not, or the ledger says it was dropped.
    Missing(String),
    /// Nothing in reach can say what it is.
    Unknown(String),
}

#[derive(Debug)]
struct Item {
    kind: MemberKind,
    uses: usize,
    files: BTreeSet<String>,
    verdict: Verdict,
}

/// (module, type as written, member).
type Key = (String, String, Option<String>);

#[derive(Debug, Default)]
struct Report {
    files: usize,
    unparsed: Vec<String>,
    items: BTreeMap<Key, Item>,
}

pub(super) fn run(check: &Check) -> anyhow::Result<()> {
    let ledger = match &check.ledger {
        Some(path) => Some(Ledger::load(path).context("reading the ledger")?),
        None => None,
    };
    let mut report = Report::default();
    for dir in &check.dirs {
        if !dir.is_dir() {
            bail!("{} is not a directory", dir.display());
        }
        if check.tree {
            for project in baseline::project_dirs(dir) {
                let label = project.strip_prefix(dir).unwrap_or(&project);
                report.project(&project, &label.to_string_lossy(), ledger.as_ref())?;
            }
        } else {
            let label = if check.dirs.len() > 1 {
                dir.to_string_lossy().into_owned()
            } else {
                String::new()
            };
            report.project(dir, &label, ledger.as_ref())?;
        }
    }
    if check.json {
        let text = serde_json::to_string_pretty(&report.to_json())?;
        println!("{text}");
    } else {
        print!("{}", report.text(check.limit));
    }
    let missing = report.count(|v| matches!(v, Verdict::Missing(_)));
    if check.deny_missing && missing > 0 {
        bail!("{missing} types or members Ljós lacks");
    }
    Ok(())
}

impl Report {
    fn project(&mut self, dir: &Path, label: &str, ledger: Option<&Ledger>) -> anyhow::Result<()> {
        let root = dir
            .canonicalize()
            .with_context(|| format!("reading {}", dir.display()))?;
        let usage = QmlFrontEnd::new(root).api_uses();
        let named = |file: &str| {
            if label.is_empty() {
                file.to_owned()
            } else {
                format!("{label}/{file}")
            }
        };
        self.files += usage.files;
        self.unparsed
            .extend(usage.unparsed.iter().map(|f| named(f)));
        for used in usage.uses {
            let verdict = match &used.resolution {
                Resolution::Unresolved(why) => Verdict::Unknown(why.clone()),
                Resolution::Resolved => verdict(
                    ledger,
                    &used.module,
                    &used.type_name,
                    used.member.as_deref(),
                ),
            };
            let module = if used.module.is_empty() {
                UNPLACED.to_owned()
            } else {
                used.module.clone()
            };
            let item = self
                .items
                .entry((module, used.type_name, used.member))
                .or_insert_with(|| Item {
                    kind: used.kind,
                    uses: 0,
                    files: BTreeSet::new(),
                    verdict,
                });
            item.uses += 1;
            item.files.insert(named(&used.file));
        }
        Ok(())
    }

    fn count(&self, pick: impl Fn(&Verdict) -> bool) -> usize {
        self.items.values().filter(|i| pick(&i.verdict)).count()
    }

    /// Modules with the most missing first, so what blocks a port is on top.
    fn modules(&self) -> Vec<ModuleRow<'_>> {
        let mut rows: BTreeMap<&str, ModuleRow<'_>> = BTreeMap::new();
        for ((module, ..), item) in &self.items {
            let row = rows.entry(module).or_insert_with(|| ModuleRow::new(module));
            match item.verdict {
                Verdict::Have => row.have += 1,
                Verdict::Missing(_) => {
                    row.missing += 1;
                    row.missing_uses += item.uses;
                }
                Verdict::Unknown(_) => row.unknown += 1,
            }
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by(|a, b| {
            (b.missing_uses, b.missing, b.have, b.unknown)
                .cmp(&(a.missing_uses, a.missing, a.have, a.unknown))
                .then(a.module.cmp(b.module))
        });
        rows
    }

    /// Most used first; ties by the files they are in, then by name.
    fn sorted<'a>(
        &'a self,
        module: &str,
        pick: impl Fn(&Verdict) -> bool,
    ) -> Vec<(&'a Key, &'a Item)> {
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|((m, ..), item)| m == module && pick(&item.verdict))
            .collect();
        items.sort_by(|(ka, a), (kb, b)| {
            b.uses
                .cmp(&a.uses)
                .then(b.files.len().cmp(&a.files.len()))
                .then(ka.cmp(kb))
        });
        items
    }

    fn text(&self, limit: Option<usize>) -> String {
        let mut out = String::new();
        let total_types = self
            .items
            .values()
            .filter(|i| i.kind == MemberKind::Type)
            .count();
        let total_members = self.items.len() - total_types;
        let uses: usize = self.items.values().map(|i| i.uses).sum();
        let _ = writeln!(
            out,
            "{} files, {uses} uses of {total_types} types and {total_members} members",
            self.files
        );
        if !self.unparsed.is_empty() {
            let _ = writeln!(
                out,
                "{} files did not parse, so what they use is not counted:",
                self.unparsed.len()
            );
            for file in &self.unparsed {
                let _ = writeln!(out, "  {file}");
            }
        }
        let have = self.count(|v| *v == Verdict::Have);
        let missing = self.count(|v| matches!(v, Verdict::Missing(_)));
        let unknown = self.count(|v| matches!(v, Verdict::Unknown(_)));
        let _ = writeln!(
            out,
            "Ljós has {have}, lacks {missing}, cannot place {unknown}\n"
        );

        let modules = self.modules();
        let width = modules
            .iter()
            .map(|m| m.module.chars().count())
            .max()
            .unwrap_or(0)
            .max("module".len());
        let _ = writeln!(
            out,
            "{:<width$}  {:>9}  {:>7}  {:>7}  {:>12}",
            "module", "supported", "missing", "unknown", "missing uses"
        );
        for m in &modules {
            let _ = writeln!(
                out,
                "{:<width$}  {:>9}  {:>7}  {:>7}  {:>12}",
                m.module, m.have, m.missing, m.unknown, m.missing_uses
            );
        }

        let is_missing = |v: &Verdict| matches!(v, Verdict::Missing(_));
        let is_unknown = |v: &Verdict| matches!(v, Verdict::Unknown(_));
        for (title, pick) in [
            ("missing", &is_missing as &dyn Fn(&Verdict) -> bool),
            ("unknown", &is_unknown),
        ] {
            let mut heading = false;
            for m in &modules {
                let items = self.sorted(m.module, pick);
                if items.is_empty() {
                    continue;
                }
                if !heading {
                    let _ = writeln!(out, "\n{title}");
                    heading = true;
                }
                let _ = writeln!(out, "{}", m.module);
                let shown = limit.unwrap_or(items.len());
                let names: Vec<String> = items.iter().map(|(key, _)| name(key)).collect();
                let name_width = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
                for ((_, item), name) in items.iter().zip(&names).take(shown) {
                    let _ = writeln!(
                        out,
                        "  {:<8} {name:<name_width$}  {:>4} {} in {} {}  {}",
                        kind_name(item.kind),
                        item.uses,
                        plural(item.uses, "use"),
                        item.files.len(),
                        plural(item.files.len(), "file"),
                        reason(&item.verdict),
                    );
                }
                if items.len() > shown {
                    let _ = writeln!(out, "  ... and {} more", items.len() - shown);
                }
            }
        }
        out
    }

    fn to_json(&self) -> Value {
        let item = |key: &Key, item: &Item| {
            json!({
                "type": key.1,
                "member": key.2,
                "kind": kind_name(item.kind),
                "uses": item.uses,
                "files": item.files.len(),
                "in": item.files,
                "reason": reason(&item.verdict),
            })
        };
        let modules: Vec<Value> = self
            .modules()
            .iter()
            .map(|m| {
                let list = |pick: fn(&Verdict) -> bool| -> Vec<Value> {
                    self.sorted(m.module, pick)
                        .into_iter()
                        .map(|(k, i)| item(k, i))
                        .collect()
                };
                json!({
                    "module": m.module,
                    "supported": m.have,
                    "missing": m.missing,
                    "unknown": m.unknown,
                    "missing_uses": m.missing_uses,
                    "missing_items": list(|v| matches!(v, Verdict::Missing(_))),
                    "unknown_items": list(|v| matches!(v, Verdict::Unknown(_))),
                })
            })
            .collect();
        json!({
            "files": self.files,
            "unparsed": self.unparsed,
            "supported": self.count(|v| *v == Verdict::Have),
            "missing": self.count(|v| matches!(v, Verdict::Missing(_))),
            "unknown": self.count(|v| matches!(v, Verdict::Unknown(_))),
            "uses": self.items.values().map(|i| i.uses).sum::<usize>(),
            "modules": modules,
        })
    }
}

struct ModuleRow<'a> {
    module: &'a str,
    have: usize,
    missing: usize,
    unknown: usize,
    missing_uses: usize,
}

impl<'a> ModuleRow<'a> {
    fn new(module: &'a str) -> Self {
        Self {
            module,
            have: 0,
            missing: 0,
            unknown: 0,
            missing_uses: 0,
        }
    }
}

/// What Ljós has of a type or member the front end placed in a module.
fn verdict(
    ledger: Option<&Ledger>,
    module: &str,
    type_name: &str,
    member: Option<&str>,
) -> Verdict {
    match api::status(ledger, module, type_name, member) {
        Some(row) => match row.status {
            ApiStatus::Supported | ApiStatus::Partial => Verdict::Have,
            ApiStatus::Dropped | ApiStatus::Unsupported => {
                Verdict::Missing(row.reason.unwrap_or_else(|| row.status.as_str().to_owned()))
            }
        },
        // Qt's table would have said: a name in neither is a slip or the
        // app's own. Without it, a name Ljós lacks is all that is known.
        None if api::has_qt_table() => Verdict::Unknown(
            "in neither Ljós's table nor Qt's: a slip, or a name the app declares".to_owned(),
        ),
        None => Verdict::Missing(format!(
            "{} (this build has no Qt table to confirm it)",
            api::NOT_IN_TABLE
        )),
    }
}

fn name(key: &Key) -> String {
    match &key.2 {
        Some(member) => format!("{}.{member}", key.1),
        None => key.1.clone(),
    }
}

fn kind_name(kind: MemberKind) -> &'static str {
    match kind {
        MemberKind::Type => "type",
        MemberKind::Property => "property",
        MemberKind::Signal => "signal",
        MemberKind::Method => "method",
        MemberKind::Enum => "enum",
    }
}

fn reason(verdict: &Verdict) -> &str {
    match verdict {
        Verdict::Have => "",
        Verdict::Missing(why) | Verdict::Unknown(why) => why,
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(text: &str) -> Ledger {
        Ledger::parse(text).unwrap()
    }

    #[test]
    fn no_ledger_type_and_member_ljos_has_are_have() {
        assert_eq!(verdict(None, "QtQuick", "Rectangle", None), Verdict::Have);
        assert_eq!(
            verdict(None, "QtQuick", "Rectangle", Some("color")),
            Verdict::Have
        );
    }

    /// Only with Qt's table can a type Ljós lacks be had at all; the name
    /// is found from the tables rather than fixed, as in `api`'s own tests,
    /// so implementing the type does not break the test.
    #[test]
    fn no_ledger_type_only_qt_has_is_missing() {
        if !qt_types::test_has_qt_table("no_ledger_type_only_qt_has_is_missing") {
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
            .find(|(m, t)| {
                api::derive(m, t, None).is_some_and(|r| r.status == ApiStatus::Unsupported)
            });
        let Some((module, name)) = missing else {
            return; // Ljós has everything of Qt's: nothing to find.
        };
        assert_eq!(
            verdict(None, module, name, None),
            Verdict::Missing(api::NOT_IN_TABLE.to_owned()),
            "{module}.{name}"
        );
    }

    /// A name no table has: with Qt's table it is a slip or the app's own;
    /// without one it is counted missing with the table's absence said.
    #[test]
    fn no_ledger_name_neither_table_has_follows_the_qt_table() {
        for v in [
            verdict(None, "QtQuick", "ZzNoSuchType", None),
            verdict(None, "QtQuick", "Rectangle", Some("zzNoSuchMember")),
        ] {
            if api::has_qt_table() {
                assert_eq!(
                    v,
                    Verdict::Unknown(
                        "in neither Ljós's table nor Qt's: a slip, or a name the app declares"
                            .to_owned()
                    )
                );
            } else {
                assert_eq!(
                    v,
                    Verdict::Missing(format!(
                        "{} (this build has no Qt table to confirm it)",
                        api::NOT_IN_TABLE
                    ))
                );
            }
        }
    }

    #[test]
    fn ledger_supported_and_partial_are_have() {
        let l = ledger(
            "[[row]]\nmodule='QtQuick'\ntype='Wobble'\nkind='type'\nstatus='supported'\n\
             [[row]]\nmodule='QtQuick'\ntype='Rectangle'\nmember='color'\nkind='property'\n\
             status='partial'\n",
        );
        // The tables have no `Wobble` at all: the row alone makes it have.
        assert_eq!(verdict(Some(&l), "QtQuick", "Wobble", None), Verdict::Have);
        assert_eq!(
            verdict(Some(&l), "QtQuick", "Rectangle", Some("color")),
            Verdict::Have
        );
    }

    #[test]
    fn ledger_dropped_and_unsupported_are_missing_with_the_reason() {
        let l = ledger(
            "[[row]]\nmodule='QtQuick'\ntype='Rectangle'\nmember='color'\nkind='property'\n\
             status='dropped'\nreason='use the palette'\nscenarios=['@api-rect-color']\n\
             [[row]]\nmodule='QtQuick.Controls'\ntype='Calendar'\nkind='type'\n\
             status='unsupported'\nreason='no calendar yet'\n",
        );
        assert_eq!(
            verdict(Some(&l), "QtQuick", "Rectangle", Some("color")),
            Verdict::Missing("use the palette".to_owned())
        );
        assert_eq!(
            verdict(Some(&l), "QtQuick.Controls", "Calendar", None),
            Verdict::Missing("no calendar yet".to_owned())
        );
    }

    /// A ledger row is exact (`api::status` asks `Ledger::row`): a type
    /// row says nothing of the type's members and a member row nothing of
    /// the type or its other members; where there is no row the tables
    /// answer. So a dropped member leaves its type `Have`, and a dropped
    /// type leaves a member Ljós has `Have` unless the member has a row.
    #[test]
    fn ledger_rows_are_exact_neither_inherits() {
        let member_only = ledger(
            "[[row]]\nmodule='QtQuick'\ntype='Rectangle'\nmember='color'\nkind='property'\n\
             status='dropped'\nreason='use the palette'\n",
        );
        assert_eq!(
            verdict(Some(&member_only), "QtQuick", "Rectangle", Some("color")),
            Verdict::Missing("use the palette".to_owned())
        );
        assert_eq!(
            verdict(Some(&member_only), "QtQuick", "Rectangle", None),
            Verdict::Have
        );
        assert_eq!(
            verdict(Some(&member_only), "QtQuick", "Rectangle", Some("radius")),
            Verdict::Have
        );

        let type_and_member = ledger(
            "[[row]]\nmodule='QtQuick'\ntype='Rectangle'\nkind='type'\nstatus='dropped'\n\
             reason='the type goes'\n\
             [[row]]\nmodule='QtQuick'\ntype='Rectangle'\nmember='color'\nkind='property'\n\
             status='supported'\n",
        );
        assert_eq!(
            verdict(Some(&type_and_member), "QtQuick", "Rectangle", None),
            Verdict::Missing("the type goes".to_owned())
        );
        // The member's own row answers a use of the member, not the type row.
        assert_eq!(
            verdict(
                Some(&type_and_member),
                "QtQuick",
                "Rectangle",
                Some("color")
            ),
            Verdict::Have
        );
        // No row for `radius`: the tables answer, the type row does not taint it.
        assert_eq!(
            verdict(
                Some(&type_and_member),
                "QtQuick",
                "Rectangle",
                Some("radius")
            ),
            Verdict::Have
        );
    }

    fn item(kind: MemberKind, uses: usize, files: &[&str], verdict: Verdict) -> Item {
        Item {
            kind,
            uses,
            files: files.iter().map(|f| (*f).to_owned()).collect(),
            verdict,
        }
    }

    fn sample_report() -> Report {
        let key = |m: &str, t: &str, member: Option<&str>| -> Key {
            (m.to_owned(), t.to_owned(), member.map(str::to_owned))
        };
        Report {
            files: 3,
            unparsed: vec!["Bad.qml".to_owned()],
            items: [
                (
                    key("QtQuick", "Rectangle", None),
                    item(MemberKind::Type, 2, &["a.qml"], Verdict::Have),
                ),
                (
                    key("QtQuick", "Text", Some("renderType")),
                    item(
                        MemberKind::Property,
                        3,
                        &["a.qml", "b.qml"],
                        Verdict::Missing("not in Ljós's type table".to_owned()),
                    ),
                ),
                (
                    key("QtQuick.Controls", "Calendar", None),
                    item(
                        MemberKind::Type,
                        5,
                        &["b.qml"],
                        Verdict::Missing("no calendar".to_owned()),
                    ),
                ),
                (
                    key("QtQuick.Controls", "MonthGrid", None),
                    item(
                        MemberKind::Type,
                        4,
                        &["b.qml"],
                        Verdict::Missing("no month grid".to_owned()),
                    ),
                ),
                (
                    key("QtQuick.Controls", "DayOfWeekRow", None),
                    item(
                        MemberKind::Type,
                        1,
                        &["c.qml"],
                        Verdict::Missing("no day row".to_owned()),
                    ),
                ),
                (
                    key("QtQuick.Controls", "Button", Some("text")),
                    item(
                        MemberKind::Property,
                        1,
                        &["c.qml"],
                        Verdict::Unknown("unplaced slip".to_owned()),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn text_counts_module_rows_and_order() {
        let report = sample_report();
        let text = report.text(None);
        assert!(
            text.starts_with("3 files, 16 uses of 4 types and 2 members\n"),
            "{text}"
        );
        assert!(
            text.contains("1 files did not parse, so what they use is not counted:\n  Bad.qml\n"),
            "{text}"
        );
        assert!(
            text.contains("Ljós has 1, lacks 4, cannot place 1\n"),
            "{text}"
        );
        // Per-module rows: the module with the most missing uses is first.
        let rows = report.modules();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].module, "QtQuick.Controls");
        assert_eq!(
            (
                rows[0].have,
                rows[0].missing,
                rows[0].unknown,
                rows[0].missing_uses
            ),
            (0, 3, 1, 10)
        );
        assert_eq!(rows[1].module, "QtQuick");
        assert_eq!(
            (
                rows[1].have,
                rows[1].missing,
                rows[1].unknown,
                rows[1].missing_uses
            ),
            (1, 1, 0, 3)
        );
        // Detail: most used first, with its reason and file count.
        let cal = text.find("Calendar").unwrap();
        let month = text.find("MonthGrid").unwrap();
        let day = text.find("DayOfWeekRow").unwrap();
        assert!(cal < month && month < day, "{text}");
        assert!(text.contains("5 uses in 1 file  no calendar"), "{text}");
        assert!(
            text.contains("Text.renderType") && text.contains("3 uses in 2 files"),
            "{text}"
        );
        assert!(
            text.contains("\nunknown\n") && text.contains("Button.text"),
            "{text}"
        );
        // What Ljós has is counted, never listed.
        assert!(!text.contains("Rectangle"), "{text}");
        assert!(!text.contains("... and"), "{text}");
    }

    #[test]
    fn text_limit_truncates_per_module() {
        let report = sample_report();
        let one = report.text(Some(1));
        assert!(one.contains("Calendar"), "{one}");
        assert!(!one.contains("MonthGrid"), "{one}");
        assert!(one.contains("  ... and 2 more\n"), "{one}");
        // The one missing item of the other module still shows.
        assert!(one.contains("Text.renderType"), "{one}");

        let two = report.text(Some(2));
        assert!(
            two.contains("MonthGrid") && !two.contains("DayOfWeekRow"),
            "{two}"
        );
        assert!(two.contains("  ... and 1 more\n"), "{two}");
    }

    #[test]
    fn json_shape() {
        let v = sample_report().to_json();
        assert_eq!(v["files"], json!(3));
        assert_eq!(v["unparsed"], json!(["Bad.qml"]));
        assert_eq!(v["supported"], json!(1));
        assert_eq!(v["missing"], json!(4));
        assert_eq!(v["unknown"], json!(1));
        assert_eq!(v["uses"], json!(16));
        let modules = v["modules"].as_array().unwrap();
        assert_eq!(modules.len(), 2);
        let controls = &modules[0];
        assert_eq!(controls["module"], json!("QtQuick.Controls"));
        assert_eq!(controls["supported"], json!(0));
        assert_eq!(controls["missing"], json!(3));
        assert_eq!(controls["unknown"], json!(1));
        assert_eq!(controls["missing_uses"], json!(10));
        let missing = controls["missing_items"].as_array().unwrap();
        assert_eq!(missing.len(), 3);
        assert_eq!(
            missing[0],
            json!({
                "type": "Calendar",
                "member": null,
                "kind": "type",
                "uses": 5,
                "files": 1,
                "in": ["b.qml"],
                "reason": "no calendar",
            })
        );
        let unknown = controls["unknown_items"].as_array().unwrap();
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0]["type"], json!("Button"));
        assert_eq!(unknown[0]["member"], json!("text"));
        assert_eq!(unknown[0]["kind"], json!("property"));
        assert_eq!(unknown[0]["reason"], json!("unplaced slip"));

        let quick = &modules[1];
        assert_eq!(quick["module"], json!("QtQuick"));
        let qmissing = quick["missing_items"].as_array().unwrap();
        assert_eq!(qmissing.len(), 1);
        assert_eq!(qmissing[0]["type"], json!("Text"));
        assert_eq!(qmissing[0]["member"], json!("renderType"));
        assert_eq!(qmissing[0]["files"], json!(2));
        assert_eq!(qmissing[0]["in"], json!(["a.qml", "b.qml"]));
        // Supported items appear in the counts only, in no list.
        assert!(!v.to_string().contains("Rectangle"), "{v}");
    }

    #[test]
    fn empty_report() {
        let report = Report::default();
        let text = report.text(None);
        assert!(
            text.starts_with(
                "0 files, 0 uses of 0 types and 0 members\nLjós has 0, lacks 0, cannot place 0\n"
            ),
            "{text}"
        );
        let v = report.to_json();
        assert_eq!(v["files"], json!(0));
        assert_eq!(v["modules"], json!([]));
        assert_eq!(v["uses"], json!(0));
    }
}
