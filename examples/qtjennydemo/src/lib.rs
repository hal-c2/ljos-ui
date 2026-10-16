//! The Rust backend of the qtdoc **qtjennydemo** example
//! (`corpus/qtdoc/examples/demos/qtjennydemo`), written against the
//! public `ljos` facade alone.
//!
//! The demo's `Main.qml` drives one native type, `BackEnd` (module
//! `qtjenny_consumer`). Its interface comes from what that file
//! reads and calls (CLEAN_ROOM.md: the C++ was never opened):
//! `minVolume`, `maxVolume`, `volume`, `brightness`,
//! `canWriteSystemSettings`, the signal `showPopup(message)`, and
//! the methods `manageWriteSystemSettings`, `vibrate`, `notify`,
//! `setFullWakeLock`, `disableFullWakeLock`, `setPartialWakeLock`
//! and `disablePartialWakeLock`.
//!
//! The demo is about Android services. On a desktop none exists, so
//! [`device::Device::desktop`] has no capability: the volume has no
//! range, brightness writes, vibration, notifications and wake
//! locks are refused and logged, and `showPopup` fires once to say
//! system settings cannot be written. Nothing pretends to work.
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_example_qtjennydemo::register(&engine);
//! ```

pub mod device;
pub mod qml;

/// The one seam a host calls: registers `BackEnd` on `engine`.
pub fn register(engine: &ljos::Engine) {
    qml::register(engine);
}
