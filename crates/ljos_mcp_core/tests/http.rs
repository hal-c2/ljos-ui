//! The HTTP server over real loopback sockets: who is answered and who is
//! turned away (ARCHITECTURE.md §19.7), written as a client that knows
//! nothing but HTTP/1.1 would write it.
#![cfg(feature = "http")]

use std::io::{Read, Write};
use std::sync::Arc;

use ljos_mcp_core::glue::http::{HttpConfig, HttpServer, Listen};
use ljos_mcp_core::{Cap, CapSet, McpSurface, ServerMeta, ToolDef, ToolRegistry, ToolReply, typed};
use serde_json::{Value, json};

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Say {
    say: String,
}

struct Demo(ToolRegistry);

impl Demo {
    fn new() -> Arc<Self> {
        Arc::new(Self(ToolRegistry::new().with(
            ToolDef::new::<Say>("echo", "Says it back."),
            typed(|_, a: Say| Ok(json!({ "said": a.say }))),
        )))
    }
}

impl McpSurface for Demo {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: "demo",
            version: "0",
            instructions: String::from("Say something."),
        }
    }
    fn tools(&self) -> &ToolRegistry {
        &self.0
    }
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        ctx: &'a ljos_mcp_core::CallCtx,
        args: Value,
    ) -> ljos_mcp_core::ToolFuture<'a> {
        Box::pin(async move {
            let (v, _) = self.0.call(name, ctx, args);
            Ok(ToolReply::from(v?).with_text("said it"))
        })
    }
}

struct Answer {
    status: u16,
    head: String,
    body: String,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// `7\r\nMozilla\r\n0\r\n\r\n` is `Mozilla`.
fn unchunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap();
        if n == 0 {
            break;
        }
        out.push_str(&after[..n]);
        rest = &after[n + 2..];
    }
    out
}

fn exchange(
    mut stream: impl Read + Write,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Answer {
    let mut request = format!("POST {path} HTTP/1.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no status line in {raw:?}"));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        unchunk(body)
    } else {
        body.to_string()
    };
    Answer {
        status,
        head: head.to_string(),
        body,
    }
}

const CALL: &str = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"echo","arguments":{"say":"hello"}}}"#;

/// What a well-behaved client sends, but for the token and the host.
fn mcp_headers<'a>(host: &'a str, auth: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut h = vec![
        ("Host", host),
        ("Accept", "application/json, text/event-stream"),
        ("Content-Type", "application/json"),
    ];
    if !auth.is_empty() {
        h.push(("Authorization", auth));
    }
    h
}

struct Rig {
    server: HttpServer,
    host: String,
    bearer: String,
}

impl Rig {
    fn start(config: HttpConfig) -> Rig {
        let server = HttpServer::start(Demo::new(), config).unwrap();
        let addr = server.addr().unwrap();
        Rig {
            host: addr.to_string(),
            bearer: format!("Bearer {}", server.token().as_str()),
            server,
        }
    }

    fn post(&self, path: &str, headers: &[(&str, &str)], body: &str) -> Answer {
        let stream = std::net::TcpStream::connect(self.server.addr().unwrap()).unwrap();
        exchange(stream, path, headers, body)
    }

    fn call(&self, auth: &str) -> Answer {
        self.post("/mcp", &mcp_headers(&self.host, auth), CALL)
    }
}

/// Nothing answers the stopped server's token at `addr`. A refused connection
/// is not asked of it: the port is free again, so a server another test starts
/// may have it already, and a connect to a free ephemeral port can even meet
/// itself. Neither knows this token.
fn assert_not_served(addr: std::net::SocketAddr, bearer: &str) {
    let Ok(mut stream) = std::net::TcpStream::connect(addr) else {
        return;
    };
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nConnection: close\r\nHost: {addr}\r\nAuthorization: {bearer}\r\nContent-Length: {}\r\n\r\n{CALL}",
        CALL.len()
    );
    // Whoever has the port may close it or say nothing: only an answer counts.
    let _ = stream.write_all(request.as_bytes());
    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw);
    assert!(!raw.starts_with("HTTP/1.1 200"), "still served: {raw}");
}

fn config() -> HttpConfig {
    HttpConfig::loopback(CapSet::of([Cap::Inspect]))
}

#[test]
fn the_server_listens_on_the_loopback_address_only() {
    let rig = Rig::start(config());
    let addr = rig.server.addr().unwrap();
    assert!(addr.ip().is_loopback(), "{addr}");
    assert_eq!(rig.server.url(), format!("http://{addr}/mcp"));
}

#[test]
fn a_client_with_the_token_is_answered() {
    let rig = Rig::start(config());
    let init = rig.post(
        "/mcp",
        &mcp_headers(&rig.host, &rig.bearer),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
    );
    assert_eq!(init.status, 200, "{}\n{}", init.head, init.body);
    let init = init.json();
    assert_eq!(init["result"]["serverInfo"]["name"], "demo");
    assert_eq!(init["result"]["instructions"], "Say something.");

    let list = rig.post(
        "/mcp",
        &mcp_headers(&rig.host, &rig.bearer),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    assert_eq!(list.status, 200, "{}", list.body);
    assert_eq!(list.json()["result"]["tools"][0]["name"], "echo");

    let call = rig.call(&rig.bearer);
    assert_eq!(call.status, 200, "{}", call.body);
    let call = call.json();
    assert_eq!(call["id"], 7);
    assert_eq!(
        call["result"]["structuredContent"],
        json!({ "said": "hello" })
    );
    assert_eq!(call["result"]["content"][0]["text"], "said it");
    // `localhost` is this machine too.
    let port = rig.server.addr().unwrap().port();
    let by_name = rig.post(
        "/mcp",
        &mcp_headers(&format!("localhost:{port}"), &rig.bearer),
        CALL,
    );
    assert_eq!(by_name.status, 200, "{}", by_name.body);
}

#[test]
fn a_client_without_the_token_is_refused() {
    let rig = Rig::start(config());
    let none = rig.call("");
    assert_eq!(none.status, 401);
    assert!(
        none.head
            .to_ascii_lowercase()
            .contains("www-authenticate: bearer")
    );
    assert_eq!(none.json()["error"]["code"], "unauthorized");
    assert!(!none.body.contains("said"));

    let wrong = rig.call(&format!("Bearer {}", "0".repeat(64)));
    assert_eq!(wrong.status, 401);
    // The token itself, without the scheme, is not a bearer header.
    let bare = rig.call(rig.server.token().as_str());
    assert_eq!(bare.status, 401);
    // A token is for one start: another server's does not open this one.
    let other = Rig::start(config());
    assert_eq!(rig.call(&other.bearer).status, 401);
    assert_eq!(other.call(&other.bearer).status, 200);
}

#[test]
fn a_page_from_another_site_is_refused() {
    let rig = Rig::start(config());
    let mut headers = mcp_headers(&rig.host, &rig.bearer);
    headers.push(("Origin", "https://example.com"));
    let cross = rig.post("/mcp", &headers, CALL);
    assert_eq!(cross.status, 403, "{}", cross.body);
    assert_eq!(cross.json()["error"]["code"], "forbidden_origin");

    let mut headers = mcp_headers(&rig.host, &rig.bearer);
    headers.push(("Origin", "null"));
    assert_eq!(rig.post("/mcp", &headers, CALL).status, 403);

    // A tool run from a page on this machine (the MCP inspector) is let in.
    let mut headers = mcp_headers(&rig.host, &rig.bearer);
    headers.push(("Origin", "http://localhost:6274"));
    assert_eq!(rig.post("/mcp", &headers, CALL).status, 200);
}

#[test]
fn a_request_for_another_host_is_refused() {
    let rig = Rig::start(config());
    // DNS rebinding: the name is the attacker's, the address is ours.
    let rebound = rig.post("/mcp", &mcp_headers("example.com", &rig.bearer), CALL);
    assert_eq!(rebound.status, 403, "{}", rebound.body);
    assert_eq!(rebound.json()["error"]["code"], "forbidden_host");
    let port = rig.server.addr().unwrap().port();
    let other_port = format!("127.0.0.1:{}", port.wrapping_add(1).max(1));
    assert_eq!(
        rig.post("/mcp", &mcp_headers(&other_port, &rig.bearer), CALL)
            .status,
        403
    );
    // The refusal comes before the token is looked at: nothing is learnt
    // about the token from a page that cannot name the host.
    assert_eq!(
        rig.post("/mcp", &mcp_headers("example.com", ""), CALL)
            .status,
        403
    );
}

#[test]
fn only_the_mcp_path_is_served() {
    let rig = Rig::start(config());
    let other = rig.post("/", &mcp_headers(&rig.host, &rig.bearer), CALL);
    assert_eq!(other.status, 404);
    // And not to a stranger either: the token is asked for first.
    assert_eq!(rig.post("/", &mcp_headers(&rig.host, ""), CALL).status, 401);
}

#[test]
fn calls_faster_than_the_rate_are_refused() {
    let mut cfg = config();
    cfg.calls_per_second = 3;
    let rig = Rig::start(cfg);
    let statuses: Vec<u16> = (0..6).map(|_| rig.call(&rig.bearer).status).collect();
    assert_eq!(&statuses[..3], [200, 200, 200]);
    assert!(statuses[3..].contains(&429), "{statuses:?}");
    let refused = rig.call(&rig.bearer);
    if refused.status == 429 {
        assert!(refused.head.to_ascii_lowercase().contains("retry-after: 1"));
        assert_eq!(refused.json()["error"]["code"], "rate_limited");
    }
    // Refusals for want of a token are not counted: a stranger cannot use
    // up the owner's calls.
    let rig = Rig::start({
        let mut c = config();
        c.calls_per_second = 3;
        c
    });
    for _ in 0..6 {
        assert_eq!(rig.call("").status, 401);
    }
    assert_eq!(rig.call(&rig.bearer).status, 200);
}

#[test]
fn a_granted_rate_is_answered_and_a_rate_outside_the_range_is_not_served() {
    let mut cfg = config();
    cfg.calls_per_second = 40;
    let rig = Rig::start(cfg);
    let statuses: Vec<u16> = (0..40).map(|_| rig.call(&rig.bearer).status).collect();
    assert!(statuses.iter().all(|s| *s == 200), "{statuses:?}");
    for rate in [0, 1001] {
        let mut cfg = config();
        cfg.calls_per_second = rate;
        let e = HttpServer::start(Demo::new(), cfg).err().unwrap();
        assert!(e.to_string().contains("from 1 to 1000"), "{rate}: {e}");
    }
}

#[cfg(unix)]
#[test]
fn the_token_file_is_the_owners_and_goes_when_the_server_stops() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    // A directory the server makes itself, so it is the user's alone whatever
    // the temporary directory's mode.
    let path = dir.path().join("ljos").join("mcp-1.json");
    let mut rig = Rig::start(config().token_file(&path));
    assert_eq!(rig.server.token_file(), Some(path.as_path()));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let body: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(body["url"], rig.server.url());
    assert_eq!(body["token"], rig.server.token().as_str());
    assert_eq!(body["capabilities"], json!(["inspect"]));
    // What the file says is enough to be answered.
    let bearer = format!("Bearer {}", body["token"].as_str().unwrap());
    assert_eq!(rig.call(&bearer).status, 200);

    let addr = rig.server.addr().unwrap();
    rig.server.stop().unwrap();
    assert!(!path.exists());
    assert_not_served(addr, &rig.bearer);
}

#[test]
fn stopping_closes_a_connection_that_was_left_open() {
    let mut rig = Rig::start(config());
    let addr = rig.server.addr().unwrap();
    // A client that connected and never asked anything.
    let mut idle = std::net::TcpStream::connect(addr).unwrap();
    idle.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    assert_eq!(rig.call(&rig.bearer).status, 200);
    rig.server.stop().unwrap();
    let mut buf = [0u8; 8];
    // End of stream (or a reset), not a timeout.
    match idle.read(&mut buf) {
        Ok(0) => {}
        Ok(n) => panic!("{n} unexpected bytes"),
        Err(e) => assert_ne!(e.kind(), std::io::ErrorKind::WouldBlock, "{e}"),
    }
    assert_not_served(addr, &rig.bearer);
}

/// A directory with a path short enough for a socket: `TMPDIR` may be
/// longer than the system allows, so these tests do not use it.
#[cfg(unix)]
fn short_dir() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt as _;
    tempfile::Builder::new()
        .prefix("lj")
        // A token file may go in it, and has to be the user's alone.
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/tmp")
        .expect("/tmp holds the socket")
}

#[cfg(unix)]
#[test]
fn a_unix_socket_path_that_is_too_long_is_refused_naming_the_path() {
    let dir = short_dir();
    let path = dir.path().join("s".repeat(200));
    let mut cfg = config();
    cfg.listen = Listen::Unix(path.clone());
    let err = HttpServer::start(Demo::new(), cfg).unwrap_err().to_string();
    assert!(err.contains(&path.display().to_string()), "{err}");
    assert!(err.contains(&path.as_os_str().len().to_string()), "{err}");
    assert!(err.contains("at most"), "{err}");
    assert!(err.contains("--mcp-socket"), "{err}");
    assert!(!path.exists(), "nothing was made, nothing moved");
}

#[cfg(unix)]
#[test]
fn a_unix_socket_is_its_owners_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = short_dir();
    let path = dir.path().join("mcp.sock");
    let mut cfg = config();
    cfg.listen = Listen::Unix(path.clone());
    let mut server = HttpServer::start(Demo::new(), cfg).unwrap();
    assert_eq!(server.addr(), None);
    assert_eq!(server.url(), format!("unix:{}", path.display()));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let bearer = format!("Bearer {}", server.token().as_str());
    let stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    let ok = exchange(stream, "/mcp", &mcp_headers("localhost", &bearer), CALL);
    assert_eq!(ok.status, 200, "{}", ok.body);
    // The file's mode is not the only lock: the token is still asked for.
    let stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    assert_eq!(
        exchange(stream, "/mcp", &mcp_headers("localhost", ""), CALL).status,
        401
    );
    server.stop().unwrap();
    assert!(!path.exists());
}

#[test]
fn an_address_given_on_purpose_is_served_and_told() {
    let mut cfg = config();
    cfg.listen = Listen::Address("127.0.0.1:0".parse().unwrap());
    let rig = Rig::start(cfg);
    let addr = rig.server.addr().unwrap();
    assert!(addr.ip().is_loopback() && addr.port() != 0, "{addr}");
    assert_eq!(rig.server.warning(), None);
    assert_eq!(rig.call(&rig.bearer).status, 200);
}

#[test]
fn an_address_beyond_this_machine_needs_the_second_word() {
    use ljos_mcp_core::glue::http::{HttpError, LAN_WARNING, bind_policy};
    let lan: std::net::SocketAddr = "0.0.0.0:7600".parse().unwrap();
    // Decided before anything is opened, so that no port is ever bound.
    let refused = bind_policy(lan, false).unwrap_err();
    let said = refused.to_string();
    assert!(said.contains("--mcp-bind 0.0.0.0:7600"), "{said}");
    assert!(said.contains("--mcp-insecure-lan"), "{said}");
    assert!(matches!(refused, HttpError::NotLoopback { .. }));
    assert_eq!(bind_policy(lan, true).unwrap(), Some(LAN_WARNING));
    assert_eq!(
        bind_policy("127.0.0.1:7600".parse().unwrap(), false).unwrap(),
        None
    );
    assert_eq!(
        bind_policy("[::1]:7600".parse().unwrap(), false).unwrap(),
        None
    );
    // The server itself refuses too, and says the same.
    let mut cfg = config();
    cfg.listen = Listen::Address(lan);
    let err = HttpServer::start(Demo::new(), cfg).err().unwrap();
    assert!(err.to_string().contains("--mcp-insecure-lan"), "{err}");
    assert!(
        LAN_WARNING.contains("anyone on the network"),
        "{LAN_WARNING}"
    );
}

/// The process the signal tests end: this test binary, started again with
/// the path of a token file. It serves until a signal comes and does not
/// return from the test function, so a signal is the only way out.
#[cfg(unix)]
fn serve_until_signalled(token: &std::path::Path, socket: &std::path::Path) -> ! {
    let mut cfg = config().token_file(token);
    cfg.listen = Listen::Unix(socket.to_path_buf());
    cfg.clean_up_on_signals = true;
    let _server = HttpServer::start(Demo::new(), cfg).unwrap();
    loop {
        std::thread::park();
    }
}

/// Starts the server in a child process, ends it with `signal` and returns
/// its exit code with whether its token file and socket are left.
#[cfg(unix)]
fn end_a_server_with(test: &str, signal: &str) -> (Option<i32>, bool, bool) {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let dir = short_dir();
    let token = dir.path().join("mcp-1.json");
    let socket = dir.path().join("mcp.sock");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--test-threads=1", "--nocapture"])
        .env("LJOS_TEST_SERVE_TOKEN", &token)
        .env("LJOS_TEST_SERVE_SOCKET", &socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    // The token file is written after the signal handlers are in place, so
    // its appearing is the moment a signal is certain to be handled.
    while !token.exists() {
        assert!(started.elapsed() < Duration::from_secs(30), "no token file");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(socket.exists());
    let sent = Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            panic!("the server did not end");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    (code, token.exists(), socket.exists())
}

#[cfg(unix)]
fn child_of(test: &str) {
    if let (Some(token), Some(socket)) = (
        std::env::var_os("LJOS_TEST_SERVE_TOKEN"),
        std::env::var_os("LJOS_TEST_SERVE_SOCKET"),
    ) {
        let _ = test;
        serve_until_signalled(token.as_ref(), socket.as_ref());
    }
}

#[cfg(unix)]
#[test]
fn sigterm_removes_the_token_file_and_the_socket() {
    child_of("sigterm_removes_the_token_file_and_the_socket");
    let (code, token, socket) =
        end_a_server_with("sigterm_removes_the_token_file_and_the_socket", "-TERM");
    assert_eq!(code, Some(143));
    assert!(!token, "the token file was left");
    assert!(!socket, "the socket was left");
}

#[cfg(unix)]
#[test]
fn sigint_removes_the_token_file_and_the_socket() {
    child_of("sigint_removes_the_token_file_and_the_socket");
    let (code, token, socket) =
        end_a_server_with("sigint_removes_the_token_file_and_the_socket", "-INT");
    assert_eq!(code, Some(130));
    assert!(!token, "the token file was left");
    assert!(!socket, "the socket was left");
}

/// Answers nothing until its call is dropped, and says when it was.
struct Waits {
    tools: ToolRegistry,
    dropped: std::sync::mpsc::SyncSender<()>,
}

struct Tell(std::sync::mpsc::SyncSender<()>);

impl Drop for Tell {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

impl McpSurface for Waits {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: "waits",
            version: "0",
            instructions: String::from("Waits."),
        }
    }
    fn tools(&self) -> &ToolRegistry {
        &self.tools
    }
    fn call_tool<'a>(
        &'a self,
        _name: &'a str,
        _ctx: &'a ljos_mcp_core::CallCtx,
        _args: Value,
    ) -> ljos_mcp_core::ToolFuture<'a> {
        let tell = Tell(self.dropped.clone());
        Box::pin(async move {
            std::future::pending::<()>().await;
            drop(tell);
            Ok(ToolReply::from(json!({})))
        })
    }
}

#[test]
fn a_call_whose_client_went_away_is_dropped() {
    let (dropped, told) = std::sync::mpsc::sync_channel(1);
    let surface = Arc::new(Waits {
        tools: ToolRegistry::new().with(
            ToolDef::new::<Say>("echo", "Waits for ever."),
            typed(|_, a: Say| Ok(json!({ "said": a.say }))),
        ),
        dropped,
    });
    let server = HttpServer::start(surface, config()).unwrap();
    let addr = server.addr().unwrap();
    let (host, bearer) = (
        addr.to_string(),
        format!("Bearer {}", server.token().as_str()),
    );
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let mut request = String::from("POST /mcp HTTP/1.1\r\n");
    for (k, v) in mcp_headers(&host, &bearer) {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{CALL}", CALL.len()));
    stream.write_all(request.as_bytes()).unwrap();
    // Not answered: the call is waiting.
    let wait = std::time::Duration::from_secs(5);
    assert!(
        told.recv_timeout(std::time::Duration::from_secs(6))
            .is_err()
    );
    drop(stream);
    // A watch dropped is what lets go of what its client held.
    told.recv_timeout(wait)
        .expect("the call outlived its client");
}

// These socket tests specify setup limits without adding raw TCP steps to the ledger.
#[test]
fn partial_headers_release_all_connection_permits() {
    let rig = Rig::start(config());
    let mut clients = Vec::new();
    for _ in 0..32 {
        let mut stream = std::net::TcpStream::connect(rig.server.addr().unwrap()).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(8)))
            .unwrap();
        stream.write_all(b"POST /mcp HTTP/1.1\r\nHost:").unwrap();
        clients.push(stream);
    }
    for mut stream in clients {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("partial header connection did not expire: {other:?}"),
        }
    }
    assert_eq!(rig.call(&rig.bearer).status, 200);
}

fn unfinished_body(rig: &Rig, framing: &str, body: &str) -> std::net::TcpStream {
    let mut stream = std::net::TcpStream::connect(rig.server.addr().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(8)))
        .unwrap();
    let mut request = String::from("POST /mcp HTTP/1.1\r\n");
    for (k, v) in mcp_headers(&rig.host, &rig.bearer) {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    request.push_str(&format!("{framing}\r\n\r\n{body}"));
    stream.write_all(request.as_bytes()).unwrap();
    stream
}

#[test]
fn incomplete_body_expires_and_releases_its_permit() {
    let mut cfg = config();
    cfg.max_connections = 1;
    let rig = Rig::start(cfg);
    let started = std::time::Instant::now();
    let mut stream = unfinished_body(&rig, "Content-Length: 100", "{");
    // Progress does not restart the total body deadline.
    for _ in 0..2 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        stream.write_all(b" ").unwrap();
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    assert!(started.elapsed() < std::time::Duration::from_secs(7));
    assert_eq!(rig.call(&rig.bearer).status, 200);
}

#[test]
fn oversized_fixed_and_chunked_bodies_are_closed() {
    let rig = Rig::start(config());
    for (framing, body) in [
        ("Content-Length: 1048577", String::new()),
        (
            "Transfer-Encoding: chunked",
            format!("100001\r\n{}\r\n0\r\n\r\n", "x".repeat(1_048_577)),
        ),
    ] {
        let mut stream = unfinished_body(&rig, framing, &body);
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 413"), "{response}");
        assert!(response.to_ascii_lowercase().contains("connection: close"));
    }
}

#[test]
fn an_unauthenticated_unfinished_body_is_closed_without_waiting() {
    let mut cfg = config();
    cfg.max_connections = 1;
    let mut rig = Rig::start(cfg);
    rig.bearer.clear();
    let mut stream = unfinished_body(&rig, "Content-Length: 100", "{");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 401"), "{response}");
    let bearer = format!("Bearer {}", rig.server.token().as_str());
    assert_eq!(rig.call(&bearer).status, 200);
}

#[test]
fn an_idle_connection_expires_before_sending_any_headers() {
    let mut cfg = config();
    cfg.max_connections = 1;
    let rig = Rig::start(cfg);
    let mut stream = std::net::TcpStream::connect(rig.server.addr().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(8)))
        .unwrap();
    assert_eq!(stream.read(&mut [0]).unwrap(), 0);
    assert_eq!(rig.call(&rig.bearer).status, 200);
}
