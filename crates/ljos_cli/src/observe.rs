//! `ljos observe`: a window on a running application, as Erlang's
//! observer is on a node (ARCHITECTURE.md §3.4, §19.5). It shows the
//! application's processes, its mailboxes and its objects, with their
//! properties for editing and a picture of the one picked.
//!
//! The window is a QML application like any other. What it shows comes
//! only from the inspector's MCP tools, which it calls as an agent would,
//! so there is nothing the window knows that an agent cannot ask. Its one
//! native type, `InspectorClient`, posts a tool call to the inspector's
//! port or Unix socket from the one client thread of the process
//! (`ljos_mcp_core::remote`) and answers on the Engine thread. Nothing is
//! asked on a timer: neither the window nor its target does work while
//! nobody touches the window.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use clap::Args;
use ljos::prelude::*;
use ljos_compiler::{Project, compile_file};
use ljos_engine::MapSource;
use ljos_mcp_core::CapSet;
use ljos_shell::{InspectorAccess, InspectorOptions, ShellConfig};
use serde_json::{Value as Json, json};

use ljos_mcp_core::remote::{self as inspector, Answer, Trust, find_token_file, read_token_file};

/// How far the token file `ljos observe` was started on is believed, kept
/// for every call `InspectorClient` reads its file again for. Before it is
/// set, a file is believed only as far as one found.
static TRUST: std::sync::OnceLock<Trust> = std::sync::OnceLock::new();

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
    /// Lets a token file named as the target give an address beyond this
    /// machine (an inspector started with `--mcp-bind`) or a socket of
    /// another user's. A file found in a directory never may.
    #[arg(long)]
    remote: bool,
    /// The window's size in logical pixels.
    #[arg(long, value_parser = crate::run::parse_size, default_value = "1200x760", value_name = "WxH")]
    size: (f32, f32),
    /// Serves an inspector on the observer's own window, as `ljos run
    /// --mcp` does for an application: what the observer costs is asked
    /// of it with the same tools.
    #[arg(
        long,
        value_name = "CAPS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "inspect",
        value_parser = crate::run::parse_caps
    )]
    mcp: Option<CapSet>,
    /// With `--mcp`: answer a client of the observer's own inspector this
    /// many calls a second, from 1 to 1000, as `ljos run --mcp-rate` does.
    #[arg(long, requires = "mcp", value_name = "N", value_parser = crate::run::calls_per_second)]
    mcp_rate: Option<u32>,
}

impl Observe {
    pub fn run(self) -> anyhow::Result<()> {
        let (token_file, trust) = find_token_file(&self.target)?;
        let trust = Trust {
            remote: self.remote,
            ..trust
        };
        let target = read_token_file(&token_file, trust)?;
        let _ = TRUST.set(trust);
        crate::stderr::line(&format!(
            "ljos: observing {} (pid {}) through {}",
            target.url,
            target.pid.map_or_else(|| "?".to_owned(), |p| p.to_string()),
            token_file.display()
        ));
        let config = ShellConfig::new(format!("{KEY}.qml")).with_size(self.size.0, self.size.1);
        let file = token_file.display().to_string();
        let mcp = self.mcp;
        let rate = self
            .mcp_rate
            .unwrap_or(ljos_mcp_core::DEFAULT_CALLS_PER_SECOND);
        ljos_shell::run::<()>(config, move |setup| {
            // The inspector prints the log as it keeps it.
            if let Some(caps) = mcp {
                let options = InspectorOptions::below(std::env::current_dir()?);
                let access = InspectorAccess {
                    caps,
                    clean_up_on_signals: true,
                    calls_per_second: rate,
                    ..InspectorAccess::default()
                };
                let at = setup.serve_inspector_as(options, access, crate::run::print_log)?;
                crate::stderr::line(&format!(
                    "ljos: the observer's inspector at {}, token in {}",
                    at.url,
                    at.token_file.display()
                ));
            } else {
                setup.engine().set_log_handler(crate::run::print_log);
            }
            let engine = setup.engine();
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
    /// The name the client gives itself in each call's `_meta`, so that
    /// the inspector can tell the window's own requests from others'.
    #[qml(notify)]
    pub client_name: String,
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

    /// The areas of the design rules the audit checks, comma separated, in
    /// the order of the rules' table: what the Audit page offers to keep
    /// the findings of. They come from the table, not from a list the
    /// window keeps, so a rule added there is offered here.
    #[qml(invokable)]
    fn audit_areas(_cx: &Cx) -> String {
        audit_areas().join(",")
    }

    /// Where a rule's guide (`docs/design/<page>#<code>`, as the audit
    /// gives it) can be read in a browser.
    #[qml(invokable)]
    fn guide_url(_cx: &Cx, guide: String) -> String {
        format!(
            "{}/blob/main/{guide}",
            env!("CARGO_PKG_REPOSITORY").trim_end_matches('/')
        )
    }

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
        let name = cx.get(InspectorClient::CLIENT_NAME)?;
        let (e, me) = (cx.engine, cx.this);
        let trust = TRUST.get().copied().unwrap_or(Trust::FOUND);
        let request = read_token_file(Path::new(&file), trust)
            .map_err(anyhow::Error::from)
            .and_then(|target| {
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
        let tool_name = tool.clone();
        // A watch is answered when its wait is over; anything else soon.
        let wait = args["wait_ms"].as_u64().unwrap_or(0);
        let limit = Duration::from_millis(wait) + PATIENCE;
        let mut params = json!({ "name": &tool, "arguments": args });
        if !name.is_empty() {
            params["_meta"] = json!({
                "io.modelcontextprotocol/clientInfo": {
                    "name": name,
                    "version": env!("CARGO_PKG_VERSION"),
                },
            });
        }
        let body = json!({
            "jsonrpc": "2.0",
            "id": serial,
            "method": "tools/call",
            "params": params,
        });
        let reply = Platform::of(e).expect_for("observer call", None);
        let job = async move {
            let got = match inspector::post(&target, body.to_string(), limit).await {
                Ok(answer) => read_answer(&answer),
                Err(why) => Err(Unreached(why.0)),
            };
            reply.send(move |e| answer_reached(e, me, serial, &tool, got));
        };
        if let Err(why) = inspector::spawn(job) {
            e.call_at(e.now(), move |e| {
                answer_reached(e, me, serial, &tool_name, Err(Unreached(why.0)));
            });
        }
        Ok(serial)
    }
}

/// The areas of the rules a running scene can be held to, each once.
fn audit_areas() -> Vec<String> {
    let mut areas: Vec<String> = Vec::new();
    for rule in ljos_design::RULES {
        if rule.check == ljos_design::Check::Static || rule.area == ljos_design::Area::Meta {
            continue;
        }
        let area = serde_json::to_value(rule.area)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned));
        if let Some(area) = area
            && !areas.contains(&area)
        {
            areas.push(area);
        }
    }
    areas
}

/// How long a call may take beyond the wait it asks for.
const PATIENCE: Duration = Duration::from_secs(30);

/// The inspector could not be asked at all.
struct Unreached(String);

/// What a response says: the tool's result, a refusal (reached), or no
/// answer (unreached).
fn read_answer(answer: &Answer) -> Result<Result<Json, String>, Unreached> {
    let text = String::from_utf8_lossy(&answer.body);
    // Turned away for asking too often: the inspector is there.
    if answer.status == 429 {
        return Ok(Err(format!("rate_limited: {}", text.trim())));
    }
    if !(200..300).contains(&answer.status) {
        return Err(Unreached(format!(
            "the inspector answered {}: {}",
            answer.status,
            text.trim()
        )));
    }
    let json = answer.messages().first().copied().unwrap_or("");
    let body: Json = serde_json::from_str(json)
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
    fn the_window_compiles() {
        let engine = ljos::engine();
        register(&engine);
        compile(&engine).unwrap();
    }

    #[test]
    fn an_answer_is_read_plain_or_streamed() {
        let response = |body: &str| Answer {
            status: 200,
            body: body.as_bytes().to_vec(),
            retry_after: None,
        };
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{"isError":false}}"#;
        assert!(matches!(read_answer(&response(plain)), Ok(Ok(_))));
        let streamed = format!("event: message\ndata: {plain}\n\n");
        assert!(matches!(read_answer(&response(&streamed)), Ok(Ok(_))));
        let refused = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"no"}}"#;
        assert!(matches!(read_answer(&response(refused)), Ok(Err(m)) if m == "no"));
    }

    #[test]
    fn the_audit_page_offers_the_areas_the_rules_table_has() {
        let areas = audit_areas();
        assert!(areas.contains(&"a11y".to_owned()), "{areas:?}");
        assert!(areas.contains(&"layout".to_owned()), "{areas:?}");
        // The audit takes each as its `rules` argument.
        for area in &areas {
            ljos_audit::Options::desktop().only(&[area]).unwrap();
        }
        let mut sorted = areas.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), areas.len());
    }

    #[test]
    fn an_inspector_asked_too_often_is_still_reached() {
        let busy = Answer {
            status: 429,
            body: br#"{"error":{"code":"rate_limited"}}"#.to_vec(),
            retry_after: None,
        };
        assert!(matches!(read_answer(&busy), Ok(Err(m)) if m.starts_with("rate_limited")));
    }
}
