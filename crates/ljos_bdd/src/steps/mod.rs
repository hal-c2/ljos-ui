//! The step catalogue, a file for each domain. Every phrasing here is
//! documented in `features/README.md`; add a step in both places or
//! `features:lint` will not know it.

use gherkin::Step;
use serde_json::Value;

use crate::backend::BackendError;
use crate::world::LjosWorld;

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
        Some((first, rest)) if matches!(first.trim(), "qml" | "xml" | "js" | "json") => rest,
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

mod a11y;
mod application;
mod authoring;
mod design_audit;
mod environment;
mod files;
mod input;
mod inspector;
mod pixels;
mod platform;
mod probes;
mod qttest;
mod reload;
mod scene;

/// The text of every file of steps, in the order they are read: what the
/// catalogue and `features:lint` read the step patterns from.
pub const SOURCES: [&str; 14] = [
    include_str!("files.rs"),
    include_str!("scene.rs"),
    include_str!("input.rs"),
    include_str!("a11y.rs"),
    include_str!("environment.rs"),
    include_str!("platform.rs"),
    include_str!("application.rs"),
    include_str!("reload.rs"),
    include_str!("pixels.rs"),
    include_str!("probes.rs"),
    include_str!("qttest.rs"),
    include_str!("inspector.rs"),
    include_str!("authoring.rs"),
    include_str!("design_audit.rs"),
];

/// Every file of steps, one after the other.
#[must_use]
pub fn source() -> String {
    SOURCES.concat()
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
