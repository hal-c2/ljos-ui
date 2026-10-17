//! `ljos observe`: a window on a running application, as Erlang's
//! observer is on a node (ARCHITECTURE.md §3.4, §19.5). It shows the
//! application's processes, its mailboxes and its objects, with their
//! properties for editing and a picture of the one picked.
//!
//! The window is a QML application like any other. What it shows comes
//! only from the inspector's MCP tools, which it calls as an agent would,
//! so there is nothing the window knows that an agent cannot ask. Its one
//! native type, `InspectorClient`, sends a tool call over HTTP on a
//! thread of its own (`Platform::request`) and answers on the Engine
//! thread. Nothing is asked on a timer: neither the window nor its
//! target does work while nobody touches the window.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context as _, bail};
use clap::Args;
use ljos::prelude::*;
use ljos_compiler::{Project, compile_file};
use ljos_engine::MapSource;
use ljos_shell::ShellConfig;
use serde_json::{Value as Json, json};

/// The window, compiled when `ljos observe` starts: the binary carries
/// it, so the observer runs wherever `ljos` does.
const MAIN: &str = include_str!("observe/Observer.qml");
const KEY: &str = "Observer";

#[derive(Args)]
pub struct Observe {
    /// The application to observe: the token file an inspector wrote
    /// (`target/ljos/mcp-<pid>.json`), or a directory to look for one in,
    /// in itself and in its `target/ljos`. A directory with several
    /// gives the newest whose process still runs. Start the application
    /// with `ljos run --mcp FILE` (`--mcp=inspect,mutate` to edit
    /// properties).
    #[arg(default_value = ".")]
    target: PathBuf,
    /// The window's size in logical pixels.
    #[arg(long, value_parser = crate::run::parse_size, default_value = "1200x760", value_name = "WxH")]
    size: (f32, f32),
}

impl Observe {
    pub fn run(self) -> anyhow::Result<()> {
        let token_file = find_token_file(&self.target)?;
        let target = read_token_file(&token_file)?;
        crate::stderr::line(&format!(
            "ljos: observing {} (pid {}) through {}",
            target.url,
            target.pid.map_or_else(|| "?".to_owned(), |p| p.to_string()),
            token_file.display()
        ));
        let config = ShellConfig::new(format!("{KEY}.qml")).with_size(self.size.0, self.size.1);
        let file = token_file.display().to_string();
        ljos_shell::run::<()>(config, move |setup| {
            let engine = setup.engine();
            engine.set_log_handler(crate::run::print_log);
            register(engine);
            engine.set_component_source(compile(engine).map_err(|e| e.to_string())?);
            Ok(engine
                .create_from_url(
                    KEY,
                    None,
                    vec![("tokenFile".to_owned(), Value::from(file.as_str()))],
                )
                .map_err(|e| e.to_string())?)
        })?;
        Ok(())
    }
}

/// The window's QML, against the table of types and `InspectorClient`.
fn compile(engine: &Engine) -> anyhow::Result<MapSource> {
    let mut project = Project::new(ljos::dev::EngineOracle::from_engine(engine));
    project.add(KEY, MAIN);
    let compiled = match compile_file(&project, KEY) {
        Ok(c) if !c.has_errors() => c,
        Ok(c) => bail!("the observer's QML: {:?}", c.diagnostics),
        Err(ds) => bail!("the observer's QML: {ds:?}"),
    };
    let mut source = MapSource::new();
    source.insert(KEY, compiled.file);
    Ok(source)
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Ljos.Observer" 1.0 => [InspectorClient]));
}

/// What a token file says of the inspector that wrote it.
#[derive(Debug)]
struct Target {
    url: String,
    token: String,
    pid: Option<u32>,
}

fn read_token_file(path: &Path) -> anyhow::Result<Target> {
    let text = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let body: Json =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    let field = |name: &str| {
        body[name]
            .as_str()
            .map(str::to_owned)
            .with_context(|| format!("{} has no {name:?}", path.display()))
    };
    let url = field("url")?;
    // A Unix socket needs a client of its own; until there is one, said
    // here rather than as a failed connection.
    if !url.starts_with("http://") {
        bail!(
            "{url} is not an HTTP address: `ljos observe` speaks HTTP only. \
             Start the application with --mcp without --mcp-socket"
        );
    }
    Ok(Target {
        url,
        token: field("token")?,
        pid: body["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
    })
}

/// The token file `target` names, or the newest live one in it.
fn find_token_file(target: &Path) -> anyhow::Result<PathBuf> {
    if target.is_file() {
        return Ok(target.to_owned());
    }
    if !target.is_dir() {
        bail!(
            "{} is neither a token file nor a directory",
            target.display()
        );
    }
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    for dir in [target.to_owned(), target.join("target/ljos")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("mcp-") && name.ends_with(".json")) {
                continue;
            }
            // A file a killed process left names a port nobody listens on.
            let alive = read_token_file(&path)
                .ok()
                .is_some_and(|t| t.pid.is_none_or(running));
            if !alive {
                continue;
            }
            let at = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((at, path));
        }
    }
    found.sort();
    match found.pop() {
        Some((_, path)) => Ok(path),
        None => bail!(
            "no running application with an inspector in {}: start one with \
             `ljos run --mcp FILE` in that directory, or name its token file",
            target.display()
        ),
    }
}

/// Whether process `pid` runs; where that cannot be told, it is taken to.
fn running(pid: u32) -> bool {
    let proc = Path::new("/proc");
    !proc.is_dir() || proc.join(pid.to_string()).exists()
}

/// `InspectorClient`: calls the tools of the inspector a token file
/// names. `call` sends one call and returns its serial at once; the
/// answer comes as `answered`, on the Engine thread, with the tool's
/// result as JSON text (`structuredContent`, `content`, `isError`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Observer", name = "InspectorClient", extends = QtObject)]
pub struct InspectorClient {
    #[qml(base)]
    pub object: QtObject,
    /// Read again for every call, so that a target started again under a
    /// new port and token is found.
    #[qml(notify)]
    pub token_file: String,
    /// The address the last call went to.
    #[qml(notify, readonly)]
    pub url: String,
    /// Whether the last call reached the inspector.
    #[qml(notify, readonly)]
    pub connected: bool,
    /// Why the last call did not reach it; empty when it did.
    #[qml(notify, readonly)]
    pub error: String,
    /// Calls sent and not answered yet.
    #[qml(notify, readonly)]
    pub pending: i32,
    #[qml(skip)]
    serial: Cell<i32>,
}

#[qml_impl]
impl InspectorClient {
    /// The answer to call `serial`: `ok` when the tool answered without
    /// an error, `result` the tool's result or the reason there is none.
    #[qml(signal)]
    fn answered(serial: i32, tool: String, ok: bool, result: String);

    /// Calls `tool` with `arguments`, a JSON object as text.
    #[qml(invokable)]
    fn call(cx: &Cx, tool: String, arguments: String) -> Result<i32, EngineError> {
        let serial = cx.with::<InspectorClient, _>(|c| {
            let n = c.serial.get() + 1;
            c.serial.set(n);
            n
        })?;
        let pending = cx.get(InspectorClient::PENDING)?;
        cx.set(InspectorClient::PENDING, pending + 1)?;
        let file = cx.get(InspectorClient::TOKEN_FILE)?;
        let (e, me) = (cx.engine, cx.this);
        let request = read_token_file(Path::new(&file)).and_then(|target| {
            let args: Json = match arguments.trim() {
                "" => json!({}),
                text => serde_json::from_str(text).context("the arguments are not JSON")?,
            };
            Ok((target, args))
        });
        let (target, args) = match request {
            Ok(r) => r,
            Err(why) => {
                // Not inside the call: QML connects to `answered` after it
                // has the serial in hand.
                let why = format!("{why:#}");
                e.call_at(e.now(), move |e| answer(e, me, serial, &tool, Err(why)));
                return Ok(serial);
            }
        };
        cx.set(InspectorClient::URL, target.url.clone())?;
        let body = json!({
            "jsonrpc": "2.0",
            "id": serial,
            "method": "tools/call",
            "params": { "name": tool, "arguments": args },
        });
        let request = Request::new(Method::Post, target.url)
            .header("Authorization", format!("Bearer {}", target.token))
            .header("Accept", "application/json, text/event-stream")
            .json(&body.to_string());
        Platform::of(e).request(request, move |e, got| {
            let got = match got {
                Ok(response) => read_answer(&response),
                Err(why) => Err(Unreached(why.to_string())),
            };
            answer_reached(e, me, serial, &tool, got);
        });
        Ok(serial)
    }
}

/// The inspector could not be asked at all.
struct Unreached(String);

/// What a response says: the tool's result, a refusal (reached), or no
/// answer (unreached).
fn read_answer(response: &Response) -> Result<Result<Json, String>, Unreached> {
    let text = String::from_utf8_lossy(&response.body);
    if !response.ok() {
        return Err(Unreached(format!(
            "the inspector answered {}: {}",
            response.status,
            text.trim()
        )));
    }
    // A streaming answer carries the message on its `data:` line.
    let json = text
        .lines()
        .find_map(|l| l.strip_prefix("data:"))
        .unwrap_or(&text);
    let body: Json = serde_json::from_str(json.trim())
        .map_err(|e| Unreached(format!("the inspector's answer is not JSON: {e}")))?;
    if let Some(error) = body.get("error") {
        let message = error["message"].as_str().unwrap_or("refused");
        return Ok(Err(message.to_owned()));
    }
    Ok(Ok(body["result"].clone()))
}

fn answer_reached(
    e: &Engine,
    me: ObjectId,
    serial: i32,
    tool: &str,
    got: Result<Result<Json, String>, Unreached>,
) {
    let cx = Cx::new(e, me);
    let (connected, error) = match &got {
        Ok(_) => (true, String::new()),
        Err(Unreached(why)) => (false, why.clone()),
    };
    let _ = cx.set(InspectorClient::CONNECTED, connected);
    let _ = cx.set(InspectorClient::ERROR, error);
    let result = match got {
        Ok(Ok(result)) if result["isError"] != json!(true) => Ok(result.to_string()),
        Ok(Ok(result)) => Err(tool_error(&result)),
        Ok(Err(why)) | Err(Unreached(why)) => Err(why),
    };
    answer(e, me, serial, tool, result);
}

/// The text a tool's refusal gives, for the window to show as it is.
fn tool_error(result: &Json) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn answer(e: &Engine, me: ObjectId, serial: i32, tool: &str, result: Result<String, String>) {
    if !e.is_alive(me) {
        return;
    }
    let cx = Cx::new(e, me);
    if let Ok(pending) = cx.get(InspectorClient::PENDING) {
        let _ = cx.set(InspectorClient::PENDING, (pending - 1).max(0));
    }
    let (ok, text) = match result {
        Ok(text) => (true, text),
        Err(why) => (false, why),
    };
    let _ = InspectorClient::emit_answered(&cx, serial, tool.to_owned(), ok, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_live_token_file_in_a_directory_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let below = dir.path().join("target/ljos");
        std::fs::create_dir_all(&below).unwrap();
        let file = |name: &str, pid: u32| {
            let path = below.join(name);
            let body = json!({"url": "http://127.0.0.1:1/mcp", "token": "t", "pid": pid});
            std::fs::write(&path, body.to_string()).unwrap();
            path
        };
        let mine = file("mcp-1.json", std::process::id());
        // No process has this pid: the kernel's limit is below it.
        file("mcp-2.json", u32::MAX - 1);
        assert_eq!(find_token_file(dir.path()).unwrap(), mine);
        assert_eq!(find_token_file(&mine).unwrap(), mine);
    }

    #[test]
    fn a_directory_without_an_inspector_says_how_to_start_one() {
        let dir = tempfile::tempdir().unwrap();
        let why = find_token_file(dir.path()).unwrap_err().to_string();
        assert!(why.contains("ljos run --mcp"), "{why}");
    }

    #[test]
    fn a_socket_target_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-1.json");
        let body = json!({"url": "unix:/run/x.sock", "token": "t", "pid": 1});
        std::fs::write(&path, body.to_string()).unwrap();
        let why = read_token_file(&path).unwrap_err().to_string();
        assert!(why.contains("HTTP only"), "{why}");
    }

    #[test]
    fn the_window_compiles() {
        let engine = ljos::engine();
        register(&engine);
        compile(&engine).unwrap();
    }

    #[test]
    fn an_answer_is_read_plain_or_streamed() {
        let response = |body: &str| Response {
            status: 200,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        };
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{"isError":false}}"#;
        assert!(matches!(read_answer(&response(plain)), Ok(Ok(_))));
        let streamed = format!("event: message\ndata: {plain}\n\n");
        assert!(matches!(read_answer(&response(&streamed)), Ok(Ok(_))));
        let refused = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"no"}}"#;
        assert!(matches!(read_answer(&response(refused)), Ok(Err(m)) if m == "no"));
    }
}
