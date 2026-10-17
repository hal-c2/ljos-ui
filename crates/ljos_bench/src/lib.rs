//! `ljos bench`: QML applications measured on Ljós and on Qt, each inside a
//! transient cgroup with a limit on memory and CPU (a systemd scope), on a
//! compositor of their own.
//!
//! Every number comes from outside the application: memory and CPU from
//! the cgroup's own accounting and `/proc`, the first frame from the
//! compositor mapping the window, frames from the compositor's screencopy.
//! An application is let start only once its cgroup is checked to have the
//! limits asked for, so nothing is measured outside a limit.

pub mod cgroup;
pub mod gate;
pub mod measure;
pub mod procfs;
pub mod profile;
pub mod report;
pub mod sway;
pub mod wayland;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;

pub use measure::{Error, Gesture, Outcome, Phases, Run, Runtime, Setup};
pub use profile::Profile;
pub use report::Report;

/// An application to measure, as a manifest gives it.
#[derive(Clone, Debug, Deserialize)]
pub struct App {
    pub id: String,
    /// Its QML entry file.
    pub file: PathBuf,
    /// Import directories, for Qt's `-I` (Ljós finds modules beside the file).
    #[serde(default)]
    pub imports: Vec<PathBuf>,
    /// The window size Ljós is asked for, `WxH`; Qt takes the root's own.
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub ljos_args: Vec<String>,
    #[serde(default)]
    pub qt_args: Vec<String>,
    /// Click a grid over it while measuring load.
    #[serde(default = "yes")]
    pub drive: bool,
}

fn yes() -> bool {
    true
}

/// The applications of a manifest: `{"apps": [App…]}`.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub apps: Vec<App>,
}

impl App {
    /// A QML file on its own: its stem is its id and its directory its
    /// imports.
    #[must_use]
    pub fn file(path: &Path) -> Self {
        Self {
            id: path
                .file_stem()
                .map_or_else(|| "app".to_owned(), |s| s.to_string_lossy().into_owned()),
            file: path.to_owned(),
            imports: path.parent().map(Path::to_owned).into_iter().collect(),
            size: None,
            ljos_args: Vec::new(),
            qt_args: Vec::new(),
            drive: true,
        }
    }
}

/// The programs each runtime is.
#[derive(Clone, Debug)]
pub struct Programs {
    pub ljos: PathBuf,
    pub qml6: PathBuf,
    /// The compositor's socket the applications draw on.
    pub wayland: Option<PathBuf>,
    /// More environment for every application.
    pub env: Vec<(String, String)>,
}

/// How `app` is started on the runtime `name`; `None` for a runtime
/// there is none of.
#[must_use]
pub fn runtime(name: &str, app: &App, programs: &Programs) -> Option<Runtime> {
    let mut env = Vec::new();
    // Both draw on the compositor's Wayland socket, never through X11.
    let unset = vec!["DISPLAY".to_owned()];
    if let Some(display) = &programs.wayland {
        env.push(("WAYLAND_DISPLAY".to_owned(), display.display().to_string()));
        // The launcher needs the user's own runtime directory to reach the
        // service manager; the application gets the compositor's, as a
        // session on that compositor would.
        if let Some(dir) = display.parent().filter(|d| !d.as_os_str().is_empty()) {
            env.push(("XDG_RUNTIME_DIR".to_owned(), dir.display().to_string()));
        }
    }
    let (program, args): (PathBuf, Vec<OsString>) = match name {
        "ljos" => {
            let mut args: Vec<OsString> = vec!["run".into(), app.file.clone().into()];
            if let Some(size) = &app.size {
                args.extend(["--size".into(), size.into()]);
            }
            args.extend(app.ljos_args.iter().map(OsString::from));
            (programs.ljos.clone(), args)
        }
        "qt" => {
            env.push(("QT_QPA_PLATFORM".to_owned(), "wayland".to_owned()));
            // The style Ljós draws, so both draw the same controls.
            env.push(("QT_QUICK_CONTROLS_STYLE".to_owned(), "Basic".to_owned()));
            let mut args = Vec::new();
            for dir in &app.imports {
                args.extend([OsString::from("-I"), dir.clone().into()]);
            }
            args.push(app.file.clone().into());
            args.extend(app.qt_args.iter().map(OsString::from));
            (programs.qml6.clone(), args)
        }
        _ => return None,
    };
    env.extend(programs.env.iter().cloned());
    Some(Runtime {
        name: name.to_owned(),
        program,
        args,
        env,
        unset,
    })
}

/// What one bench run measures.
#[derive(Debug)]
pub struct Plan {
    pub apps: Vec<App>,
    pub runtimes: Vec<String>,
    pub profiles: Vec<Profile>,
    pub phases: Phases,
    /// Start each application once, unmeasured, before measuring it, so
    /// the shared libraries both runtimes map are already in memory and
    /// charged to neither.
    pub warm_up: bool,
    /// Measure each application this many times; the tables show the
    /// median. None at all leaves only the search for the least memory.
    pub repeat: u32,
    /// Search for the least memory each starts in, between these MiB,
    /// under the profile of this name.
    pub bisect: Option<(String, u64, u64)>,
    pub notes: Vec<String>,
}

/// Runs the plan, adding to `report` and handing it as it grows to
/// `progress` (after every run) so a long run that is stopped keeps what it
/// measured.
///
/// # Errors
/// An application was not inside its limit ([`Error::Refused`]), or one
/// could not be started.
pub fn run(
    setup: &Setup,
    plan: &Plan,
    programs: &Programs,
    mut report: Report,
    mut progress: impl FnMut(&Report, &str),
) -> Result<Report, Error> {
    for app in &plan.apps {
        for name in &plan.runtimes {
            let Some(rt) = runtime(name, app, programs) else {
                continue;
            };
            if plan.warm_up && plan.repeat > 0 {
                let warm = Phases {
                    settle: Duration::from_millis(500),
                    idle: Duration::ZERO,
                    load: Duration::ZERO,
                    gesture: measure::Gesture::Click,
                    detail: false,
                };
                measure::run(setup, &app.id, &rt, &Profile::desktop(), warm, false)?;
            }
            for profile in &plan.profiles {
                for repeat in 0..plan.repeat {
                    let mut r = measure::run(setup, &app.id, &rt, profile, plan.phases, app.drive)?;
                    r.repeat = repeat;
                    let line = format!(
                        "{} {} {} #{repeat} (load {}): {:?}, first frame {}, peak {}",
                        app.id,
                        name,
                        profile.name,
                        r.load_average.map_or("?".to_owned(), |l| format!("{l:.1}")),
                        r.outcome,
                        r.first_frame_ms
                            .map_or("—".to_owned(), |v| format!("{v:.0} ms")),
                        r.peak_memory
                            .map_or("—".to_owned(), |b| format!("{} KiB", b >> 10)),
                    );
                    report.runs.push(r);
                    progress(&report, &line);
                }
            }
            if let Some((name, lo, hi)) = &plan.bisect
                && let Some(profile) = plan.profiles.iter().find(|p| &p.name == name)
            {
                let m = measure::min_memory(setup, &app.id, &rt, profile, *lo, *hi)?;
                let line = format!(
                    "{} {} least memory under {}: {}",
                    app.id,
                    rt.name,
                    profile.name,
                    m.mib.map_or("> max".to_owned(), |v| format!("{v} MiB"))
                );
                report.min_memory.push(m);
                progress(&report, &line);
            }
        }
    }
    Ok(report)
}
