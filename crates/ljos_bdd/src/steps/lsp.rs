//! An editor's session with `ljos lsp` (`features/tooling/language-server.feature`).

use cucumber::{given, then, when};
use gherkin::Step;
use serde_json::Value;

use crate::backend::lsp::Session;
use crate::backend::testrun::expected_lines;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

#[given(regex = r#"^the editor has "([^"]+)" open, changed to:$"#)]
fn editor_has(w: &mut LjosWorld, file: String, step: &Step) -> StepResult {
    w.editor_has(file, Some(docstring(step)?));
    Ok(())
}

#[given(regex = r#"^the editor has "([^"]+)" open$"#)]
fn editor_has_saved(w: &mut LjosWorld, file: String) -> StepResult {
    w.editor_has(file, None);
    Ok(())
}

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

#[when(regex = r#"^the language server opens "([^"]+)", it is changed to this and saved:$"#)]
fn opens_changes_saves(w: &mut LjosWorld, file: String, step: &Step) -> StepResult {
    w.language_server(&Session {
        file,
        change: Some(docstring(step)?),
        save: true,
        ..Session::default()
    })
}

#[when(regex = r#"^the language server opens "([^"]+)", then "([^"]+)" is closed$"#)]
fn opens_closes(w: &mut LjosWorld, file: String, closed: String) -> StepResult {
    w.language_server(&Session {
        file,
        close: Some(closed),
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

/// What the server last published for `file`: publishing nothing for an
/// opened file is not the same as publishing that nothing is wrong.
fn published(
    report: &crate::backend::testrun::TestReport,
    file: &str,
) -> Result<Vec<String>, String> {
    let all = report.json.as_ref().map(|j| &j["published"]);
    match all.and_then(|all| all[file].as_array()) {
        Some(lines) => Ok(lines
            .iter()
            .filter_map(|line| line.as_str().map(str::to_owned))
            .collect()),
        None => Err(format!(
            "the language server published no diagnostics for {file:?}; it published for {}",
            all.and_then(Value::as_object)
                .map(|all| format!("{:?}", all.keys().collect::<Vec<_>>()))
                .unwrap_or_default()
        )),
    }
}

#[then(regex = r#"^the language server publishes for "([^"]+)":$"#)]
fn publishes(w: &mut LjosWorld, file: String, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        let lines = published(report, &file)?;
        if lines == want {
            Ok(())
        } else {
            Err(format!(
                "the language server publishes for {file:?}\n{}",
                lines.join("\n")
            ))
        }
    })
}

#[then(regex = r#"^the language server publishes nothing for "([^"]+)"$"#)]
fn publishes_nothing(w: &mut LjosWorld, file: String) -> StepResult {
    w.each_test_report(|report| {
        let lines = published(report, &file)?;
        if lines.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the language server publishes for {file:?}\n{}",
                lines.join("\n")
            ))
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
            .filter_map(|a| a["title"].as_str())
            .collect();
        if actions.contains(&title.as_str()) {
            Ok(())
        } else {
            Err(format!("the language server offers {actions:?}"))
        }
    })
}

#[then(regex = r#"^the action "([^"]+)" renames "([^"]+)" to "([^"]+)"$"#)]
fn action_renames(w: &mut LjosWorld, title: String, from: String, to: String) -> StepResult {
    w.each_test_report(|report| {
        let actions = report.json.as_ref().and_then(|j| j["actions"].as_array());
        let action = actions
            .into_iter()
            .flatten()
            .find(|a| a["title"] == title.as_str())
            .ok_or_else(|| format!("the language server offers no action {title:?}"))?;
        let want = serde_json::json!([from, to]);
        let renames = action["renames"].as_array().map_or(&[][..], Vec::as_slice);
        if renames.contains(&want) {
            Ok(())
        } else {
            Err(format!("the action renames {}", action["renames"]))
        }
    })
}

#[then(regex = r#"^the action "([^"]+)" leaves "([^"]+)" in the editor as:$"#)]
fn action_leaves(w: &mut LjosWorld, title: String, file: String, step: &Step) -> StepResult {
    let want = crate::author::text_of(&docstring(step)?);
    w.each_test_report(|report| {
        let actions = report.json.as_ref().and_then(|j| j["actions"].as_array());
        let action = actions
            .into_iter()
            .flatten()
            .find(|a| a["title"] == title.as_str())
            .ok_or_else(|| format!("the language server offers no action {title:?}"))?;
        match action["edits"][file.as_str()].as_str() {
            Some(got) if got == want => Ok(()),
            Some(got) => Err(format!("the action leaves {file:?} as:\n{got}")),
            None => Err(format!(
                "the action does not edit {file:?}: {}",
                action["edits"]
            )),
        }
    })
}
