//! `RenderStatistics`: what the renderer did lately, as properties, for
//! `DebugView` to show.
//!
//! The numbers are read when `refresh()` is called, not pushed: a scene
//! that is not drawn wakes nothing, so a readout of an idle scene costs
//! nothing between its refreshes.

use ljos::item::ljos_3d::stats;
use ljos::prelude::*;

/// A frame longer than this is a pause, not a frame rate.
const LONGEST_FRAME: f64 = 1.0;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "RenderStatistics", extends = QtObject)]
pub struct RenderStatistics {
    #[qml(base)]
    object: QtObject,
    /// Frames per second while the scene is being drawn, 0 when it is not.
    #[qml(notify, readonly)]
    fps: f64,
    /// Milliseconds from the start of one frame to the next.
    #[qml(notify, readonly)]
    frame_time: f64,
    /// Draw calls of the last frame.
    #[qml(notify, readonly)]
    draw_calls: i32,
    /// Shadow-map passes drawn so far.
    #[qml(notify, readonly)]
    shadow_passes: i32,
    /// Scenes drawn so far.
    #[qml(notify, readonly)]
    frames: i32,
    /// Milliseconds the GPU took for the last frame, post-processing
    /// included; -1 when the host does not measure it.
    #[qml(notify, readonly)]
    gpu_time: f64,
    /// Of those, the milliseconds of the post-processing passes (effects,
    /// ambient occlusion, depth of field, glow, the final tonemap and
    /// FXAA); -1 when the scene is not post-processed or not measured.
    #[qml(notify, readonly)]
    gpu_post_time: f64,
    #[qml(skip)]
    seen: u64,
}

#[qml_impl]
impl RenderStatistics {
    #[qml(on_complete)]
    fn completed(cx: &Cx) {
        refresh(cx);
    }

    /// Reads the numbers again.
    #[qml(invokable)]
    fn refresh(cx: &Cx) {
        refresh(cx);
    }
}

fn refresh(cx: &Cx) {
    let s = stats::snapshot();
    let drawing = cx
        .with_mut(|t: &mut RenderStatistics| std::mem::replace(&mut t.seen, s.renders) != s.renders)
        .unwrap_or(false);
    let running = drawing && s.frame_seconds > 0.0 && s.frame_seconds < LONGEST_FRAME;
    let _ = cx.set(
        RenderStatistics::FPS,
        if running { 1.0 / s.frame_seconds } else { 0.0 },
    );
    let _ = cx.set(
        RenderStatistics::FRAME_TIME,
        if running {
            s.frame_seconds * 1000.0
        } else {
            0.0
        },
    );
    let _ = cx.set(RenderStatistics::DRAW_CALLS, s.draw_calls as i32);
    let _ = cx.set(RenderStatistics::SHADOW_PASSES, s.shadow_passes as i32);
    let _ = cx.set(RenderStatistics::FRAMES, s.renders as i32);
    let _ = cx.set(
        RenderStatistics::GPU_TIME,
        s.gpu_seconds.map_or(-1.0, |g| g * 1000.0),
    );
    let _ = cx.set(
        RenderStatistics::GPU_POST_TIME,
        s.gpu_post_seconds.map_or(-1.0, |g| g * 1000.0),
    );
}
