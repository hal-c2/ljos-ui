//! `ljos design export`: writes the module `Ljos.Design` as plain QML into a
//! directory, for an application that also runs on Qt (where the module is
//! not built in) or that wants the exact palette of its own seed colour.

use std::{fs, path::PathBuf};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use ljos_design::{
    module::{self, Seeds},
    palette::Seed,
};

#[derive(Subcommand)]
pub enum DesignCommand {
    /// Write the files of `Ljos.Design` to `<dir>/Ljos/Design`, so that a
    /// file with `<dir>` on its import path can `import Ljos.Design`.
    Export(Export),
}

#[derive(Args)]
pub struct Export {
    /// The import directory to write below.
    dir: PathBuf,
    /// The colour (`#1d4ed8`, a colour keyword) or OKLCH hue in degrees the
    /// palette grows from. Without it the module has a palette every 30°
    /// of hue and `Colors.seedHue` picks one.
    #[arg(long)]
    seed: Option<String>,
}

pub fn run(command: DesignCommand) -> anyhow::Result<()> {
    match command {
        DesignCommand::Export(export) => export.run(),
    }
}

impl Export {
    fn run(self) -> anyhow::Result<()> {
        let seeds = match &self.seed {
            None => Seeds::Hues,
            Some(s) => match Seed::parse(s) {
                Some(seed) => Seeds::One(seed),
                None => bail!("--seed {s}: not a colour (#rrggbb, a keyword) or a hue in degrees"),
            },
        };
        let dir = self.dir.join(module::DIRECTORY);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        for f in module::files(seeds) {
            let path = dir.join(&f.name);
            fs::write(&path, &f.text).with_context(|| format!("writing {}", path.display()))?;
        }
        println!("{}", dir.display());
        Ok(())
    }
}
