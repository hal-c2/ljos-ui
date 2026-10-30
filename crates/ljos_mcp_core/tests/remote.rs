//! The client of an inspector in another process (`remote`), against an
//! inspector's HTTP server served in this one: on its port and on its
//! socket, from async code and from code that blocks.
#![cfg(feature = "remote")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ljos_mcp_core::glue::http::{HttpConfig, HttpServer, Listen};
use ljos_mcp_core::remote::{
    Client, Image, ToolError, Trust, token_file_of, wait_for_token, wait_for_token_blocking,
};
use ljos_mcp_core::{
    Cap, CapSet, ErrorCode, McpSurface, ServerMeta, SurfaceError, ToolDef, ToolRegistry, ToolReply,
    typed,
};
use serde_json::{Value, json};

/// The start of every PNG, then a little more, as a screenshot is.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";

/// An inspector with a tool that says its arguments back, one that refuses
/// with a code and details, and one that answers with a picture.
struct Demo(ToolRegistry);

impl Demo {
    fn new() -> Arc<Self> {
        Arc::new(Self(
            ToolRegistry::new()
                .with(
                    ToolDef::new::<Value>("echo", "Says it back."),
                    typed(|_, a: Value| Ok(json!({ "said": a }))),
                )
                .with(
                    ToolDef::new::<Value>("forget", "Refuses: the ref is gone."),
                    typed(|_, _: Value| -> Result<Value, SurfaceError> {
                        Err(SurfaceError::new(ErrorCode::StaleRef, "e3 is gone")
                            .with_details(json!({ "ref": "e3", "snapshot": 2 })))
                    }),
                )
                .with(
                    ToolDef::new::<Value>("app_screenshot", "A picture of the window."),
                    typed(|_, _: Value| Ok(json!({ "width": 1, "height": 1 }))),
                ),
        ))
    }
}

impl McpSurface for Demo {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: "demo",
            version: "7",
            instructions: String::from("Ask it."),
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
            let reply = ToolReply::from(v?);
            Ok(match name {
                "app_screenshot" => reply
                    .with_text("1×1 pixels")
                    .with_image("image/png", PNG.to_vec()),
                _ => reply,
            })
        })
    }
}

/// The token file this process's inspector writes in `dir`, named as a
/// client looking for it by pid expects.
fn token_file(dir: &Path) -> PathBuf {
    dir.join(format!("mcp-{}.json", std::process::id()))
}

fn serve(dir: &Path, listen: Listen) -> HttpServer {
    let mut config = HttpConfig::loopback(CapSet::of([Cap::Inspect])).token_file(token_file(dir));
    config.listen = listen;
    HttpServer::start(Demo::new(), config).unwrap()
}

async fn the_tools_answer(client: &Client) {
    let server = client.initialize().await.unwrap();
    assert_eq!(server["serverInfo"]["name"], "demo", "{server}");
    assert_eq!(server["instructions"], "Ask it.", "{server}");

    let said = client.call_tool("echo", json!({ "say": "hi" })).await;
    assert_eq!(said.unwrap().structured, json!({ "said": { "say": "hi" } }));

    let names: Vec<String> = client
        .list_tools()
        .await
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_owned))
        .collect();
    // In the order they were registered.
    assert_eq!(names, ["echo", "forget", "app_screenshot"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_inspector_is_called_on_its_port_and_on_its_socket() {
    let dir = tempfile::Builder::new().prefix("lj").tempdir().unwrap();
    let mut on_port = serve(dir.path(), Listen::Loopback(0));
    let client = Client::for_pid(dir.path(), std::process::id()).unwrap();
    the_tools_answer(&client).await;
    on_port.stop().unwrap();

    let socket = dir.path().join("m.sock");
    let mut on_socket = serve(dir.path(), Listen::Unix(socket.clone()));
    let client = Client::from_token_file(token_file(dir.path()), Trust::FOUND).unwrap();
    assert_eq!(
        client.target().unwrap().url,
        format!("unix:{}", socket.display())
    );
    the_tools_answer(&client).await;
    on_socket.stop().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_refuses_keeps_its_code_and_details() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = serve(dir.path(), Listen::Loopback(0));
    let client = Client::for_pid(dir.path(), std::process::id()).unwrap();
    let why = client.call_tool("forget", json!({})).await.unwrap_err();
    assert_eq!(why.code(), Some(ErrorCode::StaleRef), "{why}");
    assert_eq!(why.details(), Some(&json!({ "ref": "e3", "snapshot": 2 })));
    let ToolError::Tool { code, message, .. } = &why else {
        panic!("not a tool's refusal: {why:?}");
    };
    assert_eq!(
        (code.as_deref(), message.as_str()),
        (Some("stale_ref"), "e3 is gone")
    );
    assert_eq!(why.to_string(), "stale_ref: e3 is gone");

    // A tool there is none of is the protocol's refusal, not a tool's.
    let why = client.call_tool("nothing", json!({})).await.unwrap_err();
    assert!(
        matches!(why, ToolError::Protocol { .. } | ToolError::Tool { .. }),
        "{why:?}"
    );
    assert!(why.to_string().contains("nothing"), "{why}");
    server.stop().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_screenshot_comes_through_as_its_png() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = serve(dir.path(), Listen::Loopback(0));
    let client = Client::for_pid(dir.path(), std::process::id()).unwrap();
    let shot = client.call_tool("app_screenshot", json!({})).await.unwrap();
    assert_eq!(
        shot.images,
        [Image {
            mime: "image/png".into(),
            data: PNG.to_vec()
        }]
    );
    assert_eq!(shot.text.as_deref(), Some("1×1 pixels"));
    assert_eq!(shot.structured, json!({ "width": 1, "height": 1 }));
    server.stop().unwrap();
}

/// The blocking client, called from a worker thread of a runtime: it
/// neither enters nor blocks on that runtime, which goes on serving.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_blocking_client_may_be_called_on_a_runtimes_thread() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = serve(dir.path(), Listen::Loopback(0));
    let client = Client::for_pid(dir.path(), std::process::id())
        .unwrap()
        .blocking();
    let server_info = client.initialize().unwrap();
    assert_eq!(server_info["serverInfo"]["version"], "7");
    let said = client.call_tool("echo", json!({ "n": 1 })).unwrap();
    assert_eq!(said.structured, json!({ "said": { "n": 1 } }));
    let why = client.call_tool("forget", json!({})).unwrap_err();
    assert_eq!(why.code(), Some(ErrorCode::StaleRef));
    assert_eq!(client.list_tools().unwrap().len(), 3);
    // The runtime this test runs on still runs tasks.
    assert_eq!(tokio::spawn(async { 2 }).await.unwrap(), 2);
    server.stop().unwrap();
}

#[test]
fn the_blocking_client_needs_no_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = serve(dir.path(), Listen::Loopback(0));
    let client = Client::for_pid(dir.path(), std::process::id())
        .unwrap()
        .blocking();
    let shot = client.call_tool("app_screenshot", json!({})).unwrap();
    assert_eq!(shot.images[0].data, PNG);
    server.stop().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_file_is_waited_for_until_the_inspector_writes_it() {
    let dir = tempfile::tempdir().unwrap();
    let below = dir.path().join("target/ljos");
    let at = below.clone();
    let starting = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        serve(&at, Listen::Loopback(0))
    });
    let path = wait_for_token(dir.path(), std::process::id(), Duration::from_secs(20))
        .await
        .unwrap();
    let mut server = starting.join().unwrap();
    assert_eq!(path, token_file(&below));
    assert_eq!(
        wait_for_token_blocking(dir.path(), std::process::id(), Duration::ZERO).unwrap(),
        path
    );
    assert_eq!(token_file_of(dir.path(), std::process::id()).unwrap(), path);
    let client = Client::for_pid(dir.path(), std::process::id()).unwrap();
    the_tools_answer(&client).await;
    server.stop().unwrap();
}

/// An inspector gone after the client was made is said to be unreached,
/// naming where it was looked for.
#[tokio::test(flavor = "multi_thread")]
async fn an_inspector_that_is_gone_is_said_to_be_unreached() {
    let dir = tempfile::Builder::new().prefix("lj").tempdir().unwrap();
    let socket = dir.path().join("gone.sock");
    let mut server = serve(dir.path(), Listen::Unix(socket.clone()));
    let target = Client::for_pid(dir.path(), std::process::id())
        .unwrap()
        .target()
        .unwrap();
    server.stop().unwrap();
    let why = Client::new(target)
        .call_tool("echo", json!({}))
        .await
        .unwrap_err();
    assert!(matches!(why, ToolError::Unreached(_)), "{why:?}");
    assert!(why.to_string().contains("gone.sock"), "{why}");
}
