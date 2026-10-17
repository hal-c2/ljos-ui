//! A `TestWindow`'s clock starts on the timer grid, so letting a timer's
//! interval pass fires it, wherever the engine's own clock was.

use std::cell::Cell;
use std::rc::Rc;

use ljos::prelude::*;
use ljos::testing::TestWindow;

#[test]
fn advancing_by_a_timers_interval_fires_it() {
    let engine = ljos::engine();
    // The engine's clock counts microseconds since it was made: wait for
    // one between whole milliseconds, where the window used to start.
    while engine.now().as_micros().is_multiple_of(1000) {}
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let timer = engine
        .create_native(
            Timer {
                interval: 100,
                running: true,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let fired = Rc::new(Cell::new(0));
    let count = Rc::clone(&fired);
    engine
        .connect_by_name(timer, "triggered", move |_, _| count.set(count.get() + 1))
        .unwrap();
    window.advance(99).unwrap();
    assert_eq!(fired.get(), 0);
    window.advance(1).unwrap();
    assert_eq!(fired.get(), 1);
}
