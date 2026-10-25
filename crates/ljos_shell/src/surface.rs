//! What the shell draws a platform window with: the graphics device all
//! windows share, a painter for each, and the surface of a window that
//! is open.
//!
//! The surface belongs to the window: it is made when the window opens
//! and goes before the window does. The painter stays with the pictures
//! it was given, so a window that is hidden and shown again draws at
//! once.

use std::num::NonZeroU32;
use std::sync::{Arc, mpsc};

use crate::present::{Change, Frame, Steps};
use egui::ViewportId;
use egui_wgpu::{
    RendererOptions, SurfaceConfig, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew,
    WgpuSetupExisting, wgpu,
};
use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_host::{WindowId, WindowState};

#[cfg(gl_painter)]
use crate::gl::{Gl, GlPainter};

/// Whether the painters add egui's noise to colours as they cut them to
/// eight bits. They do not: the noise is by place on screen, so the pixels
/// of a scrolled clip that are moved rather than drawn again would keep the
/// noise of where they were and differ from a clip drawn there by one or
/// two levels; Qt Quick does not dither either.
pub(crate) const DITHER: bool = false;

/// What the windows are drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    /// GL through EGL, and wgpu where that cannot be had.
    Gl,
    Wgpu,
}

impl Choice {
    /// From `LJOS_PAINTER` (`gl` or `wgpu`); GL where the platform has
    /// EGL, unless that says otherwise.
    pub(crate) fn from_env(value: Option<&str>) -> Choice {
        match value.map(str::trim) {
            Some(v) if v.eq_ignore_ascii_case("wgpu") => Choice::Wgpu,
            Some(v) if v.eq_ignore_ascii_case("gl") => Choice::Gl,
            _ if cfg!(gl_painter) => Choice::Gl,
            _ => Choice::Wgpu,
        }
    }
}

/// The device the first window was given. Every later window draws with
/// the same one: a second device would be a second set of everything.
#[derive(Clone)]
pub(crate) enum Gpu {
    Wgpu {
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
    },
    #[cfg(gl_painter)]
    Gl(std::rc::Rc<Gl>),
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu").finish_non_exhaustive()
    }
}

impl Gpu {
    /// The device of a painter that has drawn into a window; `None`
    /// before its first window.
    fn of(painter: &Painter) -> Option<Gpu> {
        match painter {
            Painter::Wgpu(painter) => {
                let state = painter.render_state()?;
                Some(Gpu::Wgpu {
                    instance: state.instance,
                    adapter: state.adapter,
                    device: state.device,
                    queue: state.queue,
                })
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => Some(Gpu::Gl(painter.gl())),
        }
    }
}

/// What draws the pictures of one window's context.
// One per context, made once: its size does not matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Painter {
    Wgpu(egui_wgpu::winit::Painter),
    #[cfg(gl_painter)]
    Gl(GlPainter),
}

impl Painter {
    async fn set_window(&mut self, window: &Arc<Window>) -> Result<(), String> {
        match self {
            Painter::Wgpu(painter) => painter
                .set_window(ViewportId::ROOT, Some(Arc::clone(window)))
                .await
                .map_err(|e| e.to_string()),
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.set_window(window),
        }
    }

    pub(crate) fn max_texture_side(&self) -> Option<usize> {
        match self {
            Painter::Wgpu(painter) => painter.max_texture_side(),
            #[cfg(gl_painter)]
            Painter::Gl(painter) => Some(painter.max_texture_side()),
        }
    }

    /// The window is going: what draws into it goes first.
    pub(crate) fn forget_window(&mut self) {
        match self {
            Painter::Wgpu(painter) => painter.gc_viewports(&egui::ViewportIdSet::default()),
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.forget_window(),
        }
    }

    fn resized(&mut self, w: NonZeroU32, h: NonZeroU32) {
        match self {
            Painter::Wgpu(painter) => painter.on_window_resized(ViewportId::ROOT, w, h),
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.resized(w, h),
        }
    }

    /// What draws, for the inspector; `None` before the first window.
    #[cfg(feature = "mcp")]
    pub(crate) fn describe(&self) -> Option<ljos_dev::mcp::PainterInfo> {
        match self {
            Painter::Wgpu(painter) => {
                let info = painter.render_state()?.adapter.get_info();
                let driver = [info.driver.as_str(), info.driver_info.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(ljos_dev::mcp::PainterInfo {
                    painter: "wgpu".into(),
                    backend: Some(format!("{:?}", info.backend)),
                    adapter: Some(if driver.is_empty() {
                        info.name
                    } else {
                        format!("{} ({driver})", info.name)
                    }),
                    // As `wgpu_painter` configures it.
                    present_mode: Some(format!("{:?}", wgpu::PresentMode::Fifo)),
                    partial_redraw: Some(false),
                })
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => Some(painter.describe()),
        }
    }

    /// Whether the painter fills plain rectangles cheaper than meshes
    /// (`QmlView::set_solid_rects`).
    pub(crate) fn fills_rects(&self) -> bool {
        match self {
            Painter::Wgpu(_) => false,
            #[cfg(gl_painter)]
            Painter::Gl(_) => true,
        }
    }

    /// Whether the painter runs a `ShaderEffect`'s program
    /// (`QmlView::set_draws_shaders`). wgpu would need the programs in
    /// WGSL and a pipeline of their own; it draws nothing in their place.
    pub(crate) fn draws_shaders(&self) -> bool {
        match self {
            Painter::Wgpu(_) => false,
            #[cfg(gl_painter)]
            Painter::Gl(_) => true,
        }
    }

    /// The shapes as this painter draws them best.
    pub(crate) fn prepare(
        &self,
        shapes: &[egui::epaint::ClippedShape],
        pixels_per_point: f32,
    ) -> Vec<egui::epaint::ClippedShape> {
        match self {
            // Shader callbacks never reach it (`draws_shaders`).
            Painter::Wgpu(_) => {
                let _ = pixels_per_point;
                shapes.to_vec()
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => {
                let mut shapes =
                    crate::gl::fill_rects(shapes, pixels_per_point, painter.software());
                crate::gl_shader::programs(painter.programs(), &mut shapes);
                shapes
            }
        }
    }

    /// Readies, while the window is idle, what drawing it may soon need.
    pub(crate) fn warm(&mut self) {
        match self {
            Painter::Wgpu(_) => {}
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.warm(),
        }
    }

    /// Whether [`Painter::frame`] uses where a picture changes, or only
    /// whether it does (`LastPicture::differs`): egui-wgpu draws every
    /// frame whole.
    pub(crate) fn wants_areas(&self) -> bool {
        match self {
            Painter::Wgpu(_) => false,
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.wants_areas(),
        }
    }

    /// Where `shapes` sample the pictures `pending` hands over again, for
    /// a painter that draws only where the picture changed.
    pub(crate) fn resampled(
        &self,
        shapes: &[egui::epaint::ClippedShape],
        pending: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Vec<egui::Rect> {
        match self {
            Painter::Wgpu(_) => {
                let _ = (shapes, pending, pixels_per_point);
                Vec::new()
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.resampled(shapes, pending, pixels_per_point),
        }
    }

    /// What of a picture that makes `change` is drawn, with the pictures
    /// `pending` hands over.
    pub(crate) fn frame(
        &mut self,
        change: Change,
        pending: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Frame {
        #[cfg(not(gl_painter))]
        let _ = pixels_per_point;
        match self {
            Painter::Wgpu(_) => wgpu_frame(change, pending),
            #[cfg(gl_painter)]
            Painter::Gl(painter) => painter.frame(change, pixels_per_point),
        }
    }

    /// Draws `primitives` (all of `frame`) into `window`, with the
    /// pictures `pending` hands over, and shows them; says how long the
    /// steps took where the painter can tell them apart.
    pub(crate) fn paint(
        &mut self,
        pixels_per_point: f32,
        clear: egui::Color32,
        frame: &Frame,
        primitives: &[egui::ClippedPrimitive],
        pending: &mut egui::TexturesDelta,
        window: &Arc<Window>,
    ) -> Steps {
        match self {
            Painter::Wgpu(painter) => {
                if !wgpu_presents(frame) {
                    return Steps::default();
                }
                let upload = wgpu_upload(painter, pending);
                let started = std::time::Instant::now();
                // egui-wgpu draws and presents in one call, and times only
                // the waits inside it: for the surface's next texture, the
                // submit and the present.
                let waited = painter.paint_and_update_textures(
                    ViewportId::ROOT,
                    pixels_per_point,
                    clear.to_normalized_gamma_f32(),
                    primitives,
                    pending,
                    Vec::new(),
                    window,
                );
                let all = started.elapsed();
                let waited = std::time::Duration::from_secs_f32(waited.max(0.0)).min(all);
                Steps {
                    upload,
                    draw: Some(all - waited),
                    present: Some(waited),
                    moved: false,
                }
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => {
                painter.paint(pixels_per_point, clear, frame, primitives, pending, window)
            }
        }
    }
}

/// What egui-wgpu draws of a picture that makes `change`: every frame
/// whole, or nothing when neither the picture nor its pictures changed.
fn wgpu_frame(change: Change, pending: &egui::TexturesDelta) -> Frame {
    if change == Change::None && pending.is_empty() {
        Frame::Nothing
    } else {
        Frame::Whole
    }
}

/// Whether egui-wgpu is handed `frame`. It clears and presents whatever it
/// is handed, even no shapes at all, so a frame of nothing must not reach
/// it: the window would show only its clear colour until the next change,
/// and flicker on every input that changes nothing. Nothing is pending
/// then ([`wgpu_frame`]), so no picture waits for an upload.
fn wgpu_presents(frame: &Frame) -> bool {
    *frame != Frame::Nothing
}

/// The graphics drivers being started while the application is built, on
/// a thread of its own. Starting them takes longer than building most
/// applications (hundreds of milliseconds for EGL on Mesa, more for
/// Vulkan's loader and its layers); the first window only waits for what
/// is left of it. One start-up at a time, for what the windows will be
/// drawn with: EGL's display for GL, else one wgpu instance for every
/// backend. Drivers started side by side, each on a thread of its own,
/// crashed and hung the process.
pub(crate) struct Starting {
    choice: Choice,
    started: Started,
    /// Waited for when the application ends before its first window: a
    /// process that ends while a driver is starting can hang in its exit.
    thread: Option<std::thread::JoinHandle<()>>,
}

enum Started {
    Wgpu {
        instance: mpsc::Receiver<wgpu::Instance>,
        setup: Box<WgpuSetupCreateNew>,
    },
    #[cfg(gl_painter)]
    Gl(mpsc::Receiver<Result<crate::gl::Display, String>>),
}

impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl std::fmt::Debug for Starting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Starting");
        match &self.started {
            Started::Wgpu { setup, .. } => f.field("backends", &setup.instance_descriptor.backends),
            #[cfg(gl_painter)]
            Started::Gl(_) => f.field("backends", &"EGL"),
        }
        .finish_non_exhaustive()
    }
}

impl Starting {
    /// Starts the drivers `choice` draws with.
    pub(crate) fn start(display: OwnedDisplayHandle, choice: Choice) -> Self {
        let spawn = |run: Box<dyn FnOnce() + Send>| {
            std::thread::Builder::new()
                .name("ljos-gpu".into())
                .spawn(run)
                .ok()
        };
        // Without the thread the first window's painter starts them.
        #[cfg(gl_painter)]
        if choice == Choice::Gl {
            let (tx, display_rx) = mpsc::sync_channel(1);
            let thread = spawn(Box::new(move || {
                let _ = tx.send(crate::gl::Display::start(&display));
            }));
            return Self {
                choice,
                started: Started::Gl(display_rx),
                thread,
            };
        }
        let setup = create_new(display);
        let (tx, instance) = mpsc::sync_channel(1);
        let new = WgpuSetup::CreateNew(setup.clone());
        let thread = spawn(Box::new(move || {
            let _ = tx.send(pollster::block_on(new.new_instance()));
        }));
        Self {
            choice,
            started: Started::Wgpu {
                instance,
                setup: Box::new(setup),
            },
            thread,
        }
    }

    /// What the windows are drawn with.
    pub(crate) fn choice(&self) -> Choice {
        self.choice
    }

    /// The device for `window`: GL's context on the display started, or
    /// the adapter wgpu ranks first among those that can draw into it.
    /// `None` when there is none; the painter then starts its own and says
    /// why.
    pub(crate) fn device_for(self, window: &Arc<Window>) -> Option<Gpu> {
        match &self.started {
            #[cfg(gl_painter)]
            Started::Gl(display) => {
                let display = display.recv().ok()?.ok()?;
                Gl::with_display(display, window).ok().map(Gpu::Gl)
            }
            Started::Wgpu { instance, setup } => {
                let instance = instance.recv().ok()?;
                let surface = instance.create_surface(Arc::clone(window)).ok()?;
                let adapter =
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: setup.power_preference,
                        force_fallback_adapter: false,
                        compatible_surface: Some(&surface),
                        apply_limit_buckets: false,
                    }));
                // The painter makes the window's own surface.
                drop(surface);
                let adapter = adapter.ok()?;
                let descriptor = (setup.device_descriptor)(&adapter);
                let (device, queue) =
                    pollster::block_on(adapter.request_device(&descriptor)).ok()?;
                Some(Gpu::Wgpu {
                    instance,
                    adapter,
                    device,
                    queue,
                })
            }
        }
    }
}

/// How a wgpu device is asked for: every backend wgpu was built with (or
/// those `WGPU_BACKEND` names), on the window system's display.
fn create_new(display: OwnedDisplayHandle) -> WgpuSetupCreateNew {
    let mut new = WgpuSetupCreateNew::from_display_handle(display);
    // A debug build turns wgpu's validation on, and on GL that hands
    // EGL a debug callback which wgpu-hal 30.0.1 lets read a null
    // command name: Mesa reports a failed eglInitialize through it
    // and the process segfaults as it starts. WGPU_VALIDATION=1
    // still asks for it.
    if std::env::var_os("WGPU_VALIDATION").is_none() {
        new.instance_descriptor
            .flags
            .remove(wgpu::InstanceFlags::VALIDATION);
    }
    new
}

/// Gives `painter` the window `window`, making the painter (and with the
/// first, the device) if there is none yet. `Ok(Some(why))`: GL was asked
/// for and could not be had, and wgpu draws.
pub(crate) fn attach(
    painter: &mut Option<Painter>,
    ctx: &egui::Context,
    gpu: &mut Option<Gpu>,
    display: OwnedDisplayHandle,
    window: &Arc<Window>,
    choice: Choice,
) -> Result<Option<String>, String> {
    if let Some(painter) = painter {
        return pollster::block_on(painter.set_window(window)).map(|()| None);
    }
    let mut why = None;
    #[cfg(gl_painter)]
    {
        let gl = match gpu {
            Some(Gpu::Gl(gl)) => Some(Ok(std::rc::Rc::clone(gl))),
            None if choice == Choice::Gl => Some(Gl::new(&display, window)),
            _ => None,
        };
        if let Some(gl) = gl {
            let made = gl.and_then(|gl| {
                let mut painter = GlPainter::new(gl)?;
                painter.set_window(window)?;
                Ok(painter)
            });
            match made {
                Ok(made) => {
                    let made = Painter::Gl(made);
                    if gpu.is_none() {
                        *gpu = Gpu::of(&made);
                    }
                    *painter = Some(made);
                    return Ok(None);
                }
                // Only the first window picks; a later one that cannot
                // have GL is drawn by wgpu on its own device.
                Err(e) => why = Some(e),
            }
        }
    }
    #[cfg(not(gl_painter))]
    if choice == Choice::Gl {
        why = Some("this platform has no EGL".to_owned());
    }
    let mut made = Painter::Wgpu(wgpu_painter(ctx, gpu.as_ref(), display));
    pollster::block_on(made.set_window(window))?;
    if gpu.is_none() {
        *gpu = Gpu::of(&made);
    }
    *painter = Some(made);
    Ok(why)
}

/// A wgpu painter for the windows of `ctx`. The first one (`gpu` is
/// `None`) picks the device when it is given its window.
fn wgpu_painter(
    ctx: &egui::Context,
    gpu: Option<&Gpu>,
    display: OwnedDisplayHandle,
) -> egui_wgpu::winit::Painter {
    let wgpu_setup = match gpu {
        Some(Gpu::Wgpu {
            instance,
            adapter,
            device,
            queue,
        }) => WgpuSetup::Existing(WgpuSetupExisting {
            instance: instance.clone(),
            adapter: adapter.clone(),
            device: device.clone(),
            queue: queue.clone(),
        }),
        _ => WgpuSetup::CreateNew(create_new(display)),
    };
    let config = WgpuConfiguration {
        surface: SurfaceConfig {
            // ARCHITECTURE.md §7.2 step 12: a frame waits for the display
            // and none is torn or dropped. It is also what keeps a window
            // that animates from drawing more often than it is shown.
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: Some(2),
        },
        wgpu_setup,
        ..WgpuConfiguration::default()
    };
    let options = RendererOptions {
        // B0 antialiases its own edges (feathering).
        msaa_samples: 1,
        dithering: DITHER,
        ..RendererOptions::default()
    };
    // A surface that can be seen through, for every window: whether a
    // window is, is said when the window is opened (and an opaque one
    // clears to an opaque colour), while the painter outlives it.
    let see_through = true;
    pollster::block_on(egui_wgpu::winit::Painter::new(
        ctx.clone(),
        config,
        see_through,
        options,
    ))
}

/// An open platform window and what egui needs to be shown in it.
pub(crate) struct Surface {
    pub(crate) id: WindowId,
    pub(crate) window: Arc<Window>,
    /// egui's side of the window: input, the input method, the clipboard
    /// and the AccessKit adapter.
    pub(crate) state: egui_winit::State,
    pub(crate) info: egui::ViewportInfo,
    /// Opened unseen; shown once there is a picture in it.
    pub(crate) shown: bool,
    /// The size in physical pixels the painter was last told.
    px: Option<[u32; 2]>,
}

impl std::fmt::Debug for Surface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Surface")
            .field("id", &self.id)
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl Surface {
    pub(crate) fn new(id: WindowId, window: Arc<Window>, state: egui_winit::State) -> Self {
        Self {
            id,
            window,
            state,
            info: egui::ViewportInfo::default(),
            shown: false,
            px: None,
        }
    }

    /// The window's size in points of `ctx`, and the painter is told when
    /// the size in pixels is another than it knows. `None` while the
    /// window has no area (minimized on some platforms).
    pub(crate) fn fit(&mut self, ctx: &egui::Context, painter: &mut Painter) -> Option<[f64; 2]> {
        let px = self.window.inner_size();
        let (w, h) = (NonZeroU32::new(px.width)?, NonZeroU32::new(px.height)?);
        if self.px != Some([px.width, px.height]) {
            self.px = Some([px.width, px.height]);
            painter.resized(w, h);
        }
        let ppp = f64::from(egui_winit::pixels_per_point(ctx, &self.window));
        Some([f64::from(px.width) / ppp, f64::from(px.height) / ppp])
    }

    /// How the platform shows the window now; `known` is how the shell
    /// last knew it to be.
    pub(crate) fn shown_as(&self, known: Option<WindowState>) -> WindowState {
        if self.window.fullscreen().is_some() {
            WindowState::FullScreen
        } else if self.window.is_minimized() == Some(true) {
            WindowState::Minimized
        } else if maximized(
            cfg!(target_os = "macos") && !self.window.is_decorated(),
            known,
            || self.window.is_maximized(),
        ) {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
    }
}

/// Whether a window is maximized. AppKit answers only for a window with a
/// title bar, so winit gives a window without one a title bar for the
/// question and takes it away again; the change ends its user's drag of
/// an edge after its first step and cuts the window to the screen's
/// visible frame (#133). Such a window has no zoom button: it is
/// maximized when the application asked, and the platform is not asked.
fn maximized(untitled_mac: bool, known: Option<WindowState>, ask: impl FnOnce() -> bool) -> bool {
    if untitled_mac {
        known == Some(WindowState::Maximized)
    } else {
        ask()
    }
}

/// Drops from `pending` the pictures that were made and thrown away
/// again before any window could be given them. A window that is not on
/// the screen keeps what it would have to upload; without this an
/// application that never shows it would keep everything it ever drew.
pub(crate) fn forget_freed(pending: &mut egui::TexturesDelta) {
    if pending.free.is_empty() {
        return;
    }
    let free = &pending.free;
    // The frees stay: the painter may hold an earlier upload of them.
    pending.set.retain(|id, _| !free.contains(id));
}

/// Hands the pictures `pending` sets to the GPU, the first thing
/// egui-wgpu's paint would do, so that it is timed apart from the
/// drawing; the paint then finds nothing left to upload. A frame with no
/// new picture costs nothing here.
fn wgpu_upload(
    painter: &egui_wgpu::winit::Painter,
    pending: &mut egui::TexturesDelta,
) -> Option<std::time::Duration> {
    if pending.set.is_empty() {
        return None;
    }
    let state = painter.render_state()?;
    let started = std::time::Instant::now();
    let mut renderer = state.renderer.write();
    for (id, deltas) in pending.set.drain() {
        for delta in deltas {
            renderer.update_texture(&state.device, &state.queue, id, &delta);
        }
    }
    Some(started.elapsed())
}

/// Gives the GPU memory of what `pending` frees back now, for a window
/// that is not on the screen: its next picture may be far off, and a
/// trimmed view frees most of what it drew (ARCHITECTURE §8.5).
pub(crate) fn free_now(painter: Option<&mut Painter>, pending: &mut egui::TexturesDelta) {
    if pending.free.is_empty() {
        return;
    }
    match painter {
        Some(Painter::Wgpu(painter)) => {
            let Some(state) = painter.render_state() else {
                return;
            };
            let mut renderer = state.renderer.write();
            for id in std::mem::take(&mut pending.free) {
                renderer.free_texture(&id);
            }
        }
        #[cfg(gl_painter)]
        Some(Painter::Gl(painter)) => painter.free(pending),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use egui::epaint::ImageDelta;
    use egui::{ColorImage, TextureId, TextureOptions};

    use super::*;

    #[test]
    fn the_painter_is_gl_where_there_is_egl_unless_asked_otherwise() {
        let default = if cfg!(gl_painter) {
            Choice::Gl
        } else {
            Choice::Wgpu
        };
        assert_eq!(Choice::from_env(None), default);
        assert_eq!(Choice::from_env(Some("")), default);
        assert_eq!(Choice::from_env(Some("vulkan")), default);
        assert_eq!(Choice::from_env(Some("wgpu")), Choice::Wgpu);
        assert_eq!(Choice::from_env(Some(" WGPU ")), Choice::Wgpu);
        assert_eq!(Choice::from_env(Some("gl")), Choice::Gl);
    }

    #[test]
    fn a_mac_window_without_a_title_bar_is_not_asked_whether_it_is_maximized() {
        let asked = std::cell::Cell::new(false);
        let ask = || {
            asked.set(true);
            true
        };
        assert!(!maximized(true, Some(WindowState::Normal), ask));
        assert!(maximized(true, Some(WindowState::Maximized), ask));
        assert!(!maximized(true, None, ask));
        assert!(!asked.get());
        assert!(maximized(false, Some(WindowState::Normal), ask));
        assert!(asked.get());
    }

    fn picture() -> ImageDelta {
        ImageDelta::full(
            ColorImage::filled([1, 1], egui::Color32::WHITE),
            TextureOptions::LINEAR,
        )
    }

    #[test]
    fn a_picture_thrown_away_before_it_was_shown_is_not_kept() {
        let mut pending = egui::TexturesDelta::default();
        pending.push(TextureId::Managed(1), picture());
        pending.push(TextureId::Managed(2), picture());
        pending.free(TextureId::Managed(1));
        forget_freed(&mut pending);
        let kept: Vec<_> = pending.set.keys().copied().collect();
        assert_eq!(kept, vec![TextureId::Managed(2)]);
        assert!(
            pending.free.contains(&TextureId::Managed(1)),
            "the painter may have an earlier upload to drop"
        );
        pending.clear();
    }

    #[test]
    fn a_wgpu_window_whose_picture_did_not_change_is_not_presented_again() {
        let mut pending = egui::TexturesDelta::default();
        let frame = wgpu_frame(Change::None, &pending);
        assert_eq!(frame, Frame::Nothing);
        assert!(
            !wgpu_presents(&frame),
            "egui-wgpu would show only the clear colour"
        );

        pending.push(TextureId::Managed(1), picture());
        let frame = wgpu_frame(Change::None, &pending);
        assert!(wgpu_presents(&frame), "a picture waits to be uploaded");
        pending.clear();
    }

    #[test]
    fn pictures_still_in_use_are_all_kept() {
        let mut pending = egui::TexturesDelta::default();
        pending.push(TextureId::Managed(1), picture());
        forget_freed(&mut pending);
        assert_eq!(pending.set.len(), 1);
        pending.clear();
    }
}
