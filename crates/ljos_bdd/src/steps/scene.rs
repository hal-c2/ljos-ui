//! Loading files and what a scenario does to and reads from their objects:
//! properties, methods, time, signals and the log.

use cucumber::{then, when};
use regex::Regex;

use crate::backend::{BackendError, LogLevel};
use crate::value;
use crate::world::LjosWorld;

use super::{StepResult, expect_value};

#[when(regex = r#"^"([^"]+)" is loaded$"#)]
fn file_loaded(w: &mut LjosWorld, name: String) -> StepResult {
    w.load(&name)
}

#[when(regex = r#"^the property "([^"]+)" of "([^"]+)" is set to (.+)$"#)]
fn property_set(w: &mut LjosWorld, property: String, object: String, v: String) -> StepResult {
    let v = value::parse(&v)?;
    w.each(|b| b.set(&object, &property, &v))
}

#[when(regex = r#"^the property "([^"]+)" of "([^"]+)" is bound to "([^"]+)"$"#)]
fn property_bound(
    w: &mut LjosWorld,
    property: String,
    object: String,
    expression: String,
) -> StepResult {
    w.each(|b| b.bind(&object, &property, &expression))
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called$"#)]
fn method_called(w: &mut LjosWorld, method: String, object: String) -> StepResult {
    w.each(|b| b.call(&object, &method, &[]).map(drop))
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called with (.+)$"#)]
fn method_called_with(
    w: &mut LjosWorld,
    method: String,
    object: String,
    args: String,
) -> StepResult {
    let args = value::parse_list(&args)?;
    w.each(|b| b.call(&object, &method, &args).map(drop))
}

#[when(regex = r"^(\d+) milliseconds? pass(?:es)?$")]
fn milliseconds_pass(w: &mut LjosWorld, ms: u64) -> StepResult {
    w.each(|b| b.advance(ms))
}

#[when(regex = r"^(\d+(?:\.\d+)?) seconds? pass(?:es)?$")]
fn seconds_pass(w: &mut LjosWorld, s: f64) -> StepResult {
    // Rounded to the millisecond; sub-millisecond time has no meaning to a UI.
    let ms = (s * 1000.0).round() as u64;
    w.each(|b| b.advance(ms))
}

// The value must look like JSON so that `is not …`, `is at least …` stay unambiguous.
#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is (true|false|null|[-0-9"\[{].*)$"#)]
fn property_is(
    w: &mut LjosWorld,
    property: String,
    object: String,
    expected: String,
) -> StepResult {
    let expected = value::parse(&expected)?;
    expect_value(w, &object, &property, |actual| {
        if value::same(&expected, actual) {
            Ok(())
        } else {
            Err(format!("expected {expected}, found {actual}"))
        }
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is not (.+)$"#)]
fn property_is_not(
    w: &mut LjosWorld,
    property: String,
    object: String,
    unwanted: String,
) -> StepResult {
    let unwanted = value::parse(&unwanted)?;
    expect_value(w, &object, &property, |actual| {
        if value::same(&unwanted, actual) {
            Err(format!("expected anything but {unwanted}"))
        } else {
            Ok(())
        }
    })
}

#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is at (least|most) (-?\d+(?:\.\d+)?)$"#)]
fn property_bound_check(
    w: &mut LjosWorld,
    property: String,
    object: String,
    which: String,
    limit: f64,
) -> StepResult {
    let least = which == "least";
    expect_value(w, &object, &property, |actual| {
        let n = value::number(actual).ok_or_else(|| format!("{actual} is not a number"))?;
        let ok = if least { n >= limit } else { n <= limit };
        if ok {
            Ok(())
        } else {
            Err(format!("expected at {which} {limit}, found {n}"))
        }
    })
}

// For values an easing curve computes: the two toolkits agree to rounding, not bit for bit.
#[then(regex = r#"^the property "([^"]+)" of "([^"]+)" is within ([\d.]+) of (-?[\d.]+)$"#)]
fn property_within(
    w: &mut LjosWorld,
    property: String,
    object: String,
    tolerance: f64,
    expected: f64,
) -> StepResult {
    expect_value(w, &object, &property, |actual| {
        let n = value::number(actual).ok_or_else(|| format!("{actual} is not a number"))?;
        if (n - expected).abs() <= tolerance {
            Ok(())
        } else {
            Err(format!(
                "expected {expected} give or take {tolerance}, found {n}"
            ))
        }
    })
}

#[then(regex = r#"^setting the property "([^"]+)" of "([^"]+)" to (.+) is rejected$"#)]
fn set_rejected(w: &mut LjosWorld, property: String, object: String, v: String) -> StepResult {
    let v = value::parse(&v)?;
    w.each(|b| match b.set(&object, &property, &v) {
        Err(BackendError::Failed(_)) => Ok(()),
        Err(other) => Err(other),
        Ok(()) => Err(BackendError::Failed(format!(
            "setting {property:?} of {object:?} was accepted"
        ))),
    })
}

#[then(regex = r#"^loading "([^"]+)" is rejected$"#)]
fn load_rejected(w: &mut LjosWorld, name: String) -> StepResult {
    w.load_rejected(&name)
}

#[then(regex = r#"^the object "([^"]+)" (exists|does not exist)$"#)]
fn object_exists(w: &mut LjosWorld, object: String, which: String) -> StepResult {
    let want = which == "exists";
    w.each(|b| {
        if b.exists(&object)? == want {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "object {object:?} {}",
                if want { "is missing" } else { "exists" }
            )))
        }
    })
}

#[when(regex = r#"^the object "([^"]+)" is destroyed$"#)]
fn object_destroyed(w: &mut LjosWorld, object: String) -> StepResult {
    w.each(|b| {
        if b.call(&object, "destroy", &[]).is_err() {
            b.bind(&object, "visible", "(destroy(), false)")?;
        }
        b.advance(16)?;
        if b.exists(&object)? {
            Err(BackendError::Failed(format!(
                "object {object:?} was not destroyed"
            )))
        } else {
            Ok(())
        }
    })
}

#[then(regex = r#"^the signal "([^"]+)" of "([^"]+)" has fired once$"#)]
fn signal_fired_once(w: &mut LjosWorld, signal: String, object: String) -> StepResult {
    let candidates = [
        format!("{signal}Count"),
        format!("{signal}Fired"),
        format!("{signal}_count"),
        format!("{signal}_fired"),
        signal.clone(),
    ];
    w.each(|b| {
        let mut count = None;
        for prop in &candidates {
            if let Ok(v) = b.get(&object, prop)
                && let Some(n) = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
            {
                count = Some(n);
                break;
            }
        }
        if count == Some(1) {
            Ok(())
        } else if let Some(n) = count {
            Err(BackendError::Failed(format!(
                "signal {signal:?} of {object:?} has fired {n} times, expected 1"
            )))
        } else {
            Err(BackendError::Failed(format!(
                "cannot determine if signal {signal:?} of {object:?} fired (checked properties: {candidates:?})"
            )))
        }
    })
}

fn log_matches(w: &mut LjosWorld, pattern: &str, level: LogLevel, want: bool) -> StepResult {
    let re = Regex::new(pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    w.each(|b| {
        let log = b.log()?;
        let hit = log
            .iter()
            .any(|e| e.level == level && re.is_match(&e.message));
        if hit == want {
            return Ok(());
        }
        let seen: Vec<String> = log
            .iter()
            .map(|e| format!("  {}: {}", e.level, e.message))
            .collect();
        Err(BackendError::Failed(format!(
            "expected {} {level} matching {pattern:?}; log was:\n{}",
            if want { "a" } else { "no" },
            seen.join("\n")
        )))
    })
}

#[then(regex = r#"^the log has a warning matching "([^"]+)"$"#)]
fn log_has_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Warning, true)
}

#[then(regex = r#"^the log has an info matching "([^"]+)"$"#)]
fn log_has_info(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Info, true)
}

#[then(regex = r#"^the log has no info matching "([^"]+)"$"#)]
fn log_has_no_info(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Info, false)
}

#[then(regex = r#"^the log has (\d+) (infos?|warnings?) matching "([^"]+)"$"#)]
fn log_has_infos(w: &mut LjosWorld, count: usize, kind: String, pattern: String) -> StepResult {
    let re = Regex::new(&pattern).map_err(|e| format!("bad pattern {pattern:?}: {e}"))?;
    let level = if kind.starts_with("info") {
        LogLevel::Info
    } else {
        LogLevel::Warning
    };
    w.each(|b| {
        let log = b.log()?;
        let hits = log
            .iter()
            .filter(|e| e.level == level && re.is_match(&e.message))
            .count();
        if hits == count {
            return Ok(());
        }
        let seen: Vec<String> = log
            .iter()
            .map(|e| format!("  {}: {}", e.level, e.message))
            .collect();
        Err(BackendError::Failed(format!(
            "expected {count} {kind} matching {pattern:?}, found {hits}; log was:\n{}",
            seen.join("\n")
        )))
    })
}

#[then(regex = r#"^the log has no warning matching "([^"]+)"$"#)]
fn log_has_no_warning(w: &mut LjosWorld, pattern: String) -> StepResult {
    log_matches(w, &pattern, LogLevel::Warning, false)
}

#[then("the log has no warnings")]
fn log_clean(w: &mut LjosWorld) -> StepResult {
    log_matches(w, ".*", LogLevel::Warning, false)
}

#[when(regex = r#"^the method "([^"]+)" of "([^"]+)" is called (\d+) times$"#)]
fn method_called_often(
    w: &mut LjosWorld,
    method: String,
    object: String,
    times: u32,
) -> StepResult {
    w.each(|b| (0..times).try_for_each(|_| b.call(&object, &method, &[]).map(drop)))
}
