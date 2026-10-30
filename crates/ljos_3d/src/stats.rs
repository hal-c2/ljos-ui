//! What the renderer did lately, for a readout (`DebugView`) to show.
//!
//! One record for the process: every [`Renderer`](crate::gpu::Renderer)
//! writes it when it draws a scene, and a reader takes a copy. A reader
//! never waits on the renderer for more than the copy.

use std::sync::Mutex;

/// What the last draw of a scene took, and how often scenes are drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RenderStats {
    /// Scenes drawn so far.
    pub renders: u64,
    /// Draw calls of the last scene, its sky included.
    pub draw_calls: u32,
    /// Shadow-map passes drawn so far, which only grows.
    pub shadow_passes: u64,
    /// Seconds from the start of the scene drawn before to this one's: how
    /// long a frame is while the scene is animated, and a gap when it is
    /// not. 0 before the second scene.
    pub frame_seconds: f64,
    /// Seconds the GPU took for the last scene, when the host measures it
    /// ([`set_gpu_seconds`]).
    pub gpu_seconds: Option<f64>,
}

#[derive(Default)]
struct Record {
    stats: RenderStats,
    #[cfg(not(target_arch = "wasm32"))]
    last: Option<std::time::Instant>,
}

static RECORD: Mutex<Record> = Mutex::new(Record {
    stats: RenderStats {
        renders: 0,
        draw_calls: 0,
        shadow_passes: 0,
        frame_seconds: 0.0,
        gpu_seconds: None,
    },
    #[cfg(not(target_arch = "wasm32"))]
    last: None,
});

fn record() -> std::sync::MutexGuard<'static, Record> {
    // A reader that panicked holds numbers, not an invariant.
    RECORD.lock().unwrap_or_else(|p| p.into_inner())
}

/// What the renderer did, as of now.
pub fn snapshot() -> RenderStats {
    record().stats
}

/// Called by the renderer when it has drawn a scene.
pub fn record_render(draw_calls: u32, shadow_passes: u64) {
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
        r.last = Some(now);
    }
}

/// The host says how long the GPU took for the last scene, from timestamp
/// queries: the renderer cannot tell.
pub fn set_gpu_seconds(seconds: Option<f64>) {
    record().stats.gpu_seconds = seconds;
}
