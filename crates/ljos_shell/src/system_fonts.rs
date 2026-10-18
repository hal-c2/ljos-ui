//! The system's fonts are read on a thread of their own: fontconfig takes
//! 20–25 ms to load its configuration and caches, which used to hold up
//! the main thread before anything else was built.

use std::{
    sync::Mutex,
    thread::{self, JoinHandle},
};

use ljos_quick::TextEngine;

static READING: Mutex<Option<JoinHandle<TextEngine>>> = Mutex::new(None);

/// Starts reading them unless that has been started.
pub(crate) fn start() {
    let mut reading = READING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if reading.is_none() {
        *reading = thread::Builder::new()
            .name("ljos-fonts".into())
            .spawn(TextEngine::with_system_fonts)
            .ok();
    }
}

/// The reading started, or one started now; `None` when no thread could
/// be had.
pub(crate) fn take() -> Option<JoinHandle<TextEngine>> {
    start();
    READING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// Forgets a reading nothing will use. The thread ends by itself.
pub(crate) fn forget() {
    READING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
}
