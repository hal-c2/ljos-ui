//! Running `tst_*.qml` files with a test runner, for the scenarios about
//! the `QtTest` module (`features/tooling/qttest*.feature`).
//!
//! A scenario of that kind does not drive a running application: it writes
//! test files, runs them all with the runner a QML author would use
//! (`ljos test` on Ljós, `qmltestrunner` on Qt) and reads what the runner
//! printed. Both runners are run as processes, so the exit status and the
//! command line are part of what is checked.
//!
//! What the two print differs in ways no scenario is about (the directory
//! the files are in, the name of the program, how long the run took);
//! [`normalise`] takes those out so that one expectation serves both.

use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use ljos_child::Reaped;

use serde_json::Value;

use super::{BackendError, QmlFile, Result};

/// What `When the tests … are run` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestRun {
    /// The directory the runner is given, relative to the scenario's
    /// files; all of them when `None`.
    pub dir: Option<String>,
    /// Directories of test-only modules, relative to the scenario's files.
    pub import_paths: Vec<String>,
    /// `Case::function` or `Case::function:tag`; all tests when empty.
    pub filters: Vec<String>,
    /// Nothing is run: the files are one project that `ljos check` reads,
    /// and the report is everything it says. Qt has no program of the kind.
    pub check: bool,
    /// With `check`: `--stats`, the coverage tables after what it finds.
    pub stats: bool,
    /// With `check`: `ljos migrate --write` first, and what it says before
    /// what the check says, so a scenario sees a migrated file compile.
    pub migrate: bool,
    /// With `check`: `--tsc`, TypeScript's errors at their QML lines after
    /// the checker's.
    pub tsc: bool,
    /// The `TZ` the runner starts with; UTC when `None`.
    pub time_zone: Option<String>,
    /// With `check`: `--project`, the directory `.ljos/` is written under,
    /// relative to the scenario's files.
    pub project: Option<String>,
    /// With `check`: `ljos types` instead of `ljos check`, as many times
    /// as this says, so a scenario sees what a second run writes.
    pub types: usize,
    /// With `check`: `--tree`, each directory two below the files' a
    /// project of its own.
    pub tree: bool,
}

impl TestRun {
    fn input(&self, suite: &Path) -> PathBuf {
        self.dir
            .as_ref()
            .map_or_else(|| suite.to_owned(), |d| suite.join(d))
    }

    fn time_zone(&self) -> &str {
        self.time_zone.as_deref().unwrap_or("UTC")
    }
}

/// What a runner said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TestReport {
    /// The plain text report, normalised.
    pub lines: Vec<String>,
    /// The runner's exit status.
    pub status: i32,
    /// The machine-readable report, from a runner that writes one.
    pub json: Option<Value>,
    /// Every text file of the project once the run was over, by its path
    /// below the project (`/` between its parts), dot directories too:
    /// what a check or a migration left. Empty for a runner that only
    /// reads.
    pub files: BTreeMap<String, String>,
}

impl TestReport {
    /// The report as one text, for a failure message.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether `wanted` is in the report as a block of consecutive lines.
    #[must_use]
    pub fn has_lines(&self, wanted: &[String]) -> bool {
        wanted.is_empty() || self.lines.windows(wanted.len()).any(|w| w == wanted)
    }
}

/// The lines of a doc string as they are compared with a report: without
/// trailing blanks and without empty lines at either end.
#[must_use]
pub fn expected_lines(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let last = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    lines[first..last.max(first)]
        .iter()
        .map(|l| (*l).to_owned())
        .collect()
}

static RUN_ID: AtomicU32 = AtomicU32::new(0);

/// The files of one scenario on disk, removed when the run is over.
pub(crate) struct Staged {
    pub(crate) root: PathBuf,
    /// The directory given to the runner, as the runner prints it.
    pub(crate) suite: PathBuf,
}

impl Staged {
    pub(crate) fn new(files: &[QmlFile]) -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "ljos-bdd-tests-{}-{}",
            std::process::id(),
            RUN_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let suite = root.join("suite");
        fs::create_dir_all(&suite)?;
        // What a runner prints is the path it resolved, which on a machine
        // with a linked temporary directory is not the one given.
        let suite = suite.canonicalize()?;
        for f in files {
            let path = suite.join(&f.name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, &f.source)?;
        }
        Ok(Self { root, suite })
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Takes out of a runner's text what differs from run to run and from
/// runner to runner without being behaviour: the banner lines, the name of
/// Qt's runner in front of every test, the directory of the files, and the
/// time the run took.
#[must_use]
pub fn normalise(text: &str, suite: &Path) -> Vec<String> {
    let dir = format!("{}/", suite.display());
    // The suite itself, as `ljos migrate` names where it looked, is the
    // directory a scenario's files are in: `.`.
    let suite = suite.display().to_string();
    text.lines()
        .filter(|l| !l.starts_with("********* ") && !l.starts_with("Config: "))
        .map(|l| {
            let l = l
                .replace("qmltestrunner::", "")
                .replace(&dir, "")
                .replace(&suite, ".");
            let l = match l.strip_prefix("Totals: ") {
                Some(rest) => match rest.split_once(" skipped") {
                    Some((head, _)) => format!("Totals: {head} skipped"),
                    None => l,
                },
                None => l,
            };
            l.trim_end().to_owned()
        })
        .collect()
}

pub(crate) fn failed(what: impl std::fmt::Display) -> BackendError {
    BackendError::Failed(what.to_string())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `ljos` program: `LJOS_BIN`, or the one this workspace builds, built
/// once for the whole run so that a scenario tests the program as it is.
pub(crate) fn ljos_bin() -> Result<PathBuf> {
    static BIN: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();
    BIN.get_or_init(|| workspace_bin("LJOS_BIN", "ljos_cli", "ljos"))
        .clone()
        .map_err(failed)
}

/// The `ljos-fuzz` program, as [`ljos_bin`] finds `ljos`.
pub(crate) fn fuzz_bin() -> Result<PathBuf> {
    static BIN: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();
    BIN.get_or_init(|| workspace_bin("LJOS_FUZZ_BIN", "ljos_fuzz", "ljos-fuzz"))
        .clone()
        .map_err(failed)
}

/// The program `name` of the workspace's `package`, or the one `var` names.
fn workspace_bin(var: &str, package: &str, name: &str) -> std::result::Result<PathBuf, String> {
    if let Some(bin) = std::env::var_os(var) {
        return Ok(PathBuf::from(bin));
    }
    let root = workspace_root();
    let built = build_command(package, name)
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run cargo to build `{name}`: {e}"))?;
    if !built.status.success() {
        return Err(format!(
            "`cargo build -p {package} --bin {name}` failed:\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    Ok(target.join("debug").join(name))
}

/// `cargo build` of the one program a step runs, without what cargo told
/// this test about its own package. A build script may say it is built
/// again when `CARGO_MANIFEST_DIR` changes (ring's does), and cargo then
/// takes the test's value as a change: everything above it was built again
/// here, and again by the next `cargo build` outside, minutes each time.
fn build_command(package: &str, name: &str) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args(["build", "--quiet", "-p", package, "--bin", name]);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(told_of_the_test) {
            command.env_remove(key);
        }
    }
    command
}

/// Whether cargo sets `key` for the test it runs, not for every build.
fn told_of_the_test(key: &str) -> bool {
    matches!(
        key,
        "CARGO_MANIFEST_DIR"
            | "CARGO_MANIFEST_PATH"
            | "CARGO_MANIFEST_LINKS"
            | "CARGO_CRATE_NAME"
            | "CARGO_BIN_NAME"
            | "CARGO_PRIMARY_PACKAGE"
            | "CARGO_TARGET_TMPDIR"
            | "OUT_DIR"
    ) || key.starts_with("CARGO_PKG_")
        || key.starts_with("CARGO_BIN_EXE_")
}

/// Qt's runner for QML tests. The plain name is Qt 5's on some systems, so
/// the Qt 6 places are tried first.
fn qt_runner() -> String {
    if let Ok(given) = std::env::var("QMLTESTRUNNER") {
        return given;
    }
    [
        "/usr/lib/qt6/bin/qmltestrunner",
        "/usr/lib64/qt6/bin/qmltestrunner",
        "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
    ]
    .into_iter()
    .find(|p| Path::new(p).exists())
    .unwrap_or("qmltestrunner6")
    .to_owned()
}

/// Runs the files with `ljos test`.
///
/// # Errors
/// The program cannot be built or started. A failing test is not an error:
/// it is in the report.
pub fn run_ljos(files: &[QmlFile], run: &TestRun) -> Result<TestReport> {
    if run.check {
        return check_ljos(files, run);
    }
    let staged = Staged::new(files)?;
    let json = staged.root.join("report.json");
    let mut command = Command::new(ljos_bin()?);
    command.arg("test").arg(run.input(&staged.suite));
    for dir in &run.import_paths {
        command.arg("-import").arg(staged.suite.join(dir));
    }
    for filter in &run.filters {
        command.arg("--filter").arg(filter);
    }
    command.arg("--json").arg(&json);
    super::own_home(&mut command, &staged.root)?;
    let out = command
        .env("TZ", run.time_zone())
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| failed(format!("cannot start `ljos test`: {e}")))?;
    let status = out.status.code().ok_or_else(|| {
        failed(format!(
            "`ljos test` was killed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ))
    })?;
    let json = fs::read_to_string(&json)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    Ok(TestReport {
        lines: normalise(&String::from_utf8_lossy(&out.stdout), &staged.suite),
        status,
        json,
        files: BTreeMap::new(),
    })
}

/// Reads the files as one project with `ljos check` and gives all it says:
/// what it finds, then its count, then its verdict when there are errors,
/// in the order a terminal shows them.
fn check_ljos(files: &[QmlFile], run: &TestRun) -> Result<TestReport> {
    let staged = Staged::new(files)?;
    // What is found goes to the error stream and the count to the output:
    // one pipe for both keeps them in the order they were written. A
    // migration that fails stops before the check, as a script would.
    let script = if run.types > 0 {
        r#"l=$0; d=$1; n=$2; shift 2; while [ "$n" -gt 0 ]; do "$l" types "$@" "$d" 2>&1 || exit; n=$((n - 1)); done"#
    } else if run.migrate {
        r#"l=$0; d=$1; shift 2; "$l" migrate --write "$d" 2>&1 && exec "$l" check "$@" "$d" 2>&1"#
    } else {
        r#"l=$0; d=$1; shift 2; exec "$l" check "$@" "$d" 2>&1"#
    };
    let mut command = Command::new("sh");
    command
        .args(["-c", script])
        .arg(ljos_bin()?)
        .arg(run.input(&staged.suite))
        .arg(run.types.to_string())
        .args(run.stats.then_some("--stats"))
        .args(run.tsc.then_some("--tsc"))
        .args(run.tree.then_some("--tree"))
        .args(
            run.project
                .iter()
                .flat_map(|p| ["--project".into(), staged.suite.join(p).into_os_string()]),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Its own group, so that a check that never ends is ended with what it
    // started: the migration before it, TypeScript under it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = Reaped::spawn(&mut command)
        .map_err(|e| failed(format!("cannot start `ljos check`: {e}")))?;
    let mut pipe = child.stdout.take().expect("stdout is piped");
    // The pipe ends when every process that writes to it has: read on a
    // thread, it is waited for with a deadline.
    let (said, heard) = mpsc::channel();
    thread::spawn(move || {
        let mut out = Vec::new();
        let read = pipe.read_to_end(&mut out).map(|_| out);
        let _ = said.send(read);
    });
    let deadline = check_deadline(run.tsc, crate::watchdog::time_left());
    let out = match heard.recv_timeout(deadline) {
        Ok(read) => read.map_err(|e| failed(format!("cannot read `ljos check`: {e}")))?,
        Err(_) => {
            #[cfg(unix)]
            end_group(child.id());
            return Err(failed(format!(
                "`ljos check` did not finish in {:.0} seconds",
                deadline.as_secs_f64()
            )));
        }
    };
    let status = child
        .finish()
        .map_err(|e| failed(format!("cannot wait for `ljos check`: {e}")))?
        .code()
        .ok_or_else(|| failed("`ljos check` was killed"))?;
    Ok(TestReport {
        lines: normalise(&String::from_utf8_lossy(&out), &staged.suite),
        status,
        json: None,
        files: snapshot(&staged.suite),
    })
}

/// The text files below `dir`, as [`TestReport::files`] keeps them.
pub(crate) fn snapshot(dir: &Path) -> BTreeMap<String, String> {
    fn walk(dir: &Path, below: &str, into: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = if below.is_empty() {
                name
            } else {
                format!("{below}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                walk(&path, &key, into);
            } else if let Ok(text) = fs::read_to_string(&path) {
                into.insert(key, text);
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, "", &mut files);
    files
}

/// How long `ljos check` has. It reads a scenario's few files in well under
/// a second: one still running after this never ends (#394), and failing
/// its scenario says so where the run's watchdog would end every scenario
/// after it.
const CHECK_DEADLINE: Duration = Duration::from_secs(60);

/// How long `ljos check --tsc` has. TypeScript starting cold on a loaded
/// machine can take most of a minute without anything being wrong, so it
/// has longer.
const TSC_CHECK_DEADLINE: Duration = Duration::from_secs(240);

/// How long `ljos check` has, with `left` the time its scenario has left,
/// `None` for no limit. Given four fifths of that when it is the shorter,
/// a check that never ends fails its scenario as one, however short the
/// scenario deadline and however late in the scenario the check, where the
/// watchdog would end the run and name only the step.
fn check_deadline(tsc: bool, left: Option<Duration>) -> Duration {
    let own = if tsc {
        TSC_CHECK_DEADLINE
    } else {
        CHECK_DEADLINE
    };
    left.map_or(own, |left| own.min(left.mul_f64(0.8)))
}

/// Kills the process group `leader` leads.
#[cfg(unix)]
fn end_group(leader: u32) {
    let Ok(group) = libc::pid_t::try_from(leader) else {
        return;
    };
    // SAFETY: `kill` takes any pid; a group that has already ended is an
    // error it returns, not undefined behaviour.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}

/// What `ljos types --dts` writes: the declarations of Ljós's API for
/// TypeScript, one line of the report each.
///
/// # Errors
/// The program cannot be built or started.
pub fn declarations() -> Result<TestReport> {
    let out = Command::new(ljos_bin()?)
        .args(["types", "--dts"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| failed(format!("cannot start `ljos types`: {e}")))?;
    let status = out
        .status
        .code()
        .ok_or_else(|| failed("`ljos types` was killed"))?;
    if status != 0 {
        return Err(failed(format!(
            "`ljos types --dts` failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(TestReport {
        lines: String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim_end().to_owned())
            .collect(),
        status,
        json: None,
        files: BTreeMap::new(),
    })
}

/// Runs the files with Qt's `qmltestrunner`, without a window.
///
/// # Errors
/// The runner cannot be started, or does not finish in a minute.
pub fn run_qt(files: &[QmlFile], run: &TestRun) -> Result<TestReport> {
    if run.check {
        return Err(BackendError::NotImplemented("checking a project"));
    }
    let staged = Staged::new(files)?;
    let runner = qt_runner();
    // A test case that never starts keeps Qt's runner waiting for ever.
    let mut command = Command::new("timeout");
    command
        .args(["60", runner.as_str(), "-input"])
        .arg(run.input(&staged.suite));
    for dir in &run.import_paths {
        command.arg("-import").arg(staged.suite.join(dir));
    }
    command.args(&run.filters);
    super::own_home(&mut command, &staged.root)?;
    let out = command
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("TZ", run.time_zone())
        .env("LANG", "en_US.UTF-8")
        .env("GSETTINGS_BACKEND", "memory")
        // The desktop's platform theme would give the palette its accent.
        .env_remove("QT_QPA_PLATFORMTHEME")
        .current_dir(&staged.root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| failed(format!("cannot start {runner}: {e}")))?;
    let status = out
        .status
        .code()
        .ok_or_else(|| failed(format!("{runner} was killed")))?;
    if status == 124 {
        return Err(failed(format!("{runner} did not finish in 60 seconds")));
    }
    Ok(TestReport {
        lines: normalise(&String::from_utf8_lossy(&out.stdout), &staged.suite),
        status,
        json: None,
        files: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_ends_before_the_scenario_it_is_in() {
        // A check at the start of a scenario with the default deadline.
        let fresh = Some(crate::watchdog::DEFAULT_TIMEOUT);
        assert_eq!(check_deadline(false, fresh), CHECK_DEADLINE);
        assert_eq!(check_deadline(true, fresh), TSC_CHECK_DEADLINE);
        // A scenario with less left than the check's own deadline: set
        // short, or late in a scenario after slow steps.
        let late = Some(Duration::from_secs(10));
        assert_eq!(check_deadline(false, late), Duration::from_secs(8));
        assert_eq!(check_deadline(true, late), Duration::from_secs(8));
        // Nothing keeps the scenario's time.
        assert_eq!(check_deadline(true, None), TSC_CHECK_DEADLINE);
    }

    /// The issue repro: run from a test, the build of `ljos` was told the
    /// test's `CARGO_MANIFEST_DIR`, which ring's build script follows, so
    /// it and everything above it were built again on every run.
    #[test]
    fn the_build_of_a_program_is_not_told_the_test_s_package() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let command = build_command("ljos_cli", "ljos");
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .filter_map(|(key, _)| key.to_str())
            .collect();
        assert!(removed.contains(&"CARGO_MANIFEST_DIR"), "{removed:?}");
        assert!(removed.contains(&"CARGO_PKG_NAME"), "{removed:?}");
        assert!(
            !removed
                .iter()
                .any(|key| *key == "CARGO_HOME" || *key == "CARGO_TARGET_DIR")
        );
        let args: Vec<_> = command.get_args().filter_map(|a| a.to_str()).collect();
        assert_eq!(
            args,
            ["build", "--quiet", "-p", "ljos_cli", "--bin", "ljos"]
        );
    }

    #[test]
    fn banner_paths_names_and_times_are_taken_out() {
        let text = "\
********* Start testing of qmltestrunner *********
Config: Using QtTest library 6.11.2
PASS   : qmltestrunner::Math::initTestCase()
FAIL!  : qmltestrunner::Math::test_add() Compared values are not the same
   Actual   (): 2
   Loc: [/work/suite/sub/tst_math.qml(7)]
Totals: 1 passed, 1 failed, 0 skipped, 0 blacklisted, 12ms
********* Finished testing of qmltestrunner *********
";
        assert_eq!(
            normalise(text, Path::new("/work/suite")),
            [
                "PASS   : Math::initTestCase()",
                "FAIL!  : Math::test_add() Compared values are not the same",
                "   Actual   (): 2",
                "   Loc: [sub/tst_math.qml(7)]",
                "Totals: 1 passed, 1 failed, 0 skipped",
            ]
        );
    }

    #[test]
    fn a_doc_string_is_compared_without_its_blank_ends() {
        assert_eq!(
            expected_lines("\nPASS   : a()  \n   Loc: [x]\n\n"),
            ["PASS   : a()", "   Loc: [x]"]
        );
        assert!(expected_lines("\n\n").is_empty());
    }

    #[test]
    fn lines_are_looked_for_as_a_block() {
        let report = TestReport {
                lines: vec!["a".into(), "b".into(), "c".into()],
            ..TestReport::default()
        };
        assert!(report.has_lines(&["b".into(), "c".into()]));
        assert!(!report.has_lines(&["a".into(), "c".into()]));
    }
}
