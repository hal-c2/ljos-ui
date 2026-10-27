//! The two sides a case runs on: Qt (`qml6`, a black box, many cases per
//! process) and Ljós (in process, one engine per case).

use std::{
    fs,
    io::Read,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use ljos_bdd::backend::{Backend, LogLevel, QmlFile, ljos::LjosBackend, qt};
use ljos_child::Reaped;
use serde_json::Value;

use crate::outcome::{Outcome, Phase, Rendered};

/// Marks the harness's lines among Qt's own log.
const MARKER: &str = "@@fz@@";

/// Somewhere a batch of cases runs.
pub trait Side: Sync {
    fn name(&self) -> &'static str;
    /// One outcome per case, in order.
    fn run(&self, cases: &[Rendered]) -> Vec<Outcome>;
}

/// Runs `work(i)` for every `i` in `0..n` on `jobs` threads.
fn spread<T: Send>(n: usize, jobs: usize, work: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<T>>> = Mutex::new((0..n).map(|_| None).collect());
    thread::scope(|s| {
        for _ in 0..jobs.clamp(1, n.max(1)) {
            // The engine's script interpreter recurses deeply on nested
            // expressions; a test thread's default stack is not enough.
            thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(s, || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        let r = work(i);
                        out.lock().expect("no worker panics holding it")[i] = Some(r);
                    }
                })
                .expect("a worker thread starts");
        }
    });
    out.into_inner()
        .expect("no worker panics holding it")
        .into_iter()
        .map(|o| o.expect("every index ran"))
        .collect()
}

/// Ljós, in this process.
#[derive(Debug)]
pub struct LjosSide {
    pub jobs: usize,
}

impl Side for LjosSide {
    fn name(&self) -> &'static str {
        "ljos"
    }

    fn run(&self, cases: &[Rendered]) -> Vec<Outcome> {
        spread(cases.len(), self.jobs, |i| run_ljos(&cases[i]))
    }
}

/// One case on a fresh engine: whatever an earlier case left behind (a
/// type, a global, a font) cannot change what this one shows.
#[must_use]
pub fn run_ljos(case: &Rendered) -> Outcome {
    let mut backend = LjosBackend::new();
    let run = catch_unwind(AssertUnwindSafe(|| {
        let file = QmlFile {
            name: "Case.qml".into(),
            source: case.qml.clone(),
        };
        if let Err(e) = backend.load(&[file], "Case.qml") {
            return Outcome::Rejected(e.to_string());
        }
        let mut seen = 0;
        let mut phases = Vec::new();
        for p in 0..=case.steps {
            let mut call = |m: &str| match backend.call("root", m, &[]) {
                Ok(Value::String(s)) => Ok(s),
                Ok(v) => Err(format!("{m} returned {v}")),
                Err(e) => Err(format!("{m}: {e}")),
            };
            let result = if p == 0 {
                None
            } else {
                match call(&format!("__step{}", p - 1)) {
                    Ok(r) => Some(r),
                    Err(e) => return Outcome::Crashed(e),
                }
            };
            let obs = match call("__obs") {
                Ok(o) => o,
                Err(e) => return Outcome::Crashed(e),
            };
            let log = backend.log().unwrap_or_default();
            let warnings = log
                .iter()
                .skip(seen)
                .filter(|e| matches!(e.level, LogLevel::Warning | LogLevel::Critical))
                .map(|e| e.message.clone())
                .collect();
            seen = log.len();
            match Phase::parse(&obs, result.as_deref(), warnings) {
                Ok(phase) => phases.push(phase),
                Err(e) => return Outcome::Crashed(e),
            }
        }
        Outcome::Ran(phases)
    }));
    run.unwrap_or_else(|panic| {
        let text = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default();
        Outcome::Crashed(format!("panicked: {text}"))
    })
}

/// Qt, as `qml6` runs a generated harness that loads a batch of cases one
/// after another and prints a marked JSON line per phase.
#[derive(Debug)]
pub struct QtSide {
    pub qml: String,
    /// Where batches are written; never the system's temporary directory.
    pub work: PathBuf,
    pub jobs: usize,
    pub batch: usize,
    pub timeout: Duration,
    runs: AtomicU64,
}

impl QtSide {
    #[must_use]
    pub fn new(qml: String, work: PathBuf, jobs: usize) -> Self {
        // One home for every batch, made here before any runs in parallel:
        // a fresh one per process would leave fontconfig its cache to build
        // again each time, seconds a batch.
        let _ = fs::create_dir_all(&work);
        let _ = qt::oracle_env(&mut Command::new(&qml), &work, "offscreen");
        Self {
            qml,
            work,
            jobs,
            batch: 200,
            timeout: Duration::from_secs(120),
            runs: AtomicU64::new(0),
        }
    }

    /// Runs `cases` in one process after another until each has an outcome:
    /// a crash ends a process, the case it was in is the crash's, and the
    /// rest go on in a new one.
    fn run_batch(&self, cases: &[&Rendered]) -> Vec<Outcome> {
        let mut out: Vec<Option<Outcome>> = vec![None; cases.len()];
        loop {
            let todo: Vec<usize> = (0..cases.len()).filter(|i| out[*i].is_none()).collect();
            let Some(&first) = todo.first() else { break };
            let dir = self.work.join(format!(
                "b{}-{}",
                std::process::id(),
                self.runs.fetch_add(1, Ordering::Relaxed)
            ));
            let got = self.spawn(&dir, cases, &todo);
            // The cases are cheap to write again; the drive is shared.
            let _ = fs::remove_dir_all(&dir);
            let (done, failure) = match got {
                Ok(r) => r,
                Err(e) => (Vec::new(), Some((None, e))),
            };
            for (i, o) in done {
                out[i] = Some(o);
            }
            if let Some((at, why)) = failure {
                let at = at.unwrap_or(first);
                if out[at].is_none() {
                    out[at] = Some(Outcome::Crashed(why));
                }
            }
        }
        out.into_iter()
            .map(|o| o.expect("every case ended"))
            .collect()
    }

    /// One process over the cases `todo`. What finished, and when the
    /// process ended early, the case it was in and why.
    #[allow(clippy::type_complexity)]
    fn spawn(
        &self,
        dir: &std::path::Path,
        cases: &[&Rendered],
        todo: &[usize],
    ) -> Result<(Vec<(usize, Outcome)>, Option<(Option<usize>, String)>), String> {
        let io = |e: std::io::Error| format!("cannot write the batch in {}: {e}", dir.display());
        fs::create_dir_all(dir).map_err(io)?;
        let mut list = Vec::new();
        for &i in todo {
            let case_dir = dir.join(format!("c{i}"));
            fs::create_dir_all(&case_dir).map_err(io)?;
            fs::write(case_dir.join("Case.qml"), &cases[i].qml).map_err(io)?;
            list.push(serde_json::json!({"n": i, "steps": cases[i].steps}));
        }
        let url = format!("file://{}/", dir.display());
        let harness = HARNESS
            .replace("/*CASES*/[]", &Value::Array(list).to_string())
            .replace("/*DIR*/\"\"", &Value::String(url).to_string());
        let path = dir.join("harness.qml");
        fs::write(&path, harness).map_err(io)?;
        let mut command = Command::new(&self.qml);
        qt::oracle_env(&mut command, &self.work, "offscreen").map_err(io)?;
        // Every case is loaded once: caching its compiled form is only writing.
        command.env("QML_DISABLE_DISK_CACHE", "1");
        let mut child = Reaped::spawn(
            command
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped()),
        )
        .map_err(|e| format!("cannot start {}: {e}", self.qml))?;
        let mut pipe = child.stderr.take().expect("stderr is piped");
        let (tx, rx) = mpsc::channel();
        // Read on a thread so the wait below can give up on a hung qml6.
        thread::spawn(move || {
            let mut text = Vec::new();
            let _ = pipe.read_to_end(&mut text);
            let _ = tx.send(String::from_utf8_lossy(&text).into_owned());
        });
        let (text, hung) = match rx.recv_timeout(self.timeout) {
            Ok(t) => (t, false),
            Err(_) => {
                let _ = child.kill();
                (rx.recv().unwrap_or_default(), true)
            }
        };
        let status = child.finish().map_err(|e| e.to_string())?;
        let (done, at, ended) = parse(&text);
        let failure = if !ended || hung {
            let why = if hung {
                format!("qml6 hung for {:?}", self.timeout)
            } else {
                format!("qml6 ended with {status}: {}", tail(&text))
            };
            Some((
                at.or(todo
                    .iter()
                    .copied()
                    .find(|i| !done.iter().any(|(d, _)| d == i))),
                why,
            ))
        } else {
            None
        };
        Ok((done, failure))
    }
}

impl Side for QtSide {
    fn name(&self) -> &'static str {
        "qt"
    }

    fn run(&self, cases: &[Rendered]) -> Vec<Outcome> {
        let chunks: Vec<Vec<&Rendered>> = cases
            .chunks(self.batch.max(1))
            .map(|c| c.iter().collect())
            .collect();
        spread(chunks.len(), self.jobs, |i| self.run_batch(&chunks[i]))
            .into_iter()
            .flatten()
            .collect()
    }
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.contains(MARKER)).collect();
    lines[lines.len().saturating_sub(5)..].join("\n")
}

/// The finished cases in the harness's log, the case it was in when the log
/// ends, and whether it ended where the harness ends.
fn parse(text: &str) -> (Vec<(usize, Outcome)>, Option<usize>, bool) {
    let mut done = Vec::new();
    let mut started = false;
    let mut current: Option<(usize, Vec<Phase>)> = None;
    let mut warnings: Vec<String> = Vec::new();
    for line in text.lines() {
        let Some((kind, msg)) = line.split_once('|') else {
            if let Some(last) = warnings.last_mut() {
                last.push('\n');
                last.push_str(line);
            }
            continue;
        };
        let Some(json) = msg.strip_prefix(MARKER) else {
            if started && matches!(qt::log_level(kind), LogLevel::Warning | LogLevel::Critical) {
                warnings.push(msg.to_owned());
            }
            continue;
        };
        let Ok(m) = serde_json::from_str::<Value>(json) else {
            continue;
        };
        let n = m["n"].as_u64().map(|n| n as usize);
        if m.get("start").is_some() {
            started = true;
            warnings.clear();
        } else if m.get("end").is_some() {
            return (done, None, true);
        } else if let Some(err) = m.get("rejected") {
            let n = n.expect("a rejection names its case");
            done.push((n, Outcome::Rejected(err.as_str().unwrap_or("").to_owned())));
            warnings.clear();
            current = None;
        } else if m.get("done").is_some() {
            if let Some((n, phases)) = current.take() {
                done.push((n, Outcome::Ran(phases)));
            }
            warnings.clear();
        } else if let Some(n) = n {
            let phase = Phase::parse(
                m["obs"].as_str().unwrap_or(""),
                m.get("res").and_then(Value::as_str),
                std::mem::take(&mut warnings),
            );
            let entry = current.get_or_insert_with(|| (n, Vec::new()));
            match phase {
                Ok(p) => entry.1.push(p),
                Err(e) => {
                    done.push((n, Outcome::Crashed(e)));
                    current = None;
                }
            }
        }
    }
    let at = current.map(|(n, _)| n);
    (done, at, false)
}

/// The harness: loads each case of the batch as a child of a 400 × 400 item
/// (as the scenario driver does), observes it after the load and after
/// each step, and unparents and destroys it. Everything the case does is in the case's
/// own functions, which the Ljós side calls the same way.
const HARNESS: &str = r#"import QtQuick

Item {
    id: harness
    width: 400
    height: 400
    property var cases: /*CASES*/[]
    property string dir: /*DIR*/""

    function say(o) { console.log("@@fz@@" + JSON.stringify(o)) }

    function run() {
        say({start: 1})
        for (var i = 0; i < cases.length; ++i) {
            var c = cases[i]
            var comp = Qt.createComponent(dir + "c" + c.n + "/Case.qml")
            if (comp.status !== Component.Ready) {
                say({n: c.n, rejected: comp.errorString()})
                continue
            }
            var o = comp.createObject(harness)
            if (o === null) {
                say({n: c.n, rejected: "createObject returned null"})
                continue
            }
            say({n: c.n, p: 0, obs: o.__obs()})
            for (var s = 0; s < c.steps; ++s) {
                var r = o["__step" + s]()
                say({n: c.n, p: s + 1, res: r, obs: o.__obs()})
            }
            // destroy() waits for the event loop, which a batch never returns to:
            // unparented, a case done with no longer sees the cases after it.
            o.parent = null
            o.destroy()
            say({n: c.n, done: 1})
        }
        say({end: 1})
        Qt.quit()
    }

    Component.onCompleted: Qt.callLater(run)
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_log_is_split_into_cases_and_phases() {
        let log = "warning|QStandardPaths: before the start\n\
            debug|@@fz@@{\"start\":1}\n\
            debug|@@fz@@{\"n\":0,\"rejected\":\"Case.qml:2: Invalid property assignment\"}\n\
            warning|Case.qml:4: Binding loop detected\n\
            debug|@@fz@@{\"n\":1,\"p\":0,\"obs\":\"{\\\"root.a\\\":1}\"}\n\
            debug|@@fz@@{\"n\":1,\"p\":1,\"res\":\"{\\\"$u\\\":1}\",\"obs\":\"{\\\"root.a\\\":2}\"}\n\
            debug|@@fz@@{\"n\":1,\"done\":1}\n\
            debug|@@fz@@{\"n\":2,\"p\":0,\"obs\":\"{}\"}\n";
        let (done, at, ended) = parse(log);
        assert!(!ended, "the log stops inside case 2");
        assert_eq!(at, Some(2));
        assert!(matches!(done[0], (0, Outcome::Rejected(_))));
        let (1, Outcome::Ran(phases)) = &done[1] else {
            panic!("case 1 ran: {done:?}")
        };
        assert_eq!(phases.len(), 2);
        assert_eq!(
            phases[0].warnings.len(),
            1,
            "the loop warning is the load's"
        );
        assert!(phases[1].warnings.is_empty());
    }
}
