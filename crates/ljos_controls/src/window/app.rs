//! `Application`, `Qt.application` and the screens
//! (doc.qt.io/qt-6/qml-qtquick-application.html,
//! doc.qt.io/qt-6/qml-qtquick-screen.html).
//!
//! What they say comes from the host: [`super::WindowSystem`] is told the
//! displays and whether the application has the keyboard. Without a host
//! there is one screen of 800 by 800 and the application is active, which
//! is what the oracle's offscreen platform reports.

use ljos_engine::{
    Atom, Engine, EngineError, ObjectId, QmlList, QmlObject, QmlType, QtObject, Value, WriteKind,
    qml_singleton,
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
fn screen_values(spec: &ScreenSpec) -> [(ljos_engine::PropIndex, Value); 9] {
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
        // the display does not say how large it is.
        (
            ScreenInfo::PIXEL_DENSITY.index,
            Value::Real(96.0 / 25.4 * scale),
        ),
    ]
}

/// Makes the object of a display.
pub(super) fn make_screen(engine: &Engine, spec: &ScreenSpec) -> Result<ObjectId, EngineError> {
    let landscape = spec.size[0] >= spec.size[1];
    let obj = engine.create_native(
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
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Application")]
#[qml_singleton]
pub struct Application {
    #[qml(notify, readonly, enumeration = "Qt.ApplicationState")]
    pub state: i32,
    #[qml(notify, readonly)]
    pub active: bool,
    #[qml(notify)]
    pub name: Atom,
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
    #[qml(notify, list)]
    pub screens: QmlList,
    /// What the program was started with; a host fills it.
    #[qml(notify)]
    pub arguments: Vec<Value>,
}

impl Default for Application {
    fn default() -> Self {
        Application {
            state: ApplicationState::Active.qt_value(),
            active: true,
            name: Atom::default(),
            display_name: Atom::default(),
            version: Atom::default(),
            organization: Atom::default(),
            domain: Atom::default(),
            layout_direction: 0,
            screens: QmlList::default(),
            arguments: Vec::new(),
        }
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
    engine.register_singleton::<Application>("QtQuick", ljos_engine::Version::new(2, 15));
    // Made here and not on first use: a native singleton has no hook that
    // runs when it is made, and what it says of the screens must be there
    // for whoever reads it first.
    if let Ok(app) = engine.singleton::<Application>() {
        WindowSystem::of(engine).adopt_application(engine, app);
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
