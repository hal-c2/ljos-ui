//! `ljos bench`: QML applications on Ljós and on Qt, each inside a limit
//! on memory and CPU, measured from outside (see `ljos_bench`).

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::Args;
use ljos_bench::{
    App, Gesture, Manifest, Phases, Plan, Profile, Programs, Setup, profile, sway::Sway,
};

#[derive(Args)]
// `mise run bench` gives defaults that options after them override.
#[command(args_override_self = true)]
pub struct Bench {
    /// QML files to measure, each on its own.
    files: Vec<PathBuf>,
    /// A JSON manifest of applications: `{"apps": [{"id", "file",
    /// "imports", "size", "ljos_args", "qt_args", "drive", "clicks"}]}`;
    /// "clicks" are where to click, as fractions of the window, instead of
    /// the grid.
    #[arg(long)]
    manifest: Option<PathBuf>,
    /// Runtimes, comma separated: `ljos` (`ljos run`) and `qt` (`qml6`).
    #[arg(long, value_delimiter = ',', default_value = "ljos,qt")]
    runtime: Vec<String>,
    /// Profiles, comma separated: `desktop` (no limit) and `pi-zero`.
    #[arg(long, value_delimiter = ',', default_value = "desktop,pi-zero")]
    profile: Vec<String>,
    /// Memory of the pi-zero profile, with no swap: bytes, or with K, M or G.
    #[arg(long, default_value = "256M", value_parser = size)]
    memory_max: u64,
    /// Cores of the pi-zero profile.
    #[arg(long, default_value_t = 4)]
    cpus: u32,
    /// How many times slower than one of this machine's cores a pi-zero
    /// core is; each core gets 100/factor percent of one of ours.
    #[arg(long, default_value_t = profile::DEFAULT_CPU_FACTOR)]
    cpu_factor: f64,
    /// The CPUs the pi-zero profile runs on; by default `--cpus` CPUs on
    /// distinct physical cores.
    #[arg(long)]
    cpu_list: Option<String>,
    /// What starts an application in a transient scope.
    #[arg(long, default_value = "systemd-run")]
    launcher: PathBuf,
    /// Where cgroup paths are rooted.
    #[arg(long, default_value = "/sys/fs/cgroup")]
    cgroup_root: PathBuf,
    /// The `ljos` measured; this one by default.
    #[arg(long)]
    ljos: Option<PathBuf>,
    /// The `qml6` measured.
    #[arg(long, default_value = "qml6")]
    qml6: PathBuf,
    /// The compositor's IPC socket: the first frame is when it maps the
    /// window. Without it no first frame is seen.
    #[arg(long, env = "LJOS_BENCH_SWAYSOCK")]
    sway: Option<PathBuf>,
    /// The compositor's Wayland socket the applications draw on, which the
    /// bench also clicks and counts frames through.
    #[arg(long, env = "LJOS_BENCH_WAYLAND")]
    wayland: Option<PathBuf>,
    /// More environment for every application, `KEY=VALUE`.
    #[arg(long = "app-env", value_parser = key_value)]
    app_env: Vec<(String, String)>,
    /// Seconds to wait for the first frame.
    #[arg(long, default_value_t = 30.0)]
    startup_timeout: f64,
    /// Seconds to wait for the launcher to put the application in its scope.
    #[arg(long, default_value_t = 10.0)]
    isolation_timeout: f64,
    /// Seconds after the first frame before settled memory is read.
    #[arg(long, default_value_t = 3.0)]
    settle: f64,
    /// Seconds of idle CPU and frames.
    #[arg(long, default_value_t = 10.0)]
    idle: f64,
    /// Seconds of driving the window with `--gesture`.
    #[arg(long, default_value_t = 10.0)]
    load: f64,
    /// Run the load span this many times back to back, reading memory
    /// after each: memory that keeps climbing is a leak, memory that
    /// settles a cache.
    #[arg(long, default_value_t = 1)]
    rounds: u32,
    /// What drives the window under load: `click` (a grid of clicks over
    /// it), `drag` (up and down from its middle) or `scroll` (the wheel).
    #[arg(long, default_value = "click", value_parser = gesture)]
    gesture: Gesture,
    /// Measure each application this many times; the tables show the
    /// median. 0 with `--bisect` only searches for the least memory.
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    /// Start each application once, unmeasured, first.
    #[arg(long)]
    warm_up: bool,
    /// Do not count the instructions each thread retires while measured.
    #[arg(long)]
    no_instructions: bool,
    /// Also search for the least memory each starts in under this profile.
    #[arg(long, value_name = "PROFILE")]
    bisect: Option<String>,
    /// The range searched, in MiB.
    #[arg(long, default_value = "8,512", value_delimiter = ',', num_args = 2)]
    bisect_range: Vec<u64>,
    /// A line for the report's notes (how the runtimes were built).
    #[arg(long)]
    note: Vec<String>,
    /// Add to the report already in `--out` rather than starting anew, so
    /// a long run can be made in parts.
    #[arg(long)]
    append: bool,
    /// Where `report.json`, `report.md` and the applications' logs go.
    #[arg(long, default_value = "target/bench")]
    out: PathBuf,
    /// Only check that motion is not reduced, as a run does before it
    /// measures: status 0 if it is not, 2 if it is. For scripts that
    /// measure without the bench.
    #[arg(long)]
    check_motion: bool,
}

/// `ljos bench-gate`: the gate an application passes into its scope by.
#[derive(Args)]
pub struct Gate {
    #[arg(long)]
    fifo: PathBuf,
    #[arg(long = "env", value_parser = key_value)]
    env: Vec<(String, String)>,
    #[arg(long)]
    unset: Vec<String>,
    #[arg(last = true, required = true)]
    command: Vec<OsString>,
}

impl Gate {
    pub fn run(self) -> anyhow::Error {
        let (program, args) = self.command.split_first().expect("clap requires a command");
        let e = ljos_bench::gate::pass(&self.fifo, &self.env, &self.unset, program, args);
        anyhow::anyhow!("cannot start {}: {e}", Path::new(program).display())
    }
}

fn size(text: &str) -> Result<u64, String> {
    let t = text.trim();
    let (digits, shift) = match t.chars().last() {
        Some('K' | 'k') => (&t[..t.len() - 1], 10),
        Some('M' | 'm') => (&t[..t.len() - 1], 20),
        Some('G' | 'g') => (&t[..t.len() - 1], 30),
        _ => (t, 0),
    };
    digits
        .parse::<u64>()
        .map(|n| n << shift)
        .map_err(|e| format!("{text:?} is not a size: {e}"))
}

fn key_value(text: &str) -> Result<(String, String), String> {
    text.split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| format!("{text:?} is not KEY=VALUE"))
}

fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// A program named with a directory is a file; a bare name is looked up
/// on PATH by whoever starts it.
fn program(path: &Path) -> std::io::Result<PathBuf> {
    if path.components().count() > 1 || path.exists() {
        absolute(path)
    } else {
        Ok(path.to_owned())
    }
}

/// Whether a refusal ended the run, which a script tells by the status.
pub const REFUSED: i32 = 2;

impl Bench {
    pub fn run(self) -> anyhow::Result<i32> {
        // The applications' bus, which they read the setting on.
        let bus = self
            .app_env
            .iter()
            .rev()
            .find(|(k, _)| k == "DBUS_SESSION_BUS_ADDRESS")
            .map(|(_, v)| v.as_str());
        let motion = ljos_bench::motion::read(bus);
        if motion.reduced_motion {
            eprintln!(
                "ljos bench: reduced motion is on: Ljós would move nothing, so no number would compare with Qt's. Turn it off in the desktop's settings and run again."
            );
            return Ok(REFUSED);
        }
        if self.check_motion {
            return Ok(0);
        }
        let me = std::env::current_exe().context("finding this program")?;
        let ljos = match &self.ljos {
            Some(p) => program(p)?,
            None => me.clone(),
        };
        let mut apps: Vec<App> = Vec::new();
        if let Some(path) = &self.manifest {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let m: Manifest = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a bench manifest", path.display()))?;
            apps.extend(m.apps);
        }
        for f in &self.files {
            apps.push(App::file(&absolute(f)?));
        }
        anyhow::ensure!(
            !apps.is_empty(),
            "nothing to measure: give QML files or --manifest"
        );

        for name in &self.runtime {
            anyhow::ensure!(
                name == "ljos" || name == "qt",
                "no runtime {name:?}: ljos or qt"
            );
        }
        let mut profiles = Vec::new();
        for name in &self.profile {
            profiles.push(match name.as_str() {
                "desktop" => Profile::desktop(),
                "pi-zero" => {
                    let list = match &self.cpu_list {
                        Some(l) => l.clone(),
                        None => profile::distinct_cores(self.cpus).with_context(|| {
                            format!(
                                "this machine has fewer than {} cores; give --cpu-list",
                                self.cpus
                            )
                        })?,
                    };
                    Profile::small_board(
                        "pi-zero",
                        self.memory_max,
                        self.cpus,
                        self.cpu_factor,
                        list,
                    )
                }
                other => anyhow::bail!("no profile {other:?}: desktop or pi-zero"),
            });
        }
        let secs = Duration::from_secs_f64;
        let out = absolute(&self.out)?;
        let setup = Setup {
            launcher: program(&self.launcher)?,
            cgroup_root: absolute(&self.cgroup_root)?,
            gate: me,
            work: out.join("work"),
            sway: self.sway.as_deref().map(Sway::new),
            wayland: self.wayland.clone(),
            startup_timeout: secs(self.startup_timeout),
            isolation_timeout: secs(self.isolation_timeout),
        };
        let bisect = match (&self.bisect, self.bisect_range.as_slice()) {
            (Some(p), [lo, hi]) if lo < hi => Some((p.clone(), *lo, *hi)),
            (Some(_), _) => anyhow::bail!("--bisect-range is two MiB values, the smaller first"),
            (None, _) => None,
        };
        let plan = Plan {
            apps,
            runtimes: self.runtime.clone(),
            profiles,
            phases: Phases {
                settle: secs(self.settle),
                idle: secs(self.idle),
                load: secs(self.load),
                rounds: self.rounds,
                gesture: self.gesture,
                detail: true,
                instructions: !self.no_instructions,
            },
            warm_up: self.warm_up,
            repeat: self.repeat,
            bisect,
            notes: self.note.clone(),
        };
        let programs = Programs {
            ljos,
            qml6: program(&self.qml6)?,
            wayland: self.wayland.clone(),
            env: self.app_env.clone(),
        };
        std::fs::create_dir_all(&out)?;
        let json = out.join("report.json");
        let md = out.join("report.md");
        let report = if self.append && json.exists() {
            let text = std::fs::read_to_string(&json)?;
            let mut old: ljos_bench::Report = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a bench report", json.display()))?;
            old.extend(&plan.profiles, &plan.notes);
            old
        } else {
            ljos_bench::Report::new(plan.profiles.clone(), plan.notes.clone())
        };
        let report = ljos_bench::Report { motion, ..report };
        let write = |report: &ljos_bench::Report| -> std::io::Result<()> {
            std::fs::write(&json, serde_json::to_string_pretty(report)? + "\n")?;
            std::fs::write(&md, ljos_bench::report::markdown(report))
        };
        let result = ljos_bench::run(&setup, &plan, &programs, report, |report, line| {
            eprintln!("{line}");
            if let Err(e) = write(report) {
                eprintln!("cannot write the report: {e}");
            }
        });
        match result {
            Ok(report) => {
                write(&report)?;
                println!(
                    "wrote {} and {}",
                    self.out.join("report.json").display(),
                    self.out.join("report.md").display()
                );
                Ok(0)
            }
            Err(e @ ljos_bench::Error::Refused(_)) => {
                eprintln!("ljos bench: {e}");
                Ok(REFUSED)
            }
            Err(e) => Err(e.into()),
        }
    }
}

fn gesture(s: &str) -> Result<Gesture, String> {
    match s {
        "click" => Ok(Gesture::Click),
        "drag" => Ok(Gesture::Drag),
        "scroll" => Ok(Gesture::Scroll),
        _ => Err(format!("{s}: the gestures are click, drag and scroll")),
    }
}
