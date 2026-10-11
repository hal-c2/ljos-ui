//! Spawns the real `ljos` binary and speaks MCP to it over stdio, the way an
//! agent host does. Uses a raw newline-delimited JSON-RPC exchange so the test
//! also pins the wire format, independent of any SDK.

use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

use serde_json::{Value, json};

#[test]
fn mcp_server_answers_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("ledger")).unwrap();
    std::fs::write(
        dir.path().join("ledger/qtquick.toml"),
        "[[row]]\nmodule='QtQuick'\ntype='Item'\nkind='type'\nstatus='unsupported'\nreason='M0 in progress'\n",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(["mcp", "--project"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn ljos");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut rpc = |msg: Value, expect_reply: bool| -> Option<Value> {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        expect_reply.then(|| {
            let mut line = String::new();
            stdout.read_line(&mut line).expect("reply line");
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON `{line}`: {e}"))
        })
    };

    let init = rpc(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"smoke","version":"0"}}}),
        true,
    )
    .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "ljos");
    rpc(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        false,
    );

    let tools = rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), true).unwrap();
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "qml_check",
            "qml_lint",
            "type_lookup",
            "api_status",
            "project_info",
            "qml_format",
            "binding_tier_explain",
            "docs_search",
            "features_list",
            // `features_run` is offered only with `--caps run`.
            "run_status",
            "run_cancel"
        ]
    );

    let info = rpc(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
               "params":{"name":"project_info","arguments":{}}}),
        true,
    )
    .unwrap();
    assert_eq!(info["result"]["structuredContent"]["ledger"]["rows"], 1);

    drop(stdin); // EOF is how a host ends the session.
    let status = child.wait().unwrap();
    assert!(status.success(), "ljos exited with {status}");
}

#[test]
fn bad_project_fails_with_a_message_on_stderr() {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(["mcp", "--project", "/definitely/not/here"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "stdout must stay protocol-only");
    assert!(String::from_utf8_lossy(&out.stderr).contains("not usable"));
}
