//! A window drawn on the processor (ARCHITECTURE §7.1): its pixels are
//! memory shared with the window system (a `wl_shm` buffer on Wayland, an
//! MIT-SHM image on X11, through softbuffer), drawn by R, the reference
//! renderer, with the machine's SIMD level and threads. No graphics driver
//! is started: no EGL display, no LLVM of a software GL.
//!
//! Only what changed since a buffer was last drawn is drawn again, and
//! only what changed since the last frame is handed to the window system
//! (`wl_surface.damage_buffer`). Wayland's two buffers take turns; a
//! buffer's age says which earlier frames' changes it still lacks.
//!
//! What R cannot draw is left out and said once: a `ShaderEffect`'s own
//! program (Qt's software scene graph leaves it out too) and what egui
//! draws besides the scene, such as the inspector.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_render::{
    DrawOp, FrameSnapshot, FrameView, PixelRect, Rgba, VelloCpuReference, frame_damage,
    merge_damage,
};

/// How many past frames' changes are kept for a buffer that old. Wayland's
/// back buffer is two frames old; X11's image one.
const HISTORY: usize = 3;

/// Whole frames a resize owes: each buffer is drawn whole once, as the
/// window system may hand it back resized with any pixels in it.
const BUFFERS: u8 = 2;

static SAID_SHADER: AtomicBool = AtomicBool::new(false);
static SAID_EGUI: AtomicBool = AtomicBool::new(false);

/// What the log says when a `ShaderEffect`'s own program is left out.
const SHADER_LEFT_OUT: &str = "drawn on the processor: a ShaderEffect's program is left out";
/// What the log says when egui draws something besides the scene.
const EGUI_LEFT_OUT: &str =
    "drawn on the processor: what egui draws, such as the inspector, is left out";

type Surface = softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>;

pub(crate) struct CpuPainter {
    display: OwnedDisplayHandle,
    surface: Option<Surface>,
    size: Option<(NonZeroU32, NonZeroU32)>,
    renderer: VelloCpuReference,
    /// The frame last shown, the scale and the clear colour it was drawn
    /// with; `None` when the next is drawn whole.
    last: Option<(Arc<FrameSnapshot>, f32, Rgba)>,
    /// What the last frames changed, newest first.
    history: VecDeque<Vec<PixelRect>>,
    /// Frames still to be drawn whole.
    whole: u8,
    /// What went wrong since the last frame, for the log.
    trouble: Option<String>,
}

impl std::fmt::Debug for CpuPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuPainter")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Workers beside the window's thread: up to three, and none on a single
/// core, where a worker only takes turns with the thread that waits for
/// it. Three halve a busy frame's time; more cost processor time and
/// draw no sooner (docs/BENCHMARKS.md, SHM1).
fn threads() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    u16::try_from(cores.saturating_sub(1).min(3)).unwrap_or(0)
}

impl CpuPainter {
    pub(crate) fn new(display: OwnedDisplayHandle) -> Self {
        Self {
            display,
            surface: None,
            size: None,
            renderer: VelloCpuReference::default().for_screen(threads()),
            last: None,
            history: VecDeque::with_capacity(HISTORY),
            whole: BUFFERS,
            trouble: None,
        }
    }

    pub(crate) fn set_window(&mut self, window: &Arc<Window>) -> Result<(), String> {
        let context = softbuffer::Context::new(self.display.clone()).map_err(|e| e.to_string())?;
        let mut surface =
            softbuffer::Surface::new(&context, Arc::clone(window)).map_err(|e| e.to_string())?;
        if let Some((w, h)) = self.size {
            surface.resize(w, h).map_err(|e| e.to_string())?;
        }
        self.surface = Some(surface);
        self.forget();
        Ok(())
    }

    pub(crate) fn forget_window(&mut self) {
        self.surface = None;
        self.forget();
    }

    pub(crate) fn resized(&mut self, w: NonZeroU32, h: NonZeroU32) {
        self.size = Some((w, h));
        if let Some(surface) = self.surface.as_mut()
            && let Err(e) = surface.resize(w, h)
        {
            self.trouble = Some(format!("the window cannot be resized: {e}"));
        }
        self.forget();
    }

    /// What the window showed may be gone: the next frame is drawn whole.
    pub(crate) fn forget(&mut self) {
        self.last = None;
        self.history.clear();
        self.whole = BUFFERS;
    }

    #[cfg(feature = "mcp")]
    pub(crate) fn describe(&self) -> ljos_dev::mcp::PainterInfo {
        ljos_dev::mcp::PainterInfo {
            painter: "cpu".into(),
            backend: Some(format!("vello_cpu, {} threads", threads() + 1)),
            adapter: None,
            present_mode: Some("shared memory".into()),
            partial_redraw: Some(true),
        }
    }

    /// Draws what changed of `snapshot` since the last frame and shows it.
    /// False when nothing changed, and nothing was shown. `warn` is told,
    /// once in the process, what is not drawn, and what went wrong;
    /// `egui_draws` is whether egui had anything of its own to draw.
    pub(crate) fn draw(
        &mut self,
        snapshot: &Arc<FrameSnapshot>,
        scale: f32,
        clear: Rgba,
        egui_draws: bool,
        window: &Window,
        warn: &mut dyn FnMut(String),
    ) -> bool {
        if let Some(trouble) = self.trouble.take() {
            warn(format!("drawn on the processor: {trouble}"));
        }
        let (Some(surface), Some((w, h))) = (self.surface.as_mut(), self.size) else {
            return false;
        };
        let (width, height) = (w.get(), h.get());
        let whole = PixelRect::whole(width, height);
        let changed = match &self.last {
            Some((last, s, c)) if *s == scale && *c == clear => {
                frame_damage(last, snapshot, scale, width, height)
            }
            _ => vec![whole],
        };
        if changed.is_empty() {
            return false;
        }
        if !SAID_SHADER.load(Ordering::Relaxed) && has_shader(snapshot) {
            SAID_SHADER.store(true, Ordering::Relaxed);
            warn(SHADER_LEFT_OUT.to_owned());
        }
        if egui_draws && !SAID_EGUI.swap(true, Ordering::Relaxed) {
            warn(EGUI_LEFT_OUT.to_owned());
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(e) => {
                warn(format!(
                    "drawn on the processor: no buffer to draw into: {e}"
                ));
                return false;
            }
        };
        // The buffer lacks what changed since it was last drawn.
        let age = usize::from(buffer.age());
        let parts = if self.whole > 0 || age == 0 || age > self.history.len() + 1 {
            vec![whole]
        } else {
            let mut parts = changed.clone();
            for past in self.history.iter().take(age - 1) {
                parts.extend_from_slice(past);
            }
            merge_damage(parts, width, height)
        };
        self.renderer.set_background(clear);
        let view = FrameView::new(snapshot, scale);
        for part in &parts {
            match self.renderer.render_area(&view, *part) {
                Ok(pixels) => copy_part(&mut buffer, width, *part, pixels),
                Err(e) => warn(format!("drawn on the processor: {e}")),
            }
        }
        window.pre_present_notify();
        let damage: Vec<softbuffer::Rect> = changed.iter().filter_map(to_rect).collect();
        if let Err(e) = buffer.present_with_damage(&damage) {
            warn(format!(
                "drawn on the processor: the window cannot be shown: {e}"
            ));
            self.forget();
            return false;
        }
        self.whole = self.whole.saturating_sub(1);
        self.history.push_front(changed);
        self.history.truncate(HISTORY);
        self.last = Some((Arc::clone(snapshot), scale, clear));
        true
    }
}

/// Whether a `ShaderEffect` with a program of its own is in the frame.
fn has_shader(snapshot: &FrameSnapshot) -> bool {
    snapshot
        .items
        .iter()
        .filter_map(|i| i.list.as_ref())
        .any(|l| l.ops().iter().any(|op| matches!(op, DrawOp::Shader { .. })))
}

fn to_rect(r: &PixelRect) -> Option<softbuffer::Rect> {
    Some(softbuffer::Rect {
        x: r.x,
        y: r.y,
        width: NonZeroU32::new(r.width)?,
        height: NonZeroU32::new(r.height)?,
    })
}

/// Puts R's premultiplied RGBA of `part` into the window's `0RGB` words.
/// Over the window's opaque black the premultiplied channels are the
/// colour shown.
fn copy_part(buffer: &mut [u32], width: u32, part: PixelRect, pixels: &[u8]) {
    let row = part.width as usize;
    for (y, src) in pixels.chunks_exact(row * 4).enumerate() {
        let at = (part.y as usize + y) * width as usize + part.x as usize;
        let Some(dst) = buffer.get_mut(at..at + row) else {
            return;
        };
        // Read as little-endian words, RGBA is 0xAABBGGRR: swapping the
        // outer bytes is a few shifts the compiler does many at a time.
        for (d, s) in dst.iter_mut().zip(src.as_chunks::<4>().0) {
            let v = u32::from_le_bytes(*s);
            *d = (v & 0x0000_ff00) | (v >> 16 & 0xff) | (v & 0xff) << 16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_lands_where_it_is_in_the_window() {
        let mut buffer = vec![0u32; 4 * 3];
        let pixels = [255, 0, 0, 255, 0, 128, 0, 255];
        let part = PixelRect {
            x: 2,
            y: 1,
            width: 2,
            height: 1,
        };
        copy_part(&mut buffer, 4, part, &pixels);
        assert_eq!(buffer[4 + 2], 0x00ff_0000);
        assert_eq!(buffer[4 + 3], 0x0000_8000);
        assert_eq!(buffer.iter().filter(|p| **p != 0).count(), 2);
    }
}
