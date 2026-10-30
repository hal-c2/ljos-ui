//! The authoring tools as an agent and an author meet them
//! (`features/tooling`): a project in a directory of its own, `ljos mcp`
//! started on it and spoken to over its pipes, and `ljos` run in it.
//!
//! The program is a child and not a library. The tools run this crate's own
//! scenarios (`features_run`), so the crate that has them depends on this
//! one and cannot be linked here; and a scenario should meet what an
//! author's agent meets, which is the program.

use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Component, Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        mpsc::{Receiver, RecvTimeoutError, channel},
    },
    time::Duration,
};

use ljos_child::Reaped;
use serde_json::{Value, json};

mod desktop;
mod rate;
pub use desktop::Setting;

use crate::backend::{
    inspector::{self, Answer, Ask, Refusal, Seen},
    testrun,
};

/// How long the server may take over one request. A scenario that waits for
/// a run asks for 20 seconds of it.
const ANSWER: Duration = Duration::from_secs(60);
/// How long the server may take to leave once nobody talks to it.
const LEAVE: Duration = Duration::from_secs(5);

const NOT_COUNTED: &str =
    "the wakes were not counted; use `When the wakes of the application's threads are counted`";

/// The first line of a doc string, when it only says what the text is.
const LABELS: [&str; 8] = [
    "qml", "xml", "json", "text", "markdown", "toml", "gherkin", "sh",
];

/// A doc string as the file or the text it stands for: without its label
/// and without the spaces it is indented by.
#[must_use]
pub fn text_of(doc: &str) -> String {
    let body = match doc.split_once('\n') {
        Some((first, rest)) if LABELS.contains(&first.trim()) => rest,
        _ => doc,
    };
    inspector::dedent(body)
}

/// What `ljos` said when the author ran it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ran {
    pub status: i32,
    /// Its output and its errors, in the order they were written.
    pub output: String,
}

impl Ran {
    /// # Errors
    /// The command printed something else.
    pub fn printed(&self, want: &str) -> Result<(), String> {
        if same_text(&self.output, want) {
            Ok(())
        } else {
            Err(format!("the command printed:\n{}", self.output))
        }
    }

    /// # Errors
    /// The command ended with another status.
    pub fn exited(&self, status: i32) -> Result<(), String> {
        if self.status == status {
            Ok(())
        } else {
            Err(format!(
                "the command exited with status {}, not {status}:\n{}",
                self.status, self.output
            ))
        }
    }

    /// # Errors
    /// `text` is in the output though `present` is false, or the reverse.
    pub fn mentions(&self, text: &str, present: bool) -> Result<(), String> {
        match (self.output.contains(text), present) {
            (true, true) | (false, false) => Ok(()),
            (false, true) => Err(format!(
                "{text:?} is not in what the command printed:\n{}",
                self.output
            )),
            (true, false) => Err(format!(
                "{text:?} is in what the command printed:\n{}",
                self.output
            )),
        }
    }
}

/// Whether two texts are the same but for the line ends a doc string
/// cannot carry: the last one, and spaces before one.
fn same_text(actual: &str, want: &str) -> bool {
    let lines = |text: &str| -> Vec<String> {
        text.trim_end_matches('\n')
            .lines()
            .map(|l| l.trim_end().to_owned())
            .collect()
    };
    lines(actual) == lines(want)
}

/// The tools a prompt names, in the order it first names them: the words in
/// backticks that have an underscore and nothing but small letters.
#[must_use]
pub fn tools_named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (index, word) in text.split('`').enumerate() {
        let tool = index % 2 == 1
            && word.contains('_')
            && word.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        if tool && !out.iter().any(|known| known == word) {
            out.push(word.to_owned());
        }
    }
    out
}

/// `ljos mcp` or `ljos attach`, running, and the pipe it answers on.
struct Server {
    /// The command, as errors name it.
    name: &'static str,
    child: Reaped,
    /// `None` once the pipe is closed, which is how the server is told to
    /// leave.
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next: u64,
}

impl Server {
    /// `ljos mcp` on the project.
    fn start(dir: &Path, caps: &str, runner: Option<&Path>) -> Result<(Self, Value), String> {
        let bin = testrun::ljos_bin().map_err(|e| e.to_string())?;
        let mut command = Command::new(bin);
        command.arg("mcp").arg("--project").arg(dir);
        if !caps.is_empty() {
            command.arg("--caps").arg(caps);
        }
        if let Some(runner) = runner {
            command.arg("--features-runner").arg(runner);
        }
        Self::speak(command.current_dir(dir), "`ljos mcp`")
    }

    /// `ljos attach` on the inspector whose token is in `token_file`, as
    /// an agent that starts its servers as programs reaches a running
    /// application.
    fn attach(dir: &Path, token_file: &Path) -> Result<(Self, Value), String> {
        let bin = testrun::ljos_bin().map_err(|e| e.to_string())?;
        let mut command = Command::new(bin);
        command.arg("attach").arg(token_file);
        Self::speak(command.current_dir(dir), "`ljos attach`")
    }

    /// Starts `command` and greets it as an MCP client does.
    fn speak(command: &mut Command, name: &'static str) -> Result<(Self, Value), String> {
        let mut child = Reaped::spawn(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                // The log is the server's own; what an agent sees is the answers.
                .stderr(Stdio::null()),
        )
        .map_err(|e| format!("cannot start {name}: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("{name} has no output"))?;
        let (send, lines) = channel();
        // A read cannot be given up, so a thread reads and the requests
        // wait on the channel, where they can. It ends with the pipe.
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = Server {
            name,
            child,
            stdin,
            lines,
            next: 0,
        };
        let hello = server
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "ljos_bdd", "version": "0" },
                }),
            )?
            .map_err(|e| format!("{name} refused the client: {e}"))?;
        server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        Ok((server, hello))
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let name = self.name;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| format!("{name} was told to leave"))?;
        writeln!(stdin, "{message}")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("{name} does not listen: {e}"))
    }

    /// One request and its answer: the result, or the protocol's error.
    fn request(&mut self, method: &str, params: Value) -> Result<Result<Value, Value>, String> {
        let name = self.name;
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let line = self.lines.recv_timeout(ANSWER).map_err(|e| match e {
                RecvTimeoutError::Timeout => {
                    format!("{name} did not answer `{method}` in {} s", ANSWER.as_secs())
                }
                RecvTimeoutError::Disconnected => {
                    format!("{name} ended before it answered `{method}`")
                }
            })?;
            let Ok(mut message) = serde_json::from_str::<Value>(&line) else {
                return Err(format!("{name} wrote what is not JSON: {line}"));
            };
            // Anything else on the pipe is a notification, which no step
            // asks about.
            if message["id"] != json!(id) || message.get("method").is_some() {
                continue;
            }
            return Ok(match message.get_mut("error") {
                Some(error) => Err(error.take()),
                None => Ok(message["result"].take()),
            });
        }
    }

    /// Every page of a list.
    fn list(&mut self, method: &str, key: &str) -> Result<Vec<Value>, String> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor.map_or_else(|| json!({}), |c| json!({ "cursor": c }));
            let mut page = self
                .request(method, params)?
                .map_err(|e| format!("`{method}` failed: {e}"))?;
            if let Value::Array(items) = page[key].take() {
                all.extend(items);
            }
            match page["nextCursor"].as_str() {
                Some(next) => cursor = Some(next.to_owned()),
                None => return Ok(all),
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Closing the pipe lets the server end as it does for an agent that
        // goes away, which is when it stops a run it started. Killing it
        // would leave that run behind.
        self.stdin = None;
        loop {
            match self.lines.recv_timeout(LEAVE) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.child.kill();
                    break;
                }
            }
        }
        let _ = self.child.wait();
    }
}

/// A refusal as the protocol carries one outside a tool's answer.
fn protocol_refusal(error: &Value) -> Refusal {
    let data = &error["data"];
    // The number is all there is when the protocol itself refused.
    let by_number = match error["code"].as_i64() {
        Some(-32002) => "not_found",
        Some(-32602) => "invalid_argument",
        _ => "internal",
    };
    Refusal {
        code: data["code"].as_str().unwrap_or(by_number).to_owned(),
        message: data["message"]
            .as_str()
            .or_else(|| error["message"].as_str())
            .unwrap_or_default()
            .to_owned(),
        details: data["details"].clone(),
    }
}

fn refused(refusal: Refusal) -> Answer {
    Answer {
        refusal: Some(refusal),
        ..Answer::default()
    }
}

/// The texts of a list of content blocks, one after the other.
fn texts<'a>(blocks: impl Iterator<Item = &'a Value>) -> String {
    blocks
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_answer(mut result: Value) -> Answer {
    let text = texts(result["content"].as_array().into_iter().flatten());
    let structured = result["structuredContent"].take();
    if result["isError"] != json!(true) {
        let picture = result["content"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|block| block["type"] == "image")
            .and_then(|block| ljos_mcp_core::base64_decode(block["data"].as_str()?));
        return Answer {
            structured,
            text,
            picture,
            ..Answer::default()
        };
    }
    let error = &structured["error"];
    Answer {
        refusal: Some(Refusal {
            code: error["code"].as_str().unwrap_or_default().to_owned(),
            message: error["message"].as_str().unwrap_or(&text).to_owned(),
            details: error["details"].clone(),
        }),
        text,
        ..Answer::default()
    }
}

/// `name` under `dir`, if it names something below it.
fn below(dir: &Path, name: &str) -> Option<PathBuf> {
    let inside = Path::new(name)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    (inside && !name.is_empty()).then(|| dir.join(name))
}

/// The project of one scenario and what was done in it.
struct Rig {
    dir: tempfile::TempDir,
    /// Beside the project, for a link of the project to lead out to.
    outside: tempfile::TempDir,
    /// The staged text each file was last written from. A file is written
    /// again only when the scenario gives it anew, so that what a tool
    /// wrote stays.
    written: BTreeMap<String, String>,
    server: Option<Server>,
    seen: Seen,
    ran: Option<Ran>,
    /// What the author's desktop says to the commands they run.
    settings: desktop::Settings,
    /// The application the author started and left running.
    app: Option<App>,
}

/// An application started in the background (`When the author starts`),
/// which the scenario's agent reaches through its inspector.
struct App {
    /// Killed with the scenario.
    child: Reaped,
    /// What it wrote to its errors so far, line by line.
    errors: Arc<Mutex<Vec<String>>>,
    /// The scenario's desktop, which lives as long as the application.
    _desktop: Option<desktop::Desktop>,
    /// Each thread's wakes when they began to be counted.
    wakes_from: Option<BTreeMap<u32, (String, u64)>>,
    /// The token file its inspector wrote.
    token_file: PathBuf,
    /// What the last burst of calls to its inspector came to.
    burst: Option<rate::Burst>,
}

impl App {
    fn errors(&self) -> String {
        lock(&self.errors).join("\n")
    }
}

impl Rig {
    fn new() -> Result<Self, String> {
        let dir = tempfile::Builder::new()
            .prefix("ljos-bdd-author-")
            .tempdir()
            .map_err(|e| format!("cannot make a directory for the project: {e}"))?;
        let outside = tempfile::Builder::new()
            .prefix("ljos-bdd-outside-")
            .tempdir()
            .map_err(|e| format!("cannot make a directory outside the project: {e}"))?;
        Ok(Rig {
            dir,
            outside,
            written: BTreeMap::new(),
            server: None,
            seen: Seen::default(),
            ran: None,
            settings: desktop::Settings::new(),
            app: None,
        })
    }

    /// The place of a file of the project, which must be inside it.
    fn path(&self, name: &str) -> Result<PathBuf, String> {
        below(self.dir.path(), name)
            .ok_or_else(|| format!("{name:?} is not a file of the scenario's project"))
    }

    /// The place of a file beside the project, which must be inside that.
    fn outside_path(&self, name: &str) -> Result<PathBuf, String> {
        below(self.outside.path(), name)
            .ok_or_else(|| format!("{name:?} is not a file beside the scenario's project"))
    }

    /// Writes the files the scenario gave since the last time.
    fn sync(&mut self, files: &BTreeMap<String, String>) -> Result<(), String> {
        for (name, source) in files {
            if self.written.get(name) == Some(source) {
                continue;
            }
            let path = self.path(name)?;
            let failed = |e: std::io::Error| format!("cannot write {name:?}: {e}");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(failed)?;
            }
            let mut text = text_of(source);
            text.push('\n');
            fs::write(&path, text).map_err(failed)?;
            #[cfg(unix)]
            if name.ends_with(".sh") {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(failed)?;
            }
            self.written.insert(name.clone(), source.clone());
        }
        Ok(())
    }

    fn connect(&mut self, caps: &str, runner: Option<&str>) -> Result<(), String> {
        let runner = runner.map(|name| self.path(name)).transpose()?;
        // One agent at a time: the one before goes first.
        self.server = None;
        let (server, hello) = Server::start(self.dir.path(), caps, runner.as_deref())?;
        self.greeted(server, &hello)
    }

    /// `server` answered the greeting with `hello`: what it offers is what
    /// the agent has seen.
    fn greeted(&mut self, mut server: Server, hello: &Value) -> Result<(), String> {
        let tools = server.list("tools/list", "tools")?;
        self.seen = Seen {
            tools: tools
                .iter()
                .map(|tool| {
                    (
                        tool["name"].as_str().unwrap_or_default().to_owned(),
                        tool["annotations"]["readOnlyHint"].as_bool(),
                    )
                })
                .collect(),
            instructions: hello["instructions"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            ..Seen::default()
        };
        self.server = Some(server);
        Ok(())
    }

    fn server(&mut self) -> Result<&mut Server, String> {
        self.server.as_mut().ok_or_else(|| {
            "the authoring tools are not connected; use `Given the authoring tools are connected`"
                .to_owned()
        })
    }

    fn ask(&mut self, ask: &Ask) -> Result<(), String> {
        let answer = match ask {
            Ask::Call { tool, args } => {
                let params = json!({ "name": tool, "arguments": args });
                match self.server()?.request("tools/call", params)? {
                    Ok(result) => tool_answer(result),
                    Err(error) => refused(protocol_refusal(&error)),
                }
            }
            Ask::Resource(uri) => {
                match self
                    .server()?
                    .request("resources/read", json!({ "uri": uri }))?
                {
                    Ok(result) => Answer {
                        text: texts(result["contents"].as_array().into_iter().flatten()),
                        ..Answer::default()
                    },
                    Err(error) => refused(protocol_refusal(&error)),
                }
            }
            other => {
                return Err(format!(
                    "the authoring tools have no application to ask: {other:?}"
                ));
            }
        };
        self.seen.answer = Some(answer);
        Ok(())
    }

    fn prompt(&mut self, name: &str, args: &Value) -> Result<(), String> {
        let params = json!({ "name": name, "arguments": args });
        let answer = match self.server()?.request("prompts/get", params)? {
            Ok(result) => Answer {
                text: texts(
                    result["messages"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|message| &message["content"]),
                ),
                ..Answer::default()
            },
            Err(error) => refused(protocol_refusal(&error)),
        };
        self.seen.answer = Some(answer);
        Ok(())
    }

    fn resources(&mut self) -> Result<(), String> {
        let listed = self.server()?.list("resources/list", "resources")?;
        let uris: Vec<&str> = listed.iter().filter_map(|r| r["uri"].as_str()).collect();
        self.seen.answer = Some(Answer {
            text: uris.join("\n"),
            ..Answer::default()
        });
        Ok(())
    }

    fn prompts(&mut self) -> Result<Vec<String>, String> {
        let prompts = self.server()?.list("prompts/list", "prompts")?;
        Ok(prompts
            .iter()
            .filter_map(|prompt| prompt["name"].as_str().map(str::to_owned))
            .collect())
    }

    /// `line` as a command run in the project, with the scenario's
    /// desktop when it set something, which the command must keep.
    fn command(&self, line: &str) -> Result<(Command, Option<desktop::Desktop>), String> {
        let mut words = line.split_whitespace();
        let bin = match words.next() {
            Some("ljos") => testrun::ljos_bin(),
            Some("ljos-fuzz") => testrun::fuzz_bin(),
            _ => {
                return Err(format!(
                    "the author runs `ljos` or `ljos-fuzz` and nothing else, not {line:?}"
                ));
            }
        }
        .map_err(|e| e.to_string())?;
        // The command meets the scenario's desktop and not the developer's:
        // a bus with a portal when the scenario set something, else a bus
        // that is not there, as on a machine without a desktop. Nor is
        // there a display, so that nothing a scenario runs can open a
        // window on the developer's screen.
        let desktop = if self.settings.is_empty() {
            None
        } else {
            Some(desktop::Desktop::start(
                self.outside.path(),
                &self.settings,
            )?)
        };
        let bus = desktop.as_ref().map_or_else(
            || {
                format!(
                    "unix:path={}",
                    self.outside.path().join("no-session-bus").display()
                )
            },
            |d| d.address.clone(),
        );
        let mut command = Command::new(bin);
        command
            .args(words)
            .current_dir(self.dir.path())
            .env("DBUS_SESSION_BUS_ADDRESS", bus)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .stdin(Stdio::null());
        Ok((command, desktop))
    }

    fn run(&mut self, line: &str) -> Result<(), String> {
        let (command, _desktop) = self.command(line)?;
        // One pipe for the output and the errors keeps them in the order
        // they were written, which is what a terminal shows.
        let mut sh = Command::new("sh");
        sh.args(["-c", r#"exec "$0" "$@" 2>&1"#])
            .arg(command.get_program())
            .args(command.get_args())
            .current_dir(self.dir.path())
            .stdin(Stdio::null());
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => sh.env(key, value),
                None => sh.env_remove(key),
            };
        }
        let out = sh
            .output()
            .map_err(|e| format!("cannot start `{line}`: {e}"))?;
        let status = out
            .status
            .code()
            .ok_or_else(|| format!("`{line}` was killed"))?;
        self.ran = Some(Ran {
            status,
            output: String::from_utf8_lossy(&out.stdout).into_owned(),
        });
        Ok(())
    }

    /// Starts `line` and leaves it running, and connects the agent to the
    /// inspector it names on its errors, as a parent process finds it.
    fn start(&mut self, line: &str) -> Result<(), String> {
        // One application at a time, and one agent: those before go first.
        self.server = None;
        self.app = None;
        let (mut command, desktop) = self.command(line)?;
        let mut child = Reaped::spawn(command.stdout(Stdio::null()).stderr(Stdio::piped()))
            .map_err(|e| format!("cannot start `{line}`: {e}"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("`{line}` has no errors to read"))?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let (send, lines) = channel();
        {
            let errors = Arc::clone(&errors);
            // The pipe is read to its end, so that the application never
            // waits on a full one; the lines it wrote are kept for the steps.
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    lock(&errors).push(line.clone());
                    // Nobody listens once the inspector was found.
                    let _ = send.send(line);
                }
            });
        }
        let mut app = App {
            child,
            errors,
            _desktop: desktop,
            wakes_from: None,
            token_file: PathBuf::new(),
            burst: None,
        };
        let token_file = loop {
            match lines.recv_timeout(ANSWER) {
                Ok(said) => {
                    if let Some(file) = inspector_token_file(&said) {
                        break file;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "`{line}` named no inspector in {} s; it wrote:\n{}",
                        ANSWER.as_secs(),
                        app.errors()
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "`{line}` ended before it named its inspector; it wrote:\n{}",
                        app.errors()
                    ));
                }
            }
        };
        app.token_file = self.dir.path().join(token_file);
        let (server, hello) = Server::attach(self.dir.path(), &app.token_file)?;
        self.app = Some(app);
        self.greeted(server, &hello)
    }

    fn app(&mut self) -> Result<&mut App, String> {
        self.app.as_mut().ok_or_else(|| {
            "the author has started no application; use `When the author starts`".to_owned()
        })
    }
}

#[cfg(unix)]
fn link_dir(to: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(to, link)
}

#[cfg(not(unix))]
fn link_dir(_to: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

fn octal(mode: &str) -> Result<u32, String> {
    u32::from_str_radix(mode, 8).map_err(|e| format!("the mode {mode} is not octal: {e}"))
}

#[cfg(unix)]
fn permissions(bits: u32) -> std::fs::Permissions {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::Permissions::from_mode(bits)
}

#[cfg(unix)]
fn has_mode(path: &Path, mode: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let want = octal(mode)?;
    let got = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "{} has the mode {got:o}, not {mode}",
            path.display()
        ))
    }
}

/// Locks `m` whether or not the thread that reads into it went down.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The token file the line `ljos run` writes to its errors when it serves
/// its inspector names: `ljos: inspector (--mcp=…) at URL, token in FILE`.
fn inspector_token_file(line: &str) -> Option<PathBuf> {
    let rest = line.strip_prefix("ljos: inspector (")?;
    let (_, file) = rest.split_once(", token in ")?;
    Some(PathBuf::from(file.trim()))
}

/// Each thread of process `pid` by its id: its name, and how many times
/// it went to sleep or was made to give way.
fn thread_wakes(pid: u32) -> Result<BTreeMap<u32, (String, u64)>, String> {
    let failed = |e: std::io::Error| format!("cannot read the threads of process {pid}: {e}");
    let mut threads = BTreeMap::new();
    for task in fs::read_dir(format!("/proc/{pid}/task")).map_err(failed)? {
        let task = task.map_err(failed)?;
        let Some(tid) = task.file_name().to_str().and_then(|t| t.parse().ok()) else {
            continue;
        };
        // A thread that ended between the listing and the read is gone.
        let Ok(status) = fs::read_to_string(task.path().join("status")) else {
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

/// The authoring side of a scenario: nothing until a step asks for it.
#[derive(Default)]
pub struct Slot(Option<Rig>);

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("project", &self.0.as_ref().map(|rig| rig.dir.path()))
            .finish_non_exhaustive()
    }
}

/// The files a scenario has given, by name, as the world keeps them.
pub type Files = BTreeMap<String, String>;

impl Slot {
    /// The project, with every file the scenario has given so far.
    fn rig(&mut self, files: &Files) -> Result<&mut Rig, String> {
        let rig = match &mut self.0 {
            Some(rig) => rig,
            empty => empty.insert(Rig::new()?),
        };
        rig.sync(files)?;
        Ok(rig)
    }

    /// Whether an agent of the authoring tools is the one the scenario's
    /// agent steps speak for.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.0.as_ref().is_some_and(|rig| rig.server.is_some())
    }

    /// Starts `ljos mcp` on the project with `caps` (comma separated) and,
    /// when given, the file of the project that runs features.
    ///
    /// # Errors
    /// The program cannot be built or started, or does not speak MCP.
    pub fn connect(
        &mut self,
        files: &Files,
        caps: &str,
        runner: Option<&str>,
    ) -> Result<(), String> {
        self.rig(files)?.connect(caps, runner)
    }

    /// Calls a tool or reads a resource. A refusal is an answer.
    ///
    /// # Errors
    /// No agent is connected, the server does not answer, or `ask` is a
    /// question for a running application.
    pub fn ask(&mut self, files: &Files, ask: &Ask) -> Result<(), String> {
        self.rig(files)?.ask(ask)
    }

    /// Asks for a prompt. Its text, or its refusal, is the answer.
    ///
    /// # Errors
    /// No agent is connected or the server does not answer.
    pub fn prompt(&mut self, files: &Files, name: &str, args: &Value) -> Result<(), String> {
        self.rig(files)?.prompt(name, args)
    }

    /// Lists the resources. Their URIs, one a line, are the answer.
    ///
    /// # Errors
    /// No agent is connected or the server does not answer.
    pub fn resources(&mut self, files: &Files) -> Result<(), String> {
        self.rig(files)?.resources()
    }

    /// The names of the prompts the agent is offered, in order.
    ///
    /// # Errors
    /// No agent is connected or the server does not answer.
    pub fn prompts(&mut self, files: &Files) -> Result<Vec<String>, String> {
        self.rig(files)?.prompts()
    }

    /// What the agent has seen.
    ///
    /// # Errors
    /// No agent is connected.
    pub fn seen(&self) -> Result<&Seen, String> {
        self.0
            .as_ref()
            .filter(|rig| rig.server.is_some())
            .map(|rig| &rig.seen)
            .ok_or_else(|| "the authoring tools are not connected".to_owned())
    }

    /// Runs `ljos …` in the project, as the author would in a terminal.
    ///
    /// # Errors
    /// The line is not an `ljos` command, or the program cannot be run.
    pub fn run(&mut self, files: &Files, line: &str) -> Result<(), String> {
        self.rig(files)?.run(line)
    }

    /// Starts `ljos …` in the project and leaves it running, and connects
    /// the scenario's agent to the inspector it serves.
    ///
    /// # Errors
    /// The line is not an `ljos` command, or the program cannot be run,
    /// names no inspector, or its inspector does not speak MCP.
    pub fn start(&mut self, files: &Files, line: &str) -> Result<(), String> {
        self.rig(files)?.start(line)
    }

    /// Whether an application the author started runs, so that what the
    /// scenario changes or waits for is that application's.
    #[must_use]
    pub fn started(&self) -> bool {
        self.0.as_ref().is_some_and(|rig| rig.app.is_some())
    }

    /// Writes the files the scenario changed into the project, as an
    /// editor saves them.
    ///
    /// # Errors
    /// A file cannot be written.
    pub fn save(&mut self, files: &Files) -> Result<(), String> {
        self.rig(files).map(drop)
    }

    /// Whether the application's errors have a line with `text` in it.
    ///
    /// # Errors
    /// No application was started, or none of its lines has the text.
    pub fn errors_mention(&mut self, text: &str) -> Result<(), String> {
        let app = self.app()?;
        let errors = app.errors();
        if errors.contains(text) {
            Ok(())
        } else {
            Err(format!(
                "the errors do not mention {text:?}; they are:\n{errors}"
            ))
        }
    }

    /// Makes `count` calls to the started application's inspector within
    /// one second, as one client makes them.
    ///
    /// # Errors
    /// No application was started, a call was not answered at all, or the
    /// calls took longer than the second.
    pub fn call_inspector(&mut self, count: usize) -> Result<(), String> {
        let app = self.app()?;
        app.burst = Some(rate::burst(&app.token_file, count)?);
        Ok(())
    }

    /// Whether the first `first` calls of the burst were answered.
    ///
    /// # Errors
    /// No burst was made, or one of them was refused.
    pub fn calls_answered(&mut self, first: usize) -> Result<(), String> {
        self.burst()?.answered(first)
    }

    /// Whether call `nth` of the burst was refused with `status` and the
    /// header line `header`.
    ///
    /// # Errors
    /// No burst was made, or the call was answered otherwise.
    pub fn call_refused(&mut self, nth: usize, status: u16, header: &str) -> Result<(), String> {
        self.burst()?.refused(nth, status, header)
    }

    fn burst(&mut self) -> Result<&rate::Burst, String> {
        self.app()?.burst.as_ref().ok_or_else(|| {
            "no calls were made; use `When a client makes N calls to the started \
             application's inspector within one second`"
                .to_owned()
        })
    }

    /// Whether the started application's token file states `rate` calls a
    /// second.
    ///
    /// # Errors
    /// No application was started, or its file states another rate.
    pub fn token_file_rate(&mut self, rate: u64) -> Result<(), String> {
        let stated = rate::stated(&self.app()?.token_file)?;
        if stated.as_u64() == Some(rate) {
            Ok(())
        } else {
            Err(format!(
                "the token file states {stated} calls a second, not {rate}"
            ))
        }
    }

    /// Makes a directory of the project with `mode` (octal), whatever the
    /// process's umask would have made of it.
    ///
    /// # Errors
    /// The mode is not octal, or the directory cannot be made.
    pub fn make_dir(&mut self, files: &Files, dir: &str, mode: &str) -> Result<(), String> {
        let bits = octal(mode)?;
        let path = self.rig(files)?.dir.path().join(dir);
        std::fs::create_dir_all(&path)
            .and_then(|()| std::fs::set_permissions(&path, permissions(bits)))
            .map_err(|e| format!("cannot make {}: {e}", path.display()))
    }

    /// Makes `dir` of the project a link to `to`, another of its directories.
    ///
    /// # Errors
    /// The link cannot be made, or this is not a Unix.
    pub fn make_dir_link(&mut self, files: &Files, dir: &str, to: &str) -> Result<(), String> {
        let root = self.rig(files)?.dir.path().to_path_buf();
        let path = root.join(dir);
        let made = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| link_dir(&root.join(to), &path));
        made.map_err(|e| format!("cannot link {}: {e}", path.display()))
    }

    /// Whether a directory of the project has nothing in it.
    ///
    /// # Errors
    /// It cannot be read, or something is in it.
    pub fn dir_holds_no_file(&mut self, files: &Files, dir: &str) -> Result<(), String> {
        let path = self.rig(files)?.dir.path().join(dir);
        let held: Vec<_> = std::fs::read_dir(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        if held.is_empty() {
            Ok(())
        } else {
            Err(format!("{} holds {held:?}", path.display()))
        }
    }

    /// Whether a directory of the project has `mode` (octal).
    ///
    /// # Errors
    /// It is not there, or has another mode.
    pub fn dir_has_mode(&mut self, files: &Files, dir: &str, mode: &str) -> Result<(), String> {
        let path = self.rig(files)?.dir.path().join(dir);
        has_mode(&path, mode)
    }

    /// Whether the started application's token file has `mode` (octal).
    ///
    /// # Errors
    /// No application was started, or the file has another mode.
    pub fn token_file_has_mode(&mut self, mode: &str) -> Result<(), String> {
        has_mode(&self.app()?.token_file, mode)
    }

    /// Notes how often each of the application's threads has woken.
    ///
    /// # Errors
    /// No application was started, or its threads cannot be read.
    pub fn count_wakes(&mut self) -> Result<(), String> {
        let app = self.app()?;
        app.wakes_from = Some(thread_wakes(app.child.id())?);
        Ok(())
    }

    /// The threads other than the main one that woke since the wakes were
    /// counted, by name, with how many times.
    ///
    /// # Errors
    /// No application was started, its wakes were not counted, or its
    /// threads cannot be read.
    pub fn wakes_counted(&mut self) -> Result<Vec<(String, u64)>, String> {
        let app = self.app()?;
        let pid = app.child.id();
        let from = app.wakes_from.as_ref().ok_or(NOT_COUNTED)?;
        Ok(thread_wakes(pid)?
            .into_iter()
            .filter(|(tid, _)| *tid != pid)
            .filter_map(|(tid, (name, n))| {
                let was = from.get(&tid).map_or(0, |(_, w)| *w);
                let woke = n.saturating_sub(was);
                (woke > 0).then_some((name, woke))
            })
            .collect())
    }

    /// How many times the main thread, which runs the application's
    /// loop, woke since the wakes were counted.
    ///
    /// # Errors
    /// As [`Slot::wakes_counted`].
    pub fn main_wakes_counted(&mut self) -> Result<u64, String> {
        let app = self.app()?;
        let pid = app.child.id();
        let from = app.wakes_from.as_ref().ok_or(NOT_COUNTED)?;
        let was = from.get(&pid).map_or(0, |(_, w)| *w);
        let now = thread_wakes(pid)?.get(&pid).map_or(0, |(_, w)| *w);
        Ok(now.saturating_sub(was))
    }

    fn app(&mut self) -> Result<&mut App, String> {
        self.0
            .as_mut()
            .ok_or_else(|| "the author has started no application".to_owned())?
            .app()
    }

    /// Sets what the author's desktop says for `key` of `namespace` to
    /// the commands they run from now on.
    ///
    /// # Errors
    /// The project cannot be made.
    pub fn desktop_setting(
        &mut self,
        files: &Files,
        namespace: &str,
        key: &str,
        value: Setting,
    ) -> Result<(), String> {
        self.rig(files)?
            .settings
            .insert((namespace.to_owned(), key.to_owned()), value);
        Ok(())
    }

    /// What the last command said.
    ///
    /// # Errors
    /// The author has not run anything.
    pub fn ran(&self) -> Result<&Ran, String> {
        self.0
            .as_ref()
            .and_then(|rig| rig.ran.as_ref())
            .ok_or_else(|| "the author has not run anything; use `When the author runs`".to_owned())
    }

    /// Checks a file of the project against the text of a doc string.
    ///
    /// # Errors
    /// The file cannot be read or holds something else.
    pub fn file_is(&mut self, files: &Files, name: &str, want: &str) -> Result<(), String> {
        let rig = self.rig(files)?;
        let path = rig.path(name)?;
        let actual = fs::read_to_string(&path).map_err(|e| format!("cannot read {name:?}: {e}"))?;
        if same_text(&actual, want) {
            Ok(())
        } else {
            Err(format!("the file {name:?} is:\n{actual}"))
        }
    }

    /// Where a file or directory of the project is, with every file the
    /// scenario has given so far written.
    ///
    /// # Errors
    /// The name is outside the project, or a file cannot be written.
    pub fn project_path(&mut self, files: &Files, name: &str) -> Result<PathBuf, String> {
        self.rig(files)?.path(name)
    }

    /// Writes a file outside the project, for a link of the project to
    /// lead to.
    ///
    /// # Errors
    /// The name climbs out, or the file cannot be written.
    pub fn outside_file(&mut self, files: &Files, name: &str, text: &str) -> Result<(), String> {
        let path = self.rig(files)?.outside_path(name)?;
        let failed = |e: std::io::Error| format!("cannot write {name:?}: {e}");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }
        fs::write(&path, format!("{}\n", text_of(text))).map_err(failed)
    }

    /// Makes `link` in the project a symbolic link to `target` outside it.
    ///
    /// # Errors
    /// A name climbs out, the link cannot be made, or the platform has no
    /// symbolic links.
    pub fn link_out(&mut self, files: &Files, link: &str, target: &str) -> Result<(), String> {
        let rig = self.rig(files)?;
        let (from, to) = (rig.outside_path(target)?, rig.path(link)?);
        let failed = |e: std::io::Error| format!("cannot link {link:?} to {target:?}: {e}");
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }
        #[cfg(unix)]
        return std::os::unix::fs::symlink(&from, &to).map_err(failed);
        #[cfg(not(unix))]
        return Err(format!("no symbolic link for {from:?} on this platform"));
    }

    /// Checks a value of a JSON file of the project.
    ///
    /// # Errors
    /// The file is not JSON, or has nothing or something else at `pointer`.
    pub fn json_file_has(
        &mut self,
        files: &Files,
        name: &str,
        pointer: &str,
        want: &Value,
    ) -> Result<(), String> {
        let rig = self.rig(files)?;
        let path = rig.path(name)?;
        let text = fs::read_to_string(&path).map_err(|e| format!("cannot read {name:?}: {e}"))?;
        let json: Value =
            serde_json::from_str(&text).map_err(|e| format!("{name:?} is not JSON: {e}"))?;
        match json.pointer(pointer) {
            Some(actual) if crate::value::same(want, actual) => Ok(()),
            Some(actual) => Err(format!("{name:?} has {actual} at {pointer}, not {want}")),
            None => Err(format!("{name:?} has nothing at {pointer}:\n{text}")),
        }
    }

    /// Checks the size of a PNG of the project, as a command wrote it.
    ///
    /// # Errors
    /// The file is not a PNG, or is of another size.
    pub fn picture_is(
        &mut self,
        files: &Files,
        name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let path = self.rig(files)?.path(name)?;
        let file = fs::File::open(&path).map_err(|e| format!("cannot read {name:?}: {e}"))?;
        let reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .map_err(|e| format!("{name:?} is not a PNG: {e}"))?;
        let info = reader.info();
        if (info.width, info.height) == (width, height) {
            Ok(())
        } else {
            Err(format!(
                "{name:?} is {} by {} pixels, not {width} by {height}",
                info.width, info.height
            ))
        }
    }

    /// Checks the text at `pointer` of the last answer against a doc string.
    ///
    /// # Errors
    /// The answer has no text there, or another one.
    pub fn answer_text_is(&self, pointer: &str, want: &str) -> Result<(), String> {
        let answer = self.seen()?.answered()?;
        match answer.structured.pointer(pointer).and_then(Value::as_str) {
            Some(actual) if same_text(actual, want) => Ok(()),
            Some(actual) => Err(format!("the answer's {pointer} is:\n{actual}")),
            None => Err(format!(
                "the answer has no text at {pointer}: {}",
                answer.structured
            )),
        }
    }

    /// Checks that the prompt names these tools, in this order.
    ///
    /// # Errors
    /// The prompt was refused or names other tools.
    pub fn prompt_names(&self, names: &[&str]) -> Result<(), String> {
        let named = tools_named(&self.seen()?.answered()?.text);
        if named == names {
            Ok(())
        } else {
            Err(format!("the prompt names the tools {named:?}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_string_loses_its_label_and_its_indent() {
        assert_eq!(
            text_of("sh\n    #!/bin/sh\n    echo hi"),
            "#!/bin/sh\necho hi"
        );
        assert_eq!(text_of("  a\n\n    b"), "a\n\n  b");
        // A first line that is the file's own stays.
        assert_eq!(
            text_of("import QtQuick\nItem {}"),
            "import QtQuick\nItem {}"
        );
    }

    #[test]
    fn texts_are_compared_without_their_last_line_end() {
        assert!(same_text("a\nb\n", "a\nb"));
        assert!(!same_text("a\n\nb\n", "a\nb"));
    }

    #[test]
    fn a_prompt_names_a_tool_once_in_the_order_it_comes() {
        let text = "Call `qml_check`, read `ljos://docs`, then `qml_lint` and `qml_check` again; \
                    not `Accessible.name` and not `@api-Some.thing`.";
        assert_eq!(tools_named(text), ["qml_check", "qml_lint"]);
    }

    #[test]
    fn a_file_outside_the_project_is_not_the_scenarios() {
        let rig = Rig::new().unwrap();
        assert!(rig.path("docs/A.md").is_ok());
        assert!(rig.path("../A.md").is_err());
        assert!(rig.path("/etc/passwd").is_err());
    }

    #[test]
    fn a_tool_error_is_a_refusal_with_its_code() {
        let answer = tool_answer(json!({
            "content": [{ "type": "text", "text": "busy: run-1 is running" }],
            "structuredContent": { "error": { "code": "busy", "message": "run-1 is running" } },
            "isError": true,
        }));
        let refusal = answer.refusal.unwrap();
        assert_eq!(
            (refusal.code.as_str(), refusal.message.as_str()),
            ("busy", "run-1 is running")
        );
    }
}
