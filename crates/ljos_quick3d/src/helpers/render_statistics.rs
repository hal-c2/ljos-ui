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
    /// Draw calls of the last frame, of every pass: shadows, depth, the
    /// scene and its post-processing.
    #[qml(notify, readonly)]
    draw_call_count: i32,
    /// Vertices those drew, each copy of an instanced model counted.
    #[qml(notify, readonly)]
    draw_vertex_count: f64,
    /// Render passes of the last frame.
    #[qml(notify, readonly)]
    render_pass_count: i32,
    /// Pipelines set on the last frame's passes.
    #[qml(notify, readonly)]
    pipeline_switches: i32,
    /// Bind groups set on the last frame's passes.
    #[qml(notify, readonly)]
    bind_group_sets: i32,
    /// Writes into buffers in the last frame.
    #[qml(notify, readonly)]
    buffer_writes: i32,
    /// The bytes of those.
    #[qml(notify, readonly)]
    buffer_write_bytes: f64,
    /// Writes into textures in the last frame.
    #[qml(notify, readonly)]
    texture_uploads: i32,
    /// The bytes of those.
    #[qml(notify, readonly)]
    texture_upload_bytes: f64,
    /// Render pipelines made for the last frame.
    #[qml(notify, readonly)]
    pipelines_created: i32,
    /// Models the camera saw in the last frame.
    #[qml(notify, readonly)]
    models_drawn: i32,
    /// Models left out of it because the camera did not see them.
    #[qml(notify, readonly)]
    models_culled: i32,
    /// Milliseconds spent reading the scene into the renderer's for the
    /// last frame; -1 before a frame is drawn.
    #[qml(notify, readonly)]
    sync_time: f64,
    /// Milliseconds the renderer spent preparing the last frame: walking
    /// the scene, culling, sorting, writing uniforms, making pipelines.
    #[qml(notify, readonly)]
    render_prepare_time: f64,
    /// Milliseconds it spent recording the last frame's passes.
    #[qml(notify, readonly)]
    encode_time: f64,
    /// Milliseconds the host's submit of them took; 0 where the host
    /// submits them with its own drawing and does not time them apart.
    #[qml(notify, readonly)]
    submit_time: f64,
    /// Milliseconds of rendering the last frame after preparing it:
    /// recording and submitting it.
    #[qml(notify, readonly)]
    render_time: f64,
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
    let f = &s.last;
    let ms = |seconds: f64| {
        if s.frames == 0 {
            -1.0
        } else {
            seconds * 1000.0
        }
    };
    let _ = cx.set(RenderStatistics::DRAW_CALL_COUNT, count(f.draw_calls));
    let _ = cx.set(RenderStatistics::DRAW_VERTEX_COUNT, f.vertices as f64);
    let _ = cx.set(RenderStatistics::RENDER_PASS_COUNT, count(f.render_passes));
    let _ = cx.set(
        RenderStatistics::PIPELINE_SWITCHES,
        count(f.pipeline_switches),
    );
    let _ = cx.set(RenderStatistics::BIND_GROUP_SETS, count(f.bind_group_sets));
    let _ = cx.set(RenderStatistics::BUFFER_WRITES, count(f.buffer_writes));
    let _ = cx.set(
        RenderStatistics::BUFFER_WRITE_BYTES,
        f.buffer_write_bytes as f64,
    );
    let _ = cx.set(RenderStatistics::TEXTURE_UPLOADS, count(f.texture_uploads));
    let _ = cx.set(
        RenderStatistics::TEXTURE_UPLOAD_BYTES,
        f.texture_upload_bytes as f64,
    );
    let _ = cx.set(
        RenderStatistics::PIPELINES_CREATED,
        count(f.pipelines_created),
    );
    let _ = cx.set(RenderStatistics::MODELS_DRAWN, count(f.models_drawn));
    let _ = cx.set(RenderStatistics::MODELS_CULLED, count(f.models_culled));
    let _ = cx.set(RenderStatistics::SYNC_TIME, ms(f.sync_seconds));
    let _ = cx.set(RenderStatistics::RENDER_PREPARE_TIME, ms(f.prepare_seconds));
    let _ = cx.set(RenderStatistics::ENCODE_TIME, ms(f.encode_seconds));
    let _ = cx.set(RenderStatistics::SUBMIT_TIME, ms(f.submit_seconds));
    let _ = cx.set(
        RenderStatistics::RENDER_TIME,
        ms(f.encode_seconds + f.submit_seconds),
    );
}

/// A count as QML's int, which a frame's never outgrows.
fn count(n: u64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// How often `RenderStats` reads the numbers; `fps` counts this long.
const STATS_INTERVAL_MS: i32 = 1000;

/// `View3D.renderStats`. The view makes one when something first reads
/// the property, so only a view whose numbers are shown has this timer.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "RenderStats", extends = QtObject, uncreatable)]
pub struct RenderStats {
    #[qml(base)]
    object: QtObject,
    /// Scenes drawn in the last second.
    #[qml(notify, readonly)]
    fps: i32,
    /// Milliseconds from the start of one frame to the next, 0 when none
    /// was drawn in the last second.
    #[qml(notify, readonly)]
    frame_time: f64,
    /// The longest of those in the last second.
    #[qml(notify, readonly)]
    max_frame_time: f64,
    /// Milliseconds the CPU spent rendering the last frame after preparing
    /// it: recording its passes and submitting them, as Qt has it; -1
    /// before a frame is drawn.
    #[qml(notify, readonly)]
    render_time: f64,
    /// Milliseconds spent reading the QML scene into the renderer's for
    /// the last frame; -1 before a frame is drawn.
    #[qml(notify, readonly)]
    sync_time: f64,
    /// Milliseconds the renderer spent preparing the last frame: walking
    /// the scene, culling, sorting, writing uniforms, making pipelines.
    #[qml(notify, readonly)]
    render_prepare_time: f64,
    /// Milliseconds the GPU took for a recent frame (its timestamps come
    /// back frames later); -1 when the host does not measure it.
    #[qml(notify, readonly)]
    last_completed_gpu_time: f64,
    /// Draw calls of the last frame, of every pass.
    #[qml(notify, readonly)]
    draw_call_count: i32,
    /// Vertices those drew, each copy of an instanced model counted.
    #[qml(notify, readonly)]
    draw_vertex_count: i32,
    /// Render passes of the last frame.
    #[qml(notify, readonly)]
    render_pass_count: i32,
    /// Render pipelines made so far.
    #[qml(notify, readonly)]
    pipeline_count: i32,
    #[qml(skip)]
    seen: u64,
}

#[qml_impl]
impl RenderStats {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let e = cx.engine;
        e.set_internal(me, Self::SYNC_TIME, -1.0)?;
        e.set_internal(me, Self::RENDER_TIME, -1.0)?;
        e.set_internal(me, Self::RENDER_PREPARE_TIME, -1.0)?;
        e.set_internal(me, Self::LAST_COMPLETED_GPU_TIME, -1.0)?;
        let timer = e.create_native_incomplete(Timer::default(), Some(me))?;
        e.set(timer, Timer::INTERVAL, STATS_INTERVAL_MS)?;
        e.set(timer, Timer::REPEAT, true)?;
        e.set(timer, Timer::RUNNING, true)?;
        e.complete(timer)?;
        e.connect_by_name(timer, "triggered", move |e, _| read_stats(e, me))?;
        read_stats(e, me);
        Ok(())
    }
}

fn read_stats(e: &Engine, me: ObjectId) {
    let s = stats::snapshot();
    let longest = stats::take_longest_frame();
    let seen = e
        .with_mut(me, |t: &mut RenderStats| {
            std::mem::replace(&mut t.seen, s.renders)
        })
        .unwrap_or(s.renders);
    let drawn = s.renders - seen;
    let fps = i32::try_from(drawn * 1000 / STATS_INTERVAL_MS as u64).unwrap_or(i32::MAX);
    let _ = e.set_internal(me, RenderStats::FPS, fps);
    let frame = if drawn == 0 {
        0.0
    } else {
        s.frame_seconds * 1000.0
    };
    let _ = e.set_internal(me, RenderStats::FRAME_TIME, frame);
    let longest = if drawn == 0 { 0.0 } else { longest * 1000.0 };
    let _ = e.set_internal(me, RenderStats::MAX_FRAME_TIME, longest);
    let _ = e.set_internal(
        me,
        RenderStats::LAST_COMPLETED_GPU_TIME,
        s.gpu_seconds.map_or(-1.0, |g| g * 1000.0),
    );
    if s.frames == 0 {
        return;
    }
    let f = &s.last;
    let _ = e.set_internal(
        me,
        RenderStats::RENDER_TIME,
        (f.encode_seconds + f.submit_seconds) * 1000.0,
    );
    let _ = e.set_internal(me, RenderStats::SYNC_TIME, f.sync_seconds * 1000.0);
    let _ = e.set_internal(
        me,
        RenderStats::RENDER_PREPARE_TIME,
        f.prepare_seconds * 1000.0,
    );
    let _ = e.set_internal(me, RenderStats::DRAW_CALL_COUNT, count(f.draw_calls));
    let _ = e.set_internal(me, RenderStats::DRAW_VERTEX_COUNT, count(f.vertices));
    let _ = e.set_internal(me, RenderStats::RENDER_PASS_COUNT, count(f.render_passes));
    let _ = e.set_internal(
        me,
        RenderStats::PIPELINE_COUNT,
        count(s.total.pipelines_created),
    );
}

/// Makes `view`'s `renderStats` when it is first read.
pub(crate) fn bind_render_stats(e: &Engine, view: ObjectId) -> Result<(), EngineError> {
    e.bind_on_read(view, crate::View3D::RENDER_STATS.index, |e, view, prop| {
        let stats = e.create_native(RenderStats::default(), Some(view))?;
        e.bind_initial(view, prop, move |_| Value::from(stats))
    })
}
