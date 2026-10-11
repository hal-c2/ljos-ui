//! What the system prefers, as values a binding can follow: `Qt.styleHints`
//! and `Qt.styleHints.accessibility` (ARCHITECTURE §14, `SystemPrefs`).
//!
//! A host hands over a [`SystemPrefs`] when it starts and again when the
//! platform says a setting changed ([`set_system_prefs`],
//! [`update_system_prefs`]). Each value is a property of one of two objects
//! the engine keeps, so QML reads it like any other property and Rust reads
//! it with the tracked getters here. Nothing polls: a value changes when a
//! host says so, and what read it runs again.
//!
//! The colour scheme is the one value an application may set
//! (`Qt.styleHints.colorScheme = Qt.ColorScheme.Dark`). What it asked for
//! stays while the system's scheme changes underneath, until it asks for
//! `Qt.ColorScheme.Unknown`, which is "the system's again". Qt's
//! documentation says so; the platform of the oracle keeps the last scheme
//! instead (features/a11y/style-hints.feature).

use std::cell::Cell;
use std::rc::Rc;

use ljos_a11y::{ColorScheme, Contrast, SystemPrefs};
use ljos_engine::{Engine, Interception, ObjectId, QmlObject, Value};
use ljos_reactive::WriteKind;

/// `Qt.styleHints`. Of what Qt's has, the part an application adapts its
/// looks to.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "StyleHints", uncreatable)]
pub struct StyleHints {
    /// A `Qt.ColorScheme`: the application's own when it asked for one,
    /// else the system's.
    #[qml(notify)]
    pub color_scheme: i32,
    /// The [`AccessibilityHints`], one object for as long as the engine
    /// lives.
    #[qml(constant)]
    pub accessibility: Option<ObjectId>,
}

/// `Qt.styleHints.accessibility`. `contrastPreference` is Qt's (6.10),
/// `motionPreference` is documented for Qt 6.12; `fontScale` and
/// `screenReaderActive` are ours.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "AccessibilityHints", uncreatable)]
pub struct AccessibilityHints {
    /// A `Qt.ContrastPreference`.
    #[qml(notify, readonly)]
    pub contrast_preference: i32,
    /// A `Qt.MotionPreference`.
    #[qml(notify, readonly)]
    pub motion_preference: i32,
    /// How much larger than written the user wants text.
    #[qml(notify, readonly)]
    pub font_scale: f64,
    #[qml(notify, readonly)]
    pub screen_reader_active: bool,
}

/// Per engine.
struct State {
    /// The two objects, made on first use.
    objects: Cell<Option<(ObjectId, ObjectId)>>,
    /// What the host said last.
    system: Cell<SystemPrefs>,
    /// The scheme the application asked for, while it asks for one.
    asked: Cell<Option<ColorScheme>>,
}

impl Default for State {
    fn default() -> Self {
        State {
            objects: Cell::new(None),
            system: Cell::new(SystemPrefs::default()),
            asked: Cell::new(None),
        }
    }
}

fn state(engine: &Engine) -> Rc<State> {
    engine.extension::<State>()
}

/// The scheme `Qt.styleHints.colorScheme` reads.
fn shown_scheme(s: &State) -> ColorScheme {
    s.asked.get().unwrap_or(s.system.get().color_scheme)
}

/// `(Qt.styleHints, Qt.styleHints.accessibility)`, made on first use.
fn objects(engine: &Engine) -> Option<(ObjectId, ObjectId)> {
    let s = state(engine);
    if let Some(found) = s.objects.get().filter(|(h, _)| engine.is_alive(*h)) {
        return Some(found);
    }
    let p = s.system.get();
    let a11y = engine
        .create_native(
            AccessibilityHints {
                contrast_preference: p.contrast.qt_value(),
                motion_preference: i32::from(p.reduced_motion),
                font_scale: p.font_scale,
                screen_reader_active: p.screen_reader_active,
            },
            None,
        )
        .ok()?;
    let hints = engine
        .create_native(
            StyleHints {
                color_scheme: shown_scheme(&s).qt_value(),
                accessibility: Some(a11y),
            },
            None,
        )
        .ok()?;
    // Every write an application makes comes through here; the system's
    // own go in underneath (`write_internal`).
    let weak = Rc::downgrade(&s);
    engine.set_interceptor(
        hints,
        StyleHints::COLOR_SCHEME.index,
        Some(Rc::new(move |e: &Engine, v: &Value, _: Interception| {
            let Some(s) = weak.upgrade() else {
                return false;
            };
            match ColorScheme::from_qt(v.to_number() as i32) {
                ColorScheme::Unknown => {
                    s.asked.set(None);
                    show_scheme(e, hints, s.system.get().color_scheme);
                    true
                }
                asked => {
                    s.asked.set(Some(asked));
                    show_scheme(e, hints, asked);
                    true
                }
            }
        })),
    );
    s.objects.set(Some((hints, a11y)));
    Some((hints, a11y))
}

fn show_scheme(engine: &Engine, hints: ObjectId, scheme: ColorScheme) {
    let _ = engine.write_internal(
        hints,
        StyleHints::COLOR_SCHEME.index,
        Value::Int(scheme.qt_value()),
        WriteKind::Imperative,
    );
    engine.request_wake();
}

fn show(engine: &Engine, s: &State, p: SystemPrefs) {
    let Some((hints, a11y)) = s.objects.get().filter(|(h, _)| engine.is_alive(*h)) else {
        // Nobody has read one yet: the objects start out with these.
        return;
    };
    let set = |obj, prop: ljos_engine::PropIndex, v| {
        let _ = engine.write_internal(obj, prop, v, WriteKind::Imperative);
    };
    engine.batch(|| {
        set(
            hints,
            StyleHints::COLOR_SCHEME.index,
            Value::Int(shown_scheme(s).qt_value()),
        );
        set(
            a11y,
            AccessibilityHints::CONTRAST_PREFERENCE.index,
            Value::Int(p.contrast.qt_value()),
        );
        set(
            a11y,
            AccessibilityHints::MOTION_PREFERENCE.index,
            Value::Int(i32::from(p.reduced_motion)),
        );
        set(
            a11y,
            AccessibilityHints::FONT_SCALE.index,
            Value::Real(p.font_scale),
        );
        set(
            a11y,
            AccessibilityHints::SCREEN_READER_ACTIVE.index,
            Value::Bool(p.screen_reader_active),
        );
    });
}

/// Tells the application what the system prefers. A host calls this when
/// it starts and whenever the platform reports a change; a test calls it
/// to be the system. Telling the same again costs nothing.
pub fn set_system_prefs(engine: &Engine, prefs: SystemPrefs) {
    let s = state(engine);
    let prefs = SystemPrefs {
        font_scale: SystemPrefs::sane_font_scale(prefs.font_scale),
        ..prefs
    };
    if s.system.replace(prefs) == prefs {
        return;
    }
    show(engine, &s, prefs);
    // A frame: animations under way end (or go on) with the new setting,
    // and text is laid out at the new size.
    engine.request_wake();
}

/// Changes some of the settings and leaves the others as they are.
pub fn update_system_prefs(engine: &Engine, change: impl FnOnce(&mut SystemPrefs)) {
    let mut prefs = system_prefs(engine);
    change(&mut prefs);
    set_system_prefs(engine, prefs);
}

/// What the host said last. Not tracked: for a host that compares, and for
/// what looks once (an audit).
pub fn system_prefs(engine: &Engine) -> SystemPrefs {
    state(engine).system.get()
}

/// The colour scheme in effect: the application's own when it asked for
/// one, else the system's. Tracked.
pub fn color_scheme(engine: &Engine) -> ColorScheme {
    match objects(engine) {
        Some((hints, _)) => ColorScheme::from_qt(
            engine
                .read(hints, StyleHints::COLOR_SCHEME.index)
                .to_number() as i32,
        ),
        None => shown_scheme(&state(engine)),
    }
}

/// The system's contrast preference. Tracked.
pub fn contrast(engine: &Engine) -> Contrast {
    match objects(engine) {
        Some((_, a11y)) => Contrast::from_qt(
            engine
                .read(a11y, AccessibilityHints::CONTRAST_PREFERENCE.index)
                .to_number() as i32,
        ),
        None => system_prefs(engine).contrast,
    }
}

/// Whether the user asked for less motion. Tracked.
pub fn reduced_motion(engine: &Engine) -> bool {
    match objects(engine) {
        Some((_, a11y)) => {
            engine
                .read(a11y, AccessibilityHints::MOTION_PREFERENCE.index)
                .to_number()
                != 0.0
        }
        None => system_prefs(engine).reduced_motion,
    }
}

/// How much larger than written the user wants text. Tracked.
pub fn font_scale(engine: &Engine) -> f64 {
    match objects(engine) {
        Some((_, a11y)) => engine
            .read(a11y, AccessibilityHints::FONT_SCALE.index)
            .to_number(),
        None => system_prefs(engine).font_scale,
    }
}

/// Whether an assistive technology reads the application. Tracked.
pub fn screen_reader_active(engine: &Engine) -> bool {
    match objects(engine) {
        Some((_, a11y)) => engine
            .read(a11y, AccessibilityHints::SCREEN_READER_ACTIVE.index)
            .to_boolean(),
        None => system_prefs(engine).screen_reader_active,
    }
}

/// [`reduced_motion`] for the animation driver, which asks once per run
/// per frame and follows nothing: a change of the setting wakes it.
pub(crate) fn reduced_motion_now(engine: &Engine) -> bool {
    state(engine).system.get().reduced_motion
}

/// `Qt.styleHints` and the enums that go with it, put on the `Qt` global
/// when the JS host starts.
const INSTALL: &str = r#"(function (hints) {
  Object.defineProperty(Qt, "styleHints", { value: hints, enumerable: true });
  const scopes = {
    ColorScheme: { Unknown: 0, Light: 1, Dark: 2 },
    ContrastPreference: { NoPreference: 0, HighContrast: 1 },
    MotionPreference: { NoPreference: 0, ReducedMotion: 1 },
  };
  for (const name in scopes) {
    Qt[name] = Object.freeze(scopes[name]);
    // Qt's enums are keys of `Qt` as well (`Qt.Dark`). A key something
    // else has already is left alone.
    for (const key in scopes[name]) {
      if (!(key in Qt)) Qt[key] = scopes[name][key];
    }
  }
})"#;

pub(crate) fn register(engine: &Engine) {
    engine.on_js_started(|e| {
        let Some((hints, _)) = objects(e) else {
            return;
        };
        match e.eval_js(INSTALL) {
            Ok(install) => {
                let _ = e.call_js(&install, &[Value::Object(hints)]);
            }
            Err(err) => e.log(
                ljos_engine::LogLevel::Warning,
                crate::scene::SCENE_CATEGORY,
                format!("Qt.styleHints is not available: {err}"),
            ),
        }
    });
}
