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
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

use serde_json::Value;

use super::{BackendError, QmlFile, Result};

/// What `When the tests … are run` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestRun {
    /// Directories of test-only modules, relative to the scenario's files.
    pub import_paths: Vec<String>,
    /// `Case::function` or `Case::function:tag`; all tests when empty.
    pub filters: Vec<String>,
    /// Nothing is run: the files are one project that `ljos check` reads,
    /// and the report is everything it says. Qt has no program of the kind.
    pub check: bool,
    /// The `TZ` the runner starts with; UTC when `None`.
    pub time_zone: Option<String>,
}

impl TestRun {
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
struct Staged {
    root: PathBuf,
    /// The directory given to the runner, as the runner prints it.
    suite: PathBuf,
}

impl Staged {
    fn new(files: &[QmlFile]) -> Result<Self> {
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
    text.lines()
        .filter(|l| !l.starts_with("********* ") && !l.starts_with("Config: "))
        .map(|l| {
            let l = l.replace("qmltestrunner::", "").replace(&dir, "");
            let l = match l.strip_prefix("Totals: ") {
                Some(rest) => match rest.find(" skipped") {
                    Some(end) => format!("Totals: {}", &rest[..end + " skipped".len()]),
                    None => l,
                },
                None => l,
            };
            l.trim_end().to_owned()
        })
        .collect()
}

fn failed(what: impl std::fmt::Display) -> BackendError {
    BackendError::Failed(what.to_string())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `ljos` program: `LJOS_BIN`, or the one this workspace builds, built
/// once for the whole run so that a scenario tests the program as it is.
pub(crate) fn ljos_bin() -> Result<PathBuf> {
    static BIN: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Some(bin) = std::env::var_os("LJOS_BIN") {
            return Ok(PathBuf::from(bin));
        }
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let root = workspace_root();
        let built = Command::new(cargo)
            .args(["build", "--quiet", "-p", "ljos_cli"])
            .current_dir(&root)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run cargo to build `ljos`: {e}"))?;
        if !built.status.success() {
            return Err(format!(
                "`cargo build -p ljos_cli` failed:\n{}",
                String::from_utf8_lossy(&built.stderr)
            ));
        }
        let target =
            std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
        Ok(target.join("debug/ljos"))
    })
    .clone()
    .map_err(failed)
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
        return check_ljos(files);
    }
    let staged = Staged::new(files)?;
    let json = staged.root.join("report.json");
    let mut command = Command::new(ljos_bin()?);
    command.arg("test").arg(&staged.suite);
    for dir in &run.import_paths {
        command.arg("-import").arg(staged.suite.join(dir));
    }
    for filter in &run.filters {
        command.arg("--filter").arg(filter);
    }
    command.arg("--json").arg(&json);
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
    })
}

/// Reads the files as one project with `ljos check` and gives all it says:
/// what it finds, then its count, then its verdict when there are errors,
/// in the order a terminal shows them.
fn check_ljos(files: &[QmlFile]) -> Result<TestReport> {
    let staged = Staged::new(files)?;
    // What is found goes to the error stream and the count to the output:
    // one pipe for both keeps them in the order they were written.
    let out = Command::new("sh")
        .args(["-c", r#"exec "$0" check "$1" 2>&1"#])
        .arg(ljos_bin()?)
        .arg(&staged.suite)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| failed(format!("cannot start `ljos check`: {e}")))?;
    let status = out
        .status
        .code()
        .ok_or_else(|| failed("`ljos check` was killed"))?;
    Ok(TestReport {
        lines: normalise(&String::from_utf8_lossy(&out.stdout), &staged.suite),
        status,
        json: None,
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
        .arg(&staged.suite);
    for dir in &run.import_paths {
        command.arg("-import").arg(staged.suite.join(dir));
    }
    command.args(&run.filters);
    let out = command
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("TZ", run.time_zone())
        .env("LANG", "en_US.UTF-8")
        .env("GSETTINGS_BACKEND", "memory")
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
