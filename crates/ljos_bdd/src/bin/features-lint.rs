//! `mise run features:lint`: structure, step coverage and phrasing sprawl.

use std::{path::Path, process::ExitCode};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = ljos_bdd::steps::source();
    let report = ljos_bdd::lint::lint(&root.join("../../features"), &steps);
    for e in &report.errors {
        eprintln!("error: {e}");
    }
    println!(
        "{} files, {} scenarios; {:.0}% have an oracle tag (qt-parity, qt-diverges, ljos-only, qt-pixels)",
        report.files,
        report.scenarios,
        if report.scenarios == 0 {
            0.0
        } else {
            report.verified as f64 * 100.0 / report.scenarios as f64
        }
    );
    if report.planned_steps > 0 {
        println!(
            "{} step uses in backlog scenarios have no definition yet",
            report.planned_steps
        );
    }
    println!(
        "{} distinct step shapes, {:.0} per 100 scenarios",
        report.distinct_steps,
        report.steps_per_100_scenarios()
    );
    if report.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
