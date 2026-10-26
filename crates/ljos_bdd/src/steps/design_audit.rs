//! Design audit steps that name a finding by the line of the QML it is
//! about, for what has no accessibility node to name it by (an item left
//! out of the tree, a text the audit reads as a label).

use cucumber::then;
use serde_json::Value;

use crate::backend::BackendError;
use crate::world::LjosWorld;

use super::StepResult;

fn findings(report: &Value) -> impl Iterator<Item = &Value> {
    report["findings"].as_array().into_iter().flatten()
}

/// The line of a `source` like `file:///Main.qml:8:5`.
fn line_of(source: &str) -> Option<u32> {
    source.rsplit(':').nth(1)?.parse().ok()
}

fn listed(report: &Value) -> String {
    let lines: Vec<String> = findings(report)
        .map(|f| {
            format!(
                "{} [{}] {}",
                f["source"].as_str().unwrap_or(""),
                f["code"].as_str().unwrap_or(""),
                f["message"].as_str().unwrap_or("")
            )
        })
        .collect();
    if lines.is_empty() {
        "nothing".to_owned()
    } else {
        lines.join("\n  ")
    }
}

#[then(regex = r#"^the design audit reports "([a-z0-9-]+)" at line ([0-9]+)$"#)]
fn design_audit_reports_at(w: &mut LjosWorld, code: String, line: u32) -> StepResult {
    w.each(|b| {
        let report = b.probes()?.design_audit(None)?;
        let hit = findings(&report).any(|f| {
            f["code"] == code.as_str() && f["source"].as_str().and_then(line_of) == Some(line)
        });
        if hit {
            return Ok(());
        }
        Err(BackendError::Failed(format!(
            "no {code:?} at line {line}; the design audit found:\n  {}",
            listed(&report)
        )))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_source_has_its_line() {
        assert_eq!(super::line_of("file:///Main.qml:8:5"), Some(8));
        assert_eq!(super::line_of("nowhere"), None);
    }
}
