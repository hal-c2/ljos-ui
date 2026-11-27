//! What the user and the system do to an application on a phone
//! (`mobile/`): the home screen, turning the phone, Back, the system
//! ending a hidden application's screen or process, the system short of
//! memory, the system's store
//! of secrets and places for the application's files, and the developer
//! reaching the application's inspector over adb.

use cucumber::{given, then, when};

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

#[when("the system is short of memory")]
fn short_of_memory(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.short_of_memory())
}

#[then("the application gave back memory it can rebuild")]
fn gave_back(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.device()?.memory_given_back()? {
        0 => Err(BackendError::Failed(
            "the application gave back no memory".into(),
        )),
        _ => Ok(()),
    })
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

// A process can abort after the application is done: a debug build checks
// every call native code makes into Java and ends the process on the first
// it refuses, and the C library ends one that uses what was torn down. A
// quit that ends that way is a crash, not a quit.
#[then("the application ended without aborting")]
fn not_aborted(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let said = b.device()?.aborts()?;
        if said.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the application's process aborted:\n{}",
                said.join("\n")
            )))
        }
    })
}

#[given("the inspector of the application on the phone is connected over adb")]
fn inspector_over_adb(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.device()?.connect_inspector())
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

/// The path `value` names: a plain path, or the `file:` URL that
/// `StandardPaths.writableLocation` gives, as Qt's does.
fn local_path(value: &str) -> Option<String> {
    let path = ljos_engine::url_path(value)?;
    let path = path.to_str()?;
    (!path.is_empty()).then(|| path.to_owned())
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is a directory of the application's own$"#)]
fn own_directory(w: &mut LjosWorld, property: String, object: String) -> StepResult {
    w.each(|b| {
        let value = b.get(&object, &property)?;
        let Some(path) = value.as_str().and_then(local_path) else {
            return Err(BackendError::Failed(format!(
                "property {property:?} of {object:?} is {value}, not a path"
            )));
        };
        if b.device()?.own_directory(&path)? {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "{path:?} is not a directory of the application's own"
            )))
        }
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is the application's "([^"]+)" directory$"#)]
fn app_directory(w: &mut LjosWorld, property: String, object: String, name: String) -> StepResult {
    w.each(|b| {
        let value = b.get(&object, &property)?;
        let path = value.as_str().and_then(local_path).unwrap_or_default();
        let path = path.trim_end_matches('/');
        let names = b.device()?.app_directory(&name)?;
        if names.iter().any(|n| n == path) {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "property {property:?} of {object:?} is {value}, not the application's {name:?} \
                 directory ({})",
                names.join(" or ")
            )))
        }
    })
}

#[then(
    regex = r#"^the system keeps the application's permission to the property "([^"]+)" of "([^"]+)"$"#
)]
fn keeps_permission(w: &mut LjosWorld, property: String, object: String) -> StepResult {
    w.each(|b| {
        let value = b.get(&object, &property)?;
        let Some(url) = value.as_str().filter(|u| !u.is_empty()) else {
            return Err(BackendError::Failed(format!(
                "property {property:?} of {object:?} is {value}, not a URL"
            )));
        };
        if b.device()?.keeps_permission(url)? {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system keeps no permission of the application's to {url:?}"
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

#[given(regex = r#"^the phone's Downloads hold the file "([^"]+)" with the text "([^"]*)"$"#)]
fn download_put(w: &mut LjosWorld, path: String, text: String) -> StepResult {
    w.each(|b| b.device()?.put_download(&path, &text))
}

#[then(regex = r#"^the phone's Downloads hold the file "([^"]+)"$"#)]
fn download_held(w: &mut LjosWorld, path: String) -> StepResult {
    w.each(|b| {
        if b.device()?.download_exists(&path)? {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the phone's Downloads hold no file {path:?}"
            )))
        }
    })
}
