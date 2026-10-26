//! The `@desktop` surface: `ljos run` as a Wayland client of a real
//! compositor, with its platform host, off the user's screen.
//!
//! The compositor is the desktop sandbox's (`scripts/cua-sandbox.sh`): a
//! headless sway on its own bus whose seat has a virtual pointer and no
//! keyboard. The pointer is sway's own (wlr virtual pointer), so input
//! takes the path a mouse does; what the application holds is read through
//! its inspector (`--mcp`), over `ljos attach`.
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

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use ljos_bench::sway::Sway;
use ljos_bench::wayland::Pointer;
use serde_json::{Value, json};

use super::inspector::{Answer, Ask, Refusal, Seen};
use super::{
    A11yNode, Backend, BackendError, Files, LogEntry, LogLevel, PointerStep, Probes, QmlFile,
    Result, Tooling,
};

/// Names the file with the sandbox's sockets; set when one runs.
pub const ENV: &str = "LJOS_BDD_DESKTOP";
/// Why there is no sandbox, when the task that looked for one says; empty
/// when it did not look, as none of the files it runs has a `@desktop`
/// scenario.
pub const SKIP_ENV: &str = "LJOS_BDD_DESKTOP_SKIP";

/// How long the application may take to show its window.
const STARTUP: Duration = Duration::from_secs(60);
/// The longest the application may take to see input, or to come to rest
/// before what one reading of it costs is measured.
const SEEN: Duration = Duration::from_secs(10);
/// How long a watch gathers the rests of a busy application: at most ten
/// watches a second, half of what the inspector answers.
const BURST_MS: u64 = 100;
/// The most calls sent in any second: the inspector answers twenty and
/// refuses the rest for a second; two spare cover the time to deliver one.
const CALLS_PER_SECOND: usize = 18;
/// How long the compositor's picture of a window at rest must show no
/// change before it is taken: the application has presented its last
/// frame, and the compositor shows it within a frame or two of its own.
const STILL: Duration = Duration::from_millis(100);
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

/// The running application and the ways in to it.
#[derive(Debug)]
struct Running {
    app: Child,
    attach: Child,
    to_app: ChildStdin,
    from_app: BufReader<ChildStdout>,
    next_id: u64,
    /// When the calls of the last second were sent.
    sent: VecDeque<Instant>,
    /// Where its window's content is, in the layout's pixels.
    rect: (i32, i32, i32, i32),
}

impl Running {
    /// Talks to the inspector of `app` through `ljos attach`.
    fn attach(ljos: &Path, token: &Path, app: Child, rect: (i32, i32, i32, i32)) -> Result<Self> {
        let mut attach = Command::new(ljos)
            .args(["attach".as_ref(), token.as_os_str()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let to_app = attach.stdin.take().expect("piped");
        let from_app = BufReader::new(attach.stdout.take().expect("piped"));
        Ok(Running {
            app,
            attach,
            to_app,
            from_app,
            next_id: 0,
            sent: VecDeque::new(),
            rect,
        })
    }

    /// What the process has done so far, by kind: its window's passes and
    /// frames, its jobs and messages (`app_metrics`), and what was sent to
    /// each of its mailboxes (`app_processes`).
    fn work(&mut self) -> Result<BTreeMap<String, u64>> {
        let metrics = self.ask("app_metrics", json!({}))?;
        let n = |v: &Value| v.as_u64().unwrap_or(0);
        let paint = &metrics["paint"];
        let mut work = BTreeMap::from([
            ("frames".to_owned(), n(&metrics["frames"]["count"])),
            (
                "passes".to_owned(),
                n(&paint["presented"]) + n(&paint["unchanged"]),
            ),
            ("jobs".to_owned(), n(&metrics["jobs"]["polled"])),
            ("messages".to_owned(), n(&metrics["messages"]["drained"])),
        ]);
        let processes = self.ask("app_processes", json!({"kind": "mailbox"}))?;
        for mailbox in processes["mailboxes"].as_array().into_iter().flatten() {
            let name = mailbox["name"].as_str().unwrap_or("?");
            work.insert(format!("sent to {name}"), n(&mailbox["sent"]));
        }
        Ok(work)
    }

    fn ask(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let answer = self.call(tool, arguments)?;
        match answer.refusal {
            Some(r) => Err(BackendError::Failed(format!("{tool}: {}", r.message))),
            None => Ok(answer.structured),
        }
    }

    /// Waits until a call more keeps to the inspector's calls per second.
    /// Steps that no longer wait for a while send them faster than that.
    fn pace(&mut self) {
        let second = Duration::from_secs(1);
        if self.sent.len() >= CALLS_PER_SECOND
            && let Some(first) = self.sent.pop_front()
        {
            std::thread::sleep((first + second).saturating_duration_since(Instant::now()));
        }
        let now = Instant::now();
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= second)
        {
            self.sent.pop_front();
        }
        self.sent.push_back(now);
    }

    /// A tool's answer as an agent sees it, a refusal among them.
    fn call(&mut self, tool: &str, arguments: Value) -> Result<Answer> {
        self.pace();
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments},
        });
        writeln!(self.to_app, "{message}")?;
        self.to_app.flush()?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.from_app.read_line(&mut line)? == 0 {
                return Err(BackendError::Failed(
                    "the application's inspector went away".into(),
                ));
            }
            let answer: Value = serde_json::from_str(&line)
                .map_err(|e| BackendError::Failed(format!("the inspector said {line:?}: {e}")))?;
            if answer["id"] != id {
                continue;
            }
            if let Some(error) = answer.get("error") {
                return Err(BackendError::Failed(format!(
                    "{tool}: {}",
                    error["message"]
                )));
            }
            let result = &answer["result"];
            let text = result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if result["isError"] == true {
                let error = &result["structuredContent"]["error"];
                return Ok(Answer {
                    refusal: Some(Refusal {
                        code: error["code"].as_str().unwrap_or_default().to_owned(),
                        message: error["message"].as_str().unwrap_or(&text).to_owned(),
                        details: error["details"].clone(),
                    }),
                    text,
                    ..Answer::default()
                });
            }
            let picture = result["content"].as_array().and_then(|content| {
                content
                    .iter()
                    .find(|c| c["type"] == "image")
                    .and_then(|c| ljos_mcp_core::base64_decode(c["data"].as_str()?))
            });
            return Ok(Answer {
                structured: result["structuredContent"].clone(),
                text,
                picture,
                ..Answer::default()
            });
        }
    }

    /// The UI thread's last rest (`app_watch`'s `rest`), and the count of
    /// rests to wait from.
    fn rest(&mut self) -> Result<(u64, Value)> {
        let told = self.ask("app_watch", json!({"topics": ["rests"]}))?;
        Ok((
            told["counts"]["rests"].as_u64().unwrap_or(0),
            told["rest"].clone(),
        ))
    }

    /// Waits until the UI thread rests as `done` wants, or until
    /// `deadline`: whether it did. The watch waits on the inspector's
    /// thread, so the application runs no pass for it.
    fn rest_until(&mut self, deadline: Instant, done: impl Fn(&Value) -> bool) -> Result<bool> {
        let (mut count, mut rest) = self.rest()?;
        let mut settle_ms = 0;
        loop {
            if done(&rest) {
                return Ok(true);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            let wait_ms = u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1);
            let told = self.ask(
                "app_watch",
                json!({
                    "since": {"rests": count}, "topics": ["rests"],
                    "wait_ms": wait_ms, "settle_ms": settle_ms,
                }),
            )?;
            // An animating application rests every frame. Once a rest has
            // not been the one wanted, the rests of a burst are gathered so
            // the watches stay under the inspector's calls per second.
            settle_ms = BURST_MS;
            count = told["counts"]["rests"].as_u64().unwrap_or(count);
            rest = told["rest"].clone();
        }
    }

    /// Waits until no pass of the UI thread is due: what was asked of it
    /// has run, and what that asked for after.
    fn settle(&mut self) -> Result<()> {
        self.rest_until(Instant::now() + SEEN, |rest| {
            rest["wake_in_ms"].as_u64() != Some(0)
        })?;
        // An application that keeps animating never rests; what it costs
        // is then measured as it goes.
        Ok(())
    }

    /// Waits until the application rests with its window's last frame
    /// presented: no pass due, and none owed to the window. Under load
    /// the window system lets a window draw late, and the thread rests
    /// meanwhile with only that stall's wake due, so a rest alone can
    /// come before the frame (`features/window/cpu-drawing.feature`).
    fn presented(&mut self, deadline: Instant) -> Result<()> {
        self.rest_until(deadline, |rest| {
            rest["wake_in_ms"].as_u64() != Some(0) && rest["frame_owed"] == Value::Bool(false)
        })?;
        // A window that keeps drawing owes a frame at every rest; the
        // compositor's picture is then never still, which `grab` says.
        Ok(())
    }

    fn eval(&mut self, object: &str, source: &str) -> Result<Value> {
        let answer = self.ask(
            "app_eval",
            json!({"source": source, "target": format!("#{object}")}),
        )?;
        Ok(answer["value"].clone())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // Only the two processes this backend started.
        for child in [&mut self.attach, &mut self.app] {
            let _ = child.kill();
            let _ = child.wait();
        }
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

    fn ljos() -> Result<PathBuf> {
        let bin = crate::runner::target_dir().join("debug/ljos");
        if bin.is_file() {
            Ok(bin)
        } else {
            Err(BackendError::Failed(format!(
                "{} is not built (cargo build -p ljos_cli)",
                bin.display()
            )))
        }
    }

    fn start(&mut self, entry: &str) -> Result<()> {
        self.seen = Seen::default();
        self.passes_from = None;
        self.wakes_from = None;
        self.work_from.clear();
        self.observer = None;
        self.running = None;
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
            // A Wayland client of the sandbox's compositor, not an X11 one
            // of its XWayland, and nothing of the developer's session.
            .env_remove("DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("AT_SPI_BUS_ADDRESS")
            .envs(sandbox.env.iter().filter(|(k, _)| *k != "DISPLAY"))
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
        let mut app = command.spawn()?;
        let pid = app.id();
        let token = dir.join(format!("target/ljos/mcp-{pid}.json"));
        let sway = Sway::new(&sandbox.path("SWAYSOCK")?);
        let deadline = Instant::now() + STARTUP;
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
                && let Some(rect) = sway.window_rect(pid)?
                && rect.2 > 0
            {
                break rect;
            }
            if Instant::now() > deadline {
                let _ = app.kill();
                let _ = app.wait();
                return Err(BackendError::Failed(format!(
                    "ljos run showed no window in {} s",
                    STARTUP.as_secs()
                )));
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
        let mut observer = Command::new(&ljos)
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
            )?)
            .spawn()?;
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
                let _ = observer.kill();
                let _ = observer.wait();
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
        match (paint["presented"].as_u64(), paint["unchanged"].as_u64()) {
            (Some(presented), Some(unchanged)) => Ok(presented + unchanged),
            _ => Err(BackendError::Failed(format!(
                "app_metrics says no passes: {metrics}"
            ))),
        }
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
            what: "keys: the sandbox's seat has no keyboard",
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
        let rect = self.running()?.rect;
        let display = self
            .sandbox
            .as_ref()
            .ok_or_else(|| BackendError::Failed("no sandbox".into()))?
            .path("WAYLAND_DISPLAY")?;
        let failed = |e: ljos_bench::wayland::Error| {
            BackendError::Failed(format!("the sandbox's screencopy: {e}"))
        };
        ljos_bench::wayland::quiet(&display, rect, STILL, deadline)
            .map_err(failed)?
            .ok_or_else(|| {
                BackendError::Failed(format!(
                    "the window kept changing for {} s",
                    PICTURE.as_secs()
                ))
            })?;
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

    fn as_probes(&mut self) -> Option<&mut dyn Probes> {
        Some(self)
    }

    fn as_tooling(&mut self) -> Option<&mut dyn Tooling> {
        Some(self)
    }
}

impl Files for DesktopBackend {
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir()?.join(name);
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

impl Probes for DesktopBackend {
    fn draw_on_processor(&mut self) -> Result<()> {
        self.env.push(("LJOS_PAINTER".to_owned(), "cpu".to_owned()));
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
        let answer = observer.ask(tool, arguments)?;
        // What the call woke is done before the next step looks.
        observer.settle()?;
        self.running()?.settle()?;
        Ok(answer)
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
