//! What the renderer did lately, for a readout (`DebugView`,
//! `View3D.renderStats`), the inspector and the bench to show.
//!
//! Two kinds of number. [`RenderStats`] is one record for the process:
//! every [`Renderer`](crate::gpu::Renderer) writes it when it draws a
//! scene, and a reader takes a copy. A reader never waits on the renderer
//! for more than the copy.
//!
//! [`FrameCounters`] is what one frame of a host cost: the draws, passes,
//! pipeline and bind-group changes, uploads and creations the renderer
//! made, and its CPU time in parts. The renderer counts into a tally of
//! the thread that draws, which costs an add and no lock; the host ends
//! the frame with [`frame_end`] once it has submitted, which moves the
//! tally into the record. Nothing is counted or written when no scene is
//! drawn. With `LJOS_3D_FRAMES=<file>` set, every frame ended is also one
//! JSON line in that file, for `ljos bench --frames3d`.

use std::cell::RefCell;
use std::sync::Mutex;

/// What the last draw of a scene took, and how often scenes are drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RenderStats {
    /// Scenes drawn so far.
    pub renders: u64,
    /// Draw calls of the last scene's own pass, its sky included.
    pub draw_calls: u32,
    /// Shadow-map passes drawn so far, which only grows.
    pub shadow_passes: u64,
    /// Seconds from the start of the scene drawn before to this one's: how
    /// long a frame is while the scene is animated, and a gap when it is
    /// not. 0 before the second scene.
    pub frame_seconds: f64,
    /// The longest of those since [`take_longest_frame`] last asked.
    pub longest_frame_seconds: f64,
    /// Seconds the GPU took for the last scene, its post-processing
    /// included, when the host measures it ([`set_gpu_seconds`]).
    pub gpu_seconds: Option<f64>,
    /// Of those, the seconds of the passes after the scene pass, where the
    /// environment post-processes.
    pub gpu_post_seconds: Option<f64>,
    /// Frames a host ended ([`frame_end`]) with a scene drawn in them.
    pub frames: u64,
    /// What the last of those cost.
    pub last: FrameCounters,
    /// What all of them cost together.
    pub total: FrameCounters,
}

/// What one frame of a host cost in 3D: every view it drew, every pass of
/// each (shadows, depth, the scene, post-processing), and what was made or
/// uploaded for it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameCounters {
    /// Views drawn.
    pub views: u64,
    /// Draw calls, of every pass.
    pub draw_calls: u64,
    /// Vertices those draw, each copy of an instanced model counted.
    pub vertices: u64,
    /// Render passes begun.
    pub render_passes: u64,
    /// Pipelines set on a pass.
    pub pipeline_switches: u64,
    /// Bind groups set on a pass.
    pub bind_group_sets: u64,
    /// Writes into buffers, a buffer made with its contents included.
    pub buffer_writes: u64,
    /// The bytes of those.
    pub buffer_write_bytes: u64,
    /// Writes into textures.
    pub texture_uploads: u64,
    /// The bytes of those.
    pub texture_upload_bytes: u64,
    /// Render pipelines made.
    pub pipelines_created: u64,
    /// Shader modules made.
    pub shader_modules_created: u64,
    /// Buffers made.
    pub buffers_created: u64,
    /// Textures made.
    pub textures_created: u64,
    /// Bind groups made.
    pub bind_groups_created: u64,
    /// Models the camera sees, drawn in the scene pass.
    pub models_drawn: u64,
    /// Models left out because the camera does not see them.
    pub models_culled: u64,
    /// Objects of the scene read again since the frame before: none when
    /// nothing in it changed, one for an object that changed, every one
    /// of them for a scene read whole.
    pub objects_read: u64,
    /// Seconds spent reading the QML scene into the renderer's since the
    /// frame before, on whichever thread did it.
    pub sync_seconds: f64,
    /// Seconds the renderer spent before its first pass: walking the
    /// scene, culling, sorting, writing uniforms, making pipelines.
    pub prepare_seconds: f64,
    /// Seconds the renderer spent recording its passes.
    pub encode_seconds: f64,
    /// Seconds the host's submit of them took.
    pub submit_seconds: f64,
}

impl FrameCounters {
    const ZERO: Self = Self {
        views: 0,
        draw_calls: 0,
        vertices: 0,
        render_passes: 0,
        pipeline_switches: 0,
        bind_group_sets: 0,
        buffer_writes: 0,
        buffer_write_bytes: 0,
        texture_uploads: 0,
        texture_upload_bytes: 0,
        pipelines_created: 0,
        shader_modules_created: 0,
        buffers_created: 0,
        textures_created: 0,
        bind_groups_created: 0,
        models_drawn: 0,
        models_culled: 0,
        objects_read: 0,
        sync_seconds: 0.0,
        prepare_seconds: 0.0,
        encode_seconds: 0.0,
        submit_seconds: 0.0,
    };

    fn add(&mut self, o: &Self) {
        self.views += o.views;
        self.draw_calls += o.draw_calls;
        self.vertices += o.vertices;
        self.render_passes += o.render_passes;
        self.pipeline_switches += o.pipeline_switches;
        self.bind_group_sets += o.bind_group_sets;
        self.buffer_writes += o.buffer_writes;
        self.buffer_write_bytes += o.buffer_write_bytes;
        self.texture_uploads += o.texture_uploads;
        self.texture_upload_bytes += o.texture_upload_bytes;
        self.pipelines_created += o.pipelines_created;
        self.shader_modules_created += o.shader_modules_created;
        self.buffers_created += o.buffers_created;
        self.textures_created += o.textures_created;
        self.bind_groups_created += o.bind_groups_created;
        self.models_drawn += o.models_drawn;
        self.models_culled += o.models_culled;
        self.objects_read += o.objects_read;
        self.sync_seconds += o.sync_seconds;
        self.prepare_seconds += o.prepare_seconds;
        self.encode_seconds += o.encode_seconds;
        self.submit_seconds += o.submit_seconds;
    }

    /// The CPU seconds of the frame: reading the scene, preparing,
    /// recording and submitting it.
    #[must_use]
    pub fn cpu_seconds(&self) -> f64 {
        self.sync_seconds + self.prepare_seconds + self.encode_seconds + self.submit_seconds
    }
}

#[derive(Default)]
struct Record {
    stats: RenderStats,
    /// Sync seconds not yet part of an ended frame: the scene is read on
    /// the engine's thread, which need not be the one that draws.
    sync: f64,
    /// Likewise the objects read for it.
    read: u64,
    #[cfg(not(target_arch = "wasm32"))]
    last: Option<std::time::Instant>,
}

static RECORD: Mutex<Record> = Mutex::new(Record {
    stats: RenderStats {
        renders: 0,
        draw_calls: 0,
        shadow_passes: 0,
        frame_seconds: 0.0,
        longest_frame_seconds: 0.0,
        gpu_seconds: None,
        gpu_post_seconds: None,
        frames: 0,
        last: FrameCounters::ZERO,
        total: FrameCounters::ZERO,
    },
    sync: 0.0,
    read: 0,
    #[cfg(not(target_arch = "wasm32"))]
    last: None,
});

thread_local! {
    static TALLY: RefCell<FrameCounters> = const { RefCell::new(FrameCounters::ZERO) };
}

fn record() -> std::sync::MutexGuard<'static, Record> {
    // A reader that panicked holds numbers, not an invariant.
    RECORD.lock().unwrap_or_else(|p| p.into_inner())
}

/// Adds to what this thread's frame has cost so far.
pub fn count(f: impl FnOnce(&mut FrameCounters)) {
    TALLY.with(|t| f(&mut t.borrow_mut()));
}

/// What the renderer did, as of now.
pub fn snapshot() -> RenderStats {
    record().stats
}

/// The longest frame since the last call, which starts the next count.
pub fn take_longest_frame() -> f64 {
    std::mem::take(&mut record().stats.longest_frame_seconds)
}

/// Called by the renderer when it has drawn a scene.
pub fn record_render(draw_calls: u32, shadow_passes: u64) {
    count(|c| c.views += 1);
    let mut r = record();
    r.stats.renders += 1;
    r.stats.draw_calls = draw_calls;
    r.stats.shadow_passes = shadow_passes;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let now = std::time::Instant::now();
        r.stats.frame_seconds = r
            .last
            .map_or(0.0, |before| now.duration_since(before).as_secs_f64());
        r.stats.longest_frame_seconds = r.stats.longest_frame_seconds.max(r.stats.frame_seconds);
        r.last = Some(now);
    }
}

/// Seconds spent reading a QML scene into the renderer's, for the next
/// frame ended to count.
pub fn add_sync_seconds(seconds: f64) {
    record().sync += seconds;
}

/// Objects of a QML scene read again, for the next frame ended to count.
pub fn add_objects_read(objects: u64) {
    record().read += objects;
}

/// The host has submitted its frame, which took `submit_seconds` beyond
/// the submits already counted in it (an off-screen picture's). When a
/// scene was drawn in it on this thread, what the frame cost becomes the
/// record's last frame (and a line of `LJOS_3D_FRAMES`). Otherwise what
/// was drawn is let go, since it was the host showing a picture it already
/// had while its 2D repainted, and what was made or uploaded waits for a
/// frame that draws a scene: it is that scene's cost. Returns the frame
/// ended.
pub fn frame_end(submit_seconds: f64) -> Option<FrameCounters> {
    let mut frame = TALLY.with(|t| {
        let mut t = t.borrow_mut();
        if t.views == 0 {
            t.draw_calls = 0;
            t.vertices = 0;
            t.render_passes = 0;
            t.pipeline_switches = 0;
            t.bind_group_sets = 0;
            t.submit_seconds = 0.0;
            return None;
        }
        Some(std::mem::replace(&mut *t, FrameCounters::ZERO))
    })?;
    frame.submit_seconds += submit_seconds;
    let (index, gpu) = {
        let mut r = record();
        frame.sync_seconds = std::mem::take(&mut r.sync);
        frame.objects_read = std::mem::take(&mut r.read);
        r.stats.frames += 1;
        r.stats.last = frame;
        r.stats.total.add(&frame);
        (r.stats.frames, r.stats.gpu_seconds)
    };
    sink::write(index, &frame, gpu);
    Some(frame)
}

/// The host says how long the GPU took for the last scene, and for its
/// post-processing passes of that: the renderer's timestamps come back
/// frames later, through the host that asks for them.
pub fn set_gpu_seconds(seconds: Option<f64>, post: Option<f64>) {
    let mut r = record();
    r.stats.gpu_seconds = seconds;
    r.stats.gpu_post_seconds = post;
}

/// A clock for the CPU split, which reads nothing on the web: a page has
/// no `Instant`, and its numbers come from the browser's own tools.
#[derive(Debug, Clone, Copy)]
pub struct Stopwatch {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
}

impl Stopwatch {
    #[must_use]
    pub fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
        }
    }

    /// Seconds since [`Self::start`]; 0 on the web.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64()
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }

    /// The seconds since the last lap (or the start), and starts the next.
    pub fn lap(&mut self) -> f64 {
        let s = self.seconds();
        *self = Self::start();
        s
    }
}

/// The variable naming the file each ended frame is a JSON line of.
pub const FRAMES_ENV: &str = "LJOS_3D_FRAMES";

#[cfg(not(target_arch = "wasm32"))]
mod sink {
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::sync::{Mutex, OnceLock};

    use super::FrameCounters;

    struct Sink {
        file: std::fs::File,
        line: String,
    }

    /// Opened at the first frame ended, from [`super::FRAMES_ENV`]; unset, a
    /// frame costs one load of this to learn so.
    static SINK: OnceLock<Option<Mutex<Sink>>> = OnceLock::new();

    fn sink() -> Option<&'static Mutex<Sink>> {
        SINK.get_or_init(|| {
            let path = std::env::var_os(super::FRAMES_ENV)?;
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| {
                    // The scene alone links no tracing; this is said once.
                    eprintln!(
                        "ljos_3d: {} {}: {e}",
                        super::FRAMES_ENV,
                        std::path::Path::new(&path).display()
                    );
                })
                .ok()?;
            Some(Mutex::new(Sink {
                file,
                line: String::with_capacity(768),
            }))
        })
        .as_ref()
    }

    pub(super) fn write(index: u64, f: &FrameCounters, gpu: Option<f64>) {
        let Some(sink) = sink() else { return };
        let mut s = sink.lock().unwrap_or_else(|p| p.into_inner());
        let Sink { file, line } = &mut *s;
        line.clear();
        let t_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_micros());
        let ms = |s: f64| s * 1000.0;
        // A String takes its writes without failing.
        let _ = write!(
            line,
            "{{\"t_us\":{t_us},\"frame\":{index},\"views\":{},\"draw_calls\":{},\"vertices\":{},\
             \"render_passes\":{},\"pipeline_switches\":{},\"bind_group_sets\":{},\
             \"buffer_writes\":{},\"buffer_write_bytes\":{},\"texture_uploads\":{},\
             \"texture_upload_bytes\":{},\"pipelines_created\":{},\"shader_modules_created\":{},\
             \"buffers_created\":{},\"textures_created\":{},\"bind_groups_created\":{},\
             \"models_drawn\":{},\"models_culled\":{},\"objects_read\":{},\"sync_ms\":{:.4},\"prepare_ms\":{:.4},\
             \"encode_ms\":{:.4},\"submit_ms\":{:.4},\"gpu_ms\":",
            f.views,
            f.draw_calls,
            f.vertices,
            f.render_passes,
            f.pipeline_switches,
            f.bind_group_sets,
            f.buffer_writes,
            f.buffer_write_bytes,
            f.texture_uploads,
            f.texture_upload_bytes,
            f.pipelines_created,
            f.shader_modules_created,
            f.buffers_created,
            f.textures_created,
            f.bind_groups_created,
            f.models_drawn,
            f.models_culled,
            f.objects_read,
            ms(f.sync_seconds),
            ms(f.prepare_seconds),
            ms(f.encode_seconds),
            ms(f.submit_seconds),
        );
        let _ = match gpu {
            Some(g) => writeln!(line, "{:.4}}}", ms(g)),
            None => writeln!(line, "null}}"),
        };
        // A line is written whole or not at all, so a reader that stops
        // the application mid-frame reads only whole lines but the last.
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(target_arch = "wasm32")]
mod sink {
    pub(super) fn write(_: u64, _: &super::FrameCounters, _: Option<f64>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_frame_with_no_scene_drew_is_not_the_next_ones() {
        std::thread::spawn(|| {
            // A host showing last frame's picture again: a draw, a pipeline
            // and a bind group on its pass, and a pipeline made for it.
            count(|c| {
                c.draw_calls += 1;
                c.vertices += 3;
                c.render_passes += 1;
                c.pipeline_switches += 1;
                c.bind_group_sets += 1;
                c.pipelines_created += 1;
            });
            assert_eq!(frame_end(0.0), None);
            count(|c| c.draw_calls += 5);
            record_render(5, 0);
            let frame = frame_end(0.0).expect("a scene was drawn");
            assert_eq!(
                (frame.draw_calls, frame.vertices, frame.render_passes),
                (5, 0, 0)
            );
            assert_eq!((frame.pipeline_switches, frame.bind_group_sets), (0, 0));
            // Made is made: the first scene's frame pays for it.
            assert_eq!(frame.pipelines_created, 1);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_frame_with_no_scene_drawn_is_not_a_frame() {
        // Its own thread, so its tally is its own.
        std::thread::spawn(|| {
            count(|c| c.buffers_created += 2);
            assert_eq!(frame_end(0.0), None, "nothing drawn, nothing ended");
            count(|c| c.draw_calls += 3);
            record_render(3, 0);
            let frame = frame_end(0.5).expect("a scene was drawn");
            // What was made before the first scene drawn is the cost of it.
            assert_eq!(frame.buffers_created, 2);
            assert_eq!(frame.draw_calls, 3);
            assert_eq!(frame.views, 1);
            assert!((frame.submit_seconds - 0.5).abs() < f64::EPSILON);
            assert_eq!(frame_end(0.0), None, "the tally starts again");
        })
        .join()
        .expect("the counting thread");
    }
}
