//! `QtSensors` (doc.qt.io/qt-6/qtsensors-qmlmodule.html): the device's
//! motion and environment sensors, read through a [`SensorService`].
//!
//! A sensor that is not active holds nothing of the service. Once started
//! it is given a [`SensorSink`], which the service feeds from whatever
//! thread reads the device. A reading is a message: the first one since
//! the Engine thread last looked posts a job to the platform's inbox, and
//! the ones after it, until that job runs, only replace it, so a source
//! faster than the frames costs one job per frame and the latest reading
//! is the one shown. Nothing polls.
//!
//! The sensor's `dataRate` is kept on the UI side as well: a reading that
//! comes sooner than `1 / dataRate` after the last one shown waits for
//! that moment on a timer, and a later one replaces it there. The service
//! is told the rate when the sensor starts, so a backend that can set its
//! device's rate need not send more.
//!
//! Without a backend for its kind (a desktop without sensors) a sensor is
//! not connected to one, its `reading` is null and `start()` fails, as Qt
//! documents.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject, QmlType, QtObject,
    Value, qml_impl,
};
use ljos_runtime::Time;

use crate::platform::{CATEGORY, Platform, Post};

/// What a sensor measures; one per QML sensor type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SensorKind {
    /// m/s², along x, y and z.
    Accelerometer,
    /// Degrees per second around x, y and z.
    Gyroscope,
    /// Teslas along x, y and z, and the calibration level from 0 to 1.
    Magnetometer,
    /// The azimuth in degrees from magnetic north, and the calibration
    /// level.
    Compass,
    /// Pascals, and degrees Celsius.
    Pressure,
    /// `AmbientLightReading.LightLevel`.
    AmbientLight,
    /// `OrientationReading.Orientation`.
    Orientation,
}

impl SensorKind {
    pub const ALL: [SensorKind; 7] = [
        SensorKind::Accelerometer,
        SensorKind::Gyroscope,
        SensorKind::Magnetometer,
        SensorKind::Compass,
        SensorKind::Pressure,
        SensorKind::AmbientLight,
        SensorKind::Orientation,
    ];

    /// How a scenario names it: "accelerometer", "ambient light sensor".
    pub fn describe(self) -> &'static str {
        match self {
            SensorKind::Accelerometer => "accelerometer",
            SensorKind::Gyroscope => "gyroscope",
            SensorKind::Magnetometer => "magnetometer",
            SensorKind::Compass => "compass",
            SensorKind::Pressure => "pressure sensor",
            SensorKind::AmbientLight => "ambient light sensor",
            SensorKind::Orientation => "orientation sensor",
        }
    }

    pub fn named(name: &str) -> Option<SensorKind> {
        SensorKind::ALL.into_iter().find(|k| k.describe() == name)
    }

    /// The sensor's `type`: the name of Qt's C++ class for it, which is
    /// what Qt reports.
    fn type_name(self) -> &'static str {
        match self {
            SensorKind::Accelerometer => "QAccelerometer",
            SensorKind::Gyroscope => "QGyroscope",
            SensorKind::Magnetometer => "QMagnetometer",
            SensorKind::Compass => "QCompass",
            SensorKind::Pressure => "QPressureSensor",
            SensorKind::AmbientLight => "QAmbientLightSensor",
            SensorKind::Orientation => "QOrientationSensor",
        }
    }

    /// The reading's properties, in the order of [`Measurement::values`];
    /// `true` for an enumeration, which is a whole number.
    fn values(self) -> &'static [(&'static str, bool)] {
        match self {
            SensorKind::Accelerometer | SensorKind::Gyroscope => {
                &[("x", false), ("y", false), ("z", false)]
            }
            SensorKind::Magnetometer => &[
                ("x", false),
                ("y", false),
                ("z", false),
                ("calibrationLevel", false),
            ],
            SensorKind::Compass => &[("azimuth", false), ("calibrationLevel", false)],
            SensorKind::Pressure => &[("pressure", false), ("temperature", false)],
            SensorKind::AmbientLight => &[("lightLevel", true)],
            SensorKind::Orientation => &[("orientation", true)],
        }
    }
}

/// One measurement. `values` are in the order the reading names them
/// (`x`, `y`, `z` for an accelerometer; `azimuth`, `calibrationLevel` for
/// a compass); the rest are 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measurement {
    /// Microseconds from a fixed point the backend chooses, as Qt's are.
    pub timestamp: u64,
    pub values: [f64; 4],
}

impl Measurement {
    /// A reading of up to four values; more are left out.
    pub fn new(timestamp: u64, values: &[f64]) -> Measurement {
        let mut out = [0.0; 4];
        for (to, v) in out.iter_mut().zip(values) {
            *to = *v;
        }
        Measurement {
            timestamp,
            values: out,
        }
    }
}

/// What a sensor asks of the service when it starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorRequest {
    pub kind: SensorKind,
    /// Readings a second; 0 is the backend's own rate.
    pub data_rate: i32,
    /// Keep reading while the screen is off.
    pub always_on: bool,
    /// `Accelerometer.accelerationMode`: 0 combined, 1 gravity, 2 user.
    pub acceleration_mode: i32,
    /// `Magnetometer.returnGeomagnetic`.
    pub return_geomagnetic: bool,
}

/// What a started sensor is fed through. Cheap to clone and `Send`: the
/// service keeps it on whatever thread reads the device. Feeding a sink
/// whose sensor stopped, or whose engine is gone, does nothing.
#[derive(Clone)]
pub struct SensorSink {
    id: u64,
    obj: ObjectId,
    slot: Arc<Mutex<Slot>>,
    post: Post,
}

#[derive(Default)]
struct Slot {
    latest: Option<Measurement>,
    /// A job is on its way to take `latest`.
    posted: bool,
}

impl std::fmt::Debug for SensorSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensorSink").field("id", &self.id).finish()
    }
}

static NEXT_SINK: AtomicU64 = AtomicU64::new(1);

impl SensorSink {
    /// The sensor it feeds, as [`SensorService::stop`] names it.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Hands `reading` to the sensor. It is shown at the Engine thread's
    /// next frame, unless a later one comes first.
    pub fn send(&self, reading: Measurement) {
        let first = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            slot.latest = Some(reading);
            !std::mem::replace(&mut slot.posted, true)
        };
        if first {
            let (id, obj, slot) = (self.id, self.obj, Arc::clone(&self.slot));
            self.post.post(Box::new(move |e: &Engine| {
                let reading = {
                    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                    slot.posted = false;
                    slot.latest.take()
                };
                if let Some(reading) = reading {
                    arrived(e, obj, id, reading);
                }
            }));
        }
    }
}

/// The device's sensors.
pub trait SensorService {
    /// The backend that reads `kind` (Qt's sensor `identifier`); `None`
    /// when the device has no such sensor.
    fn identifier(&self, kind: SensorKind) -> Option<String>;

    /// Starts feeding `sink` with what `request` asks for; false when it
    /// cannot. Called on the Engine thread; it must not wait for the
    /// device.
    fn start(&self, request: SensorRequest, sink: SensorSink) -> bool;

    /// Stops feeding the sink with this [`SensorSink::id`].
    fn stop(&self, sink: u64);
}

/// A device without sensors: every desktop until its host installs a
/// backend. No sensor connects or starts.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSensors;

impl SensorService for NoSensors {
    fn identifier(&self, _kind: SensorKind) -> Option<String> {
        None
    }

    fn start(&self, _request: SensorRequest, _sink: SensorSink) -> bool {
        false
    }

    fn stop(&self, _sink: u64) {}
}

#[derive(Debug, Default)]
struct Fake {
    present: Vec<SensorKind>,
    running: Vec<(SensorRequest, SensorSink)>,
}

/// Sensors a test gives the device and reads for: [`FakeSensors::add`]
/// one, and [`FakeSensors::feed`] it. Clones share the device.
#[derive(Clone, Debug, Default)]
pub struct FakeSensors {
    state: Rc<RefCell<Fake>>,
}

impl FakeSensors {
    /// The device has a sensor of `kind`.
    pub fn add(&self, kind: SensorKind) {
        let mut st = self.state.borrow_mut();
        if !st.present.contains(&kind) {
            st.present.push(kind);
        }
    }

    /// The sensor of `kind` measures `reading`: every started sensor of
    /// that kind is fed it. Returns how many were.
    pub fn feed(&self, kind: SensorKind, reading: Measurement) -> usize {
        let sinks: Vec<SensorSink> = self
            .state
            .borrow()
            .running
            .iter()
            .filter(|(r, _)| r.kind == kind)
            .map(|(_, s)| s.clone())
            .collect();
        for sink in &sinks {
            sink.send(reading);
        }
        sinks.len()
    }

    /// What the started sensors asked for, oldest first.
    pub fn running(&self) -> Vec<SensorRequest> {
        self.state
            .borrow()
            .running
            .iter()
            .map(|(r, _)| *r)
            .collect()
    }
}

impl SensorService for FakeSensors {
    fn identifier(&self, kind: SensorKind) -> Option<String> {
        self.state
            .borrow()
            .present
            .contains(&kind)
            .then(|| format!("fake.{}", kind.type_name()))
    }

    fn start(&self, request: SensorRequest, sink: SensorSink) -> bool {
        let mut st = self.state.borrow_mut();
        if !st.present.contains(&request.kind) {
            return false;
        }
        st.running.push((request, sink));
        true
    }

    fn stop(&self, sink: u64) {
        self.state
            .borrow_mut()
            .running
            .retain(|(_, s)| s.id != sink);
    }
}

// ---------------------------------------------------------------- QML types

/// What a started sensor holds of the service; dropping it stops the
/// feed, so a sensor destroyed while active stops too.
struct Link {
    id: u64,
    service: Rc<dyn SensorService>,
    /// When the last reading was shown, for `dataRate`.
    shown: Option<Time>,
    /// A reading that came too soon, and whether a timer is set to show it.
    waiting: Option<Measurement>,
    timer: bool,
    /// The last reading shown, for `skipDuplicates`.
    last: Option<Measurement>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.service.stop(self.id);
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link").field("id", &self.id).finish()
    }
}

/// `Sensor` (doc.qt.io/qt-6/qml-qtsensors-sensor.html), the base of every
/// sensor type.
#[derive(Debug, QmlObject)]
#[qml(module = "QtSensors", name = "Sensor", extends = QtObject, uncreatable)]
pub struct Sensor {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub always_on: bool,
    #[qml(notify, readonly)]
    pub busy: bool,
    #[qml(notify, readonly)]
    pub connected_to_backend: bool,
    #[qml(notify)]
    pub data_rate: i32,
    #[qml(notify, readonly)]
    pub description: Atom,
    #[qml(notify, readonly)]
    pub error: i32,
    #[qml(notify)]
    pub identifier: Atom,
    /// Notified on every reading shown: its change signal is Qt's
    /// `readingChanged`.
    #[qml(notify, readonly, object = "SensorReading")]
    pub reading: Option<ObjectId>,
    #[qml(notify)]
    pub skip_duplicates: bool,
    #[qml(notify, readonly, rename = "type")]
    pub ty: Atom,
    #[qml(skip)]
    link: Option<Link>,
}

impl Default for Sensor {
    fn default() -> Self {
        Sensor {
            object: QtObject::default(),
            active: false,
            always_on: false,
            busy: false,
            connected_to_backend: false,
            data_rate: 0,
            description: Atom::EMPTY,
            error: 0,
            identifier: Atom::EMPTY,
            reading: None,
            skip_duplicates: false,
            ty: Atom::EMPTY,
            link: None,
        }
    }
}

#[qml_impl]
impl Sensor {
    #[qml(signal)]
    fn sensor_error(error: i32);

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<Value, EngineError> {
        Ok(Value::from(start(cx.engine, cx.this)))
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        Ok(())
    }

    /// Connects to a backend once complete, and starts if `active` was
    /// declared; `active` written later starts and stops it.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if let Some(kind) = kind_of(e, me) {
            e.set_internal(me, Sensor::TY, Atom::from(kind.type_name()))?;
        }
        connect(e, me);
        e.watch(me, Sensor::ACTIVE.index, move |e, v| {
            if v.to_boolean() {
                start(e, me);
            } else {
                stop(e, me);
            }
        })?;
        if e.untrack(|| e.get(me, Sensor::ACTIVE))? {
            start(e, me);
        }
        Ok(())
    }
}

/// `SensorReading`, the base of every reading.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "SensorReading", extends = QtObject, uncreatable)]
pub struct SensorReading {
    #[qml(base)]
    pub object: QtObject,
    /// Microseconds, as [`Measurement::timestamp`].
    #[qml(notify, readonly)]
    pub timestamp: f64,
}

fn acceleration_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "AccelerationMode",
        vec![("Combined", 0), ("Gravity", 1), ("User", 2)],
    )]
}

/// `Accelerometer` (doc.qt.io/qt-6/qml-qtsensors-accelerometer.html).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "Accelerometer",
    extends = Sensor,
    enums = acceleration_enums
)]
pub struct Accelerometer {
    #[qml(base)]
    pub sensor: Sensor,
    #[qml(notify, enumeration = "AccelerationMode")]
    pub acceleration_mode: i32,
}

#[qml_impl]
impl Accelerometer {}

/// `AccelerometerReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "AccelerometerReading",
    extends = SensorReading,
    uncreatable
)]
pub struct AccelerometerReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly)]
    pub x: f64,
    #[qml(notify, readonly)]
    pub y: f64,
    #[qml(notify, readonly)]
    pub z: f64,
}

/// `Gyroscope` (doc.qt.io/qt-6/qml-qtsensors-gyroscope.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "Gyroscope", extends = Sensor)]
pub struct Gyroscope {
    #[qml(base)]
    pub sensor: Sensor,
}

#[qml_impl]
impl Gyroscope {}

/// `GyroscopeReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "GyroscopeReading",
    extends = SensorReading,
    uncreatable
)]
pub struct GyroscopeReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly)]
    pub x: f64,
    #[qml(notify, readonly)]
    pub y: f64,
    #[qml(notify, readonly)]
    pub z: f64,
}

/// `Magnetometer` (doc.qt.io/qt-6/qml-qtsensors-magnetometer.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "Magnetometer", extends = Sensor)]
pub struct Magnetometer {
    #[qml(base)]
    pub sensor: Sensor,
    #[qml(notify)]
    pub return_geomagnetic: bool,
}

#[qml_impl]
impl Magnetometer {}

/// `MagnetometerReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "MagnetometerReading",
    extends = SensorReading,
    uncreatable
)]
pub struct MagnetometerReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly)]
    pub x: f64,
    #[qml(notify, readonly)]
    pub y: f64,
    #[qml(notify, readonly)]
    pub z: f64,
    #[qml(notify, readonly)]
    pub calibration_level: f64,
}

/// `Compass` (doc.qt.io/qt-6/qml-qtsensors-compass.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "Compass", extends = Sensor)]
pub struct Compass {
    #[qml(base)]
    pub sensor: Sensor,
}

#[qml_impl]
impl Compass {}

/// `CompassReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "CompassReading",
    extends = SensorReading,
    uncreatable
)]
pub struct CompassReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly)]
    pub azimuth: f64,
    #[qml(notify, readonly)]
    pub calibration_level: f64,
}

/// `PressureSensor` (doc.qt.io/qt-6/qml-qtsensors-pressuresensor.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "PressureSensor", extends = Sensor)]
pub struct PressureSensor {
    #[qml(base)]
    pub sensor: Sensor,
}

#[qml_impl]
impl PressureSensor {}

/// `PressureReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "PressureReading",
    extends = SensorReading,
    uncreatable
)]
pub struct PressureReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly)]
    pub pressure: f64,
    #[qml(notify, readonly)]
    pub temperature: f64,
}

/// `AmbientLightSensor`
/// (doc.qt.io/qt-6/qml-qtsensors-ambientlightsensor.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "AmbientLightSensor", extends = Sensor)]
pub struct AmbientLightSensor {
    #[qml(base)]
    pub sensor: Sensor,
}

#[qml_impl]
impl AmbientLightSensor {}

fn light_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "LightLevel",
        vec![
            ("Undefined", 0),
            ("Dark", 1),
            ("Twilight", 2),
            ("Light", 3),
            ("Bright", 4),
            ("Sunny", 5),
        ],
    )]
}

/// `AmbientLightReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "AmbientLightReading",
    extends = SensorReading,
    enums = light_enums,
    uncreatable
)]
pub struct AmbientLightReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly, enumeration = "LightLevel")]
    pub light_level: i32,
}

/// `OrientationSensor`
/// (doc.qt.io/qt-6/qml-qtsensors-orientationsensor.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtSensors", name = "OrientationSensor", extends = Sensor)]
pub struct OrientationSensor {
    #[qml(base)]
    pub sensor: Sensor,
}

#[qml_impl]
impl OrientationSensor {}

fn orientation_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Orientation",
        vec![
            ("Undefined", 0),
            ("TopUp", 1),
            ("TopDown", 2),
            ("LeftUp", 3),
            ("RightUp", 4),
            ("FaceUp", 5),
            ("FaceDown", 6),
        ],
    )]
}

/// `OrientationReading`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtSensors",
    name = "OrientationReading",
    extends = SensorReading,
    enums = orientation_enums,
    uncreatable
)]
pub struct OrientationReading {
    #[qml(base)]
    pub reading: SensorReading,
    #[qml(notify, readonly, enumeration = "Orientation")]
    pub orientation: i32,
}

/// Registers `QtSensors`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtSensors" 6.0 => [
        Sensor, SensorReading,
        Accelerometer, AccelerometerReading,
        Gyroscope, GyroscopeReading,
        Magnetometer, MagnetometerReading,
        Compass, CompassReading,
        PressureSensor, PressureReading,
        AmbientLightSensor, AmbientLightReading,
        OrientationSensor, OrientationReading,
    ]));
}

// ---------------------------------------------------------------- behaviour

fn is<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

fn kind_of(engine: &Engine, obj: ObjectId) -> Option<SensorKind> {
    Some(if is::<Accelerometer>(engine, obj) {
        SensorKind::Accelerometer
    } else if is::<Gyroscope>(engine, obj) {
        SensorKind::Gyroscope
    } else if is::<Magnetometer>(engine, obj) {
        SensorKind::Magnetometer
    } else if is::<Compass>(engine, obj) {
        SensorKind::Compass
    } else if is::<PressureSensor>(engine, obj) {
        SensorKind::Pressure
    } else if is::<AmbientLightSensor>(engine, obj) {
        SensorKind::AmbientLight
    } else if is::<OrientationSensor>(engine, obj) {
        SensorKind::Orientation
    } else {
        return None;
    })
}

/// A reading object for `kind`, owned by the sensor.
fn make_reading(engine: &Engine, kind: SensorKind, owner: ObjectId) -> Option<ObjectId> {
    let made = match kind {
        SensorKind::Accelerometer => {
            engine.create_native(AccelerometerReading::default(), Some(owner))
        }
        SensorKind::Gyroscope => engine.create_native(GyroscopeReading::default(), Some(owner)),
        SensorKind::Magnetometer => {
            engine.create_native(MagnetometerReading::default(), Some(owner))
        }
        SensorKind::Compass => engine.create_native(CompassReading::default(), Some(owner)),
        SensorKind::Pressure => engine.create_native(PressureReading::default(), Some(owner)),
        SensorKind::AmbientLight => {
            engine.create_native(AmbientLightReading::default(), Some(owner))
        }
        SensorKind::Orientation => engine.create_native(OrientationReading::default(), Some(owner)),
    };
    made.map_err(|err| warn(engine, format!("a sensor's reading: {err}")))
        .ok()
}

fn warn(engine: &Engine, message: String) {
    engine.log(LogLevel::Warning, CATEGORY, message);
}

/// Finds the backend for `obj`'s kind, as Qt does when the sensor is
/// complete or first started: `connectedToBackend`, `identifier` and an
/// empty `reading` once there is one. True when connected.
fn connect(engine: &Engine, obj: ObjectId) -> bool {
    let Some(kind) = kind_of(engine, obj) else {
        return false;
    };
    let connected = engine.untrack(|| engine.get(obj, Sensor::CONNECTED_TO_BACKEND));
    if connected.unwrap_or(false) {
        return true;
    }
    let Some(identifier) = Platform::of(engine).sensors().identifier(kind) else {
        return false;
    };
    let reading = make_reading(engine, kind, obj);
    let set = (|| {
        let named = engine.untrack(|| engine.get(obj, Sensor::IDENTIFIER))?;
        if named.as_str().is_empty() {
            engine.set_internal(obj, Sensor::IDENTIFIER, Atom::from(identifier.as_str()))?;
        }
        engine.set_internal(obj, Sensor::READING, reading)?;
        engine.set_internal(obj, Sensor::CONNECTED_TO_BACKEND, true)
    })();
    if let Err(err) = set {
        warn(engine, format!("a sensor's backend: {err}"));
        return false;
    }
    true
}

fn int(engine: &Engine, obj: ObjectId, name: &str) -> i32 {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_number() as i32
}

fn flag(engine: &Engine, obj: ObjectId, name: &str) -> bool {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_boolean()
}

/// Starts `obj` reading; true when it reads, as `Sensor.start()` says.
fn start(engine: &Engine, obj: ObjectId) -> bool {
    let running = engine.with(obj, |s: &Sensor| s.link.is_some());
    if running.unwrap_or(false) {
        return true;
    }
    let Some(kind) = kind_of(engine, obj) else {
        return false;
    };
    let started = connect(engine, obj) && {
        let platform = Platform::of(engine);
        let service = platform.sensors();
        let sink = SensorSink {
            id: NEXT_SINK.fetch_add(1, Ordering::Relaxed),
            obj,
            slot: Arc::default(),
            post: platform.poster(),
        };
        let id = sink.id;
        let request = SensorRequest {
            kind,
            data_rate: int(engine, obj, "dataRate"),
            always_on: flag(engine, obj, "alwaysOn"),
            acceleration_mode: if kind == SensorKind::Accelerometer {
                int(engine, obj, "accelerationMode")
            } else {
                0
            },
            return_geomagnetic: kind == SensorKind::Magnetometer
                && flag(engine, obj, "returnGeomagnetic"),
        };
        service.start(request, sink) && {
            let link = Link {
                id,
                service,
                shown: None,
                waiting: None,
                timer: false,
                last: None,
            };
            engine
                .with_mut(obj, |s: &mut Sensor| s.link = Some(link))
                .is_ok()
        }
    };
    // Written after the link is in place, so the watch on `active` finds
    // the sensor running and does nothing.
    show_active(engine, obj, started);
    started
}

fn stop(engine: &Engine, obj: ObjectId) {
    // Dropped outside the borrow: dropping it tells the service.
    let link = engine
        .with_mut(obj, |s: &mut Sensor| s.link.take())
        .ok()
        .flatten();
    drop(link);
    show_active(engine, obj, false);
}

fn show_active(engine: &Engine, obj: ObjectId, active: bool) {
    let now = engine.untrack(|| engine.get(obj, Sensor::ACTIVE));
    if now.is_ok_and(|now| now != active)
        && let Err(err) = engine.set_internal(obj, Sensor::ACTIVE, active)
    {
        warn(engine, format!("a sensor's active: {err}"));
    }
}

/// What becomes of a reading that came.
enum Arrival {
    Show,
    /// It waits for `dataRate`; a timer is to be set for that moment.
    WaitUntil(Time),
    /// It waits, and replaces one a timer is already set for.
    Replace,
    /// The sensor stopped, or this came from before it was restarted.
    Stale,
}

/// A reading came from sink `id` of `obj`.
fn arrived(engine: &Engine, obj: ObjectId, id: u64, reading: Measurement) {
    if !engine.is_alive(obj) {
        return;
    }
    let rate = int(engine, obj, "dataRate");
    let now = engine.now();
    let arrival = engine.with_mut(obj, |s: &mut Sensor| {
        let Some(link) = s.link.as_mut().filter(|l| l.id == id) else {
            return Arrival::Stale;
        };
        let due = link
            .shown
            .filter(|_| rate > 0)
            .map(|shown| shown + Duration::from_secs(1) / rate.unsigned_abs())
            .filter(|due| *due > now);
        let Some(due) = due else {
            return Arrival::Show;
        };
        link.waiting = Some(reading);
        if std::mem::replace(&mut link.timer, true) {
            Arrival::Replace
        } else {
            Arrival::WaitUntil(due)
        }
    });
    match arrival.unwrap_or(Arrival::Stale) {
        Arrival::Show => show(engine, obj, reading),
        Arrival::WaitUntil(due) => {
            engine.call_at(due, move |e| {
                let waiting = e
                    .with_mut(obj, |s: &mut Sensor| {
                        let link = s.link.as_mut().filter(|l| l.id == id)?;
                        link.timer = false;
                        link.waiting.take()
                    })
                    .ok()
                    .flatten();
                if let Some(reading) = waiting {
                    show(e, obj, reading);
                }
            });
        }
        Arrival::Replace | Arrival::Stale => {}
    }
}

/// Writes `reading` into `obj`'s reading object and tells `readingChanged`.
fn show(engine: &Engine, obj: ObjectId, reading: Measurement) {
    let skip = flag(engine, obj, "skipDuplicates");
    let now = engine.now();
    let fresh = engine.with_mut(obj, |s: &mut Sensor| {
        let Some(link) = s.link.as_mut() else {
            return false;
        };
        // A duplicate is the same measurement at another time.
        let duplicate = skip && link.last.is_some_and(|l| l.values == reading.values);
        if !duplicate {
            link.last = Some(reading);
            link.shown = Some(now);
        }
        !duplicate
    });
    if !fresh.unwrap_or(false) {
        return;
    }
    let Some(kind) = kind_of(engine, obj) else {
        return;
    };
    let Ok(Some(target)) = engine.untrack(|| engine.get(obj, Sensor::READING)) else {
        return;
    };
    let written = (|| {
        engine.set_internal(target, SensorReading::TIMESTAMP, reading.timestamp as f64)?;
        for (&(name, whole), &v) in kind.values().iter().zip(&reading.values) {
            let value = if whole {
                Value::Int(v as i32)
            } else {
                Value::from(v)
            };
            let index: PropIndex = engine.property_index(target, name)?;
            engine.write_internal(target, index, value, ljos_reactive::WriteKind::Imperative)?;
        }
        engine.notify(obj, Sensor::READING.index)
    })();
    if let Err(err) = written {
        warn(engine, format!("a sensor's reading: {err}"));
    }
}
