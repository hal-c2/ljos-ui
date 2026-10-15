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
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    time::Duration,
};

use serde_json::{Value, json};

use crate::backend::{
    inspector::{self, Answer, Ask, Refusal, Seen},
    testrun,
};

/// How long the server may take over one request. A scenario that waits for
/// a run asks for 20 seconds of it.
const ANSWER: Duration = Duration::from_secs(60);
/// How long the server may take to leave once nobody talks to it.
const LEAVE: Duration = Duration::from_secs(5);

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

/// `ljos mcp`, running, and the pipe it answers on.
struct Server {
    child: Child,
    /// `None` once the pipe is closed, which is how the server is told to
    /// leave.
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next: u64,
}

impl Server {
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
        let mut child = command
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The log is the server's own; what an agent sees is the answers.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start `ljos mcp`: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("`ljos mcp` has no output")?;
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
            .map_err(|e| format!("`ljos mcp` refused the client: {e}"))?;
        server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        Ok((server, hello))
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("`ljos mcp` was told to leave")?;
        writeln!(stdin, "{message}")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("`ljos mcp` does not listen: {e}"))
    }

    /// One request and its answer: the result, or the protocol's error.
    fn request(&mut self, method: &str, params: Value) -> Result<Result<Value, Value>, String> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let line = self.lines.recv_timeout(ANSWER).map_err(|e| match e {
                RecvTimeoutError::Timeout => {
                    format!(
                        "`ljos mcp` did not answer `{method}` in {} s",
                        ANSWER.as_secs()
                    )
                }
                RecvTimeoutError::Disconnected => {
                    format!("`ljos mcp` ended before it answered `{method}`")
                }
            })?;
            let Ok(mut message) = serde_json::from_str::<Value>(&line) else {
                return Err(format!("`ljos mcp` wrote what is not JSON: {line}"));
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
        return Answer {
            structured,
            text,
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

/// The project of one scenario and what was done in it.
struct Rig {
    dir: tempfile::TempDir,
    /// The staged text each file was last written from. A file is written
    /// again only when the scenario gives it anew, so that what a tool
    /// wrote stays.
    written: BTreeMap<String, String>,
    server: Option<Server>,
    seen: Seen,
    ran: Option<Ran>,
}

impl Rig {
    fn new() -> Result<Self, String> {
        let dir = tempfile::Builder::new()
            .prefix("ljos-bdd-author-")
            .tempdir()
            .map_err(|e| format!("cannot make a directory for the project: {e}"))?;
        Ok(Rig {
            dir,
            written: BTreeMap::new(),
            server: None,
            seen: Seen::default(),
            ran: None,
        })
    }

    /// The place of a file of the project, which must be inside it.
    fn path(&self, name: &str) -> Result<PathBuf, String> {
        let inside = Path::new(name)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if inside && !name.is_empty() {
            Ok(self.dir.path().join(name))
        } else {
            Err(format!("{name:?} is not a file of the scenario's project"))
        }
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
        let (mut server, hello) = Server::start(self.dir.path(), caps, runner.as_deref())?;
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

    fn prompts(&mut self) -> Result<Vec<String>, String> {
        let prompts = self.server()?.list("prompts/list", "prompts")?;
        Ok(prompts
            .iter()
            .filter_map(|prompt| prompt["name"].as_str().map(str::to_owned))
            .collect())
    }

    fn run(&mut self, line: &str) -> Result<(), String> {
        let mut words = line.split_whitespace();
        if words.next() != Some("ljos") {
            return Err(format!(
                "the author runs `ljos` and nothing else, not {line:?}"
            ));
        }
        let bin = testrun::ljos_bin().map_err(|e| e.to_string())?;
        // One pipe for the output and the errors keeps them in the order
        // they were written, which is what a terminal shows.
        let out = Command::new("sh")
            .args(["-c", r#"exec "$0" "$@" 2>&1"#])
            .arg(bin)
            .args(words)
            .current_dir(self.dir.path())
            .stdin(Stdio::null())
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
