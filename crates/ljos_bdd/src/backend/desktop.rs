//! The `@desktop` surface: `ljos run` as a Wayland client of a real
//! compositor, with its platform host, off the user's screen.
//!
//! The compositor is the desktop sandbox's (`scripts/cua-sandbox.sh`): a
//! headless sway on its own bus whose seat has a virtual pointer and a
//! virtual keyboard that types nothing. The pointer is sway's own (wlr
//! virtual pointer), so input takes the path a mouse does; the keyboard is
//! there so that the compositor gives the keyboard focus and takes it away,
//! which another client of it does here. What the application holds is
//! read through its inspector (`--mcp`), over `ljos attach`.
//!
//! Nothing here waits for a while. The compositor and the inspector are
//! two sockets, so nothing orders a read after the input it follows; the
//! inspector's `app_watch` says each time the UI thread goes to sleep where
//! the window's last pass saw the pointer and when the thread wakes next
//! (`rests`), and the backend waits for the rest it needs.
//!
//! `mise run features:ljos` brings the sandbox up and names its sockets in
//! the file `LJOS_BDD_DESKTOP` points at, one `KEY=VALUE` a line, as
//! `cua-sandbox.sh env` prints them.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ljos_bench::sway::Sway;
use ljos_bench::wayland::Pointer;
use ljos_child::Reaped;
use serde_json::{Value, json};

use super::attached::{Running, SEEN};
use super::inspector::{Ask, Seen};
use super::{
    A11yNode, Backend, BackendError, Files, Input, LogEntry, LogLevel, PointerStep, Probes,
    QmlFile, Reload, Result, Tooling,
};

/// Names the file with the sandbox's sockets; set when one runs.
pub const ENV: &str = "LJOS_BDD_DESKTOP";
/// Why there is no sandbox, when the task that looked for one says; empty
/// when it did not look, as none of the files it runs has a `@desktop`
/// scenario.
pub const SKIP_ENV: &str = "LJOS_BDD_DESKTOP_SKIP";

/// How long the application may take to show its window.
const STARTUP: Duration = Duration::from_secs(60);
/// How long the compositor's picture of a window at rest must show no
/// change before it is taken: the application has presented its last
/// frame, and the compositor shows it within a frame or two of its own.
const STILL: Duration = Duration::from_millis(100);
/// The other client of the compositor, which takes the focus from the
/// application: a window of its own and nothing in it.
const OTHER_CLIENT: &str = "import QtQuick\nWindow { width: 120; height: 80; visible: true }\n";
/// How long a picture may take, the window settling included.
const PICTURE: Duration = Duration::from_secs(10);

/// Why `@desktop` scenarios cannot run here, or `None` when they can.
#[must_use]
pub fn unavailable() -> Option<String> {
    if let Some(why) = std::env::var_os(SKIP_ENV) {
        return Some(why.to_string_lossy().into_owned());
    }
    match std::env::var_os(ENV) {
        Some(file) if Path::new(&file).is_file() => None,
        Some(file) => Some(format!(
            "{ENV} names {}, which is not a file",
            Path::new(&file).display()
        )),
        None => Some(format!(
            "no desktop sandbox ({ENV} is not set; `mise run features:ljos` starts one)"
        )),
    }
}

#[derive(Debug)]
struct Sandbox {
    env: BTreeMap<String, String>,
}

impl Sandbox {
    fn from_env() -> Result<Self> {
        if let Some(why) = unavailable() {
            return Err(BackendError::Failed(why));
        }
        let file = std::env::var_os(ENV).expect("checked by unavailable");
        let text = std::fs::read_to_string(&file)?;
        let env = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Ok(Self { env })
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        self.env
            .get(key)
            .map(PathBuf::from)
            .ok_or_else(|| BackendError::Failed(format!("the sandbox does not say its {key}")))
    }
}

#[derive(Debug, Default)]
pub struct DesktopBackend {
    sandbox: Option<Sandbox>,
    pointer: Option<Pointer>,
    dir: Option<tempfile::TempDir>,
    running: Option<Running>,
    /// `ljos observe` on the running application.
    observer: Option<Running>,
    /// The passes read when counting began, and how many each reading
    /// itself runs.
    passes_from: Option<(u64, u64)>,
    /// Each of the application's threads by id, with its name and the
    /// times it had been woken when counting began.
    wakes_from: Option<BTreeMap<u32, (String, u64)>>,
    /// The work of the application and of its observer when counting
    /// began, and what each reading itself does.
    work_from: Vec<(&'static str, Counted)>,
    /// What the agent was last answered.
    seen: Seen,
    /// Whether the pointer's button is held.
    held: bool,
    /// What the application's environment has besides the sandbox's.
    env: Vec<(String, String)>,
    /// The application is a client of the sandbox's XWayland.
    x11: bool,
    /// Another client of the compositor, with a window that takes the
    /// focus from the application's, and the directory of its file, kept
    /// apart from the scenario's (which may have a file of that name).
    other: Option<(Reaped, tempfile::TempDir)>,
    /// The scenario took the seat's keyboard away, and it is given back
    /// when the scenario ends.
    keyboard_off: bool,
    /// The scenario changes files under the running application, which
    /// `ljos run --watch` applies.
    watch: bool,
}

impl Drop for DesktopBackend {
    fn drop(&mut self) {
        // Every later desktop scenario of the run would fail without a
        // keyboard, far from this cause, so a failed restore says so here.
        if self.keyboard_off
            && let Some(sandbox) = &self.sandbox
            && let Err(err) = sandbox_keyboard(sandbox, true)
        {
            eprintln!("[desktop] the seat's keyboard was not given back: {err}");
        }
    }
}

/// Gives the sandbox's seat its keyboard, or takes it away
/// (`cua-sandbox.sh keyboard on|off`), on the sandbox the description
/// names (its `LJOS_CUA_HOME`), which is the one the application runs on
/// whatever this process inherited.
fn sandbox_keyboard(sandbox: &Sandbox, on: bool) -> Result<()> {
    let script = crate::runner::workspace_root().join("scripts/cua-sandbox.sh");
    let told = Command::new(&script)
        .args(["keyboard", if on { "on" } else { "off" }])
        .env("LJOS_CUA_HOME", sandbox.path("LJOS_CUA_HOME")?)
        .stdin(Stdio::null())
        .output()?;
    if told.status.success() {
        Ok(())
    } else {
        Err(BackendError::Failed(format!(
            "{} keyboard {}: {}",
            script.display(),
            if on { "on" } else { "off" },
            String::from_utf8_lossy(&told.stderr).trim_end()
        )))
    }
}

/// Each thread of process `pid` by id: its name, and how many times the
/// kernel switched to it, as the times it was woken.
fn thread_wakes(pid: u32) -> Result<BTreeMap<u32, (String, u64)>> {
    let mut threads = BTreeMap::new();
    for task in std::fs::read_dir(format!("/proc/{pid}/task"))? {
        let task = task?;
        let Some(tid) = task.file_name().to_str().and_then(|t| t.parse().ok()) else {
            continue;
        };
        // A thread that ended between the listing and the read is gone.
        let Ok(status) = std::fs::read_to_string(task.path().join("status")) else {
            continue;
        };
        let field = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .map(str::trim)
        };
        let name = field("Name:").unwrap_or("?").to_owned();
        let n = |key| field(key).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let woke = n("voluntary_ctxt_switches:") + n("nonvoluntary_ctxt_switches:");
        threads.insert(tid, (name, woke));
    }
    Ok(threads)
}

/// Work read when counting began, and the work one reading does.
type Counted = (BTreeMap<String, u64>, BTreeMap<String, u64>);

impl DesktopBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the applications it starts from now on with `key` set to
    /// `value`, as a knob of the runtime is turned for one run.
    #[must_use]
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    fn running(&mut self) -> Result<&mut Running> {
        self.running
            .as_mut()
            .ok_or_else(|| BackendError::Failed("no application is running".into()))
    }

    /// The scenario's files, beside which the application runs.
    fn dir(&mut self) -> Result<&Path> {
        if self.dir.is_none() {
            let features = crate::runner::target_dir().join("features");
            std::fs::create_dir_all(&features)?;
            let dir = tempfile::Builder::new()
                .prefix("desktop-")
                .tempdir_in(features)?;
            self.dir = Some(dir);
        }
        Ok(self.dir.as_ref().expect("just made").path())
    }

    /// The program `features:run` built and pinned for this run
    /// (`LJOS_BIN`), as the other backends run it. The target's own
    /// `debug/ljos` is whatever was built there last, by this checkout or
    /// another that shares the target, so a scenario run on it tests
    /// another program than the branch's.
    fn ljos() -> Result<PathBuf> {
        super::testrun::ljos_bin()
    }

    fn start(&mut self, entry: &str) -> Result<()> {
        self.seen = Seen::default();
        self.passes_from = None;
        self.wakes_from = None;
        self.work_from.clear();
        self.observer = None;
        self.running = None;
        self.other = None;
        let sandbox = match &self.sandbox {
            Some(s) => s,
            None => self.sandbox.insert(Sandbox::from_env()?),
        };
        let dir = self.dir.as_ref().expect("files are written first").path();
        let ljos = Self::ljos()?;
        let mut command = Command::new(&ljos);
        command
            .current_dir(dir)
            .args(["run", entry, "--mcp=inspect,input,mutate,eval"])
            .args(self.watch.then_some("--watch"))
            // A Wayland client of the sandbox's compositor, not an X11 one
            // of its XWayland, unless the scenario says so, and nothing of
            // the developer's session.
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("AT_SPI_BUS_ADDRESS")
            .envs(sandbox.env.iter().filter(|(k, _)| {
                *k != if self.x11 {
                    "WAYLAND_DISPLAY"
                } else {
                    "DISPLAY"
                }
            }))
            // The sandbox's compositor composes in software, where the
            // shell would draw on the processor of itself: the scenarios
            // check GL's drawing unless one says otherwise.
            .env("LJOS_PAINTER", "gl")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(
                crate::runner::target_dir().join("features/desktop.log"),
            )?);
        // Killed on every way out, until the backend holds it.
        let mut app = Reaped::spawn(&mut command)?;
        let pid = app.id();
        let token = dir.join(format!("target/ljos/mcp-{pid}.json"));
        let sway = Sway::new(&sandbox.path("SWAYSOCK")?);
        let deadline = Instant::now() + STARTUP;
        let no_window = || {
            BackendError::Failed(format!(
                "ljos run showed no window in {} s",
                STARTUP.as_secs()
            ))
        };
        let rect = loop {
            if let Some(status) = app.try_wait()? {
                let log = std::fs::read_to_string(
                    crate::runner::target_dir().join("features/desktop.log"),
                )
                .unwrap_or_default();
                return Err(BackendError::Failed(format!(
                    "ljos run ended ({status}) before showing its window:\n{log}"
                )));
            }
            if token.is_file()
                && let Some(rect) = in_time(sway.window_rect(pid, Some(deadline)), no_window)?
                && rect.2 > 0
            {
                break rect;
            }
            if Instant::now() > deadline {
                return Err(no_window());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut running = Running::attach(&ljos, &token, app, rect)?;
        // The window is mapped once its first frame is up; the scene
        // answers once it is built.
        running.ask("app_eval", json!({"source": "true"}))?;
        self.running = Some(running);
        Ok(())
    }

    /// What the application and its observer have done so far.
    fn work(&mut self) -> Result<Vec<(&'static str, BTreeMap<String, u64>)>> {
        let mut work = vec![("application", self.running()?.work()?)];
        if let Some(observer) = self.observer.as_mut() {
            work.push(("observer", observer.work()?));
        }
        Ok(work)
    }

    /// `ljos observe` on the running application, with an inspector of
    /// its own through which what it does is read.
    fn start_observer(&mut self) -> Result<Running> {
        let sandbox = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?;
        let dir = self.dir.as_ref().expect("files are written first").path();
        let app =
            self.running.as_ref().map(|r| r.app.id()).ok_or_else(|| {
                BackendError::Failed("no application is running to observe".into())
            })?;
        let target = dir.join(format!("target/ljos/mcp-{app}.json"));
        // Its token file apart from the application's.
        let home = dir.join("observer");
        std::fs::create_dir_all(&home)?;
        let ljos = Self::ljos()?;
        let mut observer = Reaped::spawn(
            Command::new(&ljos)
                .current_dir(&home)
                .arg("observe")
                .arg(&target)
                .arg("--mcp=inspect,input")
                .env_remove("DISPLAY")
                .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
                .env_remove("AT_SPI_BUS_ADDRESS")
                .envs(sandbox.env.iter().filter(|(k, _)| *k != "DISPLAY"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(
                    crate::runner::target_dir().join("features/observer.log"),
                )?),
        )?;
        let token = home.join(format!("target/ljos/mcp-{}.json", observer.id()));
        let deadline = Instant::now() + STARTUP;
        while !token.is_file() {
            if let Some(status) = observer.try_wait()? {
                let log = std::fs::read_to_string(
                    crate::runner::target_dir().join("features/observer.log"),
                )
                .unwrap_or_default();
                return Err(BackendError::Failed(format!(
                    "ljos observe ended ({status}) before it served its inspector:\n{log}"
                )));
            }
            if Instant::now() > deadline {
                return Err(BackendError::Failed(format!(
                    "ljos observe served no inspector in {} s",
                    STARTUP.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut running = Running::attach(&ljos, &token, observer, (0, 0, 0, 0))?;
        running.ask("app_status", json!({}))?;
        Ok(running)
    }

    /// What the running application's inspector measures (`app_metrics`).
    ///
    /// # Errors
    /// No application is running, or its inspector went away.
    pub fn metrics(&mut self) -> Result<Value> {
        self.running()?.ask("app_metrics", json!({}))
    }

    /// The passes of the application's window so far, painted or not
    /// (`app_metrics`). The reading is served in a pass of its own, which
    /// the next reading counts.
    fn passes(&mut self) -> Result<u64> {
        let metrics = self.running()?.ask("app_metrics", json!({}))?;
        let paint = &metrics["paint"];
        match (
            paint["presented"].as_u64(),
            paint["unchanged"].as_u64(),
            paint["lost"].as_u64(),
        ) {
            (Some(presented), Some(unchanged), Some(lost)) => Ok(presented + unchanged + lost),
            _ => Err(BackendError::Failed(format!(
                "app_metrics says no passes: {metrics}"
            ))),
        }
    }

    fn sway(&self) -> Result<Sway> {
        let socket = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?
            .path("SWAYSOCK")?;
        Ok(Sway::new(&socket))
    }

    /// The process of another client of the compositor with a window up,
    /// started the first time one is wanted.
    fn other_client(&mut self) -> Result<u32> {
        if let Some((other, _)) = &self.other {
            return Ok(other.id());
        }
        let features = crate::runner::target_dir().join("features");
        std::fs::create_dir_all(&features)?;
        let own = tempfile::Builder::new()
            .prefix("desktop-other-")
            .tempdir_in(features)?;
        let dir = own.path().to_owned();
        std::fs::write(dir.join("other-client.qml"), OTHER_CLIENT)?;
        let sandbox = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?;
        let log = crate::runner::target_dir().join("features/desktop-other.log");
        let mut command = Command::new(Self::ljos()?);
        command
            .current_dir(&dir)
            .args(["run", "other-client.qml"])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("AT_SPI_BUS_ADDRESS")
            .envs(sandbox.env.iter().filter(|(k, _)| *k != "DISPLAY"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log)?);
        let mut other = Reaped::spawn(&mut command)?;
        let pid = other.id();
        let sway = self.sway()?;
        let deadline = Instant::now() + STARTUP;
        let no_window = || {
            BackendError::Failed(format!(
                "the other client showed no window in {} s",
                STARTUP.as_secs()
            ))
        };
        while in_time(sway.window_rect(pid, Some(deadline)), no_window)?.is_none_or(|r| r.2 == 0) {
            if let Some(status) = other.try_wait()? {
                let said = std::fs::read_to_string(&log).unwrap_or_default();
                return Err(BackendError::Failed(format!(
                    "the other client ended ({status}) before showing its window:\n{said}"
                )));
            }
            if Instant::now() > deadline {
                return Err(no_window());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.other = Some((other, own));
        Ok(pid)
    }

    fn pointer_device(&mut self) -> Result<&mut Pointer> {
        if self.pointer.is_none() {
            let display = self
                .sandbox
                .as_ref()
                .ok_or_else(|| BackendError::Failed("no sandbox".into()))?
                .path("WAYLAND_DISPLAY")?;
            let pointer = Pointer::new(&display)
                .map_err(|e| BackendError::Failed(format!("the sandbox's pointer: {e}")))?;
            self.pointer = Some(pointer);
        }
        Ok(self.pointer.as_mut().expect("just made"))
    }

    fn at(&mut self, x: f64, y: f64) -> Result<(u32, u32, (u32, u32))> {
        let display = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?
            .path("SWAYSOCK")?;
        let (wx, wy, _, _) = self.running()?.rect;
        let (lw, lh) = Sway::new(&display)
            .layout()?
            .ok_or_else(|| BackendError::Failed("the sandbox has no output".into()))?;
        let px = |origin: i32, v: f64| -> u32 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let p = (f64::from(origin) + v).round().max(0.0) as u32;
            p
        };
        #[allow(clippy::cast_sign_loss)]
        Ok((px(wx, x), px(wy, y), (lw as u32, lh as u32)))
    }
}

fn not_yet(what: &'static str) -> BackendError {
    BackendError::NotImplemented(what)
}

/// A compositor's answer that did not come by its deadline is the step's
/// own `late` failure, not an I/O error.
fn in_time<T>(answer: std::io::Result<T>, late: impl FnOnce() -> BackendError) -> Result<T> {
    match answer {
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Err(late()),
        other => Ok(other?),
    }
}

impl Backend for DesktopBackend {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        self.running = None;
        let dir = self.dir()?.to_owned();
        for f in files {
            let path = dir.join(&f.name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &f.source)?;
        }
        self.start(entry)
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        self.running()?
            .ask(
                "app_set_property",
                json!({"target": format!("#{object}"), "property": property, "value": value}),
            )
            .map(drop)
    }

    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()> {
        self.running()?
            .ask(
                "app_set_property",
                json!({"target": format!("#{object}"), "property": property, "binding": expression}),
            )
            .map(drop)
    }

    fn get(&mut self, object: &str, property: &str) -> Result<Value> {
        self.running()?.eval(object, property)
    }

    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value> {
        let args: Vec<String> = args.iter().map(Value::to_string).collect();
        self.running()?
            .eval(object, &format!("{method}({})", args.join(", ")))
    }

    fn exists(&mut self, object: &str) -> Result<bool> {
        Ok(self.running()?.eval(object, "true").is_ok())
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        // The application runs on the real clock here. What it does in
        // the next `ms` by itself is what it woke for by then: once it
        // sleeps with no wake of its own due by then, the time has passed
        // as far as it can tell. While work, passes or wakes are counted,
        // or another process is watched, the time is waited out: they
        // measure the threads and processes whose clocks no rest tells.
        let deadline = Instant::now() + Duration::from_millis(ms);
        let counting = self.passes_from.is_some()
            || self.wakes_from.is_some()
            || !self.work_from.is_empty()
            || self.observer.is_some();
        if counting {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            return Ok(());
        }
        // An inspector that tells no rests is waited out too.
        let quiet = |rest: &Value| match rest["wake_in_ms"].as_u64() {
            Some(n) => Instant::now() + Duration::from_millis(n) >= deadline,
            None => rest.is_object(),
        };
        // A busy application spends the time waited on it.
        self.running()?.rest_until(deadline, quiet)?;
        Ok(())
    }

    fn click(&mut self, _: &str, _: &str) -> Result<()> {
        Err(not_yet("a click by role and name on the desktop"))
    }

    fn key(&mut self, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "desktop",
            what: "keys: the sandbox's seat has a keyboard that types nothing",
        })
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        let (px, py, extent) = self.at(x, y)?;
        let pointer = self.pointer_device()?;
        let sent = match step {
            PointerStep::Press => pointer.press(px, py, extent),
            PointerStep::Move => pointer.move_to(px, py, extent),
            PointerStep::Release => pointer
                .move_to(px, py, extent)
                .and_then(|()| pointer.release()),
        };
        sent.map_err(|e| BackendError::Failed(format!("the sandbox's pointer: {e}")))?;
        let held = match step {
            PointerStep::Press => true,
            PointerStep::Release => false,
            PointerStep::Move => self.held,
        };
        self.held = held;
        // Where the window got it: the layout's pixels are whole.
        let (wx, wy) = (x.round(), y.round());
        let near = |v: &Value, want: f64| v.as_f64().is_some_and(|v| (v - want).abs() <= 1.0);
        let seen = self.running()?.rest_until(Instant::now() + SEEN, |rest| {
            let p = &rest["pointer"];
            p["pressed"] == held && near(&p["x"], wx) && near(&p["y"], wy)
        })?;
        if seen {
            Ok(())
        } else {
            let state = if held { "held" } else { "up" };
            Err(BackendError::Failed(format!(
                "the window did not see the pointer at {x}, {y} ({state}) in {} s",
                SEEN.as_secs()
            )))
        }
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        Err(not_yet("the accessibility tree on the desktop"))
    }

    fn log(&mut self) -> Result<Vec<LogEntry>> {
        // What the inspector keeps of the engine's log, the shell's own
        // lines among them, as `app_logs` lists it.
        let told = self.running()?.ask("app_logs", json!({"level": "info"}))?;
        let entries = told["entries"].as_array().cloned().unwrap_or_default();
        Ok(entries
            .iter()
            .filter_map(|e| {
                let level = match e["level"].as_str()? {
                    "debug" => LogLevel::Debug,
                    "info" => LogLevel::Info,
                    "warning" => LogLevel::Warning,
                    _ => LogLevel::Critical,
                };
                let message = e["message"].as_str()?.to_owned();
                Some(LogEntry { level, message })
            })
            .collect())
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        // What the compositor shows, as a person sees it: the frames the
        // application sent, after whatever it painted them with.
        // The application has drawn all it was going to once it rests
        // with its last frame presented; the compositor shows that frame
        // a moment later.
        let deadline = Instant::now() + PICTURE;
        self.running()?.presented(deadline)?;
        let display = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?
            .path("WAYLAND_DISPLAY")?;
        let failed = |e: ljos_bench::wayland::Error| {
            BackendError::Failed(format!("the sandbox's screencopy: {e}"))
        };
        // The window may have taken another size since it was first shown
        // (a root of another size put in it), and the compositor another
        // place for it. The application rests once it has drawn at the
        // size, and the compositor moves the window's place to it only
        // when its transaction ends, a moment later. So the picture is
        // taken once the window is still in the place the compositor
        // says, read after it was still: a place that moved while the
        // picture was still is looked at again. An X11 window asks the
        // compositor for its size, and is resizing until its place is the
        // size asked; a Wayland one draws at the size it takes, and its
        // place follows what it commits (`Place::resizing`).
        let pid = self.running()?.app.id();
        let rect = loop {
            let rect = self.running()?.rect;
            ljos_bench::wayland::quiet(&display, rect, STILL, deadline)
                .map_err(failed)?
                .ok_or_else(|| {
                    BackendError::Failed(format!(
                        "the window kept changing for {} s",
                        PICTURE.as_secs()
                    ))
                })?;
            let place = in_time(self.sway()?.window_place(pid, Some(deadline)), || {
                BackendError::Failed("the compositor did not say where the window is".into())
            })?
            .filter(|p| p.rect.2 > 0);
            match place {
                Some(p) if p.resizing => {
                    if Instant::now() >= deadline {
                        return Err(BackendError::Failed(format!(
                            "the compositor kept the window at {} by {} for {} s after it drew at another size",
                            p.rect.2,
                            p.rect.3,
                            PICTURE.as_secs()
                        )));
                    }
                    std::thread::sleep(STILL / 5);
                }
                Some(p) if p.rect != rect => self.running()?.rect = p.rect,
                _ => break rect,
            }
        };
        let (width, height, rgba) =
            ljos_bench::wayland::picture(&display, rect, deadline).map_err(failed)?;
        let encoding =
            |e: png::EncodingError| BackendError::Failed(format!("encoding the picture: {e}"));
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(std::io::Cursor::new(&mut png), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fastest);
        let mut writer = encoder.write_header().map_err(encoding)?;
        writer.write_image_data(&rgba).map_err(encoding)?;
        writer.finish().map_err(encoding)?;
        Ok(png)
    }

    fn as_files(&mut self) -> Option<&mut dyn Files> {
        Some(self)
    }

    fn as_input(&mut self) -> Option<&mut dyn Input> {
        Some(self)
    }

    fn as_reload(&mut self) -> Option<&mut dyn Reload> {
        Some(self)
    }

    fn as_probes(&mut self) -> Option<&mut dyn Probes> {
        Some(self)
    }

    fn as_tooling(&mut self) -> Option<&mut dyn Tooling> {
        Some(self)
    }
}

impl Input for DesktopBackend {
    fn key_hold(&mut self, _: &str, _: u32) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "desktop",
            what: "keys: the sandbox's seat has a keyboard that types nothing",
        })
    }

    /// The compositor moves the keyboard focus: to another client's window,
    /// or back to the application's. What the window makes of it is the
    /// scenario's to say; this waits only until the compositor has moved
    /// it and the application has woken for what it was told.
    fn window_active(&mut self, active: bool) -> Result<()> {
        let turns = self.running()?.turns()?;
        let target = if active {
            self.running()?.app.id()
        } else {
            self.other_client()?
        };
        let sway = self.sway()?;
        // One deadline from the command on: a compositor that stops
        // answering fails the step at it, whether it is the command's reply
        // or the focus after it that does not come, as one that answers with
        // another focus does.
        let deadline = Instant::now() + SEEN;
        let late = || {
            BackendError::Failed(format!(
                "sway did not move the focus to the window of pid {target} in {} s",
                SEEN.as_secs()
            ))
        };
        let told = in_time(
            sway.command(&format!("[pid={target}] focus"), Some(deadline)),
            late,
        )?;
        if told[0]["success"] != Value::Bool(true) {
            return Err(BackendError::Failed(format!(
                "sway would not focus the window of pid {target}: {told}"
            )));
        }
        while in_time(sway.focused(Some(deadline)), late)? != Some(target) {
            if Instant::now() >= deadline {
                return Err(late());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // The compositor tells the window in the same turn it moves the
        // focus. A window it tells nothing does not wake, and the steps
        // after this say what it missed.
        self.running()?.turned_after(turns, deadline)?;
        Ok(())
    }

    fn touch(&mut self, _: f64, _: f64) -> Result<()> {
        Err(not_yet("a touch on the desktop"))
    }

    fn seat_keyboard(&mut self, present: bool) -> Result<()> {
        if self.running.is_some() {
            return Err(BackendError::Failed(
                "the seat's keyboard changes before the application starts, which hears of it once"
                    .into(),
            ));
        }
        let sandbox = match &self.sandbox {
            Some(s) => s,
            None => self.sandbox.insert(Sandbox::from_env()?),
        };
        // `keyboard off` can fail after it has taken the keyboard away, so
        // the keyboard is given back whenever it was asked to go.
        if !present {
            self.keyboard_off = true;
        }
        sandbox_keyboard(sandbox, present)?;
        self.keyboard_off = !present;
        Ok(())
    }
}

impl Files for DesktopBackend {
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir()?.join(name);
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

impl Reload for DesktopBackend {
    fn watch_files(&mut self) {
        self.watch = true;
    }

    /// Saves the file beside the running application and waits until it
    /// has applied it (or failed to) and presented what it shows then:
    /// the inspector counts reloads as the watcher's quiet time ends.
    fn change_file(&mut self, name: &str, source: Option<&str>) -> Result<()> {
        let before = self
            .running()?
            .ask("app_watch", json!({"topics": ["reloads"]}))?["counts"]["reloads"]
            .as_u64()
            .unwrap_or(0);
        let path = self.dir()?.join(name);
        match source {
            Some(text) => std::fs::write(&path, text)?,
            None => std::fs::remove_file(&path)?,
        }
        let deadline = Instant::now() + SEEN;
        let wait_ms = u64::try_from(SEEN.as_millis()).unwrap_or(u64::MAX);
        let told = self.running()?.ask(
            "app_watch",
            json!({"since": {"reloads": before}, "topics": ["reloads"], "wait_ms": wait_ms}),
        )?;
        if told["counts"]["reloads"].as_u64().unwrap_or(before) == before {
            return Err(BackendError::Failed(format!(
                "the application did not reload {name} within {} s",
                SEEN.as_secs()
            )));
        }
        self.running()?.presented(deadline)
    }
}

impl Probes for DesktopBackend {
    fn draw_on_processor(&mut self) -> Result<()> {
        self.env.push(("LJOS_PAINTER".to_owned(), "cpu".to_owned()));
        Ok(())
    }

    fn application_ended(&mut self, within: Duration) -> Result<()> {
        let until = Instant::now() + within;
        let running = self.running()?;
        loop {
            if running.app.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the application is still running after {} ms",
                    within.as_millis()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn be_x11_client(&mut self) -> Result<()> {
        self.x11 = true;
        Ok(())
    }

    fn let_shell_choose_painter(&mut self) -> Result<()> {
        // Set and empty: chosen by nobody, and not GL by default (`start`).
        self.env.push(("LJOS_PAINTER".to_owned(), String::new()));
        Ok(())
    }

    fn reference_picture(&mut self) -> Result<Vec<u8>> {
        // Of the frame the window shows once it rests, as `grab` takes it.
        let running = self.running()?;
        running.settle()?;
        let answer = running.call("app_screenshot", json!({}))?;
        if let Some(r) = answer.refusal {
            return Err(BackendError::Failed(format!(
                "app_screenshot: {}",
                r.message
            )));
        }
        answer
            .picture
            .ok_or_else(|| BackendError::Failed("app_screenshot gave no picture".into()))
    }

    fn count_passes(&mut self) -> Result<()> {
        // Two readings say what one costs: its own pass, and any that pass
        // asked for, which have run once the thread rests.
        let first = self.passes()?;
        self.running()?.settle()?;
        let second = self.passes()?;
        self.passes_from = Some((second, second.saturating_sub(first)));
        Ok(())
    }

    fn passes_counted(&mut self) -> Result<u64> {
        let (from, reading) = self
            .passes_from
            .ok_or_else(|| BackendError::Failed("the passes were not being counted".into()))?;
        let now = self.passes()?;
        Ok(now.saturating_sub(from).saturating_sub(reading))
    }

    fn count_wakes(&mut self) -> Result<()> {
        let pid = self.running()?.app.id();
        self.wakes_from = Some(thread_wakes(pid)?);
        Ok(())
    }

    fn wakes_counted(&mut self) -> Result<Vec<(String, u64)>> {
        let pid = self.running()?.app.id();
        let from = self
            .wakes_from
            .as_ref()
            .ok_or_else(|| BackendError::Failed("the wakes were not being counted".into()))?;
        let now = thread_wakes(pid)?;
        Ok(now
            .into_iter()
            .filter(|(tid, _)| *tid != pid)
            // The inspector's own thread answers the waits that tell when
            // input was seen; it is the probe's, as its passes are.
            .filter(|(_, (name, _))| name != "ljos-mcp")
            .filter_map(|(tid, (name, n))| {
                let was = from.get(&tid).map_or(0, |(_, w)| *w);
                let woke = n.saturating_sub(was);
                (woke > 0).then_some((name, woke))
            })
            .collect())
    }

    fn window_wakes_counted(&mut self) -> Result<u64> {
        let pid = self.running()?.app.id();
        let from = self
            .wakes_from
            .as_ref()
            .ok_or_else(|| BackendError::Failed("the wakes were not being counted".into()))?;
        let was = from.get(&pid).map_or(0, |(_, w)| *w);
        let now = thread_wakes(pid)?.get(&pid).map_or(0, |(_, w)| *w);
        Ok(now.saturating_sub(was))
    }

    fn observe(&mut self) -> Result<()> {
        self.observer = None;
        self.observer = Some(self.start_observer()?);
        Ok(())
    }

    fn observer_call(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let observer = self
            .observer
            .as_mut()
            .ok_or_else(|| BackendError::Failed("the application is not observed".into()))?;
        let answer = observer.call(tool, arguments)?;
        if let Some(r) = answer.refusal {
            return Err(BackendError::Refused {
                tool: tool.to_owned(),
                code: r.code,
                message: r.message,
            });
        }
        // What the call woke is done before the next step looks.
        observer.settle()?;
        self.running()?.settle()?;
        Ok(answer.structured)
    }

    fn count_work(&mut self) -> Result<()> {
        // Two readings say what one costs, once what each asked for ran.
        let first = self.work()?;
        self.running()?.settle()?;
        if let Some(observer) = self.observer.as_mut() {
            observer.settle()?;
        }
        let second = self.work()?;
        self.work_from = first
            .into_iter()
            .zip(second)
            .map(|((who, first), (_, second))| {
                let cost = second
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.saturating_sub(first.get(k).copied().unwrap_or(0)),
                        )
                    })
                    .collect();
                (who, (second, cost))
            })
            .collect();
        Ok(())
    }

    fn work_counted(&mut self) -> Result<Vec<String>> {
        if self.work_from.is_empty() {
            return Err(BackendError::Failed(
                "the work was not being counted".into(),
            ));
        }
        let now = self.work()?;
        let mut did = Vec::new();
        for ((who, (from, cost)), (_, now)) in self.work_from.iter().zip(now) {
            for (kind, n) in now {
                let was = from.get(&kind).copied().unwrap_or(0);
                let reading = cost.get(&kind).copied().unwrap_or(0);
                let more = n.saturating_sub(was).saturating_sub(reading);
                if more > 0 {
                    did.push(format!("the {who} did {more} more of {kind}"));
                }
            }
        }
        Ok(did)
    }
}

impl Tooling for DesktopBackend {
    fn inspector(&mut self, ask: &Ask) -> Result<()> {
        let Ask::Call { tool, args } = ask else {
            return Err(BackendError::Unsupported {
                backend: "desktop",
                what: "an agent here only calls tools",
            });
        };
        let answer = self.running()?.call(tool, args.clone())?;
        self.seen.answer = Some(answer);
        Ok(())
    }

    fn inspector_seen(&mut self) -> Result<Seen> {
        Ok(self.seen.clone())
    }
}
