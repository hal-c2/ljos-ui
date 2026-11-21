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

#[when("the user's directory is read")]
fn user_dir_read(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.application()?.user_qml_dir_read())
}

#[when(regex = r"^(\d+) seconds? pass(?:es)? with the application answering every wake$")]
fn left_alone(w: &mut LjosWorld, seconds: u64) -> StepResult {
    w.each(|b| {
        b.application()?
            .user_qml_left_alone(std::time::Duration::from_secs(seconds))
    })
}

#[then(regex = r"^the application was woken at most (\d+) times? while it was left alone$")]
fn woken_at_most(w: &mut LjosWorld, most: u64) -> StepResult {
    seen(w, |s| {
        (s.woken <= most)
            .then_some(())
            .ok_or_else(|| format!("the application was woken {} times", s.woken))
    })
}

#[when(regex = r#"^the user removes the file "([^"]+)"$"#)]
fn user_removes(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.application()?.user_qml_file(&name, None))
}

#[when(
    regex = r#"^the application runs "([^"]+\.qml)" built ahead, with the user's directory over it$"#
)]
fn runs_with_user_qml(w: &mut LjosWorld, entry: String) -> StepResult {
    let files = w.staged()?;
    w.each(|b| {
        b.application()?
            .run_with_user_qml(&files, &entry, &[], true)
    })
}

#[when(regex = r#"^the application runs "([^"]+\.qml)" built ahead, without a user's directory$"#)]
fn runs_built_ahead(w: &mut LjosWorld, entry: String) -> StepResult {
    w.each(|b| b.application()?.run_built_ahead(&entry))
}

#[given(regex = r#"^the application's plugin file "([^"]+)":$"#)]
fn plugin_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.application()?.plugin_file(&name, &text))
}

#[when(regex = r#"^the application fetches its plugin file "([^"]+)" again:$"#)]
fn plugin_fetched_again(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    plugin_file(w, name, step)
}

#[given(
    regex = r#"^the application's plugin file "([^"]+)" links to the file "([^"]+)" outside the application's directories$"#
)]
fn plugin_links_out(w: &mut LjosWorld, name: String, target: String) -> StepResult {
    w.each(|b| b.application()?.plugin_link_out(&name, &target))
}

#[when(
    regex = r#"^the application's plugin "([^"]+)" is given to the method "([^"]+)" of "([^"]+)"$"#
)]
fn plugin_given(w: &mut LjosWorld, name: String, method: String, object: String) -> StepResult {
    w.each(|b| {
        let url = b.application()?.plugin_url(&name)?;
        b.call(&object, &method, &[serde_json::Value::String(url)])
            .map(drop)
    })
}

#[given(regex = r#"^the file "([^"]+)" outside the application's directories:$"#)]
fn outside_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.application()?.outside_file(&name, &text))
}

#[when(
    regex = r#"^the file "([^"]+)" outside the application's directories is given to the method "([^"]+)" of "([^"]+)"$"#
)]
fn outside_given(w: &mut LjosWorld, name: String, method: String, object: String) -> StepResult {
    w.each(|b| {
        let url = b.application()?.outside_url(&name)?;
        b.call(&object, &method, &[serde_json::Value::String(url)])
            .map(drop)
    })
}

#[when(
    regex = r#"^the text "([^"]+)" is given to the method "([^"]+)" of "([^"]+)" with the address of the application's plugin "([^"]+)"$"#
)]
fn text_given_at_plugin(
    w: &mut LjosWorld,
    text: String,
    method: String,
    object: String,
    name: String,
) -> StepResult {
    w.each(|b| {
        let url = b.application()?.plugin_url(&name)?;
        let args = [
            serde_json::Value::String(text.clone()),
            serde_json::Value::String(url),
        ];
        b.call(&object, &method, &args).map(drop)
    })
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
    let files = w.staged()?;
    w.each(|b| {
        b.application()?
            .run_with_user_qml(&files, &entry, &[], false)
    })
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
    let files = w.staged()?;
    w.each(|b| {
        b.application()?
            .run_with_user_qml(&files, &entry, &[(older.clone(), text.clone())], true)
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
        if s.active {
            return Err("the user's files are compiled".to_owned());
        }
        (!s.compiling)
            .then_some(())
            .ok_or_else(|| "the user's files are being compiled".to_owned())
    })
}

#[then("no compiler was made for text")]
fn no_text_compiler(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        (!s.texts)
            .then_some(())
            .ok_or_else(|| "a compiler was made for text".to_owned())
    })
}

#[then("a compiler was made for text")]
fn text_compiler(w: &mut LjosWorld) -> StepResult {
    seen(w, |s| {
        s.texts
            .then_some(())
            .ok_or_else(|| "no compiler was made for text".to_owned())
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

/// `<dir>` in the pattern is the user's directory, as a `file:` URL: a
/// message names a user's file where the user has it, or would.
#[then(
    regex = r#"^the log has a warning matching "([^"]+)" with "<dir>" for the user's directory$"#
)]
fn log_warning_in_users_dir(w: &mut LjosWorld, pattern: String) -> StepResult {
    // `seen` takes a `Fn`, as it asks every backend.
    let dir = std::cell::RefCell::new(String::new());
    seen(w, |s| {
        dir.replace(s.dir.clone());
        Ok(())
    })?;
    let pattern = pattern.replace("<dir>", &regex::escape(&dir.into_inner()));
    super::scene::log_matches(w, &pattern, crate::backend::LogLevel::Warning, true)
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
