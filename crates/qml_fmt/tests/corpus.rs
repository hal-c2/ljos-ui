//! The proof the formatter is held to: every `.qml` file of the repository
//! that parses is the same document after formatting, and formatting it
//! again changes nothing. Nothing is written: the files are only read.

use std::path::{Path, PathBuf};

fn qml_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() && entry.file_name() != "target" => qml_below(&path, out),
            Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "qml") => out.push(path),
            _ => {}
        }
    }
}

/// What is left of a text when everything the formatter may add or drop is
/// taken out: white space, the `;` after a member and the `,` of a list of
/// objects. A check that does not go through the parser.
fn ink(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != ';' && *c != ',' && *c != '\u{feff}')
        .collect()
}

struct Tally {
    files: usize,
    formatted: usize,
    refused: usize,
}

fn prove(dir: &str) -> Tally {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(dir);
    let mut files = Vec::new();
    qml_below(&root, &mut files);
    files.sort();
    let mut tally = Tally {
        files: files.len(),
        formatted: 0,
        refused: 0,
    };
    let mut faults = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let formatted = match qml_fmt::format(&source) {
            Ok(text) => text,
            Err(qml_fmt::Error::Syntax(_)) => {
                tally.refused += 1;
                continue;
            }
            Err(e) => {
                faults.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        tally.formatted += 1;
        if let Err(e) = qml_fmt::same_tree(&source, &formatted) {
            faults.push(format!("{}: not the same tree: {e}", path.display()));
        }
        if ink(&source) != ink(&formatted) {
            faults.push(format!("{}: a character came or went", path.display()));
        }
        match qml_fmt::format(&formatted) {
            Ok(again) if again == formatted => {}
            Ok(_) => faults.push(format!("{}: not idempotent", path.display())),
            Err(e) => faults.push(format!("{}: second pass: {e}", path.display())),
        }
        if !formatted.ends_with('\n') || formatted.ends_with("\n\n") || formatted.contains('\r') {
            faults.push(format!("{}: line ends", path.display()));
        }
    }
    assert!(
        faults.is_empty(),
        "{} fault(s):\n{}",
        faults.len(),
        faults.join("\n")
    );
    tally
}

#[test]
fn every_qml_file_of_the_crates_survives_formatting() {
    let tally = prove("crates");
    assert!(
        tally.formatted >= 40,
        "only {} of {} files",
        tally.formatted,
        tally.files
    );
    // Fixtures of other tests are broken on purpose; those are refused.
    assert!(
        tally.refused * 10 <= tally.files,
        "{} of {} refused",
        tally.refused,
        tally.files
    );
}

#[test]
fn every_qml_file_of_the_qtdoc_corpus_survives_formatting() {
    // A submodule that was not fetched leaves its directory behind, empty.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples");
    if !root.is_dir() {
        // The corpus is fetched, not checked in: without it there is
        // nothing to prove here.
        eprintln!("corpus/qtdoc is not there; skipped");
        return;
    }
    let tally = prove("corpus/qtdoc");
    assert!(
        tally.formatted >= 400,
        "only {} of {} files",
        tally.formatted,
        tally.files
    );
    // Qt's own examples are valid QML; a handful may use syntax the parser
    // does not have yet, and those are refused, not mangled.
    assert!(
        tally.refused * 20 <= tally.files,
        "{} of {} refused",
        tally.refused,
        tally.files
    );
}
