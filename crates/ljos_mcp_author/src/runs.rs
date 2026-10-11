//! `features_run`, `run_status`, `run_cancel`: scenario runs as jobs.
//!
//! A run is another process, because a run of the whole ledger takes
//! minutes and builds first: `features_run` starts it and answers at once,
//! and `run_status` says how it is going. The process is the project's
//! scenario runner, the one a person starts with `mise run features`; it is
//! not run inside the server, whose standard output is the MCP transport.
//!
//! Nothing here polls. Two threads read what the run prints, a third waits
//! for it to end and says so on a watch channel that `run_status` with
//! `wait_ms` sleeps on.

use std::{
    collections::VecDeque,
    io::{BufRead as _, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use ljos_mcp_core::{CallCtx, ErrorCode, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    Project,
    features::{Choice, Chosen},
};

/// Runs whose result is kept. Older ones are forgotten.
const KEPT: usize = 16;
/// The end of a run's output that is kept.
const TAIL_BYTES: usize = 6 * 1024;
const LINE_CHARS: usize = 400;
const MAX_FAILURES: usize = 12;
const DETAIL_BYTES: usize = 600;
/// The longest `run_status` waits: under what a client waits for an answer.
const MAX_WAIT_MS: u64 = 30_000;

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Backend {
    /// The Ljós runtime, headless.
    #[default]
    Ljos,
    /// Qt itself (`qml6`), for scenarios tagged `@qt-parity`. Slow.
    Qt,
    /// Both, one after the other.
    Both,
}

impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Ljos => "ljos",
            Self::Qt => "qt",
            Self::Both => "both",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunArgs {
    /// `.feature` files or directories under `features/`, with or without
    /// that prefix. Everything if absent, which takes minutes.
    #[serde(default)]
    paths: Vec<String>,
    /// Tag expression: `@qt-parity`, `@a and not @b`.
    tags: Option<String>,
    /// Regular expression the scenario's name must match.
    name: Option<String>,
    /// What runs the scenarios: `ljos` unless given.
    #[serde(default)]
    backend: Backend,
    /// Run the `@backlog` scenarios instead of the others.
    #[serde(default)]
    backlog: bool,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct Started {
    /// What `run_status` and `run_cancel` are asked about.
    run_id: String,
    /// Always `running`: ask `run_status` how it ended.
    state: State,
    backend: Backend,
    /// The command line that was started, in the project's root.
    command: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StatusArgs {
    /// From `features_run`. The latest run if absent.
    run_id: Option<String>,
    /// Wait up to this many milliseconds for the run to end before
    /// answering; 30000 at most. Answers at once if absent.
    wait_ms: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CancelArgs {
    /// From `features_run`. The latest run if absent.
    run_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    Running,
    /// Ended with exit status 0: every scenario that ran passed.
    Passed,
    /// Ended with another status: see `failures` and `output`.
    Failed,
    /// Stopped by `run_cancel`.
    Cancelled,
}

#[derive(Clone, Serialize, JsonSchema)]
struct Failure {
    scenario: String,
    /// The step that failed, with its keyword.
    step: String,
    /// What the runner printed under the step, cut to 600 bytes.
    detail: String,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct StatusOut {
    run_id: String,
    state: State,
    /// Null while running, and when a signal ended the run.
    exit_code: Option<i32>,
    elapsed_ms: u64,
    /// The runner's own counts, once it has printed them:
    /// `10 scenarios (9 passed, 1 failed); 42 steps (41 passed, 1 failed)`.
    summary: Option<String>,
    /// Failed steps seen so far, at most 12.
    failures: Vec<Failure>,
    /// Failed steps beyond those in `failures`.
    failures_omitted: usize,
    /// The end of what the run printed, at most 6 KB.
    output: String,
    /// Earlier output was dropped.
    output_truncated: bool,
}

/// What has been read of a run's output, and how the run ended.
struct Progress {
    state: State,
    exit_code: Option<i32>,
    ended: Option<Instant>,
    tail: VecDeque<String>,
    tail_bytes: usize,
    dropped: bool,
    scenario: String,
    failures: Vec<Failure>,
    failures_omitted: usize,
    /// The last failure in `failures` is still being described.
    describing: bool,
    summary: Option<Vec<String>>,
}

/// `line` without the colour codes a runner prints for a terminal.
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // `ESC [ ... letter`.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

impl Progress {
    fn new() -> Self {
        Self {
            state: State::Running,
            exit_code: None,
            ended: None,
            tail: VecDeque::new(),
            tail_bytes: 0,
            dropped: false,
            scenario: String::new(),
            failures: Vec::new(),
            failures_omitted: 0,
            describing: false,
            summary: None,
        }
    }

    /// Takes one line of the run's output: keeps it in the tail and reads
    /// what the runner says with it.
    fn line(&mut self, raw: &str) {
        let line = plain(raw);
        self.read(&line);
        let mut kept: String = line.chars().take(LINE_CHARS).collect();
        if kept.len() < line.len() {
            kept.push('…');
        }
        self.tail_bytes += kept.len() + 1;
        self.tail.push_back(kept);
        while self.tail_bytes > TAIL_BYTES && self.tail.len() > 1 {
            if let Some(first) = self.tail.pop_front() {
                self.tail_bytes -= first.len() + 1;
                self.dropped = true;
            }
        }
    }

    fn read(&mut self, line: &str) {
        let text = line.trim();
        if text == "[Summary]" {
            self.summary = Some(Vec::new());
            self.describing = false;
        } else if let Some(summary) = &mut self.summary {
            if !text.is_empty() && summary.len() < 6 {
                summary.push(text.to_owned());
            }
        } else if let Some(name) = [
            "Scenario Outline:",
            "Scenario Template:",
            "Scenario:",
            "Example:",
        ]
        .iter()
        .find_map(|keyword| text.strip_prefix(keyword))
        {
            self.scenario = name.trim().to_owned();
            self.describing = false;
        } else if text.starts_with("Feature:") || text.starts_with("Rule:") || text.starts_with('✔')
        {
            self.describing = false;
        } else if let Some(step) = text.strip_prefix('✘') {
            // A step with a doc string or a table is marked `✘>`.
            let step = step.strip_prefix('>').unwrap_or(step).trim();
            if self.failures.len() < MAX_FAILURES {
                self.failures.push(Failure {
                    scenario: self.scenario.clone(),
                    step: step.to_owned(),
                    detail: String::new(),
                });
                self.describing = true;
            } else {
                self.failures_omitted += 1;
                self.describing = false;
            }
        } else if self.describing
            && !text.is_empty()
            && let Some(failure) = self.failures.last_mut()
            && failure.detail.len() < DETAIL_BYTES
        {
            if !failure.detail.is_empty() {
                failure.detail.push('\n');
            }
            let room = DETAIL_BYTES - failure.detail.len();
            if text.len() <= room {
                failure.detail.push_str(text);
            } else {
                let mut cut = room;
                while !text.is_char_boundary(cut) {
                    cut -= 1;
                }
                failure.detail.push_str(&text[..cut]);
                failure.detail.push('…');
            }
        }
    }
}

struct Job {
    id: String,
    /// The process, which leads a group of its own so that what it starts
    /// can be stopped with it.
    pid: u32,
    started: Instant,
    progress: Mutex<Progress>,
    cancelled: AtomicBool,
    /// Becomes true when the run has ended and its output has been read.
    ended: watch::Sender<bool>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A reader that panicked has left the text it had read, which is still
    // worth showing.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Job {
    fn running(&self) -> bool {
        locked(&self.progress).state == State::Running
    }

    fn status(&self) -> StatusOut {
        let progress = locked(&self.progress);
        let until = progress.ended.unwrap_or_else(Instant::now);
        StatusOut {
            run_id: self.id.clone(),
            state: progress.state,
            exit_code: progress.exit_code,
            elapsed_ms: until.duration_since(self.started).as_millis() as u64,
            summary: progress
                .summary
                .as_ref()
                .filter(|lines| !lines.is_empty())
                .map(|lines| lines.join("; ")),
            failures: progress.failures.clone(),
            failures_omitted: progress.failures_omitted,
            output: progress.tail.iter().fold(String::new(), |mut all, line| {
                all.push_str(line);
                all.push('\n');
                all
            }),
            output_truncated: progress.dropped,
        }
    }

    /// Asks the run and everything it started to end.
    fn signal(&self, hard: bool) {
        #[cfg(unix)]
        {
            // The `kill` program and not a system call: this crate has no
            // unsafe code. A negative number names the process group.
            let signal = if hard { "-KILL" } else { "-TERM" };
            let _ = Command::new("kill")
                .args([signal, "--", &format!("-{}", self.pid)])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(unix))]
        {
            let _ = hard;
            let _ = Command::new("taskkill")
                .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    async fn wait(&self, at_most: Duration) {
        let mut ended = self.ended.subscribe();
        let _ = tokio::time::timeout(at_most, ended.wait_for(|ended| *ended)).await;
    }
}

struct Book {
    next: u32,
    jobs: VecDeque<Arc<Job>>,
}

/// The runs of one server.
pub(crate) struct Runs {
    project: Arc<Project>,
    /// The program given with `--features-runner`.
    runner: Option<PathBuf>,
    book: Mutex<Book>,
}

impl std::fmt::Debug for Runs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runs")
            .field("runner", &self.runner)
            .field("runs", &locked(&self.book).jobs.len())
            .finish_non_exhaustive()
    }
}

/// A word of a command line as a shell would need it written.
fn quoted(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn feed(stream: impl Read, job: &Job) {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&line);
                locked(&job.progress).line(text.trim_end_matches('\n'));
            }
        }
    }
}

impl Runs {
    pub(crate) fn new(project: Arc<Project>, runner: Option<PathBuf>) -> Self {
        Self {
            project,
            runner,
            book: Mutex::new(Book {
                next: 1,
                jobs: VecDeque::new(),
            }),
        }
    }

    /// The program that runs scenarios and the arguments it always gets.
    fn runner(&self) -> Result<(PathBuf, Vec<String>), SurfaceError> {
        if let Some(program) = &self.runner {
            return Ok((program.clone(), Vec::new()));
        }
        // A workspace that has the runner as a package builds and runs it.
        if self
            .project
            .root()
            .join("crates/ljos_bdd/Cargo.toml")
            .is_file()
        {
            let fixed = [
                "test", "--quiet", "-p", "ljos_bdd", "--test", "features", "--",
            ];
            return Ok(("cargo".into(), fixed.map(str::to_owned).to_vec()));
        }
        Err(SurfaceError::unavailable(
            "this project has no scenario runner the server knows how to start: start the server \
             with `ljos mcp --features-runner <program>`. The program is run in the project's \
             root with `[--input <glob>] [-t <tag expression>] [-n <name regex>] [--backlog]` and \
             `LJOS_BDD_BACKEND` set to `ljos`, `qt` or `both`",
        ))
    }

    /// What the runner is asked to run: the glob its `--input` takes.
    fn input(&self, choice: &Choice) -> Option<String> {
        let root = self.project.features_dir();
        let root = root.canonicalize().unwrap_or(root);
        let globs: Vec<String> = choice
            .paths
            .iter()
            .map(|chosen| match chosen {
                Chosen::Dir(_, below) if below.is_empty() => "**/*.feature".to_owned(),
                Chosen::Dir(_, below) => format!("{below}/**/*.feature"),
                Chosen::File(path) => path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            })
            .collect();
        let root = root.to_string_lossy();
        match globs.as_slice() {
            [] => None,
            [one] => Some(format!("{root}/{one}")),
            many => Some(format!("{root}/{{{}}}", many.join(","))),
        }
    }

    fn start(&self, args: &RunArgs) -> Result<Started, SurfaceError> {
        let choice = Choice::of(
            &self.project,
            &args.paths,
            args.tags.as_deref(),
            args.name.as_deref(),
        )?;
        let (program, mut arguments) = self.runner()?;
        if let Some(input) = self.input(&choice) {
            arguments.extend(["--input".to_owned(), input]);
        }
        if let Some((text, _)) = &choice.tags {
            arguments.extend(["-t".to_owned(), text.clone()]);
        }
        if let Some(name) = &choice.name {
            arguments.extend(["-n".to_owned(), name.as_str().to_owned()]);
        }
        if args.backlog {
            arguments.push("--backlog".to_owned());
        }

        let mut book = locked(&self.book);
        if let Some(running) = book.jobs.iter().find(|job| job.running()) {
            return Err(SurfaceError::new(
                ErrorCode::Busy,
                format!(
                    "`{}` is still running and runs do not overlap: wait for it with \
                     `run_status` (`wait_ms`), or stop it with `run_cancel`",
                    running.id
                ),
            )
            .with_details(serde_json::json!({ "run_id": running.id })));
        }

        let mut command = Command::new(&program);
        command
            .args(&arguments)
            .env("LJOS_BDD_BACKEND", args.backend.name())
            .current_dir(self.project.root())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().map_err(|e| {
            SurfaceError::unavailable(format!(
                "the scenario runner `{}` could not be started: {e}",
                program.display()
            ))
        })?;

        let id = format!("run-{}", book.next);
        book.next += 1;
        let job = Arc::new(Job {
            id: id.clone(),
            pid: child.id(),
            started: Instant::now(),
            progress: Mutex::new(Progress::new()),
            cancelled: AtomicBool::new(false),
            ended: watch::channel(false).0,
        });
        let (read, readers) = mpsc::channel::<()>();
        let mut reading = 0;
        if let Some(stream) = child.stdout.take() {
            let (job, read) = (job.clone(), read.clone());
            reading += 1;
            std::thread::spawn(move || {
                feed(stream, &job);
                let _ = read.send(());
            });
        }
        if let Some(stream) = child.stderr.take() {
            let (job, read) = (job.clone(), read.clone());
            reading += 1;
            std::thread::spawn(move || {
                feed(stream, &job);
                let _ = read.send(());
            });
        }
        drop(read);
        {
            let job = job.clone();
            std::thread::spawn(move || {
                let status = child.wait();
                // The last lines are the summary: wait for them. Not for
                // ever, because something the run left behind may hold the
                // pipe open after the run itself has ended.
                for _ in 0..reading {
                    if readers.recv_timeout(Duration::from_secs(2)).is_err() {
                        break;
                    }
                }
                let mut progress = locked(&job.progress);
                progress.ended = Some(Instant::now());
                progress.exit_code = status.as_ref().ok().and_then(|s| s.code());
                progress.state = if job.cancelled.load(Ordering::SeqCst) {
                    State::Cancelled
                } else if status.is_ok_and(|s| s.success()) {
                    State::Passed
                } else {
                    State::Failed
                };
                drop(progress);
                job.ended.send_replace(true);
            });
        }

        book.jobs.push_back(job);
        while book.jobs.len() > KEPT {
            book.jobs.pop_front();
        }
        let line: Vec<String> = std::iter::once(program.to_string_lossy().into_owned())
            .chain(arguments)
            .map(|word| quoted(&word))
            .collect();
        Ok(Started {
            run_id: id,
            state: State::Running,
            backend: args.backend,
            command: format!(
                "LJOS_BDD_BACKEND={} {}",
                args.backend.name(),
                line.join(" ")
            ),
        })
    }

    fn job(&self, id: Option<&str>) -> Result<Arc<Job>, SurfaceError> {
        let book = locked(&self.book);
        let known = || -> Vec<&str> { book.jobs.iter().map(|job| job.id.as_str()).collect() };
        match id {
            None => book.jobs.back().cloned().ok_or_else(|| {
                SurfaceError::not_found("no run has been started; `features_run` starts one")
            }),
            Some(id) => book
                .jobs
                .iter()
                .find(|job| job.id == id)
                .cloned()
                .ok_or_else(|| {
                    let known = known();
                    let hint = match known.last() {
                        Some(latest) => format!("the latest run is `{latest}`"),
                        None => "no run has been started; `features_run` starts one".to_owned(),
                    };
                    SurfaceError::not_found(format!("there is no run `{id}`: {hint}"))
                        .with_details(serde_json::json!({ "runs": known }))
                }),
        }
    }

    pub(crate) async fn status(&self, args: StatusArgs) -> Result<StatusOut, SurfaceError> {
        let job = self.job(args.run_id.as_deref())?;
        if let Some(wait) = args.wait_ms.filter(|ms| *ms > 0)
            && job.running()
        {
            job.wait(Duration::from_millis(wait.min(MAX_WAIT_MS))).await;
        }
        Ok(job.status())
    }

    pub(crate) async fn cancel(&self, args: CancelArgs) -> Result<StatusOut, SurfaceError> {
        let job = self.job(args.run_id.as_deref())?;
        if job.running() {
            job.cancelled.store(true, Ordering::SeqCst);
            job.signal(false);
            job.wait(Duration::from_millis(1500)).await;
            if job.running() {
                // It did not take the hint.
                job.signal(true);
                job.wait(Duration::from_secs(4)).await;
            }
        }
        Ok(job.status())
    }
}

impl Drop for Runs {
    /// A run does not outlive the server that started it: nobody could ask
    /// about it any more.
    fn drop(&mut self) {
        for job in &locked(&self.book).jobs {
            if job.running() {
                job.cancelled.store(true, Ordering::SeqCst);
                job.signal(true);
            }
        }
    }
}

pub(crate) const RUN: &str = "Start a run of scenarios chosen as in `features_list`, on the Ljós backend unless `backend` says `qt` or `both`. Answers a `run_id` at once: ask `run_status` how it went. One run at a time. Needs the `run` capability.";
pub(crate) const STATUS: &str = "State of a run started by `features_run`: running, passed, failed or cancelled, with the exit code, the runner's summary, the failed steps and the end of its output. `wait_ms` waits that long for the run to end.";
pub(crate) const CANCEL: &str = "Stop a run started by `features_run`. Answers the run's state, which is `cancelled` unless it had already ended.";

pub(crate) fn handler(runs: &Arc<Runs>) -> ToolHandler {
    let runs = runs.clone();
    typed(move |_: &CallCtx, args: RunArgs| runs.start(&args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(lines: &str) -> Progress {
        let mut progress = Progress::new();
        for line in lines.lines() {
            progress.line(line);
        }
        progress
    }

    #[test]
    fn failed_steps_are_read_with_their_scenario_and_what_was_said() {
        let progress = read(
            "Feature: Boxes\n  Rule: Size\n    Scenario: A box is wide\n     ✔> Given the QML:\n     \
             ✘  Then the box is 10 wide\n        Step failed:\n        the box is 12 wide\n    \
             Scenario: A box is tall\n     ✔  Given a box\n     ✘> Then the box is:\n        no\n\
             [Summary]\n1 feature\n2 scenarios (2 failed)\n3 steps (1 passed, 2 failed)\n",
        );
        assert_eq!(progress.failures.len(), 2);
        assert_eq!(progress.failures[0].scenario, "A box is wide");
        assert_eq!(progress.failures[0].step, "Then the box is 10 wide");
        assert_eq!(
            progress.failures[0].detail,
            "Step failed:\nthe box is 12 wide"
        );
        assert_eq!(progress.failures[1].scenario, "A box is tall");
        assert_eq!(progress.failures[1].step, "Then the box is:");
        assert_eq!(progress.failures[1].detail, "no");
        assert_eq!(
            progress.summary.unwrap().join("; "),
            "1 feature; 2 scenarios (2 failed); 3 steps (1 passed, 2 failed)"
        );
    }

    #[test]
    fn the_tail_and_the_failures_are_bounded() {
        let mut progress = Progress::new();
        progress.line("Scenario: Long");
        for n in 0..40 {
            progress.line(&format!("✘  Then step {n}"));
            progress.line(&"x".repeat(5000));
        }
        assert_eq!(progress.failures.len(), MAX_FAILURES);
        assert_eq!(progress.failures_omitted, 40 - MAX_FAILURES);
        assert!(
            progress
                .failures
                .iter()
                .all(|f| f.detail.len() <= DETAIL_BYTES + 4)
        );
        assert!(progress.tail_bytes <= TAIL_BYTES);
        assert!(progress.dropped);
        assert!(
            progress
                .tail
                .iter()
                .all(|line| line.chars().count() <= LINE_CHARS + 1)
        );
    }

    #[test]
    fn colour_codes_are_not_part_of_what_was_said() {
        assert_eq!(plain("\u{1b}[31m✘  Then no\u{1b}[0m\r"), "✘  Then no");
        let progress = read("Scenario: A\n\u{1b}[31m   ✘  Then no\u{1b}[0m\n");
        assert_eq!(progress.failures[0].step, "Then no");
    }

    #[test]
    fn a_command_line_is_shown_as_it_could_be_typed() {
        assert_eq!(quoted("--input"), "--input");
        assert_eq!(quoted("@a and not @b"), "'@a and not @b'");
        assert_eq!(
            quoted("/p/features/items/**/*.feature"),
            "'/p/features/items/**/*.feature'"
        );
        assert_eq!(quoted("it's"), "'it'\\''s'");
    }
}
