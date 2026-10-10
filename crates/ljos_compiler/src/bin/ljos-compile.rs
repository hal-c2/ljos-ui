//! `ljos-compile baseline [dir]`: how much of a tree of QML projects
//! compiles. `ljos-compile dump <dir> <file>`: one file's IR and module.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use ljos_compiler::{Project, baseline, compile_file, dump};
use qml_front::QtTypeOracle;

const USAGE: &str =
    "usage: ljos-compile baseline [dir] | ljos-compile dump <project dir> <file.qml>";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["baseline"] => run_baseline(&default_corpus()),
        ["baseline", dir] => run_baseline(Path::new(dir)),
        ["dump", dir, file] => run_dump(Path::new(dir), Path::new(file)),
        _ => bail!(USAGE),
    }
}

fn default_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples")
}

fn run_baseline(root: &Path) -> anyhow::Result<()> {
    if !root.is_dir() {
        bail!(
            "{} is not there: `git submodule update --init corpus/qtdoc`",
            root.display()
        );
    }
    let measured = baseline::measure(root, || QtTypeOracle);
    print!("{measured}");
    if std::env::var_os("LJOS_COMPILE_ERRORS").is_some() {
        for (file, error) in &measured.errors {
            println!("{file}: {error}");
        }
    }
    Ok(())
}

fn run_dump(dir: &Path, file: &Path) -> anyhow::Result<()> {
    let project = Project::load(dir, QtTypeOracle)?;
    let relative = file.strip_prefix(dir).unwrap_or(file);
    let key = relative.to_string_lossy().replace('\\', "/");
    let key = key.strip_suffix(".qml").unwrap_or(&key);
    match compile_file(&project, key) {
        Ok(compiled) => {
            print!("{}", dump(&compiled.file));
            for diagnostic in &compiled.diagnostics {
                eprintln!("{}:{diagnostic}", file.display());
            }
            Ok(())
        }
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                eprintln!("{}:{diagnostic}", file.display());
            }
            Err(anyhow::anyhow!("{} did not compile", file.display())).context("the parser gave up")
        }
    }
}
