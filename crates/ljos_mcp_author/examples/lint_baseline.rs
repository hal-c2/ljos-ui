//! Counts the design lints `qml_lint` reports over trees of QML, per rule:
//! the baseline `docs/design/lint-baseline.md` records, so a change to a
//! check shows as a change in its count.
//!
//! ```text
//! cargo run -q -p ljos_mcp_author --example lint_baseline -- <dir>… [--list <code>]
//! ```
//!
//! Each file is linted with its own directory as the project, as an app's
//! QML module would be. `--list <code>` prints every finding of one code,
//! to read for false positives.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use ljos_mcp_author::{FrontEnd, QmlFrontEnd, lint};

fn qml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            qml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "qml") {
            out.push(path);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dirs = Vec::new();
    let mut list = None;
    while let Some(arg) = args.next() {
        if arg == "--list" {
            list = args.next();
        } else {
            dirs.push(PathBuf::from(arg));
        }
    }
    for dir in dirs {
        let mut files = Vec::new();
        qml_files(&dir, &mut files);
        let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut unreadable = 0;
        for file in &files {
            let front = QmlFrontEnd::new(file.parent().expect("a file is in a directory"));
            let Ok(found) = front.lint(std::slice::from_ref(file)) else {
                unreadable += 1;
                continue;
            };
            let mut codes: Vec<&str> = Vec::new();
            for d in &found {
                if lint(&d.code).is_none_or(|l| l.design.is_none()) {
                    continue;
                }
                if list.as_deref() == Some(d.code.as_str()) {
                    println!("{}:{}:{}: {}", file.display(), d.line, d.column, d.message);
                }
                let entry = counts.entry(d.code.clone()).or_default();
                entry.0 += 1;
                if !codes.contains(&d.code.as_str()) {
                    codes.push(&d.code);
                    entry.1 += 1;
                }
            }
        }
        if list.is_some() {
            continue;
        }
        println!(
            "{}: {} files, {unreadable} not analysed",
            dir.display(),
            files.len()
        );
        println!();
        println!("| code | severity | findings | files |");
        println!("|---|---|---:|---:|");
        for (code, (n, in_files)) in &counts {
            let severity = lint(code).map_or(String::new(), |l| {
                format!("{:?}", l.severity).to_lowercase()
            });
            println!("| `{code}` | {severity} | {n} | {in_files} |");
        }
        println!();
    }
}
