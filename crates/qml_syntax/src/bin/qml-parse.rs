//! Parses every `.qml` file named on the command line, or found under a
//! directory named there, and reports syntax errors and a `passed/total` line.
//! The corpus baseline (`mise run parse`) is this run over `corpus/qtdoc`.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use qml_syntax::Allocator;

fn main() -> ExitCode {
    let mut files = Vec::new();
    for argument in std::env::args_os().skip(1) {
        collect(Path::new(&argument), &mut files);
    }
    files.sort();

    let mut passed = 0;
    for path in &files {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                println!("{}: {error}", path.display());
                continue;
            }
        };
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &source);
        if parsed.is_ok() {
            passed += 1;
            continue;
        }
        for diagnostic in &parsed.diagnostics {
            let (line, column) = diagnostic.position(&source);
            println!("{}:{line}:{column}: {diagnostic}", path.display());
        }
    }
    println!("{passed}/{} parsed", files.len());
    if passed == files.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if !path.is_dir() {
        files.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "qml") {
            files.push(path);
        }
    }
}
