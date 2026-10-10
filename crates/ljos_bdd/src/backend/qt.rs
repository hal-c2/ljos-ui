//! The Qt oracle (ARCHITECTURE.md §18.3).
//!
//! Qt cannot be driven interactively from a child process, so this backend is
//! a recorder: it keeps the scenario's operations as a JSON program and, when
//! a step needs an answer (`get`, `call`, `log`, ...), replays the whole
//! program in a freshly generated `driver.qml` run by `qml6` and reads the
//! observations back. Replay keeps scenarios independent and deterministic at
//! the cost of ~0.1-0.3 s per question. Nothing here polls: a run is one
//! process that the backend waits for.

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{A11yNode, Backend, BackendError, LogEntry, LogLevel, QmlFile, Result};

/// Marks the driver's result line among ordinary Qt log output.
const MARKER: &str = "@@ljos@@";
static RUN_ID: AtomicU32 = AtomicU32::new(0);

const DRIVER_TEMPLATE: &str = include_str!("driver.qml");

#[derive(Debug)]
pub struct QtBackend {
    qml: String,
    dir: Option<PathBuf>,
    files: BTreeMap<String, String>,
    program: Vec<Value>,
    /// Expressions of `bind` ops, emitted as functions in the driver.
    binders: Vec<String>,
    /// Last replay, valid while `program.len()` matches.
    cache: Option<(usize, Run)>,
}

#[derive(Debug, Clone)]
struct Run {
    results: Vec<Value>,
    log: Vec<LogEntry>,
}

impl Default for QtBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl QtBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            qml: std::env::var("QML6").unwrap_or_else(|_| "qml6".into()),
            dir: None,
            files: BTreeMap::new(),
            program: Vec::new(),
            binders: Vec::new(),
            cache: None,
        }
    }

    fn work_dir(&mut self) -> Result<PathBuf> {
        if let Some(d) = &self.dir {
            return Ok(d.clone());
        }
        let d = std::env::temp_dir().join(format!(
            "ljos-bdd-qt-{}-{}",
            std::process::id(),
            RUN_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&d)?;
        self.dir = Some(d.clone());
        Ok(d)
    }

    fn push(&mut self, op: Value) {
        self.program.push(op);
        self.cache = None;
    }

    /// Replay the program and return the observation of its last op.
    fn run(&mut self) -> Result<Run> {
        if let Some((n, run)) = &self.cache
            && *n == self.program.len()
        {
            return Ok(run.clone());
        }
        let dir = self.work_dir()?;
        for (name, source) in &self.files {
            fs::write(dir.join(name), source)?;
        }
        let program = serde_json::to_string(&self.program)
            .map_err(|e| BackendError::Failed(e.to_string()))?;
        let binders: String = self
            .binders
            .iter()
            .enumerate()
            // `with (self)` lets the expression name the target's properties unqualified,
            // exactly as a binding written inside the object would.
            .map(|(i, e)| {
                format!("    function bind{i}(self) {{ return Qt.binding(function() {{ with (self) {{ return ({e}); }} }}); }}\n")
            })
            .collect();
        let driver = DRIVER_TEMPLATE
            .replace("/*PROGRAM*/[]", &program)
            .replace("/*BINDERS*/", &binders)
            .replace("/*DIR*/\"\"", &json!(file_url(&dir)).to_string())
            .replace(
                "/*GRAB*/\"\"",
                &json!(dir.join("grab.png").to_string_lossy()).to_string(),
            );
        let driver_path = dir.join("driver.qml");
        fs::write(&driver_path, driver)?;

        let waited: u64 = self.program.iter().filter_map(|o| o["ms"].as_u64()).sum();
        let stderr = self.spawn(&driver_path, Duration::from_millis(20_000 + waited))?;
        let run = parse_stderr(&stderr)?;
        self.cache = Some((self.program.len(), run.clone()));
        Ok(run)
    }

    fn spawn(&self, driver: &Path, timeout: Duration) -> Result<String> {
        let mut child = Command::new(&self.qml)
            .arg(driver)
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("QT_QUICK_BACKEND", "software")
            .env("QT_MESSAGE_PATTERN", "%{type}|%{message}")
            .env("TZ", "UTC")
            .env("LANG", "en_US.UTF-8")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BackendError::Failed(format!("cannot start {}: {e}", self.qml)))?;
        let mut pipe = child.stderr.take().expect("stderr is piped");
        // Read on a thread so a chatty app cannot fill the pipe while we wait.
        let reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        });
        let deadline = Instant::now() + timeout;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BackendError::Failed(format!(
                    "qml6 timed out after {timeout:?}"
                )));
            }
            thread::sleep(Duration::from_millis(5));
        }
        Ok(reader.join().unwrap_or_default())
    }

    /// Record `op`, replay, and return the record of that op.
    fn ask(&mut self, op: Value) -> Result<Value> {
        self.push(op);
        let run = self.run()?;
        let record = run
            .results
            .get(self.program.len() - 1)
            .cloned()
            .ok_or_else(|| BackendError::Failed("driver stopped before answering".into()))?;
        match record["error"].as_str() {
            Some(e) => Err(BackendError::Failed(e.to_owned())),
            None => Ok(record["value"].clone()),
        }
    }
}

impl Drop for QtBackend {
    fn drop(&mut self) {
        if let Some(d) = &self.dir {
            let _ = fs::remove_dir_all(d);
        }
    }
}

fn file_url(p: &Path) -> String {
    format!("file://{}/", p.display())
}

/// Split Qt's stderr into driver observations and log entries.
fn parse_stderr(stderr: &str) -> Result<Run> {
    let mut results = None;
    let mut log: Vec<LogEntry> = Vec::new();
    for line in stderr.lines() {
        let Some((kind, msg)) = line.split_once('|') else {
            // Continuation of a multi-line message.
            if let Some(last) = log.last_mut() {
                last.message.push('\n');
                last.message.push_str(line);
            }
            continue;
        };
        if let Some(json) = msg.strip_prefix(MARKER) {
            results =
                Some(serde_json::from_str::<Vec<Value>>(json).map_err(|e| {
                    BackendError::Failed(format!("driver produced invalid JSON: {e}"))
                })?);
            continue;
        }
        let level = match kind {
            "warning" => LogLevel::Warning,
            "critical" | "fatal" => LogLevel::Critical,
            "info" => LogLevel::Info,
            _ => LogLevel::Debug,
        };
        log.push(LogEntry {
            level,
            message: msg.to_owned(),
        });
    }
    let results = results.ok_or_else(|| {
        BackendError::Failed(format!(
            "qml6 printed no result (is Qt 6 installed?). stderr:\n{stderr}"
        ))
    })?;
    Ok(Run { results, log })
}

impl Backend for QtBackend {
    fn name(&self) -> &'static str {
        "qt"
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        for f in files {
            self.files.insert(f.name.clone(), f.source.clone());
        }
        // Loading reports component errors through the result, so ask.
        self.ask(json!({"op": "load", "file": entry})).map(drop)
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        self.ask(json!({"op": "set", "obj": object, "prop": property, "value": value}))
            .map(drop)
    }

    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()> {
        self.binders.push(expression.to_owned());
        let id = self.binders.len() - 1;
        self.ask(json!({"op": "bind", "obj": object, "prop": property, "binder": id}))
            .map(drop)
    }

    fn get(&mut self, object: &str, property: &str) -> Result<Value> {
        self.ask(json!({"op": "read", "obj": object, "prop": property}))
    }

    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value> {
        self.ask(json!({"op": "call", "obj": object, "method": method, "args": args}))
    }

    fn exists(&mut self, object: &str) -> Result<bool> {
        Ok(self
            .ask(json!({"op": "exists", "obj": object}))?
            .as_bool()
            .unwrap_or(false))
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        // Recorded only: the replay performs the wait where it belongs in time.
        self.push(json!({"op": "wait", "ms": ms}));
        Ok(())
    }

    fn click(&mut self, _: &str, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "click by role and name needs the accessibility bridge",
        })
    }

    fn key(&mut self, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "key input",
        })
    }

    fn touch(&mut self, _: f64, _: f64) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "touch input",
        })
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "default accessible names need a QAccessible helper",
        })
    }

    fn log(&mut self) -> Result<Vec<LogEntry>> {
        // A `log` question is a no-op op so the replay ends exactly here.
        self.push(json!({"op": "nop"}));
        Ok(self.run()?.log)
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        self.ask(json!({"op": "grab"}))?;
        let path = self.work_dir()?.join("grab.png");
        Ok(fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_splits_results_and_log() {
        let err = "warning|x.qml:3: Binding loop detected\nsecond line\ndebug|hello\ndebug|@@ljos@@[{\"value\":1}]\n";
        let run = parse_stderr(err).unwrap();
        assert_eq!(run.results.len(), 1);
        assert_eq!(run.log.len(), 2);
        assert_eq!(run.log[0].level, LogLevel::Warning);
        assert_eq!(
            run.log[0].message,
            "x.qml:3: Binding loop detected\nsecond line"
        );
    }

    #[test]
    fn missing_result_is_an_error() {
        assert!(parse_stderr("warning|boom\n").is_err());
    }
}
