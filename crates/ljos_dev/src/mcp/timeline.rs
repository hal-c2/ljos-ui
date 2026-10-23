//! `app_frames`: a timeline of the frames, while a client asks for one.
//!
//! Each pass of the host that ran frames of the scene is an entry: where
//! its time went, by the engine's passes (timers, animations, layout,
//! creating, building the scene), inside bindings and laying out text
//! wherever they ran, painting and presenting; and what woke it. It is for
//! comparing one runtime with another frame by frame, which totals hide.
//!
//! Nothing is timed until a client asks to record: the scene, the engine
//! and the text engine read no clock for it, and an application at rest
//! runs no frame to record. The entries are a ring of [`MAX_KEPT`]; the
//! oldest give way and are counted. A client that names a `holder` keeps
//! the recording only while it watches, as traces are kept.

use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ljos_engine::Engine;
use ljos_mcp_core::{SurfaceError, ToolReply};
use ljos_quick::{FrameTimes, QuickScene};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::frames::{ms, paint_json, spread, us};
use super::paint::PaintReport;

pub const FRAMES: &str = "\
A timeline of the frames, for comparing runtimes: `record` starts or stops it. Each frame says \
what woke it (input, timer, animation, message) and where its time went: timers, animations, \
layout, scene build, bindings, text, render, present. `since` takes `next_since`.";

/// The most frames kept: ten seconds at 60 Hz.
pub const MAX_KEPT: usize = 600;
/// The most frames one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 60;
/// The most clients that hold a recording at once.
const MAX_HOLDERS: usize = 16;

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FramesArgs {
    /// Start recording (`true`) or stop (`false`). What was recorded
    /// stays until newer frames push it out.
    #[serde(default)]
    pub record: Option<bool>,
    /// The `next_since` of an earlier answer: only the frames after it,
    /// oldest first. Without it, the latest frames.
    #[serde(default)]
    pub since: Option<u64>,
    /// The most frames to list (default 60, at most 600).
    #[serde(default)]
    pub max: Option<usize>,
    /// A name for the client: with `record`, the recording it asked for
    /// ends once no `app_watch` with this `holder` has waited for a while,
    /// or when it stops it. Others' recordings go on.
    #[serde(default)]
    pub holder: Option<String>,
}

/// One pass of the host that ran frames of the scene.
struct Entry {
    seq: u64,
    at: Duration,
    times: FrameTimes,
    /// The inspector answered a request, or the host took messages from
    /// other threads, since the frame before.
    message: bool,
    paint: Option<PaintReport>,
}

impl Entry {
    fn woke(&self) -> Vec<&'static str> {
        let w = self.times.woke;
        [
            (w.input, "input"),
            (w.timer, "timer"),
            (w.animation, "animation"),
            (self.message, "message"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }

    /// Painting but presenting: turning the shapes into what the painter
    /// draws, uploading and drawing.
    fn render(&self) -> Option<Duration> {
        self.paint
            .map(|p| (p.build + p.paint).saturating_sub(p.present.unwrap_or_default()))
    }

    fn present(&self) -> Option<Duration> {
        self.paint.and_then(|p| p.present)
    }

    /// The engine's frames and the painting that showed them, as
    /// `app_metrics` counts a frame.
    fn total(&self) -> Duration {
        self.times.total + self.paint.map_or(Duration::ZERO, |p| p.build + p.paint)
    }

    fn json(&self) -> Value {
        let t = &self.times;
        json!({
            "frame": self.seq,
            "at_ms": (self.at.as_secs_f64() * 10_000.0).round() / 10.0,
            "woke": self.woke(),
            "scene_frames": t.frames,
            "total_us": us(self.total()),
            "passes": {
                "timers_us": us(t.timers),
                "animations_us": us(t.animations),
                "layout_us": us(t.layout),
                "create_us": us(t.create),
                "scene_us": us(t.scene),
                "other_us": us(t.other),
            },
            "bindings_us": us(t.bindings),
            "binding_runs": t.binding_runs,
            "text_us": us(t.text),
            "text_layouts": t.text_layouts,
            "render_us": self.render().map(us),
            "present_us": self.present().map(us),
            "paint": self.paint.as_ref().map(paint_json),
        })
    }

    fn line(&self) -> String {
        let t = &self.times;
        let woke = self.woke();
        let mut line = format!(
            "{:>5} {:>10.1} ms  {:<24} {}: timers {}, animations {}, layout {}, create {}, \
             scene {}; bindings {} ({}), text {} ({})",
            self.seq,
            self.at.as_secs_f64() * 1000.0,
            if woke.is_empty() {
                "-".to_owned()
            } else {
                woke.join(", ")
            },
            ms(self.total()),
            ms(t.timers),
            ms(t.animations),
            ms(t.layout),
            ms(t.create),
            ms(t.scene),
            ms(t.bindings),
            t.binding_runs,
            ms(t.text),
            t.text_layouts,
        );
        if let Some(render) = self.render() {
            let _ = write!(line, "; render {}", ms(render));
        }
        if let Some(present) = self.present() {
            let _ = write!(line, ", present {}", ms(present));
        }
        line
    }
}

/// The frames recorded, and who asked for the recording.
pub(crate) struct Timeline {
    started: Instant,
    frames: VecDeque<Entry>,
    /// The number the next frame gets; the first is 1.
    next: u64,
    dropped: u64,
    /// Asked for by a client that named no holder: it lasts until stopped.
    unheld: bool,
    held: HashSet<String>,
    /// A request was answered since the last frame recorded.
    asked: bool,
    /// The last frame waits for the paint of its pass.
    unpainted: bool,
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline {
            started: Instant::now(),
            frames: VecDeque::new(),
            next: 1,
            dropped: 0,
            unheld: false,
            held: HashSet::new(),
            asked: false,
            unpainted: false,
        }
    }
}

impl Timeline {
    pub(crate) fn recording(&self) -> bool {
        self.unheld || !self.held.is_empty()
    }

    /// The inspector answered requests: the next frame was woken by them.
    pub(crate) fn asked(&mut self) {
        self.asked = self.recording();
    }

    /// The frames of a pass of the host, with the messages it took from
    /// other threads.
    pub(crate) fn record(&mut self, times: FrameTimes, drained: usize) {
        if !self.recording() {
            return;
        }
        if self.frames.len() == MAX_KEPT {
            self.frames.pop_front();
            self.dropped += 1;
        }
        self.frames.push_back(Entry {
            seq: self.next,
            at: self.started.elapsed(),
            times,
            message: std::mem::take(&mut self.asked) || drained > 0,
            paint: None,
        });
        self.next += 1;
        self.unpainted = true;
    }

    /// The paint of the pass whose frames were recorded last; a pass that
    /// ran no frame paints what an earlier one made, and is not an entry.
    pub(crate) fn record_paint(&mut self, report: &PaintReport) {
        if !std::mem::take(&mut self.unpainted) {
            return;
        }
        if let Some(last) = self.frames.back_mut() {
            last.paint = Some(*report);
        }
    }

    /// Starts or stops recording for whoever asks, so that the scene
    /// is timed exactly while someone records.
    fn ask(&mut self, record: bool, holder: Option<&str>) -> Result<(), SurfaceError> {
        match (record, holder) {
            (true, Some(h)) => {
                if !self.held.contains(h) && self.held.len() >= MAX_HOLDERS {
                    return Err(SurfaceError::invalid_argument(format!(
                        "`app_frames`: {MAX_HOLDERS} clients record already"
                    )));
                }
                self.held.insert(h.to_owned());
            }
            (true, None) => self.unheld = true,
            (false, Some(h)) => {
                self.held.remove(h);
            }
            (false, None) => {
                self.unheld = false;
                self.held.clear();
            }
        }
        if !self.recording() {
            self.asked = false;
            self.unpainted = false;
        }
        Ok(())
    }

    #[cfg(test)]
    fn kept(&self) -> usize {
        self.frames.len()
    }
}

/// Answers `app_frames`.
pub(crate) fn frames(
    engine: &Engine,
    scene: &mut QuickScene,
    timeline: &mut Timeline,
    args: &FramesArgs,
) -> Result<ToolReply, SurfaceError> {
    if let Some(record) = args.record {
        timeline.ask(record, args.holder.as_deref())?;
        let on = timeline.recording();
        if on != scene.is_timed() {
            scene.set_timed(engine, on);
        }
    }
    let max = args.max.unwrap_or(DEFAULT_MAX);
    let shown: Vec<&Entry> = match args.since {
        Some(since) => timeline
            .frames
            .iter()
            .filter(|e| e.seq > since)
            .take(max)
            .collect(),
        None => {
            let skip = timeline.frames.len().saturating_sub(max);
            timeline.frames.iter().skip(skip).collect()
        }
    };
    let more = shown
        .last()
        .is_some_and(|last| last.seq + 1 < timeline.next);
    let next_since = shown.last().map_or(timeline.next - 1, |e| e.seq);
    let mut text = String::new();
    if timeline.recording() {
        let _ = writeln!(
            text,
            "recording; the last {MAX_KEPT} frames are kept ({} now)",
            timeline.frames.len()
        );
    } else {
        text.push_str("not recording; `record: true` starts it\n");
    }
    for e in &shown {
        let _ = writeln!(text, "{}", e.line());
    }
    if timeline.dropped > 0 {
        let _ = writeln!(text, "{} older frames gave way", timeline.dropped);
    }
    let summary = summary(&shown);
    if let Some(s) = &summary
        && let Some(total) = s.get("total_us")
    {
        let _ = writeln!(
            text,
            "{} frames: total p50 {} µs, p95 {} µs, max {} µs",
            shown.len(),
            total["p50"],
            total["p95"],
            total["max"]
        );
    }
    let value = json!({
        "recording": timeline.recording(),
        "frames": shown.iter().map(|e| e.json()).collect::<Vec<_>>(),
        "next_since": next_since,
        "more": more,
        "dropped": timeline.dropped,
        "kept": timeline.frames.len(),
        "summary": summary,
    });
    Ok(ToolReply::from(value).with_text(text))
}

/// A part of a frame's time; `None` where the frame has no such part.
type Part = fn(&Entry) -> Option<Duration>;

/// The 50th and 95th percentile and the largest of each part, over the
/// frames listed.
fn summary(shown: &[&Entry]) -> Option<Value> {
    if shown.is_empty() {
        return None;
    }
    let parts: [(&str, Part); 9] = [
        ("total_us", |e| Some(e.total())),
        ("timers_us", |e| Some(e.times.timers)),
        ("animations_us", |e| Some(e.times.animations)),
        ("layout_us", |e| Some(e.times.layout)),
        ("scene_us", |e| Some(e.times.scene)),
        ("bindings_us", |e| Some(e.times.bindings)),
        ("text_us", |e| Some(e.times.text)),
        ("render_us", Entry::render),
        ("present_us", Entry::present),
    ];
    let mut out = serde_json::Map::new();
    for (name, of) in parts {
        let times: Vec<Duration> = shown.iter().filter_map(|e| of(e)).collect();
        if let Some([p50, p95, max]) = spread(times) {
            out.insert(
                name.to_owned(),
                json!({ "p50": us(p50), "p95": us(p95), "max": us(max) }),
            );
        }
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> FrameTimes {
        FrameTimes {
            frames: 1,
            total: Duration::from_micros(500),
            ..FrameTimes::default()
        }
    }

    #[test]
    fn nothing_is_recorded_until_asked_and_the_ring_keeps_the_latest() {
        let mut t = Timeline::default();
        t.record(frame(), 0);
        assert_eq!(t.kept(), 0);
        t.ask(true, None).unwrap();
        for _ in 0..MAX_KEPT + 5 {
            t.record(frame(), 0);
        }
        assert_eq!(t.kept(), MAX_KEPT);
        assert_eq!(t.dropped, 5);
        assert_eq!(t.frames.front().map(|e| e.seq), Some(6));
    }

    #[test]
    fn a_recording_lasts_while_someone_holds_it() {
        let mut t = Timeline::default();
        t.ask(true, Some("a")).unwrap();
        t.ask(true, Some("b")).unwrap();
        t.ask(false, Some("a")).unwrap();
        assert!(t.recording());
        t.ask(false, Some("b")).unwrap();
        assert!(!t.recording());
        t.ask(true, Some("a")).unwrap();
        t.ask(false, None).unwrap();
        assert!(!t.recording());
    }

    #[test]
    fn a_frame_after_an_answer_was_woken_by_a_message_and_takes_its_paint() {
        let mut t = Timeline::default();
        t.ask(true, None).unwrap();
        t.asked();
        t.record(frame(), 0);
        t.record(frame(), 0);
        t.record(frame(), 2);
        let woke: Vec<Vec<&str>> = t.frames.iter().map(Entry::woke).collect();
        assert_eq!(woke, [vec!["message"], vec![], vec!["message"]]);
        let paint = PaintReport {
            build: Duration::from_micros(100),
            paint: Duration::from_micros(300),
            present: Some(Duration::from_micros(200)),
            ..PaintReport::default()
        };
        t.record_paint(&paint);
        t.record_paint(&paint);
        let last = t.frames.back().unwrap();
        assert_eq!(last.render(), Some(Duration::from_micros(200)));
        assert_eq!(last.present(), Some(Duration::from_micros(200)));
        assert!(t.frames[1].paint.is_none());
    }
}
