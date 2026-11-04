//! `ljos types`: the table of types Ljós's API is checked against, as
//! another tool reads it. `--dts` writes it as TypeScript declarations, the
//! ones `ljos check --tsc` gives TypeScript.

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use clap::Args;
use ljos_compiler::ts::{Api, dts::API};

use crate::stdout::out;

#[derive(Args)]
pub struct Types {
    /// Write the declarations of Ljós's API (`Ljos.Ui`) for TypeScript.
    #[arg(long, required = true)]
    dts: bool,
    /// Write them to this file rather than to standard output.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
}

impl Types {
    pub fn run(self) -> anyhow::Result<()> {
        if !self.dts {
            bail!("say what to write: --dts");
        }
        let Some(api) = Api::new() else {
            bail!("the table of types has no module {API}");
        };
        let declarations = api.declarations();
        match &self.out {
            Some(path) => std::fs::write(path, declarations)
                .with_context(|| format!("cannot write {}", path.display()))?,
            None => out!("{declarations}"),
        }
        Ok(())
    }
}
