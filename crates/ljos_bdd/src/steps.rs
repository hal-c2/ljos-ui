//! The step catalogue. Every phrasing here is documented in `features/README.md`;
//! add a step in both places or `features:lint` will not know it.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;
use serde_json::Value;

use crate::backend::OutsideDrag;
use crate::backend::SystemDialogAnswer;
use crate::backend::SystemSetting;
use crate::backend::testrun::{TestRun, expected_lines};
use crate::{
    backend::{
        A11yAction, A11yNode, A11yTextAction, BackendError, LogLevel, OtherButton, PointerStep,
        WheelBy,
    },
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
        Some((first, rest)) if matches!(first.trim(), "qml" | "xml" | "js") => rest,
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

#[given(regex = r#"^the HTML "([^"]*)" was copied in another application$"#)]
fn clipboard_html_given(w: &mut LjosWorld, html: String) -> StepResult {
    w.each(|b| b.set_clipboard_html(&html))
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
#[given(regex = r#"^an? (\d+) by (\d+) image file "([^"]+)" filled with "(#[0-9a-fA-F]+)"$"#)]
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

// A PDF document beside the QML, with a page of each size in points
// ("300 by 200, 150 by 400"), each painted in its middle half.
#[given(regex = r#"^a PDF file "([^"]+)" of pages "([^"]+)" painted "(#[0-9a-fA-F]+)"$"#)]
fn a_pdf_file(w: &mut LjosWorld, name: String, pages: String, colour: String) -> StepResult {
    let pages = pages
        .split(',')
        .map(|page| {
            let (w, h) = page
                .split_once(" by ")
                .ok_or_else(|| format!("a page is \"W by H\", not {page:?}"))?;
            let side = |n: &str| n.trim().parse::<f64>().map_err(|e| format!("{n:?}: {e}"));
            Ok((side(w)?, side(h)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bytes = fixture::pdf(&pages, &colour)?;
    w.each(|b| b.resource(&name, &bytes))
}

// An animated GIF beside the QML, one colour a frame, each frame shown
// for the same time; the colours are a list ("#ff0000, #00ff00").
#[given(
    regex = r#"^an? (\d+) by (\d+) animated image file "([^"]+)" of frames "([^"]+)" each shown for (\d+) milliseconds$"#
)]
fn an_animated_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colours: String,
    delay: u32,
) -> StepResult {
    let colours: Vec<String> = colours.split(',').map(|c| c.trim().to_owned()).collect();
    let bytes = fixture::animated_gif(width, height, &colours, delay)?;
    w.each(|b| b.resource(&name, &bytes))
}

// The same, at `$SERVER/X.gif` on the scenario's web server.
#[given(
    regex = r#"^the web server serves an? (\d+) by (\d+) animated image file "([^"]+)" of frames "([^"]+)" each shown for (\d+) milliseconds$"#
)]
fn a_served_animated_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colours: String,
    delay: u32,
) -> StepResult {
    let colours: Vec<String> = colours.split(',').map(|c| c.trim().to_owned()).collect();
    let bytes = fixture::animated_gif(width, height, &colours, delay)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A file with an image's name and something else inside.
#[given(regex = r#"^a file "([^"]+)" that is not an image$"#)]
fn not_an_image(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.resource(&name, &fixture::not_an_image()))
}

// The same, at `$SERVER/X.png` on the scenario's web server.
#[given(
    regex = r#"^the web server serves an? (\d+) by (\d+) image file "([^"]+)" filled with "(#[0-9a-fA-F]+)"$"#
)]
fn a_served_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
    colour: String,
) -> StepResult {
    let bytes = fixture::image_file(&name, width, height, &colour)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A picture whose bytes are about as many as its pixels, as a
// photograph's are, at `$SERVER/X.png`.
#[given(regex = r#"^the web server serves an? (\d+) by (\d+) noisy image file "([^"]+)"$"#)]
fn a_served_noisy_image_file(
    w: &mut LjosWorld,
    width: u32,
    height: u32,
    name: String,
) -> StepResult {
    let bytes = fixture::noisy_png(width, height)?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

#[given(regex = r#"^the web server serves a file "([^"]+)" that is not an image$"#)]
fn served_not_an_image(w: &mut LjosWorld, name: String) -> StepResult {
    w.web()?.serve(&name, fixture::not_an_image());
    Ok(())
}

// A font file of features/text/fonts, beside the QML under its own name.
#[given(regex = r#"^the font file "([^"]+)"$"#)]
fn the_font_file(w: &mut LjosWorld, name: String) -> StepResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/text/fonts")
        .join(&name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    w.each(|b| b.resource(&name, &bytes))
}

// A sound or a film of features/multimedia/media, beside the QML under its
// own name.
#[given(regex = r#"^the media file "([^"]+)"$"#)]
fn the_media_file(w: &mut LjosWorld, name: String) -> StepResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/multimedia/media")
        .join(&name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    w.each(|b| b.resource(&name, &bytes))
}

// The same, at `$SERVER/X` on the scenario's web server.
#[given(regex = r#"^the web server serves the media file "([^"]+)"$"#)]
fn a_served_media_file(w: &mut LjosWorld, name: String) -> StepResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/multimedia/media")
        .join(&name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    w.web()?.serve(&name, bytes);
    Ok(())
}

// A sprite sheet of features/animation/sprites, beside the QML under its
// own name.
#[given(regex = r#"^the sprite sheet "([^"]+)"$"#)]
fn the_sprite_sheet(w: &mut LjosWorld, name: String) -> StepResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/animation/sprites")
        .join(&name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    w.each(|b| b.resource(&name, &bytes))
}

// A file with a font's name and something else inside.
#[given(regex = r#"^a file "([^"]+)" that is not a font$"#)]
fn not_a_font(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.resource(&name, &fixture::not_an_image()))
}

// A text file beside the QML (a settings file, say).
#[given(regex = r#"^the text file "([^"]+)":$"#)]
fn the_text_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.text_file(&name, &text))
}

// The same, at `$SERVER/X` on the scenario's web server.
#[given(regex = r#"^the web server serves the text file "([^"]+)":$"#)]
fn a_served_text_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.web()?.serve(&name, text.trim().as_bytes().to_vec());
    Ok(())
}

// A text file under the user's home directory.
#[given(regex = r#"^the user has the text file "([^"]+)":$"#)]
fn the_user_file(w: &mut LjosWorld, path: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.user_file(&path, &text))
}

#[given(regex = r#"^the system clock reads "([^"]+)"$"#)]
fn wall_clock_reads(w: &mut LjosWorld, time: String) -> StepResult {
    let ms = ljos_runtime::parse_rfc3339_ms(&time)
        .ok_or_else(|| format!("{time:?} is not an RFC 3339 time such as 2026-10-11T17:57:09Z"))?;
    w.each(|b| b.wall_clock(ms))
}

// On in a debug build already, unless `LJOS_TIER1_CHECK` says otherwise:
// the step makes a scenario that is about the check not depend on either.
#[given(regex = r"^the tier 1 cross-check is (on|off)$")]
fn tier1_check_is(w: &mut LjosWorld, on: String) -> StepResult {
    w.each(|b| b.tier1_check(on == "on"))
}

#[given(
    regex = r#"^the application uses the key bindings of "(Linux|GNOME|KDE Plasma|Windows|macOS)"$"#
)]
fn key_bindings(w: &mut LjosWorld, platform: String) -> StepResult {
    use ljos_quick::keys::Platform;
    let platform = match platform.as_str() {
        "GNOME" => Platform::Gnome,
        "KDE Plasma" => Platform::Kde,
        "Windows" => Platform::Windows,
        "macOS" => Platform::MacOs,
        _ => Platform::Linux,
    };
    w.each(|b| b.key_bindings(platform))
}

#[given("the application uses the fonts installed on the system")]
fn system_fonts(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.system_fonts())
}

#[given("the system cannot take its dialogs away")]
fn dialogs_stay(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.system_dialogs_stay())
}

// Before the QML: a model the application writes in Rust. The table has
// a `folder` and a `name` column; an empty folder is the top.
#[given(
    regex = r#"^a tree model "([^"]+)" that reads a folder's rows (\d+) ms after a view asks for them:$"#
)]
fn lazy_tree(w: &mut LjosWorld, name: String, delay: u64, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of folder and name".to_owned())?;
    let rows: Vec<(String, String)> = table
        .rows
        .iter()
        .skip(1)
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    w.each(|b| b.lazy_tree(&name, delay, &rows))
}

// Before the QML: a list model the application writes in Rust, read a
// page at a time.
#[given(
    regex = r#"^a list model "([^"]+)" of (\d+) rows that reads (\d+) at a time, (\d+) ms after a view asks for more$"#
)]
fn paged_model(
    w: &mut LjosWorld,
    name: String,
    total: usize,
    page: usize,
    delay: u64,
) -> StepResult {
    w.each(|b| b.paged_model(&name, total, page, delay))
}

// Before the QML: what the application reads at the start.
#[given("the system asks the user before it grants a permission")]
fn permissions_ask(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.permissions_ask())
}

#[given(regex = r"^the system has an? ([a-z ]+)$")]
fn system_has_sensor(w: &mut LjosWorld, kind: String) -> StepResult {
    w.each(|b| b.add_sensor(&kind))
}

// ----------------------------------------------------------------- When

#[when(regex = r"^the ([a-z ]+) reads (-?[0-9.]+(?:, -?[0-9.]+)*)$")]
fn sensor_reads(w: &mut LjosWorld, kind: String, values: String) -> StepResult {
    let values: Vec<f64> = values
        .split(", ")
        .map(|v| v.parse().map_err(|e| format!("{v:?}: {e}")))
        .collect::<Result<_, _>>()?;
    w.each(|b| b.sensor_reads(&kind, &values))
}

#[when(regex = r#"^the user chooses "([^"]+)" in the system dialog$"#)]
fn user_chooses(w: &mut LjosWorld, path: String) -> StepResult {
    w.each(|b| b.answer_system_dialog(SystemDialogAnswer::Choose(vec![path.clone()])))
}

#[when(regex = r#"^the user chooses the files "([^"]+)" in the system dialog$"#)]
fn user_chooses_files(w: &mut LjosWorld, paths: String) -> StepResult {
    let paths: Vec<String> = paths.split(", ").map(str::to_owned).collect();
    w.each(|b| b.answer_system_dialog(SystemDialogAnswer::Choose(paths.clone())))
}

#[when(regex = r#"^the user answers "([^"]+)" in the system dialog$"#)]
fn user_answers(w: &mut LjosWorld, button: String) -> StepResult {
    w.each(|b| b.answer_system_dialog(SystemDialogAnswer::Button(button.clone())))
}

#[when("the user dismisses the system dialog")]
fn user_dismisses(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.answer_system_dialog(SystemDialogAnswer::Dismiss))
}

// The application is not told; it finds out when it asks.
#[when(regex = r#"^"([^"]*)" is copied in another application$"#)]
fn copied_elsewhere(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.copy_elsewhere(&text))
}

// A drag from another application, step by step as the window system
// tells it.
#[when(regex = r#"^another application drags the (URL|text) "([^"]*)" over ([-\d.]+), ([-\d.]+)$"#)]
fn outside_drag_one(w: &mut LjosWorld, what: String, data: String, x: f64, y: f64) -> StepResult {
    let format = if what == "URL" {
        "text/uri-list"
    } else {
        "text/plain"
    };
    let formats = vec![(format.to_owned(), data)];
    w.each(|b| {
        b.drag_from_outside(&OutsideDrag::Enter {
            formats: formats.clone(),
            at: (x, y),
        })
    })
}

// The table has a `format` and a `data` column.
#[when(regex = r#"^another application drags this over ([-\d.]+), ([-\d.]+):$"#)]
fn outside_drag_formats(w: &mut LjosWorld, x: f64, y: f64, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of format and data".to_owned())?;
    let formats: Vec<(String, String)> = table
        .rows
        .iter()
        .skip(1)
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    w.each(|b| {
        b.drag_from_outside(&OutsideDrag::Enter {
            formats: formats.clone(),
            at: (x, y),
        })
    })
}

#[when(regex = r#"^the drag from another application moves to ([-\d.]+), ([-\d.]+)$"#)]
fn outside_drag_moves(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.drag_from_outside(&OutsideDrag::Move((x, y))))
}

#[when("the drag from another application leaves the window")]
fn outside_drag_leaves(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.drag_from_outside(&OutsideDrag::Leave))
}

#[when("the drag from another application is let go")]
fn outside_drag_dropped(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.drag_from_outside(&OutsideDrag::Drop))
}

// What the application told the window system at the last step: whether
// a drop target under the drag takes it, or took the drop.
#[then(regex = r#"^the drag from another application is (taken|not taken)$"#)]
fn outside_drag_taken(w: &mut LjosWorld, which: String) -> StepResult {
    let want = which == "taken";
    w.each(|b| {
        if b.outside_drag_taken()? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the drag from another application is {}taken",
                if want { "not " } else { "" }
            )))
        }
    })
}

#[when(regex = r#"^the user (grants|denies) the "([^"]+)" permission$"#)]
fn user_answers_permission(w: &mut LjosWorld, how: String, name: String) -> StepResult {
    w.each(|b| b.answer_permission(&name, how == "grants"))
}

#[when(regex = r#"^the UI language is set to "([^"]*)"$"#)]
fn ui_language_set(w: &mut LjosWorld, language: String) -> StepResult {
    w.each(|b| b.set_ui_language(&language))
}

// The table has an attribute and its value on each row.
#[when("the application font is set to:")]
fn application_font_set(w: &mut LjosWorld, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of attributes and values".to_owned())?;
    let attrs: Vec<(String, String)> = table
        .rows
        .iter()
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    w.each(|b| b.set_application_font(Some(&attrs)))
}

#[when("the application font is set back to the default")]
fn application_font_reset(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.set_application_font(None))
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

// What the system prefers, as a scenario writes it.
fn system_setting(setting: &str, value: &str) -> Result<SystemSetting, String> {
    let flag = || match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{value} is not true or false")),
    };
    let word = value.trim_matches('"');
    match (setting, word) {
        ("color scheme", "dark") => Ok(SystemSetting::DarkScheme(true)),
        ("color scheme", "light") => Ok(SystemSetting::DarkScheme(false)),
        ("contrast", "high") => Ok(SystemSetting::HighContrast(true)),
        ("contrast", "normal") => Ok(SystemSetting::HighContrast(false)),
        ("reduced motion", _) => flag().map(SystemSetting::ReducedMotion),
        ("screen reader", _) => flag().map(SystemSetting::ScreenReader),
        ("font scale", _) => value
            .parse()
            .map(SystemSetting::FontScale)
            .map_err(|_| format!("{value} is not a number")),
        _ => Err(format!("{value} is not a {setting:?} the system has")),
    }
}

/// `LANG=de_DE.UTF-8 LC_NUMERIC=fr_FR.UTF-8`: the program's locale
/// variables, and no others; empty for none at all.
#[given(regex = r#"^the application starts with the locale environment "([^"]*)"$"#)]
fn locale_environment(w: &mut LjosWorld, vars: String) -> StepResult {
    let vars: Vec<(String, String)> = vars
        .split_whitespace()
        .map(|kv| {
            kv.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .ok_or_else(|| format!("{kv} is not NAME=value"))
        })
        .collect::<Result<_, _>>()?;
    w.each(|b| b.locale_environment(&vars))
}

/// The text of the file `QT_QUICK_CONTROLS_CONF` names.
#[given(regex = r#"^the application starts with the controls configuration:$"#)]
fn controls_configuration(w: &mut LjosWorld, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.controls_configuration(&text))
}

#[given(regex = r#"^the application starts with "(QT_QUICK_CONTROLS_[A-Z_]+)" set to "([^"]*)"$"#)]
fn controls_variable(w: &mut LjosWorld, name: String, value: String) -> StepResult {
    w.each(|b| b.controls_variable(&name, &value))
}

#[given(regex = r#"^the application starts with the controls style "([^"]+)"$"#)]
fn controls_style(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.controls_style(&name))
}

#[given(regex = r#"^the application starts with "-style ([^"]+)" on its command line$"#)]
fn command_line_style(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.command_line_style(&name))
}

#[given(regex = r#"^the system setting "([^"]+)" is (.+)$"#)]
fn system_setting_is(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.system_setting(setting))
}

#[when(regex = r#"^the system setting "([^"]+)" changes to (.+)$"#)]
fn system_setting_changes(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.system_setting(setting))
}

#[when(regex = r#"^the application sets the system setting "([^"]+)" to (.+)$"#)]
fn app_setting(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.app_setting(setting))
}

// What the system says of its network, as a scenario writes it: how far it
// reaches, then what carries it and anything else it says, comma-separated.
fn network_status(said: &str) -> Result<ljos_platform::NetworkStatus, String> {
    use ljos_platform::{Reachability as R, TransportMedium as M};
    let mut parts = said.split(',').map(str::trim);
    let mut status = ljos_platform::NetworkStatus {
        reachability: match parts.next().unwrap_or("") {
            "unknown" => R::Unknown,
            "disconnected" => R::Disconnected,
            "local" => R::Local,
            "site" => R::Site,
            "online" => R::Online,
            other => return Err(format!("{other:?} is not a reachability")),
        },
        ..Default::default()
    };
    for part in parts {
        match part {
            "ethernet" => status.transport_medium = M::Ethernet,
            "cellular" => status.transport_medium = M::Cellular,
            "wifi" => status.transport_medium = M::WiFi,
            "bluetooth" => status.transport_medium = M::Bluetooth,
            "behind a captive portal" => status.behind_captive_portal = true,
            "metered" => status.metered = true,
            other => return Err(format!("{other:?} is not something a network is")),
        }
    }
    Ok(status)
}

#[given(regex = r#"^the system's network is "([^"]+)"$"#)]
fn network_is(w: &mut LjosWorld, said: String) -> StepResult {
    let status = network_status(&said)?;
    w.each(|b| b.network(status))
}

#[when(regex = r#"^the system's network changes to "([^"]+)"$"#)]
fn network_changes(w: &mut LjosWorld, said: String) -> StepResult {
    let status = network_status(&said)?;
    w.each(|b| b.network(status))
}

#[given("the window system brings no files dragged in from other applications")]
fn no_external_drops_given(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.no_external_drops())
}

#[given(regex = r#"^the desktop setting "([^"]+)" "([^"]+)" is (.+)$"#)]
fn desktop_setting_is(
    w: &mut LjosWorld,
    namespace: String,
    key: String,
    value: String,
) -> StepResult {
    let value = setting_value(&value)?;
    w.each(|b| b.desktop_setting(&namespace, &key, value.clone()))
}

#[when(regex = r#"^the desktop setting "([^"]+)" "([^"]+)" changes to (.+)$"#)]
fn desktop_setting_changes(
    w: &mut LjosWorld,
    namespace: String,
    key: String,
    value: String,
) -> StepResult {
    let value = setting_value(&value)?;
    w.each(|b| b.desktop_setting(&namespace, &key, value.clone()))
}

/// A value as the portal sends it: `true`, `uint32 1`, `1.25` (a double),
/// `48` (an int32), `"reduce"`.
fn setting_value(value: &str) -> Result<ljos_a11y::SettingValue, String> {
    use ljos_a11y::SettingValue as V;
    let bad = || format!("{value} is not a setting's value");
    let value = value.trim();
    Ok(match value {
        "true" => V::Bool(true),
        "false" => V::Bool(false),
        _ if value.starts_with('"') => V::Text(
            value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(bad)?
                .to_owned(),
        ),
        _ => match value.strip_prefix("uint32 ") {
            Some(n) => V::Uint(n.parse().map_err(|_| bad())?),
            None if value.contains('.') => V::Double(value.parse().map_err(|_| bad())?),
            None => V::Int(value.parse().map_err(|_| bad())?),
        },
    })
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

// A context menu, a link opened in the background: what the other
// buttons are for.
#[when(
    regex = r"^the user clicks the (right|middle) button at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$"
)]
fn user_clicks_button(w: &mut LjosWorld, button: String, x: f64, y: f64) -> StepResult {
    let button = if button == "right" {
        OtherButton::Right
    } else {
        OtherButton::Middle
    };
    w.each(|b| b.click_button(button, x, y))
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

#[when(regex = r#"^the file "([^"]+\.(?:qml|m?js))" is changed to:$"#)]
fn file_changed(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    w.change_file(&name, Some(&docstring(step)?))
}

#[when(regex = r#"^the file "([^"]+\.(?:qml|m?js))" is saved and closed as:$"#)]
fn file_saved_whole(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    w.save_file_whole(&name, &docstring(step)?)
}

#[when(regex = r#"^the file "([^"]+\.(?:qml|m?js))" is removed$"#)]
fn file_removed(w: &mut LjosWorld, name: String) -> StepResult {
    w.change_file(&name, None)
}

#[when(regex = r#"^the file "([^"]+\.qml)" is emptied by a save that has not finished$"#)]
fn file_emptied(w: &mut LjosWorld, name: String) -> StepResult {
    w.empty_file(&name)
}

// A picture beside the QML saved again while the application runs.
#[when(
    regex = r#"^the image file "([^"]+)" is saved as an? (\d+) by (\d+) image filled with "(#[0-9a-fA-F]+)"$"#
)]
fn image_file_saved(
    w: &mut LjosWorld,
    name: String,
    width: u32,
    height: u32,
    colour: String,
) -> StepResult {
    let bytes = fixture::image_file(&name, width, height, &colour)?;
    w.each(|b| b.change_resource(&name, &bytes))
}

#[when(regex = r#"^the image file "([^"]+)" is saved as a file that is not an image$"#)]
fn image_file_saved_broken(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.change_resource(&name, &fixture::not_an_image()))
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

#[when(regex = r#"^the file "([^"]+\.qml)" is opened in a window of its own$"#)]
fn file_opened_in_window(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.open_window(&name))
}

#[when(regex = r#"^the application puts an object of "([^"]+\.qml)" in the window of "([^"]+)"$"#)]
fn put_in_window(w: &mut LjosWorld, name: String, old: String) -> StepResult {
    w.each(|b| b.put_in_window(&name, &old))
}

#[then(
    regex = r#"^the application cannot put an object of "([^"]+\.qml)" in the window of "([^"]+)"$"#
)]
fn cannot_put_in_window(w: &mut LjosWorld, name: String, old: String) -> StepResult {
    w.each(|b| match b.put_in_window(&name, &old) {
        Ok(()) => Err(BackendError::Failed(format!(
            "an object of {name} was put in the window of {old:?}"
        ))),
        Err(BackendError::Failed(_)) => Ok(()),
        Err(e) => Err(e),
    })
}

fn list(text: &str) -> Vec<String> {
    text.split(',').map(|s| s.trim().to_owned()).collect()
}

#[when(regex = r#"^the application watches the roots "([^"]+)" for the entries "([^"]+)"$"#)]
fn watches_roots(w: &mut LjosWorld, roots: String, entries: String) -> StepResult {
    let (roots, entries) = (list(&roots), list(&entries));
    w.each(|b| b.watch_roots(&roots, &entries))
}

#[when(regex = r#"^the file "([^"]+)" is saved on disk as:$"#)]
fn saved_on_disk(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.save_on_disk(&name, Some(&text)))
}

#[when(regex = r#"^the file "([^"]+)" is removed from disk$"#)]
fn removed_from_disk(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.save_on_disk(&name, None))
}

#[when(
    regex = r#"^the file "([^"]+\.qml)" is opened in a window of its own, given a Document titled "([^"]*)"$"#
)]
fn file_opened_in_window_given(w: &mut LjosWorld, name: String, title: String) -> StepResult {
    w.each(|b| b.open_window_given(&name, &title))
}

#[then(regex = r#"^the Document the application gave is titled "([^"]*)"$"#)]
fn given_document_titled(w: &mut LjosWorld, title: String) -> StepResult {
    w.each(|b| {
        let now = b.given_title()?;
        if now == title {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the Document is titled {now:?}, not {title:?}"
            )))
        }
    })
}

#[when(regex = r#"^the object "([^"]+)" is destroyed$"#)]
fn object_destroyed(w: &mut LjosWorld, object: String) -> StepResult {
    w.each(|b| {
        if b.call(&object, "destroy", &[]).is_err() {
            b.bind(&object, "visible", "(destroy(), false)")?;
        }
        b.advance(16)?;
        if b.exists(&object)? {
            Err(BackendError::Failed(format!(
                "object {object:?} was not destroyed"
            )))
        } else {
            Ok(())
        }
    })
}

#[then(regex = r#"^the signal "([^"]+)" of "([^"]+)" has fired once$"#)]
fn signal_fired_once(w: &mut LjosWorld, signal: String, object: String) -> StepResult {
    let candidates = [
        format!("{signal}Count"),
        format!("{signal}Fired"),
        format!("{signal}_count"),
        format!("{signal}_fired"),
        signal.clone(),
    ];
    w.each(|b| {
        let mut count = None;
        for prop in &candidates {
            if let Ok(v) = b.get(&object, prop)
                && let Some(n) = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
            {
                count = Some(n);
                break;
            }
        }
        if count == Some(1) {
            Ok(())
        } else if let Some(n) = count {
            Err(BackendError::Failed(format!(
                "signal {signal:?} of {object:?} has fired {n} times, expected 1"
            )))
        } else {
            Err(BackendError::Failed(format!(
                "cannot determine if signal {signal:?} of {object:?} fired (checked properties: {candidates:?})"
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

#[then(regex = r#"^the log has an info matching "([^"]+)"$"#)]
fn log_has_info(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Info, true)
}

#[then(regex = r#"^the log has no info matching "([^"]+)"$"#)]
fn log_has_no_info(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Info, false)
}

#[then(regex = r#"^the log has (\d+) (infos|warnings) matching "([^"]+)"$"#)]
fn log_has_infos(w: &mut LjosWorld, count: usize, kind: String, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    let level = if kind == "infos" {
        LogLevel::Info
    } else {
        LogLevel::Warning
    };
    w.each(|b| {
        let log = b.log()?;
        let hits = log
            .iter()
            .filter(|e| e.level == level && re.is_match(&e.message))
            .count();
        if hits == count {
            return Ok(());
        }
        let seen: Vec<String> = log
            .iter()
            .map(|e| format!("  {}: {}", e.level, e.message))
            .collect();
        Err(BackendError::Failed(format!(
            "expected {count} {kind} matching {pattern:?}, found {hits}; log was:\n{}",
            seen.join("\n")
        )))
    })
}

#[then(regex = r#"^the log has no warning matching "([^"]+)"$"#)]
fn log_has_no_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Warning, false)
}

#[then("the log has no warnings")]
fn log_clean(w: &mut LjosWorld) -> StepResult {
    log_matches(w, ".*", LogLevel::Warning, false)
}

// ------------------------------------------------------------- pixels

#[then(regex = r#"^the pixel at (\d+), (\d+) of the window is "(#\w{6})"$"#)]
fn window_pixel(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    window_pixel_near(w, x, y, 0, &colour)
}

#[then(regex = r#"^the pixel at (\d+), (\d+) of the window is within (\d+) of "(#\w{6})"$"#)]
fn window_pixel_within(
    w: &mut LjosWorld,
    x: u32,
    y: u32,
    tolerance: u8,
    colour: String,
) -> StepResult {
    window_pixel_near(w, x, y, tolerance, &colour)
}

/// One pixel of the window's picture, each channel give or take `tolerance`.
///
/// The picture is laid over black first: where nothing is painted Qt's
/// picture is black and Ljós's is transparent, and a half transparent pixel
/// must read the same on both.
fn window_pixel_near(w: &mut LjosWorld, x: u32, y: u32, tolerance: u8, colour: &str) -> StepResult {
    let wanted = u32::from_str_radix(&colour[1..], 16)
        .map_err(|e| format!("{colour} is not a colour as #rrggbb: {e}"))?;
    let wanted = [(wanted >> 16) as u8, (wanted >> 8) as u8, wanted as u8];
    w.each(|b| {
        let png = b.grab()?;
        let picture = ljos_render::RgbaImage::from_png(&png)
            .map_err(|e| BackendError::Failed(format!("the picture of the window: {e}")))?;
        if x >= picture.width || y >= picture.height {
            return Err(BackendError::Failed(format!(
                "{x}, {y} is outside the window's picture, which is {} by {}",
                picture.width, picture.height
            )));
        }
        let [r, g, b, a] = picture.pixel(x, y);
        let over_black = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        let seen = [over_black(r), over_black(g), over_black(b)];
        if seen
            .iter()
            .zip(wanted)
            .all(|(s, w)| s.abs_diff(w) <= tolerance)
        {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the pixel at {x}, {y} is #{:02x}{:02x}{:02x}, not {colour}",
                seen[0], seen[1], seen[2]
            )))
        }
    })
}

// ------------------------------------------------------------- faults
//
// Faults and the boundaries that contain them are Ljós's own; the oracle
// has nothing to compare with.

#[when(regex = r#"^a native handler of "([^"]+)" panics with "([^"]*)"$"#)]
fn native_handler_panics(w: &mut LjosWorld, object: String, message: String) -> StepResult {
    w.each(|b| b.panic_in(&object, &message))
}

#[when("the window's passes are counted")]
fn passes_are_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.count_passes())
}

#[then(regex = r"^the window ran at most (\d+) pass(?:es)? since they were counted$")]
fn ran_at_most_passes(w: &mut LjosWorld, most: u64) -> StepResult {
    w.each(|b| {
        let ran = b.passes_counted()?;
        if ran <= most {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the window ran {ran} passes, not at most {most}"
            )))
        }
    })
}

#[when("the times text is measured are counted")]
fn text_measures_are_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.count_text_measures())
}

#[then(regex = r"^text was measured at most (\d+) times? since that was counted$")]
fn text_measured_at_most(w: &mut LjosWorld, most: u64) -> StepResult {
    w.each(|b| {
        let measured = b.text_measures_counted()?;
        if measured <= most {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "text was measured {measured} times, not at most {most}"
            )))
        }
    })
}

#[when("the wakes of the application's threads are counted")]
fn wakes_are_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.count_wakes())
}

#[then("no thread but the window's woke since they were counted")]
fn no_other_thread_woke(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let woke = b.wakes_counted()?;
        if woke.is_empty() {
            Ok(())
        } else {
            let said: Vec<String> = woke.iter().map(|(t, n)| format!("{t} {n} times")).collect();
            Err(BackendError::Failed(format!("woke: {}", said.join(", "))))
        }
    })
}

#[when("the application is observed")]
fn application_observed(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.observe())
}

#[when("the work of the application and its observer is counted")]
fn work_is_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.count_work())
}

#[then("the application and its observer did no work since it was counted")]
fn did_no_work(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let did = b.work_counted()?;
        if did.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(did.join("; ")))
        }
    })
}

// Nothing would wake the application: 0 % CPU when idle.
#[then("the application has nothing scheduled")]
fn nothing_scheduled(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let what = b.scheduled()?;
        if what.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the application has scheduled {}",
                what.join(", ")
            )))
        }
    })
}

#[then("the application has no fault")]
fn no_application_fault(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.application_fault()? {
        None => Ok(()),
        Some(f) => Err(BackendError::Failed(format!(
            "the application has a {} fault: {}",
            f.kind, f.message
        ))),
    })
}

#[then(regex = r#"^the application fault is an? "([^"]+)" matching "([^"]+)"$"#)]
fn application_fault_is(w: &mut LjosWorld, kind: String, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| match b.application_fault()? {
        Some(f) if f.kind == kind && re.is_match(&f.message) => Ok(()),
        Some(f) => Err(BackendError::Failed(format!(
            "the application fault is a {:?}: {:?}; expected a {kind:?} matching {pattern:?}",
            f.kind, f.message
        ))),
        None => Err(BackendError::Failed("the application has no fault".into())),
    })
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

#[then(
    regex = r#"^the window system is asked for (no|a (?:(light|dark) )?(blurred|glass)) backdrop behind "([^"]+)"$"#
)]
fn window_backdrop(
    w: &mut LjosWorld,
    which: String,
    look: String,
    kind: String,
    object: String,
) -> StepResult {
    w.each(|b| {
        let asked = b.window_backdrop(&object)?;
        // A scenario that names no look leaves it open.
        let same = match &asked {
            None => which == "no",
            Some(a) => a.kind == kind && (look.is_empty() || a.look == look),
        };
        if same {
            Ok(())
        } else {
            let seen = asked.map_or("no".to_owned(), |a| format!("a {} {}", a.look, a.kind));
            Err(BackendError::Failed(format!(
                "the window system is asked for {seen} backdrop behind \"{object}\", not {which}"
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

#[then(regex = r#"^the text file "([^"]+)" has the line "([^"]*)"$"#)]
fn text_file_has_line(w: &mut LjosWorld, name: String, line: String) -> StepResult {
    w.each(|b| match b.read_text_file(&name)? {
        Some(text) if text.lines().any(|l| l == line) => Ok(()),
        Some(text) => Err(BackendError::Failed(format!(
            "{name} has no line {line:?}; it holds:\n{text}"
        ))),
        None => Err(BackendError::Failed(format!("there is no file {name}"))),
    })
}

#[then(regex = r#"^the text file "([^"]+)" does not exist$"#)]
fn text_file_absent(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| match b.read_text_file(&name)? {
        None => Ok(()),
        Some(text) => Err(BackendError::Failed(format!("{name} exists:\n{text}"))),
    })
}

#[then(regex = r#"^the system shows an? "([^"]+)" dialog titled "([^"]*)"$"#)]
fn system_shows(w: &mut LjosWorld, kind: String, title: String) -> StepResult {
    w.each(|b| {
        let shown = b.system_dialogs()?;
        if shown.iter().any(|(k, t)| *k == kind && *t == title) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system shows {shown:?}, not a {kind:?} dialog titled {title:?}"
            )))
        }
    })
}

#[then("the system shows no dialog")]
fn system_shows_none(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let shown = b.system_dialogs()?;
        if shown.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!("the system shows {shown:?}")))
        }
    })
}

#[then(regex = r#"^the system was asked to open "([^"]*)"$"#)]
fn system_opened(w: &mut LjosWorld, url: String) -> StepResult {
    w.each(|b| {
        let opened = b.opened_urls()?;
        if opened.contains(&url) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system was asked to open {opened:?}, not {url:?}"
            )))
        }
    })
}

#[then(regex = r#"^the system asks the user for the "([^"]+)" permission$"#)]
fn system_asks_permission(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| {
        let asked = b.permission_requests()?;
        if asked.contains(&name) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system asks for {asked:?}, not the {name:?} permission"
            )))
        }
    })
}

#[then("the system asks the user for no permission")]
fn system_asks_no_permission(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let asked = b.permission_requests()?;
        if asked.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system asks for {asked:?}"
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

#[when(regex = r"^the application is opened again$")]
fn reopened(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.reopen())
}

#[then(regex = r"^the application was (not )?asked to quit$")]
fn asked_to_quit(w: &mut LjosWorld, not: String) -> StepResult {
    let want = not.is_empty();
    w.each(|b| match b.quit_requested()? {
        asked if asked == want => Ok(()),
        true => Err(BackendError::Failed(
            "the application was asked to quit".into(),
        )),
        false => Err(BackendError::Failed(
            "the application was not asked to quit".into(),
        )),
    })
}

#[when(regex = r"^the window shows its first frame$")]
fn first_frame(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.first_frame())
}

#[then(regex = r#"^no picture is kept for "([^"]+)" yet$"#)]
fn no_picture_kept(w: &mut LjosWorld, object: String) -> StepResult {
    w.each(|b| match b.kept_picture(&object)? {
        None => Ok(()),
        Some((w, h)) => Err(BackendError::Failed(format!(
            "the picture of {object:?} is kept at {w} by {h} pixels already"
        ))),
    })
}

#[then(regex = r#"^the picture of "([^"]+)" is kept at (\d+) by (\d+) pixels$"#)]
fn picture_kept_at(w: &mut LjosWorld, object: String, width: u32, height: u32) -> StepResult {
    w.each(|b| match b.kept_picture(&object)? {
        Some(size) if size == (width, height) => Ok(()),
        Some((w, h)) => Err(BackendError::Failed(format!(
            "the picture of {object:?} is kept at {w} by {h} pixels, expected {width} by {height}"
        ))),
        None => Err(BackendError::Failed(format!(
            "no picture is kept for {object:?}"
        ))),
    })
}

#[then(regex = r#"^the pointer shows the "([A-Za-z]+Cursor)" cursor$"#)]
fn pointer_cursor(w: &mut LjosWorld, name: String) -> StepResult {
    let want = ljos_engine::qt_namespace()
        .into_iter()
        .flat_map(|e| e.keys)
        .find_map(|(key, value)| (key == name).then_some(value))
        .ok_or_else(|| format!("Qt.{name} is not a cursor shape"))?;
    w.each(|b| match b.pointer_cursor()? {
        shape if shape == want => Ok(()),
        shape => Err(BackendError::Failed(format!(
            "the pointer shows cursor {shape}, expected Qt.{name} ({want})"
        ))),
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

// ------------------------------------------------- QML tests (QtTest)
//
// These steps run the staged `tst_*.qml` files with a test runner and read
// what it printed. Nothing is loaded as an application for them.

#[when("the tests are run")]
fn tests_run(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun::default())
}

#[when(regex = r#"^the tests are run with the import path "([^"]+)"$"#)]
fn tests_run_with_import(w: &mut LjosWorld, dir: String) -> StepResult {
    w.run_tests(&TestRun {
        import_paths: vec![dir],
        ..TestRun::default()
    })
}

// `TZ`, as a program is started in a zone: an IANA name (`Europe/Berlin`)
// or a POSIX rule.
#[when(regex = r#"^the tests are run in the time zone "([^"]+)"$"#)]
fn tests_run_in_zone(w: &mut LjosWorld, zone: String) -> StepResult {
    w.run_tests(&TestRun {
        time_zone: Some(zone),
        ..TestRun::default()
    })
}

#[when(regex = r#"^the tests matching "([^"]+)" are run$"#)]
fn tests_run_matching(w: &mut LjosWorld, filter: String) -> StepResult {
    w.run_tests(&TestRun {
        filters: vec![filter],
        ..TestRun::default()
    })
}

// `ljos check` reads the staged files as one project; what it says is kept
// as a report, like a test runner's.
#[when("the files are checked")]
fn files_checked(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        ..TestRun::default()
    })
}

#[then("the check reports:")]
fn check_reports(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.lines == want {
            Ok(())
        } else {
            Err(format!("the check reports\n{}", report.text()))
        }
    })
}

#[then("the test report is:")]
fn test_report_is(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.lines == want {
            Ok(())
        } else {
            Err(format!("the report is\n{}", report.text()))
        }
    })
}

#[then("the test report has the lines:")]
fn test_report_has(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.has_lines(&want) {
            Ok(())
        } else {
            Err(format!(
                "these lines are not in the report, which is\n{}",
                report.text()
            ))
        }
    })
}

#[then(regex = r"^the test run exits with status (\d+)$")]
fn test_run_status(w: &mut LjosWorld, want: i32) -> StepResult {
    w.each_test_report(|report| {
        if report.status == want {
            Ok(())
        } else {
            Err(format!(
                "the run exited with status {}; the report is\n{}",
                report.status,
                report.text()
            ))
        }
    })
}

// The machine-readable report, by JSON pointer (`/totals/failed`).
#[then(regex = r#"^the test results' "([^"]*)" is (.+)$"#)]
fn test_results_value(w: &mut LjosWorld, pointer: String, want: String) -> StepResult {
    let want = value::parse(&want)?;
    w.each_test_report(|report| {
        let json = report
            .json
            .as_ref()
            .ok_or("this runner writes no machine-readable report")?;
        match json.pointer(&pointer) {
            Some(actual) if value::same(&want, actual) => Ok(()),
            Some(actual) => Err(format!("{pointer} is {actual}, expected {want}")),
            None => Err(format!("the results have no {pointer}: {json}")),
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

// ------------------------------------------------------------ Inspector
//
// The agent's side of ARCHITECTURE.md §19.4 (features/tooling/mcp). The
// attributes stay on one line each: `features:lint` reads them from here.

use crate::backend::inspector::{self, Ask, Seen, Token};

fn inspector_ask(w: &mut LjosWorld, ask: Ask) -> StepResult {
    // An agent of the authoring tools has no application: it asks the
    // tools, and the same steps read what they answer.
    let (author, files) = w.author();
    if author.connected() {
        return author.ask(files, &ask);
    }
    w.each(|b| b.inspector(&ask))
}

fn inspector_call(w: &mut LjosWorld, tool: &str, args: Value) -> StepResult {
    inspector_ask(
        w,
        Ask::Call {
            tool: tool.to_owned(),
            args,
        },
    )
}

fn inspector_seen(w: &mut LjosWorld, check: impl Fn(&Seen) -> Result<(), String>) -> StepResult {
    let (author, _) = w.author();
    if author.connected() {
        return check(author.seen()?);
    }
    w.each(|b| check(&b.inspector_seen()?).map_err(BackendError::Failed))
}

/// A `"""text` or `"""json` doc string as it was meant.
fn inspector_doc(step: &Step) -> Result<String, String> {
    let text = docstring(step)?;
    let text = match text.split_once('\n') {
        Some((first, rest)) if matches!(first.trim(), "text" | "json") => rest,
        _ => &text,
    };
    Ok(inspector::dedent(text))
}

/// A step's quoted text with `\"` for a quote in it. Other backslashes
/// are the text's own (a pattern's `\d`).
fn unquoted(text: &str) -> String {
    text.replace("\\\"", "\"")
}

fn inspector_token(words: &str) -> Result<Token, String> {
    match words {
        "the right token" => Ok(Token::Right),
        "no token" => Ok(Token::Missing),
        "a wrong token" => Ok(Token::Wrong),
        "the token of an older start" => Ok(Token::OlderStart),
        other => Err(format!("{other:?} is not a token this step knows")),
    }
}

#[given("the inspector is connected")]
fn inspector_connected(w: &mut LjosWorld) -> StepResult {
    inspector_ask(
        w,
        Ask::Connect {
            caps: None,
            redact_all: false,
        },
    )
}

#[given("the inspector is connected and masks every editable value")]
fn inspector_connected_masking(w: &mut LjosWorld) -> StepResult {
    inspector_ask(
        w,
        Ask::Connect {
            caps: None,
            redact_all: true,
        },
    )
}

#[given(regex = r#"^the inspector is connected with the capabilities "([^"]*)"$"#)]
fn inspector_connected_with(w: &mut LjosWorld, caps: String) -> StepResult {
    let caps = Some(caps);
    inspector_ask(
        w,
        Ask::Connect {
            caps,
            redact_all: false,
        },
    )
}

#[rustfmt::skip]
#[given(regex = r#"^the inspector is connected with the capabilities "([^"]*)" and masks every editable value$"#)]
fn inspector_connected_with_masking(w: &mut LjosWorld, caps: String) -> StepResult {
    let caps = Some(caps);
    inspector_ask(
        w,
        Ask::Connect {
            caps,
            redact_all: true,
        },
    )
}

#[given("the files are watched")]
fn inspector_files_watched(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Watched)
}

#[given(regex = r"^the host says the application has (\d+) windows?$")]
fn inspector_windows(w: &mut LjosWorld, count: usize) -> StepResult {
    inspector_ask(w, Ask::Windows(count))
}

#[given("the inspector is served on a socket")]
fn inspector_served_on_socket(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ServeOnSocket)
}

#[when("the inspector is stopped")]
fn inspector_stopped(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Stop)
}

#[rustfmt::skip]
#[when(regex = r#"^the inspector is asked to listen on "([^"]+)" (with|without) the word for a network$"#)]
fn inspector_listens(w: &mut LjosWorld, address: String, word: String) -> StepResult {
    let lan = word == "with";
    inspector_ask(w, Ask::Listen { address, lan })
}

#[rustfmt::skip]
#[when(regex = r"^the inspector is asked to listen on a socket whose path is (\d+) characters long$")]
fn inspector_listens_on_long_socket(w: &mut LjosWorld, len: usize) -> StepResult {
    inspector_ask(w, Ask::ListenOnSocket { path_len: Some(len) })
}

#[when("the inspector is asked to listen on a socket whose path is as long as the system allows")]
fn inspector_listens_on_longest_socket(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ListenOnSocket { path_len: None })
}

#[given("the inspector is served on the loopback address")]
fn inspector_served(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Serve)
}

#[given("the agent has requested a snapshot")]
#[when("the agent requests a snapshot")]
fn snapshot_requested(w: &mut LjosWorld) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({}))
}

#[given(regex = r"^the agent has requested a snapshot of at most (\d+) nodes$")]
#[when(regex = r"^the agent requests a snapshot of at most (\d+) nodes$")]
fn snapshot_requested_capped(w: &mut LjosWorld, nodes: u64) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({ "max_nodes": nodes }))
}

#[when("the agent requests the next part of the snapshot")]
fn snapshot_next_part(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::NextPart)
}

#[when(regex = r#"^the agent requests a snapshot below "(.*)"$"#)]
fn snapshot_below(w: &mut LjosWorld, root: String) -> StepResult {
    inspector_call(
        w,
        "app_snapshot",
        serde_json::json!({ "root": unquoted(&root) }),
    )
}

#[when(regex = r"^the agent requests a snapshot (\d+) levels? deep$")]
fn snapshot_deep(w: &mut LjosWorld, depth: u64) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({ "depth": depth }))
}

#[when("the agent requests what changed since the last snapshot")]
fn snapshot_changes(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ChangedSince)
}

// The locator is everything between the outer quotes, so that a table of
// examples can give `role=button[name="Save"]` as it is typed.
#[given(regex = r#"^the agent has searched for "(.*)"$"#)]
#[when(regex = r#"^the agent searches for "(.*)"$"#)]
fn agent_searches(w: &mut LjosWorld, locator: String) -> StepResult {
    inspector_call(
        w,
        "app_find",
        serde_json::json!({ "locator": unquoted(&locator) }),
    )
}

#[when(regex = r#"^the agent inspects "(.*)"$"#)]
fn agent_inspects(w: &mut LjosWorld, target: String) -> StepResult {
    inspector_call(
        w,
        "app_inspect",
        serde_json::json!({ "target": unquoted(&target) }),
    )
}

#[when(regex = r#"^the agent inspects every property of "(.*)"$"#)]
fn agent_inspects_all(w: &mut LjosWorld, target: String) -> StepResult {
    let args = serde_json::json!({ "target": unquoted(&target), "all": true });
    inspector_call(w, "app_inspect", args)
}

#[when(regex = r#"^the agent inspects the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_inspects_reference(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_ask(w, Ask::InspectRef { role, name })
}

#[given(regex = r#"^the agent has called the tool "([^"]+)"$"#)]
#[when(regex = r#"^the agent calls the tool "([^"]+)"$"#)]
fn agent_calls(w: &mut LjosWorld, tool: String) -> StepResult {
    inspector_call(w, &tool, serde_json::json!({}))
}

#[given(regex = r#"^the agent has called the tool "([^"]+)" with:$"#)]
#[when(regex = r#"^the agent calls the tool "([^"]+)" with:$"#)]
fn agent_calls_with(w: &mut LjosWorld, tool: String, step: &Step) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_call(w, &tool, args)
}

#[when(regex = r#"^the agent, naming itself "([^"]+)", calls the tool "([^"]+)" with:$"#)]
fn agent_named_calls_with(
    w: &mut LjosWorld,
    client: String,
    tool: String,
    step: &Step,
) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_ask(w, Ask::CallAs { client, tool, args })
}

#[when(regex = r#"^the agent calls the tool "([^"]+)" with the answer's "(/[^"]*)" as "([^"]+)"$"#)]
fn agent_calls_with_the_answer(
    w: &mut LjosWorld,
    tool: String,
    pointer: String,
    field: String,
) -> StepResult {
    inspector_ask(
        w,
        Ask::CallWithAnswer {
            tool,
            pointer,
            field,
        },
    )
}

// One line, whatever its length: the feature lint reads attributes line by line.
#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" on the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_calls_on(w: &mut LjosWorld, tool: String, role: String, name: String) -> StepResult {
    let args = serde_json::json!({});
    inspector_ask(
        w,
        Ask::CallOn {
            tool,
            role,
            name,
            args,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" on the reference of the "([^"]+)" named "([^"]*)" with:$"#)]
fn agent_calls_on_with(w: &mut LjosWorld, tool: String, role: String, name: String, step: &Step) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_ask(w, Ask::CallOn { tool, role, name, args })
}

#[rustfmt::skip]
#[given(regex = r#"^the agent has clicked the reference of the "([^"]+)" named "([^"]*)"$"#)]
#[when(regex = r#"^the agent clicks the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_clicks_reference(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_ask(
        w,
        Ask::CallOn {
            tool: "app_click".to_owned(),
            role,
            name,
            args: serde_json::json!({}),
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" with (\d+) steps of "([^"]+)"$"#)]
fn agent_calls_batch(w: &mut LjosWorld, tool: String, count: usize, inner: String) -> StepResult {
    let steps: Vec<Value> = (0..count)
        .map(|_| serde_json::json!({ "tool": inner, "args": { "keys": "Tab" } }))
        .collect();
    inspector_call(w, &tool, serde_json::json!({ "steps": steps }))
}

#[when("the agent asks for the log since its last request")]
fn agent_reads_log_on(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::LogSince)
}

#[when(regex = r#"^the agent reads the resource "([^"]+)"$"#)]
fn agent_reads_resource(w: &mut LjosWorld, uri: String) -> StepResult {
    inspector_ask(w, Ask::Resource(uri))
}

// On one line, as the lint reads the patterns.
#[rustfmt::skip]
#[when(regex = r"^a client asks for a snapshot with (the right token|no token|a wrong token|the token of an older start)$")]
fn client_asks(w: &mut LjosWorld, token: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: None,
            socket: false,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r"^a client asks on the socket for a snapshot with (the right token|no token|a wrong token)$")]
fn client_asks_on_socket(w: &mut LjosWorld, token: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: None,
            socket: true,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^a client asks for a snapshot with (the right token|no token|a wrong token) from the origin "([^"]+)"$"#)]
fn client_asks_from(w: &mut LjosWorld, token: String, origin: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: Some(origin),
            host: None,
            socket: false,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^a client asks for a snapshot with (the right token|no token|a wrong token) for the host "([^"]+)"$"#)]
fn client_asks_for_host(w: &mut LjosWorld, token: String, host: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: Some(host),
            socket: false,
        },
    )
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called (\d+) times$"#)]
fn method_called_often(
    w: &mut LjosWorld,
    method: String,
    object: String,
    times: u32,
) -> StepResult {
    w.each(|b| (0..times).try_for_each(|_| b.call(&object, &method, &[]).map(drop)))
}

#[then(regex = r#"^the snapshot has a "([^"]+)" named "([^"]*)" with a reference$"#)]
fn snapshot_has(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.outline_has(&role, &name, true))
}

#[then(regex = r#"^the snapshot has no "([^"]+)" named "([^"]*)"$"#)]
fn snapshot_has_no(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.outline_has(&role, &name, false))
}

#[then("the snapshot is:")]
fn snapshot_is(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = inspector_doc(step)?;
    inspector_seen(w, |s| s.outline_is(&want))
}

#[then(regex = r#"^the snapshot contains "(.*)"$"#)]
fn snapshot_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.outline_contains(&unquoted(&text), true))
}

#[then(regex = r#"^the snapshot does not contain "(.*)"$"#)]
fn snapshot_contains_no(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.outline_contains(&unquoted(&text), false))
}

#[then(regex = r#"^the snapshot matches "(.*)"$"#)]
fn snapshot_matches(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.outline_matches(&unquoted(&re), true))
}

#[then(regex = r#"^the snapshot does not match "(.*)"$"#)]
fn snapshot_matches_not(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.outline_matches(&unquoted(&re), false))
}

#[then(regex = r"^the snapshot lists (\d+) nodes?$")]
fn snapshot_lists(w: &mut LjosWorld, count: u64) -> StepResult {
    inspector_seen(w, |s| s.outline_lists(count))
}

#[then(regex = r"^the snapshot is (complete|cut short)$")]
fn snapshot_cut(w: &mut LjosWorld, how: String) -> StepResult {
    inspector_seen(w, |s| s.outline_cut(how == "cut short"))
}

#[then(regex = r"^the search finds (\d+) nodes?$")]
fn search_finds(w: &mut LjosWorld, count: u64) -> StepResult {
    inspector_seen(w, |s| s.found(count))
}

#[then(regex = r#"^the search finds an? "([^"]+)" named "([^"]*)"$"#)]
fn search_finds_one(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.found_one(&role, &name))
}

// The place is a JSON pointer into the answer's structured content and the
// value is JSON, which never starts with the `a` of "at least".
#[then(regex = r#"^the answer's "(/[^"]*)" is ([^a].*)$"#)]
fn answer_is(w: &mut LjosWorld, pointer: String, json: String) -> StepResult {
    let want = value::parse(&json)?;
    inspector_seen(w, |s| s.answer_is(&pointer, &want))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is at least (-?\d+(?:\.\d+)?)$"#)]
fn answer_at_least(w: &mut LjosWorld, pointer: String, least: f64) -> StepResult {
    inspector_seen(w, |s| s.answer_at_least(&pointer, least))
}

#[then(regex = r#"^the answer's "(/[^"]*)" matches "(.*)"$"#)]
fn answer_matches(w: &mut LjosWorld, pointer: String, re: String) -> StepResult {
    inspector_seen(w, |s| s.answer_matches(&pointer, &unquoted(&re)))
}

#[then(regex = r#"^the answer's "(/[^"]*)" has (\d+) entr(?:y|ies)$"#)]
fn answer_has(w: &mut LjosWorld, pointer: String, count: usize) -> StepResult {
    inspector_seen(w, |s| s.answer_has(&pointer, count))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is absent$"#)]
fn answer_absent(w: &mut LjosWorld, pointer: String) -> StepResult {
    inspector_seen(w, |s| s.answer_absent(&pointer))
}

#[then(regex = r"^the answer is at most (\d+) bytes$")]
fn answer_at_most(w: &mut LjosWorld, most: usize) -> StepResult {
    inspector_seen(w, |s| s.answer_at_most(most))
}

#[then(regex = r#"^the answer's text contains "(.*)"$"#)]
fn answer_text_has(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.answer_text_has(&unquoted(&text)))
}

#[then(regex = r#"^the answer does not contain "(.*)"$"#)]
fn answer_lacks(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.answer_lacks(&unquoted(&text)))
}

#[then(regex = r#"^the answer has no inspected property "([^"]+)"$"#)]
fn inspected_property_absent(w: &mut LjosWorld, name: String) -> StepResult {
    inspector_seen(w, |s| s.property_absent(&name))
}

#[then(regex = r#"^the inspected property "([^"]+)" is "([^"]+)"$"#)]
fn inspected_property_provenance(
    w: &mut LjosWorld,
    name: String,
    provenance: String,
) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "provenance", &Value::from(provenance.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is assigned by "([^"]+)"$"#)]
fn inspected_property_assigned(w: &mut LjosWorld, name: String, by: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "assigned_by", &Value::from(by.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is not assigned by "([^"]+)"$"#)]
fn inspected_property_not_assigned(w: &mut LjosWorld, name: String, by: String) -> StepResult {
    inspector_seen(w, |s| s.property_not_assigned_by(&name, &by))
}

#[then(regex = r#"^the inspected property "([^"]+)" has the value "(.*)"$"#)]
fn inspected_property_value(w: &mut LjosWorld, name: String, text: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "value", &Value::from(unquoted(&text)))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" has the number (-?\d+)$"#)]
fn inspected_property_number(w: &mut LjosWorld, name: String, n: i64) -> StepResult {
    inspector_seen(w, |s| s.property_says(&name, "number", &Value::from(n)))
}

#[then(regex = r#"^the inspected property "([^"]+)" has a value that matches "(.*)"$"#)]
fn inspected_property_value_matches(w: &mut LjosWorld, name: String, re: String) -> StepResult {
    inspector_seen(w, |s| s.property_matches(&name, &unquoted(&re)))
}

#[then(regex = r#"^the inspected property "([^"]+)" was written at line (\d+)$"#)]
fn inspected_property_line(w: &mut LjosWorld, name: String, line: u64) -> StepResult {
    inspector_seen(w, |s| s.property_says(&name, "line", &Value::from(line)))
}

#[then(regex = r#"^the inspected property "([^"]+)" is bound at tier (\w+)$"#)]
fn inspected_property_tier(w: &mut LjosWorld, name: String, tier: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "tier", &Value::from(tier.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is bound to the expression "(.*)"$"#)]
fn inspected_property_expression(w: &mut LjosWorld, name: String, text: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "expression", &Value::from(unquoted(&text)))
    })
}

#[then("the socket can be used by its owner alone")]
fn socket_private(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::socket_private)
}

#[then("the socket is gone")]
fn socket_gone(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::socket_gone)
}

#[then("the token file is gone")]
fn token_file_gone(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_gone)
}

#[then(regex = r#"^the inspector is refused with "(.*)"$"#)]
fn inspector_refused(w: &mut LjosWorld, word: String) -> StepResult {
    inspector_seen(w, |s| s.listening_refused(&unquoted(&word)))
}

#[then("the inspector is served")]
fn inspector_is_served(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::listening_served)
}

#[then("the inspector is allowed to listen there")]
fn inspector_allowed(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::listening_allowed)
}

#[then(regex = r#"^the warning mentions "(.*)"$"#)]
fn warning_mentions(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.listening_warned(&unquoted(&text)))
}

#[then(regex = r#"^the agent is refused with "([a-z_]+)"$"#)]
fn agent_refused(w: &mut LjosWorld, code: String) -> StepResult {
    inspector_seen(w, |s| s.refused_with(&code))
}

#[then("the agent is told the reference is stale")]
fn agent_told_stale(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, |s| s.refused_with("stale_ref"))
}

#[then(regex = r#"^the refusal mentions "(.*)"$"#)]
fn refusal_mentions(w: &mut LjosWorld, word: String) -> StepResult {
    inspector_seen(w, |s| s.refusal_mentions(&unquoted(&word)))
}

#[then(regex = r"^the refusal names (\d+) candidates?$")]
fn refusal_candidates(w: &mut LjosWorld, count: usize) -> StepResult {
    inspector_seen(w, |s| s.refusal_candidates(count))
}

#[then(regex = r#"^the agent is offered the tools "([^"]+)"$"#)]
fn agent_offered(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    inspector_seen(w, |s| s.offered(&names))
}

#[then("every tool the agent is offered is marked read-only")]
fn tools_read_only(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::all_read_only)
}

#[then(regex = r#"^the inspector's instructions match "(.*)"$"#)]
fn instructions_match(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.instructions_match(&unquoted(&re)))
}

#[then(regex = r"^the answer has a PNG picture (\d+) by (\d+) pixels$")]
fn answer_picture(w: &mut LjosWorld, width: u32, height: u32) -> StepResult {
    inspector_seen(w, |s| s.picture_is(width, height))
}

#[then(regex = r#"^the pixel at (\d+), (\d+) of the picture is "(#[0-9a-fA-F]{6})"$"#)]
fn answer_pixel(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    inspector_seen(w, |s| s.pixel_is(x, y, &colour))
}

#[then(regex = r#"^the resource contains "(.*)"$"#)]
fn resource_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.resource_contains(&unquoted(&text)))
}

#[then("the client is answered")]
fn client_answered(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::client_answered)
}

#[then(regex = r"^the client is refused with status (\d+)$")]
fn client_refused(w: &mut LjosWorld, status: u16) -> StepResult {
    inspector_seen(w, |s| s.client_refused(status))
}

#[then(regex = r#"^the client's answer contains "(.*)"$"#)]
fn client_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.client_contains(&unquoted(&text), true))
}

#[then(regex = r#"^the client's answer does not contain "(.*)"$"#)]
fn client_contains_no(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.client_contains(&unquoted(&text), false))
}

#[then("the token file can be read by its owner alone")]
fn token_file_private(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_private)
}

#[then("the token file names an address on this machine")]
fn token_file_local(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_local)
}

// Nothing is waiting, no frame is asked for and nothing woke the
// application since it last ran: a host would be asleep.
#[then("the application is left idle")]
fn application_idle(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Rests)
}

#[then("the accessibility tree is not being kept")]
fn tree_given_back(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::TreeGivenBack)
}

// The `top` overlay (ARCHITECTURE.md §3.4, §19.10). Ljós only.

use crate::backend::top::TopSeen;

fn top_seen(w: &mut LjosWorld, check: impl Fn(&TopSeen) -> Result<(), String>) -> StepResult {
    w.each(|b| check(&b.top_seen()?).map_err(BackendError::Failed))
}

#[when("the top overlay is shown")]
fn top_shown(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.top(true))
}

#[when("the top overlay is hidden")]
fn top_hidden(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.top(false))
}

#[then("the top overlay shows nothing")]
fn top_shows_nothing(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| match &seen.sheet {
        None => Ok(()),
        Some(sheet) => Err(format!("the top overlay shows {:?}", sheet.title)),
    })
}

#[then(regex = r#"^the top overlay's title is "([^"]*)"$"#)]
fn top_title_is(w: &mut LjosWorld, title: String) -> StepResult {
    top_seen(w, |seen| {
        let sheet = seen.sheet()?;
        if sheet.title == title {
            Ok(())
        } else {
            Err(format!(
                "the top overlay's title is {:?}, expected {title:?}",
                sheet.title
            ))
        }
    })
}

#[then(regex = r#"^the top overlay's title matches "(.+)"$"#)]
fn top_title_matches(w: &mut LjosWorld, pattern: String) -> StepResult {
    let wanted = Regex::new(&pattern).map_err(|e| format!("{pattern:?}: {e}"))?;
    top_seen(w, |seen| {
        let sheet = seen.sheet()?;
        if wanted.is_match(&sheet.title) {
            Ok(())
        } else {
            Err(format!(
                "the top overlay's title is {:?}, which does not match {pattern:?}",
                sheet.title
            ))
        }
    })
}

#[then(regex = r#"^the top overlay lists the lanes "([^"]*)"$"#)]
fn top_lists_lanes(w: &mut LjosWorld, lanes: String) -> StepResult {
    top_seen(w, |seen| {
        let listed = seen.sheet()?.lanes.join(", ");
        if listed == lanes {
            Ok(())
        } else {
            Err(format!(
                "the top overlay lists the lanes {listed:?}, expected {lanes:?}"
            ))
        }
    })
}

#[then(regex = r#"^the top overlay has charged "(.+)"$"#)]
fn top_has_charged(w: &mut LjosWorld, owner: String) -> StepResult {
    let owner = unquoted(&owner);
    top_seen(w, |seen| {
        let charged = &seen.sheet()?.charged;
        if charged.contains(&owner) {
            Ok(())
        } else {
            Err(format!(
                "the top overlay has charged {charged:?}, and not {owner:?}"
            ))
        }
    })
}

#[then("the top overlay has charged nobody")]
fn top_has_charged_nobody(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| {
        let charged = &seen.sheet()?.charged;
        if charged.is_empty() {
            Ok(())
        } else {
            Err(format!("the top overlay has charged {charged:?}"))
        }
    })
}

// A host would be asleep: the scene asks for no frame and for no wake,
// with the overlay over it.
#[then("the top overlay keeps no frame coming")]
fn top_keeps_no_frame_coming(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| {
        seen.sheet()?;
        match &seen.frame_wanted {
            None => Ok(()),
            Some(why) => Err(format!("a frame is wanted: {why}")),
        }
    })
}

// ------------------------------------------------------------ Authoring
//
// The agent's side of ARCHITECTURE.md §19.3 and the author's command line
// (features/tooling). There is no application: the files of the scenario
// are a project in a directory of its own, `ljos mcp` is started on it and
// `ljos` is run in it. The agent steps above speak to it once it is
// connected. The attributes stay on one line each.

/// A doc string as the text it stands for: no label, no indent.
fn author_doc(step: &Step) -> Result<String, String> {
    Ok(crate::author::text_of(&docstring(step)?))
}

#[given("the authoring tools are connected")]
fn author_connected(w: &mut LjosWorld) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, "", None)
}

#[given(regex = r#"^the authoring tools are connected with the capabilities "([^"]+)"$"#)]
fn author_connected_with(w: &mut LjosWorld, caps: String) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, &caps, None)
}

#[rustfmt::skip]
#[given(regex = r#"^the authoring tools are connected with the capabilities "([^"]+)" and the features runner "([^"]+)"$"#)]
fn author_connected_with_runner(w: &mut LjosWorld, caps: String, runner: String) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, &caps, Some(&runner))
}

#[when(regex = r#"^the agent asks for the prompt "([^"]+)"$"#)]
fn agent_asks_prompt(w: &mut LjosWorld, name: String) -> StepResult {
    let (author, files) = w.author();
    author.prompt(files, &name, &serde_json::json!({}))
}

#[when(regex = r#"^the agent asks for the prompt "([^"]+)" with:$"#)]
fn agent_asks_prompt_with(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let args = serde_json::from_str(&author_doc(step)?)
        .map_err(|e| format!("the arguments are not JSON: {e}"))?;
    let (author, files) = w.author();
    author.prompt(files, &name, &args)
}

#[given(regex = r#"^the author has run "([^"]+)"$"#)]
#[when(regex = r#"^the author runs "([^"]+)"$"#)]
fn author_runs(w: &mut LjosWorld, line: String) -> StepResult {
    let (author, files) = w.author();
    author.run(files, &line)
}

#[then(regex = r#"^the prompt contains "(.*)"$"#)]
fn prompt_contains(w: &mut LjosWorld, text: String) -> StepResult {
    let text = unquoted(&text);
    let prompt = &w.author().0.seen()?.answered()?.text;
    if prompt.contains(&text) {
        Ok(())
    } else {
        Err(format!("{text:?} is not in the prompt:\n{prompt}"))
    }
}

#[then(regex = r#"^the prompt names the tools "([^"]+)"$"#)]
fn prompt_names(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    w.author().0.prompt_names(&names)
}

#[then(regex = r#"^the agent is offered the prompts "([^"]+)"$"#)]
fn agent_offered_prompts(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    let (author, files) = w.author();
    let offered = author.prompts(files)?;
    if offered == names {
        Ok(())
    } else {
        Err(format!("the agent is offered the prompts {offered:?}"))
    }
}

#[then(regex = r#"^the authoring tools' instructions match "(.*)"$"#)]
fn author_instructions_match(w: &mut LjosWorld, re: String) -> StepResult {
    w.author().0.seen()?.instructions_match(&unquoted(&re))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is:$"#)]
fn answer_text_is(w: &mut LjosWorld, pointer: String, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    w.author().0.answer_text_is(&pointer, &want)
}

#[then(regex = r#"^the file "([^"]+)" is:$"#)]
fn file_is(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    let (author, files) = w.author();
    author.file_is(files, &name, &want)
}

// The directory stands for `/`: `proc/meminfo`, `proc/self/cgroup` and
// `sys/fs/cgroup/…/memory.max` are read under it.
#[then(
    regex = r#"^a (desktop|phone) with the system files under "([^"]+)" keeps up to ([\d.]+) MiB of decoded pictures$"#
)]
fn image_budget(w: &mut LjosWorld, class: String, root: String, mib: f64) -> StepResult {
    use ljos_runtime::{DeviceClass, image_cache_budget, usable_memory_under};
    let (author, files) = w.author();
    let root = author.project_path(files, &root)?;
    let class = if class == "phone" {
        DeviceClass::Phone
    } else {
        DeviceClass::Desktop
    };
    let bytes = image_cache_budget(class, usable_memory_under(&root));
    #[allow(clippy::cast_precision_loss)]
    let actual = bytes as f64 / (1024.0 * 1024.0);
    // To the hundredth of a MiB the scenario names it by.
    if (actual - mib).abs() < 0.005 {
        Ok(())
    } else {
        Err(format!("the budget is {actual:.2} MiB ({bytes} bytes)"))
    }
}

// The value is JSON, as in `the answer's "/pointer" is V`.
#[then(regex = r#"^the JSON file "([^"]+)" has "(/[^"]*)" equal to (.+)$"#)]
fn json_file_has(w: &mut LjosWorld, name: String, pointer: String, json: String) -> StepResult {
    let want = value::parse(&json)?;
    let (author, files) = w.author();
    author.json_file_has(files, &name, &pointer, &want)
}

#[then(regex = r#"^the picture "([^"]+\.png)" is (\d+) by (\d+) pixels$"#)]
fn picture_file_is(w: &mut LjosWorld, name: String, width: u32, height: u32) -> StepResult {
    let (author, files) = w.author();
    author.picture_is(files, &name, width, height)
}

#[then("the command prints:")]
fn command_prints(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    w.author().0.ran()?.printed(&want)
}

#[then(regex = r"^the command exits with status (\d+)$")]
fn command_exits(w: &mut LjosWorld, status: i32) -> StepResult {
    w.author().0.ran()?.exited(status)
}

#[then(regex = r#"^the command's output mentions "(.*)"$"#)]
fn command_mentions(w: &mut LjosWorld, text: String) -> StepResult {
    w.author().0.ran()?.mentions(&unquoted(&text), true)
}

#[then(regex = r#"^the command's output does not mention "(.*)"$"#)]
fn command_mentions_not(w: &mut LjosWorld, text: String) -> StepResult {
    w.author().0.ran()?.mentions(&unquoted(&text), false)
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
