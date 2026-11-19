//! A shipped application its users change while it runs (`user-qml`): the
//! files of the user's directory, and what the application was told of
//! them.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;

use crate::backend::{BackendError, UserSeen};
use crate::world::LjosWorld;

use super::{StepResult, docstring};

#[given(regex = r#"^the user's file "([^"]+)":$"#)]
fn users_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.application()?.user_qml_file(&name, Some(&text)))
}

#[given(regex = r#"^the user's file "([^"]+)" links to a file outside the user's directory$"#)]
fn users_file_links_out(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.user_qml_link_out(&name))
}

#[when(regex = r#"^the user saves the file "([^"]+)" as:$"#)]
fn user_saves(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.application()?.user_qml_file(&name, Some(&text)))
}

#[when(
    regex = r#"^the user saves the file "([^"]+)" as a link to a file outside the user's directory$"#
)]
fn user_saves_link_out(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.user_qml_link_out(&name))
}

#[given(regex = r#"^the application keeps its user's modules in "([^"]+)"$"#)]
fn keeps_modules(w: &mut LjosWorld, dir: String) -> StepResult {
    w.each(|b| b.application()?.user_qml_modules(&dir))
}

#[given(regex = r#"^the application reads its user's "theme\.json" itself$"#)]
fn reads_own_theme(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.application()?.user_qml_own_theme())
}

#[when("the user's directory is removed and made again")]
fn user_dir_made_again(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.application()?.user_qml_dir_made_again())
}

#[when(regex = r#"^the user removes the file "([^"]+)"$"#)]
fn user_removes(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.user_qml_file(&name, None))
}

#[when(
    regex = r#"^the application runs "([^"]+\.qml)" built ahead, with the user's directory over it$"#
)]
fn runs_with_user_qml(w: &mut LjosWorld, entry: String) -> StepResult {
    w.each(|b| b.application()?.run_with_user_qml(&entry, &[], true))
}

#[when(
    regex = r#"^the application runs "([^"]+\.qml)" built ahead in a window of its own, with the user's directory over it$"#
)]
fn runs_with_user_qml_in_window(w: &mut LjosWorld, entry: String) -> StepResult {
    let files = w.staged()?;
    w.each(|b| b.application()?.run_with_user_qml_in_window(&files, &entry))
}

#[when(
    regex = r#"^the application runs "([^"]+\.qml)" built ahead without its sources, with the user's directory over it$"#
)]
fn runs_without_sources(w: &mut LjosWorld, entry: String) -> StepResult {
    w.each(|b| b.application()?.run_with_user_qml(&entry, &[], false))
}

#[when(
    regex = r#"^the application runs "([^"]+\.qml)" built ahead from an older "([^"]+)", with the user's directory over it:$"#
)]
fn runs_built_from_older(
    w: &mut LjosWorld,
    entry: String,
    older: String,
    step: &Step,
) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| {
        b.application()?
            .run_with_user_qml(&entry, &[(older.clone(), text.clone())], true)
    })
}

fn seen(w: &mut LjosWorld, check: impl Fn(&UserSeen) -> Result<(), String>) -> StepResult {
    w.each(|b| check(&b.application()?.user_qml_seen()?).map_err(BackendError::Failed))
}

#[then("the user's files are compiled")]
fn compiled(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        s.active
            .then_some(())
            .ok_or_else(|| "the user's files are not compiled".to_owned())
    })
}

#[then("the user's files are not compiled")]
fn not_compiled(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        (!s.active)
            .then_some(())
            .ok_or_else(|| "the user's files are compiled".to_owned())
    })
}

#[then("the user's directory is watched")]
fn watched(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        s.watching
            .then_some(())
            .ok_or_else(|| "nothing is watched".to_owned())
    })
}

#[then("the user's directory is not watched")]
fn not_watched(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        (!s.watching)
            .then_some(())
            .ok_or_else(|| "a watch is kept".to_owned())
    })
}

#[then(regex = r"^(\d+) saves? of the user's files ha(?:s|ve) been applied$")]
fn saves_applied(w: &mut LjosWorld, want: u64) -> StepResult {
    seen(w, |s| {
        (s.reloads == want)
            .then_some(())
            .ok_or_else(|| format!("{} saves were applied, not {want}", s.reloads))
    })
}

#[then(regex = r#"^the application was told of a fault matching "(.+)"$"#)]
fn told_fault(w: &mut LjosWorld, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    seen(w, |s| {
        s.told
            .iter()
            .any(|t| re.is_match(t))
            .then_some(())
            .ok_or_else(|| format!("no fault told matches {pattern:?}: {:?}", s.told))
    })
}

#[then(regex = r#"^the application was told of (\d+) faults? matching "(.+)"$"#)]
fn told_faults(w: &mut LjosWorld, want: usize, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    seen(w, |s| {
        let n = s.told.iter().filter(|t| re.is_match(t)).count();
        (n == want).then_some(()).ok_or_else(|| {
            format!(
                "{n} faults told match {pattern:?}, not {want}: {:?}",
                s.told
            )
        })
    })
}

#[then("the application was told of no fault")]
fn told_nothing(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        s.told
            .is_empty()
            .then_some(())
            .ok_or_else(|| format!("the application was told {:?}", s.told))
    })
}
