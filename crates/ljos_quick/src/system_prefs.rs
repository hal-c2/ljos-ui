//! What the system prefers, as values a binding can follow: `Qt.styleHints`
//! and `Qt.styleHints.accessibility` (ARCHITECTURE §14, `SystemPrefs`).
//!
//! A host reports what its platform says when it starts and again when the
//! platform says a setting changed ([`report_system_prefs`]). An
//! application or a test sets a value itself ([`set_system_prefs`],
//! [`update_system_prefs`]), and what it set stays over the host's reports
//! until it sets the host's value again. Each value is a property of one of two objects
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

use ljos_a11y::{ColorScheme, Contrast, HostPrefs, SystemPrefs};
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
    /// How far a pointer goes from where it pressed, in logical pixels,
    /// before the press is a drag. Every drag in Ljós starts from it: a
    /// drag starts once the pointer is further than this along an axis.
    #[qml(notify, readonly)]
    pub start_drag_distance: i32,
    /// What qml6 reads on a desktop; a host that knows better can write them.
    #[qml(notify, readonly)]
    pub cursor_flash_time: i32,
    #[qml(notify, readonly)]
    pub font_smoothing_gamma: f64,
    #[qml(notify, readonly)]
    pub keyboard_auto_repeat_rate: i32,
    #[qml(notify, readonly)]
    pub keyboard_auto_repeat_rate_f: f64,
    #[qml(notify, readonly)]
    pub keyboard_input_interval: i32,
    #[qml(notify, readonly)]
    pub mouse_double_click_interval: i32,
    #[qml(notify, readonly)]
    pub mouse_press_and_hold_interval: i32,
    #[qml(notify, readonly)]
    pub password_mask_character: String,
    #[qml(notify, readonly)]
    pub password_mask_delay: i32,
    #[qml(notify, readonly)]
    pub set_focus_on_touch_release: bool,
    #[qml(notify, readonly)]
    pub show_is_full_screen: bool,
    #[qml(notify, readonly)]
    pub show_is_maximized: bool,
    #[qml(notify, readonly)]
    pub start_drag_time: i32,
    #[qml(notify, readonly)]
    pub start_drag_velocity: i32,
    #[qml(notify, readonly)]
    pub use_rtl_extensions: bool,
    #[qml(notify, readonly)]
    pub tab_focus_behavior: i32,
    #[qml(notify, readonly)]
    pub single_click_activation: bool,
    #[qml(notify, readonly)]
    pub wheel_scroll_lines: i32,
    #[qml(notify, readonly)]
    pub mouse_double_click_distance: i32,
    #[qml(notify, readonly)]
    pub touch_double_tap_distance: i32,
    #[qml(notify, readonly)]
    pub menu_selection_wraps: bool,
    #[qml(notify)]
    pub show_shortcuts_in_context_menus: bool,
    #[qml(notify)]
    pub context_menu_trigger: i32,
    #[qml(notify)]
    pub use_hover_effects: bool,
    #[qml(notify)]
    pub mouse_quick_selection_threshold: i32,
}

/// `startDragDistance` until a host says otherwise: what qml6 reads on a
/// desktop (features/input/drag-threshold.feature).
pub const START_DRAG_DISTANCE: i32 = 8;

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
    /// What is in effect: the host's settings, and over them those an
    /// application or a test set.
    system: Cell<SystemPrefs>,
    /// What the host said last (`report_system_prefs`).
    host: Cell<SystemPrefs>,
    /// The settings an application or a test set to other than the
    /// host's, which the host's reports leave alone.
    pinned: Cell<Pinned>,
    /// The scheme the application asked for, while it asks for one.
    asked: Cell<Option<ColorScheme>>,
    /// `startDragDistance`.
    drag_distance: Cell<i32>,
}

impl Default for State {
    fn default() -> Self {
        State {
            objects: Cell::new(None),
            system: Cell::new(SystemPrefs::default()),
            host: Cell::new(SystemPrefs::default()),
            pinned: Cell::new(Pinned::default()),
            asked: Cell::new(None),
            drag_distance: Cell::new(START_DRAG_DISTANCE),
        }
    }
}

/// `Qt.styleHints`, the object `Application.styleHints` is as well.
pub fn style_hints(engine: &Engine) -> Option<ObjectId> {
    objects(engine).map(|(h, _)| h)
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
        .create_native_incomplete(
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
        .create_native_incomplete(
            StyleHints {
                color_scheme: shown_scheme(&s).qt_value(),
                accessibility: Some(a11y),
                start_drag_distance: s.drag_distance.get(),
                cursor_flash_time: 1200,
                font_smoothing_gamma: 1.7,
                keyboard_auto_repeat_rate: 30,
                keyboard_auto_repeat_rate_f: 30.0,
                keyboard_input_interval: 400,
                mouse_double_click_interval: 400,
                mouse_press_and_hold_interval: PRESS_AND_HOLD_INTERVAL,
                password_mask_character: "\u{2022}".to_string(),
                password_mask_delay: 0,
                set_focus_on_touch_release: false,
                show_is_full_screen: false,
                show_is_maximized: false,
                start_drag_time: 500,
                start_drag_velocity: 0,
                use_rtl_extensions: false,
                tab_focus_behavior: 255,
                single_click_activation: false,
                wheel_scroll_lines: 3,
                mouse_double_click_distance: 5,
                touch_double_tap_distance: 10,
                menu_selection_wraps: true,
                show_shortcuts_in_context_menus: true,
                context_menu_trigger: 0,
                use_hover_effects: true,
                mouse_quick_selection_threshold: 10,
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

/// Which settings an application or a test holds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pinned {
    color_scheme: bool,
    contrast: bool,
    reduced_motion: bool,
    font_scale: bool,
    screen_reader_active: bool,
}

impl Pinned {
    /// The settings in which `set` differs from `host`.
    fn between(set: SystemPrefs, host: SystemPrefs) -> Self {
        Pinned {
            color_scheme: set.color_scheme != host.color_scheme,
            contrast: set.contrast != host.contrast,
            reduced_motion: set.reduced_motion != host.reduced_motion,
            font_scale: set.font_scale != host.font_scale,
            screen_reader_active: set.screen_reader_active != host.screen_reader_active,
        }
    }

    /// `host`, with `held` where a setting is held.
    fn over(self, held: SystemPrefs, host: SystemPrefs) -> SystemPrefs {
        let pick = |pinned: bool| if pinned { held } else { host };
        SystemPrefs {
            color_scheme: pick(self.color_scheme).color_scheme,
            contrast: pick(self.contrast).contrast,
            reduced_motion: pick(self.reduced_motion).reduced_motion,
            font_scale: pick(self.font_scale).font_scale,
            screen_reader_active: pick(self.screen_reader_active).screen_reader_active,
        }
    }
}

fn sane(prefs: SystemPrefs) -> SystemPrefs {
    SystemPrefs {
        font_scale: SystemPrefs::sane_font_scale(prefs.font_scale),
        ..prefs
    }
}

/// Sets what the system prefers, as an application or a test does to be
/// the system: a setting made other than the host's stays over what the
/// host reports later, until it is set to the host's value again. Telling
/// the same again costs nothing.
pub fn set_system_prefs(engine: &Engine, prefs: SystemPrefs) {
    let s = state(engine);
    let prefs = sane(prefs);
    s.pinned.set(Pinned::between(prefs, s.host.get()));
    apply(engine, &s, prefs);
}

/// What the platform prefers, as a host heard it: at start and whenever
/// the platform reports a change. A setting the report does not name, or
/// one an application or a test holds (`set_system_prefs`), stays as it
/// is.
pub fn report_system_prefs(engine: &Engine, report: HostPrefs) {
    let s = state(engine);
    let host = sane(report.over(s.host.get()));
    s.host.set(host);
    let prefs = s.pinned.get().over(s.system.get(), host);
    apply(engine, &s, prefs);
}

fn apply(engine: &Engine, s: &State, prefs: SystemPrefs) {
    if s.system.replace(prefs) == prefs {
        return;
    }
    show(engine, s, prefs);
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

/// What is in effect: the host's settings and those set over them. Not
/// tracked: for a host that compares, and for
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

/// Tells the application how far a pointer goes before a press is a drag,
/// for a platform that says. Telling the same again costs nothing.
pub fn set_start_drag_distance(engine: &Engine, pixels: i32) {
    let s = state(engine);
    let pixels = pixels.max(0);
    if s.drag_distance.replace(pixels) == pixels {
        return;
    }
    if let Some((hints, _)) = s.objects.get().filter(|(h, _)| engine.is_alive(*h)) {
        let _ = engine.write_internal(
            hints,
            StyleHints::START_DRAG_DISTANCE.index,
            Value::Int(pixels),
            WriteKind::Imperative,
        );
    }
}

/// `Qt.styleHints.startDragDistance`, for what decides at an event whether
/// a press became a drag. Not tracked: it is asked once per move.
pub fn start_drag_distance(engine: &Engine) -> f64 {
    f64::from(state(engine).drag_distance.get())
}

/// Qt's platform default for `Qt.styleHints.mousePressAndHoldInterval`,
/// in milliseconds; no desktop platform theme changes it.
pub const PRESS_AND_HOLD_INTERVAL: i32 = 800;

/// `Qt.styleHints.mousePressAndHoldInterval` in milliseconds, for what
/// decides at a press how long it waits. Not tracked.
pub fn press_and_hold_interval(engine: &Engine) -> i32 {
    style_hints(engine).map_or(PRESS_AND_HOLD_INTERVAL, |hints| {
        let p = StyleHints::MOUSE_PRESS_AND_HOLD_INTERVAL.index;
        engine.untrack(|| engine.read(hints, p)).to_number() as i32
    })
}

/// [`press_and_hold_interval`] in seconds.
pub(crate) fn press_and_hold_seconds(engine: &Engine) -> f64 {
    f64::from(press_and_hold_interval(engine)) / 1000.0
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
