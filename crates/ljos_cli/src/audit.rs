//! `ljos audit`: loads a QML file without a window, as `ljos render`
//! does, and runs the design audit (`ljos_audit`) on the scene it shows,
//! at each point of a matrix of settings.
//!
//! The report is grouped by rule, each finding at its `file:line`; with
//! `--json` it is the object `app_audit` gives. The exit status is 1 when
//! a finding is an error (certain), 0 otherwise: warnings and hints are
//! for a person to look at, not a gate.

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use clap::Args;
use ljos_audit::{Matrix, Options, Severity};
use ljos_engine::Engine;

use crate::render;

#[derive(Args)]
pub struct Audit {
    /// The QML file to audit. Every `.qml` file in its directory and
    /// below is part of its project.
    file: PathBuf,
    /// The settings to audit under: `current` (the scene as it opens),
    /// `full` (also light and dark at normal and high contrast, larger
    /// text and the small window), or points such as
    /// `"scheme=dark contrast=high; font-scale=2 size=640x480"`.
    #[arg(long, default_value = "full", value_parser = parse_matrix)]
    matrix: Matrix,
    /// Only these rules or areas, comma separated:
    /// `layout-offscreen,color`.
    #[arg(long, value_delimiter = ',')]
    rules: Vec<String>,
    /// Hold what can be pressed to the size a finger needs, not a pointer.
    #[arg(long)]
    touch: bool,
    /// The window's size in logical pixels. Without it, the root's own
    /// size, and 800x600 for a side it leaves at 0, as `ljos render`.
    #[arg(long, value_parser = render::parse_size, value_name = "WxH")]
    size: Option<(u32, u32)>,
    /// Milliseconds of application time to let pass before the audit.
    #[arg(long, default_value_t = 500, value_name = "MS")]
    settle: u64,
    /// At most this many findings.
    #[arg(long, value_name = "N")]
    max_findings: Option<usize>,
    /// Also run the checks that act: focus each stop, press Escape,
    /// toggle controls and open popups, which runs the file's own
    /// handlers. On by default, since the file runs in an engine and a
    /// home of the audit's own; `--act=false` when its handlers reach
    /// out (files, the network) and must not run.
    // DECISION: on here, unlike `app_audit` on a running application,
    // because nothing the handlers change outlives this run but what
    // they send or write outside it.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_name = "BOOL")]
    act: bool,
    /// A gallery example id whose native backends to register, as for
    /// `ljos render --example`.
    #[arg(long, value_name = "ID")]
    example: Option<String>,
    /// The report as JSON on standard output.
    #[arg(long)]
    json: bool,
}

fn parse_matrix(text: &str) -> Result<Matrix, String> {
    text.parse().map_err(|e| format!("{e}"))
}

impl Audit {
    /// The exit status: 1 when an error was found.
    pub fn run(self) -> anyhow::Result<i32> {
        let mut options = if self.touch {
            Options::mobile()
        } else {
            Options::desktop()
        };
        options.matrix = self.matrix.clone();
        options.max_findings = self.max_findings;
        options.act = self.act;
        let options = options.only(&self.rules)?;

        let file = self
            .file
            .canonicalize()
            .with_context(|| format!("{}", self.file.display()))?;
        let Some(dir) = file.parent() else {
            bail!("{} is in no directory", file.display());
        };
        let Some(entry) = file.file_name().and_then(|n| n.to_str()) else {
            bail!("{} has no file name", file.display());
        };
        let key = entry.strip_suffix(".qml").unwrap_or(entry).to_owned();

        let engine = Engine::new();
        // The bundled font: the same layout, and so the same report, on
        // every machine.
        ljos::render::set_fonts(ljos::render::Fonts::Bundled);
        render::register_offscreen(&engine);
        // `Settings` reads and writes the user's configuration: a home of
        // the run's own, so that what one audit's points wrote (a theme
        // that followed the scheme) is not what the next one starts from,
        // and the user's own settings are neither audited nor changed.
        let home = Home::new()?;
        ljos_platform::Platform::of(&engine).set_env(home.env());
        render::register_example(&engine, self.example.as_deref());
        ljos_controls::set_application_arguments(
            &engine,
            [crate::run::program(), file.display().to_string()],
        );
        let oracle = match &self.example {
            Some(_) => ljos::dev::EngineOracle::from_engine(&engine),
            None => ljos::dev::EngineOracle::table_only(),
        };
        let source = render::load(dir, &key, oracle)?;

        let (width, height) = self.size.unwrap_or(render::DEFAULT_SIZE);
        let picture = ljos::render::Picture::new(width, height).settle(self.settle);
        let picture = if self.size.is_none() {
            picture.root_sized()
        } else {
            picture
        };
        picture.install(&engine);
        engine.set_component_source(source);
        render::stage_resources(&engine, dir);
        let root = engine
            .create_from_url(
                &key,
                None,
                render::example_properties(self.example.as_deref()),
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Where the scene had its items before the settle time passed:
        // what moved since is a layout shift.
        let mut before = None;
        let mut scene = picture
            .scene_seen(&engine, root, |e, s| {
                before = Some(std::sync::Arc::new(ljos_audit::Placement::of(e, s)));
            })
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let options = Options { before, ..options };
        let report = ljos_audit::audit(&engine, &mut scene, picture.now(), &options);
        // The engine writes what `Settings` holds as it goes, before the
        // home it writes in goes.
        drop(scene);
        drop(engine);
        ljos_platform::flush_writes();
        drop(home);

        if self.json {
            println!("{}", report.json());
        } else {
            let base = format!("file://{}", dir.to_string_lossy());
            print!("{}", report.by_rule(&base));
            let points: Vec<String> = report.points.iter().map(ToString::to_string).collect();
            println!(
                "{} error(s), {} warning(s), {} hint(s) at {} point(s): {}",
                report.count(Severity::Error),
                report.count(Severity::Warning),
                report.count(Severity::Hint),
                points.len(),
                points.join("; ")
            );
        }
        Ok(i32::from(report.count(Severity::Error) > 0))
    }
}

/// A home directory for one run, under the temporary directory, removed
/// when the run ends.
struct Home(PathBuf);

impl Home {
    fn new() -> anyhow::Result<Home> {
        let dir = std::env::temp_dir().join(format!("ljos-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).with_context(|| format!("{}", dir.display()))?;
        Ok(Home(dir))
    }

    /// The process's environment with this home and no XDG directory of
    /// the user's.
    fn env(&self) -> ljos_platform::PathEnv {
        let mut env = ljos_platform::PathEnv::from_process();
        env.home = self.0.clone();
        env.vars
            .retain(|k, _| matches!(k.as_str(), "PATH" | "TMPDIR" | "XDG_RUNTIME_DIR"));
        env
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
