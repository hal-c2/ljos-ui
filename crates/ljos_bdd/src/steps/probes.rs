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

// A part of a page that does not fit is scrolled to, as a person turns
// the wheel over it: whatever scrolls under the pointer takes it.
#[when(
    regex = r#"^the observer's "((?:[^"\\]|\\.)+)" is scrolled (\d+) notches (right|left|up|down)$"#
)]
fn observer_scrolled(w: &mut LjosWorld, locator: String, notches: f64, way: String) -> StepResult {
    let locator = unquoted(&locator);
    // A wheel turned away from the reader shows what is further on, which
    // is a negative turn on both axes (app_scroll's `dy`, and its `dx`).
    let (dx, dy) = match way.as_str() {
        "right" => (-notches, 0.0),
        "left" => (notches, 0.0),
        "up" => (0.0, notches),
        _ => (0.0, -notches),
    };
    w.each(|b| {
        let probes = b.probes()?;
        let looked = observer_until(probes, &locator, true)?;
        if looked.count == 0 {
            return Err(observer_missed(
                probes,
                &locator,
                &looked,
                "nothing to scroll",
            ));
        }
        probes
            .observer_call(
                "app_scroll",
                serde_json::json!({ "target": locator, "dx": dx, "dy": dy }),
            )
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

// What the observer itself warned of is read from its own inspector, as an
// agent reads it, not from its stderr: the warnings are the same, and the
// inspector gives them one by one with their level.
#[then(regex = r#"^the observer's log has no warning matching "([^"]+)"$"#)]
fn observer_log_has_no_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    let re = regex::Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| {
        let told = b
            .probes()?
            .observer_call("app_logs", serde_json::json!({ "level": "warning" }))?;
        // An answer gives the newest lines that fit it: a warning before
        // them is not in it, and no answer reaches back to it.
        if told["truncated"] == true {
            return Err(BackendError::Failed(format!(
                "the observer warned {} times, more than the {} one answer gives: \
                 an older warning matching {pattern:?} could not be read",
                told["total"], told["count"]
            )));
        }
        let hits: Vec<&str> = told["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["message"].as_str())
            .filter(|m| re.is_match(m))
            .collect();
        if hits.is_empty() {
            return Ok(());
        }
        Err(BackendError::Failed(format!(
            "the observer warned {} time(s) matching {pattern:?}:\n  {}",
            hits.len(),
            hits.join("\n  ")
        )))
    })
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

// The observer's window is given a size as a window manager or a page
// holding it would: through its own inspector, the way an agent does.
#[when(regex = r"^the observer's window is (\d+) by (\d+) pixels$")]
fn observer_resized(w: &mut LjosWorld, width: u32, height: u32) -> StepResult {
    w.each(|b| {
        b.probes()?
            .observer_call(
                "app_resize",
                serde_json::json!({ "width": width, "height": height }),
            )
            .map(|_| ())
    })
}

// What is acted on next may be laid out by an answer still on its way: a
// page is scrolled once what makes it long is there.
#[when(regex = r#"^the observer's "((?:[^"\\]|\\.)+)" is shown$"#)]
fn observer_shown(w: &mut LjosWorld, locator: String) -> StepResult {
    let locator = unquoted(&locator);
    w.each(|b| {
        let probes = b.probes()?;
        let looked = observer_until(probes, &locator, true)?;
        if looked.count == 0 {
            return Err(observer_missed(probes, &locator, &looked, "nothing"));
        }
        Ok(())
    })
}

// An item that runs past its window's edge is cut off there: its words are
// clipped, not elided. Only across, as a list that scrolls holds rows
// below its window's bottom on purpose. A locator that finds nothing
// fails: every one of none lies anywhere, and a check that looks at
// nothing would pass whatever the window shows.
#[then(
    regex = r#"^every "((?:[^"\\]|\\.)+)" in the observer lies within the width of its window$"#
)]
fn observer_within_width(w: &mut LjosWorld, locator: String) -> StepResult {
    let locator = unquoted(&locator);
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let width = observer_width(probes)?;
            let all = boxes(probes, &locator)?;
            if all.is_empty() {
                return Ok(Some(format!("it shows nothing matching {locator}")));
            }
            let past: Vec<String> = all
                .into_iter()
                .filter(|m| m.at[0] < -EDGE || m.at[0] + m.at[2] > width + EDGE)
                .map(|m| m.to_string())
                .collect();
            Ok((!past.is_empty())
                .then(|| format!("in a window {width} wide, these run past its edge: {past:?}")))
        })
    })
}

/// The width of the observer's window. The status lists the windows; the
/// one inspected, the observer's own, is the first.
fn observer_width(probes: &mut dyn crate::backend::Probes) -> Result<f64, BackendError> {
    Ok(observer_size(probes)?[0])
}

/// The width and height of the observer's window, as [`observer_width`]
/// reads it.
fn observer_size(probes: &mut dyn crate::backend::Probes) -> Result<[f64; 2], BackendError> {
    let status = probes.observer_call("app_status", serde_json::json!({}))?;
    let side = |name: &str| {
        status["windows"][0][name].as_f64().ok_or_else(|| {
            BackendError::Failed(format!("the observer's status gives no {name}: {status}"))
        })
    };
    Ok([side("width")?, side("height")?])
}

// A text given no room is elided to nothing, one past its window's edge
// is cut off there, and one a clipping panel holds is cut off at the
// panel's edge: each is found by its words but shows not all of them. The
// text shown must have room for all its words, inside the window, drawn
// at each of its corners, and not be elided.
#[then(regex = r#"^the observer shows "([^"]+)" whole$"#)]
fn observer_shows_whole(w: &mut LjosWorld, text: String) -> StepResult {
    let locator = format!("text={text}");
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let [width, height] = observer_size(probes)?;
            let all = boxes(probes, &locator)?;
            if all.is_empty() {
                return Ok(Some(format!("it shows nothing matching {locator}")));
            }
            let mut cut = Vec::new();
            for m in &all {
                let [x, y, w, h] = m.at;
                if w <= 0.0
                    || h <= 0.0
                    || x < -EDGE
                    || x + w > width + EDGE
                    || y < -EDGE
                    || y + h > height + EDGE
                {
                    cut.push(format!("{m}, in a window {width}x{height}"));
                } else if let Some(corner) = clipped_corner(probes, m)? {
                    cut.push(format!("{m}, not drawn at {corner}"));
                } else if elided(probes, &m.reference)? {
                    cut.push(format!("{m}, elided"));
                }
            }
            Ok((!cut.is_empty()).then(|| format!("these do not show {text:?} whole: {cut:?}")))
        })
    })
}

/// A corner of `m`, a pixel inside it, where the last frame did not draw
/// it: an ancestor that clips cut it off there. What is drawn over it does
/// not count, since the answer lists every item drawn at the point.
fn clipped_corner(
    probes: &mut dyn crate::backend::Probes,
    m: &Shown,
) -> Result<Option<String>, BackendError> {
    let [x, y, w, h] = m.at;
    // A pixel in, or the middle of a side thinner than two.
    let (dx, dy) = (w.min(2.0) / 2.0, h.min(2.0) / 2.0);
    let (left, right, top, bottom) = (x + dx, x + w - dx, y + dy, y + h - dy);
    for (cx, cy) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        let at = format!("at={cx},{cy}");
        let found = probes.observer_call(
            "app_find",
            serde_json::json!({ "locator": at, "limit": 200 }),
        )?;
        let drawn = found["matches"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["ref"] == m.reference.as_str());
        // An answer cut to its budget may have left it out though it is
        // drawn there: whether it is clipped is then not known.
        if !drawn && found["truncated"] == true {
            return Err(BackendError::Failed(format!(
                "what lies at {cx},{cy} is incomplete: its answer was cut to its budget \
                 before it said whether {} is drawn there",
                m.reference
            )));
        }
        if !drawn {
            return Ok(Some(format!("{cx},{cy}")));
        }
    }
    Ok(None)
}

/// Whether the text `reference` names is elided. An item that is not a
/// text has no `truncated` and shows what it shows.
fn elided(probes: &mut dyn crate::backend::Probes, reference: &str) -> Result<bool, BackendError> {
    let told = probes.observer_call(
        "app_inspect",
        serde_json::json!({ "target": reference, "all": true }),
    )?;
    let rows = told["properties"].as_array();
    let row = rows
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "truncated");
    // An answer cut to its budget may have left the row out: whether the
    // text is elided is then not known, and a check that took it for not
    // would pass on what it did not see.
    match row {
        Some(p) => Ok(p["value"] == "true"),
        None if told["truncated"] == true => Err(BackendError::Failed(format!(
            "the inspection of {reference} is incomplete: its answer was cut to its budget \
             before it said whether the text is elided"
        ))),
        None => Ok(false),
    }
}

// What a list or a panel is given says whether it can be used: a list
// squeezed to a sliver holds no row anyone can read.
#[then(regex = r#"^the observer's "((?:[^"\\]|\\.)+)" is at least (\d+) pixels (wide|high)$"#)]
fn observer_at_least(w: &mut LjosWorld, locator: String, least: f64, side: String) -> StepResult {
    let locator = unquoted(&locator);
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let all = boxes(probes, &locator)?;
            if all.is_empty() {
                return Ok(Some(format!("it shows nothing matching {locator}")));
            }
            let small: Vec<String> = all
                .iter()
                .filter(|m| if side == "wide" { m.at[2] } else { m.at[3] } < least - EDGE)
                .map(|m| m.to_string())
                .collect();
            Ok((!small.is_empty())
                .then(|| format!("these are less than {least} pixels {side}: {small:?}")))
        })
    })
}

// Words cut off or drawn over each other, and controls past the window's
// edge, are what the audit's rules measure: the words each text paints,
// those with no accessible name too, which the outline does not list. A
// code the audit does not have is refused by it, so a check cannot pass by
// naming nothing.
#[then(regex = r#"^the observer's own audit finds no ((?:"[a-z0-9-]+"(?:, | or )?)+)$"#)]
fn observer_audit_finds_none(w: &mut LjosWorld, codes: String) -> StepResult {
    let codes: Vec<&str> = codes.split('"').skip(1).step_by(2).collect();
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let report = probes.observer_call(
                "app_audit",
                serde_json::json!({ "matrix": "current", "rules": codes }),
            )?;
            // A rule that must act on the window (press, focus) is not
            // checked by an audit that only looks: it found nothing because
            // it did not look.
            let unchecked: Vec<&str> = report["notChecked"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str())
                .filter(|c| codes.contains(c))
                .collect();
            if !unchecked.is_empty() {
                return Err(BackendError::Failed(format!(
                    "its own audit did not check {unchecked:?}, which only an audit that acts does"
                )));
            }
            let found: Vec<String> = report["findings"]
                .as_array()
                .map(|all| {
                    all.iter()
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
            Ok((!found.is_empty()).then(|| format!("its own audit finds {found:?}")))
        })
    })
}

// Two words drawn over each other can be read as neither.
#[then(regex = r#"^no two "((?:[^"\\]|\\.)+)" in the observer overlap$"#)]
fn observer_none_overlap(w: &mut LjosWorld, locator: String) -> StepResult {
    let locator = unquoted(&locator);
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let all = boxes(probes, &locator)?;
            let mut over = Vec::new();
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    if a.overlaps(b) {
                        over.push(format!("{a} and {b}"));
                    }
                }
            }
            Ok((!over.is_empty()).then(|| format!("these overlap: {over:?}")))
        })
    })
}

// Where one item lies from another says how a view is laid out (a list
// beside its page, or above it) without a picture to compare.
#[then(
    regex = r#"^the observer's "((?:[^"\\]|\\.)+)" lies (above|left of) its "((?:[^"\\]|\\.)+)"$"#
)]
fn observer_lies(w: &mut LjosWorld, first: String, side: String, second: String) -> StepResult {
    let (first, second) = (unquoted(&first), unquoted(&second));
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let (Some(a), Some(b)) = (
                boxes(probes, &first)?.into_iter().next(),
                boxes(probes, &second)?.into_iter().next(),
            ) else {
                return Ok(Some(format!("it shows no {first} or no {second}")));
            };
            let lies = match side.as_str() {
                "above" => a.at[1] + a.at[3] <= b.at[1] + EDGE,
                _ => a.at[0] + a.at[2] <= b.at[0] + EDGE,
            };
            Ok((!lies).then(|| format!("{a} does not lie {side} {b}")))
        })
    })
}

// A scroll bar is as long as what it scrolls: one that ends short of it
// leaves a gap where something else was meant to be.
#[then(regex = r#"^the observer's "((?:[^"\\]|\\.)+)" ends level with its "((?:[^"\\]|\\.)+)"$"#)]
fn observer_ends_level(w: &mut LjosWorld, first: String, second: String) -> StepResult {
    let (first, second) = (unquoted(&first), unquoted(&second));
    w.each(|b| {
        observer_holds(b.probes()?, |probes| {
            let (Some(a), Some(b)) = (
                boxes(probes, &first)?.into_iter().next(),
                boxes(probes, &second)?.into_iter().next(),
            ) else {
                return Ok(Some(format!("it shows no {first} or no {second}")));
            };
            let level = (a.at[1] + a.at[3] - (b.at[1] + b.at[3])).abs() <= EDGE;
            Ok((!level).then(|| format!("{a} does not end level with {b}")))
        })
    })
}

/// How far a box may stand past another and still be beside it: layouts
/// place items on fractions of a pixel.
const EDGE: f64 = 0.5;

/// An item the observer's window shows, where it shows it.
struct Shown {
    /// The reference that names it to the observer's inspector.
    reference: String,
    role: String,
    name: String,
    /// x, y, width and height in the window's logical pixels, before any
    /// clipping.
    at: [f64; 4],
}

impl Shown {
    fn overlaps(&self, other: &Self) -> bool {
        let [ax, ay, aw, ah] = self.at;
        let [bx, by, bw, bh] = other.at;
        // Something with no width or no height covers nothing.
        aw > 0.0
            && ah > 0.0
            && bw > 0.0
            && bh > 0.0
            && ax + aw > bx + EDGE
            && bx + bw > ax + EDGE
            && ay + ah > by + EDGE
            && by + bh > ay + EDGE
    }
}

impl std::fmt::Display for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x, y, w, h] = self.at;
        write!(f, "{} {:?} at {x},{y} {w}x{h}", self.role, self.name)
    }
}

/// Everything `locator` finds in the observer's window. A locator that
/// finds more than one answer holds is refused, so that a check never
/// passes on a part of what it names.
fn boxes(
    probes: &mut dyn crate::backend::Probes,
    locator: &str,
) -> Result<Vec<Shown>, BackendError> {
    const ALL: u64 = 200;
    let found = probes.observer_call(
        "app_find",
        serde_json::json!({ "locator": locator, "limit": ALL }),
    )?;
    let total = found["total"].as_u64().unwrap_or(0);
    if total > ALL {
        return Err(BackendError::Failed(format!(
            "{locator} finds {total} items in the observer, more than the {ALL} one look \
             answers: name fewer"
        )));
    }
    let matches = found["matches"].as_array().cloned().unwrap_or_default();
    // An answer that does not fit its budget leaves out its last matches,
    // under the limit too: what they hold would go unchecked.
    if found["truncated"] == true || (matches.len() as u64) < total {
        return Err(BackendError::Failed(format!(
            "{locator} finds {total} items in the observer, of which one look answers {}: \
             name fewer",
            matches.len()
        )));
    }
    matches
        .iter()
        .map(|m| {
            let at = m["bounds"]
                .as_array()
                .filter(|b| b.len() == 4)
                .map(|b| std::array::from_fn(|i| b[i].as_f64().unwrap_or(f64::NAN)))
                .ok_or_else(|| BackendError::Failed(format!("a match has no bounds: {m}")))?;
            Ok(Shown {
                reference: m["ref"].as_str().unwrap_or("").to_owned(),
                role: m["role"].as_str().unwrap_or("").to_owned(),
                name: m["name"].as_str().unwrap_or("").to_owned(),
                at,
            })
        })
        .collect()
}

/// Looks with `look` until it finds nothing wrong, or until
/// [`OBSERVER_ANSWERS`] is over and the last thing it found is the
/// failure. A resize is laid out, and an answer shown, in a frame, so the
/// window is looked at again only after it ran one, never on a timer.
fn observer_holds(
    probes: &mut dyn crate::backend::Probes,
    mut look: impl FnMut(&mut dyn crate::backend::Probes) -> Result<Option<String>, BackendError>,
) -> Result<(), BackendError> {
    let deadline = std::time::Instant::now() + OBSERVER_ANSWERS;
    let told = probes.observer_call("app_watch", serde_json::json!({ "topics": ["frames"] }))?;
    let mut frames = told["counts"]["frames"].as_u64();
    loop {
        let Some(wrong) = look(probes)? else {
            return Ok(());
        };
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let Some(since) = frames.filter(|_| !left.is_zero()) else {
            return Err(BackendError::Failed(format!("the observer: {wrong}")));
        };
        let wait_ms = u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1);
        let told = probes.observer_call(
            "app_watch",
            serde_json::json!({
                "since": { "frames": since },
                "topics": ["frames"],
                "wait_ms": wait_ms,
                "settle_ms": 100,
            }),
        )?;
        let now = told["counts"]["frames"].as_u64();
        if now == frames {
            // No frame in all the time left: what it shows is final.
            return Err(BackendError::Failed(format!("the observer: {wrong}")));
        }
        frames = now.or(frames);
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
