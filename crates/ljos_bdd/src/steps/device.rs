//! What the user and the system do to an application on a phone
//! (`mobile/`): the home screen, turning the phone, Back, the system
//! ending a hidden application's screen or process, and the system's store
//! of secrets and places for the application's files.

use cucumber::{then, when};

use crate::backend::{BackSwipe, BackendError};
use crate::world::LjosWorld;

use super::StepResult;

#[when("the user goes to the home screen")]
fn home(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.leave_for_home())
}

#[when("the user opens the application again")]
fn open_again(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.open_again())
}

#[when(regex = r"^the user turns the phone to (landscape|portrait)$")]
fn turn(w: &mut LjosWorld, to: String) -> StepResult {
    w.each(|b| b.device()?.turn(to == "landscape"))
}

#[when("the user presses Back")]
fn back(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.back())
}

#[when("the system ends the application's screen")]
fn end_screen(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.end_screen())
}

#[then(regex = r"^the application is (in front|in the background)$")]
fn in_front(w: &mut LjosWorld, where_: String) -> StepResult {
    let want = where_ == "in front";
    w.each(|b| {
        if b.device()?.in_front()? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the application is not {where_}"
            )))
        }
    })
}

// The application quit: its process is gone too, so the next start is a
// new one.
#[then("the application is no longer running")]
fn ended(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        if b.device()?.running()? {
            Err(BackendError::Failed(
                "the application's process is still running".into(),
            ))
        } else {
            Ok(())
        }
    })
}

#[then("the system has not found the application unresponsive")]
fn responding(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        if b.device()?.not_responding()? {
            Err(BackendError::Failed(
                "the system found the application not responding".into(),
            ))
        } else {
            Ok(())
        }
    })
}

#[when("the system ends the application's process")]
fn end_process(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.end_process())
}

#[when(regex = r#"^the application keeps the secret "([^"]*)" as "([^"]+)" of "([^"]+)"$"#)]
fn keep_secret(w: &mut LjosWorld, secret: String, key: String, service: String) -> StepResult {
    w.each(|b| b.device()?.keep_secret(&service, &key, &secret))
}

#[when(regex = r#"^the application removes the secret "([^"]+)" of "([^"]+)"$"#)]
fn remove_secret(w: &mut LjosWorld, key: String, service: String) -> StepResult {
    w.each(|b| b.device()?.remove_secret(&service, &key))
}

#[then(regex = r#"^the application reads the secret "([^"]*)" as "([^"]+)" of "([^"]+)"$"#)]
fn reads_secret(w: &mut LjosWorld, secret: String, key: String, service: String) -> StepResult {
    w.each(|b| match b.device()?.read_secret(&service, &key)? {
        Some(read) if read == secret => Ok(()),
        read => Err(BackendError::Failed(format!(
            "read {read:?} as {key:?} of {service:?}, expected {secret:?}"
        ))),
    })
}

#[then(regex = r#"^the application reads no secret as "([^"]+)" of "([^"]+)"$"#)]
fn reads_no_secret(w: &mut LjosWorld, key: String, service: String) -> StepResult {
    w.each(|b| match b.device()?.read_secret(&service, &key)? {
        None => Ok(()),
        Some(read) => Err(BackendError::Failed(format!(
            "read {read:?} as {key:?} of {service:?}, expected none"
        ))),
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is a directory of the application's own$"#)]
fn own_directory(w: &mut LjosWorld, property: String, object: String) -> StepResult {
    w.each(|b| {
        let value = b.get(&object, &property)?;
        let Some(path) = value.as_str().filter(|p| !p.is_empty()) else {
            return Err(BackendError::Failed(format!(
                "property {property:?} of {object:?} is {value}, not a path"
            )));
        };
        if b.device()?.own_directory(path)? {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "{path:?} is not a directory of the application's own"
            )))
        }
    })
}

#[then(regex = r#"^no file of the application holds "([^"]+)"$"#)]
fn no_file_holds(w: &mut LjosWorld, text: String) -> StepResult {
    w.each(|b| {
        if b.device()?.files_hold(&text)? {
            Err(BackendError::Failed(format!(
                "a file of the application holds {text:?}"
            )))
        } else {
            Ok(())
        }
    })
}

#[when("the user presses the system's Back")]
fn system_back(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.system_back(BackSwipe::Invoked, 1.0))
}

#[when("the user starts the back swipe")]
fn swipe_starts(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.system_back(BackSwipe::Started, 0.0))
}

#[when(regex = r"^the user's back swipe has come ([0-9.]+) of the way$")]
fn swipe_moves(w: &mut LjosWorld, progress: f64) -> StepResult {
    w.each(|b| b.platform()?.system_back(BackSwipe::Progressed, progress))
}

#[when("the user cancels the back swipe")]
fn swipe_cancelled(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.system_back(BackSwipe::Cancelled, 0.0))
}

#[when("the user completes the back swipe")]
fn swipe_completes(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.platform()?.system_back(BackSwipe::Invoked, 1.0))
}

#[then(regex = r"^the application (took Back|left Back to the system)$")]
fn back_taken(w: &mut LjosWorld, what: String) -> StepResult {
    let want = what == "took Back";
    w.each(|b| {
        if b.platform()?.back_taken()? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the application did not: {what}"
            )))
        }
    })
}
