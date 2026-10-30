//! What the user does with the pointer, the keyboard and an input method.

use cucumber::when;

use crate::backend::{OtherButton, PointerStep, TouchFrame, WheelBy};
use crate::world::LjosWorld;

use super::StepResult;

#[when(regex = r#"^the user clicks the "([^"]+)" "([^"]+)"$"#)]
fn user_clicks(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| b.click(&role, &name))
}

#[when(regex = r#"^the user presses "([^"]+)"$"#)]
fn user_presses(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.key(&chord))
}

// A key held down repeats; the repeats are presses without a release between.
#[when(regex = r#"^the user holds down "([^"]+)" until it repeats (\d+) times?$"#)]
fn user_holds(w: &mut LjosWorld, chord: String, repeats: u32) -> StepResult {
    w.each(|b| b.input()?.key_hold(&chord, repeats))
}

// A key kept down while time passes, and let go of.
#[when(regex = r#"^the user keeps "([^"]+)" down$"#)]
fn user_keeps_down(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.input()?.key_state(&chord, true))
}

#[when(regex = r#"^the user lets go of "([^"]+)"$"#)]
fn user_lets_go(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.input()?.key_state(&chord, false))
}

// Another window takes the keyboard, or gives it back.
#[when(regex = r"^the window (loses|gains) focus$")]
fn window_focus(w: &mut LjosWorld, which: String) -> StepResult {
    w.each(|b| b.input()?.window_active(which == "gains"))
}

#[when(regex = r"^the user touches (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_touches(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.input()?.touch(x, y))
}

// A click with keys held, as a link opened with Ctrl is.
#[rustfmt::skip]
#[when(regex = r#"^the user clicks at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?) holding "([^"]+)"$"#)]
fn user_clicks_holding(w: &mut LjosWorld, x: f64, y: f64, modifiers: String) -> StepResult {
    w.each(|b| b.input()?.click_holding(&modifiers, x, y))
}

// A finger on a touch screen, down and up at one place; "touches" is a
// click of the mouse.
#[when(regex = r"^the user taps (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?) with a finger$")]
fn user_taps_finger(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    let frames = vec![vec![(0, x.round(), y.round())]];
    w.each(|b| b.input()?.touch_frames(&frames))
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
    w.each(|b| b.input()?.click_button(button, x, y))
}

#[when(regex = r#"^the user types "([^"]+)"$"#)]
fn user_types(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.input()?.type_text(&text))
}

#[when(regex = r"^the user double-clicks at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_double_clicks(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.input()?.clicks(2, x, y))
}

#[when(regex = r"^the user triple-clicks at (-?\d+(?:\.\d+)?), (-?\d+(?:\.\d+)?)$")]
fn user_triple_clicks(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.input()?.clicks(3, x, y))
}

#[when(regex = r"^the user drags from ([-\d.]+), ([-\d.]+) to ([-\d.]+), ([-\d.]+)$")]
fn user_drags(w: &mut LjosWorld, x1: f64, y1: f64, x2: f64, y2: f64) -> StepResult {
    w.each(|b| b.input()?.drag((x1, y1), (x2, y2)))
}

// A drag with keys held, as a pan made with Ctrl is.
#[rustfmt::skip]
#[when(regex = r#"^the user drags from ([-\d.]+), ([-\d.]+) to ([-\d.]+), ([-\d.]+) holding "([^"]+)"$"#)]
fn user_drags_holding(w: &mut LjosWorld, x1: f64, y1: f64, x2: f64, y2: f64, modifiers: String) -> StepResult {
    w.each(|b| b.input()?.drag_holding(&modifiers, (x1, y1), (x2, y2)))
}

/// Two fingers come down one after the other at `ends(0)`, go through
/// `ends(k)` for each of `steps` frames, and are lifted. Positions are
/// whole pixels, as a touch screen and QtTest give them.
fn two_fingers(steps: u32, ends: impl Fn(f64) -> [(f64, f64); 2]) -> Vec<TouchFrame> {
    let frame = |t: f64| -> TouchFrame {
        ends(t)
            .iter()
            .enumerate()
            .map(|(i, p)| (i as u32, p.0.round(), p.1.round()))
            .collect()
    };
    let start = frame(0.0);
    let mut frames = vec![start[..1].to_vec(), start];
    frames.extend((1..=steps).map(|k| frame(f64::from(k) / f64::from(steps))));
    frames
}

// At most 4 pixels a finger a frame, as a quick pinch goes.
#[when(
    regex = r"^the user pinches at ([-\d.]+), ([-\d.]+) from ([\d.]+) to ([\d.]+) pixels apart$"
)]
fn user_pinches(w: &mut LjosWorld, x: f64, y: f64, from: f64, to: f64) -> StepResult {
    let steps = ((to - from).abs() / 8.0).ceil().max(1.0) as u32;
    let frames = two_fingers(steps, |t| {
        let h = (from + (to - from) * t) / 2.0;
        [(x - h, y), (x + h, y)]
    });
    w.each(|b| b.input()?.touch_frames(&frames))
}

// Clockwise on the screen, at most 5 degrees a frame, from a line across.
#[when(
    regex = r"^the user turns two fingers ([\d.]+) pixels apart at ([-\d.]+), ([-\d.]+) by (-?[\d.]+) degrees$"
)]
fn user_turns_fingers(w: &mut LjosWorld, apart: f64, x: f64, y: f64, deg: f64) -> StepResult {
    let steps = (deg.abs() / 5.0).ceil().max(1.0) as u32;
    let r = apart / 2.0;
    let frames = two_fingers(steps, |t| {
        let (sin, cos) = (deg * t).to_radians().sin_cos();
        [(x - r * cos, y - r * sin), (x + r * cos, y + r * sin)]
    });
    w.each(|b| b.input()?.touch_frames(&frames))
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
    w.each(|b| b.input()?.wheel((x, y), WheelBy::Notches(dx, dy)))
}

#[when(regex = r"^the user scrolls ([\d.]+) pixels? (\w+) at ([-\d.]+), ([-\d.]+)$")]
fn user_scrolls(w: &mut LjosWorld, n: f64, direction: String, x: f64, y: f64) -> StepResult {
    let (dx, dy) = wheel_delta(n, &direction)?;
    w.each(|b| b.input()?.wheel((x, y), WheelBy::Pixels(dx, dy)))
}

#[when(regex = r#"^the input method composes "([^"]*)"$"#)]
fn input_method_composes(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.input()?.compose(&text))
}

#[when(regex = r#"^the input method commits "([^"]*)"$"#)]
fn input_method_commits(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.input()?.commit(&text))
}
