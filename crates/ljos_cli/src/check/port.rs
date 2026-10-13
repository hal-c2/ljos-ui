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
//! every use of it. The tables of types are the default source of truth.

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
