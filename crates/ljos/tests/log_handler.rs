//! An application hears what its QML says: `console.log` and the
//! engine's warnings reach the Engine's log handler, the one place they
//! go.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

#[test]
fn the_log_handler_hears_console_output_and_a_binding_warning() {
    let engine = ljos::engine();
    let heard: Rc<RefCell<Vec<LogEntry>>> = Rc::default();
    let sink = Rc::clone(&heard);
    engine.set_log_handler(move |entry| sink.borrow_mut().push(entry.clone()));
    let root = common::create_main(
        &engine,
        "import QtQuick\nItem {\n  property var said: \"words\"\n  property int count: said\n  Component.onCompleted: console.log(\"hello\", 2)\n}\n",
    );
    // A binding runs when it is first read.
    engine.read_by_name(root, "count");
    let heard = heard.borrow();
    assert!(
        heard
            .iter()
            .any(|e| e.level == LogLevel::Debug && e.category == "qml" && e.message == "hello 2"),
        "{heard:#?}"
    );
    assert!(
        heard.iter().any(|e| e.level == LogLevel::Warning
            && e.message.contains("Unable to assign QString to int")),
        "{heard:#?}"
    );
    assert!(engine.take_log().is_empty(), "the handler took every entry");
}
