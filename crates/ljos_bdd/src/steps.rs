//! The step catalogue. Every phrasing here is documented in `features/README.md`;
//! add a step in both places or `features:lint` will not know it.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;
use serde_json::Value;

use crate::{
    backend::{A11yNode, BackendError, LogLevel},
    value,
    world::{LjosWorld, MAIN_FILE},
};

type StepResult = Result<(), String>;

/// The doc string, without the language label (`"""qml`) that gherkin keeps as its first line.
fn docstring(step: &Step) -> Result<String, String> {
    let text = step
        .docstring
        .as_deref()
        .ok_or_else(|| "this step needs a doc string".to_owned())?;
    Ok(strip_label(text).to_owned())
}

fn strip_label(text: &str) -> &str {
    match text.split_once('\n') {
        Some((first, rest)) if first.trim() == "qml" => rest,
        _ => text,
    }
}

/// A `Then` that compares a read value; `check` returns the failure text.
fn expect_value(
    w: &mut LjosWorld,
    object: &str,
    property: &str,
    check: impl Fn(&Value) -> Result<(), String>,
) -> StepResult {
    w.each(|b| {
        let actual = b.get(object, property)?;
        check(&actual).map_err(|why| {
            BackendError::Failed(format!("property {property:?} of {object:?}: {why}"))
        })
    })
}

// ---------------------------------------------------------------- Given

#[given("the QML:")]
fn the_qml(w: &mut LjosWorld, step: &Step) -> StepResult {
    w.stage(MAIN_FILE, &docstring(step)?);
    w.load(MAIN_FILE)
}

#[given(regex = r#"^the file "([^"]+)":$"#)]
fn the_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    w.stage(&name, &docstring(step)?);
    Ok(())
}

// ----------------------------------------------------------------- When

#[when(regex = r#"^"([^"]+)" is loaded$"#)]
fn file_loaded(w: &mut LjosWorld, name: String) -> StepResult {
    w.load(&name)
}

#[when(regex = r#"^the property "([^"]+)" of "([^"]+)" is set to (.+)$"#)]
fn property_set(w: &mut LjosWorld, property: String, object: String, v: String) -> StepResult {
    let v = value::parse(&v)?;
    w.each(|b| b.set(&object, &property, &v))
}

#[when(regex = r#"^the property "([^"]+)" of "([^"]+)" is bound to "([^"]+)"$"#)]
fn property_bound(
    w: &mut LjosWorld,
    property: String,
    object: String,
    expression: String,
) -> StepResult {
    w.each(|b| b.bind(&object, &property, &expression))
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called$"#)]
fn method_called(w: &mut LjosWorld, method: String, object: String) -> StepResult {
    w.each(|b| b.call(&object, &method, &[]).map(drop))
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called with (.+)$"#)]
fn method_called_with(
    w: &mut LjosWorld,
    method: String,
    object: String,
    args: String,
) -> StepResult {
    let args = value::parse_list(&args)?;
    w.each(|b| b.call(&object, &method, &args).map(drop))
}

#[when(regex = r"^(\d+) milliseconds? pass(?:es)?$")]
fn milliseconds_pass(w: &mut LjosWorld, ms: u64) -> StepResult {
    w.each(|b| b.advance(ms))
}

#[when(regex = r"^(\d+(?:\.\d+)?) seconds? pass(?:es)?$")]
fn seconds_pass(w: &mut LjosWorld, s: f64) -> StepResult {
    // Rounded to the millisecond; sub-millisecond time has no meaning to a UI.
    let ms = (s * 1000.0).round() as u64;
    w.each(|b| b.advance(ms))
}

#[when(regex = r#"^the user clicks the "([^"]+)" "([^"]+)"$"#)]
fn user_clicks(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| b.click(&role, &name))
}

#[when(regex = r#"^the user presses "([^"]+)"$"#)]
fn user_presses(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.key(&chord))
}

#[when(regex = r"^the user touches (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_touches(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.touch(x, y))
}

// ----------------------------------------------------------------- Then

// The value must look like JSON so that `is not …`, `is at least …` stay unambiguous.
#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is (true|false|null|[-0-9"\[{].*)$"#)]
fn property_is(
    w: &mut LjosWorld,
    property: String,
    object: String,
    expected: String,
) -> StepResult {
    let expected = value::parse(&expected)?;
    expect_value(w, &object, &property, |actual| {
        if value::same(&expected, actual) {
            Ok(())
        } else {
            Err(format!("expected {expected}, found {actual}"))
        }
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is not (.+)$"#)]
fn property_is_not(
    w: &mut LjosWorld,
    property: String,
    object: String,
    unwanted: String,
) -> StepResult {
    let unwanted = value::parse(&unwanted)?;
    expect_value(w, &object, &property, |actual| {
        if value::same(&unwanted, actual) {
            Err(format!("expected anything but {unwanted}"))
        } else {
            Ok(())
        }
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is at (least|most) (-?\d+(?:\.\d+)?)$"#)]
fn property_bound_check(
    w: &mut LjosWorld,
    property: String,
    object: String,
    which: String,
    limit: f64,
) -> StepResult {
    let least = which == "least";
    expect_value(w, &object, &property, |actual| {
        let n = value::number(actual).ok_or_else(|| format!("{actual} is not a number"))?;
        let ok = if least { n >= limit } else { n <= limit };
        if ok {
            Ok(())
        } else {
            Err(format!("expected at {which} {limit}, found {n}"))
        }
    })
}

#[then(regex = r#"^setting the property "([^"]+)" of "([^"]+)" to (.+) is rejected$"#)]
fn set_rejected(w: &mut LjosWorld, property: String, object: String, v: String) -> StepResult {
    let v = value::parse(&v)?;
    w.each(|b| match b.set(&object, &property, &v) {
        Err(BackendError::Failed(_)) => Ok(()),
        Err(other) => Err(other),
        Ok(()) => Err(BackendError::Failed(format!(
            "setting {property:?} of {object:?} was accepted"
        ))),
    })
}

#[then(regex = r#"^loading "([^"]+)" is rejected$"#)]
fn load_rejected(w: &mut LjosWorld, name: String) -> StepResult {
    w.load_rejected(&name)
}

#[then(regex = r#"^the object "([^"]+)" (exists|does not exist)$"#)]
fn object_exists(w: &mut LjosWorld, object: String, which: String) -> StepResult {
    let want = which == "exists";
    w.each(|b| {
        if b.exists(&object)? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "object {object:?} {}",
                if want { "is missing" } else { "exists" }
            )))
        }
    })
}

fn log_matches(w: &mut LjosWorld, pattern: &str, level: LogLevel, want: bool) -> StepResult {
    let re = Regex::new(pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| {
        let log = b.log()?;
        let hit = log
            .iter()
            .any(|e| e.level == level && re.is_match(&e.message));
        if hit == want {
            return Ok(());
        }
        let seen: Vec<String> = log
            .iter()
            .map(|e| format!("  {}: {}", e.level, e.message))
            .collect();
        Err(BackendError::Failed(format!(
            "expected {} {level} matching {pattern:?}; log was:\n{}",
            if want { "a" } else { "no" },
            seen.join("\n")
        )))
    })
}

#[then(regex = r#"^the log has a warning matching "([^"]+)"$"#)]
fn log_has_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Warning, true)
}

#[then(regex = r#"^the log has no warning matching "([^"]+)"$"#)]
fn log_has_no_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Warning, false)
}

#[then("the log has no warnings")]
fn log_clean(w: &mut LjosWorld) -> StepResult {
    log_matches(w, ".*", LogLevel::Warning, false)
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

#[cfg(test)]
mod tests {
    use super::strip_label;

    #[test]
    fn language_label_is_removed() {
        assert_eq!(
            strip_label("qml\nimport QtQuick\nItem {}"),
            "import QtQuick\nItem {}"
        );
        assert_eq!(strip_label("import QtQuick"), "import QtQuick");
    }
}
