//! `app_frames`' `wakes` with `wake` or `last_input`: what one wake did,
//! in the order it did it. The handlers that ran, the properties that
//! changed with their old and new values, the bindings that ran again, and
//! the items the scene laid out, placed and redrew, each with its time.
//! It answers why one click took 40 ms, or why it redrew everything.
//!
//! The journal is the engine's ([`Engine::take_journal`]), taken at the
//! end of each turn while wakes are kept; the latest
//! [`super::wakes::MAX_DETAILED`] wakes that were not an agent's own keep
//! it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Duration;

use ljos_engine::{Engine, JournalEntry, JournalKind};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

use super::objects::handle;
use super::wakes::{MAX_DETAILED, Wakes};

/// The most entries listed unless told otherwise.
const DEFAULT_MAX: usize = 200;
/// The slowest runs named apart.
const SLOWEST: usize = 5;

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e5).round() / 100.0
}

fn scene_step(kind: JournalKind) -> bool {
    matches!(
        kind,
        JournalKind::LaidOut | JournalKind::Placed | JournalKind::Redrawn
    )
}

fn short(engine: &Engine, v: &Option<ljos_engine::Value>) -> String {
    let Some(v) = v else {
        return "none".to_owned();
    };
    let mut s = super::act::to_json(engine, v, 0).to_string();
    if s.chars().count() > 60 {
        s = s.chars().take(59).collect::<String>() + "…";
    }
    s
}

pub(super) fn answer(
    wakes: &Wakes,
    engine: &Engine,
    seq: Option<u64>,
    max: Option<usize>,
) -> Result<ToolReply, SurfaceError> {
    let entry = wakes.pick(seq).ok_or_else(|| {
        SurfaceError::invalid_argument(match seq {
            Some(seq) => format!("`app_frames`: no wake {seq} is kept"),
            None => "`app_frames`: no wake that input caused is kept; `record: true` keeps them"
                .to_owned(),
        })
    })?;
    let journal = entry.journal.as_deref().ok_or_else(|| {
        SurfaceError::invalid_argument(format!(
            "`app_frames`: wake {} did not keep what it did: only the latest {MAX_DETAILED} wakes that were not requests do",
            entry.seq()
        ))
    })?;
    let max = max.unwrap_or(DEFAULT_MAX);
    let first = journal.entries.iter().find_map(|e| e.started);
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for e in &journal.entries {
        *counts.entry(e.kind.name()).or_default() += 1;
    }

    let to_json = |e: &JournalEntry| {
        json!({
            "kind": e.kind.name(),
            "depth": e.depth,
            "object": handle(e.object),
            "type": e.type_name,
            "name": e.name,
            "at": e.at,
            "old": e.old.as_ref().map(|v| super::act::to_json(engine, v, 0)),
            "new": e.new.as_ref().map(|v| super::act::to_json(engine, v, 0)),
            "started_ms": e.started.zip(first).map(|(s, f)| ms(s.saturating_duration_since(f))),
            "took_us": e.took.map(|t| t.as_micros() as u64),
        })
    };
    let entries: Vec<Value> = journal.entries.iter().take(max).map(to_json).collect();

    let mut slowest: Vec<&JournalEntry> = journal
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                JournalKind::Handler | JournalKind::Binding | JournalKind::Host
            )
        })
        .filter(|e| e.took.is_some())
        .collect();
    slowest.sort_by_key(|e| std::cmp::Reverse(e.took));
    slowest.truncate(SLOWEST);

    // Host work runs outside the frames: without it a wake spent in a
    // message's Rust code would show only an unexplained total.
    let host: Duration = journal
        .entries
        .iter()
        .filter(|e| e.kind == JournalKind::Host && e.depth == 0)
        .filter_map(|e| e.took)
        .sum();
    let s = entry.steps();
    let steps = json!({
        "host_ms": ms(host),
        "frames": s.frames,
        "timers_ms": ms(s.timers),
        "animations_ms": ms(s.animations),
        "layout_ms": ms(s.layout),
        "create_ms": ms(s.create),
        "scene_ms": ms(s.scene),
        "other_ms": ms(s.other),
        "total_ms": ms(s.total),
        "bindings_ms": ms(s.bindings),
        "binding_runs": s.binding_runs,
        "text_ms": ms(s.text),
        "text_layouts": s.text_layouts,
    });

    let mut text = format!(
        "wake {} at {:.3} s, caused by {}, led to {}; frames took {:.2} ms (layout {:.2}, scene {:.2}, bindings {:.2})\n",
        entry.seq(),
        entry.at().as_secs_f64(),
        entry.causes().join(", "),
        entry.led_to(),
        ms(s.total),
        ms(s.layout),
        ms(s.scene),
        ms(s.bindings),
    );
    if !host.is_zero() {
        let _ = writeln!(text, "host code outside the frames took {:.2} ms", ms(host));
    }
    let counted: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
    if counted.is_empty() {
        text.push_str("it ran no handler or binding and changed nothing\n");
    } else {
        let _ = writeln!(text, "it did: {}", counted.join(", "));
    }
    // The scene's steps are many and alike: a run of them is one line.
    let mut listed = 0;
    let mut i = 0;
    let all = &journal.entries;
    while i < all.len() && listed < max {
        let e = &all[i];
        let pad = "  ".repeat(e.depth as usize + 1);
        if scene_step(e.kind) {
            let mut j = i;
            let mut took = Duration::ZERO;
            while j < all.len() && all[j].kind == e.kind {
                took += all[j].took.unwrap_or_default();
                j += 1;
            }
            let n = j - i;
            if n == 1 {
                let _ = writeln!(
                    text,
                    "{pad}{} {} {} {} µs",
                    e.kind.name(),
                    e.type_name,
                    handle(e.object),
                    took.as_micros()
                );
            } else {
                let _ = writeln!(
                    text,
                    "{pad}{} {n} items, {} µs",
                    e.kind.name(),
                    took.as_micros()
                );
            }
            listed += 1;
            i = j;
            continue;
        }
        match e.kind {
            JournalKind::Host => {
                let _ = writeln!(
                    text,
                    "{pad}host work: {} of {}, {} µs",
                    e.name,
                    e.type_name,
                    e.took.unwrap_or_default().as_micros()
                );
            }
            JournalKind::Changed => {
                let _ = writeln!(
                    text,
                    "{pad}changed {}.{} ({}): {} → {}",
                    e.type_name,
                    e.name,
                    handle(e.object),
                    short(engine, &e.old),
                    short(engine, &e.new)
                );
            }
            _ => {
                let _ = writeln!(
                    text,
                    "{pad}{} {}.{} at {}, {} µs",
                    e.kind.name(),
                    e.type_name,
                    e.name,
                    e.at.as_deref().unwrap_or("?"),
                    e.took.unwrap_or_default().as_micros()
                );
            }
        }
        listed += 1;
        i += 1;
    }
    if i < all.len() {
        let _ = writeln!(text, "  … {} more", all.len() - i);
    }
    if !slowest.is_empty() {
        text.push_str("slowest:\n");
        for e in &slowest {
            let _ = writeln!(
                text,
                "  {} {}.{} at {}: {} µs",
                e.kind.name(),
                e.type_name,
                e.name,
                e.at.as_deref().unwrap_or("?"),
                e.took.unwrap_or_default().as_micros()
            );
        }
    }
    if journal.dropped > 0 {
        let _ = writeln!(text, "{} more not kept", journal.dropped);
    }
    if journal.gone > 0 {
        let _ = writeln!(text, "{} changes of objects since destroyed", journal.gone);
    }
    let slowest: Vec<Value> = slowest.into_iter().map(to_json).collect();
    Ok(ToolReply::from(json!({
        "wake": entry.seq(),
        "at_ms": entry.at().as_millis() as u64,
        "causes": entry.causes(),
        "led_to": entry.led_to(),
        "steps": steps,
        "counts": counts,
        "entries": entries,
        "entries_left_out": journal.entries.len().saturating_sub(max),
        "slowest": slowest,
        "dropped": journal.dropped,
        "gone": journal.gone,
    }))
    .with_text(text.trim_end().to_owned()))
}
