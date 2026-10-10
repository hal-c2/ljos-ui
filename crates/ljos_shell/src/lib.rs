//! The desktop shell: one eframe window, one [`Engine`] with QtQuick
//! registered, a full-window [`QmlView`], and the UI inbox through which
//! other threads talk to the Engine (ARCHITECTURE 7.2 step 1, principle:
//! threads talk to the Engine through `UiHandle` only).
//!
//! ```no_run
//! use ljos_shell::{ShellConfig, run};
//! # fn build(_: &ljos_engine::Engine) -> ljos_engine::ObjectId { unimplemented!() }
//! run::<()>(ShellConfig::new("Hello"), |setup| Ok(build(setup.engine()))).unwrap();
//! ```

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_egui::{QmlView, ViewError};
use ljos_engine::{Color, Engine, ObjectId};
use ljos_quick::TimerHook;
use ljos_runtime::{Clock, DrainBudget, DrainReport, SystemClock, UiHandle, UiInbox};

/// Window and pipeline settings. Later the root `Window`/`ApplicationWindow`
/// will provide title and size; until then they come from here.
#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub title: String,
    /// Initial inner size in logical pixels.
    pub size: [f32; 2],
    pub min_size: Option<[f32; 2]>,
    /// Painted behind the root item (QQuickWindow's default is white).
    pub background: Color,
    /// Bound on queued messages; a full inbox pushes back on senders.
    pub inbox_capacity: usize,
    /// How much of a frame the inbox drain may take; the rest waits for the
    /// next frame (which the inbox requests itself).
    pub drain_budget: DrainBudget,
    /// [`run`] lays text out with the fonts installed on the system, and the
    /// platform's interface font when a `Text` names no family. False keeps
    /// the bundled font only, so text has the same size on every machine.
    ///
    /// Only [`run`] reads this. A host that builds a [`ShellApp`] itself
    /// keeps the text engine its thread already has (bundled fonts, unless
    /// it called [`ljos_quick::set_text_engine`]).
    pub system_fonts: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "Ljós".to_owned(),
            size: [800.0, 600.0],
            min_size: None,
            background: Color::WHITE,
            inbox_capacity: 1024,
            drain_budget: DrainBudget::time(Duration::from_millis(4)),
            system_fonts: true,
        }
    }
}

impl ShellConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }
}

/// What a setup closure may fail with.
pub type SetupError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("window: {0}")]
    Window(#[from] eframe::Error),
    #[error("setup: {0}")]
    Setup(SetupError),
    #[error(transparent)]
    View(#[from] ViewError),
}

type MessageHandler<M> = Box<dyn FnMut(&Engine, M)>;

/// Handed to the setup closure: the Engine to build the root in, the
/// thread-safe handle for posting messages, and hooks.
pub struct Setup<'a, M> {
    engine: &'a Rc<Engine>,
    handle: UiHandle<M>,
    clock: Arc<dyn Clock>,
    on_message: Option<MessageHandler<M>>,
    timer_hook: Option<TimerHook>,
}

impl<M> Setup<'_, M> {
    /// QtQuick is already registered.
    pub fn engine(&self) -> &Rc<Engine> {
        self.engine
    }

    /// Clone this into worker threads; every `send` wakes the UI.
    pub fn handle(&self) -> UiHandle<M> {
        self.handle.clone()
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Runs on the UI thread for every message, at the start of a frame.
    pub fn on_message(&mut self, f: impl FnMut(&Engine, M) + 'static) {
        self.on_message = Some(Box::new(f));
    }

    /// See [`ljos_quick::TimerHook`]; times come from [`Setup::clock`].
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.timer_hook = Some(hook);
    }
}

impl<M> std::fmt::Debug for Setup<'_, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("on_message", &self.on_message.is_some())
            .field("timer_hook", &self.timer_hook.is_some())
            .finish_non_exhaustive()
    }
}

/// The eframe app behind [`run`]. Public so tests (and hosts with their own
/// eframe setup) can build it from an `egui::Context`.
pub struct ShellApp<M> {
    view: QmlView,
    inbox: UiInbox<M>,
    on_message: Option<MessageHandler<M>>,
    budget: DrainBudget,
    background: egui::Color32,
    last_drain: DrainReport,
}

impl<M> std::fmt::Debug for ShellApp<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellApp")
            .field("view", &self.view)
            .field("queued", &self.inbox.len())
            .field("last_drain", &self.last_drain)
            .finish_non_exhaustive()
    }
}

impl<M: Send + 'static> ShellApp<M> {
    /// Creates the Engine, registers QtQuick, runs `setup` to build the
    /// root item and wires the inbox to wake `ctx`.
    pub fn new(
        ctx: &egui::Context,
        config: &ShellConfig,
        clock: Arc<dyn Clock>,
        setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
    ) -> Result<Self, ShellError> {
        let engine = Rc::new(Engine::new());
        ljos_quick::register(&engine);
        let wake_ctx = ctx.clone();
        let (inbox, handle) = UiInbox::channel_with_wake(
            config.inbox_capacity,
            Arc::new(move || wake_ctx.request_repaint()),
        );
        let mut s = Setup {
            engine: &engine,
            handle,
            clock: Arc::clone(&clock),
            on_message: None,
            timer_hook: None,
        };
        let root = setup(&mut s).map_err(ShellError::Setup)?;
        let Setup {
            on_message,
            timer_hook,
            ..
        } = s;
        let mut view = QmlView::new(Rc::clone(&engine), root)?.with_clock(clock);
        view.bind_context(ctx);
        if let Some(h) = timer_hook {
            view.set_timer_hook(h);
        }
        let [r, g, b, a] = config.background.to_rgba8();
        Ok(Self {
            view,
            inbox,
            on_message,
            budget: config.drain_budget,
            background: egui::Color32::from_rgba_unmultiplied(r, g, b, a),
            last_drain: DrainReport::default(),
        })
    }

    pub fn handle(&self) -> UiHandle<M> {
        self.inbox.handle()
    }

    pub fn view(&self) -> &QmlView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut QmlView {
        &mut self.view
    }

    pub fn engine(&self) -> &Rc<Engine> {
        self.view.engine()
    }

    /// The most recent inbox drain.
    pub fn last_drain(&self) -> DrainReport {
        self.last_drain
    }

    /// Pipeline step 1: deliver queued messages. The view's waker is held
    /// because the pass that follows shows whatever the handlers changed.
    pub fn drain_inbox(&mut self) -> DrainReport {
        let Self {
            view,
            inbox,
            on_message,
            budget,
            ..
        } = self;
        let report = view.without_wake(|view| {
            let engine = Rc::clone(view.engine());
            let clock = Arc::clone(view.clock());
            inbox.drain(*budget, &*clock, |m| {
                if let Some(f) = on_message.as_mut() {
                    f(&engine, m);
                }
            })
        });
        self.last_drain = report;
        report
    }
}

impl<M: Send + 'static> eframe::App for ShellApp<M> {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_inbox();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, self.background);
        self.view.show(ui);
    }
}

/// Opens a window and runs until it is closed. `setup` builds the root
/// item in the given Engine.
pub fn run<M: Send + 'static>(
    config: ShellConfig,
    setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
) -> Result<(), ShellError> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(config.title.clone())
        .with_inner_size(config.size);
    if let Some(min) = config.min_size {
        viewport = viewport.with_min_inner_size(min);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let title = config.title.clone();
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            // Before `setup`: bindings may measure text while the root is
            // being built.
            if config.system_fonts {
                ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
            }
            let app = ShellApp::new(&cc.egui_ctx, &config, Arc::new(SystemClock::new()), setup)
                // `ShellError` may hold non-Send engine errors; eframe needs Send.
                .map_err(|e| -> SetupError { e.to_string().into() })?;
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}
