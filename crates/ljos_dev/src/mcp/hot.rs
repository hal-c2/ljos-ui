//! `app_frames`' `bindings`: which bindings and handlers run most and take
//! longest, while a client asks.
//!
//! The engine profiles them ([`Engine::set_profiled`]) exactly while a
//! client records: each with where it is written, the property it writes,
//! its runs and their time, its own time and what set its runs off. The
//! binding loops the graph broke are told whether or not anyone records:
//! the engine keeps the first few it met, start-up included.

use std::fmt::Write as _;
use std::time::Duration;

use ljos_engine::{Engine, HotEntry, HotKind};
use ljos_mcp_core::{SurfaceError, ToolReply};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::frames::us;
use super::held::Held;
use super::objects::handle;

/// The most rows one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 60;
/// The most rows one answer lists.
const MAX_ROWS: usize = 600;
/// The causes told per row.
const MAX_CAUSES: usize = 5;

/// How `bindings` orders its rows.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotSort {
    /// The most time in all runs, nested runs included.
    #[default]
    Total,
    /// The most time in all runs less the runs nested in them.
    Own,
    /// The most runs.
    Runs,
    /// The longest single run.
    Longest,
}

impl HotSort {
    fn name(self) -> &'static str {
        match self {
            HotSort::Total => "total",
            HotSort::Own => "own",
            HotSort::Runs => "runs",
            HotSort::Longest => "longest",
        }
    }

    fn label(self) -> &'static str {
        match self {
            HotSort::Total => "total time",
            HotSort::Own => "own time",
            HotSort::Runs => "runs",
            HotSort::Longest => "the longest run",
        }
    }

    fn key(self, e: &HotEntry) -> (Duration, u64) {
        match self {
            HotSort::Total => (e.total, e.runs),
            HotSort::Own => (e.own, e.runs),
            HotSort::Runs => (Duration::from_nanos(e.runs), e.total.as_nanos() as u64),
            HotSort::Longest => (e.longest, e.runs),
        }
    }
}

/// Who asked for the profiling.
#[derive(Default)]
pub(crate) struct Hot {
    held: Held,
}

fn kind(e: &HotEntry) -> &'static str {
    match e.kind {
        HotKind::Binding => "binding",
        HotKind::Handler => "handler",
    }
}

fn ms(d: Duration) -> String {
    format!("{:.3} ms", d.as_secs_f64() * 1000.0)
}

fn row(e: &HotEntry) -> Value {
    json!({
        "kind": kind(e),
        "object": handle(e.object),
        "type": e.type_name,
        "name": e.name,
        "at": e.at,
        "runs": e.runs,
        "total_us": us(e.total),
        "own_us": us(e.own),
        "longest_us": us(e.longest),
        "mean_us": if e.runs == 0 { 0.0 } else { us(e.total) as f64 / e.runs as f64 },
        "causes": e.causes.iter().take(MAX_CAUSES).map(|c| json!({
            "object": handle(c.object),
            "type": c.type_name,
            "name": c.name,
            "runs": c.runs,
        })).collect::<Vec<_>>(),
    })
}

fn line(e: &HotEntry) -> String {
    let mut line = format!(
        "{:>6} runs {:>12} (own {}, longest {}) {} {}.{} {}",
        e.runs,
        ms(e.total),
        ms(e.own),
        ms(e.longest),
        kind(e),
        e.type_name,
        e.name,
        handle(e.object),
    );
    if let Some(at) = &e.at {
        let _ = write!(line, " at {at}");
    }
    if !e.causes.is_empty() {
        let causes: Vec<String> = e
            .causes
            .iter()
            .take(MAX_CAUSES)
            .map(|c| {
                format!(
                    "{}.{} {} ({})",
                    c.type_name,
                    c.name,
                    handle(c.object),
                    c.runs
                )
            })
            .collect();
        let _ = write!(line, " set off by {}", causes.join(", "));
    }
    line
}

impl Hot {
    pub(crate) fn recording(&self) -> bool {
        self.held.any()
    }

    /// Starts or stops profiling for whoever asks, so that the engine
    /// profiles exactly while someone records.
    pub(crate) fn ask(
        &mut self,
        engine: &Engine,
        record: bool,
        holder: Option<&str>,
    ) -> Result<(), SurfaceError> {
        self.held.ask(record, holder, "profile bindings")?;
        engine.set_profiled(self.recording());
        Ok(())
    }

    /// Answers `app_frames` with `bindings`.
    pub(crate) fn answer(&self, engine: &Engine, sort: HotSort, max: Option<usize>) -> ToolReply {
        let max = max.unwrap_or(DEFAULT_MAX).min(MAX_ROWS);
        let profile = engine.profile();
        let mut entries = profile.entries;
        entries.sort_by_key(|e| std::cmp::Reverse(sort.key(e)));
        let count = entries.len();
        entries.truncate(max);
        let loops = engine.binding_loops();
        let window = profile.since.map(|s| s.elapsed());
        let mut text = String::new();
        match window {
            Some(w) => {
                let _ = writeln!(
                    text,
                    "profiling bindings for {:.1} s; {count} bindings and handlers ran, \
                     sorted by {}",
                    w.as_secs_f64(),
                    sort.label()
                );
            }
            None => {
                text.push_str("not profiling bindings; `bindings: true, record: true` starts it\n")
            }
        }
        for e in &entries {
            let _ = writeln!(text, "{}", line(e));
        }
        if count > entries.len() {
            let _ = writeln!(text, "{} more not listed", count - entries.len());
        }
        if profile.dropped_entries > 0 {
            let _ = writeln!(
                text,
                "{} runs of bindings past the first {} were not told apart",
                profile.dropped_entries, count
            );
        }
        for l in &loops {
            let cycle: Vec<String> = l
                .cycle
                .iter()
                .map(|(o, t, p)| format!("{t}.{p} {}", handle(*o)))
                .collect();
            let _ = writeln!(
                text,
                "binding loop ({}×): {}.{} {}{} through {}",
                l.times,
                l.type_name,
                l.property,
                handle(l.object),
                l.at.as_deref()
                    .map(|a| format!(" at {a}"))
                    .unwrap_or_default(),
                cycle.join(" → ")
            );
        }
        let value = json!({
            "recording": self.recording(),
            "window_ms": window.map(|w| (w.as_secs_f64() * 10_000.0).round() / 10.0),
            "sort": sort.name(),
            "bindings": entries.iter().map(row).collect::<Vec<_>>(),
            "count": count,
            "more": count > entries.len(),
            "dropped": profile.dropped_entries,
            "loops": loops.iter().map(|l| json!({
                "object": handle(l.object),
                "type": l.type_name,
                "property": l.property,
                "at": l.at,
                "times": l.times,
                "cycle": l.cycle.iter().map(|(o, t, p)| json!({
                    "object": handle(*o),
                    "type": t,
                    "property": p,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        ToolReply::from(value).with_text(text)
    }
}
