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
    /// Drawn, but it never reached the window: the painter had lost its
    /// surface, or the window system gave it none to draw into.
    Lost,
}

impl Drawn {
    /// Whether the window shows the pass's picture: only those are the
    /// paints a client waits for.
    pub fn presented(self) -> bool {
        matches!(self, Drawn::Whole | Drawn::Part)
    }
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
    /// How long the GPU took to draw the 3D views, from the timings read
    /// back during the pass: those of an earlier pass's drawing, as
    /// nothing waits for the GPU. `None` when none came back, or the device
    /// cannot time without waiting.
    pub gpu_3d: Option<Duration>,
    /// Of the same readings, how long the GPU took for the views'
    /// post-processing passes (the camera's depth, ambient occlusion,
    /// effects, depth of field, glow, the final tonemap and FXAA), which
    /// `gpu_3d` leaves out. `None` where no view post-processes.
    pub gpu_post: Option<Duration>,
    /// Pictures handed to the device in the pass, glyph atlas included.
    pub uploads: usize,
    pub upload_bytes: usize,
    /// Since the first repaint request the pass answered, when the host
    /// knows it.
    pub since_asked: Option<Duration>,
    /// The pass moved the pixels of a scrolled clip on screen and drew
    /// only what came into view.
    pub moved: bool,
    /// What the last 3D frame handed to the device since the pass before
    /// cost, as the 3D renderer counted it. `None` when none was.
    pub scene3d: Option<Scene3dFrame>,
}

/// What one 3D frame cost: every 3D view the host drew before one submit.
/// Qt's `renderStats` names the same counts where it has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scene3dFrame {
    pub draw_calls: u64,
    pub vertices: u64,
    pub render_passes: u64,
    /// Pipelines set on a pass.
    pub pipeline_switches: u64,
    pub bind_group_sets: u64,
    pub buffer_writes: u64,
    pub buffer_write_bytes: u64,
    pub texture_uploads: u64,
    pub texture_upload_bytes: u64,
    pub pipelines_created: u64,
    pub shader_modules_created: u64,
    pub models_drawn: u64,
    pub models_culled: u64,
    /// Reading the QML scene into the renderer's.
    pub sync: Duration,
    /// Walking, culling and sorting the scene and writing its uniforms.
    pub prepare: Duration,
    /// Recording the passes.
    pub encode: Duration,
    /// Handing them to the device.
    pub submit: Duration,
}

impl Scene3dFrame {
    pub(crate) fn json(&self) -> Value {
        json!({
            "draw_calls": self.draw_calls,
            "vertices": self.vertices,
            "render_passes": self.render_passes,
            "pipeline_switches": self.pipeline_switches,
            "bind_group_sets": self.bind_group_sets,
            "buffer_writes": self.buffer_writes,
            "buffer_write_bytes": self.buffer_write_bytes,
            "texture_uploads": self.texture_uploads,
            "texture_upload_bytes": self.texture_upload_bytes,
            "pipelines_created": self.pipelines_created,
            "shader_modules_created": self.shader_modules_created,
            "models_drawn": self.models_drawn,
            "models_culled": self.models_culled,
            "sync_us": us(self.sync),
            "prepare_us": us(self.prepare),
            "encode_us": us(self.encode),
            "submit_us": us(self.submit),
        })
    }
}

/// The 3D frames of the passes: the last one's counts, and the CPU each
/// part took over all of them.
#[derive(Clone, Debug, Default)]
struct Scene3dStats {
    frames: u64,
    last: Scene3dFrame,
    sync: Cost,
    prepare: Cost,
    encode: Cost,
    submit: Cost,
}

impl Scene3dStats {
    fn add(&mut self, f: &Scene3dFrame) {
        self.frames += 1;
        self.last = *f;
        self.sync.add(f.sync);
        self.prepare.add(f.prepare);
        self.encode.add(f.encode);
        self.submit.add(f.submit);
    }

    fn json(&self) -> Value {
        json!({
            "frames": self.frames,
            "last": self.last.json(),
            "sync_us": self.sync.json(),
            "prepare_us": self.prepare.json(),
            "encode_us": self.encode.json(),
            "submit_us": self.submit.json(),
        })
    }

    fn line(&self) -> String {
        let f = &self.last;
        format!(
            "3D: {} frames; the last drew {} times with {} pipeline switches and {} bind groups set, \
             wrote {} buffers ({} bytes) and {} textures ({} bytes), made {} pipelines; \
             reading the scene last {}, preparing {}, recording {}, submitting {}",
            self.frames,
            f.draw_calls,
            f.pipeline_switches,
            f.bind_group_sets,
            f.buffer_writes,
            f.buffer_write_bytes,
            f.texture_uploads,
            f.texture_upload_bytes,
            f.pipelines_created,
            ms(f.sync),
            ms(f.prepare),
            ms(f.encode),
            ms(f.submit),
        )
    }
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
    lost: u64,
    build: Cost,
    paint: Cost,
    upload: Option<Cost>,
    draw: Option<Cost>,
    present: Option<Cost>,
    gpu_3d: Option<Cost>,
    gpu_post: Option<Cost>,
    uploads: u64,
    upload_bytes: u64,
    asked: Option<Cost>,
    scene3d: Option<Scene3dStats>,
}

impl PaintStats {
    pub(crate) fn record(&mut self, report: &PaintReport) {
        if report.drawn == Drawn::Unchanged {
            self.unchanged += 1;
        } else if report.drawn == Drawn::Lost {
            self.lost += 1;
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
        // A reading can come back with any pass, drawn or not.
        for (cost, t) in [
            (&mut self.gpu_3d, report.gpu_3d),
            (&mut self.gpu_post, report.gpu_post),
        ] {
            if let Some(t) = t {
                cost.get_or_insert_with(Cost::default).add(t);
            }
        }
        if let Some(f) = &report.scene3d {
            self.scene3d
                .get_or_insert_with(Scene3dStats::default)
                .add(f);
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
            "lost": self.lost,
            "build_us": self.build.json(),
            "paint_us": self.paint.json(),
            "upload_us": self.upload.as_ref().map_or(Value::Null, Cost::json),
            "draw_us": self.draw.as_ref().map_or(Value::Null, Cost::json),
            "present_us": self.present.as_ref().map_or(Value::Null, Cost::json),
            "gpu_3d_us": self.gpu_3d.as_ref().map_or(Value::Null, Cost::json),
            "gpu_post_us": self.gpu_post.as_ref().map_or(Value::Null, Cost::json),
            "uploads": self.uploads,
            "upload_bytes": self.upload_bytes,
            "asked_to_presented_us": self.asked.as_ref().map_or(Value::Null, Cost::json),
            "scene3d": self.scene3d.as_ref().map_or(Value::Null, Scene3dStats::json),
        })
    }

    /// The text of `app_metrics`; nothing until the host reports a pass.
    pub(crate) fn text(&self) -> String {
        if self.presented + self.unchanged + self.lost == 0 {
            return String::new();
        }
        let lost = if self.lost > 0 {
            format!(", {} lost before reaching the window", self.lost)
        } else {
            String::new()
        };
        let mut text = format!(
            "\npaint: {} passes presented ({} in part, {} moving a scrolled clip), {} unchanged and not presented{lost}\n\
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
            ("drawing 3D on the GPU", &self.gpu_3d),
            ("post-processing 3D on the GPU", &self.gpu_post),
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
        if let Some(scene3d) = &self.scene3d {
            let _ = write!(text, "\n{}", scene3d.line());
        }
        text
    }
}
