//! A scripted session: the steps a person takes through an application,
//! a click here, a word typed, a key pressed, a pause, given once in a
//! manifest and sent the same way to every runtime, each step timed from
//! its input to what the window showed.
//!
//! Only the window's pixels and the compositor's input are used, so a
//! session drives a program of any toolkit as it drives Ljós.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::wayland::{self, Chord};

/// How often a key is pressed while typing, and a repeated step's input
/// by default: 12.5 a second, a fast typist's pace.
pub const KEY_EVERY: Duration = Duration::from_millis(80);

/// How long the window must show no change after a step for it to have
/// settled.
pub const STILL_FOR: Duration = Duration::from_millis(500);

/// A frame interval a person sees as a stall: three missed frames at 60
/// a second.
pub const LONG_FRAME: Duration = Duration::from_millis(50);

/// One step of a session, as a manifest gives it: one of
/// `{"click": [fx, fy]}` (fractions of the window), `{"key": "Ctrl+A"}`
/// ([`wayland::named`]), `{"type": "text"}` or `{"wait": ms}`, with a
/// `"name"` for the report and, for a click or a key, `"repeat"` and
/// `"every"` (ms between inputs, [`KEY_EVERY`] by default).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawStep")]
pub struct Step {
    pub name: Option<String>,
    pub action: Action,
    /// How many times its input is sent: 1 but for a repeated click or key.
    pub repeat: u32,
    /// From one input's start to the next's.
    pub every: Duration,
}

/// What a step does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A click at fractions of the window.
    Click([f64; 2]),
    /// A key, by the name the manifest gave it.
    Key { name: String, chord: Chord },
    /// Text, a key at a time, [`KEY_EVERY`] apart.
    Type { text: String, keys: Vec<Chord> },
    /// Nothing sent for a while: the step before it settles meanwhile.
    Wait(Duration),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    name: Option<String>,
    click: Option<[f64; 2]>,
    key: Option<String>,
    #[serde(rename = "type")]
    text: Option<String>,
    wait: Option<u64>,
    repeat: Option<u32>,
    every: Option<u64>,
}

impl TryFrom<RawStep> for Step {
    type Error = String;

    fn try_from(raw: RawStep) -> Result<Self, String> {
        let action = match (raw.click, raw.key, raw.text, raw.wait) {
            (Some(point), None, None, None) => {
                if point.iter().any(|f| !(0.0..=1.0).contains(f)) {
                    return Err(format!(
                        "a click at {point:?} is outside the window: it is at fractions of it, \
                         from 0 to 1"
                    ));
                }
                Action::Click(point)
            }
            (None, Some(name), None, None) => {
                let chord = wayland::named(&name).ok_or_else(|| format!("no key {name:?}"))?;
                Action::Key { name, chord }
            }
            (None, None, Some(text), None) => {
                if text.is_empty() {
                    return Err("a type step has no text".to_owned());
                }
                let keys = text
                    .chars()
                    .map(|c| wayland::typed(c).ok_or_else(|| format!("no key types {c:?}")))
                    .collect::<Result<_, _>>()?;
                Action::Type { text, keys }
            }
            (None, None, None, Some(ms)) => Action::Wait(Duration::from_millis(ms)),
            _ => return Err("a step is one of click, key, type, wait".to_owned()),
        };
        let repeats = matches!(action, Action::Click(_) | Action::Key { .. });
        if !repeats && (raw.repeat.is_some() || raw.every.is_some()) {
            return Err("only a key or a click repeats".to_owned());
        }
        let repeat = raw.repeat.unwrap_or(1);
        if repeat == 0 {
            return Err("a step repeats at least once".to_owned());
        }
        Ok(Self {
            name: raw.name,
            action,
            repeat,
            every: raw.every.map_or(KEY_EVERY, Duration::from_millis),
        })
    }
}

impl Step {
    /// What it does, as the report says it: `click 0.5 0.1`,
    /// `type "quick"`, `key PageDown ×3 every 33 ms`, `wait 200 ms`.
    #[must_use]
    pub fn describe(&self) -> String {
        let what = match &self.action {
            Action::Click([fx, fy]) => format!("click {fx} {fy}"),
            Action::Key { name, .. } => format!("key {name}"),
            Action::Type { text, .. } => format!("type {text:?}"),
            Action::Wait(time) => return format!("wait {} ms", time.as_millis()),
        };
        if self.repeat > 1 {
            format!(
                "{what} ×{} every {} ms",
                self.repeat,
                self.every.as_millis()
            )
        } else {
            what
        }
    }

    /// How many inputs it sends: a key for each character typed.
    #[must_use]
    pub fn inputs(&self) -> u32 {
        match &self.action {
            Action::Click(_) | Action::Key { .. } => self.repeat,
            Action::Type { keys, .. } => u32::try_from(keys.len()).unwrap_or(u32::MAX),
            Action::Wait(_) => 0,
        }
    }

    /// From one input's start to the next's.
    #[must_use]
    pub fn cadence(&self) -> Duration {
        match self.action {
            Action::Type { .. } => KEY_EVERY,
            _ => self.every,
        }
    }

    /// How long it lasts: its inputs at its cadence, or its wait.
    #[must_use]
    pub fn length(&self) -> Duration {
        match self.action {
            Action::Wait(time) => time,
            _ => self.cadence() * self.inputs(),
        }
    }
}

/// What happened while a step ran, on `CLOCK_MONOTONIC`
/// in nanoseconds, the clock the frames are counted on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// When it began.
    pub start: u64,
    /// When each of its inputs reached the compositor; none when there
    /// was no compositor to send them to.
    pub sent: Vec<u64>,
}

/// How one step of a session went.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepTiming {
    /// Its place in the session, from 0.
    pub index: usize,
    pub name: Option<String>,
    /// What it does ([`Step::describe`]).
    pub step: String,
    /// From the start of the load span to the step's.
    pub started_ms: f64,
    /// The inputs it sends, and of them those that reached the
    /// compositor.
    pub inputs: u32,
    pub sent: u32,
    /// When each was sent, from the start of the load span.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs_ms: Vec<f64>,
    /// From each input to the first frame that changed after it; `None`
    /// for one nothing answered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lags_ms: Vec<Option<f64>>,
    /// The first input's lag: how long a person waits to see the step
    /// answered.
    pub lag_ms: Option<f64>,
    /// From the first input to the last frame that changed before the
    /// window was still for [`STILL_FOR`], or before the next step sent
    /// input: how long the answer took to finish.
    pub settle_ms: Option<f64>,
    /// Frames that changed from the step's start to the next's, when
    /// they were counted, and the intervals between them.
    pub frames: Option<usize>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// Intervals longer than [`LONG_FRAME`].
    pub long: Option<usize>,
}

/// Times each of `steps` from what it sent (`marks`, one for each) and
/// the frames that changed (`changed`, in order; `None` when none were
/// counted), over a span that began at `start` and ended at `end`.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn timings(
    steps: &[Step],
    marks: &[Marks],
    changed: Option<&[u64]>,
    start: u64,
    end: u64,
) -> Vec<StepTiming> {
    let ms = |ns: u64| ns as f64 / 1e6;
    let since = |at: u64| ms(at.saturating_sub(start));
    let still = u64::try_from(STILL_FOR.as_nanos()).unwrap_or(u64::MAX);
    let long_frame = LONG_FRAME.as_secs_f64() * 1000.0;
    steps
        .iter()
        .zip(marks)
        .enumerate()
        .map(|(index, (step, mark))| {
            // The step's own time runs to the next step's start; its
            // answer may go on through waits, until the next input is
            // sent. A step that sent nothing (its send failed) does not
            // end it.
            let own_end = marks.get(index + 1).map_or(end, |m| m.start);
            let answer_end = marks
                .iter()
                .skip(index + 1)
                .find_map(|m| m.sent.first().copied())
                .map_or(end, |next| next.min(end));
            let lags: Vec<Option<f64>> = mark
                .sent
                .iter()
                .map(|&sent| {
                    let frames = changed?;
                    let i = frames.partition_point(|&t| t <= sent);
                    // A frame after the next input, or after the span,
                    // answers something else.
                    frames
                        .get(i)
                        .filter(|&&t| t <= answer_end)
                        .map(|&t| ms(t - sent))
                })
                .collect();
            let settle = changed
                .zip(mark.sent.first().zip(mark.sent.last()))
                .and_then(|(frames, (&first, &last))| {
                    settle(frames, first, last, answer_end, still).map(|at| ms(at - first))
                });
            let mut timing = StepTiming {
                index,
                name: step.name.clone(),
                step: step.describe(),
                started_ms: since(mark.start),
                inputs: step.inputs(),
                sent: u32::try_from(mark.sent.len()).unwrap_or(u32::MAX),
                inputs_ms: mark.sent.iter().map(|&t| since(t)).collect(),
                lag_ms: lags.first().copied().flatten(),
                lags_ms: lags,
                settle_ms: settle,
                frames: None,
                p50_ms: None,
                p95_ms: None,
                max_ms: None,
                long: None,
            };
            if let Some(frames) = changed {
                let own: Vec<u64> = frames
                    .iter()
                    .copied()
                    .filter(|&t| t >= mark.start && t < own_end)
                    .collect();
                let mut gaps: Vec<f64> = own.windows(2).map(|w| ms(w[1] - w[0])).collect();
                gaps.sort_by(f64::total_cmp);
                timing.frames = Some(own.len());
                timing.p50_ms = crate::measure::quantile(&gaps, 0.5);
                timing.p95_ms = crate::measure::quantile(&gaps, 0.95);
                timing.max_ms = gaps.last().copied();
                timing.long = Some(gaps.iter().filter(|&&g| g > long_frame).count());
            }
            timing
        })
        .collect()
}

/// The last frame of `frames` after `first` (an input) and up to `end`
/// before the window was still for `still` once the inputs, up to
/// `last`, were sent: the step's answer finished. The last frame before
/// `end` when it never was; `None` when nothing changed.
fn settle(frames: &[u64], first: u64, last: u64, end: u64, still: u64) -> Option<u64> {
    let after: Vec<u64> = frames
        .iter()
        .copied()
        .filter(|&t| t > first && t <= end)
        .collect();
    after
        .iter()
        .enumerate()
        .find(|&(i, &f)| {
            let next = after.get(i + 1).copied().unwrap_or(end);
            next.saturating_sub(f.max(last)) >= still
        })
        .map(|(_, &f)| f)
        .or_else(|| after.last().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    fn steps(json: &str) -> Result<Vec<Step>, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    #[test]
    fn a_step_says_what_it_does() {
        let s = steps(
            r#"[{"click": [0.713, 0.033]}, {"type": "quick"}, {"key": "Ctrl+A"},
                {"name": "down", "key": "PageDown", "repeat": 3, "every": 33},
                {"wait": 500}]"#,
        )
        .unwrap();
        let said: Vec<String> = s.iter().map(Step::describe).collect();
        assert_eq!(
            said,
            [
                "click 0.713 0.033",
                "type \"quick\"",
                "key Ctrl+A",
                "key PageDown ×3 every 33 ms",
                "wait 500 ms"
            ]
        );
        let inputs: Vec<u32> = s.iter().map(Step::inputs).collect();
        assert_eq!(inputs, [1, 5, 1, 3, 0]);
        assert_eq!(s[1].length(), Duration::from_millis(400));
        assert_eq!(s[3].length(), Duration::from_millis(99));
        assert_eq!(s[4].length(), Duration::from_millis(500));
        assert_eq!(s[3].name.as_deref(), Some("down"));
    }

    #[test]
    fn a_step_that_cannot_be_sent_is_refused() {
        let refused = |json: &str, says: &str| {
            let e = steps(json).unwrap_err();
            assert!(e.contains(says), "{e} does not say {says}");
        };
        refused(r#"[{"key": "Hyper+Q"}]"#, r#"no key "Hyper+Q""#);
        refused(r#"[{"key": "a", "wait": 10}]"#, "a step is one of");
        refused(r"[{}]", "a step is one of");
        refused(r#"[{"type": "ü"}]"#, "no key types 'ü'");
        refused(
            r#"[{"wait": 10, "repeat": 2}]"#,
            "only a key or a click repeats",
        );
        refused(
            r#"[{"type": "a", "every": 2}]"#,
            "only a key or a click repeats",
        );
        refused(r#"[{"key": "a", "repeat": 0}]"#, "at least once");
        refused(r#"[{"click": [1.5, 0]}]"#, "outside the window");
        refused(r#"[{"wait": 10, "hold": 2}]"#, "unknown field");
    }

    #[test]
    fn a_step_is_timed_from_its_input_to_what_answered_it() {
        let s = steps(r#"[{"key": "a"}, {"wait": 1000}, {"key": "b"}]"#).unwrap();
        let marks = [
            Marks {
                start: 0,
                sent: vec![10 * MS],
            },
            Marks {
                start: 80 * MS,
                sent: vec![],
            },
            Marks {
                start: 1080 * MS,
                sent: vec![1090 * MS],
            },
        ];
        // The key is answered at 26 ms and the answer goes on through the
        // wait, still from 120 ms; the second key is never answered.
        let changed = [26 * MS, 42 * MS, 120 * MS, 1500 * MS];
        let t = timings(&s, &marks, Some(&changed), 0, 2000 * MS);
        assert_eq!(t[0].lag_ms, Some(16.0));
        assert_eq!(t[0].settle_ms, Some(110.0));
        assert_eq!(t[0].frames, Some(2));
        assert_eq!(t[0].max_ms, Some(16.0));
        assert_eq!(t[1].frames, Some(1));
        assert_eq!(t[1].lag_ms, None);
        assert_eq!(t[1].settle_ms, None);
        assert_eq!(t[2].lag_ms, Some(410.0));
        assert_eq!(t[2].settle_ms, Some(410.0));
        assert_eq!(t[2].long, Some(0));
        assert_eq!(t[2].started_ms, 1080.0);
    }

    #[test]
    fn an_answer_that_never_stops_settles_when_the_next_step_sends() {
        let s = steps(r#"[{"key": "a"}, {"key": "b"}]"#).unwrap();
        let marks = [
            Marks {
                start: 0,
                sent: vec![0],
            },
            Marks {
                start: 300 * MS,
                sent: vec![300 * MS],
            },
        ];
        let changed: Vec<u64> = (1..40).map(|i| i * 16 * MS).collect();
        let t = timings(&s, &marks, Some(&changed), 0, 640 * MS);
        assert_eq!(t[0].settle_ms, Some(288.0));
        assert_eq!(t[0].frames, Some(18));
        // 640 ms is the end of the span, and nothing is still before it.
        assert_eq!(t[1].settle_ms, Some(324.0));
        let gappy = [100 * MS, 200 * MS];
        let t = timings(&s, &marks, Some(&gappy), 0, 640 * MS);
        assert_eq!(t[0].long, Some(1));
        assert_eq!(t[0].p50_ms, Some(100.0));
    }

    #[test]
    fn an_answer_is_bounded_by_the_next_input_sent_and_the_span() {
        let s = steps(r#"[{"key": "a"}, {"key": "b"}, {"key": "c"}]"#).unwrap();
        let marks = [
            Marks {
                start: 0,
                sent: vec![10 * MS],
            },
            // Its send failed: it does not end the answer before it.
            Marks {
                start: 100 * MS,
                sent: vec![],
            },
            Marks {
                start: 200 * MS,
                sent: vec![210 * MS],
            },
        ];
        // "a" is answered at 150 ms, after "b" started, as "b" sent nothing.
        let changed = [150 * MS, 260 * MS, 900 * MS];
        let t = timings(&s, &marks, Some(&changed), 0, 500 * MS);
        assert_eq!(t[0].lag_ms, Some(140.0));
        assert_eq!(t[0].settle_ms, Some(140.0));
        assert_eq!(t[2].lag_ms, Some(50.0));
        // The first frame after "a" comes after "c" was sent: it answers
        // "c", not "a"; one after the span answers nothing.
        let late = [220 * MS, 900 * MS];
        let t = timings(&s, &marks, Some(&late), 0, 500 * MS);
        assert_eq!(t[0].lag_ms, None);
        assert_eq!(t[2].lag_ms, Some(10.0));
        let after = [900 * MS];
        let t = timings(&s, &marks, Some(&after), 0, 500 * MS);
        assert_eq!(t[2].lag_ms, None);
    }

    #[test]
    fn without_frames_or_inputs_nothing_is_timed() {
        let s = steps(r#"[{"type": "ab"}]"#).unwrap();
        let marks = [Marks {
            start: 5 * MS,
            sent: vec![],
        }];
        let t = timings(&s, &marks, None, 0, 200 * MS);
        assert_eq!((t[0].inputs, t[0].sent), (2, 0));
        assert_eq!(
            (t[0].lag_ms, t[0].settle_ms, t[0].frames),
            (None, None, None)
        );
        assert_eq!(t[0].started_ms, 5.0);
        let json = serde_json::to_value(&t[0]).unwrap();
        assert!(json.get("inputs_ms").is_none());
        assert_eq!(json["lag_ms"], serde_json::Value::Null);
    }
}
