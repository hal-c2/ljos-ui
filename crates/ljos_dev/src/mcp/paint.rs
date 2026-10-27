//! What the host says about drawing the window: which painter, what each
//! pass cost to build and present, and when each step of the start-up
//! ended. The engine's lanes cannot tell a slow or stale window apart from
//! a slow scene; these can. `app_status` shows the painter and the
//! start-up, `app_metrics` the passes.

use std::fmt::Write as _;
use std::time::Duration;

use serde_json::{Value, json};

use super::frames::{ms, us};

/// What paints the primary window, as the host says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PainterInfo {
    /// The painter's name: `gl` or `wgpu` in the desktop shell.
    pub painter: String,
    /// The graphics interface under it (`Vulkan`, `Gl`, `EGL` …).
    pub backend: Option<String>,
    /// The device and its driver.
    pub adapter: Option<String>,
    /// How a frame reaches the display (`Fifo` waits for it).
    pub present_mode: Option<String>,
    /// Whether a pass can draw and present only what changed.
    pub partial_redraw: Option<bool>,
}

impl PainterInfo {
    fn json(&self) -> Value {
        json!({
            "painter": self.painter,
            "backend": self.backend,
            "adapter": self.adapter,
            "present_mode": self.present_mode,
            "partial_redraw": self.partial_redraw,
        })
    }

    fn line(&self) -> String {
        let mut line = format!("painted by {}", self.painter);
        if let Some(backend) = &self.backend {
            let _ = write!(line, " on {backend}");
        }
        if let Some(adapter) = &self.adapter {
            let _ = write!(line, ", {adapter}");
        }
        if let Some(mode) = &self.present_mode {
            let _ = write!(line, ", presents {mode}");
        }
        match self.partial_redraw {
            Some(true) => line.push_str(", redraws only what changed"),
            Some(false) => line.push_str(", redraws the whole window"),
            None => {}
        }
        line
    }
}

/// What a pass drew.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Drawn {
    /// The whole window, presented.
    #[default]
    Whole,
    /// Only where the picture changed, presented.
    Part,
    /// Nothing: the picture was the one shown, and nothing was presented.
    Unchanged,
}

/// A pass of the primary window the host painted, or did not because
/// nothing had changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintReport {
    pub drawn: Drawn,
    /// Turning the shapes into what the painter draws (tessellation).
    pub build: Duration,
    /// Uploading pictures, drawing, submitting and presenting.
    pub paint: Duration,
    /// The steps of `paint`, where the painter times them apart. They are
    /// the time the call took on this thread: a device works on after a
    /// call returns, and what it still had to do shows where the painter
    /// next waits for it, mostly in `present`. GL times all three; wgpu
    /// uploads first, then draws and presents in one call that times only
    /// the waits inside it (for the surface, the submit and the present),
    /// which are `present`, with the rest in `draw`.
    pub upload: Option<Duration>,
    pub draw: Option<Duration>,
    pub present: Option<Duration>,
    /// Pictures handed to the device in the pass, glyph atlas included.
    pub uploads: usize,
    pub upload_bytes: usize,
    /// Since the first repaint request the pass answered, when the host
    /// knows it.
    pub since_asked: Option<Duration>,
    /// The pass moved the pixels of a scrolled clip on screen and drew
    /// only what came into view.
    pub moved: bool,
}

/// One step of the start-up, timed from when the host started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupStep {
    /// The application is built: its QML loaded and its first scene made.
    Built,
    /// The graphics device can draw into the first window.
    Device,
    /// The first frame was presented.
    FirstFrame,
    /// The first input from the user was handled.
    FirstInput,
}

const STEPS: [(StartupStep, &str, &str); 4] = [
    (StartupStep::Built, "built_ms", "application built"),
    (StartupStep::Device, "device_ms", "device ready"),
    (
        StartupStep::FirstFrame,
        "first_frame_ms",
        "first frame presented",
    ),
    (
        StartupStep::FirstInput,
        "first_input_ms",
        "first input handled",
    ),
];

/// When each step ended; the first time a step is recorded counts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Startup([Option<Duration>; 4]);

impl Startup {
    pub(crate) fn record(&mut self, step: StartupStep, at: Duration) {
        let i = STEPS.iter().position(|(s, ..)| *s == step).unwrap_or(0);
        self.0[i].get_or_insert(at);
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    fn json(&self) -> Value {
        let mut map = serde_json::Map::new();
        for ((_, key, _), at) in STEPS.iter().zip(self.0) {
            map.insert((*key).into(), json!(at.map(|d| d.as_millis() as u64)));
        }
        Value::Object(map)
    }

    fn line(&self) -> String {
        let steps: Vec<String> = STEPS
            .iter()
            .zip(self.0)
            .filter_map(|((_, _, name), at)| Some(format!("{name} at {} ms", at?.as_millis())))
            .collect();
        format!("start-up: {}", steps.join(", "))
    }
}

/// What `app_status` adds from the host: `painter` and `startup`, and a
/// line of text for each it knows.
pub(crate) fn status(painter: Option<&PainterInfo>, startup: &Startup) -> (Value, Value, String) {
    let mut text = String::new();
    if let Some(painter) = painter {
        let _ = write!(text, "\n{}", painter.line());
    }
    if !startup.is_empty() {
        let _ = write!(text, "\n{}", startup.line());
    }
    (
        painter.map_or(Value::Null, PainterInfo::json),
        startup.json(),
        text,
    )
}

/// Last, longest and all of a cost.
#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    last: Duration,
    max: Duration,
    total: Duration,
}

impl Cost {
    fn add(&mut self, d: Duration) {
        self.last = d;
        self.max = self.max.max(d);
        self.total += d;
    }

    fn json(&self) -> Value {
        json!({ "last": us(self.last), "max": us(self.max), "total": us(self.total) })
    }
}

/// The passes of the primary window, for `app_metrics`.
#[derive(Clone, Debug, Default)]
pub(crate) struct PaintStats {
    presented: u64,
    in_part: u64,
    moved: u64,
    unchanged: u64,
    build: Cost,
    paint: Cost,
    upload: Option<Cost>,
    draw: Option<Cost>,
    present: Option<Cost>,
    uploads: u64,
    upload_bytes: u64,
    asked: Option<Cost>,
}

impl PaintStats {
    pub(crate) fn record(&mut self, report: &PaintReport) {
        if report.drawn == Drawn::Unchanged {
            self.unchanged += 1;
        } else {
            self.presented += 1;
            if report.drawn == Drawn::Part {
                self.in_part += 1;
            }
            if report.moved {
                self.moved += 1;
            }
            self.build.add(report.build);
            self.paint.add(report.paint);
            for (cost, step) in [
                (&mut self.upload, report.upload),
                (&mut self.draw, report.draw),
                (&mut self.present, report.present),
            ] {
                if let Some(t) = step {
                    cost.get_or_insert_with(Cost::default).add(t);
                }
            }
            if let Some(since) = report.since_asked {
                self.asked.get_or_insert_with(Cost::default).add(since);
            }
        }
        // A pass that presents nothing still hands pictures over.
        self.uploads += report.uploads as u64;
        self.upload_bytes += report.upload_bytes as u64;
    }

    pub(crate) fn json(&self) -> Value {
        json!({
            "presented": self.presented,
            "presented_in_part": self.in_part,
            "moved": self.moved,
            "unchanged": self.unchanged,
            "build_us": self.build.json(),
            "paint_us": self.paint.json(),
            "upload_us": self.upload.as_ref().map_or(Value::Null, Cost::json),
            "draw_us": self.draw.as_ref().map_or(Value::Null, Cost::json),
            "present_us": self.present.as_ref().map_or(Value::Null, Cost::json),
            "uploads": self.uploads,
            "upload_bytes": self.upload_bytes,
            "asked_to_presented_us": self.asked.as_ref().map_or(Value::Null, Cost::json),
        })
    }

    /// The text of `app_metrics`; nothing until the host reports a pass.
    pub(crate) fn text(&self) -> String {
        if self.presented + self.unchanged == 0 {
            return String::new();
        }
        let mut text = format!(
            "\npaint: {} passes presented ({} in part, {} moving a scrolled clip), {} unchanged and not presented\n\
             building last {}, longest {}; uploading, drawing and presenting last {}, longest {}\n\
             {} pictures uploaded, {} bytes",
            self.presented,
            self.in_part,
            self.moved,
            self.unchanged,
            ms(self.build.last),
            ms(self.build.max),
            ms(self.paint.last),
            ms(self.paint.max),
            self.uploads,
            self.upload_bytes,
        );
        let steps: Vec<String> = [
            ("uploading", &self.upload),
            ("drawing", &self.draw),
            ("presenting", &self.present),
        ]
        .into_iter()
        .filter_map(|(name, cost)| {
            let cost = cost.as_ref()?;
            Some(format!(
                "{name} last {}, longest {}",
                ms(cost.last),
                ms(cost.max)
            ))
        })
        .collect();
        if !steps.is_empty() {
            let _ = write!(text, "\n{}", steps.join("; "));
        }
        if let Some(asked) = &self.asked {
            let _ = write!(
                text,
                "; repaint asked to presented last {}, longest {}",
                ms(asked.last),
                ms(asked.max)
            );
        }
        text
    }
}
