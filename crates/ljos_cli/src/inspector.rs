//! A client of a running application's inspector (ARCHITECTURE.md §19.7):
//! the token file that names it, and one JSON-RPC message posted to it as
//! streamable HTTP, on the loopback address or on its 0600 Unix socket.
//! `ljos observe` and `ljos attach` are its two users.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, bail};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use serde_json::Value as Json;
use tokio::io::{AsyncRead, AsyncWrite};

/// What a token file says of the inspector that wrote it.
#[derive(Debug, Clone)]
pub struct Target {
    pub url: String,
    pub token: String,
    pub pid: Option<u32>,
}

pub fn read_token_file(path: &Path) -> anyhow::Result<Target> {
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
    // Said now rather than as a connection that fails on every call.
    Address::of(&url)?;
    Ok(Target {
        url,
        token: field("token")?,
        pid: body["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
    })
}

/// The token file `target` names, or the newest live one in it.
pub fn find_token_file(target: &Path) -> anyhow::Result<PathBuf> {
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
    ljos_mcp_core::process_alive(pid) != Some(false)
}

/// Where a token file's `url` points.
#[derive(Debug, PartialEq, Eq)]
enum Address {
    /// `http://host:port/path`.
    Tcp { authority: String, path: String },
    /// `unix:PATH`: the inspector's socket, served at `/mcp`.
    Unix(PathBuf),
}

impl Address {
    fn of(url: &str) -> anyhow::Result<Address> {
        if let Some(path) = url.strip_prefix("unix:") {
            return Ok(Address::Unix(PathBuf::from(path)));
        }
        let Some(rest) = url.strip_prefix("http://") else {
            bail!("{url} is neither an `http://` address nor a `unix:` socket");
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            bail!("{url} names no host");
        }
        Ok(Address::Tcp {
            authority: authority.to_owned(),
            path: path.to_owned(),
        })
    }
}

/// What the inspector sent back: its status and the body, as it came.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Answer {
    /// The JSON-RPC messages of the body: the `data:` lines of a stream,
    /// or the body itself when it is plain JSON.
    pub fn messages(&self) -> Vec<&str> {
        let text = std::str::from_utf8(&self.body).unwrap_or("");
        let data: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        match (data.is_empty(), text.trim()) {
            (true, "") => Vec::new(),
            (true, plain) => vec![plain],
            (false, _) => data,
        }
    }
}

/// Posts `message` (one JSON-RPC message as text) to `target`; `Err` is
/// why the inspector could not be reached at all. Gives up after `limit`.
pub async fn post(target: &Target, message: String, limit: Duration) -> Result<Answer, String> {
    match tokio::time::timeout(limit, post_now(target, message)).await {
        Ok(answer) => answer,
        Err(_) => Err(format!(
            "the inspector at {} did not answer within {} s",
            target.url,
            limit.as_secs()
        )),
    }
}

async fn post_now(target: &Target, message: String) -> Result<Answer, String> {
    let address = Address::of(&target.url).map_err(|e| e.to_string())?;
    let fail = |e: std::io::Error| format!("{}: {e}", target.url);
    match address {
        Address::Tcp { authority, path } => {
            let stream = tokio::net::TcpStream::connect(authority.as_str())
                .await
                .map_err(fail)?;
            exchange(stream, &authority, &path, &target.token, message).await
        }
        #[cfg(unix)]
        Address::Unix(socket) => {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .map_err(fail)?;
            // The inspector answers only requests addressed to this
            // machine, whichever way they come.
            exchange(stream, "localhost", "/mcp", &target.token, message).await
        }
        #[cfg(not(unix))]
        Address::Unix(_) => Err(format!("{}: no Unix sockets here", target.url)),
    }
}

async fn exchange<S>(
    stream: S,
    host: &str,
    path: &str,
    token: &str,
    message: String,
) -> Result<Answer, String>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|e| e.to_string())?;
    // Ends when `sender` is dropped and the answer is read.
    tokio::spawn(connection);
    let request = http::Request::post(path)
        .header(http::header::HOST, host)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(http::header::ACCEPT, "application/json, text/event-stream")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(message)))
        .map_err(|e| e.to_string())?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|e| e.to_string())?
        .to_bytes();
    Ok(Answer {
        status,
        body: body.to_vec(),
    })
}

/// Calls in flight on the client thread at most; one more is refused
/// rather than queued, as the inspector refuses one past its own queue.
pub const IN_FLIGHT: usize = 16;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Thread {
    tx: tokio::sync::mpsc::UnboundedSender<Job>,
    in_flight: Arc<AtomicUsize>,
}

/// Runs `job` on the one client thread of the process, made the first
/// time it is asked for; `Err` says why it was not run. A thread per call
/// would be born, wake the process and end each time; this one sleeps in
/// its reactor between calls, with no timer, so an idle window costs
/// nothing.
pub fn spawn(job: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
    static THREAD: OnceLock<Result<Thread, String>> = OnceLock::new();
    let thread = THREAD.get_or_init(start).as_ref().map_err(Clone::clone)?;
    let in_flight = Arc::clone(&thread.in_flight);
    if in_flight.fetch_add(1, Ordering::AcqRel) >= IN_FLIGHT {
        in_flight.fetch_sub(1, Ordering::AcqRel);
        return Err(format!(
            "busy: {IN_FLIGHT} calls are waiting for the inspector already"
        ));
    }
    let job = Box::pin(async move {
        job.await;
        in_flight.fetch_sub(1, Ordering::AcqRel);
    });
    thread
        .tx
        .send(job)
        .map_err(|_| "the client thread has ended".to_owned())
}

fn start() -> Result<Thread, String> {
    // Unbounded in type only: `spawn` counts what is in flight.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|e| format!("no runtime for the client: {e}"))?;
    std::thread::Builder::new()
        .name("ljos-observe".to_owned())
        .spawn(move || {
            runtime.block_on(async move {
                while let Some(job) = rx.recv().await {
                    tokio::spawn(job);
                }
            });
        })
        .map_err(|e| format!("no thread for the client: {e}"))?;
    Ok(Thread {
        tx,
        in_flight: Arc::new(AtomicUsize::new(0)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn an_address_is_a_port_or_a_socket() {
        assert_eq!(
            Address::of("http://127.0.0.1:7/mcp").unwrap(),
            Address::Tcp {
                authority: "127.0.0.1:7".into(),
                path: "/mcp".into()
            }
        );
        assert_eq!(
            Address::of("unix:/run/x.sock").unwrap(),
            Address::Unix("/run/x.sock".into())
        );
        let why = Address::of("https://example.com/mcp").unwrap_err();
        assert!(why.to_string().contains("unix:"), "{why}");
    }

    #[test]
    fn an_answer_is_read_plain_or_streamed() {
        let answer = |body: &str| Answer {
            status: 200,
            body: body.as_bytes().to_vec(),
        };
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        assert_eq!(answer(plain).messages(), [plain]);
        let streamed = format!("event: message\ndata: {plain}\n\nid: 2\ndata: {plain}\n\n");
        assert_eq!(answer(&streamed).messages(), [plain, plain]);
        assert!(answer("").messages().is_empty());
    }

    /// An inspector with one tool, `echo`, as `ljos run --mcp` serves one.
    struct Echo(ljos_mcp_core::ToolRegistry);

    impl ljos_mcp_core::McpSurface for Echo {
        fn meta(&self) -> ljos_mcp_core::ServerMeta {
            ljos_mcp_core::ServerMeta {
                name: "echo",
                version: "0",
                instructions: String::new(),
            }
        }
        fn tools(&self) -> &ljos_mcp_core::ToolRegistry {
            &self.0
        }
        fn call_tool<'a>(
            &'a self,
            name: &'a str,
            ctx: &'a ljos_mcp_core::CallCtx,
            args: Json,
        ) -> ljos_mcp_core::ToolFuture<'a> {
            Box::pin(async move {
                let (v, _) = self.0.call(name, ctx, args);
                Ok(ljos_mcp_core::ToolReply::from(v?))
            })
        }
    }

    fn serve(
        dir: &Path,
        listen: ljos_mcp_core::glue::http::Listen,
    ) -> ljos_mcp_core::glue::http::HttpServer {
        use ljos_mcp_core::glue::http::{HttpConfig, HttpServer};
        use ljos_mcp_core::{Cap, CapSet, ToolDef, ToolRegistry, typed};
        let echo = Echo(ToolRegistry::new().with(
            ToolDef::new::<Json>("echo", "Says it back."),
            typed(|_, a: Json| Ok(json!({ "said": a }))),
        ));
        let mut config =
            HttpConfig::loopback(CapSet::of([Cap::Inspect])).token_file(dir.join("mcp-1.json"));
        config.listen = listen;
        HttpServer::start(Arc::new(echo), config).unwrap()
    }

    /// `echo` called through the client thread, as the observer calls.
    fn echo(token_file: &Path) -> Result<Answer, String> {
        let target = read_token_file(token_file).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"say":"hi"}}}"#;
        spawn(async move {
            let _ = tx.send(post(&target, call.to_owned(), Duration::from_secs(20)).await);
        })
        .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    }

    fn said(answer: &Answer) -> Json {
        assert_eq!(answer.status, 200, "{:?}", answer.messages());
        let message: Json = serde_json::from_str(answer.messages()[0]).unwrap();
        message["result"]["structuredContent"]["said"].clone()
    }

    #[test]
    fn an_inspector_is_reached_on_its_port_and_on_its_socket() {
        use ljos_mcp_core::glue::http::Listen;
        let dir = tempfile::Builder::new().prefix("lj").tempdir().unwrap();
        let mut on_port = serve(dir.path(), Listen::Loopback(0));
        let answer = echo(&dir.path().join("mcp-1.json")).unwrap();
        assert_eq!(said(&answer), json!({"say": "hi"}));
        on_port.stop();

        let socket = dir.path().join("m.sock");
        let mut on_socket = serve(dir.path(), Listen::Unix(socket.clone()));
        let target = read_token_file(&dir.path().join("mcp-1.json")).unwrap();
        assert_eq!(target.url, format!("unix:{}", socket.display()));
        let answer = echo(&dir.path().join("mcp-1.json")).unwrap();
        assert_eq!(said(&answer), json!({"say": "hi"}));
        on_socket.stop();
    }

    #[test]
    fn an_inspector_that_is_gone_is_said_to_be_unreached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-1.json");
        let gone = dir.path().join("gone.sock");
        let body = json!({"url": format!("unix:{}", gone.display()), "token": "t"});
        std::fs::write(&path, body.to_string()).unwrap();
        let why = echo(&path).unwrap_err();
        assert!(why.contains("gone.sock"), "{why}");
    }
}
