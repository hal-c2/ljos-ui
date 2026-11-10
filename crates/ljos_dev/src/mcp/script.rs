//! `app_frames`' `script`: where the bindings and handlers of a running
//! application spend their time in JavaScript enough to matter, and the
//! cheaper form of each (docs/script-cost.md).
//!
//! It reads the recording `bindings` makes ([`Engine::set_profiled`]): the
//! engine counts each run's time in its own script, less what it set off
//! that was measured on its own. Nothing is measured while nobody records,
//! and a hint is never an error: script is allowed, the hint says where it
//! costs and what is cheaper.
//!
//! The UI thread names the place, the cost and the form it can always
//! give, moving the work into Rust. A build that links the authoring tools
//! registers a [`ScriptAdvisor`] (`ljos run` does), and the inspector asks
//! it, away from the UI thread, for the verified tier rewrite or the
//! declarative form of each place, which it prefers.

use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ljos_engine::{Engine, HotEntry, HotKind};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde::Serialize;
use serde_json::{Value, json};

use super::frames::us;
use super::objects::handle;

/// DECISION: one run that spends 2 ms in script is worth a hint: an eighth
/// of a 60 Hz frame (16.7 ms), so a few such runs in one frame or one
/// interaction miss it, and half the slice after which the engine logs a
/// script as slow (4 ms), so a hint comes before that warning does. A
/// small binding merely written in JavaScript runs far below it.
pub const MIN_RUN_MS: f64 = 2.0;
/// DECISION: script that takes a tenth of what all bindings and handlers
/// took together is worth a hint though no run of it is long (a delegate's
/// binding run a thousand times)...
pub const MIN_SHARE_PERCENT: f64 = 10.0;
/// ... and at least this much in all, so a recording in which almost
/// nothing ran hints nothing.
pub const SHARE_FLOOR: Duration = Duration::from_millis(1);
/// The guide every hint links to.
pub const GUIDE: &str = "docs/script-cost.md";

/// The most hints one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 20;
/// The most hints one answer lists.
const MAX_HINTS: usize = 200;

/// Which cheaper form a hint gives, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheaperForm {
    /// A form of the expression that means the same and runs in a lower
    /// tier, checked against the compiler (`binding_tier_explain`).
    Rewrite,
    /// A declaration that says what the script does (`decl-*`, docs/declarative.md).
    Declarative,
    /// The work moved into Rust: a native type, or the shipped files
    /// compiled ahead of time.
    Rust,
}

/// What is cheaper than the script of one binding or handler.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cheaper {
    pub form: CheaperForm,
    /// What to do, in a sentence or two.
    pub text: String,
    /// For a rewrite: what to write instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// For a rewrite: the tier it runs in (`0` or `1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// For a declarative form: the lint that names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// For a declarative form: its guideline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide: Option<String>,
    /// Why the expression is left to JavaScript, as the compiler says it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Cheaper {
    /// The form there always is, for a handler or a binding.
    pub fn rust(handler: bool, reason: Option<String>) -> Self {
        let text = if handler {
            "Move the heavy work into Rust: a native type (ARCHITECTURE.md §5.5) whose \
             method the handler calls, or, for a file the application ships, compile it ahead \
             of time (§5.4) where the handler keeps to the typed subset."
        } else {
            "Move the heavy work into Rust: a native type (ARCHITECTURE.md §5.5) that does it \
             and gives the result as a property the binding reads, or, for a file the \
             application ships, compile it ahead of time (§5.4) where the expression keeps to \
             the typed subset."
        };
        Self {
            form: CheaperForm::Rust,
            text: text.to_owned(),
            source: None,
            tier: None,
            code: None,
            guide: None,
            reason,
        }
    }
}

/// Where a hinted binding or handler is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPlace {
    /// The file as the engine names it: a `file://` URL for a file on disk.
    pub file: String,
    /// 1-based.
    pub line: u32,
    pub column: u32,
    pub handler: bool,
}

/// Finds a cheaper form than the script written at a place. Registered
/// once by a build that links the authoring tools; called away from the UI
/// thread, and may compile.
pub trait ScriptAdvisor: Send + Sync + 'static {
    /// One answer per place, in order: `None` where it knows nothing
    /// better than moving the work into Rust.
    fn advise(&self, places: &[ScriptPlace]) -> Vec<Option<Cheaper>>;
}

static ADVISOR: OnceLock<Arc<dyn ScriptAdvisor>> = OnceLock::new();

/// Registers the advisor for every inspector of the process. False if one
/// was registered already, which stays.
pub fn set_script_advisor(advisor: Arc<dyn ScriptAdvisor>) -> bool {
    ADVISOR.set(advisor).is_ok()
}

fn advisor() -> Option<Arc<dyn ScriptAdvisor>> {
    ADVISOR.get().cloned()
}

/// When a binding or handler is hinted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Thresholds {
    run: Duration,
    /// Of the busy time, 0 to 1.
    share: f64,
}

impl Thresholds {
    pub(crate) fn of(
        min_run_ms: Option<f64>,
        min_share: Option<f64>,
    ) -> Result<Self, SurfaceError> {
        let ms = min_run_ms.unwrap_or(MIN_RUN_MS);
        // Refuses NaN, infinity, a negative time and one too long for a
        // `Duration`, which would otherwise panic on the UI thread.
        let run = Duration::try_from_secs_f64(ms / 1000.0).map_err(|_| {
            SurfaceError::invalid_argument(format!(
                "`min_run_ms` is a time of at least 0 milliseconds that a \
                 duration can hold, not {ms}, which is negative, not a number \
                 or too large"
            ))
        })?;
        let share = min_share.unwrap_or(MIN_SHARE_PERCENT);
        if !share.is_finite() || !(0.0..=100.0).contains(&share) {
            return Err(SurfaceError::invalid_argument(format!(
                "`min_share` is a percentage from 0 to 100, not {share}"
            )));
        }
        Ok(Self {
            run,
            share: share / 100.0,
        })
    }

    /// Why `e` is hinted: `run`, `share`, both or neither.
    fn criteria(&self, e: &HotEntry, busy: Duration) -> Vec<&'static str> {
        let mut why = Vec::new();
        if e.script_runs == 0 {
            return why;
        }
        if e.longest_script >= self.run {
            why.push("run");
        }
        if !busy.is_zero()
            && e.script >= SHARE_FLOOR
            && e.script.as_secs_f64() >= self.share * busy.as_secs_f64()
        {
            why.push("share");
        }
        why
    }
}

fn kind(e: &HotEntry) -> &'static str {
    match e.kind {
        HotKind::Binding => "binding",
        HotKind::Handler => "handler",
    }
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

fn percent(part: Duration, of: Duration) -> f64 {
    if of.is_zero() {
        return 0.0;
    }
    (part.as_secs_f64() / of.as_secs_f64() * 1000.0).round() / 10.0
}

/// `file:///a/Main.qml:5:3` as its file, line and column.
fn place(at: &str) -> Option<(&str, u32, u32)> {
    let mut parts = at.rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let line = parts.next()?.parse().ok()?;
    Some((parts.next()?, line, column))
}

fn message(e: &HotEntry, criteria: &[&str], busy: Duration, t: &Thresholds) -> String {
    let mut parts = Vec::new();
    if criteria.contains(&"run") {
        parts.push(format!(
            "one run spent {} in script, a hint from {}",
            ms(e.longest_script),
            ms(t.run)
        ));
    }
    if criteria.contains(&"share") {
        parts.push(format!(
            "its script took {} % of the {} all bindings and handlers took ({} in {} runs), a \
             hint from {} %",
            percent(e.script, busy),
            ms(busy),
            ms(e.script),
            e.script_runs,
            (t.share * 1000.0).round() / 10.0
        ));
    }
    format!(
        "{} {}.{}: {}",
        kind(e),
        e.type_name,
        e.name,
        parts.join("; ")
    )
}

fn hint(e: &HotEntry, criteria: Vec<&'static str>, busy: Duration, t: &Thresholds) -> Value {
    let (file, line, column) = match e.at.as_deref().and_then(place) {
        Some((f, l, c)) => (Some(f), Some(l), Some(c)),
        None => (None, None, None),
    };
    json!({
        "code": "script-cost",
        "severity": "hint",
        "kind": kind(e),
        "object": handle(e.object),
        "type": e.type_name,
        "name": e.name,
        "at": e.at,
        "file": file,
        "line": line,
        "column": column,
        "runs": e.runs,
        "script_runs": e.script_runs,
        "script_us": us(e.script),
        "longest_script_us": us(e.longest_script),
        "mean_script_us": us(e.script) as f64 / e.script_runs.max(1) as f64,
        "share_percent": percent(e.script, busy),
        "message": message(e, &criteria, busy, t),
        "criteria": criteria,
        "cheaper": Cheaper::rust(e.kind == HotKind::Handler, None),
        "guide": GUIDE,
    })
}

/// Answers `app_frames` with `script`, with the form the UI thread can
/// give; [`advise`] puts better ones in.
pub(crate) fn answer(
    engine: &Engine,
    recording: bool,
    t: Thresholds,
    max: Option<usize>,
) -> ToolReply {
    let max = max.unwrap_or(DEFAULT_MAX).min(MAX_HINTS);
    let profile = engine.profile();
    let busy = profile.busy;
    let scripted = profile.entries.iter().filter(|e| e.script_runs > 0).count();
    let script: Duration = profile.entries.iter().map(|e| e.script).sum();
    let mut hinted: Vec<(&HotEntry, Vec<&'static str>)> = profile
        .entries
        .iter()
        .filter_map(|e| {
            let why = t.criteria(e, busy);
            (!why.is_empty()).then_some((e, why))
        })
        .collect();
    hinted.sort_by(|a, b| b.0.script.cmp(&a.0.script).then(b.0.runs.cmp(&a.0.runs)));
    let count = hinted.len();
    hinted.truncate(max);
    let window = profile.since.map(|s| s.elapsed());
    let value = json!({
        "recording": recording,
        "window_ms": window.map(|w| (w.as_secs_f64() * 10_000.0).round() / 10.0),
        "busy_us": us(busy),
        "script_us": us(script),
        "scripted": scripted,
        "thresholds": {
            "min_run_ms": t.run.as_secs_f64() * 1000.0,
            "min_share": (t.share * 1000.0).round() / 10.0,
            "share_floor_ms": SHARE_FLOOR.as_secs_f64() * 1000.0,
        },
        "hints": hinted
            .into_iter()
            .map(|(e, why)| hint(e, why, busy, &t))
            .collect::<Vec<_>>(),
        "count": count,
        "more": count > max,
        "dropped": profile.dropped_entries,
    });
    let text = text(&value);
    ToolReply::from(value).with_text(text)
}

/// The text of an answer, from its structured content, so that it says
/// what the advisor put in.
fn text(v: &Value) -> String {
    let mut text = String::new();
    let us_ms = |p: &str| v[p].as_f64().unwrap_or(0.0) / 1000.0;
    if v["window_ms"].is_null() {
        text.push_str(
            "not measuring script; `script: true, record: true` starts it (the recording \
             `bindings` reads)\n",
        );
        return text;
    }
    let t = &v["thresholds"];
    let _ = writeln!(
        text,
        "measuring script for {:.1} s: {} bindings and handlers ran script, {:.2} ms of the \
         {:.2} ms all of them took; a hint is one run of {} ms in script or {} % of that time \
         (and {} ms in all)",
        v["window_ms"].as_f64().unwrap_or(0.0) / 1000.0,
        v["scripted"],
        us_ms("script_us"),
        us_ms("busy_us"),
        t["min_run_ms"],
        t["min_share"],
        t["share_floor_ms"],
    );
    let hints = v["hints"].as_array().map(Vec::as_slice).unwrap_or_default();
    if hints.is_empty() {
        text.push_str("no script costs enough for a hint\n");
    }
    for h in hints {
        let at = h["at"]
            .as_str()
            .map(|a| format!(" at {a}"))
            .unwrap_or_default();
        let _ = writeln!(
            text,
            "hint script-cost: {}{at} ({})",
            h["message"].as_str().unwrap_or_default(),
            h["object"].as_str().unwrap_or_default(),
        );
        let c = &h["cheaper"];
        let mut cheaper = format!(
            "  cheaper ({}): {}",
            c["form"].as_str().unwrap_or_default(),
            c["text"].as_str().unwrap_or_default()
        );
        if let Some(source) = c["source"].as_str() {
            let _ = write!(cheaper, " Write: {source}");
        }
        if let Some(guide) = c["guide"].as_str() {
            let _ = write!(cheaper, " See {guide}.");
        }
        if let Some(reason) = c["reason"].as_str() {
            let _ = write!(cheaper, " (In JavaScript because: {reason}.)");
        }
        let _ = writeln!(text, "{cheaper}");
    }
    let (count, listed) = (v["count"].as_u64().unwrap_or(0), hints.len() as u64);
    if count > listed {
        let _ = writeln!(text, "{} more not listed", count - listed);
    }
    text
}

/// Puts the registered advisor's forms into an answer of [`answer`], away
/// from the UI thread. Without an advisor, or for a hint it knows nothing
/// better for, the answer keeps the form it has.
pub(crate) async fn advise(mut reply: ToolReply) -> ToolReply {
    let Some(advisor) = advisor() else {
        return reply;
    };
    let Some(hints) = reply.structured["hints"].as_array() else {
        return reply;
    };
    let places: Vec<(usize, ScriptPlace)> = hints
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            Some((
                i,
                ScriptPlace {
                    file: h["file"].as_str()?.to_owned(),
                    line: u32::try_from(h["line"].as_u64()?).ok()?,
                    column: u32::try_from(h["column"].as_u64()?).ok()?,
                    handler: h["kind"] == "handler",
                },
            ))
        })
        .collect();
    if places.is_empty() {
        return reply;
    }
    let asked: Vec<ScriptPlace> = places.iter().map(|(_, p)| p.clone()).collect();
    // It compiles: never on the async workers that answer other calls.
    let found = match tokio::runtime::Handle::try_current() {
        Ok(_) => tokio::task::spawn_blocking(move || advisor.advise(&asked))
            .await
            .unwrap_or_default(),
        Err(_) => advisor.advise(&asked),
    };
    for ((i, _), cheaper) in places.iter().zip(found) {
        if let Some(cheaper) = cheaper
            && let Ok(value) = serde_json::to_value(cheaper)
        {
            reply.structured["hints"][*i]["cheaper"] = value;
        }
    }
    reply.text = Some(text(&reply.structured));
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_split_from_the_right() {
        assert_eq!(
            place("file:///a/b c/Main.qml:5:3"),
            Some(("file:///a/b c/Main.qml", 5, 3))
        );
        assert_eq!(place("Main.qml:12:1"), Some(("Main.qml", 12, 1)));
        assert_eq!(place("Main.qml"), None);
    }

    #[test]
    fn thresholds_are_checked() {
        assert!(Thresholds::of(None, None).is_ok());
        assert!(Thresholds::of(Some(0.0), Some(0.0)).is_ok());
        assert!(Thresholds::of(Some(-1.0), None).is_err());
        assert!(Thresholds::of(Some(f64::NAN), None).is_err());
        assert!(Thresholds::of(Some(f64::INFINITY), None).is_err());
        assert!(Thresholds::of(Some(1e30), None).is_err());
        assert!(Thresholds::of(None, Some(100.5)).is_err());
    }

    fn entry(script_runs: u64, script_ms: u64, longest_ms: u64) -> HotEntry {
        HotEntry {
            kind: HotKind::Binding,
            object: Default::default(),
            type_name: "Item".into(),
            name: "x".into(),
            at: None,
            runs: script_runs,
            total: Duration::from_millis(script_ms),
            own: Duration::from_millis(script_ms),
            longest: Duration::from_millis(longest_ms),
            script: Duration::from_millis(script_ms),
            script_runs,
            longest_script: Duration::from_millis(longest_ms),
            causes: Vec::new(),
        }
    }

    #[test]
    fn a_long_run_or_a_large_share_is_hinted() {
        let t = Thresholds::of(None, None).unwrap();
        let busy = Duration::from_millis(100);
        assert_eq!(t.criteria(&entry(1, 3, 3), busy), ["run"]);
        assert_eq!(t.criteria(&entry(100, 20, 1), busy), ["share"]);
        assert_eq!(t.criteria(&entry(10, 50, 5), busy), ["run", "share"]);
        assert!(t.criteria(&entry(10, 5, 1), busy).is_empty());
        // A tenth of almost nothing is not worth a hint.
        let little = Duration::from_micros(900);
        assert!(t.criteria(&entry(5, 0, 0), little).is_empty());
        // What never ran script is never hinted, whatever the threshold.
        let zero = Thresholds::of(Some(0.0), Some(0.0)).unwrap();
        assert!(zero.criteria(&entry(0, 0, 0), busy).is_empty());
    }
}
