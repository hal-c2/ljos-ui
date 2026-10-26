//! `PositionSource` and its `Position`, fed by a [`PositionService`].
//!
//! The service is the system's: a host installs one with [`set_service`].
//! Until it does there is [`NoPosition`], and a source is what Qt
//! documents for no source: not valid, and `start()`, `stop()` and
//! `update()` do nothing. [`FixedPosition`] is a service that is at one
//! place until it is moved, for tests and for a host that knows where it
//! is.
//!
//! A service hands fixes to a [`PositionSink`] from any thread. The sink
//! keeps the newest and posts one job at a time to the Engine thread, so
//! a fast service cannot fill the inbox; nothing polls.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use ljos::prelude::*;

use crate::geo::GeoCoordinate;

const CATEGORY: &str = "qml";

/// `PositionSource.PositioningMethods`, as Qt's flags.
pub mod methods {
    pub const NONE: i32 = 0;
    pub const SATELLITE: i32 = 0xff;
    pub const NON_SATELLITE: i32 = !0xff;
    pub const ALL: i32 = -1;
}

/// `PositionSource.SourceError`.
pub mod source_error {
    pub const ACCESS: i32 = 0;
    pub const CLOSED: i32 = 1;
    pub const UNKNOWN_SOURCE: i32 = 2;
    pub const NONE: i32 = 3;
    pub const UPDATE_TIMEOUT: i32 = 4;
}

fn source_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "PositioningMethods",
            vec![
                ("NoPositioningMethods", i64::from(methods::NONE)),
                ("SatellitePositioningMethods", i64::from(methods::SATELLITE)),
                (
                    "NonSatellitePositioningMethods",
                    i64::from(methods::NON_SATELLITE),
                ),
                ("AllPositioningMethods", i64::from(methods::ALL)),
            ],
        ),
        (
            "SourceError",
            vec![
                ("AccessError", i64::from(source_error::ACCESS)),
                ("ClosedError", i64::from(source_error::CLOSED)),
                (
                    "UnknownSourceError",
                    i64::from(source_error::UNKNOWN_SOURCE),
                ),
                ("NoError", i64::from(source_error::NONE)),
                (
                    "UpdateTimeoutError",
                    i64::from(source_error::UPDATE_TIMEOUT),
                ),
            ],
        ),
    ]
}

// ------------------------------------------------------------------ service

/// One fix: where the system is, and what else it measured. A part it
/// did not measure is `None`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionFix {
    /// Its altitude NaN when that is not known.
    pub coordinate: GeoCoordinate,
    /// When it was taken; `None` is when it arrives.
    pub timestamp: Option<SystemTime>,
    /// Metres a second.
    pub speed: Option<f64>,
    /// Degrees from north.
    pub direction: Option<f64>,
    /// Metres a second, upwards.
    pub vertical_speed: Option<f64>,
    /// Metres.
    pub horizontal_accuracy: Option<f64>,
    pub vertical_accuracy: Option<f64>,
    /// Degrees east of true north.
    pub magnetic_variation: Option<f64>,
    pub direction_accuracy: Option<f64>,
}

impl PositionFix {
    /// At `coordinate`, and nothing else measured.
    pub fn at(coordinate: GeoCoordinate) -> Self {
        Self {
            coordinate,
            timestamp: None,
            speed: None,
            direction: None,
            vertical_speed: None,
            horizontal_accuracy: None,
            vertical_accuracy: None,
            magnetic_variation: None,
            direction_accuracy: None,
        }
    }
}

/// Where a started source's fixes go: [`PositionSink::send`] from any
/// thread, as often as there is one.
#[derive(Clone)]
pub struct PositionSink {
    id: u64,
    obj: ObjectId,
    slot: Arc<Mutex<Slot>>,
    post: Post,
}

#[derive(Default)]
struct Slot {
    latest: Option<PositionFix>,
    posted: bool,
}

impl std::fmt::Debug for PositionSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionSink")
            .field("id", &self.id)
            .finish()
    }
}

static NEXT_SINK: AtomicU64 = AtomicU64::new(1);

impl PositionSink {
    /// What [`PositionService::stop`] is told.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Hands on `fix`; one sent before the last was shown replaces it.
    pub fn send(&self, fix: PositionFix) {
        let first = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.latest = Some(fix);
            !std::mem::replace(&mut slot.posted, true)
        };
        if first {
            let (id, obj, slot) = (self.id, self.obj, Arc::clone(&self.slot));
            self.post.send(move |e: &Engine| {
                let fix = {
                    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                    slot.posted = false;
                    slot.latest.take()
                };
                if let Some(fix) = fix {
                    arrived(e, obj, id, fix);
                }
            });
        }
    }
}

/// The system's position service.
pub trait PositionService {
    /// What a source's `name` says; `None` when there is no service.
    fn name(&self) -> Option<String>;
    /// The `PositioningMethods` it has.
    fn supported_methods(&self) -> i32;
    /// Feeds `sink` until [`PositionService::stop`]; false when it cannot.
    fn start(&self, sink: PositionSink) -> bool;
    fn stop(&self, sink: u64);
}

/// No position service: what a source has until a host installs one.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPosition;

impl PositionService for NoPosition {
    fn name(&self) -> Option<String> {
        None
    }
    fn supported_methods(&self) -> i32 {
        methods::NONE
    }
    fn start(&self, _sink: PositionSink) -> bool {
        false
    }
    fn stop(&self, _sink: u64) {}
}

/// A service that is at one place until [`FixedPosition::move_to`]: a
/// started source is given that place at once and each place it moves to.
/// Its name is `fixed`, and it says it has every positioning method.
#[derive(Clone, Debug)]
pub struct FixedPosition {
    state: Rc<RefCell<Fixed>>,
}

#[derive(Debug)]
struct Fixed {
    fix: PositionFix,
    running: Vec<PositionSink>,
}

impl FixedPosition {
    pub fn new(fix: PositionFix) -> Self {
        Self {
            state: Rc::new(RefCell::new(Fixed {
                fix,
                running: Vec::new(),
            })),
        }
    }

    /// Moves to `fix`: every started source is given it. Returns how many
    /// were.
    pub fn move_to(&self, fix: PositionFix) -> usize {
        let sinks = {
            let mut st = self.state.borrow_mut();
            st.fix = fix;
            st.running.clone()
        };
        for sink in &sinks {
            sink.send(fix);
        }
        sinks.len()
    }
}

impl PositionService for FixedPosition {
    fn name(&self) -> Option<String> {
        Some("fixed".into())
    }
    fn supported_methods(&self) -> i32 {
        methods::ALL
    }
    fn start(&self, sink: PositionSink) -> bool {
        let fix = {
            let mut st = self.state.borrow_mut();
            st.running.push(sink.clone());
            st.fix
        };
        sink.send(fix);
        true
    }
    fn stop(&self, sink: u64) {
        self.state.borrow_mut().running.retain(|s| s.id != sink);
    }
}

/// The service of one engine.
#[derive(Default)]
struct Installed(RefCell<Option<Rc<dyn PositionService>>>);

/// Installs the system's position service on `engine`, for the sources
/// made from here on.
pub fn set_service(engine: &Engine, service: Rc<dyn PositionService>) {
    *engine.extension::<Installed>().0.borrow_mut() = Some(service);
}

/// The service of `engine`: [`NoPosition`] until one is installed.
pub fn service(engine: &Engine) -> Rc<dyn PositionService> {
    engine
        .extension::<Installed>()
        .0
        .borrow()
        .clone()
        .unwrap_or_else(|| Rc::new(NoPosition))
}

// ---------------------------------------------------------------- QML types

/// `Position` (doc.qt.io/qt-6/qml-qtpositioning-position.html): what a
/// source last knew. Until a fix it knows nothing.
#[derive(Debug, QmlObject)]
#[qml(module = "QtPositioning", name = "Position", extends = QtObject)]
pub struct Position {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub coordinate: GeoCoordinate,
    #[qml(notify, readonly)]
    pub latitude_valid: bool,
    #[qml(notify, readonly)]
    pub longitude_valid: bool,
    #[qml(notify, readonly)]
    pub altitude_valid: bool,
    /// A `Date`, invalid until a fix.
    #[qml(notify, readonly)]
    pub timestamp: Value,
    #[qml(notify, readonly)]
    pub speed: f64,
    #[qml(notify, readonly)]
    pub speed_valid: bool,
    #[qml(notify, readonly)]
    pub direction: f64,
    #[qml(notify, readonly)]
    pub direction_valid: bool,
    #[qml(notify, readonly)]
    pub vertical_speed: f64,
    #[qml(notify, readonly)]
    pub vertical_speed_valid: bool,
    #[qml(notify, readonly)]
    pub horizontal_accuracy: f64,
    #[qml(notify, readonly)]
    pub horizontal_accuracy_valid: bool,
    #[qml(notify, readonly)]
    pub vertical_accuracy: f64,
    #[qml(notify, readonly)]
    pub vertical_accuracy_valid: bool,
    #[qml(notify, readonly)]
    pub magnetic_variation: f64,
    #[qml(notify, readonly)]
    pub magnetic_variation_valid: bool,
    #[qml(notify, readonly)]
    pub direction_accuracy: f64,
    #[qml(notify, readonly)]
    pub direction_accuracy_valid: bool,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            coordinate: GeoCoordinate::default(),
            latitude_valid: false,
            longitude_valid: false,
            altitude_valid: false,
            timestamp: Value::Undefined,
            speed: f64::NAN,
            speed_valid: false,
            direction: f64::NAN,
            direction_valid: false,
            vertical_speed: f64::NAN,
            vertical_speed_valid: false,
            horizontal_accuracy: f64::NAN,
            horizontal_accuracy_valid: false,
            vertical_accuracy: f64::NAN,
            vertical_accuracy_valid: false,
            magnetic_variation: f64::NAN,
            magnetic_variation_valid: false,
            direction_accuracy: f64::NAN,
            direction_accuracy_valid: false,
        }
    }
}

#[qml_impl]
impl Position {}

/// What a started source holds of the service; dropping it stops the
/// feed, so a source destroyed while active stops too.
struct Link {
    id: u64,
    service: Rc<dyn PositionService>,
    /// Asked for by `update()`: one fix, then it stops.
    once: bool,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.service.stop(self.id);
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("id", &self.id)
            .field("once", &self.once)
            .finish()
    }
}

/// `PositionSource` (doc.qt.io/qt-6/qml-qtpositioning-positionsource.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtPositioning",
    name = "PositionSource",
    extends = QtObject,
    enums = source_enums,
    default = "parameters"
)]
pub struct PositionSource {
    #[qml(base)]
    pub object: QtObject,
    /// Kept for a backend; no service reads them.
    #[qml(list)]
    pub parameters: QmlList,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub name: Atom,
    /// Made once complete. Its `positionChanged` is sent on each fix
    /// only, not when it is made.
    #[qml(readonly, object = "Position")]
    pub position: Option<ObjectId>,
    #[qml(notify, flags = "PositioningMethods")]
    pub preferred_positioning_methods: i32,
    #[qml(notify, readonly, flags = "PositioningMethods")]
    pub supported_positioning_methods: i32,
    #[qml(notify, readonly, enumeration = "SourceError")]
    pub source_error: i32,
    /// Kept; a service gives fixes as it has them.
    #[qml(notify)]
    pub update_interval: i32,
    #[qml(notify, readonly)]
    pub valid: bool,
    #[qml(skip)]
    link: Option<Link>,
}

impl Default for PositionSource {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            active: false,
            name: Atom::EMPTY,
            position: None,
            preferred_positioning_methods: methods::ALL,
            supported_positioning_methods: methods::NONE,
            source_error: source_error::NONE,
            update_interval: 0,
            valid: false,
            parameters: QmlList::default(),
            link: None,
        }
    }
}

#[qml_impl]
impl PositionSource {
    #[qml(signal)]
    fn position_changed();

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        start(cx.engine, cx.this, false);
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        Ok(())
    }

    /// One fix; a running source goes on as it is. The timeout is not
    /// kept: a service gives the fix it has.
    #[qml(invokable)]
    fn update(cx: &Cx, _timeout: Value) -> Result<(), EngineError> {
        start(cx.engine, cx.this, true);
        Ok(())
    }

    /// No service has properties of its own to read.
    #[qml(invokable, name = "backendProperty")]
    fn backend_property(_cx: &Cx, _name: Value) -> Value {
        Value::Undefined
    }

    #[qml(invokable, name = "setBackendProperty")]
    fn set_backend_property(_cx: &Cx, _name: Value, _value: Value) -> bool {
        false
    }

    /// Finds the service and makes the position once complete, and starts
    /// if `active` was declared; `active` written later starts and stops.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let position = e.create_native(Position::default(), Some(me))?;
        e.set_internal(position, Position::TIMESTAMP, date(e, f64::NAN))?;
        e.set_internal(me, PositionSource::POSITION, Some(position))?;
        let service = service(e);
        if let Some(name) = service.name() {
            let named = e.untrack(|| e.get(me, PositionSource::NAME))?;
            if named.as_str().is_empty() {
                e.set_internal(me, PositionSource::NAME, Atom::from(name.as_str()))?;
            }
            e.set_internal(me, PositionSource::VALID, true)?;
            e.set_internal(
                me,
                PositionSource::SUPPORTED_POSITIONING_METHODS,
                service.supported_methods(),
            )?;
        }
        e.watch(me, PositionSource::ACTIVE.index, move |e, v| {
            if v.to_boolean() {
                start(e, me, false);
            } else {
                stop(e, me);
            }
        })?;
        if e.untrack(|| e.get(me, PositionSource::ACTIVE))? {
            start(e, me, false);
        }
        Ok(())
    }
}

/// A script's own `Date` at `ms` since the epoch; NaN is the invalid one.
fn date(e: &Engine, ms: f64) -> Value {
    e.eval_js("(function (ms) { return new Date(ms) })")
        .ok()
        .and_then(|make| e.call_js(&make, &[Value::Real(ms)]))
        .unwrap_or(Value::Undefined)
}

fn warn(engine: &Engine, message: String) {
    engine.log(LogLevel::Warning, CATEGORY, message);
}

/// Starts `obj`, or for `once` asks it for one fix. A running source
/// stays as it is.
fn start(engine: &Engine, obj: ObjectId, once: bool) {
    let running = engine.with(obj, |s: &PositionSource| s.link.is_some());
    if running.unwrap_or(true) {
        return;
    }
    let service = service(engine);
    if service.name().is_none() {
        return;
    }
    let sink = PositionSink {
        id: NEXT_SINK.fetch_add(1, Ordering::Relaxed),
        obj,
        slot: Arc::default(),
        post: Platform::of(engine).poster(),
    };
    let id = sink.id;
    let link = Link {
        id,
        service: Rc::clone(&service),
        once,
    };
    // In place before the service may send, and before `active` is
    // shown, so the watch on it finds the source running.
    if engine
        .with_mut(obj, |s: &mut PositionSource| s.link = Some(link))
        .is_err()
    {
        return;
    }
    if !service.start(sink) {
        // Dropping the link tells the service.
        let _ = engine.with_mut(obj, |s: &mut PositionSource| s.link.take());
        return;
    }
    show_active(engine, obj, true);
}

fn stop(engine: &Engine, obj: ObjectId) {
    // Dropped outside the borrow: dropping it tells the service.
    let link = engine
        .with_mut(obj, |s: &mut PositionSource| s.link.take())
        .ok()
        .flatten();
    drop(link);
    show_active(engine, obj, false);
}

fn show_active(engine: &Engine, obj: ObjectId, active: bool) {
    let now = engine.untrack(|| engine.get(obj, PositionSource::ACTIVE));
    if now.is_ok_and(|now| now != active)
        && let Err(err) = engine.set_internal(obj, PositionSource::ACTIVE, active)
    {
        warn(engine, format!("a position source's active: {err}"));
    }
}

/// A fix came from sink `id` of `obj`.
fn arrived(engine: &Engine, obj: ObjectId, id: u64, fix: PositionFix) {
    if !engine.is_alive(obj) {
        return;
    }
    let once = engine
        .with(obj, |s: &PositionSource| {
            s.link.as_ref().filter(|l| l.id == id).map(|l| l.once)
        })
        .ok()
        .flatten();
    // From a sink since stopped.
    let Some(once) = once else {
        return;
    };
    let position = engine
        .untrack(|| engine.get(obj, PositionSource::POSITION))
        .ok()
        .flatten();
    if let Some(position) = position
        && let Err(err) = show(engine, position, &fix)
    {
        warn(engine, format!("a position: {err}"));
    }
    let _ = engine.emit_by_name(obj, "positionChanged", &[]);
    if once {
        stop(engine, obj);
    }
}

/// `position` as `fix` says.
fn show(e: &Engine, p: ObjectId, fix: &PositionFix) -> Result<(), EngineError> {
    let c = fix.coordinate;
    e.set_internal(p, Position::COORDINATE, c)?;
    e.set_internal(p, Position::LATITUDE_VALID, !c.latitude.is_nan())?;
    e.set_internal(p, Position::LONGITUDE_VALID, !c.longitude.is_nan())?;
    e.set_internal(p, Position::ALTITUDE_VALID, !c.altitude.is_nan())?;
    let taken = fix.timestamp.unwrap_or_else(SystemTime::now);
    let ms = taken
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64);
    e.set_internal(p, Position::TIMESTAMP, date(e, ms))?;
    let parts = [
        (Position::SPEED, Position::SPEED_VALID, fix.speed),
        (
            Position::DIRECTION,
            Position::DIRECTION_VALID,
            fix.direction,
        ),
        (
            Position::VERTICAL_SPEED,
            Position::VERTICAL_SPEED_VALID,
            fix.vertical_speed,
        ),
        (
            Position::HORIZONTAL_ACCURACY,
            Position::HORIZONTAL_ACCURACY_VALID,
            fix.horizontal_accuracy,
        ),
        (
            Position::VERTICAL_ACCURACY,
            Position::VERTICAL_ACCURACY_VALID,
            fix.vertical_accuracy,
        ),
        (
            Position::MAGNETIC_VARIATION,
            Position::MAGNETIC_VARIATION_VALID,
            fix.magnetic_variation,
        ),
        (
            Position::DIRECTION_ACCURACY,
            Position::DIRECTION_ACCURACY_VALID,
            fix.direction_accuracy,
        ),
    ];
    for (value, valid, part) in parts {
        e.set_internal(p, value, part.unwrap_or(f64::NAN))?;
        e.set_internal(p, valid, part.is_some())?;
    }
    Ok(())
}
