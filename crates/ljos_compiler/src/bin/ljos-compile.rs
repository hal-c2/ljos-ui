//! `ljos-compile baseline [dir]`: how much of a tree of QML projects
//! compiles. `ljos-compile dump <dir> <file>`: one file's IR and module.
//! `ljos-compile aot <dir> <file>`: the Rust a shipped file's bindings and
//! handlers compile to. `ljos-compile aot-coverage [dir]`: how much of a
//! tree of QML projects compiles to Rust, and why the rest does not.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use ljos_compiler::{Project, baseline, compile_file, dump};
use qml_front::QtTypeOracle;

const USAGE: &str = "usage: ljos-compile baseline [dir] | ljos-compile dump <project dir> <file.qml> \
     | ljos-compile aot <project dir> <file.qml> | ljos-compile aot-coverage [dir]";

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
        ["aot", dir, file] => run_aot(Path::new(dir), Path::new(file)),
        ["aot-coverage"] => run_aot_coverage(&default_corpus()),
        ["aot-coverage", dir] => run_aot_coverage(Path::new(dir)),
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

fn run_aot(dir: &Path, file: &Path) -> anyhow::Result<()> {
    let project = Project::load(dir, QtTypeOracle)?;
    let relative = file.strip_prefix(dir).unwrap_or(file);
    let key = relative.to_string_lossy().replace('\\', "/");
    let key = key.strip_suffix(".qml").unwrap_or(&key);
    let compiled = compile_file(&project, key)
        .map_err(|_| anyhow::anyhow!("{} did not compile", file.display()))?;
    let generated = ljos_compiler::aot::generate(&compiled.file, &QtTypeOracle, "::ljos::aot");
    print!("{}", generated.rust);
    for (index, outcome) in generated.outcomes.iter().enumerate() {
        match outcome {
            ljos_aot::Outcome::Compiled => eprintln!("e{index}: compiled"),
            ljos_aot::Outcome::Refused(why) => eprintln!("e{index}: {why}"),
        }
    }
    Ok(())
}

fn run_aot_coverage(root: &Path) -> anyhow::Result<()> {
    if !root.is_dir() {
        bail!(
            "{} is not there: `git submodule update --init corpus/qtdoc`",
            root.display()
        );
    }
    let mut coverage = ljos_aot::Coverage::default();
    let mut files = 0;
    for dir in baseline::project_dirs(root) {
        let Ok(results) = ljos_compiler::compile_project_with(&dir, QtTypeOracle) else {
            continue;
        };
        for result in results {
            let Ok(compiled) = result.result else {
                continue;
            };
            files += 1;
            let generated =
                ljos_compiler::aot::generate(&compiled.file, &QtTypeOracle, "::ljos::aot");
            coverage.add(&generated.outcomes);
        }
    }
    let total = coverage.compiled + coverage.refused;
    println!(
        "{files} files, {total} expressions, {} compiled ({:.1} %), {} refused",
        coverage.compiled,
        100.0 * coverage.compiled as f64 / total.max(1) as f64,
        coverage.refused
    );
    for (kind, count) in coverage.top() {
        println!("{count:6}  {kind}");
        if std::env::var_os("LJOS_AOT_DETAIL").is_some() {
            let mut details: Vec<_> = coverage.reasons[kind].iter().collect();
            details.sort_by(|a, b| b.1.cmp(a.1));
            for (detail, n) in details.into_iter().take(8) {
                println!("          {n:6}  {detail}");
            }
        }
    }
    Ok(())
}
