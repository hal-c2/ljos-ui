//! What the application written around the QML does: windows of its own,
//! models written in Rust, files watched on disk, faults in native code, and
//! being opened again or asked to quit.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;

use crate::backend::BackendError;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

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
    w.each(|b| b.application()?.lazy_tree(&name, delay, &rows))
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
    w.each(|b| b.application()?.paged_model(&name, total, page, delay))
}

#[when(regex = r#"^the file "([^"]+\.qml)" is opened in a window of its own$"#)]
fn file_opened_in_window(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.open_window(&name))
}

#[when(regex = r#"^the application puts an object of "([^"]+\.qml)" in the window of "([^"]+)"$"#)]
fn put_in_window(w: &mut LjosWorld, name: String, old: String) -> StepResult {
    w.each(|b| b.application()?.put_in_window(&name, &old))
}

#[then(
    regex = r#"^the application cannot put an object of "([^"]+\.qml)" in the window of "([^"]+)"$"#
)]
fn cannot_put_in_window(w: &mut LjosWorld, name: String, old: String) -> StepResult {
    w.each(|b| match b.application()?.put_in_window(&name, &old) {
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
    w.each(|b| b.application()?.watch_roots(&roots, &entries))
}

#[given(regex = r#"^the application has watched the roots "([^"]+)" for the entries "([^"]+)"$"#)]
fn has_watched_roots(w: &mut LjosWorld, roots: String, entries: String) -> StepResult {
    watches_roots(w, roots, entries)
}

#[rustfmt::skip]
#[when(regex = r#"^the application watches the roots "([^"]+)" for the entries "([^"]+)" with the module "([^"]+)" aliased to "([^"]+)"$"#)]
fn watches_roots_aliased(
    w: &mut LjosWorld,
    roots: String,
    entries: String,
    module: String,
    dir: String,
) -> StepResult {
    let (roots, entries) = (list(&roots), list(&entries));
    let aliases = [(module, dir)];
    w.each(|b| b.application()?.watch_roots_aliased(&roots, &entries, &aliases))
}

#[rustfmt::skip]
#[when(regex = r#"^the application is asked to watch the roots "([^"]+)" for the entries "([^"]+)" with the module "([^"]+)" aliased to "([^"]+)"$"#)]
fn asked_to_watch_roots_aliased(
    w: &mut LjosWorld,
    roots: String,
    entries: String,
    module: String,
    dir: String,
) -> StepResult {
    let (roots, entries) = (list(&roots), list(&entries));
    let aliases = [(module, dir)];
    let mut refusals = Vec::new();
    w.each(|b| match b.application()?.watch_roots_aliased(&roots, &entries, &aliases) {
        Ok(()) => Err(BackendError::Failed("the application watched the roots".into())),
        Err(BackendError::Failed(why)) => {
            refusals.push(why);
            Ok(())
        }
        Err(e) => Err(e),
    })?;
    w.set_refusals(refusals);
    Ok(())
}

#[then(regex = r#"^the application is refused with a message that mentions "([^"]*)"$"#)]
#[then(regex = r#"^the message mentions "([^"]*)"$"#)]
fn refusal_message_mentions(w: &mut LjosWorld, text: String) -> StepResult {
    let refusals = w.refusals();
    if refusals.is_empty() {
        return Err("the application was refused nothing".into());
    }
    match refusals.iter().find(|why| !why.contains(&text)) {
        Some(why) => Err(format!("the refusal does not mention {text:?}: {why}")),
        None => Ok(()),
    }
}

#[when(regex = r#"^the file "([^"]+)" is saved on disk as:$"#)]
fn saved_on_disk(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.application()?.save_on_disk(&name, Some(&text)))
}

#[when(regex = r#"^the file "([^"]+)" is removed from disk$"#)]
fn removed_from_disk(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.save_on_disk(&name, None))
}

#[when(
    regex = r#"^the file "([^"]+\.qml)" is opened in a window of its own, given a Document titled "([^"]*)"$"#
)]
fn file_opened_in_window_given(w: &mut LjosWorld, name: String, title: String) -> StepResult {
    w.each(|b| b.application()?.open_window_given(&name, &title))
}

#[then(regex = r#"^the Document the application gave is titled "([^"]*)"$"#)]
fn given_document_titled(w: &mut LjosWorld, title: String) -> StepResult {
    w.each(|b| {
        let now = b.application()?.given_title()?;
        if now == title {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the Document is titled {now:?}, not {title:?}"
            )))
        }
    })
}

//
// Faults and the boundaries that contain them are Ljós's own; the oracle
// has nothing to compare with.

#[when(regex = r#"^a native handler of "([^"]+)" panics with "([^"]*)"$"#)]
fn native_handler_panics(w: &mut LjosWorld, object: String, message: String) -> StepResult {
    w.each(|b| b.application()?.panic_in(&object, &message))
}

#[then("the application has no fault")]
fn no_application_fault(w: &mut LjosWorld) -> StepResult {
    w.each(|b| match b.application()?.application_fault()? {
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
    w.each(|b| match b.application()?.application_fault()? {
        Some(f) if f.kind == kind && re.is_match(&f.message) => Ok(()),
        Some(f) => Err(BackendError::Failed(format!(
            "the application fault is a {:?}: {:?}; expected a {kind:?} matching {pattern:?}",
            f.kind, f.message
        ))),
        None => Err(BackendError::Failed("the application has no fault".into())),
    })
}

#[when(regex = r"^the application is opened again$")]
fn reopened(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.application()?.reopen())
}

#[then(regex = r"^the application was (not )?asked to quit$")]
fn asked_to_quit(w: &mut LjosWorld, not: String) -> StepResult {
    let want = not.is_empty();
    w.each(|b| match b.application()?.quit_requested()? {
        asked if asked == want => Ok(()),
        true => Err(BackendError::Failed(
            "the application was asked to quit".into(),
        )),
        false => Err(BackendError::Failed(
            "the application was not asked to quit".into(),
        )),
    })
}
