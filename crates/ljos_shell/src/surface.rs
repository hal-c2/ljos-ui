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

use crate::present::{Change, Frame};
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
    pub(crate) fn prepare(&self, shapes: &mut [egui::epaint::ClippedShape], pixels_per_point: f32) {
        match self {
            // Shader callbacks never reach it (`draws_shaders`).
            Painter::Wgpu(_) => {}
            #[cfg(gl_painter)]
            Painter::Gl(painter) => {
                crate::gl_shader::programs(painter.programs(), shapes);
                crate::gl::fill_rects(shapes, pixels_per_point);
            }
        }
        #[cfg(not(gl_painter))]
        let _ = (shapes, pixels_per_point);
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
            Painter::Gl(painter) => painter.frame(change, pending, pixels_per_point),
        }
    }

    /// Draws `primitives` (all of `frame`) into `window`, with the
    /// pictures `pending` hands over, and shows them.
    pub(crate) fn paint(
        &mut self,
        pixels_per_point: f32,
        clear: egui::Color32,
        frame: &Frame,
        primitives: &[egui::ClippedPrimitive],
        pending: &mut egui::TexturesDelta,
        window: &Arc<Window>,
    ) {
        match self {
            Painter::Wgpu(painter) => {
                if !wgpu_presents(frame) {
                    return;
                }
                painter.paint_and_update_textures(
                    ViewportId::ROOT,
                    pixels_per_point,
                    clear.to_normalized_gamma_f32(),
                    primitives,
                    pending,
                    Vec::new(),
                    window,
                );
            }
            #[cfg(gl_painter)]
            Painter::Gl(painter) => {
                painter.paint(pixels_per_point, clear, frame, primitives, pending, window);
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

    /// How the platform shows the window now.
    pub(crate) fn shown_as(&self) -> WindowState {
        if self.window.fullscreen().is_some() {
            WindowState::FullScreen
        } else if self.window.is_minimized() == Some(true) {
            WindowState::Minimized
        } else if self.window.is_maximized() {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
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
