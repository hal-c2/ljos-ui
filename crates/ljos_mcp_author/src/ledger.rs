use std::{collections::BTreeMap, path::Path};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApiStatus {
    Supported,
    Partial,
    Dropped,
    Unsupported,
}

impl ApiStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ApiStatus::Supported => "supported",
            ApiStatus::Partial => "partial",
            ApiStatus::Dropped => "dropped",
            ApiStatus::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MemberKind {
    Type,
    Property,
    Signal,
    Method,
    Enum,
}

/// One ledger row (§18.5): a type, or one member of a type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LedgerRow {
    pub module: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// Absent on the row that describes the type itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    pub kind: MemberKind,
    pub status: ApiStatus,
    /// Required for `dropped` and `unsupported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `@api-…` tags of the scenarios that prove the row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<String>,
}

impl LedgerRow {
    /// `Module.Type` or `Module.Type.member`.
    pub fn key(&self) -> String {
        match &self.member {
            Some(m) => format!("{}.{}.{m}", self.module, self.type_name),
            None => format!("{}.{}", self.module, self.type_name),
        }
    }

    pub fn type_key(&self) -> String {
        format!("{}.{}", self.module, self.type_name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("cannot read ledger `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("ledger is not valid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("ledger row `{key}`: {problem}")]
    Row { key: String, problem: &'static str },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    row: Vec<LedgerRow>,
}

#[derive(Clone, Debug, Default)]
pub struct Ledger {
    rows: Vec<LedgerRow>,
}

impl Ledger {
    pub fn parse(text: &str) -> Result<Self, LedgerError> {
        let file: File = toml::from_str(text)?;
        for row in &file.row {
            let reason_missing = row.reason.as_deref().is_none_or(str::is_empty);
            if matches!(row.status, ApiStatus::Dropped | ApiStatus::Unsupported) && reason_missing {
                return Err(LedgerError::Row {
                    key: row.key(),
                    problem: "dropped and unsupported rows need a reason",
                });
            }
            if row.scenarios.iter().any(|s| !s.starts_with("@api-")) {
                return Err(LedgerError::Row {
                    key: row.key(),
                    problem: "scenario tags must start with `@api-`",
                });
            }
            if (row.member.is_none()) != (row.kind == MemberKind::Type) {
                return Err(LedgerError::Row {
                    key: row.key(),
                    problem: "`member` is set exactly when `kind` is not `type`",
                });
            }
        }
        Ok(Self { rows: file.row })
    }

    pub fn load(path: &Path) -> Result<Self, LedgerError> {
        let text = std::fs::read_to_string(path).map_err(|source| LedgerError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text)
    }

    pub fn rows(&self) -> &[LedgerRow] {
        &self.rows
    }

    /// Rows for `Module.Type[.member]`: the exact member, or, for a bare
    /// `Module.Type`, the type row followed by its members. Modules contain
    /// dots (`QtQuick.Controls`), so the query is matched against whole keys
    /// rather than split.
    pub fn lookup(&self, query: &str) -> Vec<&LedgerRow> {
        let whole_type: Vec<_> = self.rows.iter().filter(|r| r.type_key() == query).collect();
        if !whole_type.is_empty() {
            return whole_type;
        }
        self.rows.iter().filter(|r| r.key() == query).collect()
    }

    /// The one row for the type (`member` None) or for that member, which
    /// [`lookup`](Self::lookup) cannot give: it answers a bare type with its
    /// members too.
    pub fn row(&self, module: &str, type_name: &str, member: Option<&str>) -> Option<&LedgerRow> {
        self.rows.iter().find(|r| {
            r.module == module && r.type_name == type_name && r.member.as_deref() == member
        })
    }

    pub fn modules(&self) -> Vec<&str> {
        let mut m: Vec<&str> = self.rows.iter().map(|r| r.module.as_str()).collect();
        m.sort_unstable();
        m.dedup();
        m
    }

    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for r in &self.rows {
            *out.entry(r.status.as_str()).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[row]]
module = "QtQuick"
type = "Item"
kind = "type"
status = "unsupported"
reason = "M0 in progress"

[[row]]
module = "QtQuick"
type = "Item"
member = "width"
kind = "property"
status = "supported"
scenarios = ["@api-item-width"]
"#;

    #[test]
    fn lookup_exact_member_then_whole_type() {
        let l = Ledger::parse(SAMPLE).unwrap();
        assert_eq!(l.lookup("QtQuick.Item.width").len(), 1);
        assert_eq!(l.lookup("QtQuick.Item").len(), 2);
        assert!(l.lookup("QtQuick.Nope").is_empty());
        assert_eq!(l.counts()["supported"], 1);
    }

    #[test]
    fn rejects_malformed_rows() {
        let no_reason = "[[row]]\nmodule='A'\ntype='B'\nkind='type'\nstatus='dropped'\n";
        assert!(matches!(
            Ledger::parse(no_reason),
            Err(LedgerError::Row { .. })
        ));
        let bad_tag =
            "[[row]]\nmodule='A'\ntype='B'\nkind='type'\nstatus='supported'\nscenarios=['x']\n";
        assert!(matches!(
            Ledger::parse(bad_tag),
            Err(LedgerError::Row { .. })
        ));
        let bad_member = "[[row]]\nmodule='A'\ntype='B'\nkind='property'\nstatus='supported'\n";
        assert!(matches!(
            Ledger::parse(bad_member),
            Err(LedgerError::Row { .. })
        ));
        assert!(matches!(
            Ledger::parse("[[row]]\nbogus=1\n"),
            Err(LedgerError::Parse(_))
        ));
    }
}
