//! `ljos lsp`: the language server protocol over standard input and
//! output, for an editor. An opened QML file, `.ts` module or `.lts`
//! module is checked as `ljos check` checks it, as the editor has it, and
//! what is found is published at its place in the text as written: the
//! check reads Ljós's syntax desugared and maps each finding back
//! (docs/DIALECT.md §4.21). A `.ts` module that uses Ljós's syntax is
//! offered the rename to `.lts`, with the files that import it. Saving an
//! `.lts` module writes the project's `.ljos/` again.
//!
//! Between messages the server waits on its input and looks at nothing
//! (CLAUDE.md rule 6). It handles together the messages that have come
//! while it worked, and checks an opened or changed document once, before
//! it answers the next request or waits again: a burst of keystrokes is
//! one check, of the last text. The project is read from disk once and
//! kept, with each open document's text put over its file's, so a
//! keystroke checks one file and reads none. It is read again after a
//! save, a close or a change the editor reports of a file it watches, the
//! events that can make disk differ from it; after a save or a watched
//! change every open document is checked again, as one may import what
//! changed.
//!
//! DECISION: checks run on the thread that reads the messages, not on a
//! thread of their own. A check is of one file against a project already
//! read, the changes that came while one ran are checked once, and a
//! request is answered after what was published before it, as the
//! protocol's order asks.
//!
//! DECISION: the protocol's few messages are written with `serde_json`, not
//! a crate of the protocol's types: the server speaks a handful of them.

use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use ljos_compiler::{
    Project, Severity, check_modules, compile_file,
    rename::{Importer, LtsPlan, plan_lts_with},
};
use qml_front::QtTypeOracle;
use serde_json::{Value, json};

/// The protocol's error for a request the server has no answer to.
const METHOD_NOT_FOUND: i64 = -32601;

/// Serves on standard input and output, from the current directory until
/// the editor names its root. Exits 0 after `shutdown` then `exit`, else 1,
/// as the protocol asks.
pub fn run() -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let stdin = io::BufReader::new(io::stdin());
    let stdout = io::stdout().lock();
    Ok(serve(stdin, stdout, root)?)
}

/// Serves the messages of `input` until `exit` or its end, answering on
/// `output`; what the exit status is. The messages are read on a thread of
/// their own, so that those that came while the server worked are handled
/// together.
pub fn serve(
    mut input: impl BufRead + Send + 'static,
    output: impl Write,
    root: PathBuf,
) -> io::Result<i32> {
    let (came, heard) = mpsc::channel();
    thread::spawn(move || {
        loop {
            let message = read(&mut input).transpose();
            let end = !matches!(message, Some(Ok(_)));
            if let Some(message) = message
                && came.send(message).is_err()
            {
                break;
            }
            if end {
                break;
            }
        }
    });
    let mut server = Server::new(root, output);
    while let Ok(first) = heard.recv() {
        let mut batch = Vec::new();
        let mut broken = None;
        for message in std::iter::once(first).chain(heard.try_iter()) {
            match message {
                Ok(message) => batch.push(message),
                Err(e) => {
                    broken = Some(e);
                    break;
                }
            }
        }
        if !server.handle_all(&batch)? {
            break;
        }
        if let Some(e) = broken {
            return Err(e);
        }
    }
    Ok(if server.shut { 0 } else { 1 })
}

struct Server<W> {
    root: PathBuf,
    /// The open documents, as the editor has them, by URI.
    open: HashMap<String, Document>,
    /// The open documents to check before the next request, in the order
    /// they were opened or changed.
    stale: Vec<String>,
    /// That the project cannot be read was said, and not since it was read.
    told: bool,
    /// The project as last read from disk, with the open documents' texts
    /// put over their files'; read when first needed.
    project: Option<Project<QtTypeOracle>>,
    out: W,
    /// `shutdown` was asked.
    shut: bool,
    /// How many times the project was read from disk.
    loads: usize,
    /// How many times a file was checked.
    checks: usize,
}

/// An open document: its text as the editor has it, and the version the
/// editor gave that text.
struct Document {
    text: String,
    version: Option<i64>,
}

impl<W: Write> Server<W> {
    fn new(root: PathBuf, out: W) -> Self {
        Self {
            root,
            open: HashMap::new(),
            stale: Vec::new(),
            told: false,
            project: None,
            out,
            shut: false,
            loads: 0,
            checks: 0,
        }
    }

    /// Handles the messages that have come, in order, and checks what they
    /// opened or changed: before the first message that is not one of the
    /// editor's notes about its documents, and at the end. False at `exit`.
    fn handle_all(&mut self, messages: &[Value]) -> io::Result<bool> {
        for message in messages {
            let noted = message.get("id").is_none()
                && matches!(
                    message["method"].as_str(),
                    Some(
                        "textDocument/didOpen"
                            | "textDocument/didChange"
                            | "textDocument/didClose"
                            | "textDocument/didSave"
                            | "workspace/didChangeWatchedFiles"
                    )
                );
            if !noted {
                self.check_stale()?;
            }
            if !self.handle_one(message)? {
                return Ok(false);
            }
        }
        self.check_stale()?;
        Ok(true)
    }

    /// Handles one message, as one that came alone; false at `exit`.
    #[cfg(test)]
    fn handle(&mut self, message: &Value) -> io::Result<bool> {
        self.handle_all(std::slice::from_ref(message))
    }

    /// Handles one message, leaving what it opened or changed to be
    /// checked; false at `exit`.
    fn handle_one(&mut self, message: &Value) -> io::Result<bool> {
        let id = message.get("id").cloned();
        let params = &message["params"];
        let Some(method) = message["method"].as_str() else {
            // An answer to a request of the server's: it makes none.
            return Ok(true);
        };
        match method {
            "initialize" => {
                if let Some(root) = params["rootUri"].as_str().and_then(path_of) {
                    self.root = root;
                } else if let Some(root) = params["rootPath"].as_str() {
                    self.root = PathBuf::from(root);
                }
                self.project = None;
                self.answer(
                    id,
                    json!({
                        "capabilities": {
                            "textDocumentSync": {
                                "openClose": true,
                                "change": 1,
                                "save": {"includeText": false}
                            },
                            "codeActionProvider": {"codeActionKinds": ["quickfix"]}
                        },
                        "serverInfo": {"name": "ljos", "version": env!("CARGO_PKG_VERSION")}
                    }),
                )?;
            }
            "textDocument/didOpen" => {
                let document = &params["textDocument"];
                if let (Some(uri), Some(text)) =
                    (document["uri"].as_str(), document["text"].as_str())
                {
                    self.edited(uri, text, document["version"].as_i64());
                    self.stale(uri);
                }
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str();
                // The whole text: the server asked for full changes.
                let text = params["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str());
                if let (Some(uri), Some(text)) = (uri, text) {
                    let version = params["textDocument"]["version"].as_i64();
                    self.edited(uri, text, version);
                    self.stale(uri);
                }
            }
            "textDocument/didSave" => {
                // What is on disk may now differ from what was read.
                self.project = None;
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.saved(uri)?;
                }
                self.all_stale();
            }
            "workspace/didChangeWatchedFiles" => {
                self.project = None;
                self.all_stale();
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.open.remove(uri);
                    self.stale.retain(|stale| stale != uri);
                    // Its file is read as it is on disk again, so an open
                    // file that imports it is checked against that.
                    self.project = None;
                    self.notify(
                        "textDocument/publishDiagnostics",
                        json!({"uri": uri, "diagnostics": []}),
                    )?;
                    self.all_stale();
                }
            }
            "textDocument/codeAction" => {
                let actions = params["textDocument"]["uri"]
                    .as_str()
                    .map(|uri| self.actions(uri))
                    .unwrap_or_default();
                self.answer(id, Value::Array(actions))?;
            }
            "shutdown" => {
                self.shut = true;
                self.answer(id, Value::Null)?;
            }
            "exit" => return Ok(false),
            _ => {
                // A notification the server does not need is ignored; a
                // request is told it has no answer.
                if let Some(id) = id {
                    self.send(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": METHOD_NOT_FOUND, "message": format!("`ljos lsp` does not answer `{method}`")}
                    }))?;
                }
            }
        }
        Ok(true)
    }

    /// The editor has `text` for the document at `uri`: it is kept, and put
    /// over its file in the project when the project has been read.
    fn edited(&mut self, uri: &str, text: &str, version: Option<i64>) {
        if let (Some(project), Some(key)) = (
            self.project.as_mut(),
            path_of(uri).and_then(|path| key_below(&self.root, &path)),
        ) {
            overlay(project, &key, text);
        }
        self.open.insert(
            uri.to_owned(),
            Document {
                text: text.to_owned(),
                version,
            },
        );
    }

    /// The document at `uri` is to be checked before the next request.
    fn stale(&mut self, uri: &str) {
        if !self.stale.iter().any(|stale| stale == uri) {
            self.stale.push(uri.to_owned());
        }
    }

    /// Every open document is to be checked again: one may import a file
    /// that changed on disk.
    fn all_stale(&mut self) {
        let mut open: Vec<String> = self.open.keys().cloned().collect();
        open.sort();
        for uri in open {
            self.stale(&uri);
        }
    }

    /// Checks and publishes each document left to check.
    fn check_stale(&mut self) -> io::Result<()> {
        for uri in std::mem::take(&mut self.stale) {
            self.publish(&uri)?;
        }
        Ok(())
    }

    /// The project, read from disk with the open documents put over it
    /// when it has not been read since it was last let go.
    fn project(&mut self) -> io::Result<&Project<QtTypeOracle>> {
        if self.project.is_none() {
            let mut project = Project::load(&self.root, QtTypeOracle)
                .map_err(|e| io::Error::other(format!("{e:#}")))?;
            self.loads += 1;
            for (uri, document) in &self.open {
                if let Some(key) = path_of(uri).and_then(|path| key_below(&self.root, &path)) {
                    overlay(&mut project, &key, &document.text);
                }
            }
            self.project = Some(project);
        }
        Ok(self.project.as_ref().expect("the project was just read"))
    }

    /// Publishes what the check finds in the open document at `uri`. A
    /// project that cannot be read is said once, until it is read again,
    /// and nothing is published: no finding is not the same as no check.
    fn publish(&mut self, uri: &str) -> io::Result<()> {
        let key = path_of(uri).and_then(|path| key_below(&self.root, &path));
        let diagnostics = match (key, self.open.contains_key(uri)) {
            (Some(key), true) => {
                let text = self.open[uri].text.clone();
                self.checks += 1;
                match self.project() {
                    Ok(project) => {
                        let found = check(project, &key, &text);
                        self.told = false;
                        found
                    }
                    Err(e) => {
                        if !self.told {
                            self.told = true;
                            let message = format!(
                                "ljos: cannot read the project at {}: {e}",
                                self.root.display()
                            );
                            self.notify(
                                "window/showMessage",
                                json!({"type": 1, "message": message}),
                            )?;
                        }
                        return Ok(());
                    }
                }
            }
            _ => Vec::new(),
        };
        self.notify(
            "textDocument/publishDiagnostics",
            json!({"uri": uri, "diagnostics": diagnostics}),
        )
    }

    /// An `.lts` module that is saved has its declaration written again,
    /// with the rest of the project's `.ljos/`, as `ljos check` writes it.
    fn saved(&mut self, uri: &str) -> io::Result<()> {
        let Some(path) = path_of(uri) else {
            return Ok(());
        };
        if path.extension().is_none_or(|e| e != "lts") {
            return Ok(());
        }
        let written = crate::generated::find(None, std::slice::from_ref(&self.root))
            .and_then(|lts| lts.map(|lts| lts.write()).transpose());
        if let Err(e) = written {
            self.notify(
                "window/showMessage",
                json!({"type": 1, "message": format!("ljos: cannot write the declarations: {e:#}")}),
            )?;
        }
        Ok(())
    }

    /// The rename to `.lts` of the `.ts` module at `uri`, when it uses
    /// Ljós's syntax, as a workspace edit: the file renamed, and each file
    /// that imports it given the new name.
    fn actions(&self, uri: &str) -> Vec<Value> {
        let Some(path) = path_of(uri) else {
            return Vec::new();
        };
        let name = path.to_string_lossy();
        if !name.ends_with(".ts") || name.ends_with(".d.ts") {
            return Vec::new();
        }
        let text = match self.open.get(uri) {
            Some(document) => document.text.clone(),
            None => match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => return Vec::new(),
            },
        };
        // An importer the editor has open is edited as the editor has it,
        // saved or not, and for the version it has: an editor whose text
        // has moved on since refuses the edit rather than lose its own.
        let absolute = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
        let open: HashMap<PathBuf, (&str, &Document)> = self
            .open
            .iter()
            .filter_map(|(uri, document)| {
                Some((absolute(&path_of(uri)?), (uri.as_str(), document)))
            })
            .collect();
        let read = |path: &Path| match open.get(&absolute(path)) {
            Some((_, document)) => Ok(document.text.clone()),
            None => std::fs::read_to_string(path),
        };
        let Ok(LtsPlan::Rename(rename)) = plan_lts_with(&self.root, &path, &text, read) else {
            return Vec::new();
        };
        // A module that imports itself is edited before it is renamed, under
        // the name the editor has it by: an edit of the old name after the
        // rename would make that file again.
        let (own, others): (Vec<_>, Vec<_>) = rename
            .importers
            .iter()
            .partition(|importer| importer.path == rename.target);
        let edit = |importer: &Importer| {
            // The module's own edit is of the name it has before the rename.
            let at = if importer.path == rename.target {
                &rename.module
            } else {
                &importer.path
            };
            let document = match open.get(&absolute(at)) {
                Some((uri, document)) => json!({"uri": uri, "version": document.version}),
                None => json!({"uri": url(at), "version": null}),
            };
            json!({
                "textDocument": document,
                "edits": [{"range": whole(&importer.text), "newText": importer.renamed}],
            })
        };
        let mut changes: Vec<Value> = own.into_iter().map(edit).collect();
        changes.push(json!({
            "kind": "rename",
            "oldUri": url(&rename.module),
            "newUri": url(&rename.target),
        }));
        changes.extend(others.into_iter().map(edit));
        let file = |key: &str| key.rsplit('/').next().unwrap_or(key).to_owned();
        vec![json!({
            "title": format!("Rename {} to {}", file(&rename.from), file(&rename.to)),
            "kind": "quickfix",
            "edit": {"documentChanges": changes},
        })]
    }

    fn answer(&mut self, id: Option<Value>, result: Value) -> io::Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result}))
    }

    fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&mut self, message: &Value) -> io::Result<()> {
        let body = message.to_string();
        write!(self.out, "Content-Length: {}\r\n\r\n{body}", body.len())?;
        self.out.flush()
    }
}

/// What `ljos check` finds in the file `key` of `project`, whose text the
/// editor has as `text`, as the protocol's diagnostics: the compiler's
/// findings for a QML file, the module's for a `.ts` or `.lts` module.
///
/// DECISION: the server publishes the compiler's findings, which are what
/// `ljos check` reports of one file without `--lint`; the project-wide
/// passes (members, lints, `--tsc`) stay with `ljos check`.
fn check(project: &Project<QtTypeOracle>, key: &str, text: &str) -> Vec<Value> {
    let key = key.to_owned();
    let (failed, found) = if let Some(file) = key.strip_suffix(".qml") {
        match compile_file(project, file) {
            Ok(compiled) => (false, compiled.diagnostics),
            Err(diagnostics) => (true, diagnostics),
        }
    } else if is_module(&key) {
        let found = check_modules(project, std::slice::from_ref(&key))
            .into_iter()
            .find(|module| module.key == key)
            .map(|module| module.diagnostics)
            .unwrap_or_default();
        (false, found)
    } else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.split('\n').collect();
    found
        .into_iter()
        .map(|d| {
            let line = d.span.line.saturating_sub(1);
            let start = json!({
                "line": line,
                "character": utf16(lines.get(line as usize).copied().unwrap_or(""), d.span.column.saturating_sub(1)),
            });
            json!({
                "range": {"start": start, "end": start},
                "severity": if failed || d.severity == Severity::Error { 1 } else { 2 },
                "code": d.code.as_str(),
                "source": "ljos",
                "message": d.message,
            })
        })
        .collect()
}

/// Puts `text` over the file `key` of `project`, as the editor has it.
fn overlay(project: &mut Project<QtTypeOracle>, key: &str, text: &str) {
    if let Some(file) = key.strip_suffix(".qml") {
        if project.source(file) != Some(text) {
            project.add(file, text);
        }
    } else if is_module(key) && project.script(key) != Some(text) {
        project.add_script(key, text);
    }
}

fn is_module(key: &str) -> bool {
    key.ends_with(".lts") || (key.ends_with(".ts") && !key.ends_with(".d.ts"))
}

/// `path`'s key below `root`, with `/` between its parts.
fn key_below(root: &Path, path: &Path) -> Option<String> {
    let absolute = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
    let (root, path) = (absolute(root), absolute(path));
    let below = path.strip_prefix(&root).ok()?;
    let parts: Vec<&str> = below
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    Some(parts.join("/"))
}

/// The protocol counts a line's characters in UTF-16 units; the checker
/// counts them as characters.
fn utf16(line: &str, characters: u32) -> u32 {
    line.chars()
        .take(characters as usize)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// The range of all of `text`.
fn whole(text: &str) -> Value {
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.last().copied().unwrap_or("");
    json!({
        "start": {"line": 0, "character": 0},
        "end": {"line": lines.len() - 1, "character": utf16(last, u32::MAX)},
    })
}

/// A path as a `file:` URL: each byte outside the unreserved set and `/`
/// is written as `%XX`.
fn url(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The path of a `file:` URL; None for any other.
fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = rest
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

/// One message of the base protocol: its headers, then as many bytes of
/// JSON as `Content-Length` says. None at the end of the input.
fn read(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    loop {
        let mut header = String::new();
        if input.read_line(&mut header)? == 0 {
            return Ok(None);
        }
        let header = header.trim_end();
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok();
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Write};

    use serde_json::{Value, json};

    use super::{path_of, read, serve, url};

    fn send(to: &mut impl Write, message: Value) {
        let body = message.to_string();
        write!(to, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        to.flush().unwrap();
    }

    /// An editor's session over pipes, in this process: what is published
    /// for an opened file comes before the answer to `shutdown`, and
    /// `exit` after it ends the server with 0.
    #[test]
    fn a_session_over_pipes_publishes_the_check_before_shutdown_is_answered() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("Pad.qml");
        let text = "import Ljos.Ui\nItem {\n    property real gap: 4\n    function grow() { gap = \"8\" }\n}\n";
        std::fs::write(&pad, text).unwrap();
        let (to_server, mut from_editor) = std::io::pipe().unwrap();
        let (from_server, to_editor) = std::io::pipe().unwrap();
        let root = dir.path().to_owned();
        let server = std::thread::spawn(move || serve(BufReader::new(to_server), to_editor, root));
        let uri = url(&pad);
        send(
            &mut from_editor,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"rootUri": url(dir.path()), "capabilities": {}}}),
        );
        send(
            &mut from_editor,
            json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        );
        send(
            &mut from_editor,
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "qml", "version": 1, "text": text}}}),
        );
        send(
            &mut from_editor,
            json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
        );
        let mut heard = BufReader::new(from_server);
        let mut published = None;
        loop {
            let message = read(&mut heard)
                .unwrap()
                .expect("the server answers shutdown");
            if message["method"] == "textDocument/publishDiagnostics" {
                published = Some(message["params"]["diagnostics"].clone());
            }
            if message["id"] == 2 {
                break;
            }
        }
        send(
            &mut from_editor,
            json!({"jsonrpc": "2.0", "method": "exit"}),
        );
        assert_eq!(server.join().unwrap().unwrap(), 0);
        let published =
            published.expect("the opened file is published before shutdown is answered");
        assert_eq!(published.as_array().map(Vec::len), Some(1), "{published}");
        assert_eq!(published[0]["code"], "ts-assign-type");
        assert_eq!(
            published[0]["range"]["start"],
            json!({"line": 3, "character": 28})
        );
    }

    /// A keystroke is checked against the project as it was read, with the
    /// changed text put in it: the project is read from disk again only
    /// when a file is saved.
    #[test]
    fn the_project_is_read_once_and_again_only_on_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("Pad.qml");
        std::fs::write(&pad, "import Ljos.Ui\nItem {}\n").unwrap();
        std::fs::write(dir.path().join("Other.qml"), "import Ljos.Ui\nItem {}\n").unwrap();
        let mut server = super::Server::new(dir.path().to_owned(), Vec::new());
        let uri = url(&pad);
        let change = |n: i64, text: &str| json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"textDocument": {"uri": uri, "version": n}, "contentChanges": [{"text": text}]}});
        server
            .handle(&json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "qml", "version": 1, "text": "import Ljos.Ui\nItem {}\n"}}}))
            .unwrap();
        for (n, text) in [
            (2, "import Ljos.Ui\nItem { }\n"),
            (3, "import Ljos.Ui\nItem {  }\n"),
        ] {
            server.handle(&change(n, text)).unwrap();
        }
        assert_eq!(server.loads, 1, "one read for an open and two changes");
        server
            .handle(&json!({"jsonrpc": "2.0", "method": "textDocument/didSave", "params": {"textDocument": {"uri": uri}}}))
            .unwrap();
        server
            .handle(&change(4, "import Ljos.Ui\nItem {}\n"))
            .unwrap();
        assert_eq!(server.loads, 2, "read again after the save");
    }

    /// Changes that came one after another before the server got to them
    /// are checked once, as the last has the text: a burst of keystrokes
    /// does not queue a check for each, and what is published is the last.
    #[test]
    fn changes_that_came_together_are_checked_once() {
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("Pad.qml");
        std::fs::write(&pad, "import Ljos.Ui\nItem {}\n").unwrap();
        let mut server = super::Server::new(dir.path().to_owned(), Vec::new());
        let uri = url(&pad);
        let change = |n: i64, text: &str| json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"textDocument": {"uri": uri, "version": n}, "contentChanges": [{"text": text}]}});
        server
            .handle_all(&[
                json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "qml", "version": 1, "text": "import Ljos.Ui\nItem {}\n"}}}),
                change(2, "import Ljos.Ui\nItem { property real gap: \"8\" }\n"),
                change(3, "import Ljos.Ui\nItem { property real gap: 8 }\n"),
                json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
            ])
            .unwrap();
        assert_eq!(
            server.checks, 1,
            "one check for an open and two changes that came with it"
        );
        let said = String::from_utf8(server.out).unwrap();
        let published = said.matches("textDocument/publishDiagnostics").count();
        assert_eq!(published, 1, "{said}");
        assert!(said.contains("\"diagnostics\":[]"), "{said}");
        assert!(
            said.find("publishDiagnostics") < said.find("\"id\":2"),
            "published before shutdown is answered: {said}"
        );
    }

    /// A project that cannot be read is said, not published as a file with
    /// nothing wrong in it.
    #[test]
    fn a_project_that_cannot_be_read_is_said_and_nothing_is_published() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("gone");
        let mut server = super::Server::new(root.clone(), Vec::new());
        let uri = url(&root.join("Pad.qml"));
        server
            .handle(&json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "qml", "version": 1, "text": "import Ljos.Ui\nItem {}\n"}}}))
            .unwrap();
        let said = String::from_utf8(server.out).unwrap();
        assert!(said.contains("window/showMessage"), "{said}");
        assert!(said.contains("cannot read the project"), "{said}");
        assert!(!said.contains("publishDiagnostics"), "{said}");
    }

    /// A module that imports itself is edited before it is renamed, at the
    /// name the editor has it open under, so the edit does not make a file
    /// of the old name again.
    #[test]
    fn a_module_that_imports_itself_is_edited_before_it_is_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let size = dir.path().join("size.ts");
        let text = "import * as Me from \"./size.ts\"\nexport const k: number = match (1) { default: 2 }\n";
        std::fs::write(&size, text).unwrap();
        let mut server = super::Server::new(dir.path().to_owned(), Vec::new());
        let uri = url(&size);
        server
            .handle(&json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "typescript", "version": 3, "text": text}}}))
            .unwrap();
        let actions = server.actions(&uri);
        let changes = actions[0]["edit"]["documentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(
            changes[0]["textDocument"],
            json!({"uri": uri, "version": 3}),
            "{changes:?}"
        );
        assert_eq!(changes[1]["kind"], "rename", "{changes:?}");
    }

    #[test]
    fn a_url_is_read_back_as_its_path() {
        let path = std::path::Path::new("/a b/ljós/width.lts");
        assert_eq!(url(path), "file:///a%20b/lj%C3%B3s/width.lts");
        assert_eq!(path_of(&url(path)).as_deref(), Some(path));
    }

    #[test]
    fn a_server_whose_input_ends_without_shutdown_exits_with_1() {
        assert_eq!(
            serve(&b""[..], Vec::new(), std::env::temp_dir()).unwrap(),
            1
        );
    }
}
