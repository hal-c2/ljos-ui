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

#[test]
fn a_listener_hears_the_log_whatever_the_handler_is() {
    let engine = ljos::engine();
    let heard: Rc<RefCell<Vec<String>>> = Rc::default();
    let handled: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = Rc::clone(&heard);
    let id = engine.add_log_listener(move |e| sink.borrow_mut().push(e.message.clone()));

    // With no handler the line is also kept, as it is with no listener.
    engine.log(LogLevel::Info, "app", "kept");
    assert_eq!(engine.take_log().len(), 1);

    let sink = Rc::clone(&handled);
    engine.set_log_handler(move |e| sink.borrow_mut().push(e.message.clone()));
    engine.log(LogLevel::Warning, "app", "both");
    assert_eq!(*handled.borrow(), ["both"]);
    assert_eq!(*heard.borrow(), ["kept", "both"]);

    engine.remove_log_listener(id);
    engine.log(LogLevel::Info, "app", "gone");
    assert_eq!(*heard.borrow(), ["kept", "both"]);
    assert_eq!(*handled.borrow(), ["both", "gone"]);
}
