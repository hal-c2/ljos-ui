//! The shell on Android (ARCHITECTURE.md §10.2): the application core
//! (engine, scripts, models) runs on a thread of its own for the life of
//! the process, and each activity only lends it a surface. The UI host
//! keeps the GPU device and egui's textures across activities and makes
//! the surface again on each [`Event::SurfaceCreated`]; a rotation or a
//! trip to the home screen redraws the scene that was there, it does not
//! build it again.
//!
//! The QML root fills the screen, as on any phone: there is one window.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub use android_activity::AndroidApp;
use egui_wgpu::wgpu;
use ljos_egui::ResizeMode;
use ljos_engine::ObjectId;
use ljos_host::android::{AndroidHost, Attach};
use ljos_host::relay::{RelayControl, Request, WINDOW};
use ljos_host::{
    AppState, ButtonState, Event, HostApp, HostControl, Key, PlatformHost, TouchPhase, Wake,
    WindowAttributes, WindowEvent,
};
use ljos_runtime::SystemClock;
use ndk::native_window::NativeWindow;

use crate::desktop::{HEARTBEAT, use_fonts};
use crate::platform::Display;
use crate::{Hosting, Setup, SetupError, ShellApp, ShellConfig};

/// What reaches the application's thread besides the activity's events.
#[derive(Debug)]
pub enum Message {
    Wake,
}

impl From<Wake> for Message {
    fn from(_: Wake) -> Self {
        Self::Wake
    }
}

/// One line the application logged, to logcat (`adb logcat -s ljos`).
pub fn log_entry(entry: &ljos_engine::LogEntry) {
    let level = match entry.level {
        ljos_engine::LogLevel::Debug => log::Level::Debug,
        ljos_engine::LogLevel::Info => log::Level::Info,
        ljos_engine::LogLevel::Warning => log::Level::Warn,
        ljos_engine::LogLevel::Critical => log::Level::Error,
    };
    log::log!(level, "{entry}");
}

/// The application of the process, attached to by every activity.
static ATTACH: OnceLock<Attach<Message>> = OnceLock::new();

thread_local! {
    static IN_PASS: Cell<bool> = const { Cell::new(false) };
}

/// Runs the application in an activity: call it from `android_main`, and
/// return after it. The first activity of the process starts the
/// application, with what `start` gives; each later one, after a
/// rotation the manifest does not take, Back, or the system ending a hidden
/// activity, finds it running and only gives it a surface again.
pub fn main<M, S>(app: AndroidApp, start: impl FnOnce() -> (ShellConfig, S) + Send + 'static)
where
    M: Send + 'static,
    S: FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
{
    let attach = ATTACH.get_or_init(|| {
        // What the application logs, and why it panicked, go to logcat:
        // an application's stderr goes nowhere on Android.
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("ljos"),
        );
        let panicked = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("{info}");
            panicked(info);
        }));
        let (host, attach) = ljos_host::android::host::<Message>();
        let spawned = std::thread::Builder::new()
            .name(String::from("ljos-application"))
            .spawn(move || {
                let (config, setup) = start();
                let ui = Phone::new(config, setup, host.waker());
                if let Err(e) = host.run(ui) {
                    log::error!("the application's loop ended: {e}");
                }
            });
        if let Err(e) = spawned {
            log::error!("the application's thread did not start: {e}");
        }
        attach
    });
    attach.pump(&app);
}

/// What draws: kept from one activity to the next.
struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    format: wgpu::TextureFormat,
}

/// The surface of one activity's window.
struct Surface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

/// The application before its first surface, and after.
enum Stage<M, S> {
    Waiting(ShellConfig, S),
    Running(Box<ShellApp<M>>),
    /// Its setup failed; it says so and shows nothing.
    Failed,
}

struct Phone<M, S> {
    stage: Stage<M, S>,
    ctx: egui::Context,
    /// The time egui is told is measured from here.
    started: Instant,
    gpu: Option<Gpu>,
    surface: Option<Surface>,
    px: [u32; 2],
    scale: f32,
    events: Vec<egui::Event>,
    /// The finger egui sees as its pointer, while it is down.
    pointer: Option<u64>,
    /// When the scene asked for its next pass.
    asked: Option<Instant>,
    /// What other threads and egui asked for, for the next turn.
    due: Arc<Mutex<Option<Instant>>>,
    waker: ljos_host::Waker,
    focused: bool,
}

impl<M, S> Phone<M, S>
where
    M: Send + 'static,
    S: FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
{
    fn new(config: ShellConfig, setup: S, waker: ljos_host::Waker) -> Self {
        use_fonts(&config, None);
        let due = Arc::new(Mutex::new(None::<Instant>));
        let ctx = egui::Context::default();
        ctx.set_fonts(ljos_egui::font_definitions());
        ctx.options_mut(|o| o.max_passes = NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
        {
            let due = Arc::clone(&due);
            let waker = waker.clone();
            ctx.set_request_repaint_callback(move |info| {
                if IN_PASS.get() {
                    return;
                }
                if let Some(at) = Instant::now().checked_add(info.delay) {
                    ask(&due, &waker, at);
                }
            });
        }
        Self {
            stage: Stage::Waiting(config, setup),
            ctx,
            started: Instant::now(),
            gpu: None,
            surface: None,
            px: [0, 0],
            scale: 1.0,
            events: Vec::new(),
            pointer: None,
            asked: None,
            due,
            waker,
            focused: true,
        }
    }

    /// Builds the application at its first surface, when its size is
    /// known.
    fn start(&mut self) {
        let Stage::Waiting(..) = self.stage else {
            return;
        };
        let Stage::Waiting(config, setup) = std::mem::replace(&mut self.stage, Stage::Failed)
        else {
            return;
        };
        let wake: ljos_egui::Waker = {
            let due = Arc::clone(&self.due);
            let waker = self.waker.clone();
            Arc::new(move |delay| {
                if let Some(at) = Instant::now().checked_add(delay) {
                    ask(&due, &waker, at);
                }
            })
        };
        let hosting = Hosting {
            tree: None,
            changed: None,
            wake,
            display: Display::Other,
            screens: Vec::new(),
        };
        match ShellApp::hosted(
            &self.ctx,
            &config,
            Arc::new(SystemClock::new()),
            setup,
            hosting,
        ) {
            Ok(mut app) => {
                app.view_mut().set_resize_mode(ResizeMode::RootToView);
                let opaque = config.background.to_rgba8()[3] == 255;
                app.set_backdrop(!opaque);
                app.view_mut().set_fold_backdrop(opaque);
                self.stage = Stage::Running(Box::new(app));
            }
            Err(e) => log::error!("the application did not start: {e}"),
        }
    }

    fn surface_created(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        let Some(window) = host.surface().cloned() else {
            return;
        };
        if let Err(e) = self.make_surface(&window) {
            log::error!("nothing can be drawn: {e}");
            return;
        }
        if let Stage::Waiting(..) = self.stage {
            // The one window is the screen; it comes with the first surface.
            if let Err(e) = host.create_window(&WindowAttributes::default()) {
                log::error!("{e}");
            }
            self.start();
        }
        host.request_redraw(WINDOW);
    }

    fn make_surface(&mut self, window: &NativeWindow) -> Result<(), String> {
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(window)?);
        }
        let Some(gpu) = &self.gpu else {
            return Err(String::from("no device"));
        };
        let surface = gpu.surface_for(window)?;
        let px = [
            u32::try_from(window.width()).unwrap_or(1).max(1),
            u32::try_from(window.height()).unwrap_or(1).max(1),
        ];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: px[0],
            height: px[1],
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
        };
        surface.configure(&gpu.device, &config);
        self.surface = Some(Surface { surface, config });
        Ok(())
    }

    fn resized(&mut self, px: [u32; 2], scale: f64) {
        self.px = px;
        #[allow(clippy::cast_possible_truncation)]
        {
            self.scale = scale as f32;
        }
        if let (Some(gpu), Some(surface)) = (&self.gpu, &mut self.surface) {
            surface.config.width = px[0].max(1);
            surface.config.height = px[1].max(1);
            surface.surface.configure(&gpu.device, &surface.config);
        }
    }

    fn input(&mut self, event: WindowEvent) {
        let points = |p: [f64; 2], scale: f32| {
            #[allow(clippy::cast_possible_truncation)]
            egui::pos2(p[0] as f32 / scale, p[1] as f32 / scale)
        };
        match event {
            WindowEvent::Touch(touch) => {
                let pos = points(touch.pos, self.scale);
                let phase = match touch.phase {
                    TouchPhase::Started => egui::TouchPhase::Start,
                    TouchPhase::Moved => egui::TouchPhase::Move,
                    TouchPhase::Ended => egui::TouchPhase::End,
                    TouchPhase::Cancelled => egui::TouchPhase::Cancel,
                };
                #[allow(clippy::cast_possible_truncation)]
                self.events.push(egui::Event::Touch {
                    device_id: egui::TouchDeviceId(0),
                    id: egui::TouchId(touch.id),
                    phase,
                    pos,
                    force: touch.force.map(|f| f as f32),
                });
                // The first finger down is the pointer, as egui's own hosts
                // make it.
                match touch.phase {
                    TouchPhase::Started if self.pointer.is_none() => {
                        self.pointer = Some(touch.id);
                        self.events.push(egui::Event::PointerMoved(pos));
                        self.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    TouchPhase::Moved if self.pointer == Some(touch.id) => {
                        self.events.push(egui::Event::PointerMoved(pos));
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled if self.pointer == Some(touch.id) => {
                        self.pointer = None;
                        if touch.phase == TouchPhase::Ended {
                            self.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                        self.events.push(egui::Event::PointerGone);
                    }
                    _ => {}
                }
            }
            WindowEvent::Key(key) => {
                let pressed = key.state == ButtonState::Pressed;
                match &key.key {
                    Key::Named(name) => {
                        if let Some(k) = egui::Key::from_name(name) {
                            self.events.push(egui::Event::Key {
                                key: k,
                                physical_key: None,
                                pressed,
                                repeat: key.repeat,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                    }
                    Key::Character(_) | Key::Unidentified => {}
                }
                if pressed && let Some(text) = key.text {
                    self.events.push(egui::Event::Text(text));
                }
            }
            WindowEvent::Focus(on) => {
                self.focused = on;
                self.events.push(egui::Event::WindowFocused(on));
            }
            _ => {}
        }
    }

    /// One pass of the scene, drawn into the surface.
    fn draw(&mut self) {
        let Stage::Running(app) = &mut self.stage else {
            return;
        };
        let (Some(gpu), Some(surface)) = (&mut self.gpu, &self.surface) else {
            return;
        };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        #[allow(clippy::cast_precision_loss)]
        let size = egui::vec2(self.px[0] as f32 / scale, self.px[1] as f32 / scale);
        let began = Instant::now();
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: self.focused,
            time: Some(began.duration_since(self.started).as_secs_f64()),
            events: std::mem::take(&mut self.events),
            ..egui::RawInput::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(scale);
        IN_PASS.set(true);
        let output = self.ctx.run_ui(raw, |ui| {
            app.begin_pass();
            app.show(ui);
        });
        IN_PASS.set(false);
        self.asked = app.view_mut().take_asked().map(|after| began + after);
        let engine = Rc::clone(app.engine());
        let jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        gpu.paint(
            surface,
            &jobs,
            &output.textures_delta,
            output.pixels_per_point,
        );
        ljos_quick::ImageStore::of(&engine).frame_drawn(&engine);
        for command in output.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(open) = command {
                log::info!("open {}", open.url);
            }
        }
    }

    fn quit_requested(&self) -> bool {
        match &self.stage {
            Stage::Running(app) => app.engine().quit_requested(),
            _ => false,
        }
    }

    /// When the loop wakes by itself next: the scene's next frame, or
    /// what another thread asked for.
    fn next_wake(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        let now = Instant::now();
        let posted = crate::lock(&self.due).take();
        let due = match (self.asked, posted) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match due {
            Some(at) if at <= now + HEARTBEAT / 4 => host.request_redraw(WINDOW),
            other => host.wake_at(other),
        }
    }
}

/// Asks for a turn at `at` unless one is due earlier already.
fn ask(due: &Mutex<Option<Instant>>, waker: &ljos_host::Waker, at: Instant) {
    let mut due = crate::lock(due);
    if due.is_none_or(|d| at < d) {
        *due = Some(at);
        drop(due);
        waker.wake();
    }
}

impl<M, S> HostApp<AndroidHost<Message>> for Phone<M, S>
where
    M: Send + 'static,
    S: FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
{
    fn event(&mut self, host: &mut RelayControl<'_, NativeWindow>, event: Event<'_, (), Message>) {
        match event {
            Event::SurfaceCreated => self.surface_created(host),
            // The window goes back to the system when this returns.
            Event::SurfaceDestroyed => self.surface = None,
            Event::Window { event, .. } => match event {
                WindowEvent::Resized { px, scale } => {
                    self.resized(px, scale);
                    host.request_redraw(WINDOW);
                }
                WindowEvent::RedrawRequested => self.draw(),
                WindowEvent::Back { .. } => {
                    // Nothing in the scene takes Back yet: as a phone's
                    // own applications do, the application goes to the
                    // background and lives on.
                    host.request(Request::Background);
                }
                WindowEvent::Theme { .. } => host.request_redraw(WINDOW),
                other => {
                    self.input(other);
                    host.request_redraw(WINDOW);
                }
            },
            Event::AppState(AppState::Active) => host.request_redraw(WINDOW),
            Event::AppState(_) => {}
            Event::MemoryPressure => {
                if let Stage::Running(app) = &self.stage {
                    app.engine().collect_if_idle();
                }
                crate::give_back_freed_heap();
            }
            Event::Wake | Event::User(Message::Wake) => host.request_redraw(WINDOW),
            Event::AboutToWait => {
                if self.quit_requested() {
                    host.exit();
                } else {
                    self.next_wake(host);
                }
            }
            _ => {}
        }
    }
}

impl Gpu {
    fn new(window: &NativeWindow) -> Result<Self, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if std::env::var_os("WGPU_BACKEND").is_none() {
            descriptor.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
        }
        let instance = wgpu::Instance::new(descriptor);
        let probe = surface_of(&instance, window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&probe),
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no adapter draws into the window: {e}"))?;
        let caps = probe.get_capabilities(&adapter);
        drop(probe);
        let format = egui_wgpu::preferred_framebuffer_format(&caps.formats)
            .map_err(|e| format!("the window takes no format egui draws: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ljos"),
            required_limits:
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|e| format!("the adapter gave no device: {e}"))?;
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        log::info!(
            "drawing with {}",
            egui_wgpu::adapter_info_summary(&adapter.get_info())
        );
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            renderer,
            format,
        })
    }

    fn surface_for(&self, window: &NativeWindow) -> Result<wgpu::Surface<'static>, String> {
        let surface = surface_of(&self.instance, window)?;
        if surface
            .get_capabilities(&self.adapter)
            .formats
            .contains(&self.format)
        {
            Ok(surface)
        } else {
            Err(String::from(
                "the new window does not take the format drawn",
            ))
        }
    }

    fn paint(
        &mut self,
        surface: &Surface,
        jobs: &[egui::ClippedPrimitive],
        textures: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let frame = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                log::warn!("no frame to draw into: {other:?}");
                surface.surface.configure(&self.device, &surface.config);
                return;
            }
        };
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [surface.config.width, surface.config.height],
            pixels_per_point,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ljos"),
            });
        let mut buffers =
            self.renderer
                .update_buffers(&self.device, &self.queue, &mut encoder, jobs, &screen);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), jobs, &screen);
        }
        buffers.push(encoder.finish());
        self.queue.submit(buffers);
        self.queue.present(frame);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
    }
}

fn surface_of(
    instance: &wgpu::Instance,
    window: &NativeWindow,
) -> Result<wgpu::Surface<'static>, String> {
    use raw_window_handle::HasWindowHandle;
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(raw_window_handle::DisplayHandle::android().as_raw()),
        raw_window_handle: window.window_handle().map_err(|e| e.to_string())?.as_raw(),
    };
    // SAFETY: the window outlives the surface: the relay keeps it until
    // the application has let go of the surface (`SurfaceDestroyed`).
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
}
