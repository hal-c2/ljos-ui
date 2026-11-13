//! An editor's session with `ljos lsp`, for `features/tooling/language-server.feature`.
//!
//! The scenario's files are written to a project, `ljos lsp` is started
//! in it, and one file is opened, perhaps changed, saved or asked for its
//! code actions, as an editor does: a save writes the editor's text to the
//! file, then tells the server, once the server has answered what came
//! before it, so that it read the file as it was before the save. Other files may be open in the editor
//! first, as they are on disk or with text it has not saved. An action's
//! edits are applied to what the editor has, as an editor applies them,
//! refused where they are for a version of a file the editor no longer
//! has. Then the server is shut down: its answer to `shutdown` comes after
//! everything it published for what came before, so what it said is all
//! there without waiting on a clock.

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
    /// Files open in the editor before `file`, each changed to a text it
    /// has not saved, or as it is on disk.
    pub before: Vec<(String, Option<String>)>,
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
/// its JSON has `published`, those lines for each file the server
/// published for, and `actions`, each with its `title`, its `renames`
/// (from, to) and its `edits`: by file, its text once the edit is applied
/// to what the editor has of it, or why the editor refused it.
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
    // Each file's URL is its whole path's, so a name with `%` in it is
    // that name and no other.
    let url_of = |file: &str| url(&staged.suite.join(file).display().to_string());
    let uri = url_of(&session.file);
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
    let language = |file: &str| match file.rsplit('.').next() {
        Some("qml") => "qml",
        _ => "typescript",
    };
    send(request(
        1,
        "initialize",
        json!({"processId": null, "rootUri": root, "capabilities": {}}),
    ))?;
    send(notification("initialized", json!({})))?;
    // What the editor has of each file it opened: its text and version.
    let mut editor: BTreeMap<String, (String, i64)> = BTreeMap::new();
    let mut open = |file: &str, saved: &str, change: Option<String>| -> Result<()> {
        let uri = url_of(file);
        send(notification(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": language(file), "version": 1, "text": saved}}),
        ))?;
        let mut held = (saved.to_owned(), 1);
        if let Some(text) = change {
            send(notification(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": text}]}),
            ))?;
            held = (text, 2);
        }
        editor.insert(file.to_owned(), held);
        Ok(())
    };
    for (file, text) in &session.before {
        let saved = flush
            .iter()
            .find(|f| &f.name == file)
            .map_or("", |f| f.source.as_str());
        open(file, saved, text.as_deref().map(crate::author::text_of))?;
    }
    open(
        &session.file,
        &opened.source,
        session.change.as_deref().map(crate::author::text_of),
    )?;
    let mut heard = Heard {
        from: heard,
        deadline: std::time::Instant::now() + DEADLINE,
        root: &root,
        published: BTreeMap::new(),
        actions: Vec::new(),
    };
    if session.save {
        // The server has read the file as it was before the editor saves
        // it, as it would have when an editor's save comes later than its
        // other messages: what it publishes after the save is for the save.
        send(request(
            4,
            "textDocument/codeAction",
            json!({"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "context": {"diagnostics": []}}),
        ))?;
        heard.until(4, &editor, &flush)?;
        // An editor writes the file, then says it saved it.
        if let Some((text, _)) = editor.get(&session.file) {
            std::fs::write(staged.suite.join(&session.file), text)
                .map_err(|e| failed(format!("cannot save {:?}: {e}", session.file)))?;
        }
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
    heard.until(3, &editor, &flush)?;
    let Heard {
        published, actions, ..
    } = heard;
    send(notification("exit", Value::Null))?;
    let status = child
        .finish()
        .map_err(|e| failed(format!("cannot wait for `ljos lsp`: {e}")))?
        .code()
        .ok_or_else(|| failed("`ljos lsp` was killed"))?;
    Ok(TestReport {
        lines: published.get(&session.file).cloned().unwrap_or_default(),
        status,
        json: Some(json!({"published": published, "actions": actions})),
        files: snapshot(&staged.suite),
    })
}

/// What the server has said so far.
struct Heard<'a> {
    from: mpsc::Receiver<Value>,
    deadline: std::time::Instant,
    root: &'a str,
    /// What was last published for each file, by its path in the project.
    published: BTreeMap<String, Vec<String>>,
    actions: Vec<Value>,
}

impl Heard<'_> {
    /// Hears the server until it answers the request `id`: its answers
    /// come in the order of the requests, after what it published for the
    /// messages before them.
    fn until(
        &mut self,
        id: i64,
        editor: &BTreeMap<String, (String, i64)>,
        files: &[QmlFile],
    ) -> Result<()> {
        loop {
            let left = self
                .deadline
                .saturating_duration_since(std::time::Instant::now());
            let message = self.from.recv_timeout(left).map_err(|_| {
                failed("`ljos lsp` ended or did not answer what it was asked in a minute")
            })?;
            if let Some(error) = message.get("error") {
                return Err(failed(format!("`ljos lsp` refused a request: {error}")));
            }
            match (
                message.get("id").and_then(Value::as_i64),
                message.get("method").and_then(Value::as_str),
            ) {
                (Some(2), None) => {
                    for action in message["result"].as_array().into_iter().flatten() {
                        let edits = applied(self.root, action, editor, files);
                        let renames: Vec<Value> = action["edit"]["documentChanges"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|change| change["kind"] == "rename")
                            .map(|change| {
                                let file = |at: &str| {
                                    file_of(self.root, change[at].as_str().unwrap_or(""))
                                };
                                json!([file("oldUri"), file("newUri")])
                            })
                            .collect();
                        self.actions.push(
                            json!({"title": action["title"], "renames": renames, "edits": edits}),
                        );
                    }
                }
                (Some(1 | 3 | 4), None) => {}
                (_, Some("textDocument/publishDiagnostics")) => {
                    let file = file_of(self.root, message["params"]["uri"].as_str().unwrap_or(""));
                    self.published.insert(
                        file,
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
            if message.get("id").and_then(Value::as_i64) == Some(id)
                && message.get("method").is_none()
            {
                return Ok(());
            }
        }
    }
}

/// What each file an action edits holds once its edits are applied to
/// what the editor has of it (or to the file, where the editor does not
/// have it open), by its path in the project; for an edit of a version
/// the editor no longer has, why it is refused.
fn applied(
    root: &str,
    action: &Value,
    editor: &BTreeMap<String, (String, i64)>,
    files: &[QmlFile],
) -> Value {
    let mut out = serde_json::Map::new();
    for change in action["edit"]["documentChanges"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(uri) = change["textDocument"]["uri"].as_str() else {
            continue;
        };
        let file = file_of(root, uri);
        let file = file.as_str();
        let (text, version) = match editor.get(file) {
            Some((text, version)) => (text.clone(), Some(*version)),
            None => (
                files
                    .iter()
                    .find(|f| f.name == file)
                    .map_or_else(String::new, |f| f.source.clone()),
                None,
            ),
        };
        let asked = change["textDocument"]["version"].as_i64();
        let result = match (asked, version) {
            (Some(asked), Some(held)) if asked != held => {
                format!("refused: the edit is for version {asked}, the editor has {held}")
            }
            (Some(asked), None) => {
                format!(
                    "refused: the edit is for version {asked} of a file the editor has not open"
                )
            }
            _ => edit(
                &text,
                change["edits"].as_array().map_or(&[][..], Vec::as_slice),
            ),
        };
        out.insert(file.to_owned(), Value::String(result));
    }
    Value::Object(out)
}

/// `text` with `edits` applied, as an editor applies them: each range in
/// lines and UTF-16 units of the text before any edit.
fn edit(text: &str, edits: &[Value]) -> String {
    let offset = |at: &Value| -> usize {
        let line = at["line"].as_u64().unwrap_or(0) as usize;
        let character = at["character"].as_u64().unwrap_or(0) as usize;
        let mut start = 0;
        for (i, row) in text.split('\n').enumerate() {
            if i == line {
                let mut units = 0;
                for (byte, c) in row.char_indices() {
                    if units >= character {
                        return start + byte;
                    }
                    units += c.len_utf16();
                }
                return start + row.len();
            }
            start += row.len() + 1;
        }
        text.len()
    };
    let mut spans: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|e| {
            (
                offset(&e["range"]["start"]),
                offset(&e["range"]["end"]),
                e["newText"].as_str().unwrap_or(""),
            )
        })
        .collect();
    spans.sort_by_key(|span| std::cmp::Reverse(span.0));
    let mut out = text.to_owned();
    for (start, end, new) in spans {
        out.replace_range(start..end.max(start), new);
    }
    out
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

/// The file a `file:` URL below the project's names, by its path in the
/// project; the URL itself for one outside it.
fn file_of(root: &str, uri: &str) -> String {
    let Some(rest) = uri
        .strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return uri.to_owned();
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match rest
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
