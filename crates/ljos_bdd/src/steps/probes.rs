//! What the runtime can say of itself: passes, text measures, wakes, the
//! observer's work, what is scheduled, kept pictures and the pointer's cursor.

use cucumber::{given, then, when};

use crate::backend::BackendError;
use crate::world::LjosWorld;

use super::StepResult;

#[given("the window is drawn on the processor")]
fn drawn_on_processor(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.draw_on_processor())
}

#[given("the shell chooses how the window is drawn")]
fn shell_chooses_painter(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.let_shell_choose_painter())
}

#[when("the window's passes are counted")]
fn passes_are_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.count_passes())
}

#[then(regex = r"^the window ran at most (\d+) pass(?:es)? since they were counted$")]
fn ran_at_most_passes(w: &mut LjosWorld, most: u64) -> StepResult {
    w.each(|b| {
        let ran = b.probes()?.passes_counted()?;
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
    w.each(|b| b.probes()?.count_text_measures())
}

#[then(regex = r"^text was measured at most (\d+) times? since that was counted$")]
fn text_measured_at_most(w: &mut LjosWorld, most: u64) -> StepResult {
    w.each(|b| {
        let measured = b.probes()?.text_measures_counted()?;
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
    w.each(|b| b.probes()?.count_wakes())
}

#[then("no thread but the window's woke since they were counted")]
fn no_other_thread_woke(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let woke = b.probes()?.wakes_counted()?;
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
    w.each(|b| b.probes()?.observe())
}

#[when("the work of the application and its observer is counted")]
fn work_is_counted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.count_work())
}

#[then("the application and its observer did no work since it was counted")]
fn did_no_work(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let did = b.probes()?.work_counted()?;
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
        let what = b.probes()?.scheduled()?;
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

#[when(regex = r"^the window shows its first frame$")]
fn first_frame(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.first_frame())
}

#[then(regex = r#"^no picture is kept for "([^"]+)" yet$"#)]
fn no_picture_kept(w: &mut LjosWorld, object: String) -> StepResult {
    w.each(|b| match b.probes()?.kept_picture(&object)? {
        None => Ok(()),
        Some((w, h)) => Err(BackendError::Failed(format!(
            "the picture of {object:?} is kept at {w} by {h} pixels already"
        ))),
    })
}

#[then(regex = r#"^the picture of "([^"]+)" is kept at (\d+) by (\d+) pixels$"#)]
fn picture_kept_at(w: &mut LjosWorld, object: String, width: u32, height: u32) -> StepResult {
    w.each(|b| match b.probes()?.kept_picture(&object)? {
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
    w.each(|b| match b.probes()?.pointer_cursor()? {
        shape if shape == want => Ok(()),
        shape => Err(BackendError::Failed(format!(
            "the pointer shows cursor {shape}, expected Qt.{name} ({want})"
        ))),
    })
}
