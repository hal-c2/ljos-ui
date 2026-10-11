//! The `QtTest` module: `TestCase` and `SignalSpy`, and the runner that
//! runs the test cases of a compiled test file
//! (doc.qt.io/qt-6/qml-qttest-testcase.html,
//! doc.qt.io/qt-6/qml-qttest-signalspy.html,
//! doc.qt.io/qt-6/qtquicktest-index.html).
//!
//! Three things hold everywhere in it:
//!
//! - **Input is real input.** `mouseClick`, `keyClick`, `touchEvent` and
//!   the others build the events a host builds and give them to the scene
//!   ([`ljos_quick::QuickScene`]); no handler is ever called directly.
//! - **Time is virtual.** A run owns a [`ljos_runtime::VirtualClock`];
//!   `wait(ms)` moves it, `tryCompare` moves it from one timer or frame
//!   to the next up to its timeout. Nothing sleeps, so a suite takes as
//!   long as its own work and gives the same result every time.
//! - **No parser.** The runner is given compiled files; whoever calls it
//!   (the `ljos test` command) compiles them.
//!
//! The behaviour is specified in `features/tooling/qttest-*.feature`.

mod case;
mod compare;
mod host;
mod image;
mod input;
mod report;
mod run;
mod spy;
mod touch;

use std::cell::Cell;

use ljos_engine::{Engine, TypeEntry};

pub use case::TestCase;
pub use image::GrabbedImage;
pub use report::{LineSink, Message, MessageKind, Outcome, Report, TestRecord, Totals, identity};
pub use run::{Filter, Options, Session};
pub use spy::SignalSpy;
pub use touch::TouchEventSequence;

/// The module's URI.
pub const URI: &str = "QtTest";

#[derive(Default)]
struct Registered(Cell<bool>);

/// Registers the `QtTest` module. Call it after `ljos_quick::register`;
/// calling it again does nothing.
pub fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    engine.register_module(ljos_engine::module!("QtTest" 1.0 => [
        TestCase, SignalSpy, TouchEventSequence, GrabbedImage,
    ]));
}

/// What the module's types look like, for a compiler that has to resolve
/// the names a test file uses: the types are native, so they are in no
/// table of Qt's types.
pub fn describe() -> Vec<TypeEntry> {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    register(&engine);
    engine
        .type_dump()
        .modules
        .into_iter()
        .find(|m| m.uri == URI)
        .map(|m| m.types)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
