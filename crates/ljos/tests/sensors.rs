//! The device's sensors, read through the one service the host installs:
//! a sensor that is not active holds nothing of it, a started one is fed
//! from whatever thread reads the device, and a source faster than the
//! frames costs the UI one reading a frame.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos::prelude::*;
use ljos_runtime::{Clock as _, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import QtSensors
Item {
    property alias acc: a
    Accelerometer {
        id: a
        dataRate: 50
        accelerationMode: Accelerometer.User
        property int changes: 0
        readonly property real x: (reading as AccelerometerReading)?.x ?? -1
        onReadingChanged: changes++
    }
}
"#;

fn load(engine: &Engine) -> ObjectId {
    let root = common::create_main(engine, MAIN);
    engine.read_by_name(root, "acc").as_object().unwrap()
}

fn read(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    engine.read_by_name(obj, name).to_number()
}

#[test]
fn without_a_backend_a_sensor_does_not_start() {
    let engine = ljos::engine();
    let acc = load(&engine);
    let started = engine.invoke_by_name(acc, "start", &[]).unwrap();
    assert_eq!(started, Value::from(false));
    assert!(!engine.read_by_name(acc, "active").to_boolean());
    assert!(!engine.read_by_name(acc, "connectedToBackend").to_boolean());
    assert!(engine.read_by_name(acc, "reading").as_object().is_none());
}

#[test]
fn a_started_sensor_asks_the_service_and_a_stopped_one_lets_go() {
    let engine = ljos::engine();
    let clock = Arc::new(VirtualClock::new());
    engine.set_clock(Rc::new(Arc::clone(&clock)));
    let device = FakeSensors::default();
    device.add(SensorKind::Accelerometer);
    let platform = Platform::of(&engine);
    platform.set_sensors(Rc::new(device.clone()));
    let acc = load(&engine);
    // Connected, but not started: nothing is asked of the device.
    assert!(engine.read_by_name(acc, "connectedToBackend").to_boolean());
    assert!(device.running().is_empty());
    assert_eq!(read(&engine, acc, "x"), 0.0);

    engine
        .write_by_name(acc, "active", Value::from(true))
        .unwrap();
    let asked = device.running();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        (
            asked[0].kind,
            asked[0].data_rate,
            asked[0].acceleration_mode
        ),
        (SensorKind::Accelerometer, 50, 2)
    );

    let changes = read(&engine, acc, "changes");
    device.feed(
        SensorKind::Accelerometer,
        Measurement::new(1, &[0.25, 0.0, 9.5]),
    );
    // A reading is a message: it is shown when the frame takes it.
    assert_eq!(read(&engine, acc, "x"), 0.0);
    platform.deliver(&engine);
    assert_eq!(read(&engine, acc, "x"), 0.25);
    assert_eq!(read(&engine, acc, "changes"), changes + 1.0);

    // Sooner than 1/50 s after it: it waits for its moment.
    device.feed(
        SensorKind::Accelerometer,
        Measurement::new(2, &[0.5, 0.0, 9.5]),
    );
    platform.deliver(&engine);
    assert_eq!(read(&engine, acc, "x"), 0.25);
    clock.set(clock.now() + Duration::from_millis(20));
    engine.advance_timers(clock.now());
    assert_eq!(read(&engine, acc, "x"), 0.5);

    engine.invoke_by_name(acc, "stop", &[]).unwrap();
    assert!(device.running().is_empty());
    assert!(!engine.read_by_name(acc, "active").to_boolean());
}

/// A device read on a thread of its own, as a driver is.
#[derive(Default)]
struct Threaded {
    readers: RefCell<Vec<std::thread::JoinHandle<()>>>,
}

impl SensorService for Threaded {
    fn identifier(&self, kind: SensorKind) -> Option<String> {
        (kind == SensorKind::Accelerometer).then(|| "threaded".to_owned())
    }

    fn start(&self, _request: SensorRequest, sink: SensorSink) -> bool {
        self.readers.borrow_mut().push(std::thread::spawn(move || {
            for n in 1..=1000 {
                sink.send(Measurement::new(n, &[n as f64, 0.0, 0.0]));
            }
        }));
        true
    }

    fn stop(&self, _sink: u64) {}
}

#[test]
fn a_source_faster_than_the_frames_is_shown_once_a_frame_with_its_latest_reading() {
    let engine = ljos::engine();
    let device = Rc::new(Threaded::default());
    let platform = Platform::of(&engine);
    platform.set_sensors(device.clone());
    let acc = load(&engine);
    engine
        .write_by_name(acc, "dataRate", Value::from(0))
        .unwrap();
    engine
        .write_by_name(acc, "active", Value::from(true))
        .unwrap();
    for reader in device.readers.borrow_mut().drain(..) {
        reader.join().unwrap();
    }
    let changes = read(&engine, acc, "changes");
    // A thousand readings, one frame: one job, the latest reading.
    assert_eq!(platform.deliver(&engine), 1);
    assert_eq!(read(&engine, acc, "x"), 1000.0);
    assert_eq!(read(&engine, acc, "changes"), changes + 1.0);
    assert_eq!(platform.deliver(&engine), 0);
}

#[test]
fn a_sensor_destroyed_while_active_stops_its_feed() {
    let engine = ljos::engine();
    let device = FakeSensors::default();
    device.add(SensorKind::Accelerometer);
    Platform::of(&engine).set_sensors(Rc::new(device.clone()));
    let acc = load(&engine);
    engine
        .write_by_name(acc, "active", Value::from(true))
        .unwrap();
    assert_eq!(device.running().len(), 1);
    engine.destroy_now(acc);
    assert!(device.running().is_empty());
}
