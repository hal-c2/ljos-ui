//! `app_frames`: a timeline of the frames, while a client asks for one.
//!
//! Each pass of the host that ran frames of a window's scene is an entry:
//! which window, where
//! the time went, by the engine's passes (timers, animations, layout,
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

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::{SurfaceError, ToolReply};
use ljos_quick::{FrameTimes, QuickScene};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::frames::{ms, paint_json, spread, us};
use super::objects::handle;
use super::paint::PaintReport;

pub const FRAMES: &str = "\
A timeline of every window's frames: `record` starts or stops it; each says what woke it and \
where its time went, pass by pass. Modes instead: `wakes`, `bindings` (hot bindings, loops), \
`growth` (instances per component over time), `export` (a trace file). `since` takes `next_since`.";

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
    /// The UI thread's wakes instead of the frames: what woke each (input,
    /// a timer, an animation, a message and its kind, the platform) and
    /// what it led to. `record` starts or stops keeping them, apart from
    /// the frames.
    #[serde(default)]
    pub wakes: bool,
    /// With `wakes`: what this wake (its `wake` number) did, in order: the
    /// handlers and bindings that ran, the properties that changed (old
    /// and new), the items laid out, placed and redrawn, and their times.
    #[serde(default)]
    pub wake: Option<u64>,
    /// With `wakes`: as `wake`, for the latest wake input caused.
    #[serde(default)]
    pub last_input: bool,
    /// The bindings and handlers that ran instead: per each where it is
    /// written, what it writes, its runs, their time and what set them
    /// off; and the binding loops met. `record` starts or stops profiling.
    #[serde(default)]
    pub bindings: bool,
    /// With `bindings`: the rows by `total` time (default), `own` time
    /// (less what ran nested in them), `runs` or the `longest` run.
    #[serde(default)]
    pub sort: Option<super::hot::HotSort>,
    /// The instances and bytes of each component over time instead, those
    /// that keep growing first. `record` starts or stops sampling.
    #[serde(default)]
    pub growth: bool,
    /// Writes the frames, wakes and runs of bindings recorded to a trace
    /// file of this name (letters, digits, `.`, `_`, `-`) in the
    /// application's trace folder, for a trace viewer (Perfetto); the
    /// answer says where.
    #[serde(default)]
    pub export: Option<String>,
}

impl FramesArgs {
    /// The mode asked for, or a refusal when more than one is.
    pub(crate) fn mode(&self) -> Result<Mode<'_>, SurfaceError> {
        let modes = [
            self.wakes.then_some(Mode::Wakes),
            self.bindings.then_some(Mode::Bindings),
            self.growth.then_some(Mode::Growth),
            self.export.as_deref().map(Mode::Export),
        ];
        let mut asked = modes.into_iter().flatten();
        let mode = asked.next().unwrap_or(Mode::Frames);
        if asked.next().is_some() {
            return Err(SurfaceError::invalid_argument(
                "`app_frames`: ask for one of `wakes`, `bindings`, `growth` and `export` at a time",
            ));
        }
        if (self.wake.is_some() || self.last_input) && !matches!(mode, Mode::Wakes) {
            return Err(SurfaceError::invalid_argument(
                "`app_frames`: `wake` and `last_input` go with `wakes`",
            ));
        }
        if self.wake.is_some() && self.last_input {
            return Err(SurfaceError::invalid_argument(
                "`app_frames`: give `wake` or `last_input`, not both",
            ));
        }
        if self.sort.is_some() && !matches!(mode, Mode::Bindings) {
            return Err(SurfaceError::invalid_argument(
                "`app_frames`: `sort` goes with `bindings`",
            ));
        }
        if matches!(mode, Mode::Export(_)) && self.record.is_some() {
            return Err(SurfaceError::invalid_argument(
                "`app_frames`: `export` writes what is recorded; `record` goes without it",
            ));
        }
        Ok(mode)
    }
}

/// What `app_frames` is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode<'a> {
    Frames,
    Wakes,
    Bindings,
    Growth,
    Export(&'a str),
}

/// One pass of the host that ran frames of a window's scene.
struct Entry {
    seq: u64,
    at: Duration,
    /// The QML `Window` whose scene it was; `None` for the main window.
    window: Option<ObjectId>,
    times: FrameTimes,
    /// The clients whose requests the inspector answered since the frame
    /// before, by the names they gave (`None`: one gave none).
    asked_by: Vec<Option<String>>,
    /// The messages the host took from other threads since then.
    messages: usize,
    paint: Option<PaintReport>,
}

impl Entry {
    fn woke(&self) -> Vec<&'static str> {
        let w = self.times.woke;
        [
            (w.input, "input"),
            (w.timer, "timer"),
            (w.animation, "animation"),
            (!self.asked_by.is_empty() || self.messages > 0, "message"),
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
            "window": self.window.map(handle),
            "at_ms": (self.at.as_secs_f64() * 10_000.0).round() / 10.0,
            "woke": self.woke(),
            "asked_by": self.asked_by,
            "thread_messages": self.messages,
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
            "{:>5} {:>10.1} ms {:<8} {:<24} {}: timers {}, animations {}, layout {}, create {}, \
             scene {}; bindings {} ({}), text {} ({})",
            self.seq,
            self.at.as_secs_f64() * 1000.0,
            self.window.map_or_else(|| "main".to_owned(), handle),
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
        if !self.asked_by.is_empty() {
            let names: Vec<&str> = self
                .asked_by
                .iter()
                .map(|c| c.as_deref().unwrap_or("?"))
                .collect();
            let _ = write!(line, "; asked by {}", names.join(", "));
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
    /// Who asked what was answered since the last frame recorded.
    asked: Vec<Option<String>>,
    /// The frames, by window, that wait for the paint of their pass.
    unpainted: Vec<(Option<ObjectId>, u64)>,
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
            asked: Vec::new(),
            unpainted: Vec::new(),
        }
    }
}

impl Timeline {
    pub(crate) fn recording(&self) -> bool {
        self.unheld || !self.held.is_empty()
    }

    /// The inspector answered requests of these clients: the next frame
    /// was woken by them.
    pub(crate) fn asked(&mut self, by: impl IntoIterator<Item = Option<String>>) {
        if !self.recording() {
            return;
        }
        for client in by {
            if !self.asked.contains(&client) && self.asked.len() < MAX_HOLDERS {
                self.asked.push(client);
            }
        }
    }

    /// The frames of a pass of the host over `window`'s scene (`None`: the
    /// main window's), with the messages it took from other threads.
    pub(crate) fn record(&mut self, times: FrameTimes, drained: usize, window: Option<ObjectId>) {
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
            window,
            times,
            asked_by: std::mem::take(&mut self.asked),
            messages: drained,
            paint: None,
        });
        self.unpainted.retain(|(w, _)| *w != window);
        self.unpainted.push((window, self.next));
        self.next += 1;
    }

    /// The paint of the pass over `window` whose frames were recorded last;
    /// a pass that ran no frame paints what an earlier one made, and is not
    /// an entry.
    pub(crate) fn record_paint(&mut self, window: Option<ObjectId>, report: &PaintReport) {
        let Some(at) = self.unpainted.iter().position(|(w, _)| *w == window) else {
            return;
        };
        let (_, seq) = self.unpainted.swap_remove(at);
        if let Some(entry) = self.frames.iter_mut().rev().find(|e| e.seq == seq) {
            entry.paint = Some(*report);
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
            self.asked.clear();
            self.unpainted.clear();
        }
        Ok(())
    }

    /// The frames kept, for a trace file: each a slice on the frames'
    /// track, its passes slices in it one after another (the timeline
    /// keeps how long each took, not when it began), and its painting
    /// after them.
    pub(crate) fn trace_events(&self, out: &mut Vec<super::export::Event>) {
        use super::export::{Event, TID_FRAMES};
        for e in &self.frames {
            let t = &e.times;
            let end = self.started + e.at;
            let start = end.checked_sub(t.total).unwrap_or(end);
            let window = e.window.map_or_else(|| "main".to_owned(), handle);
            out.push(Event::slice(
                start,
                e.total(),
                TID_FRAMES,
                format!("frame {}", e.seq),
                "frame",
                json!({
                    "window": window,
                    "woke": e.woke(),
                    "asked_by": e.asked_by,
                    "scene_frames": t.frames,
                    "bindings_us": us(t.bindings),
                    "binding_runs": t.binding_runs,
                    "text_layouts": t.text_layouts,
                }),
            ));
            let mut at = start;
            for (name, d) in [
                ("timers", t.timers),
                ("animations", t.animations),
                ("layout", t.layout),
                ("create", t.create),
                ("scene", t.scene),
                ("other", t.other),
            ] {
                if d.is_zero() {
                    continue;
                }
                out.push(Event::slice(
                    at,
                    d,
                    TID_FRAMES,
                    name.to_owned(),
                    "pass",
                    json!({}),
                ));
                at += d;
            }
            if let Some(render) = e.render() {
                out.push(Event::slice(
                    end,
                    render,
                    TID_FRAMES,
                    "render".to_owned(),
                    "paint",
                    json!({}),
                ));
                if let Some(present) = e.present() {
                    out.push(Event::slice(
                        end + render,
                        present,
                        TID_FRAMES,
                        "present".to_owned(),
                        "paint",
                        json!({}),
                    ));
                }
            }
        }
    }

    pub(crate) fn kept(&self) -> usize {
        self.frames.len()
    }
}

/// Answers `app_frames`.
pub(crate) fn frames(
    engine: &Engine,
    scene: &mut QuickScene,
    timeline: &mut Timeline,
    args: &FramesArgs,
    keeps_wakes: bool,
) -> Result<ToolReply, SurfaceError> {
    if let Some(record) = args.record {
        timeline.ask(record, args.holder.as_deref())?;
        // The frames say what woke them for the wakes too.
        let on = timeline.recording() || keeps_wakes;
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
        t.record(frame(), 0, None);
        assert_eq!(t.kept(), 0);
        t.ask(true, None).unwrap();
        for _ in 0..MAX_KEPT + 5 {
            t.record(frame(), 0, None);
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
        t.asked([
            Some("observer".to_owned()),
            None,
            Some("observer".to_owned()),
        ]);
        t.record(frame(), 0, None);
        t.record(frame(), 0, None);
        t.record(frame(), 2, None);
        let woke: Vec<Vec<&str>> = t.frames.iter().map(Entry::woke).collect();
        assert_eq!(woke, [vec!["message"], vec![], vec!["message"]]);
        assert_eq!(t.frames[0].asked_by, [Some("observer".to_owned()), None]);
        assert!(t.frames[2].asked_by.is_empty());
        let paint = PaintReport {
            build: Duration::from_micros(100),
            paint: Duration::from_micros(300),
            present: Some(Duration::from_micros(200)),
            ..PaintReport::default()
        };
        t.record_paint(None, &paint);
        t.record_paint(None, &paint);
        let last = t.frames.back().unwrap();
        assert_eq!(last.render(), Some(Duration::from_micros(200)));
        assert_eq!(last.present(), Some(Duration::from_micros(200)));
        assert!(t.frames[1].paint.is_none());
    }

    #[test]
    fn each_window_takes_the_paint_of_its_own_pass() {
        let mut t = Timeline::default();
        t.ask(true, None).unwrap();
        let other = super::super::objects::object_of("@7.1");
        assert!(other.is_some());
        t.record(frame(), 0, None);
        t.record(frame(), 0, other);
        let paint = |us| PaintReport {
            paint: Duration::from_micros(us),
            ..PaintReport::default()
        };
        t.record_paint(None, &paint(100));
        t.record_paint(other, &paint(300));
        t.record_paint(other, &paint(900));
        let painted: Vec<_> = t.frames.iter().map(|e| e.render()).collect();
        assert_eq!(
            painted,
            [
                Some(Duration::from_micros(100)),
                Some(Duration::from_micros(300))
            ]
        );
        assert_eq!(t.frames[1].window, other);
    }
}
