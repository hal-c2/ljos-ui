//! Files changed under the running application, and what reloading them did.

use cucumber::{then, when};
use gherkin::Step;
use regex::Regex;

use crate::backend::BackendError;
use crate::fixture;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

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
    w.each(|b| b.reloading()?.change_resource(&name, &bytes))
}

#[when(regex = r#"^the image file "([^"]+)" is saved as a file that is not an image$"#)]
fn image_file_saved_broken(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| {
        b.reloading()?
            .change_resource(&name, &fixture::not_an_image())
    })
}

#[when("a reload happens")]
fn reload_happens(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.reloading()?.reload())
}

#[then(regex = r#"^the reload error matches "([^"]+)"$"#)]
fn reload_error_matches(w: &mut LjosWorld, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| match b.reloading()?.reload_error()? {
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
    w.each(|b| match b.reloading()?.reload_error()? {
        None => Ok(()),
        Some(e) => Err(BackendError::Failed(format!(
            "the reload error is {}:{}: {}",
            e.file, e.line, e.message
        ))),
    })
}

#[then(regex = r#"^the reload error is in "([^"]+)" at line (\d+)$"#)]
fn reload_error_is_in(w: &mut LjosWorld, file: String, line: u32) -> StepResult {
    w.each(|b| match b.reloading()?.reload_error()? {
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
        let count = b.reloading()?.reload_count()?;
        if count == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "{count} reload(s) happened, expected {want}"
            )))
        }
    })
}
