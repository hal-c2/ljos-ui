//! The GL painter: egui's GL renderer on an EGL context.
//!
//! wgpu reaches GL through a present that copies the whole window once
//! more (flipped) and waits on fences around it. On a software rasteriser
//! that copy is most of what a frame costs, and it is paid in every frame
//! an animation draws. EGL swaps the window's own buffer.
//!
//! One context serves every window of the process, made current with the
//! surface of the window that is drawn; the pictures of each window stay
//! in its painter, in that context.

use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use egui::epaint::{
    ClippedShape, CornerRadius, PaintCallback, PaintCallbackInfo, RectShape, Shape,
};
use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use egui_winit::winit::window::Window;
use glow::HasContext;
use glutin::api::egl::config::Config;
use glutin::api::egl::context::PossiblyCurrentContext;
use glutin::api::egl::display::Display as EglDisplay;
use glutin::api::egl::surface::Surface;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentGlContext};
use glutin::display::GlDisplay;
use glutin::surface::{GlSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};

/// The display, the context and GL's functions all windows share.
pub(crate) struct Gl {
    // Field order is drop order: the context goes before its display.
    glow: Arc<glow::Context>,
    context: PossiblyCurrentContext,
    config: Config,
    display: EglDisplay,
    /// The surface the context is current with (0: none), so a window
    /// that draws again and again does not make it current every frame.
    current: Cell<u64>,
    /// Hands out the numbers `current` tells surfaces apart by.
    surfaces: Cell<u64>,
}

/// EGL's display with its driver started: the slow part of making a
/// context, which the shell does on a thread of its own while the
/// application is built.
pub(crate) struct Display(EglDisplay);

// SAFETY: EGL may be called from any thread and its display is a handle
// that holds in all of them; this one is moved to the thread that makes
// the context, not shared.
unsafe impl Send for Display {}

impl Display {
    /// Starts EGL's driver on the window system's `display`.
    pub(crate) fn start(display: &OwnedDisplayHandle) -> Result<Display, String> {
        let raw_display = display
            .display_handle()
            .map_err(|e| e.to_string())?
            .as_raw();
        // SAFETY: the display handle is the event loop's, which outlives
        // every window and so this context.
        let egl = unsafe { EglDisplay::new(raw_display) }.map_err(|e| e.to_string())?;
        Ok(Display(egl))
    }
}

impl Gl {
    /// A context for windows like `window` on `display`.
    pub(crate) fn new(display: &OwnedDisplayHandle, window: &Window) -> Result<Rc<Gl>, String> {
        Gl::with_display(Display::start(display)?, window)
    }

    /// A context for windows like `window` on a display already started.
    pub(crate) fn with_display(display: Display, window: &Window) -> Result<Rc<Gl>, String> {
        let Display(egl) = display;
        let raw_window = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        // No depth, stencil or multisampling: B0 sorts its own shapes and
        // feathers its own edges.
        let template = |window| {
            let template = ConfigTemplateBuilder::new()
                .with_alpha_size(8)
                .with_transparency(true);
            match window {
                Some(window) => template.compatible_with_native_window(window),
                None => template,
            }
            .build()
        };
        // A window that can be seen through needs a config that can; one
        // that cannot is still better than none.
        let pick = |a: Config, b: Config| {
            if a.supports_transparency() != Some(true) && b.supports_transparency() == Some(true) {
                b
            } else {
                a
            }
        };
        // SAFETY: the template's window is alive for the call.
        let config = unsafe { egl.find_configs(template(Some(raw_window))) }
            .map_err(|e| e.to_string())?
            .reduce(pick)
            // Mesa's EGL on X11 has no config for a window with an alpha
            // visual, and draws into one with any config all the same.
            .or_else(|| {
                // SAFETY: as above.
                unsafe { egl.find_configs(template(None)) }
                    .ok()?
                    .reduce(pick)
            })
            .ok_or("EGL has no config for the window")?;
        let gl = ContextAttributesBuilder::new().build(Some(raw_window));
        let gles = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window));
        // SAFETY: as above.
        let context = unsafe {
            egl.create_context(&config, &gl)
                .or_else(|_| egl.create_context(&config, &gles))
        }
        .map_err(|e| e.to_string())?
        .make_current_surfaceless()
        .map_err(|e| e.to_string())?;
        // SAFETY: the context is current and its functions are the
        // display's.
        let glow = unsafe { glow::Context::from_loader_function_cstr(|s| egl.get_proc_address(s)) };
        Ok(Rc::new(Gl {
            glow: Arc::new(glow),
            context,
            config,
            display: egl,
            current: Cell::new(0),
            surfaces: Cell::new(0),
        }))
    }

    /// Makes the context current with surface `id` of `surface`, or with
    /// none.
    fn make_current(&self, surface: Option<(u64, &Surface<WindowSurface>)>) -> bool {
        let id = surface.map_or(0, |(id, _)| id);
        if self.current.get() == id {
            return true;
        }
        let made = match surface {
            Some((_, surface)) => self.context.make_current(surface),
            None => self.context.make_current_surfaceless(),
        };
        self.current.set(if made.is_ok() { id } else { u64::MAX });
        made.is_ok()
    }
}

/// The painter of one window's pictures, and the surface of the window
/// while it is open.
pub(crate) struct GlPainter {
    gl: Rc<Gl>,
    painter: egui_glow::Painter,
    window: Option<GlWindow>,
}

struct GlWindow {
    id: u64,
    surface: Surface<WindowSurface>,
    px: [u32; 2],
    // The surface draws into the window: it stays while the surface does.
    _window: Arc<Window>,
}

impl GlPainter {
    pub(crate) fn new(gl: Rc<Gl>) -> Result<Self, String> {
        if !gl.make_current(None) {
            return Err("the GL context cannot be made current".into());
        }
        let painter = egui_glow::Painter::new(Arc::clone(&gl.glow), "", None, true)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            gl,
            painter,
            window: None,
        })
    }

    pub(crate) fn gl(&self) -> Rc<Gl> {
        Rc::clone(&self.gl)
    }

    pub(crate) fn max_texture_side(&self) -> usize {
        self.painter.max_texture_side()
    }

    /// Draws into `window` from now on.
    pub(crate) fn set_window(&mut self, window: &Arc<Window>) -> Result<(), String> {
        self.forget_window();
        let raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let size = window.inner_size();
        let px = [size.width.max(1), size.height.max(1)];
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw,
            NonZeroU32::new(px[0]).unwrap_or(NonZeroU32::MIN),
            NonZeroU32::new(px[1]).unwrap_or(NonZeroU32::MIN),
        );
        let gl = &self.gl;
        // SAFETY: the window outlives the surface (`GlWindow` keeps it).
        let surface = unsafe { gl.display.create_window_surface(&gl.config, &attrs) }
            .map_err(|e| e.to_string())?;
        gl.surfaces.set(gl.surfaces.get() + 1);
        let id = gl.surfaces.get();
        if !gl.make_current(Some((id, &surface))) {
            return Err("the GL context cannot draw into the window".into());
        }
        // ARCHITECTURE.md §7.2 step 12: a frame waits for the display
        // and none is torn or dropped. Not every driver lets it be set.
        let _ = surface.set_swap_interval(&gl.context, SwapInterval::Wait(NonZeroU32::MIN));
        self.window = Some(GlWindow {
            id,
            surface,
            px,
            _window: Arc::clone(window),
        });
        Ok(())
    }

    /// The window is going: its surface goes first.
    pub(crate) fn forget_window(&mut self) {
        if let Some(window) = self.window.take()
            && self.gl.current.get() == window.id
        {
            self.gl.make_current(None);
        }
    }

    pub(crate) fn resized(&mut self, w: NonZeroU32, h: NonZeroU32) {
        if let Some(window) = self.window.as_mut() {
            window.surface.resize(&self.gl.context, w, h);
            window.px = [w.get(), h.get()];
        }
    }

    /// Draws a frame into the window and shows it.
    pub(crate) fn paint(
        &mut self,
        pixels_per_point: f32,
        clear: [f32; 4],
        primitives: &[egui::ClippedPrimitive],
        pending: &mut egui::TexturesDelta,
        window: &Window,
    ) {
        let Some(gw) = self.window.as_ref() else {
            return;
        };
        if !self.gl.make_current(Some((gw.id, &gw.surface))) {
            return;
        }
        self.painter.clear(gw.px, clear);
        self.painter
            .paint_and_update_textures(gw.px, pixels_per_point, primitives, pending);
        window.pre_present_notify();
        // A lost frame is drawn again with the next change; there is
        // nobody to tell.
        let _ = gw.surface.swap_buffers(&self.gl.context);
    }

    /// Gives back what `pending` frees, while the window is not open.
    pub(crate) fn free(&mut self, pending: &mut egui::TexturesDelta) {
        if pending.free.is_empty() {
            return;
        }
        let surface = self.window.as_ref().map(|w| (w.id, &w.surface));
        if !self.gl.make_current(surface) {
            return;
        }
        for id in std::mem::take(&mut pending.free) {
            self.painter.free_texture(id);
        }
    }
}

impl Drop for GlPainter {
    fn drop(&mut self) {
        let surface = self.window.as_ref().map(|w| (w.id, &w.surface));
        if self.gl.make_current(surface) {
            self.painter.destroy();
        }
        self.forget_window();
    }
}

/// Turns each opaque rectangle with square corners and no border that
/// lies on whole pixels into a clear of its pixels. A mesh is blended
/// into the frame and samples a texture at every pixel it covers; on a
/// software rasteriser one the size of the window costs an eighth of a
/// core at 60 frames a second, and its clear next to nothing. Done to the
/// shapes about to be drawn, after they were compared with the last
/// picture (a callback is never the same as another).
pub(crate) fn fill_rects(shapes: &mut [ClippedShape], pixels_per_point: f32) {
    for shape in shapes {
        let Shape::Rect(rect) = &shape.shape else {
            continue;
        };
        let Some(color) = clearable(rect, pixels_per_point) else {
            continue;
        };
        let color = color.to_normalized_gamma_f32();
        let clear = egui_glow::CallbackFn::new(move |info, painter| {
            clear_in(&info, painter.gl(), color);
        });
        shape.shape = Shape::Callback(PaintCallback {
            rect: rect.rect,
            callback: Arc::new(clear),
        });
    }
}

/// The colour of `rect` when a clear draws the same pixels it does.
fn clearable(rect: &RectShape, pixels_per_point: f32) -> Option<egui::Color32> {
    let whole = |v: f32| {
        let px = v * pixels_per_point;
        (px - px.round()).abs() < 1e-3
    };
    let r = rect.rect;
    let plain = rect.corner_radius == CornerRadius::ZERO
        && rect.stroke.is_empty()
        && rect.blur_width <= 0.0
        && rect.brush.is_none()
        && rect.fill.is_opaque();
    let on_pixels =
        r.is_positive() && whole(r.min.x) && whole(r.min.y) && whole(r.max.x) && whole(r.max.y);
    (plain && on_pixels).then_some(rect.fill)
}

fn clear_in(info: &PaintCallbackInfo, gl: &glow::Context, color: [f32; 4]) {
    let (v, c) = (info.viewport_in_pixels(), info.clip_rect_in_pixels());
    let left = v.left_px.max(c.left_px);
    let right = (v.left_px + v.width_px).min(c.left_px + c.width_px);
    let bottom = v.from_bottom_px.max(c.from_bottom_px);
    let top = (v.from_bottom_px + v.height_px).min(c.from_bottom_px + c.height_px);
    if right <= left || top <= bottom {
        return;
    }
    // SAFETY: called by the painter while its context is current; it
    // sets the scissor again for the shapes after.
    unsafe {
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(left, bottom, right - left, top - bottom);
        gl.clear_color(color[0], color[1], color[2], color[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, Rect, Stroke, pos2};

    use super::*;

    fn rect(min: [f32; 2], max: [f32; 2], fill: Color32) -> ClippedShape {
        ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_max(pos2(min[0], min[1]), pos2(max[0], max[1])),
                CornerRadius::ZERO,
                fill,
            )),
        }
    }

    fn cleared(shapes: &[ClippedShape]) -> Vec<bool> {
        shapes
            .iter()
            .map(|s| matches!(s.shape, Shape::Callback(_)))
            .collect()
    }

    #[test]
    fn opaque_rectangles_on_whole_pixels_are_cleared() {
        let mut shapes = vec![
            rect([0.0, 0.0], [400.0, 40.0], Color32::WHITE),
            rect([0.0, 40.0], [400.0, 80.0], Color32::from_gray(0xee)),
        ];
        fill_rects(&mut shapes, 1.0);
        assert_eq!(cleared(&shapes), [true, true]);
    }

    #[test]
    fn what_a_clear_would_draw_otherwise_stays_a_rectangle() {
        let mut rounded = rect([0.0, 0.0], [10.0, 10.0], Color32::RED);
        let mut bordered = rect([0.0, 0.0], [10.0, 10.0], Color32::RED);
        if let Shape::Rect(r) = &mut rounded.shape {
            r.corner_radius = CornerRadius::same(2);
        }
        if let Shape::Rect(r) = &mut bordered.shape {
            r.stroke = Stroke::new(1.0, Color32::BLACK);
        }
        let mut shapes = vec![
            // Between pixels: its edges are blended.
            rect([0.5, 0.0], [10.0, 10.0], Color32::RED),
            // Seen through.
            rect([0.0, 0.0], [10.0, 10.0], Color32::from_black_alpha(128)),
            rounded,
            bordered,
            // No area.
            rect([5.0, 0.0], [5.0, 10.0], Color32::RED),
        ];
        fill_rects(&mut shapes, 1.0);
        assert_eq!(cleared(&shapes), [false; 5]);
    }

    #[test]
    fn whole_pixels_are_the_device_s() {
        // 0.5 points at a scale of 2 is a whole pixel.
        let mut shapes = vec![rect([0.5, 0.5], [10.0, 10.0], Color32::RED)];
        fill_rects(&mut shapes, 2.0);
        assert_eq!(cleared(&shapes), [true]);
        let mut shapes = vec![rect([0.5, 0.5], [10.0, 10.0], Color32::RED)];
        fill_rects(&mut shapes, 1.0);
        assert_eq!(cleared(&shapes), [false]);
    }
}
