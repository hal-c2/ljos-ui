//! An editor's session with `ljos lsp`, for `features/tooling/language-server.feature`.
//!
//! The scenario's files are written to a project, `ljos lsp` is started
//! in it, and one file is opened, perhaps changed, saved or asked for its
//! code actions, as an editor does. Then the server is shut down: its
//! answer to `shutdown` comes after everything it published for what came
//! before, so what it said is all there without waiting on a clock.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use ljos_child::Reaped;
use serde_json::{Value, json};

use super::testrun::{Staged, TestReport, failed, ljos_bin, snapshot};
use super::{QmlFile, Result};

/// What the editor does with its one file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// The file opened, by its path in the project.
    pub file: String,
    /// The text it is changed to after it is opened.
    pub change: Option<String>,
    pub save: bool,
    /// Asks for the code actions on the whole file.
    pub actions: bool,
}

/// How long a session has: one check of a few files, with the start of
/// the program.
const DEADLINE: Duration = Duration::from_secs(60);

/// Runs the session. The report's lines are what was last published for
/// the file, as `line:column: severity: message [code]`, counted from 1;
/// its JSON has `actions`, the titles of the actions offered.
///
/// # Errors
/// The program cannot be started, breaks the protocol or takes too long.
pub fn run(files: &[QmlFile], session: &Session) -> Result<TestReport> {
    // An editor's files are as they are outside a scenario: from their
    // first column.
    let flush: Vec<QmlFile> = files
        .iter()
        .map(|f| QmlFile {
            name: f.name.clone(),
            source: crate::author::text_of(&f.source),
        })
        .collect();
    let staged = Staged::new(&flush)?;
    let opened = flush
        .iter()
        .find(|f| f.name == session.file)
        .ok_or_else(|| failed(format!("no file {:?} was given", session.file)))?;
    let root = url(&staged.suite.display().to_string());
    let uri = format!("{root}/{}", session.file);
    let mut command = Command::new(ljos_bin()?);
    command
        .arg("lsp")
        .current_dir(&staged.suite)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child =
        Reaped::spawn(&mut command).map_err(|e| failed(format!("cannot start `ljos lsp`: {e}")))?;
    let mut input = child.stdin.take().expect("stdin is piped");
    let output = child.stdout.take().expect("stdout is piped");
    let (said, heard) = mpsc::channel();
    thread::spawn(move || {
        let mut output = BufReader::new(output);
        while let Some(message) = read(&mut output) {
            if said.send(message).is_err() {
                break;
            }
        }
    });
    let mut send = |message: Value| -> Result<()> {
        let body = message.to_string();
        write!(input, "Content-Length: {}\r\n\r\n{body}", body.len())
            .and_then(|()| input.flush())
            .map_err(|e| failed(format!("cannot write to `ljos lsp`: {e}")))
    };
    let language = match session.file.rsplit('.').next() {
        Some("qml") => "qml",
        _ => "typescript",
    };
    send(request(
        1,
        "initialize",
        json!({"processId": null, "rootUri": root, "capabilities": {}}),
    ))?;
    send(notification("initialized", json!({})))?;
    send(notification(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": language, "version": 1, "text": opened.source}}),
    ))?;
    if let Some(text) = &session.change {
        send(notification(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": crate::author::text_of(text)}]}),
        ))?;
    }
    if session.save {
        send(notification(
            "textDocument/didSave",
            json!({"textDocument": {"uri": uri}}),
        ))?;
    }
    if session.actions {
        send(request(
            2,
            "textDocument/codeAction",
            json!({"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": u32::MAX, "character": 0}}, "context": {"diagnostics": []}}),
        ))?;
    }
    send(request(3, "shutdown", Value::Null))?;
    let mut published: Option<Vec<String>> = None;
    let mut actions: Vec<String> = Vec::new();
    let mut shut = false;
    let deadline = std::time::Instant::now() + DEADLINE;
    while !shut {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let message = heard
            .recv_timeout(left)
            .map_err(|_| failed("`ljos lsp` ended or did not answer `shutdown` in a minute"))?;
        match (
            message.get("id").and_then(Value::as_i64),
            message.get("method").and_then(Value::as_str),
        ) {
            (Some(1), None) => {}
            (Some(2), None) => {
                for action in message["result"].as_array().into_iter().flatten() {
                    if let Some(title) = action["title"].as_str() {
                        actions.push(title.to_owned());
                    }
                }
            }
            (Some(3), None) => shut = true,
            (_, Some("textDocument/publishDiagnostics"))
                if message["params"]["uri"] == uri.as_str() =>
            {
                published = Some(
                    message["params"]["diagnostics"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(line)
                        .collect(),
                );
            }
            (_, Some(_)) => {}
            (id, None) => {
                return Err(failed(format!(
                    "`ljos lsp` answered a request never made ({id:?}): {message}"
                )));
            }
        }
        if let Some(error) = message.get("error") {
            return Err(failed(format!("`ljos lsp` refused a request: {error}")));
        }
    }
    send(notification("exit", Value::Null))?;
    let status = child
        .finish()
        .map_err(|e| failed(format!("cannot wait for `ljos lsp`: {e}")))?
        .code()
        .ok_or_else(|| failed("`ljos lsp` was killed"))?;
    Ok(TestReport {
        lines: published.clone().unwrap_or_default(),
        status,
        json: Some(json!({"published": published.is_some(), "actions": actions})),
        files: snapshot(&staged.suite),
    })
}

/// A path as a `file:` URL: each byte outside the unreserved set and `/`
/// is written as `%XX`.
fn url(path: &str) -> String {
    let mut out = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// One diagnostic as a report line.
fn line(d: &Value) -> String {
    let at = &d["range"]["start"];
    let severity = match d["severity"].as_u64() {
        Some(1) => "error",
        Some(2) => "warning",
        Some(3) => "information",
        _ => "hint",
    };
    let code = d["code"]
        .as_str()
        .map_or_else(String::new, |c| format!(" [{c}]"));
    format!(
        "{}:{}: {severity}: {}{code}",
        at["line"].as_u64().unwrap_or(0) + 1,
        at["character"].as_u64().unwrap_or(0) + 1,
        d["message"].as_str().unwrap_or("")
    )
}

/// One message of the base protocol: its headers, then as many bytes of
/// JSON as `Content-Length` says. None at the end of the stream or on a
/// message that breaks the protocol.
fn read(from: &mut impl BufRead) -> Option<Value> {
    let mut headers = BTreeMap::new();
    loop {
        let mut header = String::new();
        if from.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    let length: usize = headers.get("content-length")?.parse().ok()?;
    let mut body = vec![0; length];
    from.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}
