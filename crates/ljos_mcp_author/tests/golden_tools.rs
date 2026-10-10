//! Golden JSON of every tool's schema and description (§19.9): any drift in what
//! an agent sees is a reviewed diff.

use std::sync::Arc;

use ljos_mcp_author::{AuthorServer, Project, StubFrontEnd};
use ljos_mcp_core::McpSurface;
use serde_json::json;

#[test]
fn every_tool_definition_matches_its_golden_file() {
    let dir = tempfile::tempdir().unwrap();
    let server = AuthorServer::new(Project::open(dir.path()).unwrap(), Arc::new(StubFrontEnd));
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("golden");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for def in server.tools().all() {
            let value = json!({
                "name": def.name,
                "description": def.description,
                "required_cap": def.required_cap,
                "annotations": def.annotations,
                "max_result_bytes": def.max_result_bytes,
                "inputSchema": &*def.input_schema,
                "outputSchema": def.output_schema.as_deref(),
            });
            insta::assert_json_snapshot!(def.name, value);
        }
    });
}

#[test]
fn descriptions_stay_within_budget_and_every_tool_has_an_output_schema() {
    let dir = tempfile::tempdir().unwrap();
    let server = AuthorServer::new(Project::open(dir.path()).unwrap(), Arc::new(StubFrontEnd));
    assert!(server.tools().len() <= 25);
    for def in server.tools().all() {
        assert!(def.description.chars().count() <= 300, "{}", def.name);
        assert!(
            def.output_schema.is_some(),
            "{} lacks outputSchema",
            def.name
        );
    }
}
