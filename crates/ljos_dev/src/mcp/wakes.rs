//! `app_frames`' `wakes`: why the UI thread woke, while a client asks.
//!
//! An application at rest runs nothing (ARCHITECTURE.md principle 6), so
//! one that does not rest wakes for something. Each turn of the host's
//! loop is a wake: what the host saw in it (input, the platform, posts
//! into the loop, the time it set itself) and what the inspector saw (the
//! requests it answered, the messages of other threads, the timers and
//! animations the frames ran), and what it led to: passes, frames, a
//! picture drawn or found unchanged, or nothing.
//!
//! Nothing is kept until a client asks; a host checks
//! [`Inspector::records_wakes`](super::Inspector::records_wakes) once a
//! turn and gathers nothing otherwise. The wakes are a ring of
//! [`MAX_KEPT`], held as `app_frames`' recording is.
//!
//! While wakes are kept the engine journals each turn, and the latest
//! [`MAX_DETAILED`] wakes that were not only an agent's requests keep
//! what they did, in order (`wake`, `one_wake.rs`): the handlers and
//! bindings that ran, the properties that changed, the items laid out,
//! placed and redrawn, and the time each took.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ljos_engine::{Engine, Journal};
use ljos_mcp_core::{SurfaceError, ToolReply};
use ljos_quick::FrameTimes;
use serde_json::{Value, json};

use super::room::{self, Room};

/// The most wakes kept.
pub const MAX_KEPT: usize = 600;
/// The most wakes one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 60;
/// The most clients that hold the wakes at once.
const MAX_HOLDERS: usize = 16;
/// The most wakes that keep what they did.
pub const MAX_DETAILED: usize = 16;
/// How long after a wake only requests caused the loop's own follow-up
/// (a redraw it asked for, a deadline it set) is still the requests'.
const ECHO: Duration = Duration::from_millis(250);

/// What a host saw of one turn of its loop, from the first event after it
/// last waited until it waits again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostWake {
    /// Input events: a pointer, a key, the wheel, a touch, the input
    /// method.
    pub input: u32,
    /// Other events of a window: resized, moved, focused, hidden.
    pub window: u32,
    /// Redraws the platform asked for by itself: a window exposed or
    /// resized.
    pub redraws: u32,
    /// Redraws the loop had asked the platform for at an earlier wake.
    pub requested_redraws: u32,
    /// The loop woke at the time it had set itself.
    pub deadline: bool,
    /// Posts into the loop: a scene's or another thread's waker, a
    /// context asking for a pass.
    pub posted: u32,
    /// What the accessibility adapter asked.
    pub accessibility: u32,
    /// A window of the application wanted something else than its
    /// platform window had.
    pub dirty: bool,
    /// Passes run over a window's scene, drawn or not.
    pub passes: u32,
    /// Passes that drew a changed picture.
    pub drawn: u32,
    /// Passes that were to draw and found the picture unchanged.
    pub unchanged: u32,
    /// Redraws the loop asked the platform for, for a later wake.
    pub redraws_asked: u32,
}

/// What the inspector saw since the last wake was recorded.
#[derive(Debug, Default)]
struct Seen {
    asked_by: Vec<Option<String>>,
    answered: u32,
    thread: usize,
    input: bool,
    timer: bool,
    animation: bool,
    frames: u32,
    /// The frames' steps, added up.
    steps: FrameTimes,
}

pub(super) struct Entry {
    seq: u64,
    at: Duration,
    host: HostWake,
    seen: Seen,
    /// Only an agent's requests caused it, or the loop's follow-up to such
    /// a wake.
    requests_only: bool,
    /// What it did, for the latest [`MAX_DETAILED`].
    pub(super) journal: Option<Box<Journal>>,
}

impl Entry {
    pub(super) fn seq(&self) -> u64 {
        self.seq
    }

    pub(super) fn at(&self) -> Duration {
        self.at
    }

    pub(super) fn steps(&self) -> &FrameTimes {
        &self.seen.steps
    }

    pub(super) fn input(&self) -> bool {
        self.host.input > 0 || self.seen.input
    }

    fn messages(&self) -> u64 {
        u64::from(self.seen.answered)
            + self.seen.thread as u64
            + u64::from(self.host.posted)
            + u64::from(self.host.accessibility)
    }

    pub(super) fn causes(&self) -> Vec<&'static str> {
        let (h, s) = (&self.host, &self.seen);
        [
            (h.input > 0 || s.input, "input"),
            (s.timer, "timer"),
            (s.animation, "animation"),
            (self.messages() > 0, "message"),
            (h.redraws > 0, "redraw"),
            (h.window > 0, "window"),
            (h.dirty, "dirty"),
            (h.deadline, "deadline"),
            (h.requested_redraws > 0, "requested_redraw"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }

    pub(super) fn led_to(&self) -> &'static str {
        let h = &self.host;
        if h.drawn > 0 {
            "drawn"
        } else if h.unchanged > 0 {
            "unchanged"
        } else if self.seen.frames > 0 {
            "frames"
        } else if h.redraws_asked > 0 {
            "redraw asked"
        } else {
            "nothing"
        }
    }

    fn json(&self) -> Value {
        let (h, s) = (&self.host, &self.seen);
        json!({
            "wake": self.seq,
            "at_ms": (self.at.as_secs_f64() * 10_000.0).round() / 10.0,
            "causes": self.causes(),
            "messages": {
                "inspector": s.answered,
                "thread": s.thread,
                "posted": h.posted,
                "accessibility": h.accessibility,
            },
            "asked_by": s.asked_by,
            "requests_only": self.requests_only,
            "input_events": h.input,
            "window_events": h.window,
            "passes": h.passes,
            "frames": s.frames,
            "drawn": h.drawn,
            "unchanged": h.unchanged,
            "redraws_asked": h.redraws_asked,
            "led_to": self.led_to(),
            "detailed": self.journal.is_some(),
        })
    }

    fn line(&self) -> String {
        let causes = self.causes();
        let mut line = format!(
            "{:>5} {:>10.1} ms woke by {}; led to {}: passes {}, frames {}, drawn {}, unchanged {}",
            self.seq,
            self.at.as_secs_f64() * 1000.0,
            if causes.is_empty() {
                "nothing known".to_owned()
            } else {
                causes.join(", ")
            },
            self.led_to(),
            self.host.passes,
            self.seen.frames,
            self.host.drawn,
            self.host.unchanged,
        );
        if !self.seen.asked_by.is_empty() {
            let names: Vec<&str> = self
                .seen
                .asked_by
                .iter()
                .map(|c| c.as_deref().unwrap_or("?"))
                .collect();
            let _ = write!(line, "; asked by {}", names.join(", "));
        }
        line
    }
}

/// The wakes kept, and who asked for them.
pub(crate) struct Wakes {
    started: Instant,
    wakes: VecDeque<Entry>,
    /// The number the next wake gets; the first is 1.
    next: u64,
    dropped: u64,
    /// Asked for by a client that named no holder: it lasts until stopped.
    unheld: bool,
    held: HashSet<String>,
    seen: Seen,
    /// The wakes that keep what they did, oldest first.
    detailed: VecDeque<u64>,
    /// Until when the loop's own follow-up is the requests'.
    echo_until: Option<Duration>,
}

impl Default for Wakes {
    fn default() -> Self {
        Wakes {
            started: Instant::now(),
            wakes: VecDeque::new(),
            next: 1,
            dropped: 0,
            unheld: false,
            held: HashSet::new(),
            seen: Seen::default(),
            detailed: VecDeque::new(),
            echo_until: None,
        }
    }
}

impl Wakes {
    pub(crate) fn recording(&self) -> bool {
        self.unheld || !self.held.is_empty()
    }

    /// The inspector answered requests of these clients in this wake.
    pub(crate) fn asked(&mut self, by: &[Option<String>]) {
        if !self.recording() {
            return;
        }
        self.seen.answered += u32::try_from(by.len()).unwrap_or(u32::MAX);
        for client in by {
            if !self.seen.asked_by.contains(client) && self.seen.asked_by.len() < MAX_HOLDERS {
                self.seen.asked_by.push(client.clone());
            }
        }
    }

    /// A scene ran these frames in this wake, after the host took `drained`
    /// messages from other threads.
    pub(crate) fn ran(&mut self, times: &FrameTimes, drained: usize) {
        if !self.recording() {
            return;
        }
        let s = &mut self.seen;
        s.frames += times.frames;
        s.input |= times.woke.input;
        s.timer |= times.woke.timer;
        s.animation |= times.woke.animation;
        s.thread += drained;
        let t = &mut s.steps;
        t.frames += times.frames;
        t.timers += times.timers;
        t.animations += times.animations;
        t.layout += times.layout;
        t.create += times.create;
        t.scene += times.scene;
        t.other += times.other;
        t.total += times.total;
        t.bindings += times.bindings;
        t.binding_runs += times.binding_runs;
        t.text += times.text;
        t.text_layouts += times.text_layouts;
    }

    /// The host's turn ended, having done what `journal` says. Whether
    /// it was the application's and not only an agent's requests: only
    /// then is a watch told.
    pub(crate) fn record(&mut self, host: &HostWake, journal: Journal) -> bool {
        if !self.recording() {
            return false;
        }
        let at = self.started.elapsed();
        let seen = std::mem::take(&mut self.seen);
        let mut entry = Entry {
            seq: self.next,
            at,
            host: *host,
            seen,
            requests_only: false,
            journal: None,
        };
        entry.requests_only = self.requests_only(&entry);
        // An agent's own requests are not what it asks about.
        if !entry.requests_only {
            entry.journal = Some(Box::new(journal));
            self.detailed.push_back(entry.seq);
        }
        if entry.seen.answered > 0 && entry.requests_only {
            self.echo_until = Some(at + ECHO);
        }
        let told = !entry.requests_only;
        if self.wakes.len() == MAX_KEPT {
            self.wakes.pop_front();
            self.dropped += 1;
        }
        self.wakes.push_back(entry);
        self.next += 1;
        while self.detailed.len() > MAX_DETAILED {
            if let Some(seq) = self.detailed.pop_front()
                && let Some(e) = self.get_mut(seq)
            {
                e.journal = None;
            }
        }
        told
    }

    fn get_mut(&mut self, seq: u64) -> Option<&mut Entry> {
        let first = self.wakes.front()?.seq;
        let i = usize::try_from(seq.checked_sub(first)?).ok()?;
        self.wakes.get_mut(i)
    }

    /// The wake `seq`, or the latest that input woke.
    pub(super) fn pick(&self, seq: Option<u64>) -> Option<&Entry> {
        match seq {
            Some(seq) => {
                let first = self.wakes.front()?.seq;
                self.wakes
                    .get(usize::try_from(seq.checked_sub(first)?).ok()?)
            }
            None => self
                .wakes
                .iter()
                .rev()
                .find(|e| e.input() && e.journal.is_some()),
        }
    }

    /// Nothing but the requests woke it: no input, timer, animation,
    /// message of a thread or the accessibility adapter, nor the
    /// platform. A turn that has only the loop's own causes (a deadline, a
    /// redraw it asked for, a post) shortly after one that requests woke
    /// is that wake's follow-up.
    fn requests_only(&self, e: &Entry) -> bool {
        let (h, s) = (&e.host, &e.seen);
        let outside = h.input > 0
            || s.input
            || s.timer
            || s.animation
            || s.thread > 0
            || h.accessibility > 0
            || h.redraws > 0
            || h.window > 0
            || h.dirty;
        if outside {
            return false;
        }
        s.answered > 0 || self.echo_until.is_some_and(|until| e.at <= until)
    }

    /// Starts or stops keeping wakes for whoever asks.
    pub(crate) fn ask(&mut self, record: bool, holder: Option<&str>) -> Result<(), SurfaceError> {
        match (record, holder) {
            (true, Some(h)) => {
                if !self.held.contains(h) && self.held.len() >= MAX_HOLDERS {
                    return Err(SurfaceError::invalid_argument(format!(
                        "`app_frames`: {MAX_HOLDERS} clients keep wakes already"
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
            self.seen = Seen::default();
            self.echo_until = None;
        }
        Ok(())
    }

    /// Answers `app_frames` with `wakes` and `wake` or `last_input`.
    pub(crate) fn answer_one(
        &self,
        engine: &Engine,
        seq: Option<u64>,
        max: Option<usize>,
    ) -> Result<ToolReply, SurfaceError> {
        super::one_wake::answer(self, engine, seq, max)
    }

    /// The wakes kept, for a trace file: each an instant on the wakes'
    /// track, at the end of its turn.
    pub(crate) fn trace_events(&self, out: &mut Vec<super::export::Event>) {
        use super::export::{Event, TID_WAKES};
        for e in &self.wakes {
            let causes = e.causes();
            out.push(Event::instant(
                self.started + e.at,
                TID_WAKES,
                format!(
                    "wake: {}",
                    if causes.is_empty() {
                        "nothing known".to_owned()
                    } else {
                        causes.join(", ")
                    }
                ),
                "wake",
                e.json(),
            ));
        }
    }

    pub(crate) fn kept(&self) -> usize {
        self.wakes.len()
    }

    /// Answers `app_frames` with `wakes`.
    pub(crate) fn answer(
        &self,
        since: Option<u64>,
        max: Option<usize>,
        per_copy: usize,
    ) -> ToolReply {
        let max = max.unwrap_or(DEFAULT_MAX);
        let shown: Vec<&Entry> = match since {
            Some(since) => self
                .wakes
                .iter()
                .filter(|e| e.seq > since)
                .take(max)
                .collect(),
            None => {
                let skip = self.wakes.len().saturating_sub(max);
                self.wakes.iter().skip(skip).collect()
            }
        };
        // What does not fit is the next answer's: `next_since` stops at
        // the last wake told.
        let mut room = Room::new(
            per_copy,
            &json!({ "wakes": [], "next_since": self.next, "kept": MAX_KEPT, "by_cause": {} }),
            "not keeping wakes; `wakes: true, record: true` starts it",
        );
        let rows = room::rows(&mut room, &shown, since.is_none(), |e| (e.json(), e.line()));
        let shown: Vec<&Entry> = rows.iter().map(|(e, _, _)| *e).collect();
        let more = shown.last().is_some_and(|last| last.seq + 1 < self.next);
        let next_since = shown.last().map_or(self.next - 1, |e| e.seq);
        let idle = shown.iter().filter(|e| e.led_to() == "nothing").count();
        let mut by_cause: BTreeMap<&str, u64> = BTreeMap::new();
        for e in &shown {
            for c in e.causes() {
                *by_cause.entry(c).or_default() += 1;
            }
        }
        let mut text = String::new();
        if self.recording() {
            let _ = writeln!(
                text,
                "keeping wakes; the last {MAX_KEPT} are kept ({} now)",
                self.wakes.len()
            );
        } else {
            text.push_str("not keeping wakes; `wakes: true, record: true` starts it\n");
        }
        for (_, _, line) in &rows {
            let _ = writeln!(text, "{line}");
        }
        if self.dropped > 0 {
            let _ = writeln!(text, "{} older wakes gave way", self.dropped);
        }
        if !shown.is_empty() {
            let _ = writeln!(text, "{} wakes, {idle} led to nothing", shown.len());
        }
        let value = json!({
            "recording": self.recording(),
            "wakes": rows.into_iter().map(|(_, v, _)| v).collect::<Vec<_>>(),
            "next_since": next_since,
            "more": more,
            "dropped": self.dropped,
            "kept": self.wakes.len(),
            "idle": idle,
            "by_cause": by_cause,
        });
        ToolReply::from(value).with_text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(w: &mut Wakes, host: &HostWake) -> bool {
        w.record(host, Journal::default())
    }

    #[test]
    fn nothing_is_kept_until_asked_and_the_ring_keeps_the_latest() {
        let mut w = Wakes::default();
        w.asked(&[None]);
        assert!(!rec(&mut w, &HostWake::default()));
        assert!(w.wakes.is_empty());
        assert_eq!(w.seen.answered, 0);
        w.ask(true, None).unwrap();
        let tick = HostWake {
            deadline: true,
            ..HostWake::default()
        };
        for _ in 0..MAX_KEPT + 3 {
            rec(&mut w, &tick);
        }
        assert_eq!(w.wakes.len(), MAX_KEPT);
        assert_eq!(w.dropped, 3);
        assert_eq!(w.wakes.front().map(|e| e.seq), Some(4));
    }

    #[test]
    fn a_wake_says_its_causes_and_what_it_led_to() {
        let mut w = Wakes::default();
        w.ask(true, None).unwrap();
        let frames = FrameTimes {
            frames: 1,
            woke: ljos_quick::Woke {
                timer: true,
                ..Default::default()
            },
            ..FrameTimes::default()
        };
        w.ran(&frames, 2);
        assert!(rec(
            &mut w,
            &HostWake {
                requested_redraws: 1,
                passes: 1,
                unchanged: 1,
                ..HostWake::default()
            }
        ));
        let e = &w.wakes[0];
        assert_eq!(e.causes(), ["timer", "message", "requested_redraw"]);
        assert_eq!(e.led_to(), "unchanged");
        assert_eq!(e.json()["messages"]["thread"], 2);
        // A message that dirtied nothing runs no pass on a host.
        assert!(rec(
            &mut w,
            &HostWake {
                posted: 1,
                ..HostWake::default()
            }
        ));
        assert_eq!(w.wakes[1].led_to(), "nothing");
        assert_eq!(
            w.answer(None, None, ljos_mcp_core::DEFAULT_RESULT_BYTES / 2)
                .structured["idle"],
            1
        );
    }

    #[test]
    fn only_the_latest_wakes_keep_what_they_did_and_not_an_agents_own() {
        let mut w = Wakes::default();
        w.ask(true, None).unwrap();
        let input = HostWake {
            input: 1,
            ..HostWake::default()
        };
        for _ in 0..MAX_DETAILED + 2 {
            rec(&mut w, &input);
        }
        let kept: Vec<bool> = w.wakes.iter().map(|e| e.journal.is_some()).collect();
        assert_eq!(kept.iter().filter(|k| **k).count(), MAX_DETAILED);
        assert!(!kept[0] && !kept[1] && kept[2]);
        assert_eq!(w.pick(None).map(Entry::seq), Some(MAX_DETAILED as u64 + 2));
        w.asked(&[None]);
        rec(&mut w, &HostWake::default());
        assert!(w.wakes.back().is_some_and(|e| e.journal.is_none()));
        assert_eq!(w.pick(Some(1)).map(Entry::seq), Some(1));
        assert!(w.pick(Some(99)).is_none());
    }

    #[test]
    fn the_loops_follow_up_to_a_request_is_the_requests_but_a_timer_is_not() {
        let mut w = Wakes::default();
        w.ask(true, Some("observer")).unwrap();
        w.asked(&[Some("observer".to_owned())]);
        assert!(!rec(
            &mut w,
            &HostWake {
                posted: 1,
                ..HostWake::default()
            }
        ));
        assert!(!rec(
            &mut w,
            &HostWake {
                requested_redraws: 1,
                drawn: 1,
                ..HostWake::default()
            }
        ));
        let timer = FrameTimes {
            frames: 1,
            woke: ljos_quick::Woke {
                timer: true,
                ..Default::default()
            },
            ..FrameTimes::default()
        };
        w.ran(&timer, 0);
        assert!(rec(&mut w, &HostWake::default()));
        let only: Vec<bool> = w.wakes.iter().map(|e| e.requests_only).collect();
        assert_eq!(only, [true, true, false]);
    }

    #[test]
    fn wakes_are_kept_while_someone_holds_them() {
        let mut w = Wakes::default();
        w.ask(true, Some("a")).unwrap();
        w.ask(true, Some("b")).unwrap();
        w.ask(false, Some("a")).unwrap();
        assert!(w.recording());
        w.ask(false, Some("b")).unwrap();
        assert!(!w.recording());
    }
}
