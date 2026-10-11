//! The step adapter of the example suite.
//!
//! The suite's feature files (`../fixture/features`) are written in the words
//! of a kiosk application ("the visitor taps Add"), not in Ljos's. This file
//! says what each of those sentences does to a running application, through
//! `Backend`, which is all a suite's adapter needs. The application under
//! test is written out here as QML because the example suite has no
//! application of its own; a real suite's adapter would load the suite's QML
//! instead.

use std::{path::Path, process::ExitCode};

use cucumber::{given, then, when};
use ljos_bdd::acceptance::AcceptanceWorld;
use serde_json::Value;

type StepResult = Result<(), String>;

const SCREEN: &str = "Screen.qml";

fn counter_screen(start: i64) -> String {
    format!(
        r#"import QtQuick
import QtQuick.Controls.Basic
Item {{
  objectName: "screen"
  width: 320; height: 200
  property int count: {start}
  Text {{ objectName: "readout"; text: parent.count }}
  Row {{
    y: 40
    Button {{ text: "Add"; onClicked: count++ }}
    Button {{ text: "Remove"; onClicked: count-- }}
  }}
}}
"#
    )
}

fn welcome_screen(badge: &str) -> String {
    let quoted = badge.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"import QtQuick
Item {{
  width: 320; height: 200
  Text {{ objectName: "headline"; text: "Hello, " + "{quoted}" + "!" }}
}}
"#
    )
}

fn show(world: &mut AcceptanceWorld, source: &str) -> StepResult {
    world.stage(SCREEN, source);
    world.load(SCREEN)
}

/// A property as the text a person would read off the screen.
fn shown(world: &mut AcceptanceWorld, object: &str, property: &str) -> Result<String, String> {
    let value = world
        .backend()
        .get(object, property)
        .map_err(|e| e.to_string())?;
    Ok(match value {
        Value::String(s) => s,
        other => other.to_string(),
    })
}

#[given(expr = "a counter screen that starts at {int}")]
fn counter_starts_at(world: &mut AcceptanceWorld, start: i64) -> StepResult {
    show(world, &counter_screen(start))
}

#[given(expr = "a welcome screen for the badge {string}")]
fn welcome_for(world: &mut AcceptanceWorld, badge: String) -> StepResult {
    show(world, &welcome_screen(&badge))
}

#[when(expr = "the visitor taps {word}")]
fn taps(world: &mut AcceptanceWorld, button: String) -> StepResult {
    world
        .backend()
        .click("button", &button)
        .map_err(|e| e.to_string())
}

#[when(expr = "the visitor taps {word} {int} times")]
fn taps_times(world: &mut AcceptanceWorld, button: String, times: u32) -> StepResult {
    for _ in 0..times {
        taps(world, button.clone())?;
    }
    Ok(())
}

#[then(expr = "the counter shows {int}")]
fn counter_shows(world: &mut AcceptanceWorld, expected: i64) -> StepResult {
    let text = shown(world, "readout", "text")?;
    if text == expected.to_string() {
        Ok(())
    } else {
        Err(format!("the counter shows {text}, not {expected}"))
    }
}

#[then(expr = "the headline reads {string}")]
fn headline_reads(world: &mut AcceptanceWorld, expected: String) -> StepResult {
    let text = shown(world, "headline", "text")?;
    if text == expected {
        Ok(())
    } else {
        Err(format!("the headline reads {text:?}, not {expected:?}"))
    }
}

#[then(expr = "the screen offers the buttons {word} and {word}")]
fn offers_buttons(world: &mut AcceptanceWorld, first: String, second: String) -> StepResult {
    let tree = world.backend().a11y_tree().map_err(|e| e.to_string())?;
    for name in [first, second] {
        if tree.find("button", &name).is_none() {
            return Err(format!("no button named {name:?} on the screen"));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    // The suite's directory in this repository: `PIN`, `features.globs`,
    // `RATCHET`. `--suite` overrides it (the tests run copies).
    ljos_bdd::acceptance::main(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}
