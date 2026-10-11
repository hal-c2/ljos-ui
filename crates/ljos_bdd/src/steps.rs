//! The step catalogue. Every phrasing here is documented in `features/README.md`;
//! add a step in both places or `features:lint` will not know it.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;
use serde_json::Value;

use crate::{
    backend::{A11yAction, A11yNode, A11yTextAction, BackendError, LogLevel, PointerStep, WheelBy},
    fixture, value,
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
        Some((first, rest)) if matches!(first.trim(), "qml" | "xml") => rest,
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

#[given(regex = r#"^"([^"]*)" was copied in another application$"#)]
fn clipboard_given(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.set_clipboard(&text))
}

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

// The tree is built once something reads it and then follows every change;
// without this step a scenario's first `Then` would be that first reader,
// and would only ever see a tree built in one go.
#[given("an assistive technology is listening")]
fn listening(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.a11y_tree().map(drop))
}

// A Qt Linguist file, as a translator hands it over. Its `language`
// attribute says which language it is for.
#[given(regex = r#"^the translation file "([^"]+\.ts)":$"#)]
fn the_translation_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let ts = docstring(step)?;
    // An XML declaration must be the first thing in the file, and the doc
    // string keeps the indentation it has in the scenario.
    let ts = ts.trim_start();
    w.each(|b| b.translation(&name, ts))
}

// An image the QML can name by a relative URL. It is made here, in one
// colour, so that no binary file has to live beside the scenarios.
#[given(regex = r#"^an? (\d+) by (\d+) image file "([^"]+)" filled with "(#[0-9a-fA-F]{6})"$"#)]
fn an_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colour: String,
) -> StepResult {
    let bytes = fixture::image_file(&name, width, height, &colour)?;
    w.each(|b| b.resource(&name, &bytes))
}

// A file with an image's name and something else inside.
#[given(regex = r#"^a file "([^"]+)" that is not an image$"#)]
fn not_an_image(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.resource(&name, &fixture::not_an_image()))
}

// ----------------------------------------------------------------- When

#[when(regex = r#"^the UI language is set to "([^"]*)"$"#)]
fn ui_language_set(w: &mut LjosWorld, language: String) -> StepResult {
    w.each(|b| b.set_ui_language(&language))
}

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

#[when(regex = r#"^an assistive technology focuses the "([^"]+)" named "([^"]*)"$"#)]
fn a11y_focuses(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| b.a11y_focus(&role, &name))
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
    w.each(|b| b.a11y_act(&role, &name, action))
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
    w.each(|b| b.a11y_text(&role, &name, A11yTextAction::Select(from, to)))
}

#[rustfmt::skip]
#[when(regex = r#"^an assistive technology replaces the selection of the "([^"]+)" named "([^"]*)" with "([^"]*)"$"#)]
fn a11y_replaces(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| b.a11y_text(&role, &name, A11yTextAction::ReplaceSelection(text.clone())))
}

#[rustfmt::skip]
#[when(regex = r#"^an assistive technology sets the text of the "([^"]+)" named "([^"]*)" to "([^"]*)"$"#)]
fn a11y_sets_text(w: &mut LjosWorld, role: String, name: String, text: String) -> StepResult {
    w.each(|b| b.a11y_text(&role, &name, A11yTextAction::SetValue(text.clone())))
}

#[when(regex = r#"^the user presses "([^"]+)"$"#)]
fn user_presses(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.key(&chord))
}

// A key held down repeats; the repeats are presses without a release between.
#[when(regex = r#"^the user holds down "([^"]+)" until it repeats (\d+) times?$"#)]
fn user_holds(w: &mut LjosWorld, chord: String, repeats: u32) -> StepResult {
    w.each(|b| b.key_hold(&chord, repeats))
}

// Another window takes the keyboard, or gives it back.
#[when(regex = r"^the window (loses|gains) focus$")]
fn window_focus(w: &mut LjosWorld, which: String) -> StepResult {
    w.each(|b| b.window_active(which == "gains"))
}

#[when(regex = r"^the user touches (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_touches(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.touch(x, y))
}

// A click in parts, for what happens while the button is held.
#[when(regex = r"^the user presses the pointer at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_presses_pointer(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.pointer(PointerStep::Press, x, y))
}

#[when(regex = r"^the user moves the pointer to (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_moves_pointer(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.pointer(PointerStep::Move, x, y))
}

#[when(regex = r"^the user releases the pointer at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_releases_pointer(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.pointer(PointerStep::Release, x, y))
}

#[when(regex = r#"^the user types "([^"]+)"$"#)]
fn user_types(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.type_text(&text))
}

#[when(regex = r"^the user double-clicks at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_double_clicks(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.double_click(x, y))
}

#[when(regex = r"^the user drags from ([-\d.]+), ([-\d.]+) to ([-\d.]+), ([-\d.]+)$")]
fn user_drags(w: &mut LjosWorld, x1: f64, y1: f64, x2: f64, y2: f64) -> StepResult {
    w.each(|b| b.drag((x1, y1), (x2, y2)))
}

// "Down" is towards the end of the content, as the user means it: the
// wheel turns towards the user, which is a negative delta in Qt.
fn wheel_delta(amount: f64, direction: &str) -> Result<(f64, f64), String> {
    match direction {
        "up" => Ok((0.0, amount)),
        "down" => Ok((0.0, -amount)),
        "left" => Ok((amount, 0.0)),
        "right" => Ok((-amount, 0.0)),
        _ => Err(format!("{direction:?} is not up, down, left or right")),
    }
}

// The patterns are kept to one line each: the ledger's lint reads them so.
#[when(regex = r"^the user turns the wheel ([\d.]+) notch(?:es)? (\w+) at ([-\d.]+), ([-\d.]+)$")]
fn user_turns_wheel(w: &mut LjosWorld, n: f64, direction: String, x: f64, y: f64) -> StepResult {
    let (dx, dy) = wheel_delta(n, &direction)?;
    w.each(|b| b.wheel((x, y), WheelBy::Notches(dx, dy)))
}

#[when(regex = r"^the user scrolls ([\d.]+) pixels? (\w+) at ([-\d.]+), ([-\d.]+)$")]
fn user_scrolls(w: &mut LjosWorld, n: f64, direction: String, x: f64, y: f64) -> StepResult {
    let (dx, dy) = wheel_delta(n, &direction)?;
    w.each(|b| b.wheel((x, y), WheelBy::Pixels(dx, dy)))
}

#[when(regex = r#"^the input method composes "([^"]*)"$"#)]
fn input_method_composes(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.compose(&text))
}

#[when(regex = r#"^the input method commits "([^"]*)"$"#)]
fn input_method_commits(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.commit(&text))
}

#[when(regex = r#"^the file "([^"]+\.qml)" is changed to:$"#)]
fn file_changed(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    w.change_file(&name, Some(&docstring(step)?))
}

#[when(regex = r#"^the file "([^"]+\.qml)" is removed$"#)]
fn file_removed(w: &mut LjosWorld, name: String) -> StepResult {
    w.change_file(&name, None)
}

#[when("a reload happens")]
fn reload_happens(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.reload())
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

// For values an easing curve computes: the two toolkits agree to rounding, not bit for bit.
#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is within ([\d.]+) of (-?[\d.]+)$"#)]
fn property_within(
    w: &mut LjosWorld,
    property: String,
    object: String,
    tolerance: f64,
    expected: f64,
) -> StepResult {
    expect_value(w, &object, &property, |actual| {
        let n = value::number(actual).ok_or_else(|| format!("{actual} is not a number"))?;
        if (n - expected).abs() <= tolerance {
            Ok(())
        } else {
            Err(format!(
                "expected {expected} give or take {tolerance}, found {n}"
            ))
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

#[then(regex = r#"^the clipboard holds "([^"]*)"$"#)]
fn clipboard_holds(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| {
        let held = b.clipboard()?;
        if held == text {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the clipboard holds {held:?}, expected {text:?}"
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

#[then(regex = r#"^the reload error matches "([^"]+)"$"#)]
fn reload_error_matches(w: &mut LjosWorld, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| match b.reload_error()? {
        Some(e) if re.is_match(&e.message) => Ok(()),
        Some(e) => Err(BackendError::Failed(format!(
            "the reload error is {:?}, expected one matching {pattern:?}",
            e.message
        ))),
        None => Err(BackendError::Failed("there is no reload error".into())),
    })
}

#[then("there is no reload error")]
fn no_reload_error(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.reload_error()? {
        None => Ok(()),
        Some(e) => Err(BackendError::Failed(format!(
            "the reload error is {}:{}: {}",
            e.file, e.line, e.message
        ))),
    })
}

#[then(regex = r#"^the reload error is in "([^"]+)" at line (\d+)$"#)]
fn reload_error_is_in(w: &mut LjosWorld, file: String, line: u32) -> StepResult {
    w.each(|b| match b.reload_error()? {
        Some(e) if e.file == file && e.line == line => Ok(()),
        Some(e) => Err(BackendError::Failed(format!(
            "the reload error is in {:?} at line {}: {}",
            e.file, e.line, e.message
        ))),
        None => Err(BackendError::Failed("there is no reload error".into())),
    })
}

#[then(regex = r"^(\d+) reloads? ha(?:s|ve) happened$")]
fn reloads_happened(w: &mut LjosWorld, want: u64) -> StepResult {
    w.each(|b| {
        let count = b.reload_count()?;
        if count == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "{count} reload(s) happened, expected {want}"
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
