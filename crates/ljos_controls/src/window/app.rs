//! `Application`, `Qt.application` and the screens
//! (doc.qt.io/qt-6/qml-qtquick-application.html,
//! doc.qt.io/qt-6/qml-qtquick-screen.html).
//!
//! What they say comes from the host: [`super::WindowSystem`] is told the
//! displays and whether the application has the keyboard. Without a host
//! there is one screen of 800 by 800 and the application is active, which
//! is what the oracle's offscreen platform reports.

use ljos_engine::{
    Atom, Engine, EngineError, Font, ObjectId, QmlList, QmlObject, QmlType, QtObject, Value,
    WriteKind, qml_impl, qml_singleton,
};

use super::system::WindowSystem;

/// `Qt.ApplicationState`: how much of the application its user has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApplicationState {
    Suspended,
    /// Running without a visible window.
    Hidden,
    /// Visible, but no window of it has the keyboard.
    Inactive,
    #[default]
    Active,
}

impl ApplicationState {
    /// The number Qt gives the state.
    pub fn qt_value(self) -> i32 {
        match self {
            ApplicationState::Suspended => 0,
            ApplicationState::Hidden => 1,
            ApplicationState::Inactive => 2,
            ApplicationState::Active => 4,
        }
    }
}

/// A display as a host describes it. Sizes are in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenSpec {
    pub name: String,
    pub size: [f64; 2],
    /// Of its top left corner among the displays.
    pub position: [f64; 2],
    /// Physical pixels per logical pixel.
    pub scale: f64,
}

impl Default for ScreenSpec {
    fn default() -> Self {
        ScreenSpec {
            name: String::new(),
            size: [800.0, 800.0],
            position: [0.0, 0.0],
            scale: 1.0,
        }
    }
}

/// What `Window.screen` and `Application.screens` hold: one display.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ScreenInfo",
    extends = QtObject,
    uncreatable
)]
pub struct ScreenInfo {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub name: Atom,
    #[qml(notify, readonly)]
    pub width: i32,
    #[qml(notify, readonly)]
    pub height: i32,
    #[qml(notify, readonly)]
    pub desktop_available_width: i32,
    #[qml(notify, readonly)]
    pub desktop_available_height: i32,
    #[qml(notify, readonly)]
    pub virtual_x: i32,
    #[qml(notify, readonly)]
    pub virtual_y: i32,
    #[qml(notify, readonly)]
    pub device_pixel_ratio: f64,
    /// Physical pixels per millimetre.
    #[qml(notify, readonly)]
    pub pixel_density: f64,
    /// Logical pixels per millimetre.
    #[qml(notify, readonly)]
    pub logical_pixel_density: f64,
    #[qml(notify, readonly, enumeration = "Qt.ScreenOrientation")]
    pub orientation: i32,
    #[qml(notify, readonly, enumeration = "Qt.ScreenOrientation")]
    pub primary_orientation: i32,
    #[qml(notify, readonly)]
    pub manufacturer: Atom,
    #[qml(notify, readonly)]
    pub model: Atom,
    #[qml(notify, readonly)]
    pub serial_number: Atom,
}

/// What a host said of a display, as property values.
fn screen_values(spec: &ScreenSpec) -> [(ljos_engine::PropIndex, Value); 10] {
    let [w, h] = spec.size.map(|v| v.round() as i32);
    let scale = if spec.scale > 0.0 { spec.scale } else { 1.0 };
    [
        (
            ScreenInfo::NAME.index,
            Value::String(Atom::from(spec.name.as_str())),
        ),
        (ScreenInfo::WIDTH.index, Value::Int(w)),
        (ScreenInfo::HEIGHT.index, Value::Int(h)),
        (ScreenInfo::DESKTOP_AVAILABLE_WIDTH.index, Value::Int(w)),
        (ScreenInfo::DESKTOP_AVAILABLE_HEIGHT.index, Value::Int(h)),
        (
            ScreenInfo::VIRTUAL_X.index,
            Value::Int(spec.position[0].round() as i32),
        ),
        (
            ScreenInfo::VIRTUAL_Y.index,
            Value::Int(spec.position[1].round() as i32),
        ),
        (ScreenInfo::DEVICE_PIXEL_RATIO.index, Value::Real(scale)),
        // 96 logical pixels to the inch, the desktop's convention, when
        // the display does not say how large it is. The logical density
        // is the platform's logical DPI, as points are (72 on macOS).
        (
            ScreenInfo::PIXEL_DENSITY.index,
            Value::Real(96.0 / 25.4 * scale),
        ),
        (
            ScreenInfo::LOGICAL_PIXEL_DENSITY.index,
            Value::Real(ljos_runtime::Platform::CURRENT.logical_dpi() / 25.4),
        ),
    ]
}

/// Makes the object of a display.
pub(super) fn make_screen(engine: &Engine, spec: &ScreenSpec) -> Result<ObjectId, EngineError> {
    let landscape = spec.size[0] >= spec.size[1];
    let obj = engine.create_native_incomplete(
        ScreenInfo {
            // Qt.LandscapeOrientation or Qt.PortraitOrientation.
            primary_orientation: if landscape { 2 } else { 1 },
            ..ScreenInfo::default()
        },
        None,
    )?;
    update_screen(engine, obj, spec);
    Ok(obj)
}

/// Gives the object of a display what the host says now.
pub(super) fn update_screen(engine: &Engine, obj: ObjectId, spec: &ScreenSpec) {
    engine.batch(|| {
        for (prop, value) in screen_values(spec) {
            let _ = engine.write_internal(obj, prop, value, WriteKind::Imperative);
        }
    });
}

/// `Application` (QtQuick), and `Qt.application`.
///
/// One for the engine. `Application.onStateChanged` as an attached
/// handler is not there: connect to the singleton.
///
/// `quitOnLastWindowClosed` and `reopened` are Ljós's: Qt has the first
/// on `QGuiApplication` only, and nothing in QML for the second
/// (features/window/keep-running.feature).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Application")]
#[qml_singleton]
pub struct Application {
    #[qml(notify, readonly, enumeration = "Qt.ApplicationState")]
    pub state: i32,
    #[qml(notify, readonly)]
    pub active: bool,
    /// The program's file name without its extension until something
    /// names the application, as Qt's.
    #[qml(notify)]
    pub name: Atom,
    /// The name, until something gives the application a display name of
    /// its own (features/window/application-name.feature).
    #[qml(notify)]
    pub display_name: Atom,
    #[qml(notify)]
    pub version: Atom,
    #[qml(notify)]
    pub organization: Atom,
    #[qml(notify)]
    pub domain: Atom,
    #[qml(notify, readonly, enumeration = "Qt.LayoutDirection")]
    pub layout_direction: i32,
    /// The font text has when nothing sets one, which the host can change
    /// while the application runs (`set_application_font`). Qt's has no
    /// signal of its change, and a binding that reads it follows it anyway.
    #[qml(readonly)]
    pub font: Font,
    #[qml(notify, list)]
    pub screens: QmlList,
    /// What the program was started with; a host fills it.
    #[qml(notify)]
    pub arguments: Vec<Value>,
    /// `Qt.styleHints`: the same object, made when the engine starts.
    #[qml(readonly)]
    pub style_hints: Option<ObjectId>,
    /// Whether the host ends the application when its last window has
    /// closed, as `QGuiApplication::quitOnLastWindowClosed`. Without it
    /// the application runs on with no window until `Qt.quit()`.
    #[qml(notify)]
    pub quit_on_last_window_closed: bool,
}

#[qml_impl]
impl Application {
    /// The user opened the application again while it runs: on macOS a
    /// click on its icon in the Dock. With no window shown it is the time
    /// to show one.
    #[qml(signal)]
    fn reopened(has_visible_windows: bool);
}

impl Default for Application {
    fn default() -> Self {
        Application {
            state: ApplicationState::Active.qt_value(),
            active: true,
            name: program_name(),
            display_name: program_name(),
            version: Atom::default(),
            organization: Atom::default(),
            domain: Atom::default(),
            layout_direction: 0,
            font: Font::default(),
            screens: QmlList::default(),
            arguments: Vec::new(),
            style_hints: None,
            quit_on_last_window_closed: true,
        }
    }
}

/// What the application is called before anything names it.
fn program_name() -> Atom {
    let exe = std::env::current_exe().ok();
    let stem = exe.as_deref().and_then(|p| p.file_stem()?.to_str());
    Atom::from(stem.unwrap_or("ljos"))
}

/// The host says what command line the application was started with,
/// the program first: `Application.arguments`. Before the root is made,
/// so that it reads them when it completes.
pub fn set_application_arguments(engine: &Engine, args: impl IntoIterator<Item = String>) {
    if let Ok(app) = engine.singleton::<Application>() {
        let args = args
            .into_iter()
            .map(|a| Value::String(Atom::from(a.as_str())))
            .collect();
        let _ = engine.write_internal(
            app,
            Application::ARGUMENTS.index,
            Value::list(args),
            WriteKind::Imperative,
        );
    }
}

/// The application font is now `font` (`set_application_font`).
pub(crate) fn application_font_changed(engine: &Engine, font: std::sync::Arc<Font>) {
    if let Ok(app) = engine.untrack(|| engine.singleton::<Application>()) {
        let _ = engine.write_internal(
            app,
            Application::FONT.index,
            Value::Font(font),
            WriteKind::Imperative,
        );
    }
}

/// The host or the application says whether closing the last window ends
/// the application (`Qt.application.quitOnLastWindowClosed`).
pub fn set_quit_on_last_window_closed(engine: &Engine, quits: bool) {
    if let Ok(app) = engine.singleton::<Application>() {
        let _ = engine.write_internal(
            app,
            Application::QUIT_ON_LAST_WINDOW_CLOSED.index,
            Value::Bool(quits),
            WriteKind::Imperative,
        );
    }
}

/// Whether the host ends the application when its last window has
/// closed: what [`set_quit_on_last_window_closed`] or the application
/// said last, true when neither did.
pub fn quit_on_last_window_closed(engine: &Engine) -> bool {
    let Ok(app) = engine.singleton::<Application>() else {
        return true;
    };
    let quits = engine.untrack(|| engine.read(app, Application::QUIT_ON_LAST_WINDOW_CLOSED.index));
    !matches!(quits, Value::Bool(false))
}

/// The host says the user opened the application again
/// (`Qt.application.reopened`), and whether a window of it was shown.
pub fn application_reopened(engine: &Engine, has_visible_windows: bool) {
    if let Ok(app) = engine.singleton::<Application>() {
        let _ = engine.emit_by_name(app, "reopened", &[Value::Bool(has_visible_windows)]);
    }
}

/// Puts the singleton on the `Qt` global as well.
const INSTALL: &str = r#"(function (app) {
  Object.defineProperty(Qt, "application", { value: app, enumerable: true });
})"#;

pub(super) fn register(engine: &Engine) {
    engine.register_type(
        "QtQuick",
        ljos_engine::Version::new(2, 15),
        ljos_engine::MetaRef::Static(ScreenInfo::static_meta()),
    );
    engine.register_type(
        "QtQuick",
        ljos_engine::Version::new(2, 15),
        ljos_engine::MetaRef::Static(super::screen::Screen::static_meta()),
    );
    super::screen::register(engine);
    engine.register_singleton::<Application>("QtQuick", ljos_engine::Version::new(2, 15));
    // Made here and not on first use: a native singleton has no hook that
    // runs when it is made, and what it says of the screens must be there
    // for whoever reads it first.
    if let Ok(app) = engine.singleton::<Application>() {
        WindowSystem::of(engine).adopt_application(engine, app);
        // A binding nobody wrote: a write from QML replaces it for good.
        let _ = engine.bind_computed(app, Application::DISPLAY_NAME.index, move |e| {
            e.read(app, Application::NAME.index)
        });
        if let Some(hints) = ljos_quick::style_hints(engine) {
            let _ = engine.write_internal(
                app,
                Application::STYLE_HINTS.index,
                Value::Object(hints),
                WriteKind::Imperative,
            );
        }
        // The direction is the UI language's at every moment
        // (features/i18n/layout-direction.feature), and this object is
        // what `Qt.application` is from here on.
        let follows = engine.effect_in(app, move |e| {
            e.track_translation();
            // Qt.LeftToRight is 0 and Qt.RightToLeft 1.
            let direction = Value::Int(i32::from(e.is_right_to_left()));
            e.untrack(|| {
                let _ = e.write_internal(
                    app,
                    Application::LAYOUT_DIRECTION.index,
                    direction,
                    WriteKind::Imperative,
                );
            });
        });
        if let Err(err) = follows {
            engine.log(
                ljos_engine::LogLevel::Warning,
                crate::util::CATEGORY,
                format!("Application.layoutDirection does not follow the language: {err}"),
            );
        }
    }
    engine.on_js_started(|e| {
        let installed = e
            .singleton::<Application>()
            .and_then(|app| Ok((app, e.eval_js(INSTALL)?)));
        match installed {
            Ok((app, install)) => {
                let _ = e.call_js(&install, &[Value::Object(app)]);
            }
            Err(err) => e.log(
                ljos_engine::LogLevel::Warning,
                crate::util::CATEGORY,
                format!("Qt.application is not available: {err}"),
            ),
        }
    });
}
