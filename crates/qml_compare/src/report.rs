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

/// What to show where a score would be: `-` when nothing rendered, `no ref`
/// when there is no reference picture, `size` when the sizes differ.
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
