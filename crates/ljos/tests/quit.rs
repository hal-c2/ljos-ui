//! A Rust application asks to quit as QML does with `Qt.quit()`.

use std::cell::Cell;
use std::rc::Rc;

#[test]
fn request_quit_is_seen_by_the_host_and_wakes_it() {
    let engine = ljos::engine();
    let woken = Rc::new(Cell::new(0));
    let w = Rc::clone(&woken);
    engine.set_timer_wake(move || w.set(w.get() + 1));
    assert!(!engine.quit_requested());

    engine.request_quit();

    assert!(engine.quit_requested());
    assert!(woken.get() > 0, "the host was not woken");
}

#[test]
fn qt_quit_and_request_quit_are_the_same_request() {
    let engine = ljos::engine();
    engine.eval_js("Qt.quit()").unwrap();
    assert!(engine.quit_requested());
}
