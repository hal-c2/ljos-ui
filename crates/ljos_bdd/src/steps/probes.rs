//! What the runtime can say of itself: passes, text measures, wakes, the
//! observer's work, what is scheduled, kept pictures and the pointer's cursor.

use cucumber::{given, then, when};

use crate::backend::BackendError;
use crate::world::LjosWorld;

use super::StepResult;
use super::inspector::unquoted;

#[given("the window is drawn on the processor")]
fn drawn_on_processor(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.draw_on_processor())
}

#[then(regex = r"^the application has ended within (\d+) milliseconds$")]
fn application_ended(w: &mut LjosWorld, within: u64) -> StepResult {
    w.each(|b| {
        b.probes()?
            .application_ended(std::time::Duration::from_millis(within))
    })
}

#[given("the application is a client of an X server")]
fn x11_client(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.be_x11_client())
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
    if w.author().0.started() {
        return w.author().0.count_wakes();
    }
    w.each(|b| b.probes()?.count_wakes())
}

#[then("no thread but the window's woke since they were counted")]
fn no_other_thread_woke(w: &mut LjosWorld) -> StepResult {
    if w.author().0.started() {
        let woke = w.author().0.wakes_counted()?;
        return threads_woke(&woke).map_err(|e| e.to_string());
    }
    w.each(|b| threads_woke(&b.probes()?.wakes_counted()?))
}

fn threads_woke(woke: &[(String, u64)]) -> Result<(), BackendError> {
    if woke.is_empty() {
        Ok(())
    } else {
        let said: Vec<String> = woke.iter().map(|(t, n)| format!("{t} {n} times")).collect();
        Err(BackendError::Failed(format!("woke: {}", said.join(", "))))
    }
}

#[then(regex = r"^the window's thread woke at most (\d+) times? since the wakes were counted$")]
fn window_woke_at_most(w: &mut LjosWorld, most: u64) -> StepResult {
    // The main thread runs the loop of an application the author started.
    if w.author().0.started() {
        let woke = w.author().0.main_wakes_counted()?;
        return woke_at_most(woke, most).map_err(|e| e.to_string());
    }
    w.each(|b| woke_at_most(b.probes()?.window_wakes_counted()?, most))
}

fn woke_at_most(woke: u64, most: u64) -> Result<(), BackendError> {
    if woke <= most {
        Ok(())
    } else {
        Err(BackendError::Failed(format!(
            "the window's thread woke {woke} times, not at most {most}"
        )))
    }
}

#[when("the application is observed")]
fn application_observed(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.probes()?.observe())
}

// The observer's window is driven with its own inspector, as an agent
// would: it is found by locator and pressed, never reached into.
// A quote in a locator is written \" in a feature.
#[when(regex = r#"^the observer's "((?:[^"\\]|\\.)+)" is clicked$"#)]
fn observer_clicked(w: &mut LjosWorld, locator: String) -> StepResult {
    let locator = unquoted(&locator);
    w.each(|b| {
        let probes = b.probes()?;
        // What is clicked may be what an answer still on its way shows.
        let looked = observer_until(probes, &locator, true)?;
        if looked.count == 0 {
            return Err(observer_missed(
                probes,
                &locator,
                &looked,
                "nothing to click",
            ));
        }
        probes
            .observer_call("app_click", serde_json::json!({ "target": locator }))
            .map(|_| ())
    })
}

#[then(regex = r#"^the observer shows "([^"]+)"$"#)]
fn observer_shows(w: &mut LjosWorld, text: String) -> StepResult {
    observer_matches(w, &format!("text={text}"), true)
}

#[then(regex = r#"^the observer does not show "([^"]+)"$"#)]
fn observer_does_not_show(w: &mut LjosWorld, text: String) -> StepResult {
    observer_matches(w, &format!("text={text}"), false)
}

#[then(regex = r#"^the observer has "((?:[^"\\]|\\.)+)"$"#)]
fn observer_has(w: &mut LjosWorld, locator: String) -> StepResult {
    observer_matches(w, &unquoted(&locator), true)
}

#[then(regex = r#"^the observer does not have "((?:[^"\\]|\\.)+)"$"#)]
fn observer_does_not_have(w: &mut LjosWorld, locator: String) -> StepResult {
    observer_matches(w, &unquoted(&locator), false)
}

fn observer_matches(w: &mut LjosWorld, locator: &str, present: bool) -> StepResult {
    w.each(|b| {
        let probes = b.probes()?;
        let looked = observer_until(probes, locator, present)?;
        match (present, looked.count) {
            (true, 0) => Err(observer_missed(probes, locator, &looked, "nothing")),
            (false, count @ 1..) => Err(BackendError::Failed(format!(
                "the observer still shows {count} matching {locator} {}",
                looked.how()
            ))),
            _ => Ok(()),
        }
    })
}

/// The failure for an item the observer never showed, with what its
/// window shows instead, so that the cause is in the failure.
fn observer_missed(
    probes: &mut dyn crate::backend::Probes,
    locator: &str,
    looked: &Looked,
    what: &str,
) -> BackendError {
    let names: Vec<String> = match probes.observer_call(
        "app_find",
        serde_json::json!({ "locator": "text=", "limit": 200 }),
    ) {
        Ok(seen) => seen["matches"]
            .as_array()
            .map(|all| {
                all.iter()
                    .map(|m| format!("{} {}", m["role"], m["name"]))
                    .collect()
            })
            .unwrap_or_default(),
        Err(e) => vec![format!("(its window could not be listed: {e})")],
    };
    BackendError::Failed(format!(
        "the observer showed {what} matching {locator} {}; it shows {names:?}",
        looked.how()
    ))
}

/// What [`observer_until`] saw when it stopped looking.
struct Looked {
    count: u64,
    /// From the first look to the last, the calls' own settling included.
    waited: std::time::Duration,
    /// Whether the observer's inspector counts frames, without which it
    /// cannot be waited on.
    could_wait: bool,
}

impl Looked {
    fn how(&self) -> String {
        if self.could_wait {
            format!("after {:.1} s of waiting", self.waited.as_secs_f64())
        } else {
            "at once: its inspector counts no frames to wait on".to_owned()
        }
    }
}

/// How long the observer may take to show what it asked its application
/// for: as long as it waits for an answer itself (`ljos observe`'s
/// `PATIENCE`).
const OBSERVER_ANSWERS: std::time::Duration = std::time::Duration::from_secs(30);

/// What `locator` finds in the observer's window once it does
/// (`present`) or does not show them, or when [`OBSERVER_ANSWERS`] is
/// over. A click on the observer returns when its window rests, which it
/// does while what it asked the application is still on its way: the
/// Audit page says "Auditing…" until the audit answers. The window is
/// looked at again only after it ran a frame, never on a timer: an answer
/// shown runs one, and finding items in it runs none.
fn observer_until(
    probes: &mut dyn crate::backend::Probes,
    locator: &str,
    present: bool,
) -> Result<Looked, BackendError> {
    let started = std::time::Instant::now();
    let deadline = started + OBSERVER_ANSWERS;
    // Without `since` the counts are answered at once. They are taken
    // before the window is looked at, so that a frame run between the two
    // is not missed.
    let told = probes.observer_call("app_watch", serde_json::json!({ "topics": ["frames"] }))?;
    let mut frames = told["counts"]["frames"].as_u64();
    loop {
        let found = probes.observer_call("app_find", serde_json::json!({ "locator": locator }))?;
        let count = found["total"].as_u64().unwrap_or(0);
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let looked = |could_wait| Looked {
            count,
            waited: started.elapsed(),
            could_wait,
        };
        let Some(since) = frames else {
            // An inspector that counts no frames cannot say when to look
            // again: what it shows now is the answer.
            return Ok(looked(false));
        };
        if (count > 0) == present || left.is_zero() {
            return Ok(looked(true));
        }
        let wait_ms = u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1);
        let told = probes.observer_call(
            "app_watch",
            serde_json::json!({
                "since": { "frames": since },
                "topics": ["frames"],
                "wait_ms": wait_ms,
                // A burst of frames is one look.
                "settle_ms": 100,
            }),
        )?;
        frames = told["counts"]["frames"].as_u64().or(frames);
    }
}

// The observer is itself an application with a window: the audit it
// offers is held to its own rules.
#[then("the observer passes its own audit")]
fn observer_passes_audit(w: &mut LjosWorld) -> StepResult {
    w.each(|b| {
        let report = b
            .probes()?
            .observer_call("app_audit", serde_json::json!({ "matrix": "current" }))?;
        let errors: Vec<String> = report["findings"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter(|f| f["severity"] == "error")
                    .map(|f| {
                        format!(
                            "{}: {}",
                            f["code"].as_str().unwrap_or("?"),
                            f["message"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(errors.join("; ")))
        }
    })
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

/// `"Ctrl+Shift"` as the bits of `Qt.KeyboardModifier`.
fn modifier_bits(names: &str) -> Result<i64, String> {
    names.split('+').try_fold(0, |bits, name| {
        Ok(bits
            | match name {
                "Shift" => 0x0200_0000,
                "Ctrl" => 0x0400_0000,
                "Alt" => 0x0800_0000,
                "Meta" => 0x1000_0000,
                _ => return Err(format!("{name:?} is not Shift, Ctrl, Alt or Meta")),
            })
    })
}

#[then(regex = r#"^the application sees "([^"]+)" held$"#)]
fn modifiers_held(w: &mut LjosWorld, names: String) -> StepResult {
    let want = modifier_bits(&names)?;
    application_sees(w, want, &names)
}

#[then(regex = r"^the application sees no modifier held$")]
fn no_modifier_held(w: &mut LjosWorld) -> StepResult {
    application_sees(w, 0, "none")
}

fn application_sees(w: &mut LjosWorld, want: i64, names: &str) -> StepResult {
    w.each(|b| match b.probes()?.keyboard_modifiers()? {
        held if held == want => Ok(()),
        held => Err(BackendError::Failed(format!(
            "the application sees the modifiers {held:#x} held, expected {want:#x} ({names:?})"
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

// The file as the application has it now: one edited since runs none.
#[then(regex = r#"^the file "([^"]+)" ran (no )?compiled code$"#)]
fn ran_compiled_code(w: &mut LjosWorld, file: String, no: String) -> StepResult {
    let want = no.is_empty();
    w.each(|b| {
        let (matched, runs) = b.probes()?.compiled_code(&file)?;
        match (want, matched, runs) {
            (true, true, 1..) | (false, false, _) => Ok(()),
            (true, true, 0) => Err(BackendError::Failed(format!(
                "{file} has compiled code, and none of it has run"
            ))),
            (true, false, _) => Err(BackendError::Failed(format!(
                "{file} runs no compiled code: none was made from it as it is now"
            ))),
            (false, true, _) => Err(BackendError::Failed(format!(
                "{file} runs compiled code ({runs} runs so far)"
            ))),
        }
    })
}
