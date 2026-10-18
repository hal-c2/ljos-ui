//! One application on one runtime under one profile: started through the
//! gate inside a limit, watched until its first frame, left to settle and
//! idle, driven, and stopped.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    cgroup::{self, Cgroup},
    gate::Gate,
    procfs,
    profile::{Profile, cpu_set},
    sway::{Mapped, Sway},
    wayland::{self, FrameCounter, Pointer},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The application was not inside the limit asked for, so nothing was
    /// measured.
    #[error("refused: {0}")]
    Refused(String),
    #[error("cannot start {0}")]
    Launch(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// How an application is started on one runtime.
#[derive(Clone, Debug)]
pub struct Runtime {
    /// `ljos` or `qt`.
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Set for the application only, by the gate, so the launcher keeps
    /// the session it needs to reach the service manager.
    pub env: Vec<(String, String)>,
    pub unset: Vec<String>,
}

/// What every run shares.
#[derive(Debug)]
pub struct Setup {
    /// `systemd-run`, or a stand-in with its arguments.
    pub launcher: PathBuf,
    pub cgroup_root: PathBuf,
    /// The program that is the gate (`ljos`), given `bench-gate`.
    pub gate: PathBuf,
    /// Where FIFOs and the applications' logs go.
    pub work: PathBuf,
    pub sway: Option<Sway>,
    /// The compositor's Wayland socket, for the pointer and frame counting.
    pub wayland: Option<PathBuf>,
    pub startup_timeout: Duration,
    pub isolation_timeout: Duration,
}

/// How long each phase lasts.
#[derive(Clone, Copy, Debug)]
pub struct Phases {
    pub settle: Duration,
    pub idle: Duration,
    pub load: Duration,
    /// What the application is driven with under load.
    pub gesture: Gesture,
    /// Read `/proc` for resident memory and mapped files.
    pub detail: bool,
}

/// What drives an application under load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gesture {
    /// Clicks on a grid over the window, as a user going through it would.
    #[default]
    Click,
    /// The left button held in the middle of the window and moved up and
    /// down, which flicks a list or drags what is there.
    Drag,
    /// The wheel turned down and up again in the middle of the window.
    Scroll,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// It showed a frame and was still running when it was stopped.
    Ok,
    /// No frame was seen in time; it may still have been running.
    NoFrame,
    /// The kernel killed it for want of memory.
    OomKilled,
    /// It quit, or died of something else.
    Exited,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LimitsOut {
    pub memory_max: Option<u64>,
    pub cpu_quota_percent: Option<u32>,
    pub cpus: Option<String>,
    pub cpu_factor: Option<f64>,
}

/// Memory once the application has settled.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settled {
    pub memory_current: Option<u64>,
    pub memory_peak: Option<u64>,
    /// `memory.stat`: anonymous memory, page cache charged here, and
    /// kernel memory (page tables, stacks, slab).
    pub anon: Option<u64>,
    pub file: Option<u64>,
    pub kernel: Option<u64>,
    #[serde(flatten)]
    pub resident: Option<procfs::Resident>,
    pub threads: Option<u64>,
}

/// Frames the compositor showed in a window of time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frames {
    pub count: usize,
    pub fps: f64,
    /// Intervals between frames, when there were two or more.
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// From each input the bench sent to the first frame shown after it,
    /// when it drove the window: how long a person waits to see a drag or
    /// a wheel answered (bounded below by the compositor's refresh).
    #[serde(default)]
    pub latency_p50_ms: Option<f64>,
    #[serde(default)]
    pub latency_p95_ms: Option<f64>,
    #[serde(default)]
    pub latency_max_ms: Option<f64>,
}

/// CPU over a window of time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    pub seconds: f64,
    pub cpu_ms: f64,
    /// Of one CPU of this machine.
    pub cpu_percent: f64,
    /// Time the quota held it back.
    pub throttled_ms: Option<f64>,
    /// Times a thread slept and was woken, a second.
    pub wakeups_per_s: f64,
    /// The threads that woke, by name, and how often: what wakes an
    /// application that should sleep.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub woken: BTreeMap<String, u64>,
    /// Of `cpu_ms`, what threads that ended in the window used: short
    /// workers whose wakeups `woken` cannot count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_cpu_ms: Option<f64>,
    pub frames: Option<Frames>,
    /// Clicks, moves or wheel notches given, for a driven window.
    pub clicks: Option<u32>,
}

/// What of an install the running application had mapped.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Install {
    pub runtime_bytes: Option<u64>,
    /// By part (`runtime`, `qt`, `graphics`, `fonts`, `system`, `data`):
    /// files, their bytes on disk and their resident bytes.
    pub groups: BTreeMap<String, Group>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Group {
    pub files: usize,
    pub bytes: u64,
    pub rss: u64,
}

/// One application, one runtime, one profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub app: String,
    pub runtime: String,
    pub profile: String,
    pub limits: LimitsOut,
    /// Which of the repeated runs this is, from 0.
    pub repeat: u32,
    /// The machine's one-minute load average when the run started: other
    /// work on it slows every timing.
    pub load_average: Option<f64>,
    pub outcome: Outcome,
    pub oom_killed: bool,
    /// How it ended when it ended by itself.
    pub exit: Option<String>,
    /// From the gate opening to the compositor mapping its window.
    pub first_frame_ms: Option<f64>,
    /// From the gate opening to the first frame drawn in answer to the
    /// compositor once the window was mapped: a resize asked for at the
    /// map, seen on screen. Input reaches an application the same way, so
    /// until then a click would have waited too.
    #[serde(default)]
    pub input_ready_ms: Option<f64>,
    /// CPU used by then.
    pub startup_cpu_ms: Option<f64>,
    /// The most memory the cgroup held by then.
    pub peak_memory: Option<u64>,
    pub settled: Option<Settled>,
    pub idle: Option<Span>,
    pub load: Option<Span>,
    /// What drove it under load.
    #[serde(default)]
    pub gesture: Gesture,
    pub install: Option<Install>,
    pub log: String,
}

#[allow(clippy::cast_precision_loss)]
fn ms(usec: u64) -> f64 {
    usec as f64 / 1000.0
}

/// The one-minute load average.
#[must_use]
pub fn load_average() -> Option<f64> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Checks that `cg` is a cgroup of its own with the profile's limits. A
/// launcher that only starts the command leaves it in the bench's cgroup,
/// which holds the bench too.
fn isolated(cg: &Path, pid: u32, profile: &Profile) -> Result<(), String> {
    if !cg.is_dir() {
        return Err(format!("its cgroup {} does not exist", cg.display()));
    }
    let c = Cgroup { dir: cg.to_owned() };
    let unreadable = |file: &str| format!("{}/{file} cannot be read", cg.display());
    let procs = c.procs().ok_or_else(|| unreadable("cgroup.procs"))?;
    if procs != [pid] {
        return Err(format!("its cgroup holds {procs:?}, not only {pid}"));
    }
    let l = &profile.limits;
    if let Some(bytes) = l.memory_max {
        let have = c.memory_max().ok_or_else(|| unreadable("memory.max"))?;
        if have != bytes.to_string() {
            return Err(format!("memory.max is {have}, not {bytes}"));
        }
    }
    if l.no_swap {
        let have = c.swap_max().ok_or_else(|| unreadable("memory.swap.max"))?;
        if have != "0" {
            return Err(format!("memory.swap.max is {have}, not 0"));
        }
    }
    if let Some((quota, period)) = profile.cpu_max() {
        let have = c.cpu_max().ok_or_else(|| unreadable("cpu.max"))?;
        if have != format!("{quota} {period}") {
            return Err(format!("cpu.max is {have}, not {quota} {period}"));
        }
    }
    if let Some(want) = &l.cpus {
        let have = c
            .cpus()
            .ok_or_else(|| unreadable("cpuset.cpus.effective"))?;
        if cpu_set(&have) != cpu_set(want) {
            return Err(format!("cpuset.cpus.effective is {have}, not {want}"));
        }
    }
    Ok(())
}

fn signal(pid: u32, sig: i32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: kill(2) on our own child's pid; it cannot be reused while
        // the child is not waited for.
        unsafe {
            libc::kill(pid, sig);
        }
    }
}

/// Ends the child: asked first, then made.
fn stop(child: &mut Child) -> Option<ExitStatus> {
    if let Ok(Some(status)) = child.try_wait() {
        return Some(status);
    }
    signal(child.id(), libc::SIGTERM);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

fn describe(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("status {code}"),
        (None, Some(sig)) => format!("signal {sig}"),
        _ => "unknown".to_owned(),
    }
}

/// The systemd unit a scope's cgroup is, when the launcher is systemd's.
fn unit(setup: &Setup, cg: &Path) -> Option<String> {
    let name = setup.launcher.file_name()?.to_str()?;
    (name == "systemd-run")
        .then(|| cg.file_name()?.to_str().map(str::to_owned))
        .flatten()
}

/// Whether systemd says the unit's process was killed for want of memory.
/// The kernel's count goes with the cgroup once its last process is gone.
fn systemd_oom(unit: &str) -> bool {
    let out = Command::new("systemctl")
        .args(["--user", "show", unit, "-p", "Result", "--value"])
        .stderr(Stdio::null())
        .output();
    let oom = out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "oom-kill");
    let _ = Command::new("systemctl")
        .args(["--user", "reset-failed", unit])
        .stderr(Stdio::null())
        .status();
    oom
}

/// What the run has seen while waiting.
struct Watch<'a> {
    child: &'a mut Child,
    cg: Cgroup,
    oom: u64,
    peak: Option<u64>,
    exited: Option<ExitStatus>,
}

impl Watch<'_> {
    fn sample(&mut self) {
        if let Some(n) = self.cg.oom_kills() {
            self.oom = self.oom.max(n);
        }
        if let Some(p) = self.cg.peak() {
            self.peak = Some(self.peak.map_or(p, |q| q.max(p)));
        }
        if self.exited.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.exited = Some(status);
        }
    }

    /// Waits `time`, still watching. False when the application ended.
    fn wait(&mut self, time: Duration) -> bool {
        let end = Instant::now() + time;
        loop {
            self.sample();
            if self.exited.is_some() {
                return false;
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            thread::sleep(left.min(Duration::from_millis(50)));
        }
    }
}

/// The value at quantile `q` of sorted `values`.
fn quantile(values: &[f64], q: f64) -> Option<f64> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((values.len().checked_sub(1)?) as f64 * q).round() as usize;
    values.get(i).copied()
}

fn frames(times: &[u64], seconds: f64, inputs: &[u64]) -> Frames {
    #[allow(clippy::cast_precision_loss)]
    let mut gaps: Vec<f64> = times
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]) as f64 / 1e6)
        .collect();
    gaps.sort_by(f64::total_cmp);
    // An input after the last frame was never answered while counting.
    #[allow(clippy::cast_precision_loss)]
    let mut waits: Vec<f64> = inputs
        .iter()
        .filter_map(|&sent| {
            let i = times.partition_point(|&t| t <= sent);
            times.get(i).map(|&t| (t - sent) as f64 / 1e6)
        })
        .collect();
    waits.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let fps = if seconds > 0.0 {
        times.len() as f64 / seconds
    } else {
        0.0
    };
    Frames {
        count: times.len(),
        fps,
        p50_ms: quantile(&gaps, 0.5),
        p95_ms: quantile(&gaps, 0.95),
        max_ms: gaps.last().copied(),
        latency_p50_ms: quantile(&waits, 0.5),
        latency_p95_ms: quantile(&waits, 0.95),
        latency_max_ms: waits.last().copied(),
    }
}

/// Measures a window of time: CPU, wakeups and, when the compositor can
/// be asked, frames; `drive` is the window `gesture` drives meanwhile.
fn span(
    setup: &Setup,
    w: &mut Watch<'_>,
    pid: u32,
    time: Duration,
    gesture: Gesture,
    drive: Option<(i32, i32, i32, i32)>,
    region: Option<(i32, i32, i32, i32)>,
) -> Option<Span> {
    let counter = match (&setup.wayland, region) {
        (Some(display), Some(r)) => FrameCounter::start(display, r).ok(),
        _ => None,
    };
    let cpu0 = w.cg.cpu_usec()?;
    let thr0 = w.cg.throttled_usec();
    let sw0 = procfs::thread_switches(pid);
    let ns0 = procfs::thread_cpu_ns(pid);
    let start = Instant::now();
    let mut clicks = None;
    let mut inputs = Vec::new();
    match (drive, &setup.wayland, &setup.sway) {
        (Some(rect), Some(display), Some(sway)) => {
            let (n, sent) = match gesture {
                Gesture::Click => click_grid(display, sway, rect, time, w),
                Gesture::Drag => drag(display, sway, rect, time, w),
                Gesture::Scroll => scroll(display, sway, rect, time, w),
            };
            clicks = Some(n);
            inputs = sent;
        }
        _ => {
            w.wait(time);
        }
    }
    let seconds = start.elapsed().as_secs_f64();
    let cpu1 = w.cg.cpu_usec().unwrap_or(cpu0);
    let sw1 = procfs::thread_switches(pid);
    let woken = procfs::woken(&sw0, &sw1);
    let live_ms = procfs::live_cpu_ns(&ns0, &procfs::thread_cpu_ns(pid)) as f64 / 1e6;
    let times = counter.and_then(|c| c.finish().ok());
    let cpu_ms = ms(cpu1.saturating_sub(cpu0));
    #[allow(clippy::cast_precision_loss)]
    Some(Span {
        seconds,
        cpu_ms,
        cpu_percent: if seconds > 0.0 {
            cpu_ms / 10.0 / seconds
        } else {
            0.0
        },
        throttled_ms: thr0
            .zip(w.cg.throttled_usec())
            .map(|(a, b)| ms(b.saturating_sub(a))),
        wakeups_per_s: if seconds > 0.0 {
            woken.values().sum::<u64>() as f64 / seconds
        } else {
            0.0
        },
        frames: times.map(|t| frames(&t, seconds, &inputs)),
        clicks,
        woken,
        // Rounding between the two clocks is not an ended thread.
        ended_cpu_ms: Some((cpu_ms - live_ms).max(0.0)).filter(|ms| *ms >= 1.0),
    })
}

/// Clicks a 5 × 5 grid over the window, evenly over `time`.
fn click_grid(
    display: &Path,
    sway: &Sway,
    (x, y, width, height): (i32, i32, i32, i32),
    time: Duration,
    w: &mut Watch<'_>,
) -> (u32, Vec<u64>) {
    let Ok(mut pointer) = Pointer::new(display) else {
        w.wait(time);
        return (0, Vec::new());
    };
    let extent = sway
        .layout()
        .ok()
        .flatten()
        .unwrap_or((x + width, y + height));
    let step = time / 25;
    let mut clicks = 0;
    for row in 0..5 {
        for col in 0..5 {
            let px = x + width * (2 * col + 1) / 10;
            let py = y + height * (2 * row + 1) / 10;
            let (Ok(px), Ok(py), Ok(ew), Ok(eh)) = (
                u32::try_from(px),
                u32::try_from(py),
                u32::try_from(extent.0),
                u32::try_from(extent.1),
            ) else {
                continue;
            };
            if pointer.click(px, py, (ew, eh)).is_ok() {
                clicks += 1;
            }
            if !w.wait(step) {
                return (clicks, pointer.sent().to_vec());
            }
        }
    }
    (clicks, pointer.sent().to_vec())
}

/// The size of the compositor's layout, which pointer positions are in,
/// and the point `(x, y)` in it.
fn on_layout(
    sway: &Sway,
    rect: (i32, i32, i32, i32),
    x: i32,
    y: i32,
) -> Option<(u32, u32, (u32, u32))> {
    let extent = sway
        .layout()
        .ok()
        .flatten()
        .unwrap_or((rect.0 + rect.2, rect.1 + rect.3));
    Some((
        u32::try_from(x).ok()?,
        u32::try_from(y).ok()?,
        (u32::try_from(extent.0).ok()?, u32::try_from(extent.1).ok()?),
    ))
}

/// Presses in the middle of the window and moves up and down a third of
/// its height each way, once a second, a move every 16 ms, over `time`.
fn drag(
    display: &Path,
    sway: &Sway,
    rect @ (x, y, width, height): (i32, i32, i32, i32),
    time: Duration,
    w: &mut Watch<'_>,
) -> (u32, Vec<u64>) {
    let Ok(mut pointer) = Pointer::new(display) else {
        w.wait(time);
        return (0, Vec::new());
    };
    let (cx, cy, reach) = (x + width / 2, y + height / 2, height / 3);
    let start = Instant::now();
    let mut moves = 0;
    let mut held = false;
    while start.elapsed() < time {
        // A triangle wave from -1 to 1 and back.
        let t = start.elapsed().as_secs_f64().fract();
        let wave = if t < 0.5 {
            4.0 * t - 1.0
        } else {
            3.0 - 4.0 * t
        };
        #[allow(clippy::cast_possible_truncation)]
        let py = cy + (f64::from(reach) * wave).round() as i32;
        let Some((px, py, extent)) = on_layout(sway, rect, cx, py) else {
            break;
        };
        let sent = if held {
            pointer.move_to(px, py, extent)
        } else {
            pointer.press(px, py, extent)
        };
        if sent.is_err() {
            break;
        }
        held = true;
        moves += 1;
        if !w.wait(Duration::from_millis(16)) {
            break;
        }
    }
    if held {
        let _ = pointer.release();
    }
    (moves, pointer.sent().to_vec())
}

/// Turns the wheel in the middle of the window, ten notches down and ten
/// up, a notch every 50 ms, over `time`.
fn scroll(
    display: &Path,
    sway: &Sway,
    rect @ (x, y, width, height): (i32, i32, i32, i32),
    time: Duration,
    w: &mut Watch<'_>,
) -> (u32, Vec<u64>) {
    let Ok(mut pointer) = Pointer::new(display) else {
        w.wait(time);
        return (0, Vec::new());
    };
    let Some((px, py, extent)) = on_layout(sway, rect, x + width / 2, y + height / 2) else {
        w.wait(time);
        return (0, Vec::new());
    };
    let start = Instant::now();
    let mut notches = 0;
    while start.elapsed() < time {
        let steps = if (notches / 10) % 2 == 0 { 1 } else { -1 };
        if pointer.wheel(px, py, extent, steps).is_err() {
            break;
        }
        notches += 1;
        if !w.wait(Duration::from_millis(50)) {
            break;
        }
    }
    (notches, pointer.sent().to_vec())
}

fn install(pid: u32) -> Install {
    let runtime = procfs::exe(pid)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut out = Install {
        runtime_bytes: std::fs::metadata(&runtime).ok().map(|m| m.len()),
        groups: BTreeMap::new(),
    };
    for m in procfs::mapped_files(pid) {
        let g = out
            .groups
            .entry(procfs::group(&m.path, &runtime).to_owned())
            .or_default();
        g.files += 1;
        g.bytes += m.bytes;
        g.rss += m.rss;
    }
    out
}

/// Waits for the window of `pid` among those mapped.
fn first_frame(
    windows: &Receiver<Mapped>,
    pid: u32,
    w: &mut Watch<'_>,
    deadline: Instant,
) -> Option<Instant> {
    loop {
        w.sample();
        if w.exited.is_some() {
            return None;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        match windows.recv_timeout(left.min(Duration::from_millis(20))) {
            Ok(m) if m.pid == pid => return Some(m.at),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// Asks the compositor to make the window of `pid` wider and waits for
/// the application to fill what it grew by; then asks for its size back,
/// so what is measured afterwards sees the window it asked for.
///
/// sway keeps a floating window's centre and says at once that it has the
/// new size, so the frame is looked for: until the application commits one
/// of the new width, a strip just past its old right edge shows the
/// background.
fn answered(
    sway: &Sway,
    display: &Path,
    pid: u32,
    w: &mut Watch<'_>,
    deadline: Instant,
) -> Option<Instant> {
    let (x, y, width, height) = sway.window_rect(pid).ok().flatten()?;
    let resize = |w: i32| format!("[pid={pid}] resize set width {w} px height {height} px");
    let strip = (x + width + 8, y + height / 4, 16, (height / 2).max(1));
    let at = wayland::first_change(display, strip, deadline, || {
        let _ = sway.command(&resize(width + 64));
    });
    let _ = sway.command(&resize(width));
    w.sample();
    at.ok().flatten().filter(|_| w.exited.is_none())
}

/// Runs one application on one runtime under one profile.
///
/// # Errors
/// The application was not inside the limit ([`Error::Refused`]) at any of
/// [`LAUNCHES`] launches, or could not be started.
pub fn run(
    setup: &Setup,
    app: &str,
    runtime: &Runtime,
    profile: &Profile,
    phases: Phases,
    drive: bool,
) -> Result<Run, Error> {
    let mut launch = 1;
    loop {
        match run_once(setup, app, runtime, profile, phases, drive) {
            // A refused application was stopped before its gate opened, so
            // it ran nothing and another launch measures as the first would.
            Err(Error::Refused(why)) if launch < LAUNCHES => {
                eprintln!("ljos bench: launching again: {why}");
                launch += 1;
            }
            other => return other,
        }
    }
}

/// How many times a launch that was not isolated is tried. On a busy
/// machine a launch now and then came out of the launcher in the bench's
/// cgroup; a launcher that never isolates is refused each time.
pub const LAUNCHES: u32 = 3;

#[allow(clippy::too_many_lines)]
fn run_once(
    setup: &Setup,
    app: &str,
    runtime: &Runtime,
    profile: &Profile,
    phases: Phases,
    drive: bool,
) -> Result<Run, Error> {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::fs::create_dir_all(&setup.work)?;
    let mut gate = Gate::new(&setup.work.join(format!("gate-{}-{n}", std::process::id())))?;
    let log_path = setup
        .work
        .join(format!("{app}.{}.{}.log", runtime.name, profile.name));
    let log = File::create(&log_path)?;
    let windows = match &setup.sway {
        Some(sway) => Some(sway.windows()?),
        None => None,
    };

    let mut command = Command::new(&setup.launcher);
    command
        .args(profile.launcher_args())
        .arg("--")
        .arg(&setup.gate)
        .arg("bench-gate")
        .arg("--fifo")
        .arg(gate.path());
    for (k, v) in &runtime.env {
        command.arg("--env").arg(format!("{k}={v}"));
    }
    for k in &runtime.unset {
        command.arg("--unset").arg(k);
    }
    command
        .arg("--")
        .arg(&runtime.program)
        .args(&runtime.args)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let mut child = command
        .spawn()
        .map_err(|e| Error::Launch(format!("{}: {e}", setup.launcher.display())))?;
    let pid = child.id();
    let gate_exe = std::fs::canonicalize(&setup.gate).unwrap_or_else(|_| setup.gate.clone());
    let what = format!("{app} on {}", runtime.name);

    // Not one instruction of the application runs until its cgroup is the
    // one asked for.
    let launched = Instant::now();
    let deadline = launched + setup.isolation_timeout;
    let cg = loop {
        if let Ok(Some(status)) = child.try_wait() {
            let said = std::fs::read_to_string(&log_path).unwrap_or_default();
            return Err(Error::Launch(format!(
                "{what}: the launcher ended ({}) before the application started: {}",
                describe(status),
                said.trim()
            )));
        }
        // The gate first: the launcher moves itself before it execs the
        // gate, so a cgroup read after the gate is seen is the final one.
        // The gate is this program, and so is the child between its fork
        // and the launcher's exec; its own fifo tells the gate apart.
        let handed = procfs::exe(pid).is_some_and(|e| e == gate_exe)
            && procfs::has_arg(pid, gate.path().as_os_str());
        let cg = cgroup::of_pid(&setup.cgroup_root, pid)?;
        match isolated(&cg, pid, profile) {
            Ok(()) => break cg,
            Err(reason) if handed || Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Refused(format!(
                    "{what} is not inside the limit of {} {} ms after its launch{}: {reason}",
                    profile.name,
                    launched.elapsed().as_millis(),
                    if handed { ", in the gate" } else { "" },
                )));
            }
            Err(_) => thread::sleep(Duration::from_millis(5)),
        }
    };
    let unit = unit(setup, &cg);
    let mut w = Watch {
        child: &mut child,
        cg: Cgroup { dir: cg.clone() },
        oom: 0,
        peak: None,
        exited: None,
    };
    let cpu0 = w.cg.cpu_usec().unwrap_or(0);
    let t0 = Instant::now();
    gate.open()?;

    let deadline = t0 + setup.startup_timeout;
    let shown = match &windows {
        Some(rx) => first_frame(rx, pid, &mut w, deadline),
        None => {
            w.wait(setup.startup_timeout);
            None
        }
    };
    let startup_cpu = w.cg.cpu_usec();
    let ready = match (&setup.sway, &setup.wayland, shown) {
        (Some(sway), Some(display), Some(_)) => answered(sway, display, pid, &mut w, deadline),
        _ => None,
    };
    w.sample();
    let mut out = Run {
        app: app.to_owned(),
        runtime: runtime.name.clone(),
        profile: profile.name.clone(),
        limits: LimitsOut {
            memory_max: profile.limits.memory_max,
            cpu_quota_percent: profile.limits.cpu_quota_percent,
            cpus: profile.limits.cpus.clone(),
            cpu_factor: profile.limits.cpu_factor,
        },
        repeat: 0,
        load_average: load_average(),
        outcome: if shown.is_some() {
            Outcome::Ok
        } else {
            Outcome::NoFrame
        },
        oom_killed: false,
        exit: None,
        first_frame_ms: shown.map(|at| at.duration_since(t0).as_secs_f64() * 1000.0),
        input_ready_ms: ready.map(|at| at.duration_since(t0).as_secs_f64() * 1000.0),
        startup_cpu_ms: startup_cpu.map(|c| ms(c.saturating_sub(cpu0))),
        peak_memory: w.cg.peak().or(w.peak),
        settled: None,
        idle: None,
        load: None,
        gesture: phases.gesture,
        install: None,
        log: log_path.display().to_string(),
    };

    if w.exited.is_none() && w.wait(phases.settle) {
        let resident = phases.detail.then(|| procfs::resident(pid).ok()).flatten();
        out.settled = Some(Settled {
            memory_current: w.cg.current(),
            memory_peak: w.cg.peak(),
            anon: w.cg.stat("anon"),
            file: w.cg.stat("file"),
            kernel: w.cg.stat("kernel"),
            resident,
            threads: procfs::threads(pid),
        });
        let rect = match (&setup.sway, shown) {
            (Some(sway), Some(_)) => sway.window_rect(pid).ok().flatten(),
            _ => None,
        };
        if !phases.idle.is_zero() {
            out.idle = span(setup, &mut w, pid, phases.idle, phases.gesture, None, rect);
        }
        if !phases.load.is_zero() && w.exited.is_none() {
            out.load = span(
                setup,
                &mut w,
                pid,
                phases.load,
                phases.gesture,
                rect.filter(|_| drive),
                rect,
            );
        }
        if phases.detail && w.exited.is_none() {
            out.install = Some(install(pid));
        }
    }

    w.sample();
    let ended = w.exited.or_else(|| stop(w.child));
    w.sample();
    if let Some(status) = ended {
        out.exit = Some(describe(status));
        use std::os::unix::process::ExitStatusExt;
        let killed = status.signal() == Some(libc::SIGKILL);
        let oom = w.oom > 0 || (killed && unit.as_deref().is_some_and(systemd_oom));
        out.oom_killed = oom;
        out.outcome = if oom {
            Outcome::OomKilled
        } else {
            Outcome::Exited
        };
    } else {
        // Stopped by us: a kill for memory may still have happened before.
        out.oom_killed = w.oom > 0;
        if out.oom_killed {
            out.outcome = Outcome::OomKilled;
        }
        if let Some(unit) = &unit {
            let _ = Command::new("systemctl")
                .args(["--user", "reset-failed", unit])
                .stderr(Stdio::null())
                .status();
        }
    }
    Ok(out)
}

/// The least memory, in MiB, an application starts in under a profile:
/// its first frame shown and still running a second later, with no kill.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinMemory {
    pub app: String,
    pub runtime: String,
    pub profile: String,
    /// `None` when it did not start even at the most allowed.
    pub mib: Option<u64>,
    /// Each limit tried, in MiB, and whether it started.
    pub probes: Vec<(u64, bool)>,
}

/// Searches between `lo` and `hi` MiB, halving the ratio between the
/// largest that failed and the smallest that worked until they are within
/// 3 %.
///
/// # Errors
/// A probe was refused or could not start.
pub fn min_memory(
    setup: &Setup,
    app: &str,
    runtime: &Runtime,
    profile: &Profile,
    lo: u64,
    hi: u64,
) -> Result<MinMemory, Error> {
    let phases = Phases {
        settle: Duration::from_secs(1),
        idle: Duration::ZERO,
        load: Duration::ZERO,
        gesture: Gesture::Click,
        detail: false,
    };
    let mut probes = Vec::new();
    let mut probe = |mib: u64| -> Result<bool, Error> {
        let r = run(
            setup,
            app,
            runtime,
            &profile.with_memory_max(mib << 20),
            phases,
            false,
        )?;
        let ok = r.outcome == Outcome::Ok && r.settled.is_some();
        probes.push((mib, ok));
        Ok(ok)
    };
    let mib = if !probe(hi)? {
        None
    } else if probe(lo)? {
        Some(lo)
    } else {
        let (mut fail, mut pass) = (lo, hi);
        while pass - fail > (fail / 32).max(1) {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let mid = ((fail as f64 * pass as f64).sqrt().round() as u64).clamp(fail + 1, pass - 1);
            if probe(mid)? {
                pass = mid;
            } else {
                fail = mid;
            }
        }
        Some(pass)
    };
    Ok(MinMemory {
        app: app.to_owned(),
        runtime: runtime.name.clone(),
        profile: profile.name.clone(),
        mib,
        probes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_intervals() {
        let f = frames(&[0, 16_000_000, 33_000_000, 100_000_000], 1.0, &[]);
        assert_eq!(f.count, 4);
        assert_eq!(f.max_ms, Some(67.0));
        assert_eq!(f.p50_ms, Some(17.0));
        assert_eq!(f.latency_p50_ms, None);
        let none = frames(&[], 1.0, &[]);
        assert_eq!((none.count, none.p50_ms), (0, None));
    }

    #[test]
    fn input_to_frame() {
        // Inputs at 5, 20 and 40 ms; frames at 16, 33 and 100 ms; one
        // input after the last frame is not answered.
        let ms = 1_000_000;
        let f = frames(
            &[16 * ms, 33 * ms, 100 * ms],
            1.0,
            &[5 * ms, 20 * ms, 40 * ms, 120 * ms],
        );
        assert_eq!(f.latency_p50_ms, Some(13.0));
        assert_eq!(f.latency_max_ms, Some(60.0));
    }
}
