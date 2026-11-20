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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

pub use android_activity::AndroidApp;
use egui_wgpu::wgpu;
use ljos_egui::ResizeMode;
use ljos_engine::ObjectId;
use ljos_host::android::{AndroidHost, Attach};
use ljos_host::relay::{RelayControl, Request, WINDOW};
use ljos_host::{
    AppState, BackPhase, ButtonState, Event, HostApp, HostControl, Insets, Key, PlatformHost,
    TextEvent, TouchPhase, Wake, WindowAttributes, WindowEvent,
};
use ljos_runtime::SystemClock;
use ndk::native_window::NativeWindow;

use crate::desktop::use_fonts;
use crate::platform::Display;
use crate::soft_keyboard::SoftKeyboard;
use crate::wake::{Next, next_wake, posted_met};
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

/// The application asked to quit: its loop is ending, and has asked the
/// activity to finish.
static QUITTING: AtomicBool = AtomicBool::new(false);

/// The application's thread, joined before a quit ends the process.
static APPLICATION: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

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
        let quit = attach.clone();
        let spawned = std::thread::Builder::new()
            .name(String::from("ljos-application"))
            .spawn(move || {
                let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let (config, setup) = start();
                    let ui = Phone::new(config, setup, host.waker());
                    host.run(ui)
                }));
                match ran {
                    // The activity was asked to finish: the process ends on
                    // the activity's thread once it has (see `main`). With
                    // none attached (a quit after Back, from a timer, say)
                    // nothing would take that ask: the process ends here,
                    // not at the next launch, which would open and close.
                    Ok(Ok(())) => {
                        QUITTING.store(true, Ordering::Release);
                        quit.when_detached(|| {
                            end_process(0);
                        });
                    }
                    Ok(Err(e)) => {
                        log::error!("the application's loop ended: {e}");
                        end_process(1);
                    }
                    // The hook logged why.
                    Err(_) => end_process(1),
                }
            });
        match spawned {
            Ok(thread) => *crate::lock(&APPLICATION) = Some(thread),
            Err(e) => {
                log::error!("the application's thread did not start: {e}");
                end_process(1);
            }
        }
        attach
    });
    // Before anything else here: a quit from now on is this activity's to
    // finish, not one to end the process under it.
    attach.arriving();
    attach.pump(&app);
    // Not from the application's thread as its loop ends: the activity it
    // asked to finish would still be in front, and the process's exit
    // would tear down what the activity's threads are still using. Here
    // the activity was destroyed (the pump waits for that once it has
    // asked for a finish) and its thread is back in `android_main`.
    if QUITTING.load(Ordering::Acquire) {
        if let Some(thread) = crate::lock(&APPLICATION).take() {
            let _ = thread.join();
        }
        end_process(0);
    }
}

/// The process ends with its application. Android keeps a process it
/// cached after its last activity finished, and `ATTACH` holds the
/// application of the process: an activity the launcher started in it
/// later would attach to an application that is gone and close at once,
/// until the system killed the process. Ended, the next start is a new
/// process with a new application, as after the system ended it.
fn end_process(code: i32) -> ! {
    log::info!("the application ended; so does its process");
    std::process::exit(code)
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
    /// What of the screen the system keeps, in physical pixels.
    insets: Insets,
    /// What the activity was last told about Back: whether the
    /// application takes it.
    back_taken: crate::back_told::BackTold,
    /// The on-screen keyboard and its input method.
    keyboard: SoftKeyboard,
    /// Frames tried again since one was last drawn.
    retries: u32,
}

/// How long after a frame that could not be drawn it is tried again, and
/// how many times.
const RETRY: std::time::Duration = std::time::Duration::from_millis(16);
const RETRIES: u32 = 8;

/// What became of a frame.
enum Painted {
    Drawn,
    /// The window is hidden.
    Skipped,
    /// None could be had now; another may soon.
    Again,
    /// The surface is gone and is made again.
    Lost,
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
            insets: Insets::default(),
            back_taken: crate::back_told::BackTold::default(),
            keyboard: SoftKeyboard::default(),
            retries: 0,
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
                // Content is thrown and stopped as Android's own lists do.
                ljos_quick::flickable::set_scroll_physics(
                    app.engine(),
                    ljos_quick::flickable::ScrollPhysics::Kinetic(
                        ljos_quick::flickable::Kinetic::ANDROID,
                    ),
                );
                self.stage = Stage::Running(Box::new(app));
                self.apply_insets();
            }
            Err(e) => log::error!("the application did not start: {e}"),
        }
    }

    fn surface_created(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        let Some(window) = host.surface().cloned() else {
            return;
        };
        // The activity may be a new one, which takes Back until told.
        self.back_taken.surface_came();
        // And its input method knows no editor until told.
        self.keyboard.surface_changed();
        if let Err(e) = self.make_surface(&window) {
            log::error!("nothing can be drawn: {e}");
            return;
        }
        self.retries = 0;
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

    /// Tells the scene what of the screen it keeps clear: the safe area,
    /// and the keyboard, which it gives up (see [`Phone::draw`]).
    fn apply_insets(&self) {
        let Stage::Running(app) = &self.stage else {
            return;
        };
        let scale = if self.scale > 0.0 {
            f64::from(self.scale)
        } else {
            1.0
        };
        let [l, t, r, b] = self.insets.safe_area().map(|n| n / scale);
        let engine = app.engine();
        ljos_quick::set_safe_area_insets(engine, l, t, r, b);
        let keyboard = self.insets.keyboard() / scale;
        let covers = (keyboard > 0.0).then(|| {
            let w = f64::from(self.px[0]) / scale;
            let h = f64::from(self.px[1]) / scale;
            ljos_render::kurbo::Rect::new(0.0, (h - keyboard).max(0.0), w, h)
        });
        ljos_quick::set_on_screen_keyboard(engine, covers);
    }

    /// The system's Back, to the application's `Back` handlers; what none
    /// takes is the system's.
    fn back(&mut self, host: &mut RelayControl<'_, NativeWindow>, phase: BackPhase, progress: f32) {
        let taken = match &self.stage {
            Stage::Running(app) => {
                let step = match phase {
                    BackPhase::Started => ljos_quick::BackStep::Started,
                    BackPhase::Progressed => ljos_quick::BackStep::Progressed,
                    BackPhase::Cancelled => ljos_quick::BackStep::Cancelled,
                    BackPhase::Invoked => ljos_quick::BackStep::Invoked,
                };
                ljos_quick::deliver_back(app.engine(), step, f64::from(progress))
            }
            _ => false,
        };
        if !taken && phase == BackPhase::Invoked {
            // As a phone's own applications do, the application goes to
            // the background and lives on.
            host.request(Request::Background);
        }
    }

    /// Tells the activity whether the application takes Back now, when
    /// that changed, so the system plays its own animation for a Back
    /// that is its own.
    fn tell_back_taken(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        let Stage::Running(app) = &self.stage else {
            return;
        };
        let taken = ljos_quick::wants_back(app.engine());
        if let Some(taken) = self.back_taken.tell(taken) {
            host.request(Request::BackTaken(taken));
        }
    }

    /// Tells the input method what the scene's editors want of it: the
    /// keyboard for the editor with focus, and what it is to hold.
    fn tell_keyboard(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        let Stage::Running(app) = &self.stage else {
            return;
        };
        for request in self.keyboard.sync(app.view().scene(), app.engine()) {
            host.request(request);
        }
    }

    /// What the input method did to the editor with focus.
    fn text(&mut self, event: TextEvent) {
        match event {
            TextEvent::Held(now) => {
                if let Stage::Running(app) = &self.stage {
                    self.keyboard.held(app.view().scene(), app.engine(), now);
                }
            }
            // The keyboard's action key is the editor's Enter: Done ends
            // a line edit as Enter does (`accepted`, `editingFinished`).
            TextEvent::Action => {
                for pressed in [true, false] {
                    self.events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            _ => {}
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
                    Key::Character(c) => {
                        // A hardware keyboard's printable key, as a key and
                        // not only as the text it types: what reads keys
                        // (a shortcut, Keys.onPressed) sees it pressed and
                        // let go.
                        if let Some(k) = egui::Key::from_name(&c.to_uppercase()) {
                            self.events.push(egui::Event::Key {
                                key: k,
                                physical_key: None,
                                pressed,
                                repeat: key.repeat,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                    }
                    Key::Unidentified => {}
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
    fn draw(&mut self, host: &mut RelayControl<'_, NativeWindow>) {
        // Met by this turn even when nothing can be drawn: a surface that
        // comes back asks for a pass of its own.
        posted_met(&self.due, Instant::now());
        let Stage::Running(app) = &mut self.stage else {
            return;
        };
        let (Some(gpu), Some(surface)) = (&mut self.gpu, &self.surface) else {
            return;
        };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        // The keyboard's part of the screen is given up, as a window that
        // resizes for it (`adjustResize`) gives it up: what is at the
        // bottom stays in sight above it.
        #[allow(clippy::cast_possible_truncation)]
        let keyboard = self.insets.keyboard() as f32 / scale;
        #[allow(clippy::cast_precision_loss)]
        let size = egui::vec2(
            self.px[0] as f32 / scale,
            (self.px[1] as f32 / scale - keyboard).max(1.0),
        );
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
        let painted = gpu.paint(
            surface,
            &jobs,
            &output.textures_delta,
            output.pixels_per_point,
        );
        match painted {
            Painted::Drawn => {
                self.retries = 0;
                ljos_quick::ImageStore::of(&engine).frame_drawn(&engine);
            }
            // Hidden: drawn again when it shows, which asks for a pass.
            Painted::Skipped => {}
            Painted::Again => self.again(began),
            Painted::Lost => {
                self.surface = None;
                if let Some(window) = host.surface().cloned() {
                    match self.make_surface(&window) {
                        Ok(()) => self.again(began),
                        Err(e) => log::error!("the surface was lost and not made again: {e}"),
                    }
                }
            }
        }
        for command in output.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(open) = command {
                log::info!("open {}", open.url);
            }
        }
    }

    /// What the scene changed is not on the screen yet: another pass is
    /// asked for soon, a few times, and then only when the scene asks.
    fn again(&mut self, began: Instant) {
        if self.retries < RETRIES {
            self.retries += 1;
            let at = began + RETRY;
            self.asked = Some(self.asked.map_or(at, |asked| asked.min(at)));
        } else {
            log::warn!("the screen was not drawn after {RETRIES} tries");
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
        match next_wake(self.asked, &self.due, Instant::now()) {
            Next::Now => host.request_redraw(WINDOW),
            Next::At(at) => host.wake_at(at),
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
                    self.apply_insets();
                    host.request_redraw(WINDOW);
                }
                WindowEvent::RedrawRequested => self.draw(host),
                WindowEvent::Insets(insets) => {
                    self.insets = insets;
                    self.apply_insets();
                    host.request_redraw(WINDOW);
                }
                WindowEvent::Back { phase, progress } => {
                    self.back(host, phase, progress);
                    host.request_redraw(WINDOW);
                }
                WindowEvent::Theme { .. } => host.request_redraw(WINDOW),
                WindowEvent::Text(text) => {
                    self.text(text);
                    host.request_redraw(WINDOW);
                }
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
                    // Before the activity is asked to finish, so that it
                    // is known once it has.
                    QUITTING.store(true, Ordering::Release);
                    host.exit();
                } else {
                    self.tell_back_taken(host);
                    self.tell_keyboard(host);
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
    ) -> Painted {
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
                for id in &textures.free {
                    self.renderer.free_texture(id);
                }
                return match other {
                    wgpu::CurrentSurfaceTexture::Occluded => Painted::Skipped,
                    wgpu::CurrentSurfaceTexture::Lost => Painted::Lost,
                    wgpu::CurrentSurfaceTexture::Timeout => Painted::Again,
                    other => {
                        log::warn!("no frame to draw into: {other:?}");
                        surface.surface.configure(&self.device, &surface.config);
                        Painted::Again
                    }
                };
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
        Painted::Drawn
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
