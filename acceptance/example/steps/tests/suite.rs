//! The acceptance mechanism, end to end, on the example suite: the program
//! an adapter is, run on copies of the suite's directory in temporary
//! directories. `cargo test --manifest-path acceptance/example/steps/Cargo.toml`
//! or `mise run features:acceptance-test`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// The suite's directory in this repository (`PIN`, `features.globs`,
/// `RATCHET`, `fixture/`).
fn original() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "steps" || name == "README.md" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A fresh copy of the suite; its fixture is the checkout.
struct Suite {
    dir: tempfile::TempDir,
}

impl Suite {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy_dir(&original(), dir.path());
        Self { dir }
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.path(file)).unwrap()
    }

    fn write(&self, file: &str, text: &str) {
        fs::write(self.path(file), text).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ljos-acceptance-example"))
            .arg("--suite")
            .arg(self.dir.path())
            .args(args)
            .output()
            .unwrap()
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Every file below `dir` with its bytes: what "never edited" is checked on.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(snapshot(&path));
        } else {
            out.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    out.sort();
    out
}

#[test]
fn the_committed_ratchet_holds_and_the_suite_is_left_alone() {
    let suite = Suite::new();
    let before = snapshot(&suite.path("fixture"));
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(0), "{}\n{}", out(&o), err(&o));
    let text = out(&o);
    // 7 scenarios run, 1 more is on Ljos's backlog and only counted.
    assert!(
        text.contains("7 scenarios run on Ljos: 6 pass, 1 fail"),
        "{text}"
    );
    assert!(text.contains("1 skipped (@backlog-ljos)"), "{text}");
    // A failing scenario the ratchet does not hold is reported, not fatal.
    assert!(text.contains("not yet     features/later.feature :: Counting past a hundred"));
    assert_eq!(before, snapshot(&suite.path("fixture")));
}

#[test]
fn a_held_scenario_that_fails_is_a_regression() {
    let suite = Suite::new();
    let ratchet = suite.read("RATCHET");
    suite.write(
        "RATCHET",
        &format!("{ratchet}features/later.feature :: Counting past a hundred\n"),
    );
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(
        out(&o).contains("REGRESSION  features/later.feature :: Counting past a hundred: fails"),
        "{}",
        out(&o)
    );
    // The same for a held scenario the suite no longer has, and for one that
    // moved onto the backlog.
    suite.write(
        "RATCHET",
        &format!(
            "{ratchet}features/counter.feature :: Gone\nfeatures/later.feature :: Turning the dial\n"
        ),
    );
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        out(&o).contains("Gone: is not in the suite any more"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("Turning the dial: is tagged @backlog-ljos"));
}

#[test]
fn raise_adds_what_passes_and_never_lowers() {
    let suite = Suite::new();
    suite.write(
        "RATCHET",
        "# kept header\n\nfeatures/counter.feature :: Adding one\n",
    );
    let o = suite.run(&["--raise"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    assert!(out(&o).contains("RATCHET raised by 5"), "{}", out(&o));
    let raised = suite.read("RATCHET");
    assert!(raised.starts_with("# kept header\n\n"), "{raised}");
    assert_eq!(raised.lines().filter(|l| l.contains(" :: ")).count(), 6);
    assert!(
        !raised.contains("Counting past a hundred"),
        "a failing scenario is not held"
    );

    // Nothing new to hold: the file is not touched.
    let o = suite.run(&["--raise"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("RATCHET unchanged"));
    assert_eq!(raised, suite.read("RATCHET"));

    // While a held scenario fails nothing is written, so the bar cannot be
    // lowered by a run that happened to go badly.
    let with_failing = format!("{raised}features/later.feature :: Counting past a hundred\n");
    suite.write("RATCHET", &with_failing);
    let o = suite.run(&["--raise"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("RATCHET not raised"), "{}", out(&o));
    assert_eq!(with_failing, suite.read("RATCHET"));
}

#[test]
fn a_checkout_at_another_digest_is_refused() {
    let suite = Suite::new();
    fs::OpenOptions::new()
        .append(true)
        .open(suite.path("fixture/features/greeting.feature"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"# edited\n"))
        .unwrap();
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    assert!(err(&o).contains("but the PIN says blake3:"), "{}", err(&o));
    assert!(
        !out(&o).contains("scenarios run"),
        "nothing is run on an unpinned suite"
    );
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", err(&o));
    out(&o).trim().to_owned()
}

#[test]
fn a_git_checkout_must_be_at_the_pinned_commit() {
    let suite = Suite::new();
    // The suite's own repository: the fixture's files, committed.
    let checkout = tempfile::tempdir().unwrap();
    copy_dir(&suite.path("fixture"), checkout.path());
    git(checkout.path(), &["init", "-q"]);
    git(checkout.path(), &["add", "."]);
    git(checkout.path(), &["commit", "-q", "-m", "suite"]);
    let head = git(checkout.path(), &["rev-parse", "HEAD"]);
    let path = checkout.path().to_str().unwrap();

    suite.write(
        "PIN",
        &format!("repo = https://example.invalid/kiosk.git\ncommit = {head}\n"),
    );
    let o = suite.run(&["--path", path]);
    assert_eq!(o.status.code(), Some(0), "{}\n{}", out(&o), err(&o));
    // An abbreviation of the commit is enough.
    suite.write("PIN", &format!("commit = {}\n", &head[..10]));
    assert_eq!(suite.run(&["--path", path]).status.code(), Some(0));

    // The wrong commit, however it is wrong.
    suite.write(
        "PIN",
        &format!(
            "repo = https://example.invalid/kiosk.git\ncommit = {}\n",
            "ab".repeat(20)
        ),
    );
    let o = suite.run(&["--path", path]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    let said = err(&o);
    assert!(
        said.contains(&head) && said.contains(&"ab".repeat(20)),
        "{said}"
    );
    assert!(said.contains("https://example.invalid/kiosk.git"), "{said}");
    assert!(!out(&o).contains("scenarios run"));

    // A later commit is another revision, and a dirty tree is not the commit.
    suite.write("PIN", &format!("commit = {head}\n"));
    fs::write(
        checkout.path().join("features/greeting.feature"),
        "Feature: changed\n",
    )
    .unwrap();
    let o = suite.run(&["--path", path]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("uncommitted changes"), "{}", err(&o));
    git(checkout.path(), &["commit", "-q", "-am", "later"]);
    let o = suite.run(&["--path", path]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("but the PIN says"), "{}", err(&o));
}

#[test]
fn the_checkout_can_come_from_the_environment() {
    let suite = Suite::new();
    let pin = suite
        .read("PIN")
        .replace("path = fixture\n", "env = KIOSK_SUITE\n");
    suite.write("PIN", &pin);
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("$KIOSK_SUITE"), "{}", err(&o));
    let fixture = suite.path("fixture");
    let o = Command::new(env!("CARGO_BIN_EXE_ljos-acceptance-example"))
        .arg("--suite")
        .arg(suite.dir.path())
        .env("KIOSK_SUITE", &fixture)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}\n{}", out(&o), err(&o));
}

#[test]
fn a_glob_that_matches_nothing_is_an_error() {
    let suite = Suite::new();
    suite.write("features.globs", "features/*.feature\nrenamed/*.feature\n");
    let o = suite.run(&[]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    assert!(err(&o).contains("renamed/*.feature"), "{}", err(&o));
    assert!(err(&o).contains("matches no file"), "{}", err(&o));
    assert!(
        !out(&o).contains("scenarios run"),
        "an empty allowlist must not look green"
    );
}
