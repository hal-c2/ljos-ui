//! Building a scene by hand, as the view's tests do, and taking turns on
//! the GPU to draw it.

// Each test binary takes the helpers it needs and leaves the others.
#![allow(dead_code)]

use std::io::Write as _;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{Item, add_data};

pub fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

pub fn geom(e: &Engine, o: ObjectId, x: f64, y: f64, w: f64, h: f64) {
    e.set(o, Item::X, x).unwrap();
    e.set(o, Item::Y, y).unwrap();
    e.set(o, Item::WIDTH, w).unwrap();
    e.set(o, Item::HEIGHT, h).unwrap();
}

pub fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

/// The calling test's turn on the GPU, held until it ends. Two threads that
/// start the Vulkan loader together can crash it (with the NVIDIA driver
/// in `vkEnumerateInstanceExtensionProperties`), and two devices alive at
/// once crash lavapipe: `cargo test` runs this binary's tests on threads of
/// one process, as the scheduled `qt-types` job does, so a turn only around
/// making the renderer is not enough.
///
/// Taken first in a test that draws, so it is let go of last, after the
/// harness and its device.
pub fn gpu_turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    // A test that failed holding its turn has dropped its device all the
    // same; the next one may go.
    let turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    // A crash inside the driver prints nothing of its own, and the harness
    // holds back what a test prints until it ends: the last such line
    // names the test a crash was in.
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("a test");
    let _ = writeln!(std::io::stderr(), "ljos_egui: {test} takes the device");
    turn
}
