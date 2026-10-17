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
use std::collections::VecDeque;
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

use crate::present::{Change, Frame};

/// How many frames back the areas drawn are remembered: a buffer older
/// than that is drawn whole.
const REMEMBERED: usize = 4;

/// How many separate areas a frame draws at most. Each costs a scissored
/// clear and a draw of every shape that reaches it, which is cheap next to
/// the pixels it spares; past this, they are drawn as one area around them
/// all, so a frame never spends long working out where to draw.
const AREAS: usize = 256;

/// Areas of at least this many pixels are cleared to the background;
/// smaller ones are drawn together as one mesh.
const CLEARED: u32 = 64 * 64;

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
    programs: crate::gl_shader::Programs,
}

struct GlWindow {
    id: u64,
    surface: Surface<WindowSurface>,
    px: [u32; 2],
    /// The pixels each of the last frames drew, the last first (`None`:
    /// all of them), for a buffer that holds an older frame.
    drawn: VecDeque<Option<Damage>>,
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
            programs: crate::gl_shader::Programs::default(),
        })
    }

    pub(crate) fn gl(&self) -> Rc<Gl> {
        Rc::clone(&self.gl)
    }

    /// The `ShaderEffect` programs this painter links.
    pub(crate) fn programs(&self) -> &crate::gl_shader::Programs {
        &self.programs
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
            drawn: VecDeque::new(),
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
            window.drawn.clear();
        }
    }

    /// What of a picture that makes `change` is drawn. The buffer drawn
    /// into still holds the frame shown `age` frames ago (when EGL can
    /// say), so it is drawn again where this frame and the ones since
    /// changed it. A picture handed over again under the same name may be
    /// anywhere.
    pub(crate) fn frame(
        &mut self,
        change: Change,
        pending: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Frame {
        let replaced = pending.set.iter().any(|(id, deltas)| {
            self.painter.texture(*id).is_some() || deltas.iter().any(|d| d.pos.is_some())
        });
        let Some(gw) = self.window.as_mut() else {
            return Frame::Whole;
        };
        let now = match change {
            _ if replaced => None,
            Change::None => return Frame::Nothing,
            Change::Area(areas) => Some(Damage::of(
                areas
                    .into_iter()
                    .map(|area| Px::around(area, pixels_per_point, gw.px)),
            )),
            Change::All => None,
        };
        // Asked before the context is current with the surface: EGL
        // answers for the surface either way.
        let age = gw.surface.buffer_age() as usize;
        gw.drawn.push_front(now);
        gw.drawn.truncate(REMEMBERED);
        if age == 0 || age > gw.drawn.len() {
            return Frame::Whole;
        }
        let mut drawn = Vec::new();
        for d in gw.drawn.iter().take(age) {
            match d {
                Some(d) => drawn.extend_from_slice(&d.0),
                None => return Frame::Whole,
            }
        }
        let damage = Damage::of(drawn);
        if damage.0.is_empty() {
            Frame::Nothing
        } else {
            Frame::Part(
                damage
                    .0
                    .iter()
                    .map(|px| px.points(pixels_per_point))
                    .collect(),
            )
        }
    }

    /// Draws `primitives` (all of `frame`) into the window and shows it.
    pub(crate) fn paint(
        &mut self,
        pixels_per_point: f32,
        clear: egui::Color32,
        frame: &Frame,
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
        let part = match frame {
            Frame::Nothing => {
                // Only the pictures: nothing is shown.
                self.painter
                    .paint_and_update_textures(gw.px, pixels_per_point, &[], pending);
                return;
            }
            Frame::Part(areas) => Some(
                areas
                    .iter()
                    .map(|area| Px::around(*area, pixels_per_point, gw.px))
                    .collect::<Vec<_>>(),
            ),
            Frame::Whole => None,
        };
        let gamma = clear.to_normalized_gamma_f32();
        match &part {
            Some(areas) => {
                // A clear costs a software rasteriser next to nothing per
                // pixel but is a draw of its own; a quad costs per pixel.
                // Large areas are cleared, small ones are one mesh.
                let mut mesh = egui::Mesh::default();
                for px in areas {
                    if clear.is_opaque() && px.pixels() < CLEARED {
                        mesh.add_colored_rect(px.points(pixels_per_point), clear);
                    } else {
                        px.clear(self.painter.gl(), gw.px, gamma);
                    }
                }
                if !mesh.is_empty() {
                    let backdrop = egui::ClippedPrimitive {
                        clip_rect: egui::Rect::EVERYTHING,
                        primitive: egui::epaint::Primitive::Mesh(mesh),
                    };
                    let free = std::mem::take(&mut pending.free);
                    self.painter.paint_and_update_textures(
                        gw.px,
                        pixels_per_point,
                        &[backdrop],
                        pending,
                    );
                    pending.free = free;
                }
            }
            None => self.painter.clear(gw.px, gamma),
        }
        self.painter
            .paint_and_update_textures(gw.px, pixels_per_point, primitives, pending);
        window.pre_present_notify();
        // A lost frame is drawn again with the next change; there is
        // nobody to tell. The compositor is told what changed, so it can
        // take only that.
        let _ = match part {
            Some(areas) => {
                let rects: Vec<_> = areas.iter().map(|px| px.egl(gw.px)).collect();
                gw.surface
                    .swap_buffers_with_damage(&self.gl.context, &rects)
            }
            None => gw.surface.swap_buffers(&self.gl.context),
        };
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
            self.programs.destroy(&self.gl.glow);
            self.painter.destroy();
        }
        self.forget_window();
    }
}

/// A rectangle of the window's pixels, from the top left: `[x0, y0, x1,
/// y1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Px([u32; 4]);

impl Px {
    /// The pixels `area` (in points) touches, in a window of `size`.
    fn around(area: egui::Rect, pixels_per_point: f32, size: [u32; 2]) -> Px {
        let lo = |v: f32, max: u32| ((v * pixels_per_point).floor().max(0.0) as u32).min(max);
        let hi = |v: f32, max: u32| ((v * pixels_per_point).ceil().max(0.0) as u32).min(max);
        Px([
            lo(area.min.x, size[0]),
            lo(area.min.y, size[1]),
            hi(area.max.x, size[0]),
            hi(area.max.y, size[1]),
        ])
    }

    fn is_empty(self) -> bool {
        let [x0, y0, x1, y1] = self.0;
        x1 <= x0 || y1 <= y0
    }

    fn union(self, other: Px) -> Px {
        let ([a0, b0, a1, b1], [c0, d0, c1, d1]) = (self.0, other.0);
        Px([a0.min(c0), b0.min(d0), a1.max(c1), b1.max(d1)])
    }

    fn pixels(self) -> u32 {
        let [x0, y0, x1, y1] = self.0;
        (x1 - x0) * (y1 - y0)
    }

    fn overlaps(self, other: Px) -> bool {
        let ([a0, b0, a1, b1], [c0, d0, c1, d1]) = (self.0, other.0);
        a0 < c1 && c0 < a1 && b0 < d1 && d0 < b1
    }

    fn points(self, pixels_per_point: f32) -> egui::Rect {
        let [x0, y0, x1, y1] = self.0.map(|v| v as f32 / pixels_per_point);
        egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
    }

    /// As EGL counts: from the bottom left.
    fn egl(self, size: [u32; 2]) -> glutin::surface::Rect {
        let [x0, y0, x1, y1] = self.0;
        glutin::surface::Rect::new(
            x0 as i32,
            size[1].saturating_sub(y1) as i32,
            (x1 - x0) as i32,
            (y1 - y0) as i32,
        )
    }

    fn clear(self, gl: &glow::Context, size: [u32; 2], color: [f32; 4]) {
        let r = self.egl(size);
        // SAFETY: the context is current; the painter sets the scissor
        // again for every shape.
        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(r.x, r.y, r.width, r.height);
            gl.clear_color(color[0], color[1], color[2], color[3]);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }
}

/// Where a frame draws: rectangles of pixels, none overlapping another
/// (a shape drawn in two would be blended twice where they meet), at most
/// [`AREAS`] of them.
#[derive(Debug, Clone, Default, PartialEq)]
struct Damage(Vec<Px>);

impl Damage {
    /// The areas that cover `areas`: those that overlap become one around
    /// them, which may overlap more. Swept top to bottom, so an area is
    /// only compared with those that start above its bottom edge, not with
    /// every other: a frame of two hundred moving bars compares each with
    /// a few, not with two hundred.
    fn of(areas: impl IntoIterator<Item = Px>) -> Damage {
        let mut v: Vec<Px> = areas.into_iter().filter(|px| !px.is_empty()).collect();
        let mut root: Vec<usize> = Vec::new();
        loop {
            v.sort_unstable_by_key(|px| px.0[1]);
            root.clear();
            root.extend(0..v.len());
            let mut joined = false;
            for i in 0..v.len() {
                for j in i + 1..v.len() {
                    count_compared();
                    if v[j].0[1] >= v[i].0[3] {
                        break;
                    }
                    if v[i].overlaps(v[j]) {
                        let (a, b) = (find(&mut root, i), find(&mut root, j));
                        if a != b {
                            root[b] = a;
                        }
                        joined = true;
                    }
                }
            }
            if !joined {
                break;
            }
            // Each group becomes the area around it, in its first member's
            // place.
            for i in 0..v.len() {
                let r = find(&mut root, i);
                if r != i {
                    v[r] = v[r].union(v[i]);
                }
            }
            let mut i = 0;
            v.retain(|_| {
                i += 1;
                root[i - 1] == i - 1
            });
        }
        if v.len() > AREAS {
            let all = v.drain(..).reduce(Px::union);
            v.extend(all);
        }
        Damage(v)
    }
}

fn find(root: &mut [usize], mut i: usize) -> usize {
    while root[i] != i {
        root[i] = root[root[i]];
        i = root[i];
    }
    i
}

#[cfg(test)]
thread_local! {
    static COMPARED: Cell<usize> = const { Cell::new(0) };
}

fn count_compared() {
    #[cfg(test)]
    COMPARED.set(COMPARED.get() + 1);
}

/// Turns each opaque rectangle with square corners and no border that
/// lies on whole pixels into what draws exactly its pixels, with no
/// smoothed edge. A mesh is blended into the frame and samples a texture
/// at every pixel it covers; on a software rasteriser one the size of the
/// window costs an eighth of a core at 60 frames a second, and its clear
/// next to nothing. So one of at least [`CLEARED`] pixels becomes a clear
/// of them. A smaller one costs little either way, but a clear is a draw
/// of its own and ends the mesh around it (two hundred small bars were two
/// hundred draws), so it becomes a quad on the pixels' edges, drawn with
/// the shapes next to it, and quads one after another are one mesh. Done to the shapes about to be drawn, after they
/// were compared with the last picture (a callback is never the same as
/// another).
pub(crate) fn fill_rects(shapes: &mut [ClippedShape], pixels_per_point: f32) {
    // The mesh the quads just before went into, while nothing came between
    // them: one more joins it, so a run of bars is one mesh made, not one
    // each, and drawn in the same order.
    let mut quads: Option<usize> = None;
    for i in 0..shapes.len() {
        let fill = match &shapes[i].shape {
            Shape::Rect(rect) => clearable(rect, pixels_per_point).map(|color| (rect.rect, color)),
            _ => None,
        };
        let Some((r, color)) = fill else {
            quads = None;
            continue;
        };
        if r.area() * pixels_per_point * pixels_per_point < CLEARED as f32 {
            let clip = shapes[i].clip_rect;
            let run = quads.filter(|&j| shapes[j].clip_rect == clip);
            if let Some(Shape::Mesh(mesh)) = run.map(|j| &mut shapes[j].shape) {
                Arc::make_mut(mesh).add_colored_rect(r, color);
                shapes[i].shape = Shape::Noop;
            } else {
                let mut mesh = egui::Mesh::default();
                mesh.add_colored_rect(r, color);
                shapes[i].shape = Shape::mesh(mesh);
                quads = Some(i);
            }
            continue;
        }
        quads = None;
        let color = color.to_normalized_gamma_f32();
        let clear = egui_glow::CallbackFn::new(move |info, painter| {
            clear_in(&info, painter.gl(), color);
        });
        shapes[i].shape = Shape::Callback(PaintCallback {
            rect: r,
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

    fn disjoint(damage: &Damage) -> bool {
        let d = &damage.0;
        (0..d.len()).all(|i| (i + 1..d.len()).all(|j| !d[i].overlaps(d[j])))
    }

    #[test]
    fn areas_apart_stay_apart_and_overlapping_ones_join() {
        let damage = Damage::of([
            Px([0, 0, 10, 10]),
            Px([100, 0, 110, 10]),
            Px([10, 0, 20, 10]),
        ]);
        assert_eq!(damage.0.len(), 3, "areas that only touch are not joined");
        // Overlaps the first and, once joined, reaches the third.
        let damage = Damage::of(damage.0.into_iter().chain([Px([5, 5, 12, 12])]));
        assert!(disjoint(&damage));
        assert_eq!(damage.0.len(), 2);
        assert!(damage.0.contains(&Px([0, 0, 20, 12])));
        assert!(damage.0.contains(&Px([100, 0, 110, 10])));
    }

    #[test]
    fn an_area_made_by_joining_takes_in_what_it_then_overlaps() {
        // The first two join into [0, 0, 30, 30], which overlaps the third
        // though neither did.
        let damage = Damage::of([
            Px([0, 0, 10, 30]),
            Px([0, 20, 30, 30]),
            Px([20, 0, 30, 10]),
            Px([50, 50, 60, 60]),
        ]);
        assert!(disjoint(&damage));
        assert_eq!(damage.0.len(), 2);
        assert!(damage.0.contains(&Px([0, 0, 30, 30])));
    }

    #[test]
    fn many_small_areas_each_draw_only_themselves() {
        // Two hundred bars across a window, each in a row of its own.
        let damage = Damage::of((0..200).map(|row| {
            let x = row * 37 % 776;
            Px([x, row * 6, x + 24, row * 6 + 6])
        }));
        assert_eq!(damage.0.len(), 200);
        assert!(disjoint(&damage));
        let drawn: u32 = damage
            .0
            .iter()
            .map(|Px([x0, y0, x1, y1])| (x1 - x0) * (y1 - y0))
            .sum();
        assert_eq!(drawn, 200 * 24 * 6);
    }

    #[test]
    fn moving_bars_are_each_compared_with_a_few_areas_not_all() {
        // Two hundred bars, each where it was and where it is, over the
        // two frames a buffer is behind: 800 areas, which compared each
        // with each other were 320 000 comparisons.
        COMPARED.set(0);
        let areas = (0..2u32).flat_map(|frame| {
            (0..200u32).flat_map(move |row| {
                let x = (row * 37 + frame * 4) % 776;
                [x, x + 2].map(|x| Px([x, row * 3, x + 12, row * 3 + 3]))
            })
        });
        let damage = Damage::of(areas);
        assert_eq!(damage.0.len(), 200);
        assert!(disjoint(&damage));
        assert!(COMPARED.get() < 800 * 8, "{} comparisons", COMPARED.get());
    }

    #[test]
    fn more_areas_than_the_cap_are_drawn_as_one_around_them() {
        let damage = Damage::of((0..AREAS as u32 + 1).map(|i| Px([i * 3, 0, i * 3 + 2, 2])));
        assert_eq!(damage.0, [Px([0, 0, AREAS as u32 * 3 + 2, 2])]);
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
        let mut rounded = rect([0.0, 0.0], [100.0, 100.0], Color32::RED);
        let mut bordered = rect([0.0, 0.0], [100.0, 100.0], Color32::RED);
        if let Shape::Rect(r) = &mut rounded.shape {
            r.corner_radius = CornerRadius::same(2);
        }
        if let Shape::Rect(r) = &mut bordered.shape {
            r.stroke = Stroke::new(1.0, Color32::BLACK);
        }
        let mut shapes = vec![
            // Between pixels: its edges are blended.
            rect([0.5, 0.0], [100.0, 100.0], Color32::RED),
            // Seen through.
            rect([0.0, 0.0], [100.0, 100.0], Color32::from_black_alpha(128)),
            rounded,
            bordered,
            // No area.
            rect([5.0, 0.0], [5.0, 100.0], Color32::RED),
        ];
        fill_rects(&mut shapes, 1.0);
        assert!(shapes.iter().all(|s| matches!(s.shape, Shape::Rect(_))));
    }

    #[test]
    fn whole_pixels_are_the_device_s() {
        // 0.5 points at a scale of 2 is a whole pixel.
        let mut shapes = vec![rect([0.5, 0.5], [100.0, 100.0], Color32::RED)];
        fill_rects(&mut shapes, 2.0);
        assert_eq!(cleared(&shapes), [true]);
        let mut shapes = vec![rect([0.5, 0.5], [100.0, 100.0], Color32::RED)];
        fill_rects(&mut shapes, 1.0);
        assert_eq!(cleared(&shapes), [false]);
    }

    #[test]
    fn quads_are_joined_only_with_those_drawn_right_before_them() {
        let mut rounded = rect([0.0, 0.0], [10.0, 10.0], Color32::BLUE);
        if let Shape::Rect(r) = &mut rounded.shape {
            r.corner_radius = CornerRadius::same(2);
        }
        let mut clipped = rect([0.0, 20.0], [10.0, 30.0], Color32::RED);
        clipped.clip_rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(50.0, 50.0));
        let mut shapes = vec![
            rect([0.0, 0.0], [10.0, 10.0], Color32::RED),
            rect([0.0, 10.0], [10.0, 20.0], Color32::RED),
            // Drawn over the first two and under the next: it keeps them apart.
            rounded,
            rect([0.0, 0.0], [10.0, 10.0], Color32::RED),
            // Cut by a clip of its own.
            clipped,
        ];
        fill_rects(&mut shapes, 1.0);
        let vertices: Vec<_> = shapes
            .iter()
            .map(|s| match &s.shape {
                Shape::Mesh(m) => Some(m.vertices.len()),
                Shape::Noop => None,
                Shape::Rect(_) => Some(0),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(vertices, [Some(8), None, Some(0), Some(4), Some(4)]);
    }

    #[test]
    fn small_rectangles_are_quads_drawn_with_the_rest() {
        // Two hundred bars of 12 × 3 pixels: one mesh, one draw, not two
        // hundred clears.
        let mut shapes: Vec<_> = (0..200)
            .map(|i| {
                let y = i as f32 * 3.0;
                rect([0.0, y], [12.0, y + 3.0], Color32::RED)
            })
            .collect();
        fill_rects(&mut shapes, 1.0);
        assert_eq!(cleared(&shapes), [false; 200]);
        let meshes: Vec<_> = shapes
            .iter()
            .filter(|s| matches!(s.shape, Shape::Mesh(_)))
            .collect();
        assert_eq!(meshes.len(), 1, "one mesh made for the run, not one a bar");
        let Shape::Mesh(quads) = &shapes[0].shape else {
            panic!("the quads: {:?}", shapes[0].shape);
        };
        // On the pixels' edges, with no smoothed edge around them.
        assert_eq!(quads.vertices.len(), 4 * 200);
        let corners: Vec<_> = quads.vertices[4..8].iter().map(|v| v.pos).collect();
        assert!(corners.contains(&pos2(0.0, 3.0)) && corners.contains(&pos2(12.0, 6.0)));
        assert!(quads.vertices.iter().all(|v| v.color == Color32::RED));
        let primitives = egui::epaint::tessellator::Tessellator::new(
            1.0,
            egui::epaint::TessellationOptions::default(),
            [1, 1],
            Vec::new(),
        )
        .tessellate_shapes(shapes);
        assert_eq!(primitives.len(), 1, "drawn together");
        // At a scale of 2 a rectangle of 32 × 32 points is 64 × 64 pixels.
        let mut shapes = vec![
            rect([0.0, 0.0], [32.0, 32.0], Color32::RED),
            rect([0.0, 0.0], [32.0, 31.0], Color32::RED),
        ];
        fill_rects(&mut shapes, 2.0);
        assert_eq!(cleared(&shapes), [true, false]);
    }
}
