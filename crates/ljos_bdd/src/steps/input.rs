//! What the user does with the pointer, the keyboard and an input method.

use cucumber::{given, then, when};
use gherkin::Step;

use crate::backend::{
    BackendError, HeldText, InputMethodGeometry, OtherButton, PointerStep, TouchFrame, WheelBy,
};
use crate::world::LjosWorld;

use super::{StepResult, docstring};

#[when(regex = r#"^the user clicks the "([^"]+)" "([^"]+)"$"#)]
fn user_clicks(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    w.each(|b| b.click(&role, &name))
}

#[when(regex = r#"^the user presses "([^"]+)"$"#)]
fn user_presses(w: &mut LjosWorld, chord: String) -> StepResult {
    w.each(|b| b.key(&chord))
}

// A native type's own event, handed to one item that need not have the
// focus (`ljos::input::send_input`).
#[when(regex = r#"^the application sends "([^"]+)" to "([^"]+)"$"#)]
fn application_sends_key(w: &mut LjosWorld, chord: String, path: String) -> StepResult {
    w.each(|b| b.input()?.send_key_to(&chord, &path))
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

// A seat with a pointer and no keyboard: a kiosk, a tablet. Before the
// application starts, which hears of the seat's keyboards once.
#[given(regex = r"^the seat has no keyboard$")]
fn seat_without_keyboard(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.input()?.seat_keyboard(false))
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

/// One frame a line: `none`, or the fingers down then as `id at x, y`
/// joined by `;`. Ids are whole numbers from 0.
fn finger_frames(text: &str) -> Result<Vec<TouchFrame>, String> {
    let finger = |f: &str| -> Result<(u32, f64, f64), String> {
        let bad = || format!("{f:?} is not `id at x, y`");
        let (id, at) = f.split_once(" at ").ok_or_else(bad)?;
        let (x, y) = at.split_once(',').ok_or_else(bad)?;
        let n = |s: &str| s.trim().parse::<f64>().map_err(|_| bad());
        Ok((id.trim().parse().map_err(|_| bad())?, n(x)?, n(y)?))
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            if l == "none" {
                Ok(Vec::new())
            } else {
                l.split(';').map(|f| finger(f.trim())).collect()
            }
        })
        .collect()
}

// Fingers as written, frame by frame; every finger is lifted after the
// last frame.
#[when(regex = r"^the user's fingers go through these positions:$")]
fn user_finger_frames(w: &mut LjosWorld, step: &Step) -> StepResult {
    let frames = finger_frames(&docstring(step)?)?;
    w.each(|b| b.input()?.touch_frames(&frames))
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

// A wheel turned with keys held, as a zoom made with Ctrl is.
#[rustfmt::skip]
#[when(regex = r#"^the user turns the wheel ([\d.]+) notch(?:es)? (\w+) at ([-\d.]+), ([-\d.]+) holding "([^"]+)"$"#)]
fn user_turns_wheel_holding(w: &mut LjosWorld, n: f64, direction: String, x: f64, y: f64, modifiers: String) -> StepResult {
    let (dx, dy) = wheel_delta(n, &direction)?;
    w.each(|b| b.input()?.wheel_holding(&modifiers, (x, y), WheelBy::Notches(dx, dy)))
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

#[when(regex = r#"^the input method holds "([^"]*)" with the cursor at (\d+)$"#)]
fn input_method_holds(w: &mut LjosWorld, text: String, cursor: usize) -> StepResult {
    let held = HeldText {
        text,
        anchor: cursor,
        cursor,
        compose: None,
    };
    w.each(|b| b.input()?.input_method_holds(&held))
}

#[when(regex = r#"^the input method holds "([^"]*)" selecting from (\d+) to (\d+)$"#)]
fn input_method_holds_selected(
    w: &mut LjosWorld,
    text: String,
    anchor: usize,
    cursor: usize,
) -> StepResult {
    let held = HeldText {
        text,
        anchor,
        cursor,
        compose: None,
    };
    w.each(|b| b.input()?.input_method_holds(&held))
}

#[when(
    regex = r#"^the input method holds "([^"]*)" composing (\d+) to (\d+) with the cursor at (\d+)$"#
)]
fn input_method_holds_composing(
    w: &mut LjosWorld,
    text: String,
    start: usize,
    end: usize,
    cursor: usize,
) -> StepResult {
    let held = HeldText {
        text,
        anchor: cursor,
        cursor,
        compose: Some(start..end),
    };
    w.each(|b| b.input()?.input_method_holds(&held))
}

#[when(regex = r#"^the input method commits "([^"]*)"$"#)]
fn input_method_commits(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.input()?.commit(&text))
}

/// What the input method is told now, or why it is told nothing.
fn geometry(
    w: &mut LjosWorld,
    check: impl Fn(&InputMethodGeometry) -> Result<(), String>,
) -> StepResult {
    w.each(|b| {
        let told = b.input()?.input_method_geometry()?.ok_or_else(|| {
            BackendError::Failed("the input method is told of no editor".to_owned())
        })?;
        check(&told).map_err(BackendError::Failed)
    })
}

fn same_rect(what: &str, told: [f64; 4], want: [f64; 4]) -> Result<(), String> {
    if told.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3) {
        Ok(())
    } else {
        let [x, y, w, h] = told;
        Err(format!(
            "the input method is told {what} is at {x}, {y} sized {w} by {h}"
        ))
    }
}

#[then(
    regex = r#"^the input method is told the (editor|cursor) is at (-?[\d.]+), (-?[\d.]+) sized ([\d.]+) by ([\d.]+)$"#
)]
fn input_method_told_rect(
    w: &mut LjosWorld,
    what: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> StepResult {
    geometry(w, |g| {
        let told = if what == "editor" { g.editor } else { g.cursor };
        same_rect(&format!("the {what}"), told, [x, y, width, height])
    })
}

// A row a UTF-16 unit: `x`, `y`, `width` and `height` columns.
#[then("the input method is told the composed characters are at:")]
fn input_method_told_composing(w: &mut LjosWorld, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of x, y, width and height".to_owned())?;
    let want = table
        .rows
        .iter()
        .skip(1)
        .map(|r| {
            let n = |i: usize| {
                r[i].trim()
                    .parse::<f64>()
                    .map_err(|e| format!("{:?} is no number: {e}", r[i]))
            };
            Ok([n(0)?, n(1)?, n(2)?, n(3)?])
        })
        .collect::<Result<Vec<_>, String>>()?;
    geometry(w, |g| {
        if g.composing.len() != want.len() {
            return Err(format!(
                "the input method is told of {} composed units, not {}: {:?}",
                g.composing.len(),
                want.len(),
                g.composing
            ));
        }
        for (i, (told, want)) in g.composing.iter().zip(&want).enumerate() {
            same_rect(&format!("composed unit {i}"), *told, *want)?;
        }
        Ok(())
    })
}

#[then("the input method is told nothing is composed")]
fn input_method_told_nothing_composed(w: &mut LjosWorld) -> StepResult {
    geometry(w, |g| {
        if g.composing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the input method is told of composed units at {:?}",
                g.composing
            ))
        }
    })
}

#[then(regex = r#"^the input method is told of (\d+) composed units?$"#)]
fn input_method_told_units(w: &mut LjosWorld, count: usize) -> StepResult {
    geometry(w, |g| {
        if g.composing.len() == count {
            Ok(())
        } else {
            Err(format!(
                "the input method is told of {} composed units: {:?}",
                g.composing.len(),
                g.composing
            ))
        }
    })
}

// The two units of one character outside the Basic Multilingual Plane.
#[then(regex = r#"^the input method is told composed units (\d+) and (\d+) share a box$"#)]
fn input_method_told_shared(w: &mut LjosWorld, a: usize, b: usize) -> StepResult {
    geometry(w, |g| match (g.composing.get(a), g.composing.get(b)) {
        (Some(x), Some(y)) => same_rect(&format!("composed unit {a}"), *x, *y)
            .map_err(|e| format!("{e}, not where unit {b} is: {y:?}")),
        _ => Err(format!(
            "the input method is told of {} composed units: {:?}",
            g.composing.len(),
            g.composing
        )),
    })
}

// A box of no width is one an input method cannot place a window by:
// only a line break may have one.
#[then("the input method is told each composed unit has a width")]
fn input_method_told_widths(w: &mut LjosWorld) -> StepResult {
    geometry(w, |g| match g.composing.iter().position(|r| r[2] <= 1e-3) {
        None if !g.composing.is_empty() => Ok(()),
        None => Err("the input method is told nothing is composed".to_owned()),
        Some(i) => Err(format!(
            "the input method is told composed unit {i} has no width: {:?}",
            g.composing
        )),
    })
}

// Characters of both directions side by side: each box is its own
// character's, not the stretch between two cursor positions.
#[then("the input method is told no two composed characters overlap")]
fn input_method_told_apart(w: &mut LjosWorld) -> StepResult {
    geometry(w, |g| {
        let rects = &g.composing;
        if rects.is_empty() {
            return Err("the input method is told nothing is composed".to_owned());
        }
        for (i, a) in rects.iter().enumerate() {
            for (j, b) in rects.iter().enumerate().skip(i + 1) {
                let same = a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-3);
                let across = a[0] < b[0] + b[2] - 1e-3 && b[0] < a[0] + a[2] - 1e-3;
                let down = a[1] < b[1] + b[3] - 1e-3 && b[1] < a[1] + a[3] - 1e-3;
                if !same && across && down {
                    return Err(format!(
                        "the input method is told composed units {i} and {j} overlap: {a:?} and {b:?}"
                    ));
                }
            }
        }
        Ok(())
    })
}
