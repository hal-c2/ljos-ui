//! The device a test draws on, one test at a time.
//!
//! Each test gets a device of its own, so no test sees another's caches or
//! counts. They take turns because each `Offscreen::new` brings up wgpu's
//! Vulkan and GL backends together. On a software driver both are Mesa,
//! and they share one LLVM. Eight tests doing that at once while others JIT
//! and tear their devices down killed a test run with SIGSEGV on lavapipe
//! (CI run 41716115603) and printed nothing. One live device per process is
//! also what a host has (`ljos_render`'s off-screen 3D keeps one).
//!
//! A crash inside the driver prints nothing of its own, and the harness
//! holds back what a test prints until it ends. So each turn writes the
//! test's name straight to the process's stderr: the last such line names
//! the test a crash was in.

use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard, PoisonError};

use ljos_3d::offscreen::Offscreen;

static TURN: Mutex<()> = Mutex::new(());

/// A test's device, held until the test lets go of it.
pub struct Device {
    // Declared before the turn, so the device is gone before the next test
    // makes its own.
    pub offscreen: Offscreen,
    _turn: MutexGuard<'static, ()>,
}

impl Deref for Device {
    type Target = Offscreen;
    fn deref(&self) -> &Offscreen {
        &self.offscreen
    }
}

impl DerefMut for Device {
    fn deref_mut(&mut self) -> &mut Offscreen {
        &mut self.offscreen
    }
}

/// The calling test's turn on a device of its own; `None`, said, when there
/// is no adapter.
pub fn offscreen() -> Option<Device> {
    // A test that failed holding its turn has dropped its device all the
    // same; the next one may go.
    let turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("a test");
    let _ = writeln!(std::io::stderr(), "ljos_3d: {test} takes the device");
    match Offscreen::new() {
        Ok(offscreen) => Some(Device {
            offscreen,
            _turn: turn,
        }),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}
