//! The Ljós backend (ARCHITECTURE.md §18.3): the scenario runs in-process on
//! a real [`Engine`] with `QtQuick` registered, under a [`QuickScene`], on a
//! virtual clock.
//!
//! It mirrors the Qt oracle's setup so that one scenario means the same on
//! both: every loaded file is created as a child of a 400×400 driver item,
//! objects are found by `objectName` below the loaded roots, and Qt-style
//! messages are collected from the engine and the scene. Unlike the oracle,
//! nothing is replayed: each step acts on the live engine, then the scene is
//! settled (frames run until nothing is dirty), as a host would before the
//! user could see anything. Time is virtual: "1 second passes" fires each
//! timer at its own deadline without sleeping.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use ljos_a11y::NodeKey;
use ljos_compiler::{Project, compile_file, failure_reason};
use ljos_drive::{Advance, Chord, DriveError, DriveState, Driver, Host, Wheel, parse_modifiers};
use ljos_engine::{Engine, EngineError, MapSource, ObjectId, Value as QmlValue};
use ljos_quick::{Modifiers, MouseButtons, PointerEvent, PointerId, QuickScene};
use ljos_render::RasterConfig;
use ljos_render::kurbo::{self, Point};
use ljos_runtime::{Clock as _, VirtualClock};
use qml_front::project::{ControlsStyle, is_script};
use serde_json::{Map, Number, Value, json};

use super::fake_platform::{self, FakePlatform};
use super::{
    A11yNode, Application, ApplicationFault, Assistive, Backend, BackendError, Environment, Files,
    Input, LogEntry, LogLevel, Platform, PointerStep, Probes, QmlFile, Reload, Result, Tooling,
};

mod aot;
mod compiled;
mod disk;
mod environment;
mod fixtures;
mod inspect;
mod lazy;
mod paged;
mod platform;
mod reload;
mod text;
mod top;

/// The driver's file key. Not a valid QML type name for a scenario author
/// to collide with by accident.
const DRIVER: &str = "__LjosBddDriver";

/// Same size as the Qt oracle's driver item, so `parent.width` agrees.
const VIEWPORT: f64 = 400.0;

/// Helpers that need JavaScript semantics. `bindWith` is how `Qt.binding`
/// gets a function with the target's properties in scope; the function
/// itself is made by a sloppy-mode `eval` (QML modules are strict and do
/// not allow `with`).
const DRIVER_SOURCE: &str = r#"import QtQuick
Item {
    width: 400
    height: 400
    function bindWith(target, prop, maker) {
        var parts = prop.split(".")
        var o = target
        for (var i = 0; i < parts.length - 1; ++i) o = o[parts[i]]
        o[parts[parts.length - 1]] = Qt.binding(maker(target))
    }
    function toJson(v) {
        try { return JSON.stringify(v) } catch (e) { return JSON.stringify(String(v)) }
    }
}
"#;

/// The time one call into JS may take, as a wall clock measures it. A
/// frame-driven host interrupts a dispatch after 100 ms, but a scenario
/// must mean the same whatever the machine's speed (the clock is virtual
/// and image decodes are held for the same reason), and a handler that
/// does its work in a loop — a Canvas paint reading its pixels back
/// takes most of that budget on a fast machine in a debug build — would
/// otherwise be stopped mid-handler on a slow runner, leaving the
/// properties it had not reached at their defaults. Five seconds is far
/// past any handler that means to finish and still ends one that never
/// would, at the price of those few scenarios taking that long.
const JS_BUDGET: Duration = Duration::from_secs(5);

/// The live parts, made by the first `load`.
struct Live {
    engine: Engine,
    clock: Arc<VirtualClock>,
    scene: QuickScene,
    driver: ObjectId,
    /// What each `load` created, in order; lookups start here.
    roots: Vec<ObjectId>,
    /// Those of `roots` opened in a window of their own: made with no
    /// parent, so no child of the driver.
    windows: Vec<ObjectId>,
    /// The `Fixtures` `Document` the application made and gave a window.
    given: Option<ObjectId>,
    /// Shared with the engine's and the scene's log handlers, so the order
    /// of messages across both is kept.
    log: Rc<RefCell<Vec<LogEntry>>>,
    /// The staged files the component source was last built from.
    compiled_from: BTreeMap<String, String>,
    /// The style behind `import QtQuick.Controls`, chosen before the first
    /// load as a program chooses it when it starts.
    controls_style: Option<ControlsStyle>,
    /// What the scene driver keeps between steps: when the scene next
    /// asks to be woken, and the buttons held down.
    drive: DriveState,
    /// The reload session of a scenario that changes files while it runs.
    reloads: Option<reload::Rig>,
    /// The watch over files on disk of a scenario that has one.
    disk: Option<disk::Disk>,
    /// The `top` overlay; hidden unless a scenario shows it.
    top: ljos_dev::Top,
    /// The files given run code compiled ahead of time.
    ahead: bool,
    /// What the application answered the last step of a drag from
    /// another application.
    outside_taken: Option<ljos_platform::DropAction>,
    /// Where that drag was last.
    outside_at: (f64, f64),
    /// Where the scene had its items when the last load came to rest: a
    /// design audit after time passed finds what moved since.
    rest: Option<Arc<ljos_audit::Placement>>,
    /// When the grace of the user's last input ends, while it has not:
    /// what moves until then is that input's doing, and `rest` is taken
    /// again there.
    rest_due: Option<ljos_runtime::Time>,
}

#[derive(Default)]
pub struct LjosBackend {
    live: Option<Live>,
    /// The report of the last design audit a step ran at a point of
    /// its own, for the steps that read it.
    audited: Option<Value>,
    /// Catalogues given before anything was loaded; the engine gets them
    /// when it is made.
    catalogs: Vec<ljos_i18n::Catalog>,
    /// Picture files given before anything was loaded, by URL.
    resources: Vec<(String, Vec<u8>)>,
    /// Text files given before anything was loaded, for the picture store.
    image_texts: Vec<(String, Vec<u8>)>,
    /// Whether the scenario changes files while it runs.
    watching: bool,
    /// The theme file given before anything was loaded: its name, and
    /// what it says.
    theme: Option<(String, String)>,
    /// What the system prefers, said before anything was loaded.
    settings: Vec<HostSetting>,
    /// What the desktop's settings portal said.
    desktop: ljos_a11y::DesktopSettings,
    /// The inspector of a scenario that an agent looks at.
    inspector: Option<inspect::Rig>,
    /// The scenario's file system and the system's dialogs.
    platform: FakePlatform,
    /// What the drags other applications make allow and propose.
    outside_actions: ljos_platform::DragActions,
    /// The wall clock the scenario pinned, in milliseconds since the UNIX
    /// epoch; given before anything was loaded.
    wall_clock: Option<f64>,
    /// The tier 1 cross-check the scenario asked for, when it asked.
    tier1_check: Option<bool>,
    /// The scenario's files run code compiled ahead of time.
    ahead: bool,
    /// The cross-check of that code the scenario asked for, when it asked.
    aot_check: Option<bool>,
    /// What native work takes on the clock of JavaScript's time limits,
    /// when the scenario gave them one.
    js_native: Option<Duration>,
    /// The platform whose key bindings the scenario asked for.
    key_bindings: Option<ljos_quick::keys::Platform>,
    /// The thread's text engine reads the machine's fonts for this scenario.
    system_fonts: bool,
    /// The style the scenario chose, given before anything was loaded.
    controls_style: Option<ControlsStyle>,
    /// The controls configuration file and variables the program starts
    /// with.
    controls_conf: Option<String>,
    controls_vars: Vec<(String, String)>,
    /// The style `-style` names on the program's command line.
    command_line_style: Option<String>,
    /// The locale variables the program starts with, as a desktop
    /// program reads them.
    locale_env: Option<Vec<(String, String)>>,
    /// Tree models in Rust the QML is given, by name, before it is loaded.
    lazy_trees: Vec<(String, lazy::LazyTree)>,
    paged_models: Vec<(String, paged::PagedModel)>,
    /// [`ljos_quick::texts_measured`] when the scenario started counting.
    measures_from: Option<u64>,
}

impl Drop for LjosBackend {
    fn drop(&mut self) {
        // The text engine belongs to the thread, which runs the next
        // scenario too: that one starts with the bundled font again.
        if self.system_fonts {
            ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
        }
    }
}

impl std::fmt::Debug for LjosBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LjosBackend")
            .field("loaded", &self.live.as_ref().map_or(0, |l| l.roots.len()))
            .finish()
    }
}

/// Who said what the user prefers.
#[derive(Clone, Debug)]
enum HostSetting {
    System(super::SystemSetting),
    App(super::SystemSetting),
    Desktop(ljos_a11y::HostPrefs),
    /// The desktop's interface font, which the shell makes the
    /// application font.
    Font(ljos_engine::Font),
    Network(ljos_platform::NetworkStatus),
    /// The application palette's link colour, as a host sets it.
    LinkColour(ljos_engine::Color),
    NoExternalDrops,
}

fn apply(engine: &Engine, said: HostSetting) {
    match said {
        // The platform's say, which an assistive technology that listens no
        // longer stands in for.
        HostSetting::System(super::SystemSetting::ScreenReader(active)) => {
            ljos_quick::set_screen_reader_from_platform(engine, active)
        }
        HostSetting::System(setting) => {
            let mut prefs = ljos_quick::system_prefs(engine);
            change_setting(&mut prefs, setting);
            report_all(engine, prefs);
        }
        HostSetting::App(setting) => {
            ljos_quick::update_system_prefs(engine, |p| change_setting(p, setting))
        }
        HostSetting::Desktop(report) => ljos_quick::report_system_prefs(engine, report),
        HostSetting::Font(font) => ljos_controls::set_application_font(engine, Some(&font)),
        HostSetting::Network(status) => ljos_platform::report_network_status(engine, status),
        HostSetting::LinkColour(link) => {
            let palette = ljos_quick::application_palette(engine).with_link(link);
            ljos_quick::set_application_palette(engine, palette)
        }
        HostSetting::NoExternalDrops => ljos_platform::Platform::of(engine)
            .set_external_drops_unavailable(engine, "the window system has no drag-and-drop"),
    }
}

/// A drop action by the name a scenario gives it.
fn drop_action(name: &str) -> Result<ljos_platform::DropAction> {
    ljos_platform::DropAction::named(name)
        .ok_or_else(|| BackendError::Failed(format!("no drop action is called {name:?}")))
}

/// The system, which says every setting.
fn report_all(engine: &Engine, p: ljos_a11y::SystemPrefs) {
    ljos_quick::report_system_prefs(
        engine,
        ljos_a11y::HostPrefs {
            color_scheme: Some(p.color_scheme),
            contrast: Some(p.contrast),
            reduced_motion: Some(p.reduced_motion),
            font_scale: Some(p.font_scale),
            screen_reader_active: Some(p.screen_reader_active),
        },
    );
}

fn change_setting(p: &mut ljos_a11y::SystemPrefs, setting: super::SystemSetting) {
    use super::SystemSetting as S;
    match setting {
        S::DarkScheme(dark) => {
            p.color_scheme = if dark {
                ljos_a11y::ColorScheme::Dark
            } else {
                ljos_a11y::ColorScheme::Light
            }
        }
        S::HighContrast(high) => {
            p.contrast = if high {
                ljos_a11y::Contrast::High
            } else {
                ljos_a11y::Contrast::NoPreference
            }
        }
        S::ReducedMotion(reduced) => p.reduced_motion = reduced,
        S::FontScale(scale) => p.font_scale = scale,
        S::ScreenReader(active) => p.screen_reader_active = active,
    }
}

fn failed(e: impl std::fmt::Display) -> BackendError {
    BackendError::Failed(e.to_string())
}

fn not_loaded() -> BackendError {
    BackendError::Failed("nothing is loaded yet".into())
}

/// `Scenario.qml` → `Scenario`, the engine's file key.
fn key_of(name: &str) -> &str {
    name.strip_suffix(".qml").unwrap_or(name)
}

/// The directory `name` is the `qmldir` file of (empty for the scenario's
/// own), if it is one: it describes a module and is not QML.
fn qmldir_of(name: &str) -> Option<&str> {
    match name.strip_suffix("qmldir")? {
        "" => Some(""),
        dir => dir.strip_suffix('/'),
    }
}

impl LjosBackend {
    /// Runs a step of the user's input and takes where the items rest
    /// again once its grace has passed: what the user's own input moves
    /// (a panel they open, sliding in) is not a layout shift, and what
    /// moves later by itself still is. A settle does not run an
    /// animation the input started, so where the items are when the step
    /// ends is only its first frame.
    fn input(&mut self, step: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        step(self)?;
        let live = self.live()?;
        live.take_rest();
        let grace =
            Duration::from_millis(u64::from(ljos_design::limits::LAYOUT_SHIFT_INPUT_GRACE_MS));
        live.rest_due = Some(live.clock.now().saturating_add(grace));
        Ok(())
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many drags the window system has carried out of the window.
    fn drags_out_started(&self) -> usize {
        self.platform
            .drag_out()
            .map_or(0, ljos_platform::RecordedDragOut::started)
    }

    fn live(&mut self) -> Result<&mut Live> {
        self.live.as_mut().ok_or_else(not_loaded)
    }

    /// A setting said now, or at the start when nothing is loaded yet.
    fn apply_or_queue(&mut self, said: HostSetting) -> Result<()> {
        match self.live.as_mut() {
            Some(live) => {
                apply(&live.engine, said);
                live.settle()
            }
            None => {
                self.settings.push(said);
                Ok(())
            }
        }
    }

    fn start() -> Result<Live> {
        let engine = Engine::new();
        // Qt evaluates a binding when it is installed, which is when it
        // reports a binding loop; lazy evaluation would stay silent.
        engine.set_eager_bindings(true);
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        // Opt-in modules a scenario may import (QtQml.XmlListModel,
        // Qt.labs.folderlistmodel, Qt.labs.lottieqt, QtPositioning,
        // QtMultimedia, QtQuick.LocalStorage).
        // Sound goes nowhere: the clock is virtual.
        ljos_xml::register(&engine);
        ljos_lottie::register(&engine);
        ljos_quick3d::register(&engine);
        ljos_folderlistmodel::register(&engine);
        ljos_positioning::register(&engine);
        ljos_media::register(&engine);
        ljos_media::use_null_audio(&engine);
        ljos_localstorage::register(&engine);
        fixtures::register(&engine);
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(clock.clone()));
        let log: Rc<RefCell<Vec<LogEntry>>> = Rc::default();
        let sink = log.clone();
        engine.set_log_handler(move |e| {
            sink.borrow_mut().push(LogEntry {
                level: level(e.level),
                message: e.message.clone(),
            });
        });
        fixtures::log_documents(&engine);
        let mut compiled = BTreeMap::new();
        compiled.insert(format!("{DRIVER}.qml"), DRIVER_SOURCE.to_owned());
        engine.set_component_source(compile(&compiled, None)?);
        let driver = engine
            .create_from_url(DRIVER, None, Vec::new())
            .map_err(|e| failed(format!("the test driver does not load: {e}")))?;
        // The budget is the JS host's, which the driver has started by now.
        engine
            .set_js_dispatch_budget(JS_BUDGET, JS_BUDGET)
            .map_err(|e| failed(format!("the JS budget is not set: {e}")))?;
        let mut scene = QuickScene::new(kurbo::Size::new(VIEWPORT, VIEWPORT));
        scene.set_root(&engine, driver).map_err(failed)?;
        let mut live = Live {
            engine,
            clock,
            scene,
            driver,
            roots: Vec::new(),
            windows: Vec::new(),
            given: None,
            log,
            compiled_from: compiled,
            controls_style: None,
            drive: DriveState::default(),
            reloads: None,
            disk: None,
            top: ljos_dev::Top::new(),
            ahead: false,
            outside_taken: None,
            outside_at: (0.0, 0.0),
            rest: None,
            rest_due: None,
        };
        // A decode takes as long as it takes; scenarios see its result when
        // time is advanced and not before, whatever the machine's speed.
        ljos_quick::ImageStore::of(&live.engine).set_held(true);
        live.settle()?;
        Ok(live)
    }

    /// Gives a new engine what the scenario said before anything was
    /// loaded: settings, locale, style, platform, clocks and the data
    /// and resources its program will find.
    fn configure(&mut self, live: &mut Live) -> Result<()> {
        for catalog in self.catalogs.drain(..) {
            live.engine.install_catalog(Box::new(catalog));
        }
        for said in self.settings.drain(..) {
            apply(&live.engine, said);
        }
        if let Some(vars) = self.locale_env.take() {
            let system = ljos_i18n::SystemLocale::from_vars(|k| {
                vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
            });
            live.engine.use_system_locale_from(&system);
        }
        let conf = self.controls_conf.take();
        let mut vars = std::mem::take(&mut self.controls_vars);
        // `-style` comes before the variable; `ljos run` gives it as one.
        if let Some(name) = self.command_line_style.take() {
            vars.push(("QT_QUICK_CONTROLS_STYLE".to_owned(), name));
        }
        let var = |name: &str| match name {
            "QT_QUICK_CONTROLS_CONF" => conf.as_ref().map(|_| "qtquickcontrols2.conf".into()),
            _ => vars
                .iter()
                .rev()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone()),
        };
        let read = |_: &str| conf.clone();
        if conf.is_some() || !vars.is_empty() {
            ljos_controls::set_material_defaults(
                &live.engine,
                ljos_controls::MaterialDefaults::from_vars(var, read),
            );
        }
        // With nothing chosen, the platform's style, as a program has.
        if self.controls_style.is_none() {
            self.controls_style = ljos_compiler::controls_style(var, read);
        }
        self.platform.install(&live.engine)?;
        live.engine.set_wall_clock_origin(self.wall_clock);
        if let Some(on) = self.tier1_check {
            live.engine.set_tier1_check(on);
        }
        if self.ahead {
            live.engine
                .register_compiled(compiled::COMPILED)
                .map_err(|e| failed(e.to_string()))?;
            live.ahead = true;
        }
        if let Some(on) = self.aot_check {
            live.engine.set_aot_check(on);
        }
        if let Some(native) = self.js_native {
            set_js_clock(&live.engine, native)?;
        }
        // A scenario is not told which desktop ran it: with none asked
        // for, the build's platform, and on Linux no desktop's own keys.
        let platform = self.key_bindings.unwrap_or_else(|| {
            use ljos_quick::keys::Platform;
            match Platform::current() {
                Platform::Gnome | Platform::Kde => Platform::Linux,
                p => p,
            }
        });
        ljos_quick::keys::set_platform(&live.engine, platform);
        for (name, tree) in self.lazy_trees.drain(..) {
            let e = &live.engine;
            let obj = e
                .create_by_name("QtQml", "QtObject", None)
                .map_err(failed)?;
            e.set_object_tree(obj, Rc::new(tree)).map_err(failed)?;
            e.set_context_property(e.root_context(), &name, QmlValue::Object(obj));
        }
        for (name, model) in self.paged_models.drain(..) {
            let e = &live.engine;
            let obj = e
                .create_by_name("QtQml", "QtObject", None)
                .map_err(failed)?;
            e.set_object_model(obj, Some(Rc::new(model)))
                .map_err(failed)?;
            e.set_context_property(e.root_context(), &name, QmlValue::Object(obj));
        }
        let images = ljos_quick::ImageStore::of(&live.engine);
        for (url, bytes) in self.resources.drain(..) {
            ljos_platform::Platform::of(&live.engine).register_resource(&url, bytes.clone());
            images.register_resource(url, bytes);
        }
        for (url, bytes) in self.image_texts.drain(..) {
            images.register_resource(url, bytes);
        }
        live.controls_style = self.controls_style.take();
        Ok(())
    }
}

fn level(l: ljos_engine::LogLevel) -> LogLevel {
    match l {
        ljos_engine::LogLevel::Debug => LogLevel::Debug,
        ljos_engine::LogLevel::Info => LogLevel::Info,
        ljos_engine::LogLevel::Warning => LogLevel::Warning,
        ljos_engine::LogLevel::Critical => LogLevel::Critical,
    }
}

/// Compiles every file against every other, as `qml6` sees one directory.
/// A file with errors fails the whole load, as `Qt.createComponent` would
/// for the file that uses it.
/// The oracle every compile in a scenario uses: the table plus the types
/// a BDD engine registers itself (ljos_xml is in no table), snapshotted
/// once per thread.
pub(super) fn oracle() -> ljos::dev::EngineOracle {
    thread_local! {
        static ORACLE: std::cell::RefCell<Option<ljos::dev::EngineOracle>> =
            const { std::cell::RefCell::new(None) };
    }
    ORACLE.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                let snapshot = Engine::new();
                ljos_quick::register(&snapshot);
                ljos_controls::register(&snapshot);
                ljos_xml::register(&snapshot);
                ljos_lottie::register(&snapshot);
                ljos_quick3d::register(&snapshot);
                ljos_folderlistmodel::register(&snapshot);
                ljos_positioning::register(&snapshot);
                ljos_media::register(&snapshot);
                ljos_localstorage::register(&snapshot);
                fixtures::register(&snapshot);
                ljos::dev::EngineOracle::from_engine(&snapshot)
            })
            .clone()
    })
}

fn compile(files: &BTreeMap<String, String>, style: Option<&ControlsStyle>) -> Result<MapSource> {
    let mut project = Project::new(oracle());
    // A scenario's files have the URLs of files at the root of the disk
    // (`file:///Main.qml`), so that is where a `file:` URL leads.
    project.add_place(std::path::Path::new("/"), "");
    project.set_controls_style(style.cloned());
    for (name, source) in files {
        if is_script(name) {
            project.add_script(name, source.as_str());
        } else if qmldir_of(name).is_none() {
            project.add(key_of(name), source.as_str());
        }
    }
    // After the files, which are what a module's lines name.
    for (name, source) in files {
        if let Some(dir) = qmldir_of(name) {
            project.add_qmldir(dir, source);
        }
    }
    let mut out = MapSource::new();
    for name in files
        .keys()
        .filter(|name| qmldir_of(name).is_none() && !is_script(name))
    {
        // A file that does not compile stops nothing until an object of
        // it is made, which is then told why.
        let key = key_of(name);
        match compile_file(&project, key) {
            Ok(c) if !c.has_errors() => out.insert(key, c.file),
            Ok(c) => out.insert_failure(key, failure_reason(&c.diagnostics)),
            Err(ds) => out.insert_failure(key, failure_reason(&ds)),
        }
    }
    Ok(out)
}

impl Live {
    /// Runs frames until the scene is idle: polish (positioners, text),
    /// anchors, deferred deletes and sync, as a host does before the user
    /// sees anything.
    fn settle(&mut self) -> Result<()> {
        self.drive(|d| d.settle())
    }

    /// Takes where the items rest once the last input's grace ends, if
    /// it ends by `end`: when nothing is due before then, the scene stays
    /// as it is now until then, so no frame is made to look.
    fn rest_after_grace(&mut self, end: ljos_runtime::Time) {
        let Some(due) = self.rest_due.filter(|due| *due <= end) else {
            return;
        };
        let next = [
            self.engine.next_timer_deadline(),
            self.drive.wake,
            self.reload_due(),
        ]
        .into_iter()
        .flatten()
        .min();
        if next.is_none_or(|t| t > due) {
            self.take_rest();
            self.rest_due = None;
        }
    }

    /// Takes where the items are now as where they rest.
    fn take_rest(&mut self) {
        self.rest = Some(Arc::new(ljos_audit::Placement::of(
            &self.engine,
            &self.scene,
        )));
    }

    /// Runs `act` on the scene driver, with this backend playing the
    /// host's part around each settle.
    fn drive<R>(
        &mut self,
        act: impl FnOnce(&mut Driver<'_>) -> std::result::Result<R, DriveError>,
    ) -> Result<R> {
        let mut host = Hosting {
            windows: &mut self.windows,
            roots: &mut self.roots,
            top: &mut self.top,
        };
        let mut driver = Driver::new(&self.engine, &mut self.scene, &*self.clock, &mut self.drive)
            .hosted(&mut host);
        act(&mut driver).map_err(failed)
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        let mut wanted: BTreeMap<String, String> = files
            .iter()
            .map(|f| (f.name.clone(), f.source.clone()))
            .collect();
        wanted.insert(format!("{DRIVER}.qml"), DRIVER_SOURCE.to_owned());
        if wanted != self.compiled_from {
            // A scenario that reloads compiles through its reload session.
            if !self.reload_compile(&wanted)? {
                let source = compile(&wanted, self.controls_style.as_ref())?;
                self.engine.set_component_source(source);
            }
            self.engine
                .set_qml_text_compiler(Some(text::compiler(&wanted)));
            self.compiled_from = wanted;
            if self.ahead {
                let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
                self.compiled_ahead(&names)?;
            }
        }
        let obj = self
            .engine
            .create_from_url(key_of(entry), Some(self.driver), Vec::new())
            .map_err(|e| failed(format!("cannot load {entry}: {e}")))?;
        // `createObject(driver)`: an item becomes a visual child.
        ljos_quick::add_data(&self.engine, self.driver, obj).map_err(failed)?;
        self.roots.push(obj);
        self.settle()?;
        self.take_rest();
        self.rest_due = None;
        Ok(())
    }

    fn open_window(&mut self, name: &str) -> Result<()> {
        let obj = self
            .engine
            .create_from_url(key_of(name), None, Vec::new())
            .map_err(|e| failed(format!("cannot open {name}: {e}")))?;
        self.roots.push(obj);
        self.windows.push(obj);
        ljos_quick::roots::RootSwaps::of(&self.engine).shows(None, obj);
        self.settle()
    }

    /// What an application does that makes its interface again: a new
    /// root, put in the window of the old one by the host before its next
    /// frame.
    fn put_in_window(&mut self, name: &str, old: &str) -> Result<()> {
        let old = self.find(old)?;
        let new = self
            .engine
            .create_from_url(key_of(name), None, Vec::new())
            .map_err(|e| failed(format!("cannot make {name}: {e}")))?;
        if let Err(e) = ljos_quick::roots::replace_root(&self.engine, old, new) {
            self.engine.destroy(new);
            return Err(failed(e));
        }
        self.settle()
    }

    /// As an application that makes a window's `Document` before the
    /// window does: the object is made, given to a scope of the window's
    /// own, and the root is made in that scope.
    fn open_window_given(&mut self, name: &str, title: &str) -> Result<()> {
        let made = fixtures::document(&self.engine, title).map_err(failed)?;
        let window = self.engine.window_scope();
        self.engine
            .set_singleton_in::<fixtures::Document>(window, made)
            .map_err(failed)?;
        let obj = self
            .engine
            .in_window_scope(window, || {
                self.engine.create_from_url(key_of(name), None, Vec::new())
            })
            .map_err(|e| failed(format!("cannot open {name}: {e}")))?;
        self.given = Some(made);
        self.roots.push(obj);
        self.windows.push(obj);
        self.settle()
    }

    /// An object path below the loaded roots ([`ljos_drive::find`]).
    fn find(&self, path: &str) -> Result<ObjectId> {
        ljos_drive::find(&self.engine, &self.roots, path).map_err(failed)
    }

    /// The object and property a dotted path names: `anchors.margins` is
    /// one (flattened) property of the item; `foo.bar` where `foo` holds an
    /// object is `bar` of that object; `Accessible.name` is `name` of the
    /// object that type attaches.
    fn owner(&self, obj: ObjectId, prop: &str) -> Result<(ObjectId, String)> {
        let mut obj = obj;
        let mut rest = prop;
        loop {
            if self.engine.property_index(obj, rest).is_ok() {
                return Ok((obj, rest.to_owned()));
            }
            let Some((head, tail)) = rest.split_once('.') else {
                return Ok((obj, rest.to_owned()));
            };
            let next = match self.engine.read_by_name(obj, head).as_object() {
                Some(next) => Some(next),
                None => self.attached(obj, head),
            };
            match next {
                Some(next) => {
                    obj = next;
                    rest = tail;
                }
                None => match self.further(obj, rest) {
                    Some((next, tail)) => {
                        obj = next;
                        rest = tail;
                    }
                    None => return Ok((obj, rest.to_owned())),
                },
            }
        }
    }

    /// The object further along `rest` than its first name: the one a
    /// member of a group holds (`up.indicator.x`), or an element of a list
    /// by its number (`children.0.visible`), and the path that is left.
    fn further<'p>(&self, obj: ObjectId, rest: &'p str) -> Option<(ObjectId, &'p str)> {
        for (at, _) in rest.match_indices('.') {
            let (head, tail) = (&rest[..at], &rest[at + 1..]);
            if self.engine.property_index(obj, head).is_err() {
                continue;
            }
            match self.engine.read_by_name(obj, head) {
                QmlValue::List(items) => {
                    let (index, after) = tail.split_once('.')?;
                    let next = items.get(index.parse::<usize>().ok()?)?.as_object()?;
                    return Some((next, after));
                }
                held => {
                    if let Some(next) = held.as_object() {
                        return Some((next, tail));
                    }
                }
            }
        }
        None
    }

    /// A write to a member of a value type (`font.pixelSize`) as the write
    /// of the whole value with that member changed, which is what QML does.
    /// Anything else comes back as it was.
    fn member_write(&self, owner: ObjectId, prop: String, v: QmlValue) -> (String, QmlValue) {
        if self.engine.property_index(owner, &prop).is_ok() {
            return (prop, v);
        }
        let whole = prop.split_once('.').and_then(|(head, member)| {
            let meta = self.engine.meta_of(owner)?;
            let ty = meta.property(meta.property_index(head)?)?.ty.clone();
            let changed = self
                .engine
                .read_by_name(owner, head)
                .with_member(&ty, member, &v)?;
            Some((head.to_owned(), changed))
        });
        whole.unwrap_or((prop, v))
    }

    /// The object the type `name` attaches to `obj`, made on first use as
    /// QML makes it. Type names are capitalised; properties are not.
    fn attached(&self, obj: ObjectId, name: &str) -> Option<ObjectId> {
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let attacher = self.engine.lookup_type_any(name)?;
        self.engine.attached_object(obj, &attacher).ok()
    }

    fn to_qml(&self, v: &Value) -> Result<QmlValue> {
        Ok(match v {
            Value::Null => QmlValue::Null,
            Value::Bool(b) => QmlValue::Bool(*b),
            Value::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
                Some(i) => QmlValue::Int(i),
                None => QmlValue::Real(n.as_f64().unwrap_or(f64::NAN)),
            },
            Value::String(s) => QmlValue::string(s.as_str()),
            Value::Array(items) => QmlValue::list(
                items
                    .iter()
                    .map(|i| self.to_qml(i))
                    .collect::<Result<_>>()?,
            ),
            // A JS object literal, made by JS so that it is one.
            Value::Object(_) => self.engine.eval_js(&format!("({v})")).map_err(failed)?,
        })
    }

    /// As the Qt driver's `ser`: what `JSON.stringify` would give, colours
    /// as their `#rrggbb` text.
    fn to_json(&self, v: &QmlValue) -> Value {
        // Whole numbers print as integers, as `JSON.stringify` does.
        let num = |n: f64| {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                json!(n as i64)
            } else {
                Number::from_f64(n).map_or_else(|| Value::String(n.to_string()), Value::Number)
            }
        };
        match v {
            QmlValue::Undefined | QmlValue::Null => Value::Null,
            QmlValue::Bool(b) => Value::Bool(*b),
            QmlValue::Int(i) => json!(i),
            QmlValue::Real(r) => num(*r),
            QmlValue::String(s) | QmlValue::Url(s) => Value::String(s.to_string()),
            QmlValue::Color(c) => Value::String(c.to_string()),
            QmlValue::Enum(e) => json!(e.value),
            QmlValue::Point(p) => json!({"x": num(p.x), "y": num(p.y)}),
            QmlValue::IntPoint(p) => json!({"x": p.x, "y": p.y}),
            QmlValue::Size(s) => json!({"width": num(s.width), "height": num(s.height)}),
            QmlValue::Rect(r) => json!({
                "x": num(r.x), "y": num(r.y), "width": num(r.width), "height": num(r.height)
            }),
            QmlValue::List(items) => Value::Array(items.iter().map(|i| self.to_json(i)).collect()),
            QmlValue::Map(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.to_string(), self.to_json(v)))
                    .collect(),
            ),
            QmlValue::Font(_)
            | QmlValue::Vector2d(_)
            | QmlValue::Vector3d(_)
            | QmlValue::Vector4d(_)
            | QmlValue::Quaternion(_)
            | QmlValue::Js(_)
            | QmlValue::Custom(_)
            | QmlValue::Object(_) => self.js_json(v),
        }
    }

    /// Values only JavaScript can describe, through `JSON.stringify`.
    fn js_json(&self, v: &QmlValue) -> Value {
        match self
            .engine
            .invoke_by_name(self.driver, "toJson", std::slice::from_ref(v))
        {
            Ok(QmlValue::String(s)) => {
                serde_json::from_str(s.as_str()).unwrap_or(Value::Object(Map::new()))
            }
            _ => Value::String(self.engine.to_js_string(v).to_string()),
        }
    }

    /// Lets `ms` pass on the test clock as the scene driver does, also
    /// stopping when a saved file's quiet time ends, to reload it there.
    fn advance(&mut self, ms: u64) -> Result<()> {
        let mut advance = Advance::new(&*self.clock, ms).map_err(failed)?;
        // Every step ends settled, so the scene's next wake is known: a
        // settle here would make a frame with nothing new, and take back
        // the next frame a held press asked for.
        self.drive(|d| d.finish_promised())?;
        loop {
            self.rest_after_grace(advance.end());
            let due = self.reload_due();
            let stop = advance
                .stop(&self.engine, &*self.clock, self.drive.wake, due)
                .map_err(failed)?;
            if stop.is_none() {
                return Ok(());
            }
            self.reload_if_due()?;
            self.drive(|d| d.finish_promised())?;
            self.settle()?;
        }
    }

    /// Each frame as one touch event of what changed since the last, as
    /// the shell gives the scene a pass's touches.
    fn touch_frames(&mut self, frames: &[super::TouchFrame]) -> Result<()> {
        use ljos_quick::touch::{TouchEvent, TouchPhase};
        let mut down: Vec<(u32, f64, f64)> = Vec::new();
        for frame in frames.iter().chain(std::iter::once(&Vec::new())) {
            let now = self.clock.now();
            let event = |id: u32, phase, x, y| TouchEvent {
                id: PointerId(u64::from(id) + 1),
                phase,
                pos: kurbo::Point::new(x, y),
                time: now,
            };
            let mut events = Vec::new();
            for &(id, x, y) in &down {
                if !frame.iter().any(|f| f.0 == id) {
                    events.push(event(id, TouchPhase::End, x, y));
                }
            }
            for &(id, x, y) in frame {
                match down.iter().find(|d| d.0 == id) {
                    Some(d) if (d.1, d.2) == (x, y) => {}
                    Some(_) => events.push(event(id, TouchPhase::Move, x, y)),
                    None => events.push(event(id, TouchPhase::Begin, x, y)),
                }
            }
            if !events.is_empty() {
                self.scene.touch_event(&self.engine, &events);
                self.settle()?;
            }
            down.clone_from(frame);
        }
        Ok(())
    }

    /// A click of the left button with `held` down, where it is: no
    /// move first, as a test's click is.
    fn tap_holding(&mut self, held: Modifiers, x: f64, y: f64) -> Result<()> {
        self.drive(|d| {
            let now = d.now();
            d.send_pointer(&PointerEvent::press(x, y, now).with_modifiers(held));
            d.send_pointer(&PointerEvent::release(x, y, now).with_modifiers(held));
            d.settle()
        })
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        let at = Point::new(x, y);
        self.drive(|d| {
            match step {
                PointerStep::Press => d.press(at, MouseButtons::LEFT),
                PointerStep::Move => d.move_to(at),
                PointerStep::Release => d.release(at, MouseButtons::LEFT),
            }
            .map(|_| ())
        })
    }

    /// The press goes to the window system: the scene lets go of it.
    fn pointer_taken(&mut self) -> Result<()> {
        self.drive(|d| {
            let now = d.now();
            d.send_pointer(&PointerEvent::cancel(now));
            d.settle()
        })
    }

    /// The tree as steps see it: role words, names and nesting.
    fn a11y(&self) -> A11yNode {
        // The window is named by its title, as the shell tells it.
        let sys = ljos_controls::WindowSystem::of(&self.engine);
        let name = self
            .roots
            .iter()
            .find_map(|r| sys.spec(&self.engine, *r))
            .map(|s| s.title)
            .unwrap_or_default();
        ljos_drive::a11y::tree(&self.scene, name)
    }

    /// The first node, in tree order, with this role and name.
    fn accessible_key(&self, role: &str, name: &str) -> Option<NodeKey> {
        ljos_drive::a11y::node(&self.scene, role, name)
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        self.drive(|d| d.grab(RasterConfig::default()))
    }
}

/// The host's part a scenario plays around each settle.
struct Hosting<'a> {
    windows: &'a mut Vec<ObjectId>,
    roots: &'a mut Vec<ObjectId>,
    top: &'a mut ljos_dev::Top,
}

impl Host for Hosting<'_> {
    /// The host's part of [`Live::put_in_window`]: each window of its own
    /// shows the root asked for, at its size, and the old root goes.
    fn before_settle(&mut self, engine: &Engine, _: &mut QuickScene) {
        let swaps = ljos_quick::roots::RootSwaps::of(engine);
        if !swaps.is_pending() {
            return;
        }
        for i in 0..self.windows.len() {
            while let Some(new) = swaps.take(engine, self.windows[i]) {
                let old = self.windows[i];
                ljos_quick::roots::adopt(engine, old, new);
                self.windows[i] = new;
                for r in self.roots.iter_mut() {
                    if *r == old {
                        *r = new;
                    }
                }
                engine.destroy(old);
            }
        }
    }

    /// After a frame: what its creation jobs cost goes to the `top`
    /// overlay while it is shown. Hidden, the report is left where it is,
    /// for the inspector's measurements.
    fn after_frame(&mut self, engine: &Engine, scene: &mut QuickScene) {
        if self.top.visible() {
            let report = scene.take_incubation_report().unwrap_or_default();
            self.top.record(engine, &report);
        }
    }
}

impl Backend for LjosBackend {
    fn name(&self) -> &'static str {
        "ljos"
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        if self.live.is_none() {
            let mut live = Self::start()?;
            self.configure(&mut live)?;
            self.live = Some(live);
            // An inspector connected before anything was loaded reads
            // this application's log from its first message.
            if let (Some(rig), Some(live)) = (&mut self.inspector, &self.live) {
                rig.attach(live);
            }
            // Before the scenario's first object: a reload knows only
            // what was made while it was noted.
            if self.watching
                && let Some(live) = self.live.as_mut()
            {
                reload::Rig::install(live);
            }
            if let (Some((name, text)), Some(live)) = (self.theme.take(), self.live.as_mut()) {
                live.start_theme(&name, text);
            }
        }
        self.live()?.load(files, entry)
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let (owner, prop) = live.owner(obj, property)?;
        let v = live.to_qml(value)?;
        let (prop, v) = live.member_write(owner, prop, v);
        live.engine
            .write_by_name(owner, &prop, v)
            .map_err(|e| failed(format!("cannot set {property}: {e}")))?;
        live.settle()
    }

    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        // Sloppy mode, so `with` can put the object's properties in scope,
        // exactly as a binding written inside the object would see them.
        // The host evaluates scripts in strict mode, so the sloppy function
        // comes from the `Function` constructor.
        let body = format!("return function () {{ with (self) {{ return ({expression}); }} }};");
        let body = Value::String(body).to_string();
        let maker = live
            .engine
            .eval_js(&format!("new Function(\"self\", {body})"))
            .map_err(|e| failed(format!("cannot compile {expression:?}: {e}")))?;
        live.engine
            .invoke_by_name(
                live.driver,
                "bindWith",
                &[QmlValue::Object(obj), QmlValue::string(property), maker],
            )
            .map_err(failed)?;
        live.settle()
    }

    fn get(&mut self, object: &str, property: &str) -> Result<Value> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let (owner, prop) = live.owner(obj, property)?;
        if live.engine.property_index(owner, &prop).is_err()
            && let Some((head, members)) = prop.split_once('.')
        {
            // A member of a value type (`childrenRect.x`, `font.pixelSize`):
            // the value is a copy, so read it whole and pick the member out.
            let whole = live.to_json(&live.engine.read_by_name(owner, head));
            return Ok(members
                .split('.')
                .try_fold(whole, |v, m| match (&v, m) {
                    // A list has a length, as it has in a binding.
                    (Value::Array(items), "length") => Some(Value::from(items.len())),
                    _ => v.get(m).cloned(),
                })
                .unwrap_or(Value::Null));
        }
        let v = live.engine.read_by_name(owner, &prop);
        Ok(live.to_json(&v))
    }

    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let args = args
            .iter()
            .map(|a| live.to_qml(a))
            .collect::<Result<Vec<_>>>()?;
        // A signal is called as a method is, and calling it emits it.
        let out = match live.engine.invoke_by_name(obj, method, &args) {
            Err(EngineError::UnknownMethod { .. })
                if live.engine.signal_index(obj, method).is_ok() =>
            {
                live.engine
                    .emit_by_name(obj, method, &args)
                    .map(|()| QmlValue::Undefined)
            }
            r => r,
        }
        .map_err(|e| failed(format!("cannot call {method}: {e}")))?;
        let out = live.to_json(&out);
        live.settle()?;
        Ok(out)
    }

    fn exists(&mut self, object: &str) -> Result<bool> {
        let live = self.live()?;
        Ok(live.find(object).is_ok())
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        self.live()?.advance(ms)
    }

    fn click(&mut self, role: &str, name: &str) -> Result<()> {
        self.input(|b| {
            // A user's click: a pointer in the middle of the control, so
            // whatever covers it there takes the click, as on screen. One a
            // user could not click there (disabled, outside the window) is
            // not clicked, and the step goes on, as the user would.
            b.live()?.drive(|d| match d.click_accessible(role, name) {
                Err(DriveError::Refused { .. }) => Ok(()),
                r => r,
            })
        })
    }

    fn key(&mut self, chord: &str) -> Result<()> {
        self.key_hold(chord, 0)
    }

    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()> {
        self.input(|b| {
            let before = b.drags_out_started();
            b.live()?.pointer(step, x, y)?;
            // The window system took the press for the drag it carries now,
            // as the shell's windows let it go (`QmlView::pointer_taken`).
            if b.drags_out_started() != before {
                b.live()?.pointer_taken()?;
            }
            Ok(())
        })
    }

    fn a11y_tree(&mut self) -> Result<A11yNode> {
        let live = self.live()?;
        live.drive(|d| d.listen())?;
        Ok(live.a11y())
    }

    fn log(&mut self) -> Result<Vec<LogEntry>> {
        // A load that failed before anything was made has an empty log.
        Ok(self
            .live
            .as_ref()
            .map(|l| l.log.borrow().clone())
            .unwrap_or_default())
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        self.live()?.grab()
    }

    fn as_input(&mut self) -> Option<&mut dyn Input> {
        Some(self)
    }

    fn as_assistive(&mut self) -> Option<&mut dyn Assistive> {
        Some(self)
    }

    fn as_files(&mut self) -> Option<&mut dyn Files> {
        Some(self)
    }

    fn as_environment(&mut self) -> Option<&mut dyn Environment> {
        Some(self)
    }

    fn as_platform(&mut self) -> Option<&mut dyn Platform> {
        Some(self)
    }

    fn as_application(&mut self) -> Option<&mut dyn Application> {
        Some(self)
    }

    fn as_reload(&mut self) -> Option<&mut dyn Reload> {
        Some(self)
    }

    fn as_probes(&mut self) -> Option<&mut dyn Probes> {
        Some(self)
    }

    fn as_tooling(&mut self) -> Option<&mut dyn Tooling> {
        Some(self)
    }
}

impl Input for LjosBackend {
    fn key_hold(&mut self, chord: &str, repeats: u32) -> Result<()> {
        self.input(|b| {
            let chord = Chord::parse(chord).map_err(failed)?;
            b.live()?.drive(|d| d.chord(&chord, repeats))
        })
    }

    fn window_active(&mut self, active: bool) -> Result<()> {
        self.input(|b| {
            let live = b.live()?;
            live.scene.set_window_active(&live.engine, active);
            live.settle()
        })
    }

    fn touch(&mut self, x: f64, y: f64) -> Result<()> {
        self.input(|b| {
            b.live()?
                .drive(|d| d.touch(Point::new(x, y), kurbo::Vec2::ZERO))
        })
    }

    fn click_holding(&mut self, modifiers: &str, x: f64, y: f64) -> Result<()> {
        self.input(|b| {
            let held = parse_modifiers(modifiers).map_err(failed)?;
            b.live()?.tap_holding(held, x, y)
        })
    }

    fn touch_frames(&mut self, frames: &[super::TouchFrame]) -> Result<()> {
        self.input(|b| b.live()?.touch_frames(frames))
    }

    fn wheel(&mut self, at: (f64, f64), by: super::WheelBy) -> Result<()> {
        self.input(|b| {
            let by = match by {
                super::WheelBy::Notches(x, y) => Wheel::Notches(x, y),
                super::WheelBy::Pixels(x, y) => Wheel::Pixels(x, y),
            };
            b.live()?
                .drive(|d| d.wheel(Point::new(at.0, at.1), by).map(|_| ()))
        })
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        self.input(|b| b.live()?.drive(|d| d.type_text(text)))
    }

    fn click_button(&mut self, button: super::OtherButton, x: f64, y: f64) -> Result<()> {
        self.input(|b| {
            let which = match button {
                super::OtherButton::Right => MouseButtons::RIGHT,
                super::OtherButton::Middle => MouseButtons::MIDDLE,
            };
            b.live()?.drive(|d| d.click(Point::new(x, y), which, 1))
        })
    }

    fn clicks(&mut self, count: u8, x: f64, y: f64) -> Result<()> {
        self.input(|b| {
            b.live()?
                .drive(|d| d.click(Point::new(x, y), MouseButtons::LEFT, count.into()))
        })
    }

    fn drag(&mut self, from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.input(|b| {
            let (from, to) = (Point::new(from.0, from.1), Point::new(to.0, to.1));
            b.live()?.drive(|d| d.drag(from, to))
        })
    }

    fn compose(&mut self, text: &str) -> Result<()> {
        self.input(|b| {
            let live = b.live()?;
            // With no editor in focus the input method has nowhere to compose;
            // the scenario says what it expects of that.
            live.drive(|d| d.preedit(text, None).map(|_| ()))
        })
    }

    fn commit(&mut self, text: &str) -> Result<()> {
        self.input(|b| b.live()?.drive(|d| d.commit(text).map(|_| ())))
    }
}

impl Assistive for LjosBackend {
    fn a11y_focus(&mut self, role: &str, name: &str) -> Result<()> {
        let live = self.live()?;
        live.drive(|d| d.listen())?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_focus(&live.engine, key);
        live.settle()
    }

    fn a11y_act(&mut self, role: &str, name: &str, action: super::A11yAction) -> Result<()> {
        use ljos_quick::AccessibleAction as To;
        let live = self.live()?;
        live.drive(|d| d.listen())?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        let action = match action {
            super::A11yAction::Press => {
                let now = live.clock.now();
                live.scene.accessible_press(&live.engine, key, now);
                return live.settle();
            }
            super::A11yAction::ShowOnScreen => To::ShowOnScreen,
            super::A11yAction::Expand => To::Expand,
            super::A11yAction::Collapse => To::Collapse,
            super::A11yAction::Increase => To::Increase,
            super::A11yAction::Decrease => To::Decrease,
            super::A11yAction::SetValue(to) => To::SetValue(to),
            super::A11yAction::Scroll { dx, dy, page } => match (dx, dy) {
                (0, 1..) => To::ScrollDown { page },
                (0, _) => To::ScrollUp { page },
                (1.., _) => To::ScrollRight { page },
                _ => To::ScrollLeft { page },
            },
        };
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_action(&live.engine, key, action);
        live.settle()
    }

    fn a11y_text(&mut self, role: &str, name: &str, action: super::A11yTextAction) -> Result<()> {
        use ljos_quick::AccessibleTextAction as To;
        let live = self.live()?;
        live.drive(|d| d.listen())?;
        let key = live
            .accessible_key(role, name)
            .ok_or_else(|| failed(format!("no {role:?} named {name:?}")))?;
        let value = live
            .scene
            .accessibility_tree()
            .get(key)
            .and_then(|d| d.text.as_ref())
            .map(|t| t.value.clone())
            .unwrap_or_default();
        // The scenario counts characters; the tree counts bytes.
        let byte = |n: usize| {
            value
                .char_indices()
                .nth(n)
                .map_or(value.len(), |(at, _)| at)
        };
        let action = match action {
            super::A11yTextAction::Select(from, to) => To::Select {
                anchor: byte(from),
                cursor: byte(to),
            },
            super::A11yTextAction::ReplaceSelection(text) => To::ReplaceSelection(text),
            super::A11yTextAction::SetValue(text) => To::SetValue(text),
        };
        // A node may refuse; the scenario says what it expects of that.
        live.scene.accessible_text_action(&live.engine, key, action);
        live.settle()
    }
}

impl Files for LjosBackend {
    fn text_file(&mut self, name: &str, text: &str) -> Result<()> {
        self.platform.write(name, text)?;
        // A text file may be a picture too (an SVG). Only the picture store
        // sees this copy: the platform's files stay writable.
        let url = format!("file:///{name}");
        match self.live.as_ref() {
            Some(live) => {
                ljos_quick::ImageStore::of(&live.engine).register_resource(url, text.as_bytes())
            }
            None => self.image_texts.push((url, text.as_bytes().to_vec())),
        }
        Ok(())
    }

    fn user_file(&mut self, path: &str, text: &str) -> Result<()> {
        self.platform
            .write(&format!("{}/{path}", fake_platform::HOME), text)
    }

    fn read_text_file(&mut self, name: &str) -> Result<Option<String>> {
        if let Some(live) = self.live.as_ref() {
            fake_platform::settle_writes(&live.engine);
        }
        self.platform.read(name)
    }

    // Scenario files live at `file:///<name>`, so a picture beside them
    // does too; nothing is written to disk.
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let url = format!("file:///{name}");
        match self.live.as_ref() {
            Some(live) => {
                ljos_platform::Platform::of(&live.engine).register_resource(&url, bytes.to_vec());
                ljos_quick::ImageStore::of(&live.engine).register_resource(url, bytes.to_vec());
            }
            None => self.resources.push((url, bytes.to_vec())),
        }
        Ok(())
    }
}

impl Application for LjosBackend {
    fn open_window(&mut self, name: &str) -> Result<()> {
        self.live()?.open_window(name)
    }

    fn put_in_window(&mut self, name: &str, old: &str) -> Result<()> {
        self.live()?.put_in_window(name, old)
    }

    fn watch_roots(&mut self, roots: &[String], entries: &[String]) -> Result<()> {
        self.live()?.watch_roots(roots, entries, &[])
    }

    fn watch_roots_aliased(
        &mut self,
        roots: &[String],
        entries: &[String],
        aliases: &[(String, String)],
    ) -> Result<()> {
        self.live()?.watch_roots(roots, entries, aliases)
    }

    fn save_on_disk(&mut self, name: &str, text: Option<&str>) -> Result<()> {
        self.live()?.save_on_disk(name, text)
    }

    fn open_window_given(&mut self, name: &str, title: &str) -> Result<()> {
        self.live()?.open_window_given(name, title)
    }

    fn given_title(&mut self) -> Result<String> {
        let live = self.live()?;
        let given = live
            .given
            .ok_or_else(|| failed("the application gave no window a Document"))?;
        fixtures::title(&live.engine, given).map_err(failed)
    }

    fn lazy_tree(&mut self, name: &str, delay_ms: u64, rows: &[(String, String)]) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "a tree model is given to the QML before it is loaded",
            ));
        }
        let tree = lazy::LazyTree::new(rows, Duration::from_millis(delay_ms));
        self.lazy_trees.push((name.to_owned(), tree));
        Ok(())
    }

    fn paged_model(&mut self, name: &str, total: usize, page: usize, delay_ms: u64) -> Result<()> {
        if self.live.is_some() {
            return Err(failed(
                "a list model is given to the QML before it is loaded",
            ));
        }
        let model = paged::PagedModel::new(total, page, Duration::from_millis(delay_ms));
        self.paged_models.push((name.to_owned(), model));
        Ok(())
    }

    fn reopen(&mut self) -> Result<()> {
        let live = self.live()?;
        let engine = &live.engine;
        let sys = ljos_controls::WindowSystem::of(engine);
        let shown = sys
            .windows(engine)
            .iter()
            .any(|w| sys.spec(engine, *w).is_some_and(|s| s.visible));
        ljos_controls::application_reopened(engine, shown);
        live.settle()
    }

    fn quit_requested(&mut self) -> Result<bool> {
        Ok(self.live()?.engine.quit_requested())
    }

    fn panic_in(&mut self, object: &str, message: &str) -> Result<()> {
        let live = self.live()?;
        let obj = live.find(object)?;
        let message = message.to_owned();
        // A turn of the object's own code, as a host runs one: the panic
        // is caught where the turn ends.
        let _: Option<()> = live
            .engine
            .turn(Some(obj), move || std::panic::panic_any(message));
        live.settle()
    }

    fn application_fault(&mut self) -> Result<Option<ApplicationFault>> {
        let live = self.live()?;
        Ok(live.engine.application_fault().map(|f| ApplicationFault {
            kind: f.kind().to_owned(),
            message: f.message(),
        }))
    }
}

impl Reload for LjosBackend {
    fn watch_files(&mut self) {
        self.watching = true;
    }

    fn change_file(&mut self, name: &str, source: Option<&str>) -> Result<()> {
        self.live()?.change_file(name, source)
    }

    fn empty_file(&mut self, name: &str) -> Result<()> {
        self.live()?.empty_file(name)
    }

    fn finish_save(&mut self, _name: &str) -> Result<()> {
        self.live()?.finish_save()
    }

    fn change_resource(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let url = format!("file:///{name}");
        let live = self.live()?;
        ljos_platform::Platform::of(&live.engine).register_resource(&url, bytes.to_vec());
        ljos_quick::ImageStore::of(&live.engine).register_resource(url.clone(), bytes.to_vec());
        live.change_resource(url)
    }

    fn reload(&mut self) -> Result<()> {
        self.live()?.reload_flush()?;
        self.note_reload();
        Ok(())
    }

    fn reload_error(&mut self) -> Result<Option<super::ReloadFailure>> {
        let fault = self.live()?.engine.last_reload_error();
        Ok(fault.map(|f| super::ReloadFailure {
            file: f.file,
            line: f.line,
            message: f.message,
        }))
    }

    fn reload_count(&mut self) -> Result<u64> {
        self.live()?.reload_count()
    }

    fn change_theme(&mut self, name: &str, text: Option<&str>) -> Result<()> {
        match &mut self.live {
            Some(live) => live.change_theme(name, text),
            None => {
                self.theme = text.map(|t| (name.to_owned(), t.to_owned()));
                Ok(())
            }
        }
    }

    fn reload_generation(&mut self) -> Result<u64> {
        Ok(self.live()?.engine.reload_generation())
    }
}

impl Probes for LjosBackend {
    fn compiled_code(&mut self, file: &str) -> Result<(bool, u64)> {
        let live = self.live()?;
        let source = live
            .engine
            .component_source()
            .ok_or_else(|| failed("no files are given"))?;
        let url = source.url(key_of(file));
        let ran = live.engine.aot_file(&url).unwrap_or(false);
        Ok((ran, live.engine.aot_stats().runs))
    }

    fn scheduled(&mut self) -> Result<Vec<String>> {
        let live = self.live()?;
        let now = live.clock.now();
        let mut what = Vec::new();
        if let Some(t) = live.engine.next_timer_deadline() {
            what.push(format!(
                "a timer in {} ms",
                t.saturating_since(now).as_millis()
            ));
        }
        if let Some(t) = live.drive.wake {
            what.push(format!(
                "a frame in {} ms",
                t.saturating_since(now).as_millis()
            ));
        }
        let pending = ljos_platform::Platform::of(&live.engine).pending();
        if pending > 0 {
            what.push(format!("{pending} answers from the platform"));
        }
        Ok(what)
    }

    fn first_frame(&mut self) -> Result<()> {
        let live = self.live()?;
        // That the frame waits is the behaviour; how long a desktop host
        // lets it is a budget a debug build on a busy runner can miss.
        ljos_quick::ImageStore::of(&live.engine)
            .wait_for_frame(&live.engine, Duration::from_secs(30));
        live.settle()
    }

    fn kept_picture(&mut self, object: &str) -> Result<Option<(u32, u32)>> {
        let live = self.live()?;
        let obj = live.find(object)?;
        Ok(ljos_quick::kept_pixels(&live.engine, obj))
    }

    fn design_audit(&mut self, matrix: Option<&str>) -> Result<Value> {
        if matrix.is_none()
            && let Some(kept) = &self.audited
        {
            return Ok(kept.clone());
        }
        let live = self.live()?;
        let options = ljos_audit::Options {
            matrix: matrix
                .unwrap_or("current")
                .parse()
                .map_err(|e| failed(format!("{e}")))?,
            // Within an input's grace, what moved since is its doing.
            before: live.rest.clone().filter(|_| live.rest_due.is_none()),
            // The scenario's application is the test's own: acting on it
            // is what the acting checks' scenarios are about.
            act: true,
            ..ljos_audit::Options::desktop()
        };
        live.settle()?;
        let now = live.clock.now();
        let report = ljos_audit::audit(&live.engine, &mut live.scene, now, &options);
        // The acting checks framed the scene later than `now`; the
        // scenario's clock goes on from there rather than back.
        live.clock.set(report.until);
        live.settle()?;
        let report = report.json();
        if matrix.is_some() {
            self.audited = Some(report.clone());
        }
        Ok(report)
    }

    fn count_text_measures(&mut self) -> Result<()> {
        self.live()?.settle()?;
        self.measures_from = Some(ljos_quick::texts_measured());
        Ok(())
    }

    fn text_measures_counted(&mut self) -> Result<u64> {
        let from = self
            .measures_from
            .ok_or_else(|| BackendError::Failed("text measures were not being counted".into()))?;
        self.live()?.settle()?;
        Ok(ljos_quick::texts_measured() - from)
    }

    fn pointer_cursor(&mut self) -> Result<i64> {
        // A shape that changed under the pointer shows at the frame.
        self.live()?.settle()?;
        Ok(i64::from(self.live()?.scene.cursor_shape()))
    }
}

impl Tooling for LjosBackend {
    fn run_tests(
        &mut self,
        files: &[QmlFile],
        run: &super::testrun::TestRun,
    ) -> Result<super::testrun::TestReport> {
        super::testrun::run_ljos(files, run)
    }

    fn inspector(&mut self, ask: &super::inspector::Ask) -> Result<()> {
        self.ask_inspector(ask)
    }

    fn inspector_seen(&mut self) -> Result<super::inspector::Seen> {
        self.seen_by_inspector()
    }

    fn top(&mut self, shown: bool) -> Result<()> {
        self.show_top(shown)
    }

    fn top_seen(&mut self) -> Result<super::top::TopSeen> {
        self.top_as_seen()
    }
}

/// The default font with the attributes a scenario names.
fn application_font(attrs: &[(String, String)]) -> Result<ljos_engine::Font> {
    let mut font = ljos_engine::Font::default();
    let bad = |k: &str, v: &str| BackendError::Failed(format!("{v:?} is not a font's {k}"));
    for (k, v) in attrs {
        let number = || v.parse::<f64>().map_err(|_| bad(k, v));
        let flag = || v.parse::<bool>().map_err(|_| bad(k, v));
        match k.as_str() {
            "family" => font.family = ljos_engine::Atom::from(v.as_str()),
            "pointSize" => {
                font.point_size = number()?;
                font.pixel_size = -1;
            }
            "pixelSize" => {
                font.pixel_size = number()? as i32;
                font.point_size = -1.0;
            }
            "weight" => font.weight = number()? as i32,
            "bold" => font.weight = if flag()? { 700 } else { 400 },
            "italic" => font.italic = flag()?,
            _ => return Err(BackendError::Failed(format!("a font has no {k:?}"))),
        }
    }
    Ok(font)
}

/// The clock of `Given JavaScript is timed by a clock ...`: a poll of the
/// interrupt moves it ten milliseconds, native work by `native`.
fn set_js_clock(engine: &Engine, native: Duration) -> Result<()> {
    use ljos_engine::JsTiming;
    let now = Rc::new(Cell::new(std::time::Instant::now()));
    engine
        .set_js_clock(Some(Rc::new(move |t| {
            match t {
                JsTiming::Poll => now.set(now.get() + Duration::from_millis(10)),
                JsTiming::NativeEnd => now.set(now.get() + native),
                _ => {}
            }
            now.get()
        })))
        .map_err(|e| failed(format!("the JS clock is not set: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, source: &str) -> QmlFile {
        QmlFile {
            name: name.into(),
            source: source.into(),
        }
    }

    fn loaded(source: &str) -> LjosBackend {
        let mut b = LjosBackend::new();
        b.load(&[file("Main.qml", source)], "Main.qml")
            .expect("Main.qml loads");
        b
    }

    const BINDING: &str = r#"import QtQuick
Item {
    objectName: "root"
    property int a: 1
    property int b: a + 1
    function twice(n) { return n * 2 }
    Item { objectName: "kid"; width: parent.width / 2 }
}
"#;

    #[test]
    fn a_heavy_handler_finishes_whatever_the_machines_speed() {
        // A handler that does a quarter of a second of wall-clock work in
        // one dispatch (a Canvas paint that reads its pixels back in a
        // loop does more, in a debug build) must run to its last line:
        // the budget of a frame-driven host would stop it mid-handler on
        // a slow machine and the scenario's later properties would keep
        // their defaults.
        let mut b = loaded(
            r#"import QtQuick
Item {
    objectName: "root"
    property bool done: false
    function work() {
        var until = Date.now() + 250
        while (Date.now() < until) { }
        done = true
    }
}
"#,
        );
        b.call("root", "work", &[]).unwrap();
        assert_eq!(b.get("root", "done").unwrap(), json!(true));
    }

    #[test]
    fn nothing_works_before_a_load() {
        let mut b = LjosBackend::new();
        assert!(matches!(b.get("root", "a"), Err(BackendError::Failed(_))));
        assert!(b.log().unwrap().is_empty());
    }

    #[test]
    fn bindings_follow_and_break() {
        let mut b = loaded(BINDING);
        assert_eq!(b.get("root", "b").unwrap(), json!(2));
        b.set("root", "a", &json!(5)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(6));
        b.set("root", "b", &json!(100)).unwrap();
        b.set("root", "a", &json!(8)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(100));
    }

    #[test]
    fn bind_installs_a_binding_over_the_objects_properties() {
        let mut b = loaded(BINDING);
        b.bind("root", "b", "a * 10").unwrap();
        b.set("root", "a", &json!(3)).unwrap();
        assert_eq!(b.get("root", "b").unwrap(), json!(30));
    }

    #[test]
    fn methods_are_called_with_arguments() {
        let mut b = loaded(BINDING);
        assert_eq!(b.call("root", "twice", &[json!(21)]).unwrap(), json!(42));
    }

    #[test]
    fn roots_are_children_of_a_400_pixel_driver() {
        let mut b = loaded(BINDING);
        assert_eq!(b.get("root", "parent.width").unwrap(), json!(400));
        assert!(b.exists("root/kid").unwrap());
        assert!(!b.exists("root/nobody").unwrap());
    }

    #[test]
    fn unknown_properties_are_rejected() {
        let mut b = loaded(BINDING);
        assert!(matches!(
            b.set("root", "nothing", &json!(1)),
            Err(BackendError::Failed(_))
        ));
    }

    #[test]
    fn syntax_errors_reject_the_load() {
        let mut b = LjosBackend::new();
        let r = b.load(
            &[file(
                "Broken.qml",
                "import QtQuick\nItem { property int p: }\n",
            )],
            "Broken.qml",
        );
        assert!(matches!(r, Err(BackendError::Failed(_))), "{r:?}");
    }

    #[test]
    fn colours_read_as_lower_case_hex() {
        let mut b = loaded("import QtQuick\nRectangle { objectName: \"r\"; color: \"#0000FF\" }\n");
        assert_eq!(b.get("r", "color").unwrap(), json!("#0000ff"));
    }

    #[test]
    fn members_of_value_types_are_read_from_a_copy() {
        let mut b = loaded(
            r#"import QtQuick
Item {
    objectName: "root"
    property rect area: Qt.rect(1, 2, 30, 40)
}
"#,
        );
        assert_eq!(b.get("root", "area.width").unwrap(), json!(30));
        assert_eq!(b.get("root", "area.nothing").unwrap(), Value::Null);
    }

    #[test]
    fn time_is_virtual_and_fires_timers_at_their_deadlines() {
        let mut b = loaded(
            r#"import QtQuick
Item {
    objectName: "root"
    property int ticks: 0
    Timer { interval: 100; repeat: true; running: true; onTriggered: root.ticks++ }
    id: root
}
"#,
        );
        b.advance(350).unwrap();
        assert_eq!(b.get("root", "ticks").unwrap(), json!(3));
    }

    #[test]
    fn binding_loops_reach_the_log_as_warnings() {
        let mut b =
            loaded("import QtQuick\nItem { property int p: q + 1; property int q: p + 1 }\n");
        let log = b.log().unwrap();
        assert!(
            log.iter().any(
                |e| e.level == LogLevel::Warning && e.message.contains("Binding loop detected")
            ),
            "{log:?}"
        );
    }

    #[test]
    fn touches_reach_mouse_areas() {
        // The MouseArea is the root, so the test does not depend on how
        // declared children join the item tree.
        let mut b = loaded(
            r#"import QtQuick
MouseArea {
    objectName: "root"
    width: 100; height: 100
    property int clicks: 0
    onClicked: clicks++
}
"#,
        );
        b.touch(10.0, 10.0).unwrap();
        b.touch(150.0, 150.0).unwrap();
        assert_eq!(b.get("root", "clicks").unwrap(), json!(1));
    }

    #[test]
    fn grab_renders_the_window() {
        let mut b =
            loaded("import QtQuick\nRectangle { width: 10; height: 10; color: \"#ff0000\" }\n");
        let png = b.grab().unwrap();
        let image = ljos_render::RgbaImage::from_png(&png).unwrap();
        assert_eq!((image.width, image.height), (400, 400));
        assert_eq!(image.pixel(5, 5), [255, 0, 0, 255]);
        assert_eq!(image.pixel(50, 50)[3], 0);
    }

    const TOOLS: &str = r#"import QtQuick
Item {
    objectName: "root"
    width: 200; height: 100
    property int clicks: 0
    Item {
        objectName: "tools"
        x: 10; y: 10; width: 180; height: 40
        Accessible.role: Accessible.ToolBar
        Accessible.name: "Tools"
        Rectangle {
            objectName: "save"
            x: 5; y: 5; width: 80; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "Save"
            Accessible.description: "Writes the file"
            MouseArea { anchors.fill: parent; onClicked: root.clicks++ }
        }
    }
    id: root
}
"#;

    #[test]
    fn the_accessibility_tree_has_roles_names_and_nesting() {
        let mut b = loaded(TOOLS);
        let tree = b.a11y_tree().unwrap();
        assert_eq!(tree.role, "window");
        let save = tree.find("button", "Save").expect("the button");
        assert_eq!(save.object_name, "save");
        assert_eq!(save.description, "Writes the file");
        assert!(
            tree.parent_of("button", "Save")
                .unwrap()
                .is("toolbar", "Tools")
        );
        // The plain root item is not a node.
        assert_eq!(tree.children.len(), 1);
    }

    #[test]
    fn attached_properties_are_set_and_read_by_their_type_name() {
        let mut b = loaded(TOOLS);
        assert_eq!(b.get("save", "Accessible.name").unwrap(), json!("Save"));
        b.set("save", "Accessible.name", &json!("Store")).unwrap();
        let tree = b.a11y_tree().unwrap();
        assert!(tree.find("button", "Store").is_some());
        assert!(tree.find("button", "Save").is_none());
        // Once read, the tree follows later changes too.
        b.set("save", "Accessible.ignored", &json!(true)).unwrap();
        assert!(b.a11y_tree().unwrap().find("button", "Store").is_none());
    }

    #[test]
    fn clicks_find_the_control_by_role_and_name() {
        let mut b = loaded(TOOLS);
        b.click("button", "Save").unwrap();
        assert_eq!(b.get("root", "clicks").unwrap(), json!(1));
        assert!(matches!(
            b.click("button", "Open"),
            Err(BackendError::Failed(_))
        ));
    }

    const KEYS: &str = r#"import QtQuick
Item {
    objectName: "root"
    focus: true
    property string log: ""
    property bool lit: activeFocus
    Keys.onPressed: (event) => log += event.key + (event.isAutoRepeat ? "r " : " ")
    Keys.onReleased: (event) => log += "up "
}
"#;

    #[test]
    fn keys_reach_the_focus_item() {
        let mut b = loaded(KEYS);
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up "));
        b.key_hold("B", 2).unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up 66 66r 66r up "));
    }

    #[test]
    fn an_inactive_window_gets_no_keys() {
        let mut b = loaded(KEYS);
        assert_eq!(b.get("root", "lit").unwrap(), json!(true));
        b.window_active(false).unwrap();
        assert_eq!(b.get("root", "lit").unwrap(), json!(false));
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!(""));
        b.window_active(true).unwrap();
        b.key("A").unwrap();
        assert_eq!(b.get("root", "log").unwrap(), json!("65 up "));
    }
}
