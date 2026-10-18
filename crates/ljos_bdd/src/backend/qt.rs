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
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{
    A11yNode, Backend, BackendError, LogEntry, LogLevel, PointerStep, QmlFile, Result,
    rfb::{Chord, Endpoint, HeldStep, Rfb, SCREEN, char_keysym},
};

/// Marks the driver's result line among ordinary Qt log output.
const MARKER: &str = "@@ljos@@";
/// The driver's request for the input of the op with the index that follows.
const INPUT_MARKER: &str = "@@input@@";
/// The input platform's start-up notice, a warning that no scenario caused.
const VNC_NOTICE: &str = "QVncServer created on port";

/// What the driver gets only in a run that sends input: the handler that
/// sees the pointer move ending each input step (see `rfb.rs`).
const INPUT_HANDLER: &str = r"    HoverHandler {
        onPointChanged: {
            var p = point.position
            if (driver.waiting && p.y === driver.height - 1 && p.x === driver.width - 2 + driver.acks % 2) {
                driver.waiting = false
                driver.acks++
                driver.run()
            }
        }
    }
";
/// What the driver gets besides in a run that holds the pointer down across
/// steps. Such a step cannot end with the move to the corner pixel: with the
/// button down that move would be part of the drag. A handler above
/// everything sees the step's own event instead (a passive grab, which takes
/// nothing from what is beneath) and goes on once Qt has delivered it.
const HELD_HANDLER: &str = r#"    function heldDone() { expect = null; waiting = false; Qt.callLater(run) }
    function heldAt(p) { return p.x === expect.x && p.y === expect.y }
    HoverHandler {
        onPointChanged: {
            var e = driver.expect
            if (driver.waiting && e && e.step === "hover" && driver.heldAt(point.position)) driver.heldDone()
        }
    }
    Item {
        anchors.fill: parent
        z: 1000000
        PointHandler {
            onActiveChanged: {
                var e = driver.expect
                if (driver.waiting && e && e.step === (active ? "press" : "release")) driver.heldDone()
            }
            onPointChanged: {
                var e = driver.expect
                if (active && driver.waiting && e && e.step === "move" && driver.heldAt(point.position)) driver.heldDone()
            }
        }
    }
"#;
/// The ops that are input, sent over the input channel. `driver.qml`
/// stops at the same ones and waits for the acknowledgement.
const INPUT_OPS: [&str; 5] = ["key", "pointer", "type", "double", "drag"];
static RUN_ID: AtomicU32 = AtomicU32::new(0);

const DRIVER_TEMPLATE: &str = include_str!("driver.qml");

#[derive(Debug)]
pub struct QtBackend {
    qml: String,
    dir: Option<PathBuf>,
    files: BTreeMap<String, String>,
    /// `.ts` sources by file name; released next to the driver before a run.
    translations: BTreeMap<String, String>,
    /// Files that are not QML (images), written beside the QML files.
    resources: BTreeMap<String, Vec<u8>>,
    program: Vec<Value>,
    /// Expressions of `bind` ops, emitted as functions in the driver.
    binders: Vec<String>,
    /// Last replay, valid while `program.len()` matches.
    cache: Option<(usize, Run)>,
    /// `QT_QUICK_CONTROLS_STYLE` for the run.
    controls_style: Option<String>,
    /// `-style` on qml6's command line.
    command_line_style: Option<String>,
    /// The scenario's controls configuration file and variables.
    controls_conf: Option<String>,
    controls_vars: Vec<(String, String)>,
    /// The locale variables of the run; `None` is `LANG=en_US.UTF-8`.
    locale_env: Option<Vec<(String, String)>>,
}

#[derive(Debug, Clone)]
struct Run {
    results: Vec<Value>,
    log: Vec<LogEntry>,
}

/// The variables a program's locale is read from.
const LOCALE_VARS: [&str; 7] = [
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_MESSAGES",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
];

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
            translations: BTreeMap::new(),
            resources: BTreeMap::new(),
            program: Vec::new(),
            binders: Vec::new(),
            cache: None,
            controls_style: None,
            command_line_style: None,
            controls_conf: None,
            controls_vars: Vec::new(),
            locale_env: None,
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
            // `look/Palette.qml`: a file of a directory the scenario imports.
            if let Some(parent) = dir.join(name).parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(dir.join(name), source)?;
        }
        for (name, bytes) in &self.resources {
            if let Some(parent) = dir.join(name).parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(dir.join(name), bytes)?;
        }
        self.release_translations(&dir)?;
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
            .replace(
                "/*INPUT*/\n",
                if self.sends_input() {
                    INPUT_HANDLER
                } else {
                    ""
                },
            )
            .replace(
                "/*HELD*/\n",
                if self.holds_pointer() {
                    HELD_HANDLER
                } else {
                    ""
                },
            )
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

    /// Qt loads translations only from `.qm` files, so each staged `.ts` is
    /// compiled by Qt's own `lrelease`, to where `qml6` looks for the
    /// catalogue of the UI language: `i18n/qml_<language>.qm` beside the
    /// main file. The `.qm` is an opaque artefact of the oracle; nothing
    /// here reads it.
    fn release_translations(&self, dir: &Path) -> Result<()> {
        if self.translations.is_empty() {
            return Ok(());
        }
        let out_dir = dir.join("i18n");
        fs::create_dir_all(&out_dir)?;
        for (name, ts) in &self.translations {
            let language = ts_language(ts).ok_or_else(|| {
                BackendError::Failed(format!("{name}: the TS element has no language attribute"))
            })?;
            let ts_path = dir.join(name);
            fs::write(&ts_path, ts)?;
            let qm = out_dir.join(format!("qml_{language}.qm"));
            let tool = lrelease();
            let out = Command::new(&tool)
                .arg(&ts_path)
                .arg("-qm")
                .arg(&qm)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| BackendError::Failed(format!("cannot start {tool}: {e}")))?;
            if !out.status.success() {
                return Err(BackendError::Failed(format!(
                    "{tool} rejected {name}: {}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                )));
            }
        }
        Ok(())
    }

    /// Whether the program presses keys or clicks. Only then does the replay
    /// run on Qt's `vnc` platform, the one that takes input (`rfb.rs`); every
    /// other scenario stays on `offscreen`.
    fn sends_input(&self) -> bool {
        self.program
            .iter()
            .any(|op| op["op"].as_str().is_some_and(|o| INPUT_OPS.contains(&o)))
    }

    /// Whether the program holds the pointer down across steps, or moves it
    /// with no button down.
    fn holds_pointer(&self) -> bool {
        self.program.iter().any(|op| op["step"].is_string())
    }

    /// Whether the button is down after the program as recorded so far.
    fn pointer_held(&self) -> bool {
        self.program
            .iter()
            .rev()
            .find_map(|op| op["step"].as_str())
            .is_some_and(|step| step == "press" || step == "move")
    }

    /// Where a step of a held pointer left the pointer, when it was the
    /// last input. Any other input leaves it at the corner pixel.
    fn pointer_at(&self) -> Option<(u64, u64)> {
        let last = self
            .program
            .iter()
            .rev()
            .find(|op| op["op"].as_str().is_some_and(|o| INPUT_OPS.contains(&o)))?;
        last["step"].as_str()?;
        last["x"].as_u64().zip(last["y"].as_u64())
    }

    /// Input that ends with the acknowledgement move cannot be sent while
    /// the button is down: the move would let go.
    fn not_held(&self) -> Result<()> {
        if self.pointer_held() {
            return Err(BackendError::Unsupported {
                backend: "qt",
                what: "other input while the pointer is held down",
            });
        }
        Ok(())
    }

    /// Sends what the op at `index` asks for, then the acknowledgement move.
    fn send_input(&self, rfb: &mut Rfb, index: usize) -> Result<()> {
        let op = self
            .program
            .get(index)
            .ok_or_else(|| BackendError::Failed(format!("the driver asked for input {index}")))?;
        match op["op"].as_str() {
            Some("key") => {
                let chord = Chord::parse(op["chord"].as_str().unwrap_or_default())
                    .map_err(BackendError::Failed)?;
                rfb.press(&chord)?;
            }
            Some("pointer") if op["step"].is_string() => {
                let at = |k: &str| op[k].as_u64().and_then(|v| u16::try_from(v).ok());
                let step = op["step"].as_str().and_then(HeldStep::parse);
                let (step, (x, y)) = step.zip(at("x").zip(at("y"))).ok_or_else(|| {
                    BackendError::Failed(format!("op {index} is no step of a held pointer"))
                })?;
                rfb.held(step, x, y)?;
            }
            Some("pointer") => {
                let at = |k: &str| op[k].as_u64().and_then(|v| u16::try_from(v).ok());
                let (x, y) = at("x").zip(at("y")).ok_or_else(|| {
                    BackendError::Failed(format!("op {index} has no pointer position"))
                })?;
                rfb.click(x, y)?;
            }
            Some("double") => {
                let at = |k: &str| op[k].as_u64().and_then(|v| u16::try_from(v).ok());
                let (x, y) = at("x").zip(at("y")).ok_or_else(|| {
                    BackendError::Failed(format!("op {index} has no pointer position"))
                })?;
                rfb.double_click(x, y)?;
            }
            Some("drag") => {
                let at = |k: &str| op[k].as_u64().and_then(|v| u16::try_from(v).ok());
                let ends = at("x").zip(at("y")).zip(at("x2").zip(at("y2")));
                let (from, to) = ends.ok_or_else(|| {
                    BackendError::Failed(format!("op {index} has no pointer positions"))
                })?;
                rfb.drag(from, to)?;
            }
            Some("type") => rfb.type_text(op["text"].as_str().unwrap_or_default())?,
            other => {
                return Err(BackendError::Failed(format!(
                    "the driver asked for input at op {index}, which is {other:?}"
                )));
            }
        }
        Ok(())
    }

    fn spawn(&self, driver: &Path, timeout: Duration) -> Result<String> {
        let input = if self.sends_input() {
            Some(Endpoint::choose()?)
        } else {
            None
        };
        let platform = input
            .as_ref()
            .map_or_else(|| "offscreen".to_owned(), Endpoint::platform);
        let mut command = input
            .as_ref()
            .map_or_else(|| Command::new(&self.qml), |i| i.command(&self.qml));
        match &self.controls_style {
            Some(style) => command.env("QT_QUICK_CONTROLS_STYLE", style),
            None => command.env_remove("QT_QUICK_CONTROLS_STYLE"),
        };
        super::own_home(&mut command, driver.parent().unwrap_or(driver))?;
        // The style is the scenario's, never the developer's.
        command
            .env_remove("QT_QUICK_CONTROLS_CONF")
            .env_remove("QT_QUICK_CONTROLS_FALLBACK_STYLE");
        if let Some(text) = &self.controls_conf {
            let path = driver.with_file_name("qtquickcontrols2.conf");
            fs::write(&path, text)?;
            command.env("QT_QUICK_CONTROLS_CONF", path);
        }
        for (k, v) in &self.controls_vars {
            command.env(k, v);
        }
        // The developer's own locale stays out: a parity scenario has one
        // expectation.
        for k in LOCALE_VARS {
            command.env_remove(k);
        }
        match &self.locale_env {
            Some(vars) => {
                for (k, v) in vars {
                    command.env(k, v);
                }
            }
            None => {
                command.env("LANG", "en_US.UTF-8");
            }
        }
        command.arg(driver);
        if let Some(name) = &self.command_line_style {
            command.arg("-style").arg(name);
        }
        let mut child = command
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("QT_QPA_PLATFORM", platform)
            .env("QT_QUICK_BACKEND", "software")
            .env("QT_MESSAGE_PATTERN", "%{type}|%{message}")
            // A module the scenario writes (`Kit/qmldir`) is found by its
            // name from the scenario's own directory.
            .env("QML_IMPORT_PATH", driver.parent().unwrap_or(Path::new(".")))
            .env("TZ", "UTC")
            // The desktop's settings stay out: a platform theme that reads
            // them would give the controls a dark palette on a desktop that
            // prefers one, and a parity scenario has one expectation.
            .env("GSETTINGS_BACKEND", "memory")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BackendError::Failed(format!("cannot start {}: {e}", self.qml)))?;
        let pipe = child.stderr.take().expect("stderr is piped");
        // Read on a thread so a chatty app cannot fill the pipe while we wait.
        // The driver's requests for input come out of the same pipe; they are
        // passed on as they arrive and kept out of the log.
        let (requests, wanted) = mpsc::channel::<usize>();
        let reader = thread::spawn(move || {
            let mut text = String::new();
            let mut pipe = BufReader::new(pipe);
            let mut line = Vec::new();
            while pipe.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
                let l = String::from_utf8_lossy(&line);
                if let Some((_, index)) = l.split_once(INPUT_MARKER) {
                    if let Ok(index) = index.trim().parse() {
                        let _ = requests.send(index);
                    }
                } else if !l.contains(VNC_NOTICE) {
                    text.push_str(&l);
                }
                line.clear();
            }
            text
        });
        let deadline = Instant::now() + timeout;
        let mut rfb: Option<Rfb> = None;
        let outcome = loop {
            if child.try_wait()?.is_some() {
                break Ok(());
            }
            if Instant::now() > deadline {
                break Err(BackendError::Failed(format!(
                    "qml6 timed out after {timeout:?}"
                )));
            }
            match wanted.recv_timeout(Duration::from_millis(5)) {
                Ok(index) => {
                    let sent = input
                        .as_ref()
                        .ok_or_else(|| BackendError::Failed("input on a run without it".into()))
                        .and_then(|endpoint| {
                            if rfb.is_none() {
                                rfb = Some(Rfb::connect(endpoint, deadline)?);
                            }
                            let rfb = rfb.as_mut().expect("just connected");
                            self.send_input(rfb, index)
                        });
                    if let Err(e) = sent {
                        break Err(e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                // The pipe closed: the process is on its way out.
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        };
        if outcome.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let text = reader.join().unwrap_or_default();
        outcome.map(|()| text)
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

/// Qt 6's `lrelease`. Distributions that also ship Qt 5 put the Qt 5 tool
/// first on the path, so the Qt 6 names are tried before the bare one.
fn lrelease() -> String {
    if let Ok(tool) = std::env::var("LRELEASE") {
        return tool;
    }
    let candidates = ["lrelease6", "lrelease-qt6", "/usr/lib/qt6/bin/lrelease"];
    candidates
        .iter()
        .find(|c| {
            Command::new(c)
                .arg("-version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
        .map_or_else(|| "lrelease".to_owned(), |c| (*c).to_owned())
}

/// The `language` attribute of the `TS` element.
fn ts_language(ts: &str) -> Option<String> {
    let start = ts.find("<TS")?;
    let tag = &ts[start..start + ts[start..].find('>')?];
    let rest = &tag[tag.find("language=\"")? + "language=\"".len()..];
    let language = &rest[..rest.find('"')?];
    (!language.is_empty()).then(|| language.to_owned())
}

fn file_url(p: &Path) -> String {
    format!("{}/", ljos_engine::file_url(p))
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

    fn controls_style(&mut self, name: &str) -> Result<()> {
        self.controls_style = Some(name.to_owned());
        self.cache = None;
        Ok(())
    }

    fn command_line_style(&mut self, name: &str) -> Result<()> {
        self.command_line_style = Some(name.to_owned());
        self.cache = None;
        Ok(())
    }

    fn controls_configuration(&mut self, text: &str) -> Result<()> {
        self.controls_conf = Some(text.to_owned());
        self.cache = None;
        Ok(())
    }

    fn controls_variable(&mut self, name: &str, value: &str) -> Result<()> {
        self.controls_vars.push((name.to_owned(), value.to_owned()));
        self.cache = None;
        Ok(())
    }

    fn locale_environment(&mut self, vars: &[(String, String)]) -> Result<()> {
        self.locale_env = Some(vars.to_vec());
        self.cache = None;
        Ok(())
    }

    fn run_tests(
        &mut self,
        files: &[QmlFile],
        run: &super::testrun::TestRun,
    ) -> Result<super::testrun::TestReport> {
        super::testrun::run_qt(files, run)
    }

    // Qt always reads the machine's fonts.
    fn system_fonts(&mut self) -> Result<()> {
        Ok(())
    }

    // qml6 takes its key bindings from the desktop, and its `offscreen` and
    // `vnc` platforms ask none: they have the Windows column, whatever the
    // machine (Ctrl+K deletes nothing there, as on Windows).
    fn key_bindings(&mut self, platform: ljos_quick::keys::Platform) -> Result<()> {
        match platform {
            ljos_quick::keys::Platform::Windows => Ok(()),
            _ => Err(BackendError::NotImplemented(
                "another platform's key bindings",
            )),
        }
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

    fn key(&mut self, chord: &str) -> Result<()> {
        // Recorded only, like a wait; a chord the oracle cannot type is
        // reported by the step that names it.
        Chord::parse(chord).map_err(BackendError::Failed)?;
        self.not_held()?;
        self.push(json!({"op": "key", "chord": chord}));
        Ok(())
    }

    fn key_hold(&mut self, _: &str, _: u32) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "auto-repeat: the input platform does not mark repeated presses",
        })
    }

    fn window_active(&mut self, active: bool) -> Result<()> {
        self.push(json!({"op": "active", "value": active}));
        Ok(())
    }

    /// A click of the left mouse button: what the input platform can send,
    /// and what a touch becomes for a `MouseArea` or a `TapHandler`.
    fn touch(&mut self, x: f64, y: f64) -> Result<()> {
        let inside = |v: f64| (0.0..f64::from(SCREEN)).contains(&v);
        if !inside(x) || !inside(y) {
            return Err(BackendError::Failed(format!(
                "{x}, {y} is outside the {SCREEN} pixel window"
            )));
        }
        self.not_held()?;
        self.push(json!({"op": "pointer", "x": x as u16, "y": y as u16}));
        Ok(())
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        let (x, y) = on_screen(x, y)?;
        let held = self.pointer_held();
        let step = match step {
            PointerStep::Press if held => {
                return Err(BackendError::Failed(
                    "the pointer is pressed already".into(),
                ));
            }
            PointerStep::Release if !held => {
                return Err(BackendError::Failed("the pointer is not pressed".into()));
            }
            PointerStep::Press => HeldStep::Press,
            PointerStep::Release => HeldStep::Release,
            PointerStep::Move if held => HeldStep::Move,
            PointerStep::Move => HeldStep::Hover,
        };
        let moves = matches!(step, HeldStep::Move | HeldStep::Hover);
        if moves && self.pointer_at() == Some((u64::from(x), u64::from(y))) {
            // A move to where the pointer is, is no event: nothing would
            // acknowledge it.
            self.push(json!({"op": "nop"}));
            return Ok(());
        }
        self.push(json!({"op": "pointer", "step": step.name(), "x": x, "y": y}));
        Ok(())
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "default accessible names need a QAccessible helper",
        })
    }

    fn a11y_focus(&mut self, _: &str, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "an assistive technology's requests need the accessibility bridge",
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

    fn translation(&mut self, name: &str, ts: &str) -> Result<()> {
        self.translations.insert(name.to_owned(), ts.to_owned());
        self.cache = None;
        Ok(())
    }

    fn set_ui_language(&mut self, language: &str) -> Result<()> {
        self.ask(json!({"op": "language", "value": language}))
            .map(drop)
    }

    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.resources.insert(name.to_owned(), bytes.to_vec());
        self.cache = None;
        Ok(())
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        if let Some(c) = text.chars().find(|c| char_keysym(*c).is_none()) {
            return Err(BackendError::Failed(format!(
                "the Qt oracle cannot type {c:?}"
            )));
        }
        self.not_held()?;
        self.push(json!({"op": "type", "text": text}));
        Ok(())
    }

    fn double_click(&mut self, x: f64, y: f64) -> Result<()> {
        let (x, y) = on_screen(x, y)?;
        self.not_held()?;
        self.push(json!({"op": "double", "x": x, "y": y}));
        Ok(())
    }

    fn drag(&mut self, from: (f64, f64), to: (f64, f64)) -> Result<()> {
        let (x, y) = on_screen(from.0, from.1)?;
        let (x2, y2) = on_screen(to.0, to.1)?;
        self.not_held()?;
        self.push(json!({"op": "drag", "x": x, "y": y, "x2": x2, "y2": y2}));
        Ok(())
    }

    fn wheel(&mut self, _: (f64, f64), _: super::WheelBy) -> Result<()> {
        // RFB has wheel buttons (4 and 5), and Qt's `vnc` platform makes no
        // event of them (checked on Qt 6.11).
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "wheel events: the input platform has none",
        })
    }

    fn set_clipboard(&mut self, text: &str) -> Result<()> {
        self.ask(json!({"op": "copy", "value": text})).map(drop)
    }

    fn set_clipboard_html(&mut self, html: &str) -> Result<()> {
        self.ask(json!({"op": "copyHtml", "value": html})).map(drop)
    }

    fn clipboard(&mut self) -> Result<String> {
        Ok(self
            .ask(json!({"op": "paste"}))?
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    fn compose(&mut self, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "an input method: the input platform has none",
        })
    }

    fn commit(&mut self, _: &str) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: "qt",
            what: "an input method: the input platform has none",
        })
    }
}

/// A point of the window in whole pixels, as the input channel sends it.
fn on_screen(x: f64, y: f64) -> Result<(u16, u16)> {
    let inside = |v: f64| (0.0..f64::from(SCREEN)).contains(&v);
    if !inside(x) || !inside(y) {
        return Err(BackendError::Failed(format!(
            "{x}, {y} is outside the {SCREEN} pixel window"
        )));
    }
    Ok((x as u16, y as u16))
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
    fn language_comes_from_the_ts_element() {
        let ts = "<?xml version=\"1.0\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"is_IS\">\n</TS>";
        assert_eq!(ts_language(ts).as_deref(), Some("is_IS"));
        assert_eq!(ts_language("<TS version=\"2.1\">"), None);
    }

    #[test]
    fn missing_result_is_an_error() {
        assert!(parse_stderr("warning|boom\n").is_err());
    }

    #[test]
    fn a_held_pointer_is_recorded_step_by_step() {
        let mut qt = QtBackend::new();
        qt.pointer(PointerStep::Move, 10.0, 10.0).unwrap();
        qt.pointer(PointerStep::Press, 10.0, 20.0).unwrap();
        qt.pointer(PointerStep::Move, 10.0, 40.0).unwrap();
        // No event comes of a move to where the pointer is.
        qt.pointer(PointerStep::Move, 10.0, 40.0).unwrap();
        qt.pointer(PointerStep::Release, 10.0, 40.0).unwrap();
        let steps: Vec<_> = qt
            .program
            .iter()
            .map(|op| op["step"].as_str().unwrap_or("-"))
            .collect();
        assert_eq!(steps, ["hover", "press", "move", "-", "release"]);
        assert!(qt.holds_pointer());
        assert!(!qt.pointer_held());
    }

    #[test]
    fn nothing_else_is_sent_while_the_pointer_is_held() {
        let mut qt = QtBackend::new();
        assert!(qt.pointer(PointerStep::Release, 10.0, 10.0).is_err());
        qt.pointer(PointerStep::Press, 10.0, 10.0).unwrap();
        assert!(qt.pointer(PointerStep::Press, 10.0, 10.0).is_err());
        assert!(matches!(
            qt.key("Tab"),
            Err(BackendError::Unsupported { .. })
        ));
        assert!(matches!(
            qt.touch(5.0, 5.0),
            Err(BackendError::Unsupported { .. })
        ));
        qt.pointer(PointerStep::Release, 10.0, 10.0).unwrap();
        qt.key("Tab").unwrap();
        // The key's acknowledgement left the pointer in the corner.
        assert_eq!(qt.pointer_at(), None);
    }
}
