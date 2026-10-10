//! In-process protocol test: an rmcp client talks to a tiny surface over a
//! duplex pipe, so the glue, capability gate and size cap run end to end.

use std::sync::Arc;

use ljos_mcp_core::{
    Cap, CapSet, McpSurface, NoArgs, ServerMeta, ToolDef, ToolRegistry, glue::RmcpServer, typed,
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Value, json};

struct Demo(ToolRegistry);

impl Demo {
    fn new() -> Self {
        Self(
            ToolRegistry::new()
                .with(
                    ToolDef::new::<NoArgs>("big", "Returns far more than the budget allows.")
                        .max_result_bytes(2048),
                    typed(|_, _: NoArgs| {
                        let items: Vec<String> = (0..2000).map(|i| format!("row-{i}")).collect();
                        Ok(json!({ "items": items }))
                    }),
                )
                .with(
                    ToolDef::new::<NoArgs>("write", "Needs the fs capability.").requires(Cap::Fs),
                    typed(|_, _: NoArgs| Ok(json!({ "written": true }))),
                ),
        )
    }
}

impl McpSurface for Demo {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: "demo",
            version: "0",
            instructions: String::new(),
        }
    }
    fn tools(&self) -> &ToolRegistry {
        &self.0
    }
}

async fn connect(caps: CapSet) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let _ = RmcpServer::new(Arc::new(Demo::new()), caps)
            .serve(server_io)
            .await;
    });
    ().serve(client_io).await.expect("client handshake")
}

fn args() -> serde_json::Map<String, Value> {
    serde_json::Map::new()
}

#[tokio::test]
async fn denied_tools_are_hidden_but_calls_report_capability_denied() {
    let client = connect(CapSet::EMPTY).await;
    let names: Vec<_> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(names, ["big"]);

    let res = client
        .call_tool(CallToolRequestParams::new("write").with_arguments(args()))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    let body = res.structured_content.unwrap();
    assert_eq!(body["error"]["code"], "capability_denied");
    assert_eq!(body["error"]["details"]["required"], "fs");
}

#[tokio::test]
async fn granted_capability_unlocks_the_tool() {
    let client = connect(CapSet::of([Cap::Fs])).await;
    let res = client
        .call_tool(CallToolRequestParams::new("write").with_arguments(args()))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(res.structured_content.unwrap(), json!({ "written": true }));
}

#[tokio::test]
async fn oversize_results_are_truncated_with_a_cursor_and_text_mirror() {
    let client = connect(CapSet::EMPTY).await;
    let res = client
        .call_tool(CallToolRequestParams::new("big").with_arguments(args()))
        .await
        .unwrap();
    let body = res.structured_content.clone().unwrap();
    assert_eq!(body["truncated"], true);
    assert!(body["next_cursor"].is_string());
    let text = res.content[0].as_text().unwrap().text.clone();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), body);
    // The budget covers both copies, not just the structured one.
    let both = serde_json::to_vec(&body).unwrap().len() + text.len();
    assert!(both <= 2048, "structured + text mirror = {both} bytes");
}
