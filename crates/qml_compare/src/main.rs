//! `qml-compare`: score pictures, and run a corpus through a renderer.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use qml_compare::image::Rgba;
use qml_compare::manifest::Manifest;
use qml_compare::metric::{self, DEFAULT_THRESHOLD};
use qml_compare::report::{self, Ratchet};
use qml_compare::run::{self, Options};
use serde_json::json;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Score an actual picture against a reference; prints JSON.
    Diff {
        reference: PathBuf,
        actual: PathBuf,
        /// Matching threshold, 0 (strict) to 1 (loose).
        #[arg(long, default_value_t = DEFAULT_THRESHOLD)]
        threshold: f64,
        /// Write a picture of where they differ.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run every example of a corpus through a renderer and score it.
    Run {
        #[arg(long)]
        manifest: PathBuf,
        /// Shell command; `{id}` and `{out}` are replaced by the example id
        /// and the PNG path it must write.
        #[arg(long)]
        renderer: String,
        /// Floors the scores must not fall under.
        #[arg(long)]
        ratchet: Option<PathBuf>,
        /// Add newly rendering examples to the ratchet file.
        #[arg(long, requires = "ratchet")]
        update: bool,
        /// Where the report and pictures go.
        #[arg(long, default_value = "report")]
        report_dir: PathBuf,
        #[arg(long, default_value_t = DEFAULT_THRESHOLD)]
        threshold: f64,
        /// Run only these example ids.
        ids: Vec<String>,
    },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("qml-compare: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Diff {
            reference,
            actual,
            threshold,
            out,
        } => {
            let reference = Rgba::read(&reference)?;
            let actual = Rgba::read(&actual)?;
            match metric::compare(&actual, &reference, threshold) {
                Ok(c) => {
                    if let Some(out) = out {
                        c.diff.write(&out)?;
                    }
                    println!("{}", json!({ "pixels": c.pixels, "content": c.content }));
                    Ok(ExitCode::SUCCESS)
                }
                Err(m) => {
                    println!("{}", json!({ "mismatch": m.0 }));
                    Ok(ExitCode::from(1))
                }
            }
        }
        Command::Run {
            manifest,
            renderer,
            ratchet,
            update,
            report_dir,
            threshold,
            ids,
        } => {
            let manifest = Manifest::read(&manifest)?;
            let options = Options {
                renderer,
                report_dir: report_dir.clone(),
                threshold,
                only: ids,
            };
            let rows = run::run(&manifest, &options)?;

            let width = std::env::var("COLUMNS")
                .ok()
                .and_then(|c| c.parse().ok())
                .unwrap_or(160);
            println!("\n{}", report::table(&rows, width));
            println!(
                "files: sources the renderer takes; entry: the entry file is one; renders: a picture came out;"
            );
            println!(
                "pixels: pixels the same as the reference; content: the same, not counting background."
            );

            let mut text = serde_json::to_string_pretty(&rows)?;
            text.push('\n');
            std::fs::write(report_dir.join("report.json"), text).context("writing report.json")?;
            std::fs::write(
                report_dir.join("index.html"),
                report::html(&rows, &report_dir),
            )
            .context("writing index.html")?;
            println!(
                "pictures and the whole report: {}",
                report_dir.join("index.html").display()
            );

            let mut failures = Vec::new();
            if let Some(path) = ratchet {
                let mut floors = Ratchet::read(&path)?;
                failures = floors.check(&rows);
                if update {
                    let added = floors.extend(&rows);
                    floors.write(&path)?;
                    println!(
                        "{}: {}",
                        path.display(),
                        if added.is_empty() {
                            "nothing new renders".into()
                        } else {
                            format!("added {}", added.join(", "))
                        }
                    );
                }
            }
            if failures.is_empty() {
                Ok(ExitCode::SUCCESS)
            } else {
                println!("\nFAILED:");
                for f in &failures {
                    println!("  {f}");
                }
                Ok(ExitCode::from(1))
            }
        }
    }
}
