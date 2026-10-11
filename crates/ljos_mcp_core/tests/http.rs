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

#[cfg(unix)]
#[test]
fn the_token_file_is_the_owners_and_goes_when_the_server_stops() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp-1.json");
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
    rig.server.stop();
    assert!(!path.exists());
    // Nothing is left listening.
    assert!(std::net::TcpStream::connect(addr).is_err());
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
    rig.server.stop();
    let mut buf = [0u8; 8];
    // End of stream (or a reset), not a timeout.
    match idle.read(&mut buf) {
        Ok(0) => {}
        Ok(n) => panic!("{n} unexpected bytes"),
        Err(e) => assert_ne!(e.kind(), std::io::ErrorKind::WouldBlock, "{e}"),
    }
    assert!(std::net::TcpStream::connect(addr).is_err());
}

#[cfg(unix)]
#[test]
fn a_unix_socket_is_its_owners_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
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
    server.stop();
    assert!(!path.exists());
}
