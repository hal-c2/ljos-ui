//! `ljos types`: what another tool reads of Ljós's types. `--dts` writes
//! the table of types as TypeScript declarations, the ones `ljos check
//! --tsc` gives TypeScript. Given directories, it writes their `.lts`
//! modules' declarations to the project's `.ljos/`, as `ljos check` does
//! (docs/DIALECT.md §4.21), for a tool that runs before any check.

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use clap::Args;
use ljos_compiler::ts::{Api, dts::API};

use crate::stdout::{out, outln};

#[derive(Args)]
pub struct Types {
    /// Write the declarations of Ljós's API (`Ljos.Ui`) for TypeScript.
    #[arg(long, conflicts_with_all = ["paths", "project"])]
    dts: bool,
    /// Write them to this file rather than to standard output.
    #[arg(long, value_name = "FILE", requires = "dts")]
    out: Option<PathBuf>,
    /// The project's root, under which `.ljos/` is written; by default the
    /// nearest directory up with a `tsconfig.json`.
    #[arg(long, value_name = "DIR")]
    project: Option<PathBuf>,
    /// The directories whose `.lts` modules are declared; by default this
    /// one.
    paths: Vec<PathBuf>,
}

impl Types {
    pub fn run(self) -> anyhow::Result<()> {
        if self.dts {
            return dts(self.out.as_ref());
        }
        let dirs = if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths
        };
        let Some(lts) = crate::generated::find(self.project.as_deref(), &dirs)? else {
            outln!("0 modules, 0 files written, 0 removed");
            return Ok(());
        };
        let written = lts.write()?;
        let below =
            |path: &std::path::Path| crate::generated::shown(std::slice::from_ref(&lts.root), path);
        let mut lines: Vec<String> = written
            .changed
            .iter()
            .map(|p| format!("wrote {}", below(p)))
            .chain(
                written
                    .removed
                    .iter()
                    .map(|p| format!("removed {}", below(p))),
            )
            .collect();
        lines.sort();
        for line in lines {
            outln!("{line}");
        }
        outln!(
            "{} modules, {} files written, {} removed",
            lts.modules.len(),
            written.changed.len(),
            written.removed.len()
        );
        Ok(())
    }
}

fn dts(to: Option<&PathBuf>) -> anyhow::Result<()> {
    let Some(api) = Api::new() else {
        bail!("the table of types has no module {API}");
    };
    let declarations = api.declarations();
    match to {
        Some(path) => std::fs::write(path, declarations)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => out!("{declarations}"),
    }
    Ok(())
}
