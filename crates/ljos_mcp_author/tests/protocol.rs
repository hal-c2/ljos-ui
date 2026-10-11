//! In-process protocol tests: a real rmcp client against the authoring surface
//! over a duplex pipe, with a throwaway project directory.
//!
//! Every tool is called here the way an agent calls it. Three things are
//! held for each: what it answers, that a long answer comes in pages no
//! larger than one copy of a response may be (§19.6) and that the pages add
//! up to the whole, and that what needs a capability is refused without it.

use std::{collections::BTreeSet, fmt::Write as _, path::Path, sync::Arc};

use ljos_mcp_author::{AuthorServer, FrontEnd, Project, QmlFrontEnd, StubFrontEnd};
use ljos_mcp_core::{Cap, CapSet, DEFAULT_RESULT_BYTES, HARD_RESULT_BYTES, glue::RmcpServer};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ReadResourceRequestParams},
    service::RunningService,
};
use serde_json::{Value, json};

const LEDGER: &str = include_str!("../../../ledger/qtquick.toml");

/// What one copy of an answer may be: the model's text and the structured
/// value each get half of a tool's budget.
const ONE_COPY: usize = DEFAULT_RESULT_BYTES / 2;

type Client = RunningService<RoleClient, ()>;

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

    fn write(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        self
    }

    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.root.join(name)).unwrap()
    }

    /// A program a run can start, written into the project.
    #[cfg(unix)]
    fn script(&self, name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        self.write(name, &format!("#!/bin/sh\n{body}"));
        let path = self.root.join(name);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("proj");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(tmp.path().join("outside-secret.txt"), "secret").unwrap();
    let fixture = Fixture {
        // The server names files by the canonical root; so do the tests.
        root: root.canonicalize().unwrap(),
        _tmp: tmp,
    };
    fixture
        .write("ledger/qtquick.toml", LEDGER)
        .write("docs/ARCHITECTURE.md", "# Architecture\n")
        .write("docs/research/10-mcp.md", "# MCP\n")
        .write(
            "features/README.md",
            "# Features\n\n## Step catalogue\n\n- `Given a window`\n\n## Tags\n\nignored\n",
        )
        .write("Main.qml", "Item {}\n");
    fixture
}

/// A server that analyses QML, as `ljos mcp` serves it.
fn server(root: &Path) -> AuthorServer {
    let front: Arc<dyn FrontEnd> = Arc::new(QmlFrontEnd::new(root));
    AuthorServer::new(Project::open(root).unwrap(), front)
}

async fn serve(server: AuthorServer, caps: CapSet) -> Client {
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        let _ = RmcpServer::new(Arc::new(server), caps)
            .serve(server_io)
            .await;
    });
    ().serve(client_io).await.expect("handshake")
}

async fn connect(root: &Path, caps: CapSet) -> Client {
    serve(server(root), caps).await
}

/// A server built without the QML front end.
async fn connect_stub(root: &Path, caps: CapSet) -> Client {
    let server = AuthorServer::new(Project::open(root).unwrap(), Arc::new(StubFrontEnd));
    serve(server, caps).await
}

/// The answer as the client gets it: whether it is an error, the structured
/// value, and the text the model reads.
async fn call_whole(c: &Client, name: &str, args: Value) -> (bool, Value, String) {
    let args = args.as_object().cloned().unwrap_or_default();
    let res = c
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args))
        .await
        .unwrap();
    let text: String = res
        .content
        .iter()
        .filter_map(|part| part.as_text().map(|t| t.text.clone()))
        .collect();
    (
        res.is_error == Some(true),
        res.structured_content.expect("structured content"),
        text,
    )
}

async fn call(c: &Client, name: &str, args: Value) -> (bool, Value) {
    let (is_error, body, _) = call_whole(c, name, args).await;
    (is_error, body)
}

/// An answer that is not an error.
async fn ok(c: &Client, name: &str, args: Value) -> Value {
    let (is_error, body) = call(c, name, args).await;
    assert!(!is_error, "`{name}` was refused: {body}");
    body
}

/// The code and the message of a refusal.
async fn refused(c: &Client, name: &str, args: Value) -> (String, String) {
    let (is_error, body) = call(c, name, args).await;
    assert!(is_error, "`{name}` answered: {body}");
    (
        body["error"]["code"].as_str().unwrap().to_owned(),
        body["error"]["message"].as_str().unwrap().to_owned(),
    )
}

/// Every page of a paged answer: calls `name` with the cursor each answer
/// gives until one gives none, and returns the items of `list` in order with
/// the number of calls it took.
///
/// Each page is held to the size one copy of an answer may be, in both of
/// its copies, and must not have been cut by the response cap: a tool that
/// pages cuts at an item and says where to go on, where the cap cuts where
/// it must and cannot move the tool's cursor.
async fn pages(c: &Client, name: &str, args: &Value, list: &str) -> (Vec<Value>, usize) {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    let mut calls = 0;
    loop {
        let mut args = args.clone();
        if let Some(cursor) = &cursor {
            args["cursor"] = json!(cursor);
        }
        let (is_error, body, text) = call_whole(c, name, args).await;
        assert!(!is_error, "`{name}` was refused: {body}");
        calls += 1;
        let size = serde_json::to_vec(&body).unwrap().len();
        assert!(size <= ONE_COPY, "a page of `{name}` is {size} bytes");
        assert!(
            text.len() <= ONE_COPY + 1024,
            "the text of `{name}` is {} bytes",
            text.len()
        );
        assert!(
            body.get("truncated").is_none(),
            "`{name}` was cut by the cap: {body}"
        );
        let page = body[list]
            .as_array()
            .unwrap_or_else(|| panic!("no `{list}` in {body}"));
        assert!(!page.is_empty(), "an empty page of `{name}`");
        items.extend(page.iter().cloned());
        match body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return (items, calls),
        }
        assert!(calls < 500, "`{name}` never ends");
    }
}

/// The values of `key` over `items`, which must all differ.
fn distinct(items: &[Value], key: impl Fn(&Value) -> String) -> BTreeSet<String> {
    let all: BTreeSet<String> = items.iter().map(&key).collect();
    assert_eq!(all.len(), items.len(), "an item came twice");
    all
}

const TOOLS: [&str; 12] = [
    "qml_check",
    "qml_lint",
    "type_lookup",
    "api_status",
    "project_info",
    "qml_format",
    "binding_tier_explain",
    "docs_search",
    "features_list",
    "features_run",
    "run_status",
    "run_cancel",
];

async fn tool_names(c: &Client) -> Vec<String> {
    c.list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect()
}

#[tokio::test]
async fn tools_list_is_deterministic_and_in_fixed_order() {
    let dir = fixture();
    let mut seen = Vec::new();
    for _ in 0..3 {
        let c = connect(dir.path(), CapSet::EMPTY).await;
        seen.push(tool_names(&c).await);
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]));
    // Without the right to run things the tool that starts a run is not
    // offered; the two that ask about a run are harmless and stay.
    let without_run: Vec<&str> = TOOLS.into_iter().filter(|t| *t != "features_run").collect();
    assert_eq!(seen[0], without_run);

    let c = connect(dir.path(), CapSet::of([Cap::Run])).await;
    assert_eq!(tool_names(&c).await, TOOLS);
}

#[tokio::test]
async fn every_tool_says_what_it_takes_and_what_it_answers() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::of(Cap::ALL)).await;
    let tools = c.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), TOOLS.len());
    for tool in tools {
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.len() > 40, "{}: {description}", tool.name);
        assert!(
            tool.output_schema.is_some(),
            "{} has no output schema",
            tool.name
        );
        // Every argument is described: a schema is all an agent has.
        let properties = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object);
        for (name, schema) in properties.into_iter().flatten() {
            // A cursor is described once, by the answer that gives it.
            assert!(
                name == "cursor" || schema.get("description").is_some(),
                "{}: the argument `{name}` is not described",
                tool.name
            );
        }
    }
}

#[tokio::test]
async fn project_info_reports_ledger_and_capabilities() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::of([Cap::Inspect])).await;
    let body = ok(&c, "project_info", json!({})).await;
    assert_eq!(body["ledger"]["rows"], 5);
    assert_eq!(body["ledger"]["by_status"]["unsupported"], 5);
    assert_eq!(body["ledger"]["modules"], json!(["QtQuick"]));
    assert_eq!(body["front_end_wired"], true);
    assert_eq!(body["capabilities"], json!(["inspect"]));
    assert_eq!(body["project_root"], dir.path().display().to_string());

    let c = connect_stub(dir.path(), CapSet::EMPTY).await;
    let body = ok(&c, "project_info", json!({})).await;
    assert_eq!(body["front_end_wired"], false);
    assert_eq!(body["capabilities"], json!([]));
}

#[tokio::test]
async fn api_status_answers_for_types_and_reports_unknowns() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let body = ok(&c, "api_status", json!({ "query": "QtQuick.Rectangle" })).await;
    assert_eq!(body["rows"][0]["status"], "unsupported");
    assert_eq!(body["rows"][0]["reason"], "M0 in progress");

    let (code, message) = refused(&c, "api_status", json!({ "query": "QtQuick.Nope" })).await;
    assert_eq!(code, "not_found");
    assert!(message.contains("Module.Type"), "{message}");

    let (code, _) = refused(&c, "api_status", json!({ "bogus": 1 })).await;
    assert_eq!(code, "invalid_argument");
}

#[tokio::test]
async fn api_status_pages_through_a_type_with_many_members() {
    let dir = fixture();
    let mut ledger = String::from(
        "[[row]]\nmodule = \"Big\"\ntype = \"Thing\"\nkind = \"type\"\nstatus = \"unsupported\"\nreason = \"none\"\n",
    );
    for n in 0..260 {
        write!(
            ledger,
            "\n[[row]]\nmodule = \"Big\"\ntype = \"Thing\"\nmember = \"p{n}\"\nkind = \"property\"\n\
             status = \"dropped\"\nreason = \"{}\"\n",
            "use something else. ".repeat(8)
        )
        .unwrap();
    }
    dir.write("ledger/qtquick.toml", &ledger);
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (rows, calls) = pages(&c, "api_status", &json!({ "query": "Big.Thing" }), "rows").await;
    assert_eq!(rows.len(), 261);
    assert!(calls > 2, "{calls} calls");
    distinct(&rows, |row| row["member"].to_string());
    assert_eq!(rows[1]["member"], "p0");
    assert_eq!(rows[260]["member"], "p259");

    let (code, message) = refused(
        &c,
        "api_status",
        json!({ "query": "Big.Thing", "cursor": "x" }),
    )
    .await;
    assert_eq!(code, "invalid_argument");
    assert!(message.contains("next_cursor"), "{message}");
}

#[tokio::test]
async fn front_end_tools_say_unavailable_without_a_front_end() {
    let dir = fixture();
    let c = connect_stub(dir.path(), CapSet::EMPTY).await;
    for (tool, args) in [
        ("qml_check", json!({ "paths": ["Main.qml"] })),
        ("qml_lint", json!({ "paths": ["Main.qml"] })),
        (
            "type_lookup",
            json!({ "module": "QtQuick", "type": "Item" }),
        ),
    ] {
        let (code, message) = refused(&c, tool, args).await;
        assert_eq!(code, "unavailable", "{tool}");
        assert!(message.contains("not wired yet"), "{message}");
    }
}

#[tokio::test]
async fn paths_are_confined_to_the_project() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::of(Cap::ALL)).await;
    let outside = dir.path().parent().unwrap().join("outside-secret.txt");
    let outside = outside.to_str().unwrap();
    for (tool, args) in [
        ("qml_check", json!({ "paths": ["../outside-secret.txt"] })),
        ("qml_check", json!({ "paths": [outside] })),
        ("qml_lint", json!({ "paths": [outside] })),
        ("qml_format", json!({ "paths": [outside] })),
        (
            "qml_format",
            json!({ "paths": ["../outside-secret.txt"], "write": true }),
        ),
        ("binding_tier_explain", json!({ "file": outside })),
        (
            "binding_tier_explain",
            json!({ "file": "../outside-secret.txt" }),
        ),
        (
            "features_list",
            json!({ "paths": ["../../outside-secret.txt"] }),
        ),
        ("features_list", json!({ "paths": [outside] })),
        (
            "features_run",
            json!({ "paths": ["../../outside-secret.txt"] }),
        ),
    ] {
        let (code, message) = refused(&c, tool, args.clone()).await;
        assert!(
            matches!(code.as_str(), "path_denied" | "not_found"),
            "{tool} {args}: {code}: {message}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().parent().unwrap().join("outside-secret.txt")).unwrap(),
        "secret"
    );
}

#[tokio::test]
async fn qml_check_reports_what_is_wrong_and_where() {
    let dir = fixture();
    dir.write("Main.qml", "import QtQuick\n\nItem {\n    Nonesuch {}\n}\n")
        .write("Broken.qml", "import QtQuick\n\nItem {\n")
        .write("Fine.qml", "import QtQuick\n\nItem {\n    width: 10\n}\n");
    let c = connect(dir.path(), CapSet::EMPTY).await;

    let body = ok(&c, "qml_check", json!({ "paths": ["Fine.qml"] })).await;
    assert_eq!(body["diagnostics"], json!([]));
    assert_eq!(body["next_cursor"], Value::Null);

    let body = ok(&c, "qml_check", json!({ "paths": ["Main.qml"] })).await;
    let found = &body["diagnostics"][0];
    assert_eq!(found["file"], "Main.qml");
    assert_eq!(found["line"], 4);
    assert_eq!(found["severity"], "error");
    assert!(
        found["message"].as_str().unwrap().contains("Nonesuch"),
        "{found}"
    );
    assert!(found["code"].is_string());

    // A directory is every `.qml` file below it, in path order.
    let body = ok(&c, "qml_check", json!({ "paths": ["."] })).await;
    let files: Vec<&str> = body["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, ["Broken.qml", "Main.qml"]);

    let (code, _) = refused(&c, "qml_check", json!({ "paths": ["Missing.qml"] })).await;
    assert_eq!(code, "not_found");
    let (code, message) = refused(
        &c,
        "qml_check",
        json!({ "paths": ["docs/ARCHITECTURE.md"] }),
    )
    .await;
    assert_eq!(code, "invalid_argument");
    assert!(message.contains(".qml"), "{message}");
    let (code, _) = refused(&c, "qml_check", json!({})).await;
    assert_eq!(code, "invalid_argument");
}

#[tokio::test]
async fn qml_check_pages_through_many_diagnostics() {
    let dir = fixture();
    for n in 0..230 {
        dir.write(
            &format!("views/V{n:03}.qml"),
            "import QtQuick\n\nItem {\n    NoSuchTypeAnywhere {}\n}\n",
        );
    }
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (found, calls) = pages(
        &c,
        "qml_check",
        &json!({ "paths": ["views"] }),
        "diagnostics",
    )
    .await;
    assert_eq!(found.len(), 230);
    assert!(calls >= 3, "{calls} calls");
    let files = distinct(&found, |d| d["file"].as_str().unwrap().to_owned());
    assert_eq!(files.first().unwrap(), "views/V000.qml");
    assert_eq!(files.last().unwrap(), "views/V229.qml");
    // In path order, so a cursor means the same file on the next call.
    let order: Vec<&str> = found.iter().map(|d| d["file"].as_str().unwrap()).collect();
    assert!(order.is_sorted());
}

const UNNAMED: &str =
    "import QtQuick\n\nItem {\n    MouseArea {\n        onClicked: console.log(\"x\")\n    }\n}\n";

#[tokio::test]
async fn qml_lint_reports_the_accessibility_lints_by_their_codes() {
    let dir = fixture();
    dir.write("Main.qml", UNNAMED).write(
        "Named.qml",
        "import QtQuick\n\nItem {\n    MouseArea {\n        Accessible.name: qsTr(\"Close\")\n        \
         Accessible.role: Accessible.Button\n        onClicked: console.log(\"x\")\n    }\n}\n",
    );
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let body = ok(&c, "qml_lint", json!({ "paths": ["Main.qml"] })).await;
    let found = &body["diagnostics"][0];
    assert_eq!(found["code"], "a11y-unnamed-interactive");
    assert_eq!(found["severity"], "warning");
    assert_eq!(
        (found["file"].as_str(), found["line"].as_u64()),
        (Some("Main.qml"), Some(4))
    );
    assert!(
        found["message"]
            .as_str()
            .unwrap()
            .contains("Accessible.name")
    );

    let body = ok(&c, "qml_lint", json!({ "paths": ["Named.qml"] })).await;
    let codes: Vec<&str> = body["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .filter(|code| code.starts_with("a11y-"))
        .collect();
    assert!(codes.is_empty(), "{codes:?}");

    // The codes are the ones `ljos check` prints, from the one table both
    // read; the resource says what each wants.
    let rules = read(&c, "ljos://lints").await.unwrap();
    for lint in ljos_mcp_author::LINTS {
        assert!(
            rules.contains(lint.code),
            "{} is not in ljos://lints",
            lint.code
        );
    }
    assert!(
        ljos_mcp_author::LINTS
            .iter()
            .any(|l| l.code == "a11y-unnamed-interactive")
    );
}

#[tokio::test]
async fn qml_lint_pages_through_many_findings() {
    let dir = fixture();
    for n in 0..130 {
        dir.write(&format!("views/V{n:03}.qml"), UNNAMED);
    }
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (found, calls) = pages(
        &c,
        "qml_lint",
        &json!({ "paths": ["views"] }),
        "diagnostics",
    )
    .await;
    assert!(calls >= 2, "{calls} calls");
    let unnamed: Vec<Value> = found
        .into_iter()
        .filter(|d| d["code"] == "a11y-unnamed-interactive")
        .collect();
    assert_eq!(unnamed.len(), 130);
    distinct(&unnamed, |d| d["file"].as_str().unwrap().to_owned());
}

#[tokio::test]
async fn type_lookup_describes_a_type_and_says_when_there_is_none() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (_, body, text) = call_whole(
        &c,
        "type_lookup",
        json!({ "module": "QtQuick", "type": "Rectangle" }),
    )
    .await;
    assert_eq!(
        (body["module"].as_str(), body["name"].as_str()),
        (Some("QtQuick"), Some("Rectangle"))
    );
    // Its own members, each as it would be declared; what it inherits is
    // in the types of the chain, which are asked for by name.
    let properties: Vec<&str> = body["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert!(properties.contains(&"color: color"), "{body}");
    assert!(properties.contains(&"radius: real"), "{body}");
    assert!(
        !properties.iter().any(|p| p.starts_with("width:")),
        "{body}"
    );
    assert_eq!(body["base_chain"], json!(["Item", "QtObject"]));
    assert_eq!(body["default_property"], "data");
    let base = ok(
        &c,
        "type_lookup",
        json!({ "module": "QtQuick", "type": "Item" }),
    )
    .await;
    assert!(
        base["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().starts_with("width:"))
    );
    assert!(serde_json::to_vec(&body).unwrap().len() <= ONE_COPY);
    assert!(text.len() <= ONE_COPY + 1024);

    let (code, _) = refused(
        &c,
        "type_lookup",
        json!({ "module": "QtQuick", "type": "Nonesuch" }),
    )
    .await;
    assert_eq!(code, "not_found");
    let (code, _) = refused(&c, "type_lookup", json!({ "module": "QtQuick" })).await;
    assert_eq!(code, "invalid_argument");
}

const UNTIDY: &str = "import QtQuick\nItem {\n  width:100;height:200\n      Text{text:\"hi\"}\n}\n";
const TIDY: &str = "import QtQuick\n\nItem {\n    width: 100\n    height: 200\n\n    Text {\n        text: \"hi\"\n    }\n}\n";

#[tokio::test]
async fn qml_format_shows_a_diff_and_writes_only_with_the_right() {
    let dir = fixture();
    dir.write("Main.qml", UNTIDY).write("Tidy.qml", TIDY);
    let c = connect(dir.path(), CapSet::EMPTY).await;

    // Text in, text out: nothing on disk is read or changed.
    let body = ok(&c, "qml_format", json!({ "source": UNTIDY })).await;
    let formatted = body["formatted"].as_str().unwrap().to_owned();
    assert_eq!(body["files"][0]["file"], "<source>");
    assert_eq!(body["files"][0]["status"], "changed");
    assert!(
        formatted.contains("\n    width: 100\n    height: 200\n"),
        "{formatted}"
    );
    // What it gives it leaves alone.
    let again = ok(&c, "qml_format", json!({ "source": formatted })).await;
    assert_eq!(again["files"][0]["status"], "unchanged");
    assert_eq!(again["formatted"], json!(formatted));

    // Files: a diff for the one that changes, and nothing written.
    let body = ok(&c, "qml_format", json!({ "paths": ["."] })).await;
    assert_eq!(
        (body["changed"].as_u64(), body["unchanged"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(body["written"], 0);
    assert_eq!(body["files"][0]["file"], "Main.qml");
    let diff = body["files"][0]["diff"].as_str().unwrap();
    assert!(
        diff.contains("--- ") && diff.contains("+++ ") && diff.contains("@@"),
        "{diff}"
    );
    assert!(diff.contains("\n+    width: 100\n"), "{diff}");
    assert_eq!(body["files"][1]["status"], "unchanged");
    assert_eq!(body["files"][1]["diff"], Value::Null);
    assert_eq!(dir.text("Main.qml"), UNTIDY);

    // Asking to write without the right is refused, and writes nothing.
    let (is_error, body) = call(
        &c,
        "qml_format",
        json!({ "paths": ["Main.qml"], "write": true }),
    )
    .await;
    assert!(is_error);
    assert_eq!(body["error"]["code"], "capability_denied");
    assert_eq!(body["error"]["details"]["required"], "fs");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--caps fs")
    );
    assert_eq!(dir.text("Main.qml"), UNTIDY);

    // With the right, the file is what `source` was laid out to.
    let c = connect(dir.path(), CapSet::of([Cap::Fs])).await;
    let body = ok(
        &c,
        "qml_format",
        json!({ "paths": ["Main.qml"], "write": true }),
    )
    .await;
    assert_eq!(body["files"][0]["status"], "written");
    assert_eq!(body["written"], 1);
    assert_eq!(dir.text("Main.qml"), formatted);
    let body = ok(
        &c,
        "qml_format",
        json!({ "paths": ["Main.qml"], "write": true }),
    )
    .await;
    assert_eq!(body["files"][0]["status"], "unchanged");

    for args in [
        json!({}),
        json!({ "paths": ["Main.qml"], "source": "Item {}" }),
        json!({ "source": "Item {}", "write": true }),
    ] {
        let (code, _) = refused(&c, "qml_format", args).await;
        assert_eq!(code, "invalid_argument");
    }
}

#[tokio::test]
async fn qml_format_refuses_a_file_that_does_not_parse() {
    let dir = fixture();
    let broken = "import QtQuick\n\nItem {\n  width: \n";
    dir.write("Broken.qml", broken).write("Main.qml", UNTIDY);
    let c = connect(dir.path(), CapSet::of([Cap::Fs])).await;
    let body = ok(&c, "qml_format", json!({ "paths": ["."], "write": true })).await;
    assert_eq!(body["refused"], 1);
    assert_eq!(body["written"], 1);
    let refusal = &body["files"][0];
    assert_eq!(
        (refusal["file"].as_str(), refusal["status"].as_str()),
        (Some("Broken.qml"), Some("refused"))
    );
    assert!(
        refusal["diagnostics"][0]["line"].as_u64().unwrap() >= 3,
        "{refusal}"
    );
    assert!(
        !refusal["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(dir.text("Broken.qml"), broken);

    let body = ok(&c, "qml_format", json!({ "source": broken })).await;
    assert_eq!(body["files"][0]["status"], "refused");
    assert_eq!(body["formatted"], Value::Null);
}

#[tokio::test]
async fn qml_format_pages_through_many_files_and_cuts_a_long_diff() {
    let dir = fixture();
    std::fs::remove_file(dir.path().join("Main.qml")).unwrap();
    for n in 0..120 {
        // Enough lines that the diffs of a page run into its budget.
        let mut text = String::from("import QtQuick\nItem {\n");
        for p in 0..12 {
            writeln!(text, "  property int value{p}:{p}").unwrap();
        }
        text.push_str("}\n");
        dir.write(&format!("views/V{n:03}.qml"), &text);
    }
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (files, calls) = pages(&c, "qml_format", &json!({ "paths": ["views"] }), "files").await;
    assert_eq!(files.len(), 120);
    assert!(calls >= 3, "{calls} calls");
    distinct(&files, |f| f["file"].as_str().unwrap().to_owned());
    assert!(
        files
            .iter()
            .all(|f| f["status"] == "changed" && f["diff_truncated"] == false)
    );

    // One file whose diff alone is longer than a diff may be.
    let mut long = String::from("import QtQuick\nItem {\n");
    for p in 0..900 {
        writeln!(long, "  property int value{p}:{p}").unwrap();
    }
    long.push_str("}\n");
    dir.write("Long.qml", &long);
    let (_, body, text) = call_whole(&c, "qml_format", json!({ "paths": ["Long.qml"] })).await;
    let file = &body["files"][0];
    assert_eq!(file["diff_truncated"], true);
    let diff = file["diff"].as_str().unwrap();
    assert!(diff.len() <= 7 * 1024, "{} bytes", diff.len());
    assert!(diff.ends_with('\n'), "cut inside a line");
    assert!(serde_json::to_vec(&body).unwrap().len() <= ONE_COPY);
    assert!(text.len() <= ONE_COPY + 1024);

    // A text of any length comes back whole or not at all: half a file
    // that looks like a whole one is worse than none.
    let (_, body, _) = call_whole(&c, "qml_format", json!({ "source": long })).await;
    let size = serde_json::to_vec(&body).unwrap().len();
    assert!(size <= HARD_RESULT_BYTES, "{size} bytes");
}

const BINDINGS: &str = r#"import QtQuick

Item {
    id: root
    width: 200
    height: parent.height
    property int n: 2
    property string label: n > 1 ? "many" : "one"
    property string word: {
        switch (n) {
        case 1: return "one";
        default: return "many";
        }
    }
    Text { id: caption; width: root.width; text: root.label }
    Rectangle { width: root.width / 2 }
    MouseArea { onClicked: console.log("x") }
}
"#;

#[tokio::test]
async fn binding_tier_explain_says_the_tier_the_reason_and_a_rewrite() {
    let dir = fixture();
    dir.write("Main.qml", BINDINGS);
    let c = connect(dir.path(), CapSet::EMPTY).await;

    let body = ok(&c, "binding_tier_explain", json!({ "file": "Main.qml" })).await;
    assert_eq!(
        body["summary"],
        json!({ "expressions": 7, "tier0": 3, "tier1": 2, "js": 2, "assigned": 2 })
    );
    let tiers: Vec<(String, Value)> = body["expressions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["property"].as_str().unwrap().to_owned(),
                e["tier"].clone(),
            )
        })
        .collect();
    let tier = |name: &str| {
        tiers
            .iter()
            .find(|(p, _)| p == name)
            .map(|(_, t)| t.clone())
            .unwrap()
    };
    assert_eq!(tier("height"), "0");
    assert_eq!(tier("label"), "1");
    assert_eq!(tier("word"), "js");
    assert_eq!(tier("onClicked"), "js");
    assert_eq!(tier("n"), Value::Null);

    // By property path: the reason is the compiler's, and the rewrite is
    // one the compiler was asked about before it was offered.
    let body = ok(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "property": "word" }),
    )
    .await;
    assert_eq!(body["expressions"].as_array().unwrap().len(), 1);
    let word = &body["expressions"][0];
    assert_eq!(
        (word["line"].as_u64(), word["column"].as_u64()),
        (Some(9), Some(27))
    );
    assert_eq!(
        word["reason"],
        json!({ "kind": "statement", "detail": "switch" })
    );
    assert_eq!(word["rewrite"]["source"], "n === 1 ? \"one\" : \"many\"");
    assert_eq!(word["rewrite"]["tier"], "1");
    assert!(word["why"].as_str().unwrap().contains("switch"));
    // The counts are of the whole file however the answer was narrowed.
    assert_eq!(body["summary"]["expressions"], 7);

    // The rewrite is true: with it in place the compiler says tier 1.
    let rewritten = BINDINGS.replace(
        "{\n        switch (n) {\n        case 1: return \"one\";\n        default: return \"many\";\n        }\n    }",
        word["rewrite"]["source"].as_str().unwrap(),
    );
    assert_ne!(rewritten, BINDINGS);
    dir.write("Rewritten.qml", &rewritten);
    let body = ok(
        &c,
        "binding_tier_explain",
        json!({ "file": "Rewritten.qml", "property": "word" }),
    )
    .await;
    assert_eq!(body["expressions"][0]["tier"], "1");
    assert_eq!(body["expressions"][0]["rewrite"], Value::Null);

    // By line, and by line and column.
    let body = ok(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "line": 15 }),
    )
    .await;
    let on_line: Vec<&str> = body["expressions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["property"].as_str().unwrap())
        .collect();
    assert_eq!(on_line, ["width", "text"]);
    let body = ok(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "line": 15, "column": 50 }),
    )
    .await;
    assert_eq!(body["expressions"].as_array().unwrap().len(), 1);
    assert_eq!(body["expressions"][0]["property"], "text");

    // A handler that stays in JavaScript says what to do about it.
    let body = ok(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "property": "onClicked" }),
    )
    .await;
    assert_eq!(body["expressions"][0]["kind"], "handler");
    assert_eq!(body["expressions"][0]["reason"]["detail"], "console.log");
    assert!(
        body["expressions"][0]["advice"]
            .as_str()
            .unwrap()
            .contains("console.log")
    );
}

#[tokio::test]
async fn binding_tier_explain_refusals_say_what_to_ask_instead() {
    let dir = fixture();
    dir.write("Main.qml", BINDINGS)
        .write("Broken.qml", "import QtQuick\n\nItem {\n");
    let c = connect(dir.path(), CapSet::EMPTY).await;

    // Three objects have a `width`: the refusal names them, and each name
    // it gives is one that works.
    let (is_error, body) = call(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "property": "width" }),
    )
    .await;
    assert!(is_error);
    assert_eq!(body["error"]["code"], "ambiguous");
    let candidates = body["error"]["details"]["candidates"]
        .as_array()
        .unwrap()
        .clone();
    let names: Vec<&str> = candidates
        .iter()
        .map(|c| c["property"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["root.width", "caption.width", "Rectangle.width"]);
    for candidate in &candidates {
        let body = ok(
            &c,
            "binding_tier_explain",
            json!({ "file": "Main.qml", "property": candidate["property"] }),
        )
        .await;
        assert_eq!(body["expressions"].as_array().unwrap().len(), 1);
        assert_eq!(body["expressions"][0]["line"], candidate["line"]);
    }

    let (code, message) = refused(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "property": "colour" }),
    )
    .await;
    assert_eq!(code, "not_found");
    assert!(message.contains("colour"), "{message}");
    let (code, _) = refused(
        &c,
        "binding_tier_explain",
        json!({ "file": "Main.qml", "line": 400 }),
    )
    .await;
    assert_eq!(code, "not_found");
    let (code, _) = refused(&c, "binding_tier_explain", json!({ "file": "Missing.qml" })).await;
    assert_eq!(code, "not_found");
    let (code, _) = refused(&c, "binding_tier_explain", json!({})).await;
    assert_eq!(code, "invalid_argument");
    // A file that does not parse has no tiers: the refusal sends the agent
    // to the tool that says why.
    let (code, message) =
        refused(&c, "binding_tier_explain", json!({ "file": "Broken.qml" })).await;
    assert_eq!(code, "invalid_argument");
    assert!(message.contains("qml_check"), "{message}");
}

#[tokio::test]
async fn binding_tier_explain_pages_through_a_long_file() {
    let dir = fixture();
    let mut text = String::from("import QtQuick\n\nItem {\n    id: root\n    property int n: 2\n");
    for p in 0..400 {
        // Long expressions, cut where a value's text is cut.
        writeln!(
            text,
            "    property int value{p}: root.n + {p} + {}",
            "root.n + ".repeat(90) + "1"
        )
        .unwrap();
    }
    text.push_str("}\n");
    dir.write("Long.qml", &text);
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (found, calls) = pages(
        &c,
        "binding_tier_explain",
        &json!({ "file": "Long.qml" }),
        "expressions",
    )
    .await;
    assert_eq!(found.len(), 401);
    assert!(calls >= 7, "{calls} calls");
    distinct(&found, |e| e["property"].as_str().unwrap().to_owned());
    let longest = found
        .iter()
        .map(|e| e["source"].as_str().unwrap().len())
        .max()
        .unwrap();
    assert!(longest <= 600 + '…'.len_utf8(), "{longest} bytes");
    assert!(found[1]["source"].as_str().unwrap().ends_with('…'));
}

/// Documents with thirty sections that all say something about reloading.
fn many_sections(dir: &Fixture) {
    let mut doc = String::from("# Guide\n\nHow things are done.\n");
    for n in 0..30 {
        write!(
            doc,
            "\n## Topic {n}\n\nReloading a component keeps its state when topic {n} allows it. {}\n",
            "More is said here about how it is done. ".repeat(12)
        )
        .unwrap();
    }
    dir.write("docs/GUIDE.md", &doc);
}

#[tokio::test]
async fn docs_search_answers_sections_with_snippets_and_links() {
    let dir = fixture();
    dir.write(
        "docs/ARCHITECTURE.md",
        "# Architecture\n\nAn overview.\n\n## Binding tiers\n\nA binding runs in one of three tiers. \
         See https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html and \
         https://code.qt.io/cgit/qt/qtdeclarative.git/tree/src for more.\n\n## Hot reload\n\nA changed \
         file is loaded again and the state is kept.\n",
    )
    .write(
        "features/README.md",
        "# Features\n\n## Tags\n\nA scenario that only Ljós has is tagged `@ljos-only`.\n",
    );
    let c = connect(dir.path(), CapSet::EMPTY).await;

    let body = ok(&c, "docs_search", json!({ "query": "binding tiers" })).await;
    let best = &body["results"][0];
    assert_eq!(best["uri"], "ljos://docs/ARCHITECTURE.md");
    assert_eq!(best["title"], "Architecture > Binding tiers");
    assert_eq!(best["line"], 5);
    assert!(
        best["snippet"].as_str().unwrap().contains("three tiers"),
        "{best}"
    );
    assert!(best["score"].as_f64().unwrap() > 0.0);
    // Qt's documentation is linked; Qt's source never is, not even in the
    // words of the snippet.
    assert_eq!(
        best["qt_docs"],
        json!(["https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html"])
    );
    assert!(!body.to_string().contains("code.qt.io"), "{body}");
    // The link reads the section and no more.
    let section = read(&c, best["section"].as_str().unwrap()).await.unwrap();
    assert!(section.starts_with("## Binding tiers"), "{section}");
    assert!(!section.contains("Hot reload"), "{section}");
    let whole = read(&c, best["uri"].as_str().unwrap()).await.unwrap();
    assert!(whole.contains("Hot reload"));

    // The README of the features and the ledger's reasons are searched too.
    let body = ok(&c, "docs_search", json!({ "query": "ljos-only tag" })).await;
    assert_eq!(body["results"][0]["uri"], "ljos://features/README.md");
    let body = ok(&c, "docs_search", json!({ "query": "Rectangle" })).await;
    let ledger: Vec<&Value> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["uri"].as_str().unwrap().starts_with("ljos://ledger"))
        .collect();
    assert!(!ledger.is_empty(), "{body}");
    assert!(read(&c, ledger[0]["uri"].as_str().unwrap()).await.is_ok());

    // Nothing found is an answer, not an error.
    let body = ok(&c, "docs_search", json!({ "query": "zeppelin" })).await;
    assert_eq!(
        (
            body["total"].as_u64(),
            body["results"].as_array().unwrap().len()
        ),
        (Some(0), 0)
    );
    for args in [json!({}), json!({ "query": "  " })] {
        let (code, _) = refused(&c, "docs_search", args).await;
        assert_eq!(code, "invalid_argument");
    }
}

#[tokio::test]
async fn docs_search_pages_through_many_sections() {
    let dir = fixture();
    many_sections(&dir);
    let c = connect(dir.path(), CapSet::EMPTY).await;

    let body = ok(&c, "docs_search", json!({ "query": "reloading" })).await;
    assert_eq!(body["total"], 30);
    assert_eq!(body["results"].as_array().unwrap().len(), 8);
    assert_eq!(body["next_cursor"], "8");

    let (found, calls) = pages(
        &c,
        "docs_search",
        &json!({ "query": "reloading", "limit": 7 }),
        "results",
    )
    .await;
    assert_eq!((found.len(), calls), (30, 5));
    distinct(&found, |r| r["section"].as_str().unwrap().to_owned());
    // Best first, across pages as within one.
    let scores: Vec<f64> = found.iter().map(|r| r["score"].as_f64().unwrap()).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
    // A snippet is a few lines, not the section.
    assert!(
        found
            .iter()
            .all(|r| r["snippet"].as_str().unwrap().len() <= 1200)
    );

    // No more than twenty at once, whatever is asked for.
    let (found, calls) = pages(
        &c,
        "docs_search",
        &json!({ "query": "reloading", "limit": 500 }),
        "results",
    )
    .await;
    assert_eq!((found.len(), calls), (30, 2));
}

const BOX_FEATURE: &str = "# Sources:\n#   https://doc.qt.io/qt-6/qml-qtquick-item.html\n@qt-diverges @api-QtQuick.Item.width\n\
    Feature: The size of a box\n\n  Scenario: A box is as wide as it is told\n    Given a box\n    Then the box is 10 wide\n\n  \
    @backlog\n  Scenario: A box can be turned\n    Given a box\n";

#[tokio::test]
async fn features_list_lists_and_narrows() {
    let dir = fixture();
    dir.write("features/items/box.feature", BOX_FEATURE).write(
        "features/tooling/hints.feature",
        "@ljos-only\nFeature: Hints\n\n  Rule: A hint is short\n\n    Scenario: A hint fits a line\n      Given a hint\n",
    );
    let c = connect(dir.path(), CapSet::EMPTY).await;

    let body = ok(&c, "features_list", json!({})).await;
    assert_eq!(
        (body["total"].as_u64(), body["files"].as_u64()),
        (Some(3), Some(2))
    );
    assert_eq!(
        body["scenarios"][0],
        json!({
            "file": "features/items/box.feature",
            "line": 6,
            "feature": "The size of a box",
            "rule": null,
            "name": "A box is as wide as it is told",
            "tags": ["@qt-diverges", "@api-QtQuick.Item.width"],
            "status": "active",
            "examples": 0,
        })
    );
    assert_eq!(body["scenarios"][1]["status"], "backlog");
    assert_eq!(body["scenarios"][2]["rule"], "A hint is short");

    for (args, total) in [
        (json!({ "paths": ["items"] }), 2),
        (json!({ "paths": ["features/tooling/hints.feature"] }), 1),
        (json!({ "tags": "@qt-diverges and not @backlog" }), 1),
        (json!({ "tags": "@backlog or @ljos-only" }), 2),
        (json!({ "name": "turned|hint" }), 2),
        (json!({ "paths": ["items"], "tags": "@ljos-only" }), 0),
    ] {
        let body = ok(&c, "features_list", args.clone()).await;
        assert_eq!(body["total"], total, "{args}");
    }

    let (code, message) = refused(&c, "features_list", json!({ "tags": "@qt-diverges not" })).await;
    assert_eq!(code, "invalid_argument");
    assert!(message.contains("@a and not @b"), "{message}");
    let (code, _) = refused(&c, "features_list", json!({ "name": "(" })).await;
    assert_eq!(code, "invalid_argument");
    let (code, _) = refused(
        &c,
        "features_list",
        json!({ "paths": ["features/nowhere"] }),
    )
    .await;
    assert_eq!(code, "not_found");

    // What is listed can be read.
    let text = read(&c, "ljos://features/items/box.feature").await.unwrap();
    assert_eq!(text, BOX_FEATURE);
}

#[tokio::test]
async fn features_list_pages_through_many_scenarios() {
    let dir = fixture();
    for file in 0..6 {
        let mut text = format!("@ljos-only @api-QtQuick.Item.width\nFeature: Part {file}\n");
        for n in 0..75 {
            write!(
                text,
                "\n  Scenario: Part {file} holds number {n} whatever else is {}\n    Given a box\n",
                "said ".repeat(20)
            )
            .unwrap();
        }
        dir.write(&format!("features/parts/part{file}.feature"), &text);
    }
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let (found, calls) = pages(&c, "features_list", &json!({}), "scenarios").await;
    assert_eq!(found.len(), 450);
    assert!(calls >= 5, "{calls} calls");
    distinct(&found, |s| {
        format!("{}:{}", s["file"].as_str().unwrap(), s["line"])
    });
    // Every page says how many there are in all.
    let body = ok(&c, "features_list", json!({ "cursor": "440" })).await;
    assert_eq!(
        (
            body["total"].as_u64(),
            body["scenarios"].as_array().unwrap().len()
        ),
        (Some(450), 10)
    );
    assert_eq!(body["next_cursor"], Value::Null);
}

#[tokio::test]
async fn running_needs_the_right_and_a_runner() {
    let dir = fixture();
    dir.write("features/items/box.feature", BOX_FEATURE);

    // Without the right: not offered, and refused with what is missing.
    let c = connect(dir.path(), CapSet::of([Cap::Inspect, Cap::Fs])).await;
    assert!(!tool_names(&c).await.contains(&"features_run".to_owned()));
    let (is_error, body) = call(&c, "features_run", json!({})).await;
    assert!(is_error);
    assert_eq!(body["error"]["code"], "capability_denied");
    assert!(
        body["error"]["message"].as_str().unwrap().contains("run"),
        "{body}"
    );
    // Asking about a run needs no right, and there is none to ask about.
    let (code, _) = refused(&c, "run_status", json!({})).await;
    assert_eq!(code, "not_found");
    let (code, _) = refused(&c, "run_cancel", json!({})).await;
    assert_eq!(code, "not_found");

    // With the right but nothing to run scenarios with.
    let c = connect(dir.path(), CapSet::of([Cap::Run])).await;
    let (code, message) = refused(&c, "features_run", json!({})).await;
    assert_eq!(code, "unavailable");
    assert!(message.contains("--features-runner"), "{message}");
}

#[cfg(unix)]
mod runs {
    use super::*;

    async fn connect_runner(dir: &Fixture, body: &str) -> Client {
        let runner = dir.script("runner.sh", body);
        serve(
            server(dir.path()).with_runner(runner),
            CapSet::of([Cap::Run]),
        )
        .await
    }

    const WAIT: u64 = 30_000;

    #[tokio::test]
    async fn a_run_is_a_job_the_agent_asks_about() {
        let dir = fixture();
        dir.write("features/items/box.feature", BOX_FEATURE);
        let c = connect_runner(
            &dir,
            "echo \"backend $LJOS_BDD_BACKEND\"\necho \"asked $*\"\necho \"in $(pwd)\"\necho '[Summary]'\necho '1 scenario (1 passed)'\n",
        )
        .await;

        let started = ok(
            &c,
            "features_run",
            json!({ "paths": ["items"], "tags": "@qt-diverges and not @backlog", "name": "wide" }),
        )
        .await;
        assert_eq!(started["run_id"], "run-1");
        assert_eq!(started["state"], "running");
        assert_eq!(started["backend"], "ljos");
        assert!(
            started["command"].as_str().unwrap().contains("runner.sh"),
            "{started}"
        );

        let ended = ok(
            &c,
            "run_status",
            json!({ "run_id": "run-1", "wait_ms": WAIT }),
        )
        .await;
        assert_eq!(ended["state"], "passed");
        assert_eq!(ended["exit_code"], 0);
        assert_eq!(ended["summary"], "1 scenario (1 passed)");
        assert_eq!(ended["failures"], json!([]));
        let output = ended["output"].as_str().unwrap();
        // On Ljós unless asked otherwise, in the project's root, and with
        // what to run chosen as `features_list` chooses it.
        assert!(output.contains("backend ljos"), "{output}");
        assert!(
            output.contains(&format!("in {}", dir.path().display())),
            "{output}"
        );
        assert!(output.contains("/features/items/**/*.feature"), "{output}");
        assert!(
            output.contains("-t @qt-diverges and not @backlog -n wide"),
            "{output}"
        );
        assert_eq!(ended["output_truncated"], false);

        // The latest run is the one asked about when none is named.
        let latest = ok(&c, "run_status", json!({})).await;
        assert_eq!(latest["run_id"], "run-1");
        assert_eq!(latest["state"], "passed");
        // A run that ended is not stopped by being cancelled.
        let cancelled = ok(&c, "run_cancel", json!({ "run_id": "run-1" })).await;
        assert_eq!(cancelled["state"], "passed");

        let other = ok(&c, "features_run", json!({ "backend": "qt" })).await;
        assert_eq!(
            (other["run_id"].as_str(), other["backend"].as_str()),
            (Some("run-2"), Some("qt"))
        );
        let ended = ok(&c, "run_status", json!({ "wait_ms": WAIT })).await;
        assert!(
            ended["output"].as_str().unwrap().contains("backend qt"),
            "{ended}"
        );

        let (code, message) = refused(&c, "run_status", json!({ "run_id": "run-9" })).await;
        assert_eq!(code, "not_found");
        assert!(message.contains("run-9"), "{message}");
        for (tool, args) in [
            ("features_run", json!({ "paths": ["features/nowhere"] })),
            ("features_run", json!({ "tags": "@a not" })),
            ("features_run", json!({ "backend": "gtk" })),
            ("run_status", json!({ "wait_ms": "long" })),
        ] {
            let (code, _) = refused(&c, tool, args.clone()).await;
            assert!(
                matches!(code.as_str(), "not_found" | "invalid_argument"),
                "{tool} {args}: {code}"
            );
        }
    }

    #[tokio::test]
    async fn a_run_that_fails_says_what_failed() {
        let dir = fixture();
        let c = connect_runner(
            &dir,
            "echo 'Feature: The size of a box'\necho '  Scenario: A box is as wide as it is told'\n\
             echo '   ✔  Given a box'\necho '   ✘  Then the box is 10 wide'\n\
             echo '      Step failed: the box is 12 wide'\necho '[Summary]'\necho '1 scenario (1 failed)'\nexit 1\n",
        )
        .await;
        ok(&c, "features_run", json!({})).await;
        let ended = ok(&c, "run_status", json!({ "wait_ms": WAIT })).await;
        assert_eq!(
            (ended["state"].as_str(), ended["exit_code"].as_i64()),
            (Some("failed"), Some(1))
        );
        assert_eq!(ended["summary"], "1 scenario (1 failed)");
        assert_eq!(ended["failures"].as_array().unwrap().len(), 1);
        let failure = &ended["failures"][0];
        assert_eq!(failure["scenario"], "A box is as wide as it is told");
        assert_eq!(failure["step"], "Then the box is 10 wide");
        assert!(
            failure["detail"]
                .as_str()
                .unwrap()
                .contains("the box is 12 wide")
        );
        assert_eq!(ended["failures_omitted"], 0);
    }

    /// A run that prints megabytes and fails a thousand times is answered
    /// in the size of any other answer.
    #[tokio::test]
    async fn the_output_of_a_run_is_bounded() {
        let dir = fixture();
        let c = connect_runner(
            &dir,
            "n=0\nwhile [ $n -lt 1000 ]; do\n  echo \"  Scenario: Number $n\"\n  echo \"   ✘  Then step $n holds\"\n  \
             echo \"      Step failed: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx $n\"\n  \
             n=$((n+1))\ndone\nhead -c 300000 /dev/zero | tr '\\0' 'y'\necho\necho '[Summary]'\necho '1000 scenarios (1000 failed)'\nexit 3\n",
        )
        .await;
        ok(&c, "features_run", json!({})).await;
        let (is_error, ended, text) =
            call_whole(&c, "run_status", json!({ "wait_ms": WAIT })).await;
        assert!(!is_error, "{ended}");
        assert_eq!(
            (ended["state"].as_str(), ended["exit_code"].as_i64()),
            (Some("failed"), Some(3))
        );
        assert_eq!(ended["summary"], "1000 scenarios (1000 failed)");
        let failures = ended["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 12);
        assert_eq!(failures[0]["scenario"], "Number 0");
        assert_eq!(ended["failures_omitted"], 988);
        assert_eq!(ended["output_truncated"], true);
        let output = ended["output"].as_str().unwrap();
        assert!(output.len() <= 6 * 1024 + 256, "{} bytes", output.len());
        // The end is what is kept: that is where a runner says how it went.
        assert!(output.contains("1000 scenarios (1000 failed)"), "{output}");
        let size = serde_json::to_vec(&ended).unwrap().len();
        assert!(size <= ONE_COPY, "{size} bytes");
        assert!(text.len() <= ONE_COPY + 1024);
        assert!(ended.get("truncated").is_none(), "cut by the cap: {ended}");
    }

    #[tokio::test]
    async fn one_run_at_a_time_and_a_run_can_be_stopped() {
        let dir = fixture();
        let c = connect_runner(&dir, "echo started\nexec sleep 60\n").await;
        ok(&c, "features_run", json!({})).await;

        let running = ok(&c, "run_status", json!({ "run_id": "run-1" })).await;
        assert_eq!(running["state"], "running");
        assert_eq!(running["exit_code"], Value::Null);
        // Waiting comes back when the time is up, with the run still going.
        let before = std::time::Instant::now();
        let running = ok(&c, "run_status", json!({ "wait_ms": 150 })).await;
        assert_eq!(running["state"], "running");
        assert!(before.elapsed() >= std::time::Duration::from_millis(140));
        assert!(before.elapsed() < std::time::Duration::from_secs(20));

        let (code, message) = refused(&c, "features_run", json!({})).await;
        assert_eq!(code, "busy");
        assert!(message.contains("run-1"), "{message}");

        let before = std::time::Instant::now();
        let stopped = ok(&c, "run_cancel", json!({ "run_id": "run-1" })).await;
        assert_eq!(stopped["state"], "cancelled");
        assert!(
            before.elapsed() < std::time::Duration::from_secs(20),
            "the run was waited for"
        );
        let still = ok(&c, "run_status", json!({ "run_id": "run-1" })).await;
        assert_eq!(still["state"], "cancelled");

        // The stopped run made room for the next.
        let next = ok(&c, "features_run", json!({})).await;
        assert_eq!(next["run_id"], "run-2");
        let stopped = ok(&c, "run_cancel", json!({})).await;
        assert_eq!(
            (stopped["run_id"].as_str(), stopped["state"].as_str()),
            (Some("run-2"), Some("cancelled"))
        );
    }
}

async fn read(c: &Client, uri: &str) -> Result<String, rmcp::ServiceError> {
    let res = c
        .read_resource(ReadResourceRequestParams::new(uri.to_owned()))
        .await?;
    match &res.contents[0] {
        rmcp::model::ResourceContents::TextResourceContents { text, .. } => Ok(text.clone()),
        other => panic!("expected text, got {other:?}"),
    }
}

#[tokio::test]
async fn resources_serve_ledger_docs_features_and_steps() {
    let dir = fixture();
    dir.write("features/items/box.feature", BOX_FEATURE);
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
    assert_eq!(
        read(&c, "ljos://features/items/box.feature").await.unwrap(),
        BOX_FEATURE
    );
    assert!(
        read(&c, "ljos://features/README.md")
            .await
            .unwrap()
            .contains("## Step catalogue")
    );

    // The steps are the runner's own, not what a project's README says of
    // them: the README here names a step no runner has.
    let steps = read(&c, "ljos://features/steps").await.unwrap();
    assert_eq!(steps, ljos_bdd::catalogue::markdown());
    assert!(steps.contains("\n- Given the QML:\n"), "{steps}");
    assert!(!steps.contains("Given a window"));
    for step in ljos_bdd::catalogue::steps() {
        assert!(
            steps.contains(&step.text),
            "`{}` is not in the catalogue",
            step.text
        );
    }

    // Escapes: parent traversal, a project file outside docs/ or features/.
    for uri in [
        "ljos://docs/../Main.qml",
        "ljos://docs/../../outside-secret.txt",
        "ljos://features/../Main.qml",
        "ljos://features/../../outside-secret.txt",
        "ljos://docs/nowhere.md",
        "ljos://features/nowhere.feature",
        "ljos://nothing",
        // Neither shape of the uri: no module, or a type with a path in it.
        "ljos://types/",
        "ljos://types/QtQuick/Item/x",
        "ljos://types/QtQuick/Nonesuch",
    ] {
        assert!(read(&c, uri).await.is_err(), "{uri} was read");
    }
    assert!(
        read(&c, "ljos://types/QtQuick/Item")
            .await
            .unwrap()
            .contains("width")
    );
    assert!(
        read(&c, "ljos://types/QtQuick")
            .await
            .unwrap()
            .contains("Rectangle")
    );

    let uris: Vec<String> = c
        .list_all_resources()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.uri.to_string())
        .collect();
    assert_eq!(
        uris,
        [
            "ljos://ledger",
            "ljos://features/steps",
            "ljos://lints",
            "ljos://features/README.md",
            "ljos://docs/ARCHITECTURE.md",
        ]
    );
    // Everything that is listed can be read.
    for uri in &uris {
        assert!(read(&c, uri).await.is_ok(), "{uri}");
    }
    let mut templates: Vec<String> = c
        .list_all_resource_templates()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.uri_template.clone())
        .collect();
    templates.sort();
    assert_eq!(
        templates,
        [
            "ljos://docs/{path}",
            "ljos://features/{path}",
            "ljos://ledger/{module}/{type}",
            "ljos://types/{module}",
            "ljos://types/{module}/{type}",
        ]
    );
}

#[tokio::test]
async fn a_long_document_is_cut_and_says_how_to_read_the_rest() {
    let dir = fixture();
    let mut doc = String::from("# Long\n");
    for n in 0..400 {
        write!(
            doc,
            "\n## Part {n}\n\n{}\n",
            "Something is said about this part. ".repeat(12)
        )
        .unwrap();
    }
    assert!(doc.len() > 2 * HARD_RESULT_BYTES);
    dir.write("docs/LONG.md", &doc);
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let text = read(&c, "ljos://docs/LONG.md").await.unwrap();
    assert!(
        text.len() <= HARD_RESULT_BYTES + 512,
        "{} bytes",
        text.len()
    );
    assert!(text.contains("[truncated: "), "the cut is not marked");
    assert!(
        text.contains("docs_search") || text.contains("#L"),
        "{}",
        &text[text.len() - 300..]
    );

    // The part that was cut off can still be had: by its section.
    let body = ok(&c, "docs_search", json!({ "query": "\"Part 399\"" })).await;
    let hit = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["title"] == "Long > Part 399")
        .unwrap_or_else(|| panic!("{body}"));
    let section = read(&c, hit["section"].as_str().unwrap()).await.unwrap();
    assert!(section.starts_with("## Part 399\n"), "{section}");
    assert!(section.len() < 1024);
}

fn tools_named(text: &str) -> Vec<String> {
    ljos_bdd::author::tools_named(text)
}

#[tokio::test]
async fn prompts_are_listed_and_name_only_tools_that_exist() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::of(Cap::ALL)).await;
    let tools = tool_names(&c).await;
    let prompts = c.list_all_prompts().await.unwrap();
    let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "write-component",
            "port-from-qtquick",
            "add-accessibility",
            "localise",
            "write-feature"
        ]
    );
    for prompt in &prompts {
        let args: serde_json::Map<String, Value> = prompt
            .arguments
            .iter()
            .flatten()
            .map(|a| (a.name.clone(), json!("NoteCard")))
            .collect();
        let got = c
            .get_prompt(
                rmcp::model::GetPromptRequestParams::new(prompt.name.clone()).with_arguments(args),
            )
            .await
            .unwrap();
        let text = format!("{:?}", got.messages);
        assert!(text.contains("NoteCard"), "{}: {text}", prompt.name);
        let named = tools_named(&text);
        assert!(named.len() >= 3, "{} names {named:?}", prompt.name);
        for tool in &named {
            assert!(
                tools.contains(tool),
                "the prompt `{}` names `{tool}`, which is not a tool",
                prompt.name
            );
        }
        // In order: a numbered list, each step with the tool that does it.
        assert!(
            text.contains("1. ") && text.contains("2. "),
            "{}",
            prompt.name
        );

        // A prompt asked for without what it needs says which argument.
        let needed = prompt
            .arguments
            .iter()
            .flatten()
            .find(|a| a.required == Some(true))
            .unwrap();
        let error = c
            .get_prompt(rmcp::model::GetPromptRequestParams::new(
                prompt.name.clone(),
            ))
            .await
            .unwrap_err();
        assert!(error.to_string().contains(&needed.name), "{error}");
    }
    assert!(
        c.get_prompt(rmcp::model::GetPromptRequestParams::new("nonesuch"))
            .await
            .is_err()
    );
}

/// The order the first prompt gives: the ledger, then the documents and the
/// scenarios, then the checks on what was written, then the run.
#[tokio::test]
async fn write_component_names_its_tools_in_the_order_they_are_used() {
    let dir = fixture();
    let c = connect(dir.path(), CapSet::EMPTY).await;
    let got = c
        .get_prompt(
            rmcp::model::GetPromptRequestParams::new("write-component")
                .with_arguments(json!({ "name": "NoteCard" }).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let text = format!("{:?}", got.messages);
    assert_eq!(
        tools_named(&text),
        [
            "api_status",
            "docs_search",
            "features_list",
            "qml_format",
            "qml_check",
            "qml_lint",
            "binding_tier_explain",
            "features_run",
            "run_status",
        ]
    );
}
