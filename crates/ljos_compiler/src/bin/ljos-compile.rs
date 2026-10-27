//! `ljos-compile baseline [dir]`: how much of a tree of QML projects
//! compiles. `ljos-compile dump <dir> <file>`: one file's IR and module.
//! `ljos-compile aot <dir> <file>`: the Rust a shipped file's bindings and
//! handlers compile to. `ljos-compile aot-coverage [dir]`: how much of a
//! tree of QML projects compiles to Rust, and why the rest does not; with
//! `LJOS_AOT_OUT=<file.rs>` it writes that Rust too, every file's module
//! and the table, as an application's build would (to time rustc on it);
//! with `LJOS_AOT_FILES=1`, each file's compiled and total counts on stderr.

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
    let mut generated = Vec::new();
    for dir in baseline::project_dirs(root) {
        let Ok(results) = ljos_compiler::compile_project_with(&dir, QtTypeOracle) else {
            continue;
        };
        for result in results {
            let Ok(compiled) = result.result else {
                continue;
            };
            files += 1;
            let outcome =
                ljos_compiler::aot::generate(&compiled.file, &QtTypeOracle, "ljos_engine::aot");
            coverage.add(&outcome.outcomes);
            // One line per file, to count a subset of the corpus (the files
            // another compiler could resolve, say) the same way.
            if std::env::var_os("LJOS_AOT_FILES").is_some() {
                let compiled = outcome
                    .outcomes
                    .iter()
                    .filter(|o| matches!(o, ljos_aot::Outcome::Compiled))
                    .count();
                eprintln!(
                    "{compiled}\t{}\t{}/{}",
                    outcome.outcomes.len(),
                    dir.display(),
                    result.key
                );
            }
            generated.push((format!("{}/{}", dir.display(), result.key), outcome));
        }
    }
    if let Some(out) = std::env::var_os("LJOS_AOT_OUT") {
        let named: Vec<_> = generated.iter().map(|(k, g)| (k.as_str(), g)).collect();
        std::fs::write(&out, ljos_aot::assemble("ljos_engine::aot", &named))
            .with_context(|| format!("writing {}", Path::new(&out).display()))?;
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
