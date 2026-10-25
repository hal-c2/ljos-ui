//! The system's fonts are read on a thread of their own: fontconfig takes
//! 20–25 ms to load its configuration and caches, which used to hold up
//! the main thread before anything else was built.

use std::{
    sync::Mutex,
    thread::{self, JoinHandle},
};

use ljos_quick::TextEngine;

static READING: Mutex<Option<JoinHandle<TextEngine>>> = Mutex::new(None);

/// Starts reading them unless that has been started. The drivers are
/// chosen first: that sets the environment, which fontconfig and the
/// threads the reader starts read from C while it is set.
pub(crate) fn start() {
    start_in(&READING, choose_drivers, TextEngine::with_system_fonts);
}

fn choose_drivers() {
    #[cfg(target_os = "linux")]
    crate::drivers::choose();
}

fn start_in<T: Send + 'static>(
    reading: &Mutex<Option<JoinHandle<T>>>,
    choose: impl FnOnce(),
    read: impl FnOnce() -> T + Send + 'static,
) {
    let mut reading = reading
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if reading.is_none() {
        choose();
        *reading = thread::Builder::new()
            .name("ljos-fonts".into())
            .spawn(read)
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Once};

    use super::*;

    type Log = Arc<Mutex<Vec<&'static str>>>;

    fn note(log: &Log, what: &'static str) {
        log.lock().unwrap().push(what);
    }

    #[test]
    fn the_drivers_are_chosen_before_the_fonts_are_read() {
        let reading = Mutex::new(None);
        let log = Log::default();
        let read_log = log.clone();
        start_in(
            &reading,
            || note(&log, "chosen"),
            move || note(&read_log, "read"),
        );
        reading.lock().unwrap().take().unwrap().join().unwrap();
        assert_eq!(*log.lock().unwrap(), ["chosen", "read"]);
    }

    #[test]
    fn an_early_start_and_run_choose_and_read_once() {
        // `run` chooses through the same `Once` as the early start.
        let reading = Mutex::new(None);
        let chosen = Once::new();
        let log = Log::default();
        let early = |log: &Log| {
            let read_log = log.clone();
            start_in(
                &reading,
                || chosen.call_once(|| note(log, "chosen")),
                move || note(&read_log, "read"),
            );
        };
        early(&log);
        early(&log);
        chosen.call_once(|| note(&log, "chosen"));
        reading.lock().unwrap().take().unwrap().join().unwrap();
        assert_eq!(*log.lock().unwrap(), ["chosen", "read"]);
    }
}
