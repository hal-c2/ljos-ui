//! In-process protocol tests: a real rmcp client against the authoring surface
//! over a duplex pipe, with a throwaway project directory.

use std::{path::Path, sync::Arc};

use ljos_mcp_author::{AuthorServer, Project, StubFrontEnd};
use ljos_mcp_core::{Cap, CapSet, glue::RmcpServer};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ReadResourceRequestParams},
    service::RunningService,
};
use serde_json::{Value, json};

const LEDGER: &str = include_str!("../../../ledger/qtquick.toml");

/// The project lives in `<tmp>/proj` so `<tmp>/outside-secret.txt` is a real
/// file outside it, and everything is removed with the temp dir.
struct Fixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Fixture {
    fn path(&self) -> &Path {
        &self.root
    }
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("proj");
    std::fs::create_dir(&root).unwrap();
    let root = root.as_path();
    std::fs::create_dir_all(root.join("ledger")).unwrap();
    std::fs::write(root.join("ledger/qtquick.toml"), LEDGER).unwrap();
    std::fs::create_dir_all(root.join("docs/research")).unwrap();
    std::fs::write(root.join("docs/ARCHITECTURE.md"), "# Architecture\n").unwrap();
    std::fs::write(root.join("docs/research/10-mcp.md"), "# MCP\n").unwrap();
    std::fs::create_dir_all(root.join("features")).unwrap();
    std::fs::write(
        root.join("features/README.md"),
        "# Features\n\n## Step catalogue\n\n- `Given a window`\n\n## Tags\n\nignored\n",
    )
    .unwrap();
    std::fs::write(root.join("Main.qml"), "Item {}\n").unwrap();
    std::fs::write(tmp.path().join("outside-secret.txt"), "secret").unwrap();
    Fixture {
        root: root.to_path_buf(),
        _tmp: tmp,
    }
}

async fn connect(root: &Path, caps: CapSet) -> RunningService<RoleClient, ()> {
    let server = AuthorServer::new(Project::open(root).unwrap(), Arc::new(StubFrontEnd));
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        let _ = RmcpServer::new(Arc::new(server), caps)
            .serve(server_io)
            .await;
    });
    ().serve(client_io).await.expect("handshake")
}

async fn call(c: &RunningService<RoleClient, ()>, name: &str, args: Value) -> (bool, Value) {
    let args = args.as_object().cloned().unwrap_or_default();
    let res = c
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args))
        .await
        .unwrap();
    (
        res.is_error == Some(true),
        res.structured_content.expect("structured content"),
    )
}

#[tokio::test]
async fn tools_list_is_deterministic_and_in_fixed_order() {
    let dir = fixture();
    let mut seen = Vec::new();
    for _ in 0..3 {
        let c = connect(dir.path(), CapSet::EMPTY).await;
        let names: Vec<String> = c
            .list_all_tools()
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        seen.push(names);
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(
        seen[0],
        [
            "qml_check",
            "qml_lint",
            "type_lookup",
            "api_status",
            "project_info"
        ]
    );
}

#[tokio::test]
async fn project_info_reports_ledger_and_capabilities() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::of([Cap::Inspect])).await;
    let (err, body) = call(&c, "project_info", json!({})).await;
    assert!(!err);
    assert_eq!(body["ledger"]["rows"], 5);
    assert_eq!(body["ledger"]["by_status"]["unsupported"], 5);
    assert_eq!(body["ledger"]["modules"], json!(["QtQuick"]));
    assert_eq!(body["front_end_wired"], false);
    assert_eq!(body["capabilities"], json!(["inspect"]));
}

#[tokio::test]
async fn api_status_answers_for_types_and_reports_unknowns() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (err, body) = call(&c, "api_status", json!({ "query": "QtQuick.Rectangle" })).await;
    assert!(!err);
    assert_eq!(body["rows"][0]["status"], "unsupported");
    assert_eq!(body["rows"][0]["reason"], "M0 in progress");

    let (err, body) = call(&c, "api_status", json!({ "query": "QtQuick.Nope" })).await;
    assert!(err);
    assert_eq!(body["error"]["code"], "not_found");

    let (err, body) = call(&c, "api_status", json!({ "bogus": 1 })).await;
    assert!(err);
    assert_eq!(body["error"]["code"], "invalid_argument");
}

#[tokio::test]
async fn front_end_tools_say_unavailable_and_paths_are_confined() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (err, body) = call(&c, "qml_check", json!({ "paths": ["Main.qml"] })).await;
    assert!(err);
    assert_eq!(body["error"]["code"], "unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not wired yet")
    );

    let (_, body) = call(
        &c,
        "qml_check",
        json!({ "paths": ["../outside-secret.txt"] }),
    )
    .await;
    assert!(matches!(
        body["error"]["code"].as_str(),
        Some("path_denied" | "not_found")
    ));
    let outside = dir.path().parent().unwrap().join("outside-secret.txt");
    let (_, body) = call(&c, "qml_lint", json!({ "paths": [outside] })).await;
    assert_eq!(body["error"]["code"], "path_denied");

    let (err, body) = call(
        &c,
        "type_lookup",
        json!({ "module": "QtQuick", "type": "Item" }),
    )
    .await;
    assert!(err);
    assert_eq!(body["error"]["code"], "unavailable");
}

async fn read(c: &RunningService<RoleClient, ()>, uri: &str) -> Result<String, rmcp::ServiceError> {
    let res = c
        .read_resource(ReadResourceRequestParams::new(uri.to_owned()))
        .await?;
    match &res.contents[0] {
        rmcp::model::ResourceContents::TextResourceContents { text, .. } => Ok(text.clone()),
        other => panic!("expected text, got {other:?}"),
    }
}

#[tokio::test]
async fn resources_serve_ledger_docs_and_steps_confined_to_docs() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;

    assert!(read(&c, "ljos://ledger").await.unwrap().contains("[[row]]"));
    assert_eq!(
        read(&c, "ljos://docs/ARCHITECTURE.md").await.unwrap(),
        "# Architecture\n"
    );
    assert_eq!(
        read(&c, "ljos://docs/research/10-mcp.md").await.unwrap(),
        "# MCP\n"
    );
    let steps = read(&c, "ljos://features/steps").await.unwrap();
    assert!(steps.contains("Given a window") && !steps.contains("ignored"));

    // Escapes: parent traversal and a project file outside docs/.
    assert!(read(&c, "ljos://docs/../Main.qml").await.is_err());
    assert!(
        read(&c, "ljos://docs/../../outside-secret.txt")
            .await
            .is_err()
    );
    // The type template is stubbed like the tools.
    assert!(read(&c, "ljos://types/QtQuick/Item").await.is_err());

    let uris: Vec<String> = c
        .list_all_resources()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.uri.to_string())
        .collect();
    assert!(uris.contains(&"ljos://ledger".to_owned()));
    assert!(uris.contains(&"ljos://docs/ARCHITECTURE.md".to_owned()));
    let templates = c.list_all_resource_templates().await.unwrap();
    assert_eq!(templates.len(), 2);
}

#[tokio::test]
async fn prompts_are_listed_and_rendered() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let names: Vec<String> = c
        .list_all_prompts()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["write-component", "port-from-qtquick"]);
    let got = c
        .get_prompt(
            rmcp::model::GetPromptRequestParams::new("write-component")
                .with_arguments(json!({ "name": "NoteCard" }).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let text = format!("{:?}", got.messages);
    assert!(text.contains("NoteCard") && text.contains("api_status"));
}
