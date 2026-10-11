//! The formatter is a tool: it parses QML, and a shipped application has no
//! parser (ARCHITECTURE.md §5). This asks cargo which crates of the
//! workspace reach `qml_fmt`, as `ljos_shell/tests/no_mcp.rs` asks what the
//! shell links.

use std::process::Command;

/// The only crates that may link the formatter: the command line and the
/// authoring server it starts.
const TOOLS: [&str; 3] = ["qml_fmt", "ljos_cli", "ljos_mcp_author"];

#[test]
fn only_the_tools_depend_on_the_formatter() {
    let out = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "--workspace", "--invert", "qml_fmt"])
        .args(["--edges", "normal", "--prefix", "none"])
        .args(["--format", "{p}", "--offline"])
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("cargo tree prints text");
    let users: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        users.contains(&"qml_fmt"),
        "the list is the formatter's: {users:?}"
    );
    for user in users {
        assert!(
            TOOLS.contains(&user),
            "{user} depends on qml_fmt: only {TOOLS:?} may"
        );
    }
}
