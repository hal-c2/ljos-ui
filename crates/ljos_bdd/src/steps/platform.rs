//! The system's services around the application: the clipboard, its
//! dialogs, drags from and to other applications, permissions, sensors, a
//! position, opened URLs and a window's backdrop.

use cucumber::{given, then, when};
use gherkin::Step;

use crate::backend::{BackendError, OutsideDrag, SystemDialogAnswer};
use crate::world::LjosWorld;

use super::StepResult;

#[given(regex = r#"^"([^"]*)" was copied in another application$"#)]
fn clipboard_given(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.platform()?.set_clipboard(&text))
}

#[given(regex = r#"^the HTML "([^"]*)" was copied in another application$"#)]
fn clipboard_html_given(w: &mut LjosWorld, html: String) -> StepResult {
    w.each(|b| b.platform()?.set_clipboard_html(&html))
}

#[given("the system cannot take its dialogs away")]
fn dialogs_stay(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.system_dialogs_stay())
}

// Before the QML: what the application reads at the start.
#[given("the system asks the user before it grants a permission")]
fn permissions_ask(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.permissions_ask())
}

#[given(regex = r"^the system has an? ([a-z ]+)$")]
fn system_has_sensor(w: &mut LjosWorld, kind: String) -> StepResult {
    w.each(|b| b.platform()?.add_sensor(&kind))
}

#[given(
    regex = r"^the system's position is latitude (-?[0-9.]+), longitude (-?[0-9.]+)(?:, altitude (-?[0-9.]+))?$"
)]
fn system_position(
    w: &mut LjosWorld,
    latitude: f64,
    longitude: f64,
    altitude: String,
) -> StepResult {
    let altitude = altitude.parse().unwrap_or(f64::NAN);
    w.each(|b| b.platform()?.system_position(latitude, longitude, altitude))
}

#[when(
    regex = r"^the system's position moves to latitude (-?[0-9.]+), longitude (-?[0-9.]+)(?:, altitude (-?[0-9.]+))?$"
)]
fn system_position_moves(
    w: &mut LjosWorld,
    latitude: f64,
    longitude: f64,
    altitude: String,
) -> StepResult {
    let altitude = altitude.parse().unwrap_or(f64::NAN);
    w.each(|b| {
        b.platform()?
            .system_position_moves(latitude, longitude, altitude)
    })
}

#[when(regex = r"^the ([a-z ]+) reads (-?[0-9.]+(?:, -?[0-9.]+)*)$")]
fn sensor_reads(w: &mut LjosWorld, kind: String, values: String) -> StepResult {
    let values: Vec<f64> = values
        .split(", ")
        .map(|v| v.parse().map_err(|e| format!("{v:?}: {e}")))
        .collect::<Result<_, _>>()?;
    w.each(|b| b.platform()?.sensor_reads(&kind, &values))
}

#[when(regex = r#"^the user chooses "([^"]+)" in the system dialog$"#)]
fn user_chooses(w: &mut LjosWorld, path: String) -> StepResult {
    w.each(|b| {
        b.platform()?
            .answer_system_dialog(SystemDialogAnswer::Choose(vec![path.clone()]))
    })
}

#[when(regex = r#"^the user chooses the files "([^"]+)" in the system dialog$"#)]
fn user_chooses_files(w: &mut LjosWorld, paths: String) -> StepResult {
    let paths: Vec<String> = paths.split(", ").map(str::to_owned).collect();
    w.each(|b| {
        b.platform()?
            .answer_system_dialog(SystemDialogAnswer::Choose(paths.clone()))
    })
}

#[when(regex = r#"^the user answers "([^"]+)" in the system dialog$"#)]
fn user_answers(w: &mut LjosWorld, button: String) -> StepResult {
    w.each(|b| {
        b.platform()?
            .answer_system_dialog(SystemDialogAnswer::Button(button.clone()))
    })
}

#[when("the user dismisses the system dialog")]
fn user_dismisses(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        b.platform()?
            .answer_system_dialog(SystemDialogAnswer::Dismiss)
    })
}

// The application is not told; it finds out when it asks.
#[when(regex = r#"^"([^"]*)" is copied in another application$"#)]
fn copied_elsewhere(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| b.platform()?.copy_elsewhere(&text))
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
        b.platform()?.drag_from_outside(&OutsideDrag::Enter {
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
        b.platform()?.drag_from_outside(&OutsideDrag::Enter {
            formats: formats.clone(),
            at: (x, y),
        })
    })
}

// The window system has not handed over what the formats hold yet.
#[when(regex = r#"^another application offers the formats "([^"]*)" over ([-\d.]+), ([-\d.]+)$"#)]
fn outside_drag_offers(w: &mut LjosWorld, formats: String, x: f64, y: f64) -> StepResult {
    let formats = action_names(&formats);
    w.each(|b| {
        b.platform()?.drag_from_outside(&OutsideDrag::Offer {
            formats: formats.clone(),
            at: (x, y),
        })
    })
}

#[when(regex = r#"^the drag from another application moves to ([-\d.]+), ([-\d.]+)$"#)]
fn outside_drag_moves(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.platform()?.drag_from_outside(&OutsideDrag::Move((x, y))))
}

#[when("the drag from another application leaves the window")]
fn outside_drag_leaves(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.drag_from_outside(&OutsideDrag::Leave))
}

#[when("the drag from another application is let go")]
fn outside_drag_dropped(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.drag_from_outside(&OutsideDrag::Drop))
}

// What the application told the window system at the last step: whether
// a drop target under the drag takes it, or took the drop.
#[then(regex = r#"^the drag from another application is (taken|not taken)$"#)]
fn outside_drag_taken(w: &mut LjosWorld, which: String) -> StepResult {
    let want = which == "taken";
    w.each(|b| {
        if b.platform()?.outside_drag_taken()? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the drag from another application is {}taken",
                if want { "not " } else { "" }
            )))
        }
    })
}

#[then(regex = r#"^the drag from another application is taken as (copy|move|link)$"#)]
fn outside_drag_taken_as(w: &mut LjosWorld, action: String) -> StepResult {
    w.each(|b| match b.platform()?.outside_drag_action()? {
        Some(a) if a == action => Ok(()),
        Some(a) => Err(BackendError::Failed(format!(
            "the drag from another application is taken as {a}"
        ))),
        None => Err(BackendError::Failed(
            "the drag from another application is not taken".into(),
        )),
    })
}

/// `"copy, move"` as the names it lists.
fn action_names(list: &str) -> Vec<String> {
    list.split(',')
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty())
        .collect()
}

#[given(
    regex = r#"^the drag from another application allows "([^"]*)" and proposes "(copy|move|link)"$"#
)]
fn outside_drags_allow(w: &mut LjosWorld, allowed: String, proposed: String) -> StepResult {
    let allowed = action_names(&allowed);
    w.each(|b| b.platform()?.outside_drags_allow(&allowed, &proposed))
}

#[when(regex = r#"^the drag from another application now proposes "(copy|move|link)"$"#)]
fn outside_drag_proposes(w: &mut LjosWorld, action: String) -> StepResult {
    w.each(|b| {
        b.platform()?
            .drag_from_outside(&OutsideDrag::Propose(action.clone()))
    })
}

// A drag out of the window, as the window system is asked to carry it and
// tells back how it ended.
#[given("the window system carries drags to other applications")]
fn drags_out(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.drags_out())
}

// The table has a `format` and a `data` column.
#[then("the window system is asked to carry a drag out of the window with:")]
fn drag_out_asked_with(w: &mut LjosWorld, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of format and data".to_owned())?;
    let want: Vec<(String, String)> = table
        .rows
        .iter()
        .skip(1)
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    w.each(|b| match b.platform()?.drag_out_asked()? {
        Some(asked) if asked.formats == want => Ok(()),
        Some(asked) => Err(BackendError::Failed(format!(
            "the drag out of the window offers {:?}",
            asked.formats
        ))),
        None => Err(BackendError::Failed(
            "the window system is asked to carry no drag out of the window".into(),
        )),
    })
}

#[then("the window system is asked to carry no drag out of the window")]
fn no_drag_out_asked(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.platform()?.drag_out_asked() {
        Ok(None) | Err(BackendError::NotImplemented(_)) => Ok(()),
        Ok(Some(asked)) => Err(BackendError::Failed(format!(
            "the window system is asked to carry a drag out of the window: {asked:?}"
        ))),
        Err(e) => Err(e),
    })
}

#[then(regex = r#"^the drag out of the window allows "([^"]*)" and proposes "(copy|move|link)"$"#)]
fn drag_out_actions(w: &mut LjosWorld, allowed: String, proposed: String) -> StepResult {
    let allowed = action_names(&allowed);
    w.each(|b| match b.platform()?.drag_out_asked()? {
        Some(asked) if asked.allowed == allowed && asked.proposed == proposed => Ok(()),
        Some(asked) => Err(BackendError::Failed(format!(
            "the drag out of the window allows \"{}\" and proposes \"{}\"",
            asked.allowed.join(", "),
            asked.proposed
        ))),
        None => Err(BackendError::Failed(
            "the window system is asked to carry no drag out of the window".into(),
        )),
    })
}

#[when(regex = r#"^the other application takes the drag out of the window as (copy|move|link)$"#)]
fn drag_out_taken(w: &mut LjosWorld, action: String) -> StepResult {
    w.each(|b| b.platform()?.drag_out_ends(Some(&action)))
}

#[when("the other application turns the drag out of the window down")]
fn drag_out_turned_down(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.drag_out_ends(None))
}

#[then("the drag out of the window was given up")]
fn drag_out_given_up(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        if b.platform()?.drag_out_given_up()? {
            Ok(())
        } else {
            Err(BackendError::Failed(
                "the drag out of the window was not given up".into(),
            ))
        }
    })
}

#[when(regex = r#"^the drag out of the window comes back over ([-\d.]+), ([-\d.]+)$"#)]
fn drag_out_comes_back(w: &mut LjosWorld, x: f64, y: f64) -> StepResult {
    w.each(|b| b.platform()?.drag_out_comes_back((x, y)))
}

// Let go over the window it came from: the window system drops it there
// and tells the source the action the drop was taken with.
#[when("the drag out of the window is let go there")]
fn drag_out_let_go_there(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let p = b.platform()?;
        p.drag_from_outside(&OutsideDrag::Drop)?;
        let action = p.outside_drag_action()?;
        p.drag_out_ends(action.as_deref())
    })
}

#[when(regex = r#"^the user (grants|denies) the "([^"]+)" permission$"#)]
fn user_answers_permission(w: &mut LjosWorld, how: String, name: String) -> StepResult {
    w.each(|b| b.platform()?.answer_permission(&name, how == "grants"))
}

#[given("the window system brings no files dragged in from other applications")]
fn no_external_drops_given(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.no_external_drops())
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
        let asked = b.platform()?.window_backdrop(&object)?;
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
        let held = b.platform()?.clipboard()?;
        if held == text {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the clipboard holds {held:?}, expected {text:?}"
            )))
        }
    })
}

#[then(regex = r#"^the system shows an? "([^"]+)" dialog titled "([^"]*)"$"#)]
fn system_shows(w: &mut LjosWorld, kind: String, title: String) -> StepResult {
    w.each(|b| {
        let shown = b.platform()?.system_dialogs()?;
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
        let shown = b.platform()?.system_dialogs()?;
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
        let opened = b.platform()?.opened_urls()?;
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
        let asked = b.platform()?.permission_requests()?;
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
        let asked = b.platform()?.permission_requests()?;
        if asked.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system asks for {asked:?}"
            )))
        }
    })
}
