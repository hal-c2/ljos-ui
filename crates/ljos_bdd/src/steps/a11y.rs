//! The application as an assistive technology sees it and acts on it.

use cucumber::{given, then, when};
use serde_json::Value;

use crate::backend::{A11yAction, A11yNode, A11yTextAction, BackendError};
use crate::world::LjosWorld;

use super::StepResult;

// The tree is built once something reads it and then follows every change;
// without this step a scenario's first `Then` would be that first reader,
// and would only ever see a tree built in one go.
#[given("an assistive technology is listening")]
fn listening(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.a11y_tree().map(drop))
}

#[when(regex = r#"^an assistive technology focuses the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_focuses(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| b.assistive()?.a11y_focus(&role, &name))
}

// What a screen reader's "next step" and "scroll" commands ask of a node.
fn a11y_action(what: &str) -> Result<A11yAction, String> {
    let scroll = |dx, dy, page| Ok(A11yAction::Scroll { dx, dy, page });
    let (way, page) = match what.strip_suffix(" a page") {
        Some(way) => (way, true),
        None => (what, false),
    };
    match way {
        "increase" if !page => Ok(A11yAction::Increase),
        "decrease" if !page => Ok(A11yAction::Decrease),
        "press" if !page => Ok(A11yAction::Press),
        "show itself" if !page => Ok(A11yAction::ShowOnScreen),
        "expand" if !page => Ok(A11yAction::Expand),
        "collapse" if !page => Ok(A11yAction::Collapse),
        "scroll up" => scroll(0, -1, page),
        "scroll down" => scroll(0, 1, page),
        "scroll left" => scroll(-1, 0, page),
        "scroll right" => scroll(1, 0, page),
        _ => match what.strip_prefix("take the value ").map(str::parse) {
            Some(Ok(to)) => Ok(A11yAction::SetValue(to)),
            _ => Err(format!("{what:?} is not something to ask of a node")),
        },
    }
}

#[when(regex = r#"^an assistive technology asks the "([^"]+)" named "([^"]*)" to (.+)$"#)]
fn a11y_asks(w: &mut LjosWorld, role: String, name: String, what: String) -> StepResult {
    let action = a11y_action(&what)?;
    w.each(|b| b.assistive()?.a11y_act(&role, &name, action))
}

// On one line, as the lint reads the patterns.
#[rustfmt::skip]
#[when(regex = r#"^an assistive technology selects characters (\d+) to (\d+) of the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_selects(
    w: &mut LjosWorld,
    from: usize,
    to: usize,
    role: String,
    name: String,
) -> StepResult {
    w.each(|b| b.assistive()?.a11y_text(&role, &name, A11yTextAction::Select(from, to)))
}

#[rustfmt::skip]
#[when(regex = r#"^an assistive technology replaces the selection of the "([^"]+)" named "([^"]*)" with "([^"]*)"$"#)]
fn a11y_replaces(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| b.assistive()?.a11y_text(&role, &name, A11yTextAction::ReplaceSelection(text.clone())))
}

#[rustfmt::skip]
#[when(regex = r#"^an assistive technology sets the text of the "([^"]+)" named "([^"]*)" to "([^"]*)"$"#)]
fn a11y_sets_text(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| b.assistive()?.a11y_text(&role, &name, A11yTextAction::SetValue(text.clone())))
}

#[when(
    regex = r"^the design audit runs at font scale ([0-9]+(?:\.[0-9]+)?) and window size ([0-9]+)x([0-9]+)$"
)]
fn design_audit_at(w: &mut LjosWorld, scale: String, width: String, height: String) -> StepResult {
    let matrix = format!("font-scale={scale} size={width}x{height}");
    w.each(|b| b.probes()?.design_audit(Some(&matrix)).map(drop))
}

/// The findings of a design audit's report, one a line.
fn audit_lines(report: &Value) -> Vec<String> {
    let text = |f: &Value, k: &str| f[k].as_str().unwrap_or("").to_owned();
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            format!(
                "{} [{}] on the {:?} named {:?}: {}",
                text(f, "source"),
                text(f, "code"),
                text(f, "role"),
                text(f, "name"),
                text(f, "message")
            )
        })
        .collect()
}

#[then("the design audit finds nothing")]
fn design_audit_clean(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let lines = audit_lines(&b.probes()?.design_audit(None)?);
        if lines.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the design audit found:\n  {}",
                lines.join("\n  ")
            )))
        }
    })
}

#[then(regex = r#"^the design audit reports "([a-z0-9-]+)" on the "([^"]+)" named "([^"]*)"$"#)]
fn design_audit_reports(w: &mut LjosWorld, code: String, role: String, name: String) -> StepResult {
    w.each(|b| {
        let report = b.probes()?.design_audit(None)?;
        let hit = report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|f| {
                f["code"] == code.as_str()
                    && f["role"] == role.as_str()
                    && f["name"].as_str().unwrap_or("") == name
            });
        if hit {
            return Ok(());
        }
        let lines = audit_lines(&report);
        Err(BackendError::Failed(format!(
            "no {code:?} on the {role:?} named {name:?}; the design audit found:\n  {}",
            if lines.is_empty() {
                "nothing".to_owned()
            } else {
                lines.join("\n  ")
            }
        )))
    })
}

/// A rule's findings, wherever they are: a part a style or an
/// application draws (a background, an indicator) has no node to name.
fn audit_codes(w: &mut LjosWorld, code: &str, want: bool) -> StepResult {
    w.each(|b| {
        let report = b.probes()?.design_audit(None)?;
        let hit = report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|f| f["code"] == code);
        if hit == want {
            return Ok(());
        }
        let lines = audit_lines(&report);
        Err(BackendError::Failed(format!(
            "{} {code:?}; the design audit found:\n  {}",
            if want { "no" } else { "a finding of" },
            if lines.is_empty() {
                "nothing".to_owned()
            } else {
                lines.join("\n  ")
            }
        )))
    })
}

#[then(regex = r#"^the design audit reports "([a-z0-9-]+)"$"#)]
fn design_audit_reports_code(w: &mut LjosWorld, code: String) -> StepResult {
    audit_codes(w, &code, true)
}

#[then(regex = r#"^the design audit does not report "([a-z0-9-]+)"$"#)]
fn design_audit_not_code(w: &mut LjosWorld, code: String) -> StepResult {
    audit_codes(w, &code, false)
}

#[then(regex = r#"^the accessibility tree has an? "([^"]+)" named "([^"]*)"$"#)]
fn a11y_has(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        tree.find(&role, &name).map(drop).ok_or_else(|| {
            BackendError::Failed(format!(
                "no {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))
        })
    })
}

#[then(regex = r#"^the accessibility tree has no "([^"]+)" named "([^"]*)"$"#)]
fn a11y_lacks(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        match tree.find(&role, &name) {
            None => Ok(()),
            Some(_) => Err(BackendError::Failed(format!(
                "found {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))),
        }
    })
}

#[then(regex = r#"^the object "([^"]+)" is exposed as an? "([^"]+)" named "([^"]*)"$"#)]
fn a11y_object(w: &mut LjosWorld, object: String, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = find_object(&tree, &object).ok_or_else(|| {
            BackendError::Failed(format!(
                "object {object:?} is not exposed:\n{}",
                render(&tree, 0)
            ))
        })?;
        if node.role == role && node.name == name {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "object {object:?} is a {:?} named {:?}, expected a {role:?} named {name:?}",
                node.role, node.name
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is described as "([^"]*)"$"#)]
fn a11y_described(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = tree.find(&role, &name).ok_or_else(|| {
            BackendError::Failed(format!(
                "no {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))
        })?;
        if node.description == text {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is described as {:?}, expected {text:?}",
                node.description
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" has the value "([^"]*)"$"#)]
fn a11y_value(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = tree.find(&role, &name).ok_or_else(|| {
            BackendError::Failed(format!(
                "no {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))
        })?;
        if node.value == text {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} has the value {:?}, expected {text:?}",
                node.value
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" has "([^"]*)" selected$"#)]
fn a11y_selected(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = tree.find(&role, &name).ok_or_else(|| {
            BackendError::Failed(format!(
                "no {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))
        })?;
        if node.selected == text {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} has {:?} selected, expected {text:?}",
                node.selected
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is inside the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_inside(
    w: &mut LjosWorld,
    role: String,
    name: String,
    outer_role: String,
    outer_name: String,
) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let parent = tree.parent_of(&role, &name).ok_or_else(|| {
            BackendError::Failed(format!(
                "no {role:?} named {name:?} in:\n{}",
                render(&tree, 0)
            ))
        })?;
        if parent.is(&outer_role, &outer_name) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is inside the {:?} named {:?}:\n{}",
                parent.role,
                parent.name,
                render(&tree, 0)
            )))
        }
    })
}

// The node a step names, or what the tree has instead.
fn a11y_node<'t>(tree: &'t A11yNode, role: &str, name: &str) -> Result<&'t A11yNode, BackendError> {
    tree.find(role, name).ok_or_else(|| {
        BackendError::Failed(format!(
            "no {role:?} named {name:?} in:\n{}",
            render(tree, 0)
        ))
    })
}

#[rustfmt::skip]
#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is (not )?(checkable|checked|partially checked|pressed|expandable|expanded|selectable|selected|read-only|disabled|live|assertive|off screen)$"#)]
fn a11y_state(
    w: &mut LjosWorld,
    role: String,
    name: String,
    not: String,
    state: String,
) -> StepResult {
    let want = not.is_empty();
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        if node.states.contains(&state) == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is {}{state}; it says {:?}",
                if want { "not " } else { "" },
                node.states
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is item (\d+) of (\d+)$"#)]
fn a11y_item_of(
    w: &mut LjosWorld,
    role: String,
    name: String,
    place: usize,
    of: usize,
) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        match node.set {
            Some(set) if set == (place, of) => Ok(()),
            Some((p, n)) => Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is item {p} of {n}, expected {place} of {of}"
            ))),
            None => Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} does not say which item it is"
            ))),
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is not an item of a set$"#)]
fn a11y_not_item_of(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        match a11y_node(&tree, &role, &name)?.set {
            None => Ok(()),
            Some((p, n)) => Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is item {p} of {n}, expected no place in a set"
            ))),
        }
    })
}

#[rustfmt::skip]
#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is at (-?[\d.]+) in the range (-?[\d.]+) to (-?[\d.]+)$"#)]
fn a11y_range(
    w: &mut LjosWorld,
    role: String,
    name: String,
    value: f64,
    min: f64,
    max: f64,
) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        let Some((v, lo, hi)) = &node.range else {
            return Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} has no range"
            )));
        };
        let same = |found: &str, want: f64| {
            found
                .parse::<f64>()
                .is_ok_and(|f| (f - want).abs() <= 1e-6 * want.abs().max(1.0))
        };
        if same(v, value) && same(lo, min) && same(hi, max) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is at {v} in the range {lo} to {hi}, \
                 expected {value} in {min} to {max}"
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" has no range$"#)]
fn a11y_no_range(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        match &a11y_node(&tree, &role, &name)?.range {
            None => Ok(()),
            Some((v, lo, hi)) => Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is at {v} in the range {lo} to {hi}"
            ))),
        }
    })
}

#[rustfmt::skip]
#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is labelled by the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_labelled_by(
    w: &mut LjosWorld,
    role: String,
    name: String,
    label_role: String,
    label_name: String,
) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        let label = (label_role.clone(), label_name.clone());
        if node.labelled_by.contains(&label) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is labelled by {:?}, \
                 expected the {label_role:?} named {label_name:?}",
                node.labelled_by
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is labelled by nothing$"#)]
fn a11y_unlabelled(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        if node.labelled_by.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is labelled by {:?}",
                node.labelled_by
            )))
        }
    })
}

#[then(regex = r#"^the "([^"]+)" named "([^"]*)" is called an? "([^"]+)"$"#)]
fn a11y_called(w: &mut LjosWorld, role: String, name: String, called: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        let node = a11y_node(&tree, &role, &name)?;
        if node.called == called {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the {role:?} named {name:?} is called {:?}, expected {called:?}",
                node.called
            )))
        }
    })
}

#[then(regex = r#"^the accessibility tree's focus is the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_focus_is(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| {
        let tree = b.a11y_tree()?;
        match tree.focus() {
            Some(n) if n.is(&role, &name) => Ok(()),
            Some(n) => Err(BackendError::Failed(format!(
                "the focus is the {:?} named {:?}, expected the {role:?} named {name:?}:\n{}",
                n.role,
                n.name,
                render(&tree, 0)
            ))),
            None => Err(BackendError::Failed(format!(
                "nothing has the focus:\n{}",
                render(&tree, 0)
            ))),
        }
    })
}

fn find_object<'a>(n: &'a A11yNode, object: &str) -> Option<&'a A11yNode> {
    if n.object_name == object {
        return Some(n);
    }
    n.children.iter().find_map(|c| find_object(c, object))
}

fn render(n: &A11yNode, depth: usize) -> String {
    let mut out = format!("{}{} {:?}\n", "  ".repeat(depth), n.role, n.name);
    for c in &n.children {
        out.push_str(&render(c, depth + 1));
    }
    out
}
