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
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use egui::epaint::{ClippedShape, CornerRadius, PaintCallback, RectShape, Shape};
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

use crate::present::{Change, Frame, Steps};

/// How many frames back the areas drawn are remembered: a buffer older
/// than that is drawn whole.
const REMEMBERED: usize = 4;

/// How many separate areas a frame draws at most. Each costs a scissored
/// clear, a draw of every shape that reaches it, and a rectangle told to
/// the compositor, which on Wayland is a request of its own (two hundred
/// moving bars were two hundred requests a frame, a fifth of the frame's
/// work). Past this, the window around them is cut into [`AREAS`] cells,
/// and each draws the area around what lies in it: a frame never spends
/// long working out where to draw, and still spares what lies away from
/// every area.
const AREAS: usize = (GRID * GRID) as usize;

/// The cells a side of the window is cut into past [`AREAS`].
const GRID: u32 = 4;

/// Past this many areas, a frame that changes a fifth of the window is
/// drawn whole ([`Damage::mostly`]).
const SCATTERED: usize = GRID as usize;

/// Past this many areas a frame goes to the grid without joining them.
const JOINED: usize = 4 * AREAS;

/// On a device, how many frames after one that changed most of the
/// window are drawn whole without working out where they change. Two
/// hundred moving bars spent a seventh of each frame finding the areas
/// that were then not used; drawing a frame whole that would have been
/// drawn in part costs a device next to nothing, and a run of half a
/// second notices soon enough when the change has become small.
const WHOLE_RUN: u32 = 30;

/// Areas of at least this many pixels are cleared to the background;
/// smaller ones are drawn together as one mesh.
const CLEARED: u32 = 64 * 64;

/// How many frames drawn without a scroll the pixels of the clip last
/// scrolled are kept for.
const KEPT_FOR: u32 = 30;

/// How many shapes back a clear or quad is moved past at most (see
/// [`fill_rects`]): a list's rows lie within a screen of each other, and
/// a frame of ten thousand shapes does not compare each with the rest.
const HOISTED: usize = 256;

/// What a [`fill_rects`] clear is, as the callback the meshes draw it
/// from: the colour, in gamma, that the shape's rectangle (in its clip)
/// is cleared to.
pub(crate) struct Clear(pub(crate) [f32; 4]);

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
    /// Whether the pixels are drawn by the CPU (llvmpipe and the like):
    /// then every pixel a frame spares is work spared, and on a device
    /// next to none is.
    software: bool,
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
        // SAFETY: a plain query of a string on the current context.
        let software = is_software(&unsafe { glow.get_parameter_string(glow::RENDERER) });
        Ok(Rc::new(Gl {
            glow: Arc::new(glow),
            context,
            config,
            display: egl,
            current: Cell::new(0),
            surfaces: Cell::new(0),
            software,
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
    meshes: crate::gl_mesh::Meshes,
    window: Option<GlWindow>,
    programs: crate::gl_shader::Programs,
    /// Whether GL can copy pixels between framebuffers (GL 3, GLES 3).
    blits: bool,
    /// Whether the copies a scroll makes have been made once.
    warmed: bool,
    /// The size of each picture handed over, for where a part of one
    /// handed over again lies in it.
    sizes: HashMap<egui::TextureId, [usize; 2]>,
}

struct GlWindow {
    id: u64,
    surface: Surface<WindowSurface>,
    px: [u32; 2],
    /// The pixels each of the last frames drew, the last first (`None`:
    /// all of them), for a buffer that holds an older frame.
    drawn: VecDeque<Option<Damage>>,
    /// The pixels of the clip a scroll last moved, as the last frame
    /// drew them.
    kept: Option<Kept>,
    /// What the frame [`GlPainter::frame`] decided on moves before it
    /// draws: the clip's pixels, kept, by how many pixels.
    scroll: Option<(Px, [i32; 2])>,
    /// The clip whose pixels the frame keeps once drawn, from now on.
    keep: Option<Px>,
    /// How many more frames are drawn whole without working out where
    /// they change ([`WHOLE_RUN`]).
    whole_for: u32,
    // The surface draws into the window: it stays while the surface does.
    _window: Arc<Window>,
}

impl GlPainter {
    pub(crate) fn new(gl: Rc<Gl>) -> Result<Self, String> {
        if !gl.make_current(None) {
            return Err("the GL context cannot be made current".into());
        }
        let painter =
            egui_glow::Painter::new(Arc::clone(&gl.glow), "", None, crate::surface::DITHER)
                .map_err(|e| e.to_string())?;
        let meshes = crate::gl_mesh::Meshes::new(&gl.glow)?;
        // `LJOS_SCROLL_COPIES=off` draws a scrolled clip again instead of
        // moving its pixels, so the two can be compared.
        let blits = gl.glow.version().major >= 3
            && std::env::var_os("LJOS_SCROLL_COPIES").is_none_or(|v| v != "off");
        Ok(Self {
            gl,
            painter,
            meshes,
            window: None,
            programs: crate::gl_shader::Programs::default(),
            blits,
            warmed: false,
            sizes: HashMap::new(),
        })
    }

    pub(crate) fn gl(&self) -> Rc<Gl> {
        Rc::clone(&self.gl)
    }

    /// The `ShaderEffect` programs this painter links.
    /// Whether the context draws on the CPU.
    pub(crate) fn software(&self) -> bool {
        self.gl.software
    }

    pub(crate) fn programs(&self) -> &crate::gl_shader::Programs {
        &self.programs
    }

    pub(crate) fn max_texture_side(&self) -> usize {
        self.painter.max_texture_side()
    }

    /// What draws, for the inspector: GL's renderer on EGL, which swaps
    /// once the display is ready and redraws only what changed where EGL
    /// says how old a buffer is.
    #[cfg(feature = "mcp")]
    pub(crate) fn describe(&self) -> ljos_dev::mcp::PainterInfo {
        use glutin::display::GetDisplayExtensions;
        let glow = &self.gl.glow;
        // SAFETY: plain queries of strings on the context GL's functions
        // were loaded for.
        let (renderer, version) = unsafe {
            (
                glow.get_parameter_string(glow::RENDERER),
                glow.get_parameter_string(glow::VERSION),
            )
        };
        ljos_dev::mcp::PainterInfo {
            painter: "gl".into(),
            backend: Some(format!(
                "{}, OpenGL {version}",
                self.gl.display.version_string()
            )),
            adapter: Some(renderer),
            present_mode: Some("swap interval 1".into()),
            partial_redraw: Some(self.gl.display.extensions().contains("EGL_EXT_buffer_age")),
        }
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
            kept: None,
            scroll: None,
            keep: None,
            whole_for: 0,
            _window: Arc::clone(window),
        });
        Ok(())
    }

    /// The window is going: its surface goes first.
    pub(crate) fn forget_window(&mut self) {
        let Some(mut window) = self.window.take() else {
            return;
        };
        if let Some(kept) = window.kept.take()
            && self.gl.make_current(Some((window.id, &window.surface)))
        {
            kept.destroy(&self.gl.glow);
        }
        if self.gl.current.get() == window.id {
            self.gl.make_current(None);
        }
    }

    pub(crate) fn resized(&mut self, w: NonZeroU32, h: NonZeroU32) {
        if let Some(window) = self.window.as_mut() {
            window.surface.resize(&self.gl.context, w, h);
            window.px = [w.get(), h.get()];
            window.drawn.clear();
            if let Some(kept) = &mut window.kept {
                kept.drawn = false;
            }
            window.whole_for = 0;
        }
    }

    /// Makes, once, the two copies a scroll makes, one pixel large and
    /// back to where it was. A software rasteriser compiles the code of
    /// each copy the first time it is made, about a frame's time each;
    /// made while the window is idle, the first frame of a scroll does not
    /// pay for it.
    pub(crate) fn warm(&mut self) {
        if !self.gl.software || !self.blits || std::mem::replace(&mut self.warmed, true) {
            return;
        }
        let Some(gw) = self.window.as_ref() else {
            return;
        };
        if !self.gl.make_current(Some((gw.id, &gw.surface))) {
            return;
        }
        let gl = &self.gl.glow;
        let Some(mut kept) = Kept::new(gl, Px([0, 0, 1, 1])) else {
            return;
        };
        kept.copy_from(gl, gw.px);
        kept.move_into(gl, gw.px, [0, 0]);
        kept.destroy(gl);
    }

    /// Whether [`GlPainter::frame`] would use the areas a picture changes
    /// in: not while it draws a run of frames whole.
    pub(crate) fn wants_areas(&self) -> bool {
        self.window.as_ref().is_none_or(|gw| gw.whole_for == 0)
    }

    /// Where `shapes` sample the texels `pending` hands over anew
    /// ([`crate::present::resampled`]).
    pub(crate) fn resampled(
        &self,
        shapes: &[ClippedShape],
        pending: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Vec<egui::Rect> {
        let mut changed = Vec::new();
        for (&id, deltas) in &pending.set {
            let mut size = self.sizes.get(&id).copied();
            for d in deltas {
                let texels = match (d.pos, size) {
                    (Some(at), Some(size)) => {
                        Some(crate::present::texels(at, d.image.size(), size))
                    }
                    (Some(_), None) => None,
                    (None, _) => {
                        size = Some(d.image.size());
                        None
                    }
                };
                changed.push((id, texels));
            }
        }
        crate::present::resampled(shapes, &changed, pixels_per_point)
    }

    /// What of a picture that makes `change` is drawn. The buffer drawn
    /// into still holds the frame shown `age` frames ago (when EGL can
    /// say), so it is drawn again where this frame and the ones since
    /// changed it. Where a picture handed over again is sampled is in
    /// `change` ([`GlPainter::resampled`]).
    pub(crate) fn frame(&mut self, change: Change, pixels_per_point: f32) -> Frame {
        let Some(gw) = self.window.as_mut() else {
            return Frame::Whole;
        };
        gw.scroll = None;
        let px = |areas: Vec<egui::Rect>| -> Vec<Px> {
            areas
                .into_iter()
                .map(|area| Px::around(area, pixels_per_point, gw.px))
                .collect()
        };
        // What this frame changes, and the clip whose pixels it moves.
        let (now, scroll) = match change {
            Change::None => {
                // What changes next may be small.
                gw.whole_for = 0;
                return Frame::Nothing;
            }
            Change::Area(areas) => (Some(px(areas)), None),
            Change::Scroll(s) if self.blits => {
                let rect = Px::around(s.rect, pixels_per_point, gw.px);
                let by = (s.by * pixels_per_point).round();
                let mut areas = px(s.areas);
                let kept = gw.kept.as_mut().filter(|k| k.px == rect && k.drawn);
                match kept {
                    Some(kept) => {
                        kept.idle = 0;
                        (Some(areas), Some((rect, [by.x as i32, by.y as i32])))
                    }
                    // The first frame of a scroll draws the clip, and
                    // keeps it for the next.
                    None => {
                        gw.keep = Some(rect);
                        areas.push(rect);
                        (Some(areas), None)
                    }
                }
            }
            Change::Scroll(s) => {
                let mut areas = px(s.areas);
                areas.push(Px::around(s.rect, pixels_per_point, gw.px));
                (Some(areas), None)
            }
            Change::All => (None, None),
        };
        // What it draws with the frames before it covers at least as
        // much: drawn whole, then and in the frames that redraw it.
        // The clip a scroll moves is copied, not drawn, so it is left out.
        let software = self.gl.software;
        gw.whole_for = gw.whole_for.saturating_sub(1);
        let now = now.map(Damage::of).filter(|d| {
            let whole = d.mostly(gw.px, software);
            if whole && !software {
                gw.whole_for = WHOLE_RUN;
            }
            !whole
        });
        // Asked before the context is current with the surface: EGL
        // answers for the surface either way.
        let age = gw.surface.buffer_age() as usize;
        let mut drawn = now.as_ref().map(|d| d.0.clone()).unwrap_or_default();
        // For a later frame, this one changed all of the clip it moved.
        gw.drawn.push_front(now.map(|now| match scroll {
            Some((rect, _)) => Damage::of(now.0.into_iter().chain([rect])),
            None => now,
        }));
        gw.drawn.truncate(REMEMBERED);
        if age == 0 || age > gw.drawn.len() || gw.drawn[0].is_none() {
            return Frame::Whole;
        }
        for d in gw.drawn.iter().take(age).skip(1) {
            match (d, scroll) {
                (None, _) => return Frame::Whole,
                (Some(d), None) => drawn.extend_from_slice(&d.0),
                // The clip is all moved or drawn again: what the frames
                // since changed in it is not drawn again.
                (Some(d), Some((rect, _))) => {
                    drawn.extend(d.0.iter().flat_map(|px| px.minus(rect)))
                }
            }
        }
        let damage = Damage::of(drawn);
        if damage.mostly(gw.px, self.gl.software) {
            return Frame::Whole;
        }
        gw.scroll = scroll;
        if damage.0.is_empty() && scroll.is_none() {
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

    /// Draws `primitives` (all of `frame`) into the window and shows it,
    /// timing the uploads, the drawing and the present apart. GL works
    /// on after a call returns; what the device still had to do is
    /// waited for in the present.
    pub(crate) fn paint(
        &mut self,
        pixels_per_point: f32,
        clear: egui::Color32,
        frame: &Frame,
        primitives: &[egui::ClippedPrimitive],
        pending: &mut egui::TexturesDelta,
        window: &Window,
    ) -> Steps {
        let Some(gw) = self.window.as_mut() else {
            return Steps::default();
        };
        if !self.gl.make_current(Some((gw.id, &gw.surface))) {
            return Steps::default();
        }
        let started = Instant::now();
        // As egui_glow's own paint does: the pictures first, and those
        // freed only once the frame that may still use them is drawn.
        for (id, deltas) in pending.set.drain() {
            for delta in deltas {
                if delta.pos.is_none() {
                    self.sizes.insert(id, delta.image.size());
                }
                self.painter.set_texture(id, &delta);
            }
        }
        let uploaded = Instant::now();
        let upload = Some(uploaded - started);
        let part = match frame {
            Frame::Nothing => {
                // Only the pictures: nothing is shown.
                free_pending(&mut self.painter, &mut self.sizes, pending);
                return Steps {
                    upload,
                    ..Steps::default()
                };
            }
            Frame::Part(areas) => Some(
                areas
                    .iter()
                    .map(|area| Px::around(*area, pixels_per_point, gw.px))
                    .collect::<Vec<_>>(),
            ),
            Frame::Whole => None,
        };
        let mut moved = None;
        if part.is_some()
            && let Some((rect, by)) = gw.scroll.take()
            && let Some(kept) = &gw.kept
        {
            kept.move_into(&self.gl.glow, gw.px, by);
            moved = Some(rect);
        }
        let gamma = clear.to_normalized_gamma_f32();
        let mut backdrop = None;
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
                    backdrop = Some(egui::ClippedPrimitive {
                        clip_rect: egui::Rect::EVERYTHING,
                        primitive: egui::epaint::Primitive::Mesh(mesh),
                    });
                }
            }
            None => self.painter.clear(gw.px, gamma),
        }
        self.meshes.paint(
            &mut self.painter,
            gw.px,
            pixels_per_point,
            backdrop.iter().chain(primitives),
        );
        free_pending(&mut self.painter, &mut self.sizes, pending);
        keep(&self.gl.glow, gw);
        let drawn = Instant::now();
        window.pre_present_notify();
        // A lost frame is drawn again with the next change; there is
        // nobody to tell. The compositor is told what changed, so it can
        // take only that.
        let _ = match part {
            Some(areas) => {
                let rects: Vec<_> = areas.iter().chain(&moved).map(|px| px.egl(gw.px)).collect();
                gw.surface
                    .swap_buffers_with_damage(&self.gl.context, &rects)
            }
            None => gw.surface.swap_buffers(&self.gl.context),
        };
        Steps {
            upload,
            draw: Some(drawn - uploaded),
            present: Some(drawn.elapsed()),
            moved: moved.is_some(),
        }
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
            self.sizes.remove(&id);
            self.painter.free_texture(id);
        }
    }
}

/// Gives back the pictures `pending` frees, on the current context.
fn free_pending(
    painter: &mut egui_glow::Painter,
    sizes: &mut HashMap<egui::TextureId, [usize; 2]>,
    pending: &mut egui::TexturesDelta,
) {
    for id in std::mem::take(&mut pending.free) {
        sizes.remove(&id);
        painter.free_texture(id);
    }
}

impl Drop for GlPainter {
    fn drop(&mut self) {
        let surface = self.window.as_ref().map(|w| (w.id, &w.surface));
        if self.gl.make_current(surface) {
            self.programs.destroy(&self.gl.glow);
            self.meshes.destroy(&self.gl.glow);
            self.painter.destroy();
        }
        self.forget_window();
    }
}

/// Whether GL's `renderer` draws on the CPU: Mesa's llvmpipe and
/// softpipe, SwiftShader, and the software renderers of Windows and macOS.
fn is_software(renderer: &str) -> bool {
    let r = renderer.to_ascii_lowercase();
    [
        "llvmpipe",
        "softpipe",
        "swiftshader",
        "software",
        "gdi generic",
    ]
    .iter()
    .any(|s| r.contains(s))
}

/// Keeps the pixels of the clip a scroll moves as this frame drew them,
/// for the next frame to move; lets go of them after [`KEPT_FOR`] frames
/// without a scroll.
fn keep(gl: &glow::Context, gw: &mut GlWindow) {
    if let Some(rect) = gw.keep.take()
        && gw.kept.as_ref().is_none_or(|k| k.px != rect)
    {
        if let Some(old) = gw.kept.take() {
            old.destroy(gl);
        }
        gw.kept = Kept::new(gl, rect);
    }
    let Some(kept) = gw.kept.as_mut() else {
        return;
    };
    kept.idle += 1;
    if kept.idle > KEPT_FOR {
        if let Some(kept) = gw.kept.take() {
            kept.destroy(gl);
        }
        return;
    }
    kept.copy_from(gl, gw.px);
}

/// The pixels of a clip of the window, kept in a framebuffer of their own.
/// Moving them is a copy GL makes without drawing a shape; drawing what
/// they show again is every shape in the clip, rasterised.
struct Kept {
    px: Px,
    framebuffer: glow::Framebuffer,
    pixels: Pixels,
    /// Whether they are what the last frame drew there.
    drawn: bool,
    /// Frames drawn since the last that moved them.
    idle: u32,
}

/// What a [`Kept`] clip's pixels are held in.
///
/// A software rasteriser copies between buffers byte for byte only when
/// they share a format and an orientation; otherwise it runs a shader over
/// every pixel, which on llvmpipe costs more than drawing the clip anew. A
/// window's buffer is BGRA and bottom-up, so where the driver can hold BGRA
/// the clip is kept so, upside down; a plain RGBA buffer is the fallback.
enum Pixels {
    Bgra(glow::Texture),
    Rgba(glow::Renderbuffer),
}

impl Kept {
    fn new(gl: &glow::Context, px: Px) -> Option<Kept> {
        let [w, h] = [(px.0[2] - px.0[0]) as i32, (px.0[3] - px.0[1]) as i32];
        // SAFETY: the context is current; the framebuffer bound is put
        // back to the window's before anything else draws.
        unsafe {
            let framebuffer = gl.create_framebuffer().ok()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            let mut pixels = None;
            // Not every driver takes BGRA as a texture's own format; one
            // that does not says so in an error, and the clip is RGBA.
            if let Ok(t) = gl.create_texture() {
                for _ in 0..8 {
                    if gl.get_error() == glow::NO_ERROR {
                        break;
                    }
                }
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::BGRA as i32,
                    w,
                    h,
                    0,
                    glow::BGRA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(t),
                    0,
                );
                if gl.get_error() == glow::NO_ERROR
                    && gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE
                {
                    pixels = Some(Pixels::Bgra(t));
                } else {
                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        None,
                        0,
                    );
                    gl.delete_texture(t);
                }
            }
            if pixels.is_none()
                && let Ok(r) = gl.create_renderbuffer()
            {
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(r));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
                gl.bind_renderbuffer(glow::RENDERBUFFER, None);
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(r),
                );
                pixels = Some(Pixels::Rgba(r));
            }
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let Some(pixels) = pixels else {
                gl.delete_framebuffer(framebuffer);
                return None;
            };
            let kept = Kept {
                px,
                framebuffer,
                pixels,
                drawn: false,
                idle: 0,
            };
            if complete {
                Some(kept)
            } else {
                kept.destroy(gl);
                None
            }
        }
    }

    /// Takes the clip's pixels from the window's buffer, drawn.
    fn copy_from(&mut self, gl: &glow::Context, size: [u32; 2]) {
        let [w, h] = self.size();
        let flip = self.upside_down();
        let r = self.px.egl(size);
        // SAFETY: as in `new`; a copy is not scissored.
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.framebuffer));
            gl.blit_framebuffer(
                r.x,
                r.y,
                r.x + r.width,
                r.y + r.height,
                0,
                if flip { h } else { 0 },
                w,
                if flip { 0 } else { h },
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        self.drawn = true;
    }

    /// Puts the clip's pixels back into the window's buffer, moved `by`
    /// pixels (right and down): all of the clip but what comes into view.
    fn move_into(&self, gl: &glow::Context, size: [u32; 2], by: [i32; 2]) {
        let [w, h] = self.size();
        let [dx, dy] = by;
        // What stays in the clip, in its own pixels from the top left.
        let (sx0, sx1) = ((-dx).max(0), (w - dx).min(w));
        let (sy0, sy1) = ((-dy).max(0), (h - dy).min(h));
        if sx1 <= sx0 || sy1 <= sy0 {
            return;
        }
        let [x0, y0, ..] = self.px.0.map(|v| v as i32);
        let height = size[1] as i32;
        let flip = self.upside_down();
        // SAFETY: as in `copy_from`.
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.framebuffer));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(
                sx0,
                if flip { sy1 } else { h - sy1 },
                sx1,
                if flip { sy0 } else { h - sy0 },
                x0 + sx0 + dx,
                height - (y0 + sy1 + dy),
                x0 + sx1 + dx,
                height - (y0 + sy0 + dy),
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
    }

    /// Whether the clip is kept top row first, as the window's buffer is
    /// not: only a BGRA clip, which a software rasteriser then copies as it
    /// is, row by row.
    fn upside_down(&self) -> bool {
        matches!(self.pixels, Pixels::Bgra(_))
    }

    fn size(&self) -> [i32; 2] {
        let [x0, y0, x1, y1] = self.px.0;
        [(x1 - x0) as i32, (y1 - y0) as i32]
    }

    fn destroy(self, gl: &glow::Context) {
        // SAFETY: the context is current; nothing draws with them after.
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            match self.pixels {
                Pixels::Bgra(t) => gl.delete_texture(t),
                Pixels::Rgba(r) => gl.delete_renderbuffer(r),
            }
        }
    }
}

/// A rectangle of the window's pixels, from the top left: `[x0, y0, x1,
/// y1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Px([u32; 4]);

impl Px {
    /// The pixels `area` (in points) touches, in a window of `size`.
    fn around(area: egui::Rect, pixels_per_point: f32, size: [u32; 2]) -> Px {
        // On what is not below zero a cast to an integer is the floor,
        // and one more where it fell short the ceiling: no call to libm,
        // for each corner of every area of every frame.
        let lo = |v: f32, max: u32| ((v * pixels_per_point).max(0.0) as u32).min(max);
        let hi = |v: f32, max: u32| {
            let v = (v * pixels_per_point).max(0.0);
            let t = (v as u32).min(max);
            (t + u32::from((t as f32) < v)).min(max)
        };
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

    /// The pixels of `self` that are not `other`'s: up to four rectangles.
    fn minus(self, other: Px) -> impl Iterator<Item = Px> {
        let ([x0, y0, x1, y1], [a0, b0, a1, b1]) = (self.0, other.0);
        let parts = if self.overlaps(other) {
            let (top, bottom) = (b0.max(y0), b1.min(y1));
            [
                Px([x0, y0, x1, top]),
                Px([x0, bottom, x1, y1]),
                Px([x0, top, a0.min(x1), bottom]),
                Px([a1.max(x0), top, x1, bottom]),
            ]
        } else {
            [self, Px([0; 4]), Px([0; 4]), Px([0; 4])]
        };
        parts.into_iter().filter(|px| !px.is_empty())
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
    /// Whether the areas leave too little of a window of `size` out to be
    /// worth drawing apart: every shape that crosses from one into another
    /// is cut and drawn in each, each is a clear of its own and a
    /// rectangle told to the compositor. On a `software` rasteriser that
    /// is paid back by each pixel spared, so a quarter of the window left
    /// out is worth it. On a device, pixels cost the CPU nothing and only
    /// the shapes left out spare it work: two hundred moving bars drawn
    /// in the cells around them spared a third of the window and cost a
    /// tenth of the frame more than drawing it whole.
    ///
    /// Areas scattered over the window ([`SCATTERED`]) are drawn whole
    /// sooner: each draws again every shape that reaches it, and a driver
    /// pays for each draw on the thread that makes it. A page sliding
    /// over another changed a third of the window in a dozen cells, which
    /// made forty draws where the whole window is ten; on llvmpipe the
    /// thread that draws spent a fifth less drawing it whole, and the
    /// rasteriser's threads about as much more as that spared.
    fn mostly(&self, size: [u32; 2], software: bool) -> bool {
        let window = u64::from(size[0]) * u64::from(size[1]);
        let covered: u64 = self.0.iter().map(|px| u64::from(px.pixels())).sum();
        let (part, of) = if software { (3, 4) } else { (1, 2) };
        let scattered = self.0.len() > SCATTERED && covered * 5 >= window;
        window > 0 && (covered * of >= window * part || scattered)
    }

    /// The areas that cover `areas`: those that overlap become one around
    /// them, which may overlap more. Swept top to bottom, so an area is
    /// only compared with those that start above its bottom edge, not with
    /// every other: a frame of two hundred moving bars compares each with
    /// a few, not with two hundred.
    fn of(areas: impl IntoIterator<Item = Px>) -> Damage {
        let mut v: Vec<Px> = areas.into_iter().filter(|px| !px.is_empty()).collect();
        // So many that joining them would not bring them down to a few:
        // the grid takes areas that overlap as they are. Two hundred bars,
        // each overlapping the bars above and below once widened to whole
        // pixels, were joined a few at a time over many sorted passes, the
        // most of what a frame spent working out where to draw.
        if v.len() > JOINED {
            return Damage(grid(&v));
        }
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
            return Damage(grid(&v));
        }
        Damage(v)
    }
}

/// `areas` drawn as at most [`AREAS`]: the rectangle around them all is
/// cut into a grid, and each cell draws the rectangle around the parts of
/// `areas` inside it. Cells do not overlap, so neither do these.
fn grid(areas: &[Px]) -> Vec<Px> {
    let Some(Px([x0, y0, x1, y1])) = areas.iter().copied().reduce(Px::union) else {
        return Vec::new();
    };
    let (w, h) = (x1 - x0, y1 - y0);
    // Cell `i` along a side of length `len` from `lo` holds the pixels
    // `p` with `(p - lo) * GRID / len == i`.
    let edges = |lo: u32, len: u32| {
        std::array::from_fn::<u32, { GRID as usize + 1 }, _>(|i| {
            lo + (len * i as u32).div_ceil(GRID)
        })
    };
    let (ex, ey) = (edges(x0, w), edges(y0, h));
    let cell = |lo: u32, len: u32, p: u32| ((p - lo) * GRID / len).min(GRID - 1);
    let mut cells = [None::<Px>; AREAS];
    for a in areas {
        let [ax0, ay0, ax1, ay1] = a.0;
        for cy in cell(y0, h, ay0)..=cell(y0, h, ay1 - 1) {
            for cx in cell(x0, w, ax0)..=cell(x0, w, ax1 - 1) {
                let (cx, cy) = (cx as usize, cy as usize);
                let part = Px([
                    ax0.max(ex[cx]),
                    ay0.max(ey[cy]),
                    ax1.min(ex[cx + 1]),
                    ay1.min(ey[cy + 1]),
                ]);
                if part.is_empty() {
                    continue;
                }
                let c = &mut cells[cy * GRID as usize + cx];
                *c = Some(c.map_or(part, |c| c.union(part)));
            }
        }
    }
    cells.into_iter().flatten().collect()
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
/// next to nothing. So one that shows at least [`CLEARED`] pixels in its
/// clip becomes a clear of them. A smaller one costs little either way,
/// but a clear is a draw of its own and ends the mesh around it (two
/// hundred small bars were two hundred draws), so it becomes a quad on
/// the pixels' edges, cut to its clip so that it needs none, and quads
/// one after another are one mesh. A frame drawn in part cuts a list's
/// rows to the few pixels around each changed text: by their own size
/// they were cleared, fifteen draws a frame of a few hundred pixels each.
///
/// A clear or a quad is then drawn before the shapes ahead of it that it
/// does not touch, which leaves the same pixels: a list's rows, each a
/// clear and a text, were a draw a row, and llvmpipe spends about 60 000
/// instructions on the drawing thread for each; their texts are now one
/// draw. A device fills pixels for next to nothing, and each clear ends
/// the draw and sets up egui's GL state twice around it: on one, every
/// rectangle is a quad (the sixteen rows of a scrolled list were sixteen
/// clears and half the driver's work a frame). Done to the shapes about to
/// be drawn, after they were compared with the last picture (a callback is
/// never the same as another).
pub(crate) fn fill_rects(
    all: &[ClippedShape],
    pixels_per_point: f32,
    software: bool,
) -> Vec<ClippedShape> {
    // Made anew from what the window keeps (`LastPicture::keep`) rather
    // than from a copy of it: a rectangle that becomes a quad or a clear
    // is only read, and two hundred bars were two hundred clones and
    // drops a frame.
    let mut fills: Vec<Fill<'_>> = Vec::with_capacity(all.len());
    let mut any = false;
    for cs in all {
        let fill = match &cs.shape {
            Shape::Rect(rect) => clearable(rect, pixels_per_point).map(|color| (rect.rect, color)),
            _ => None,
        };
        let Some((r, color)) = fill else {
            fills.push(Fill::Shape(cs));
            continue;
        };
        any = true;
        let shown = r.intersect(scissored(cs.clip_rect, pixels_per_point));
        if !shown.is_positive() {
            continue;
        }
        fills.push(
            if !software || shown.area() * pixels_per_point * pixels_per_point < CLEARED as f32 {
                Fill::Quad(shown, color)
            } else {
                Fill::Clear(cs.clip_rect, r, color.to_normalized_gamma_f32())
            },
        );
    }
    if any {
        hoist(&mut fills, pixels_per_point);
    }
    let mut shapes = Vec::with_capacity(fills.len());
    // The quads since something else came between them: they become one
    // mesh, made at its size once the run ends (grown a quad at a time,
    // two hundred bars were a hundred reallocations a frame).
    // Room for every shape once: grown as runs lengthened, it was seven
    // allocations a frame.
    let mut run: Vec<(egui::Rect, egui::Color32)> = Vec::with_capacity(fills.len());
    for fill in fills {
        match fill {
            Fill::Quad(r, color) => {
                run.push((r, color));
                continue;
            }
            Fill::Shape(cs) => {
                quads_into_mesh(&mut shapes, &mut run);
                shapes.push(cs.clone());
            }
            Fill::Clear(clip_rect, rect, color) => {
                quads_into_mesh(&mut shapes, &mut run);
                shapes.push(ClippedShape {
                    clip_rect,
                    shape: Shape::Callback(PaintCallback {
                        rect,
                        callback: Arc::new(Clear(color)),
                    }),
                });
            }
        }
    }
    quads_into_mesh(&mut shapes, &mut run);
    shapes
}

/// A shape as [`fill_rects`] draws it.
enum Fill<'a> {
    /// As it is.
    Shape(&'a ClippedShape),
    /// A quad of these pixels, already cut to its clip.
    Quad(egui::Rect, egui::Color32),
    /// A clear, in this clip, of this rectangle, to this colour in gamma.
    Clear(egui::Rect, egui::Rect, [f32; 4]),
}

/// `clip` as the scissor cuts it: at the pixel edges nearest its own
/// (`gl_mesh::scissor`), so a quad cut to it covers the pixels the
/// rectangle would have in it.
fn scissored(clip: egui::Rect, pixels_per_point: f32) -> egui::Rect {
    let at = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    egui::Rect::from_min_max(
        egui::pos2(at(clip.min.x), at(clip.min.y)),
        egui::pos2(at(clip.max.x), at(clip.max.y)),
    )
}

/// Moves each clear and quad back past the shapes before it that draw none
/// of its pixels, so that what lay between them is drawn together: up to
/// the last clear or quad, whose run it joins. Another callback draws what
/// only it knows, so a clear or quad stays after it.
fn hoist(fills: &mut [Fill<'_>], pixels_per_point: f32) {
    // Where each shape may draw, in points: feathered edges reach half a
    // pixel out, and a pixel is kept for rounding.
    let margin = 1.0 / pixels_per_point;
    let reach = |f: &Fill<'_>| -> Option<egui::Rect> {
        let s = match f {
            Fill::Quad(r, _) => return Some(*r),
            Fill::Clear(clip, r, _) => return Some(r.intersect(*clip)),
            Fill::Shape(s) => s,
        };
        let r = match &s.shape {
            Shape::Noop => return Some(egui::Rect::NOTHING),
            Shape::Callback(_) => return None,
            other => other.visual_bounding_rect().expand(margin),
        };
        Some(r.intersect(s.clip_rect))
    };
    let mut reaches: Vec<Option<egui::Rect>> = fills.iter().map(reach).collect();
    for i in 0..fills.len() {
        let Some(mine) = reaches[i] else { continue };
        let quad = match fills[i] {
            Fill::Shape(_) => continue,
            Fill::Quad(..) => true,
            Fill::Clear(..) => false,
        };
        let mut to = i;
        while to > 0 && i - to < HOISTED {
            let kin = match fills[to - 1] {
                Fill::Shape(_) => false,
                Fill::Quad(..) => quad,
                Fill::Clear(..) => !quad,
            };
            match reaches[to - 1] {
                Some(r) if !kin && !r.intersects(mine) => to -= 1,
                _ => break,
            }
        }
        if to < i {
            fills[to..=i].rotate_right(1);
            reaches[to..=i].rotate_right(1);
        }
    }
}

/// Adds the quads of `run` to `shapes` as one mesh, and leaves `run`
/// empty. They are cut to their clips already.
fn quads_into_mesh(shapes: &mut Vec<ClippedShape>, run: &mut Vec<(egui::Rect, egui::Color32)>) {
    if run.is_empty() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    mesh.reserve_vertices(4 * run.len());
    mesh.reserve_triangles(2 * run.len());
    for &(r, color) in run.iter() {
        mesh.add_colored_rect(r, color);
    }
    shapes.push(ClippedShape {
        clip_rect: egui::Rect::EVERYTHING,
        shape: Shape::mesh(mesh),
    });
    run.clear();
}

/// The colour of `rect` when a clear draws the same pixels it does.
fn clearable(rect: &RectShape, pixels_per_point: f32) -> Option<egui::Color32> {
    let r = rect.rect;
    let plain = rect.corner_radius == CornerRadius::ZERO
        && rect.stroke.is_empty()
        && rect.blur_width <= 0.0
        && rect.brush.is_none()
        && rect.fill.is_opaque();
    let on_pixels = r.is_positive()
        && ljos_render::b0::all_near_whole(
            [r.min.x, r.min.y, r.max.x, r.max.y].map(|v| v * pixels_per_point),
        );
    (plain && on_pixels).then_some(rect.fill)
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
    fn areas_that_leave_little_of_the_window_out_draw_it_whole() {
        let most = Damage(vec![Px([0, 0, 100, 60]), Px([0, 60, 50, 100])]);
        assert!(most.mostly([100, 100], true));
        let some = Damage(vec![Px([0, 0, 100, 60]), Px([0, 60, 30, 100])]);
        assert!(!some.mostly([100, 100], true));
        assert!(!Damage::default().mostly([0, 0], true));
    }

    #[test]
    fn on_a_device_areas_that_leave_less_than_half_out_draw_it_whole() {
        let some = Damage(vec![Px([0, 0, 100, 60]), Px([0, 60, 30, 100])]);
        assert!(some.mostly([100, 100], false));
        let half = Damage(vec![Px([0, 0, 100, 50])]);
        assert!(half.mostly([100, 100], false));
        let less = Damage(vec![Px([0, 0, 100, 40]), Px([0, 90, 50, 100])]);
        assert!(!less.mostly([100, 100], false));
        assert!(!Damage::default().mostly([0, 0], false));
    }

    #[test]
    fn areas_scattered_over_a_fair_part_of_the_window_draw_it_whole() {
        // A row of cells across a fifth of the window, as a page sliding
        // in changes it.
        let cells = |n: u32| Damage((0..n).map(|i| Px([i * 20, 0, i * 20 + 20, 20])).collect());
        for software in [true, false] {
            assert!(cells(5).mostly([100, 100], software));
            // As many cells, over less of the window, are drawn apart.
            let small = Damage((0..5).map(|i| Px([i * 20, 0, i * 20 + 20, 10])).collect());
            assert!(!small.mostly([100, 100], software));
        }
        // A few cells over as much stay apart on a rasteriser on the CPU.
        assert!(!cells(4).mostly([100, 100], true));
    }

    #[test]
    fn rasterisers_on_the_cpu_are_told_by_their_names() {
        assert!(is_software("llvmpipe (LLVM 21.1.0, 256 bits)"));
        assert!(is_software("softpipe"));
        assert!(is_software("SwiftShader Device (LLVM 10.0.0)"));
        assert!(is_software("Apple Software Renderer"));
        assert!(!is_software(
            "AMD Radeon Graphics (radeonsi, gfx1103, LLVM 21.1.0, DRM 3.64)"
        ));
        assert!(!is_software("Mesa Intel(R) UHD Graphics 620 (KBL GT2)"));
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
    fn a_few_small_areas_each_draw_only_themselves() {
        let damage = Damage::of((0..AREAS as u32).map(|row| {
            let x = row * 37 % 776;
            Px([x, row * 6, x + 24, row * 6 + 6])
        }));
        assert_eq!(damage.0.len(), AREAS);
        assert!(disjoint(&damage));
        let drawn: u32 = damage.0.iter().map(|px| px.pixels()).sum();
        assert_eq!(drawn, AREAS as u32 * 24 * 6);
    }

    /// Whether every pixel of `areas` is inside one of `damage`'s.
    fn covers(damage: &Damage, areas: &[Px]) -> bool {
        areas.iter().all(|a| {
            let [x0, y0, x1, y1] = a.0;
            (y0..y1).all(|y| {
                (x0..x1).all(|x| {
                    damage
                        .0
                        .iter()
                        .any(|d| d.0[0] <= x && x < d.0[2] && d.0[1] <= y && y < d.0[3])
                })
            })
        })
    }

    #[test]
    fn many_areas_are_drawn_as_a_few_cells_that_cover_them() {
        // Two hundred bars across a window, each in a row of its own.
        let areas: Vec<Px> = (0..200)
            .map(|row| {
                let x = row * 37 % 776;
                Px([x, row * 6, x + 24, row * 6 + 6])
            })
            .collect();
        let damage = Damage::of(areas.iter().copied());
        assert!(damage.0.len() <= AREAS, "{} areas", damage.0.len());
        assert!(disjoint(&damage));
        assert!(covers(&damage, &areas));
    }

    #[test]
    fn cells_spare_what_lies_away_from_every_area() {
        // Two columns of small areas, far apart: what lies between them is
        // not drawn.
        let areas: Vec<Px> = (0..100u32)
            .flat_map(|row| [0, 700].map(|x| Px([x, row * 6, x + 20, row * 6 + 4])))
            .collect();
        let damage = Damage::of(areas.iter().copied());
        assert!(damage.0.len() <= AREAS, "{} areas", damage.0.len());
        assert!(disjoint(&damage));
        assert!(covers(&damage, &areas));
        let drawn: u32 = damage.0.iter().map(|px| px.pixels()).sum();
        assert!(drawn < 720 * 600 / 2, "{drawn} pixels drawn");
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
        assert!(damage.0.len() <= AREAS);
        assert!(disjoint(&damage));
        assert!(COMPARED.get() < 800 * 8, "{} comparisons", COMPARED.get());
    }

    #[test]
    fn opaque_rectangles_on_whole_pixels_are_cleared() {
        let shapes = vec![
            rect([0.0, 0.0], [400.0, 40.0], Color32::WHITE),
            rect([0.0, 40.0], [400.0, 80.0], Color32::from_gray(0xee)),
        ];
        let shapes = fill_rects(&shapes, 1.0, true);
        assert_eq!(cleared(&shapes), [true, true]);
    }

    /// A rectangle that is not cleared: a row's text, as far as order goes.
    fn label(min: [f32; 2], max: [f32; 2]) -> ClippedShape {
        let mut s = rect(min, max, Color32::BLACK);
        if let Shape::Rect(r) = &mut s.shape {
            r.corner_radius = CornerRadius::same(2);
        }
        s
    }

    /// Each shape as `c` (a clear), `l` (anything else) or `x` (another
    /// callback), in the order they are drawn, with where each started.
    fn order(shapes: &[ClippedShape], before: &[ClippedShape]) -> Vec<(char, usize)> {
        shapes
            .iter()
            .map(|s| {
                let kind = match &s.shape {
                    Shape::Callback(cb) if cb.callback.is::<Clear>() => 'c',
                    Shape::Callback(_) => 'x',
                    _ => 'l',
                };
                let from = before
                    .iter()
                    .position(|b| b.shape.visual_bounding_rect() == s.shape.visual_bounding_rect())
                    .unwrap_or(usize::MAX);
                (kind, from)
            })
            .collect()
    }

    #[test]
    fn rows_are_cleared_first_and_their_texts_drawn_together() {
        let mut before = Vec::new();
        for row in 0..4 {
            let y = row as f32 * 40.0;
            before.push(rect([0.0, y], [400.0, y + 40.0], Color32::WHITE));
            before.push(label([12.0, y + 10.0], [80.0, y + 30.0]));
        }
        let shapes = fill_rects(&before, 1.0, true);
        assert_eq!(
            order(&shapes, &before),
            [
                ('c', 0),
                ('c', 2),
                ('c', 4),
                ('c', 6),
                ('l', 1),
                ('l', 3),
                ('l', 5),
                ('l', 7)
            ]
        );
    }

    #[test]
    fn a_clear_stays_after_what_it_covers_and_after_other_callbacks() {
        let other = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Callback(PaintCallback {
                rect: Rect::from_min_max(pos2(0.0, 500.0), pos2(10.0, 510.0)),
                callback: Arc::new(egui_glow::CallbackFn::new(|_, _| {})),
            }),
        };
        let before = vec![
            label([0.0, 0.0], [50.0, 50.0]),
            // Covers the first label: drawn after it.
            rect([0.0, 0.0], [400.0, 80.0], Color32::WHITE),
            label([0.0, 100.0], [50.0, 150.0]),
            other,
            // Drawn after the callback, though it lies apart from it.
            rect([0.0, 200.0], [400.0, 280.0], Color32::WHITE),
            // Half a pixel from the clear below: a smoothed edge reaches it.
            label([0.0, 300.0], [50.0, 399.5]),
            rect([0.0, 400.0], [400.0, 480.0], Color32::WHITE),
        ];
        let shapes = fill_rects(&before, 1.0, true);
        assert_eq!(
            order(&shapes, &before),
            [
                ('l', 0),
                ('c', 1),
                ('l', 2),
                ('x', 3),
                ('c', 4),
                ('l', 5),
                ('c', 6)
            ]
        );
    }

    #[test]
    fn a_device_draws_large_rectangles_with_the_mesh() {
        let shapes: Vec<_> = (0..16)
            .map(|i| {
                let y = i as f32 * 40.0;
                rect([0.0, y], [400.0, y + 40.0], Color32::WHITE)
            })
            .collect();
        let shapes = fill_rects(&shapes, 1.0, false);
        assert_eq!(shapes.len(), 1, "one mesh, no clears");
        let Shape::Mesh(quads) = &shapes[0].shape else {
            panic!("the quads: {:?}", shapes[0].shape);
        };
        assert_eq!(quads.vertices.len(), 4 * 16);
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
        let shapes = vec![
            // Between pixels: its edges are blended.
            rect([0.5, 0.0], [100.0, 100.0], Color32::RED),
            // Seen through.
            rect([0.0, 0.0], [100.0, 100.0], Color32::from_black_alpha(128)),
            rounded,
            bordered,
            // No area.
            rect([5.0, 0.0], [5.0, 100.0], Color32::RED),
        ];
        let shapes = fill_rects(&shapes, 1.0, true);
        assert!(shapes.iter().all(|s| matches!(s.shape, Shape::Rect(_))));
    }

    #[test]
    fn an_area_takes_in_every_pixel_it_touches() {
        for i in -40..4000 {
            let v = i as f32 * 0.37;
            let Px([x0, y0, x1, y1]) = Px::around(
                egui::Rect::from_min_max([v, v].into(), [v + 1.3, v].into()),
                1.25,
                [2000, 2000],
            );
            let (lo, hi) = (v * 1.25, (v + 1.3) * 1.25);
            assert_eq!(x0, (lo.floor().max(0.0) as u32).min(2000), "{v}");
            assert_eq!(y1, (lo.ceil().max(0.0) as u32).min(2000), "{v}");
            assert_eq!(x1, (hi.ceil().max(0.0) as u32).min(2000), "{v}");
            assert_eq!(y0, x0);
        }
    }

    #[test]
    fn what_lies_outside_a_rectangle_is_at_most_four() {
        let window = Px([0, 0, 100, 100]);
        let middle = Px([20, 30, 60, 70]);
        let rest: Vec<_> = window.minus(middle).collect();
        assert_eq!(
            rest.iter().map(|px| px.pixels()).sum::<u32>(),
            10_000 - 1_600
        );
        assert!(rest.iter().all(|px| !px.overlaps(middle)));
        assert_eq!(middle.minus(window).count(), 0);
        let apart = Px([200, 200, 210, 210]);
        assert_eq!(apart.minus(window).collect::<Vec<_>>(), [apart]);
        let top = Px([0, 0, 100, 50]);
        assert_eq!(
            window.minus(top).collect::<Vec<_>>(),
            [Px([0, 50, 100, 100])]
        );
    }

    #[test]
    fn whole_pixels_are_the_device_s() {
        // 0.5 points at a scale of 2 is a whole pixel.
        let shapes = vec![rect([0.5, 0.5], [100.0, 100.0], Color32::RED)];
        let shapes = fill_rects(&shapes, 2.0, true);
        assert_eq!(cleared(&shapes), [true]);
        let shapes = vec![rect([0.5, 0.5], [100.0, 100.0], Color32::RED)];
        let shapes = fill_rects(&shapes, 1.0, true);
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
        let shapes = vec![
            rect([0.0, 0.0], [10.0, 10.0], Color32::RED),
            rect([0.0, 10.0], [10.0, 20.0], Color32::RED),
            // Drawn over the first two and under the next: it keeps them apart.
            rounded,
            rect([0.0, 0.0], [10.0, 10.0], Color32::RED),
            // Cut to a clip of its own already: it joins the one before.
            clipped,
        ];
        let shapes = fill_rects(&shapes, 1.0, true);
        let vertices: Vec<_> = shapes
            .iter()
            .map(|s| match &s.shape {
                Shape::Mesh(m) => Some(m.vertices.len()),
                Shape::Rect(_) => Some(0),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(vertices, [Some(8), Some(0), Some(8)]);
    }

    #[test]
    fn a_row_cut_to_a_few_pixels_is_a_quad_of_them() {
        // A frame drawn in part cuts a row to the pixels around its text:
        // they are too few to be worth a clear of their own, and a row
        // whose part is drawn elsewhere is not drawn here at all.
        let mut row = rect([0.0, 0.0], [400.0, 40.0], Color32::RED);
        row.clip_rect = Rect::from_min_max(pos2(10.6, 10.2), pos2(66.4, 23.0));
        let mut gone = rect([0.0, 40.0], [400.0, 80.0], Color32::RED);
        gone.clip_rect = row.clip_rect;
        let shapes = fill_rects(&[row, gone], 1.0, true);
        assert_eq!(shapes.len(), 1);
        let Shape::Mesh(m) = &shapes[0].shape else {
            panic!("{:?}", shapes[0].shape)
        };
        assert_eq!(shapes[0].clip_rect, Rect::EVERYTHING);
        // The scissor would have cut it at the nearest pixel edges.
        assert_eq!(
            m.calc_bounds(),
            Rect::from_min_max(pos2(11.0, 10.0), pos2(66.0, 23.0))
        );
    }

    #[test]
    fn small_rectangles_are_quads_drawn_with_the_rest() {
        // Two hundred bars of 12 × 3 pixels: one mesh, one draw, not two
        // hundred clears.
        let shapes: Vec<_> = (0..200)
            .map(|i| {
                let y = i as f32 * 3.0;
                rect([0.0, y], [12.0, y + 3.0], Color32::RED)
            })
            .collect();
        let shapes = fill_rects(&shapes, 1.0, true);
        assert_eq!(
            shapes.len(),
            1,
            "one mesh made for the run, not one a bar, and no place left for the others"
        );
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
        let shapes = vec![
            rect([0.0, 0.0], [32.0, 32.0], Color32::RED),
            rect([0.0, 0.0], [32.0, 31.0], Color32::RED),
        ];
        let shapes = fill_rects(&shapes, 2.0, true);
        assert_eq!(cleared(&shapes), [true, false]);
    }
}
