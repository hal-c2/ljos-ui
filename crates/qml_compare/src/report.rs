//! Report rows (the schema shared with the web gallery), the terminal table
//! and the ratchet.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::manifest::{ManifestError, Window, read_json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub text: String,
    pub times: u32,
}

/// One example's result. Field names are the gallery's, so one report viewer
/// serves every renderer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub entry: String,
    pub window: Window,
    pub files: u32,
    pub compiled: u32,
    pub entry_compiles: bool,
    pub renders: bool,
    pub error: Option<String>,
    pub stack: Option<String>,
    pub reference: bool,
    pub qt: Option<String>,
    pub pixels: Option<f64>,
    pub content: Option<f64>,
    pub mismatch: Option<String>,
    pub lacks: Vec<String>,
    pub compile_errors: BTreeMap<String, String>,
    pub said: Vec<Said>,
}

fn percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

/// What to show where a score would be: `no ref` when there is no reference
/// picture, `size` when the sizes differ, `-` when no score was made.
fn score(row: &Row, value: Option<f64>) -> String {
    if !row.reference {
        "no ref".into()
    } else if row.mismatch.is_some() {
        "size".into()
    } else {
        value.map_or_else(|| "-".into(), percent)
    }
}

fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

pub fn table(rows: &[Row], width: usize) -> String {
    let sum = |f: &dyn Fn(&Row) -> u32| rows.iter().map(f).sum::<u32>();
    let mut lines: Vec<[String; 7]> = vec![
        [
            "example",
            "files",
            "entry",
            "renders",
            "pixels",
            "content",
            "what stops it",
        ]
        .map(String::from),
    ];
    for row in rows {
        lines.push([
            row.id.clone(),
            format!("{}/{}", row.compiled, row.files),
            yes(row.entry_compiles).into(),
            yes(row.renders).into(),
            score(row, row.pixels),
            score(row, row.content),
            row.error
                .as_deref()
                .or(row.mismatch.as_deref())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .into(),
        ]);
    }
    lines.push([
        "total".into(),
        format!("{}/{}", sum(&|r| r.compiled), sum(&|r| r.files)),
        format!("{}/{}", sum(&|r| u32::from(r.entry_compiles)), rows.len()),
        format!("{}/{}", sum(&|r| u32::from(r.renders)), rows.len()),
        String::new(),
        String::new(),
        format!(
            "{} with a reference picture",
            sum(&|r| u32::from(r.reference))
        ),
    ]);
    let widths: Vec<usize> = (0..7)
        .map(|c| {
            lines
                .iter()
                .map(|l| l[c].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    let last = lines.len() - 1;
    for (index, line) in lines.iter().enumerate() {
        if index == last {
            out.push('\n');
        }
        let cells: Vec<String> = line
            .iter()
            .enumerate()
            .map(|(c, cell)| {
                if c == 0 || c == 6 {
                    format!("{cell:<w$}", w = widths[c])
                } else {
                    format!("{cell:>w$}", w = widths[c])
                }
            })
            .collect();
        let text = cells.join("  ");
        let text = text.trim_end();
        if text.chars().count() > width {
            out.extend(text.chars().take(width - 1));
            out.push('…');
        } else {
            out.push_str(text);
        }
        out.push('\n');
        if index == 0 {
            out.push('\n');
        }
    }
    out
}

fn escape(text: &str) -> String {
    text.chars().fold(String::new(), |mut s, c| {
        match c {
            '&' | '<' | '>' | '"' => {
                let _ = write!(s, "&#{};", c as u32);
            }
            _ => s.push(c),
        }
        s
    })
}

/// A page with the pictures side by side, for looking at.
pub fn html(rows: &[Row], dir: &Path) -> String {
    let picture = |row: &Row, kind: &str| {
        let name = format!("{}.{kind}.png", escape(&row.id));
        if dir.join(format!("{}.{kind}.png", row.id)).is_file() {
            format!("<a href=\"{name}\"><img src=\"{name}\"></a>")
        } else {
            String::new()
        }
    };
    let mut body = String::new();
    for row in rows {
        let reference = picture(row, "reference");
        let reference = if reference.is_empty() {
            format!(
                "<i>{}</i>",
                escape(row.qt.as_deref().unwrap_or("no reference picture"))
            )
        } else {
            reference
        };
        let said: Vec<String> = row
            .said
            .iter()
            .map(|s| {
                let first = s.text.lines().next().unwrap_or("");
                if s.times > 1 {
                    format!("{}x {first}", s.times)
                } else {
                    first.into()
                }
            })
            .collect();
        let _ = write!(
            body,
            "<tr>\n  <td><b>{}</b><br>{}<br>{}x{}</td>\n  <td>{}/{}<br>entry: {}<br>renders: {}<br>pixels: {}<br>content: {}</td>\n  <td>{}</td>\n  <td>{}</td>\n  <td>{}</td>\n  <td><pre>{}</pre></td>\n  <td><pre>{}</pre></td>\n</tr>\n",
            escape(&row.id),
            escape(&row.entry),
            row.window.width,
            row.window.height,
            row.compiled,
            row.files,
            yes(row.entry_compiles),
            yes(row.renders),
            score(row, row.pixels),
            score(row, row.content),
            picture(row, "actual"),
            reference,
            picture(row, "diff"),
            escape(
                row.error
                    .as_deref()
                    .or(row.mismatch.as_deref())
                    .unwrap_or("")
            ),
            escape(&said.join("\n")),
        );
    }
    format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>qml-compare</title>\n<style>\n  body {{ font: 13px/1.4 system-ui, sans-serif; margin: 16px; }}\n  table {{ border-collapse: collapse; }}\n  th, td {{ text-align: left; vertical-align: top; padding: 8px; border-bottom: 1px solid #ddd; }}\n  img {{ max-width: 300px; max-height: 300px; border: 1px solid #ccc; }}\n  pre {{ white-space: pre-wrap; max-width: 420px; margin: 0; font-size: 11px; }}\n  i {{ color: #777; display: block; max-width: 300px; }}\n</style>\n<table>\n<tr><th>example</th><th>report</th><th>actual</th><th>reference</th><th>difference</th><th>what stops it</th><th>what it warns of</th></tr>\n{body}</table>\n"
    )
}

/// Least scores an example may fall to; an empty floor only requires that it
/// keeps rendering.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Floor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixels: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<f64>,
}

/// `expected.json`. Unknown top-level fields (such as `about`) survive a
/// rewrite.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ratchet {
    #[serde(default)]
    pub examples: BTreeMap<String, Floor>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Ratchet {
    pub fn read(path: &Path) -> Result<Self, ManifestError> {
        if path.exists() {
            read_json(path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        text.push('\n');
        std::fs::write(path, text)
    }

    /// One line per broken promise. A listed example must keep rendering and
    /// keep its scores.
    pub fn check(&self, rows: &[Row]) -> Vec<String> {
        let mut failures = Vec::new();
        for row in rows {
            // Needing something this machine lacks is told of, not held to.
            if row
                .error
                .as_deref()
                .is_some_and(|e| e.contains("is not installed here"))
            {
                continue;
            }
            let Some(floor) = self.examples.get(&row.id) else {
                continue;
            };
            if !row.renders {
                failures.push(format!(
                    "{} must keep rendering: {}",
                    row.id,
                    row.error.as_deref().unwrap_or("it does not")
                ));
                continue;
            }
            for (name, floor, got) in [
                ("pixels", floor.pixels, row.pixels),
                ("content", floor.content, row.content),
            ] {
                let Some(floor) = floor else { continue };
                match got {
                    Some(got) if got >= floor => {}
                    Some(got) => failures.push(format!(
                        "{} must stay as like its reference picture: {name} {} is under {}",
                        row.id,
                        percent(got),
                        percent(floor)
                    )),
                    None => failures.push(format!(
                        "{} has no {name} score to hold to {}",
                        row.id,
                        percent(floor)
                    )),
                }
            }
        }
        failures
    }

    /// Add every example that renders now and is not listed; returns them.
    pub fn extend(&mut self, rows: &[Row]) -> Vec<String> {
        let added: Vec<String> = rows
            .iter()
            .filter(|r| r.renders && !self.examples.contains_key(&r.id))
            .map(|r| r.id.clone())
            .collect();
        for id in &added {
            self.examples.insert(id.clone(), Floor::default());
        }
        added
    }
}

/// A floor is the score cut down to a hundredth, so that a platform that
/// anti-aliases a pixel differently does not break the build.
fn floor_of(score: f64) -> f64 {
    (score * 100.0).floor() / 100.0
}

impl Ratchet {
    /// Add every example that renders and is not listed, and lift each floor
    /// to what the example scores now. A floor is never lowered. Returns one
    /// line per change.
    pub fn raise(&mut self, rows: &[Row]) -> Vec<String> {
        let mut changes = Vec::new();
        for row in rows.iter().filter(|r| r.renders) {
            let listed = self.examples.contains_key(&row.id);
            let floor = self.examples.entry(row.id.clone()).or_default();
            let mut said = Vec::new();
            for (name, floor, got) in [
                ("pixels", &mut floor.pixels, row.pixels),
                ("content", &mut floor.content, row.content),
            ] {
                let Some(got) = got.map(floor_of) else {
                    continue;
                };
                if floor.is_none_or(|f| got > f) {
                    *floor = Some(got);
                    said.push(format!("{name} {}", percent(got)));
                }
            }
            if !listed {
                changes.push(format!("{}: added", row.id));
            } else if !said.is_empty() {
                changes.push(format!("{}: {}", row.id, said.join(", ")));
            }
            if !listed
                && !said.is_empty()
                && let Some(last) = changes.last_mut()
            {
                last.push_str(&format!(" ({})", said.join(", ")));
            }
        }
        changes
    }
}

/// The first thing that stopped an example, without where in which file:
/// `Main.qml:3:5: error: x` and `Other.qml:9:1: error: x` are one kind.
pub fn error_kind(error: &str) -> String {
    let line = error.lines().next().unwrap_or("").trim();
    let mut parts = line.splitn(4, ':');
    let (file, row, column, rest) = (parts.next(), parts.next(), parts.next(), parts.next());
    let numeric =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let rest = match (file, rest) {
        (Some(f), Some(rest)) if f.ends_with(".qml") && numeric(row) && numeric(column) => rest,
        _ => line,
    };
    rest.trim().trim_start_matches("error:").trim().to_owned()
}

const BUCKETS: [(&str, f64, f64); 7] = [
    ("0%", 0.0, 0.0),
    ("under 25%", 0.0, 0.25),
    ("25% to under 50%", 0.25, 0.5),
    ("50% to under 75%", 0.5, 0.75),
    ("75% to under 90%", 0.75, 0.9),
    ("90% to under 100%", 0.9, 1.0),
    ("100%", 1.0, 1.0),
];

/// Where `score` falls in [`BUCKETS`].
fn bucket(score: f64) -> usize {
    if score <= 0.0 {
        0
    } else if score >= 1.0 {
        BUCKETS.len() - 1
    } else {
        BUCKETS[1..BUCKETS.len() - 1]
            .iter()
            .position(|(_, low, high)| score >= *low && score < *high)
            .map_or(1, |i| i + 1)
    }
}

/// How many examples load and how well the ones with a reference picture
/// score; what stops the rest, most common first.
pub fn summary(rows: &[Row], top: usize) -> String {
    let count = |f: &dyn Fn(&Row) -> bool| rows.iter().filter(|r| f(r)).count();
    let scored: Vec<&Row> = rows
        .iter()
        .filter(|r| r.reference && r.mismatch.is_none())
        .collect();
    let mut out = format!(
        "{} examples: {} compile their entry file, {} render, {} have a reference picture, {} are scored.\n",
        rows.len(),
        count(&|r| r.entry_compiles),
        count(&|r| r.renders),
        count(&|r| r.reference),
        scored.len(),
    );
    // A picture that never came out scores nothing: 0, with its error beside.
    let value =
        |r: &Row, pick: fn(&Row) -> Option<f64>| if r.renders { pick(r) } else { Some(0.0) };
    for (name, pick) in [
        ("pixels", (|r: &Row| r.pixels) as fn(&Row) -> Option<f64>),
        ("content", |r: &Row| r.content),
    ] {
        let mut counts = [0usize; BUCKETS.len()];
        let mut sum = 0.0;
        for row in &scored {
            if let Some(v) = value(row, pick) {
                counts[bucket(v)] += 1;
                sum += v;
            }
        }
        let n: usize = counts.iter().sum();
        let _ = writeln!(
            out,
            "\n{name} over {n} scored examples (mean {}):",
            if n == 0 {
                "-".into()
            } else {
                percent(sum / n as f64)
            }
        );
        for ((label, _, _), c) in BUCKETS.iter().zip(counts) {
            let _ = writeln!(out, "  {label:<20} {c:>4}");
        }
    }
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    for row in rows {
        if let Some(error) = row.error.as_deref() {
            *kinds.entry(error_kind(error)).or_default() += 1;
        }
    }
    let mut kinds: Vec<(String, u32)> = kinds.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if !kinds.is_empty() {
        let _ = writeln!(
            out,
            "\nwhat stops examples first (top {top} of {}):",
            kinds.len()
        );
        for (kind, n) in kinds.iter().take(top) {
            let _ = writeln!(out, "  {n:>4}  {kind}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, renders: bool, pixels: Option<f64>, error: Option<&str>) -> Row {
        Row {
            id: id.into(),
            entry: "Main.qml".into(),
            window: Window::default(),
            files: 1,
            compiled: u32::from(renders),
            entry_compiles: renders,
            renders,
            error: error.map(str::to_owned),
            stack: None,
            reference: true,
            qt: None,
            pixels,
            content: pixels,
            mismatch: None,
            lacks: Vec::new(),
            compile_errors: BTreeMap::new(),
            said: Vec::new(),
        }
    }

    #[test]
    fn raise_adds_and_lifts_but_never_lowers() {
        let mut ratchet = Ratchet::default();
        ratchet.examples.insert(
            "a".into(),
            Floor {
                pixels: Some(0.9),
                content: Some(0.5),
            },
        );
        ratchet.examples.insert(
            "b".into(),
            Floor {
                pixels: Some(0.9),
                content: None,
            },
        );
        let rows = [
            row("a", true, Some(0.9567), None),
            row("b", true, Some(0.7), None),
            row("c", true, Some(1.0), None),
            row("d", false, None, Some("nope")),
        ];
        let changes = ratchet.raise(&rows);
        assert_eq!(ratchet.examples["a"].pixels, Some(0.95));
        assert_eq!(ratchet.examples["a"].content, Some(0.95));
        // Under its floor: kept, and the check says so.
        assert_eq!(ratchet.examples["b"].pixels, Some(0.9));
        assert_eq!(ratchet.examples["b"].content, Some(0.7));
        assert_eq!(ratchet.examples["c"].pixels, Some(1.0));
        assert!(!ratchet.examples.contains_key("d"));
        assert_eq!(changes.len(), 3, "{changes:?}");
        assert!(changes.iter().any(|c| c.starts_with("c: added")));
        assert!(
            ratchet
                .check(&rows[..2])
                .iter()
                .any(|f| f.starts_with("b must"))
        );
    }

    #[test]
    fn errors_of_one_kind_are_counted_together() {
        assert_eq!(
            error_kind("Main.qml:3:5: error: unknown type X"),
            "unknown type X"
        );
        assert_eq!(
            error_kind("a/B.qml:30:1: unknown type X\nmore"),
            "unknown type X"
        );
        assert_eq!(
            error_kind("type \"ListView\" is not in module \"QtQuick\""),
            "type \"ListView\" is not in module \"QtQuick\""
        );
        assert_eq!(error_kind("cannot read: file"), "cannot read: file");
    }

    #[test]
    fn the_summary_counts_loads_scores_and_errors() {
        let rows = [
            row("a", true, Some(1.0), None),
            row("b", true, Some(0.6), None),
            row(
                "c",
                false,
                None,
                Some("Main.qml:1:1: error: unknown type X"),
            ),
            row(
                "d",
                false,
                None,
                Some("Other.qml:7:2: error: unknown type X"),
            ),
            row("e", false, None, Some("boom")),
        ];
        let text = summary(&rows, 10);
        assert!(text.starts_with("5 examples: 2 compile their entry file, 2 render, 5 have a reference picture, 5 are scored."), "{text}");
        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
        // Examples that do not render score 0.
        assert!(words.contains("0% 3 under 25% 0"), "{text}");
        assert!(words.contains("50% to under 75% 1"), "{text}");
        assert!(words.contains("90% to under 100% 0 100% 1"), "{text}");
        assert!(words.contains("2 unknown type X 1 boom"), "{text}");
    }
}
