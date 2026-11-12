//! `ljos lsp`: the language server protocol over standard input and
//! output, for an editor. An opened QML file, `.ts` module or `.lts`
//! module is checked as `ljos check` checks it, as the editor has it, and
//! what is found is published at its place in the text as written: the
//! check reads Ljós's syntax desugared and maps each finding back
//! (docs/DIALECT.md §4.21). A `.ts` module that uses Ljós's syntax is
//! offered the rename to `.lts`, with the files that import it. Saving an
//! `.lts` module writes the project's `.ljos/` again.
//!
//! The server reads one message, answers it, and reads the next: between
//! messages it waits on its input and looks at nothing (CLAUDE.md rule 6).
//!
//! DECISION: the protocol's few messages are written with `serde_json`, not
//! a crate of the protocol's types: the server speaks a handful of them.

use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use ljos_compiler::{
    Project, Severity, check_modules, compile_file,
    rename::{LtsPlan, plan_lts},
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
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    Ok(serve(stdin, stdout, root)?)
}

/// Serves the messages of `input` until `exit` or its end, answering on
/// `output`; what the exit status is.
pub fn serve(mut input: impl BufRead, output: impl Write, root: PathBuf) -> io::Result<i32> {
    let mut server = Server {
        root,
        open: HashMap::new(),
        out: output,
        shut: false,
    };
    while let Some(message) = read(&mut input)? {
        if !server.handle(&message)? {
            break;
        }
    }
    Ok(if server.shut { 0 } else { 1 })
}

struct Server<W> {
    root: PathBuf,
    /// The open documents' texts, as the editor has them, by URI.
    open: HashMap<String, String>,
    out: W,
    /// `shutdown` was asked.
    shut: bool,
}

impl<W: Write> Server<W> {
    /// Handles one message; false at `exit`.
    fn handle(&mut self, message: &Value) -> io::Result<bool> {
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
                    self.open.insert(uri.to_owned(), text.to_owned());
                    self.publish(uri)?;
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
                    self.open.insert(uri.to_owned(), text.to_owned());
                    self.publish(uri)?;
                }
            }
            "textDocument/didSave" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.saved(uri)?;
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.open.remove(uri);
                    self.notify(
                        "textDocument/publishDiagnostics",
                        json!({"uri": uri, "diagnostics": []}),
                    )?;
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

    /// Publishes what the check finds in the open document at `uri`.
    fn publish(&mut self, uri: &str) -> io::Result<()> {
        let diagnostics = match (path_of(uri), self.open.get(uri)) {
            (Some(path), Some(text)) => check(&self.root, &path, text),
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
            Some(text) => text.clone(),
            None => match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => return Vec::new(),
            },
        };
        let Ok(LtsPlan::Rename(rename)) = plan_lts(&self.root, &path, &text) else {
            return Vec::new();
        };
        let mut changes = vec![json!({
            "kind": "rename",
            "oldUri": url(&rename.module),
            "newUri": url(&rename.target),
        })];
        for importer in &rename.importers {
            changes.push(json!({
                "textDocument": {"uri": url(&importer.path), "version": null},
                "edits": [{"range": whole(&importer.text), "newText": importer.renamed}],
            }));
        }
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

/// What `ljos check` finds in the file at `path` when its text is `text`,
/// as the protocol's diagnostics: the compiler's findings for a QML file,
/// the module's for a `.ts` or `.lts` module.
///
/// DECISION: the server publishes the compiler's findings, which are what
/// `ljos check` reports of one file without `--lint`; the project-wide
/// passes (members, lints, `--tsc`) stay with `ljos check`.
fn check(root: &Path, path: &Path, text: &str) -> Vec<Value> {
    let Some(key) = key_below(root, path) else {
        return Vec::new();
    };
    let Ok(mut project) = Project::load(root, QtTypeOracle) else {
        return Vec::new();
    };
    let (failed, found) = if let Some(file) = key.strip_suffix(".qml") {
        if project.source(file) != Some(text) {
            project.add(file, text);
        }
        match compile_file(&project, file) {
            Ok(compiled) => (false, compiled.diagnostics),
            Err(diagnostics) => (true, diagnostics),
        }
    } else if is_module(&key) {
        if project.script(&key) != Some(text) {
            project.add_script(&key, text);
        }
        let found = check_modules(&project, std::slice::from_ref(&key))
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
