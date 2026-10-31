//! The recent frames one by one, for `app_metrics`: how their times are
//! spread, and a log of those that went over budget with what took the
//! time. Totals hide the one bad frame; these find it and say why.
//!
//! A frame is the engine's (its lanes and owners) and the pass of the
//! primary window that painted it, when the host painted one. Nothing is
//! kept while no frame runs, so an idle application costs nothing here.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::Duration;

use ljos_engine::Engine;
use ljos_runtime::{FrameReport, Lane, ProcId};
use serde_json::{Map, Value, json};

use super::paint::{Drawn, PaintReport};
use crate::top::{LANE_NAMES, owner_name};

/// Frames whose times are spread: four seconds at 60 Hz.
const RECENT: usize = 240;
/// Slow frames kept, the oldest dropped first.
const SLOW: usize = 32;

/// One frame.
#[derive(Clone, Debug)]
struct Entry {
    /// Counted from 1 by the frames recorded.
    seq: u64,
    elapsed: Duration,
    overrun: bool,
    lanes: [Duration; Lane::COUNT],
    /// The owner charged the most.
    owner: Option<(ProcId, Duration)>,
    paint: Option<PaintReport>,
}

impl Entry {
    fn painting(&self) -> Duration {
        self.paint.map_or(Duration::ZERO, |p| p.build + p.paint)
    }

    fn total(&self) -> Duration {
        self.elapsed + self.painting()
    }

    /// The part of the frame that took the most time: a lane, `frame`
    /// (the engine's frame outside its lanes), or a step of painting.
    fn cause(&self) -> (&'static str, Duration) {
        let mut parts: Vec<(&'static str, Duration)> =
            LANE_NAMES.iter().copied().zip(self.lanes).collect();
        let in_lanes: Duration = self.lanes.iter().sum();
        parts.push(("frame", self.elapsed.saturating_sub(in_lanes)));
        if let Some(p) = &self.paint {
            parts.push(("build", p.build));
            if p.upload.is_none() && p.draw.is_none() && p.present.is_none() {
                parts.push(("paint", p.paint));
            }
            for (name, step) in [
                ("upload", p.upload),
                ("draw", p.draw),
                ("present", p.present),
            ] {
                if let Some(t) = step {
                    parts.push((name, t));
                }
            }
        }
        // The first of equals: lanes come before painting.
        parts
            .into_iter()
            .fold(("frame", Duration::ZERO), |most, part| {
                if part.1 > most.1 { part } else { most }
            })
    }

    fn json(&self, engine: &Engine) -> Value {
        let (cause, took) = self.cause();
        let lanes: Map<String, Value> = LANE_NAMES
            .iter()
            .zip(self.lanes)
            .map(|(name, t)| ((*name).to_owned(), json!(us(t))))
            .collect();
        json!({
            "frame": self.seq,
            "total_us": us(self.total()),
            "frame_us": us(self.elapsed),
            "overrun": self.overrun,
            "cause": cause,
            "cause_us": us(took),
            "owner": self.owner.map(|(proc, _)| owner_name(engine, proc)),
            "lanes": lanes,
            "paint": self.paint.as_ref().map(paint_json),
        })
    }

    fn line(&self, engine: &Engine) -> String {
        let (cause, took) = self.cause();
        let what = if LANE_NAMES.contains(&cause) {
            format!("the {cause} lane")
        } else if cause == "frame" {
            "the frame outside its lanes".to_owned()
        } else {
            cause.to_owned()
        };
        let mut line = format!(
            "  frame {}: {}, mostly {what} ({})",
            self.seq,
            ms(self.total()),
            ms(took)
        );
        if let Some((proc, _)) = self.owner {
            let _ = write!(line, ", {}", owner_name(engine, proc));
        }
        if self.overrun {
            line.push_str("; the engine's deadline passed");
        }
        line
    }
}

pub(super) fn paint_json(p: &PaintReport) -> Value {
    json!({
        "drawn": match p.drawn {
            Drawn::Whole => "whole",
            Drawn::Part => "part",
            Drawn::Unchanged => "unchanged",
        },
        "build_us": us(p.build),
        "paint_us": us(p.paint),
        "upload_us": p.upload.map(us),
        "draw_us": p.draw.map(us),
        "present_us": p.present.map(us),
        "gpu_3d_us": p.gpu_3d.map(us),
        "gpu_post_us": p.gpu_post.map(us),
        "moved": p.moved,
    })
}

pub(super) fn us(d: Duration) -> u64 {
    d.as_micros() as u64
}

pub(super) fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

/// The 50th and 95th percentile and the largest of `times`, by nearest
/// rank.
pub(super) fn spread(mut times: Vec<Duration>) -> Option<[Duration; 3]> {
    if times.is_empty() {
        return None;
    }
    times.sort_unstable();
    let rank = |p: f64| times[((p * times.len() as f64).ceil() as usize).clamp(1, times.len()) - 1];
    Some([rank(0.5), rank(0.95), times[times.len() - 1]])
}

fn spread_json(s: Option<[Duration; 3]>) -> Value {
    s.map_or(
        Value::Null,
        |[p50, p95, max]| json!({ "p50": us(p50), "p95": us(p95), "max": us(max) }),
    )
}

/// The recent frames and the slow ones.
#[derive(Clone, Debug)]
pub(crate) struct FrameLog {
    /// A frame that takes longer is slow: a frame of the display.
    budget: Duration,
    seq: u64,
    recent: VecDeque<Entry>,
    slow: VecDeque<Entry>,
}

impl Default for FrameLog {
    fn default() -> Self {
        Self {
            budget: Duration::from_micros(16_667),
            seq: 0,
            recent: VecDeque::new(),
            slow: VecDeque::new(),
        }
    }
}

impl FrameLog {
    pub(crate) fn set_budget(&mut self, budget: Duration) {
        if budget > Duration::ZERO {
            self.budget = budget;
        }
    }

    pub(crate) fn record_frame(&mut self, report: &FrameReport) {
        self.seq += 1;
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(Entry {
            seq: self.seq,
            elapsed: report.elapsed,
            overrun: report.overrun,
            lanes: report.per_lane,
            owner: report.top_owners.first().copied(),
            paint: None,
        });
        self.settle();
    }

    /// The pass that painted the last frame. A second pass after one
    /// frame (egui runs one more) is not that frame's.
    pub(crate) fn record_paint(&mut self, report: &PaintReport) {
        if let Some(last) = self.recent.back_mut()
            && last.paint.is_none()
        {
            last.paint = Some(*report);
            self.settle();
        }
    }

    /// Logs the last frame if it is slow, once, with what is known of it.
    fn settle(&mut self) {
        let Some(last) = self.recent.back() else {
            return;
        };
        if !last.overrun && last.total() <= self.budget {
            return;
        }
        match self.slow.back_mut() {
            Some(logged) if logged.seq == last.seq => *logged = last.clone(),
            _ => {
                if self.slow.len() == SLOW {
                    self.slow.pop_front();
                }
                self.slow.push_back(last.clone());
            }
        }
    }

    /// `recent` and `slow` for `app_metrics`.
    pub(crate) fn json(&self, engine: &Engine) -> (Value, Value) {
        let painted: Vec<Duration> = self
            .recent
            .iter()
            .filter(|e| e.paint.is_some_and(|p| p.drawn != Drawn::Unchanged))
            .map(Entry::painting)
            .collect();
        let recent = json!({
            "frames": self.recent.len(),
            "budget_us": us(self.budget),
            "total_us": spread_json(spread(self.recent.iter().map(Entry::total).collect())),
            "frame_us": spread_json(spread(self.recent.iter().map(|e| e.elapsed).collect())),
            "paint_us": spread_json(spread(painted)),
        });
        let slow = self.slow.iter().map(|e| e.json(engine)).collect();
        (recent, Value::Array(slow))
    }

    /// The text of `app_metrics`; nothing until a frame is recorded.
    pub(crate) fn text(&self, engine: &Engine) -> String {
        let Some([p50, p95, max]) = spread(self.recent.iter().map(Entry::total).collect()) else {
            return String::new();
        };
        let mut text = format!(
            "\nlast {} frames: p50 {}, p95 {}, longest {} (budget {})",
            self.recent.len(),
            ms(p50),
            ms(p95),
            ms(max),
            ms(self.budget)
        );
        if !self.slow.is_empty() {
            let _ = write!(
                text,
                "\nslow frames (over {}), oldest first:",
                ms(self.budget)
            );
            for entry in &self.slow {
                let _ = write!(text, "\n{}", entry.line(engine));
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_by_nearest_rank() {
        let ms = |n: u64| Duration::from_millis(n);
        assert_eq!(spread(Vec::new()), None);
        assert_eq!(spread(vec![ms(4)]), Some([ms(4), ms(4), ms(4)]));
        let one_to_twenty = (1..=20).rev().map(ms).collect();
        assert_eq!(spread(one_to_twenty), Some([ms(10), ms(19), ms(20)]));
    }
}
