//! Whether closing the last window ends the application, said from Rust
//! as `QGuiApplication::setQuitOnLastWindowClosed` and read in QML as
//! `Qt.application.quitOnLastWindowClosed`
//! (features/window/keep-running.feature).

use ljos::prelude::*;

fn quits(engine: &Engine) -> Value {
    let app = engine.singleton::<Application>().expect("the singleton");
    engine.read(app, Application::QUIT_ON_LAST_WINDOW_CLOSED.index)
}

#[test]
fn an_application_ends_with_its_last_window_unless_it_says_otherwise() {
    let engine = ljos::App::new("Notes").build_engine();
    assert_eq!(quits(&engine), Value::Bool(true));
}

#[test]
fn the_app_builder_keeps_the_application_running_without_a_window() {
    let engine = ljos::App::new("Notes")
        .quit_on_last_window_closed(false)
        .build_engine();
    assert_eq!(quits(&engine), Value::Bool(false));
}
