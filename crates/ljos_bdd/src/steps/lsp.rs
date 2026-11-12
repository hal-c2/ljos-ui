//! An editor's session with `ljos lsp` (`features/tooling/language-server.feature`).

use cucumber::{then, when};
use gherkin::Step;

use crate::backend::lsp::Session;
use crate::backend::testrun::expected_lines;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

#[when(regex = r#"^the language server opens "([^"]+)"$"#)]
fn opens(w: &mut LjosWorld, file: String) -> StepResult {
    w.language_server(&Session {
        file,
        ..Session::default()
    })
}

#[when(regex = r#"^the language server opens "([^"]+)" and saves it$"#)]
fn opens_saves(w: &mut LjosWorld, file: String) -> StepResult {
    w.language_server(&Session {
        file,
        save: true,
        ..Session::default()
    })
}

#[when(regex = r#"^the language server opens "([^"]+)" and it is changed to:$"#)]
fn opens_changes(w: &mut LjosWorld, file: String, step: &Step) -> StepResult {
    w.language_server(&Session {
        file,
        change: Some(docstring(step)?),
        ..Session::default()
    })
}

#[when(regex = r#"^the language server is asked for the actions on "([^"]+)"$"#)]
fn asked_actions(w: &mut LjosWorld, file: String) -> StepResult {
    w.language_server(&Session {
        file,
        actions: true,
        ..Session::default()
    })
}

/// Whether the server published for the file at all: publishing nothing
/// for an opened file is not the same as publishing that nothing is wrong.
fn published(report: &crate::backend::testrun::TestReport) -> Result<(), String> {
    match report.json.as_ref().and_then(|j| j["published"].as_bool()) {
        Some(true) => Ok(()),
        _ => Err("the language server published no diagnostics for the file".into()),
    }
}

#[then(regex = r#"^the language server publishes for "([^"]+)":$"#)]
fn publishes(w: &mut LjosWorld, _file: String, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        published(report)?;
        if report.lines == want {
            Ok(())
        } else {
            Err(format!("the language server publishes\n{}", report.text()))
        }
    })
}

#[then(regex = r#"^the language server publishes nothing for "([^"]+)"$"#)]
fn publishes_nothing(w: &mut LjosWorld, _file: String) -> StepResult {
    w.each_test_report(|report| {
        published(report)?;
        if report.lines.is_empty() {
            Ok(())
        } else {
            Err(format!("the language server publishes\n{}", report.text()))
        }
    })
}

#[then(regex = r#"^the language server offers the action "([^"]+)"$"#)]
fn offers(w: &mut LjosWorld, title: String) -> StepResult {
    w.each_test_report(|report| {
        let actions: Vec<&str> = report
            .json
            .as_ref()
            .and_then(|j| j["actions"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str())
            .collect();
        if actions.contains(&title.as_str()) {
            Ok(())
        } else {
            Err(format!("the language server offers {actions:?}"))
        }
    })
}
