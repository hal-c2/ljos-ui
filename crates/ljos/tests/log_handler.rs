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

const SAYS_HELLO: &str =
    "import QtQuick\nItem {\n  Component.onCompleted: console.log(\"hello\", 2)\n}\n";

/// An application of `Main` that says hello as it is made.
fn says_hello() -> ljos::App {
    let source = common::compile_main(&ljos::engine(), SAYS_HELLO);
    ljos::App::new("Test").source(source).root("Main")
}

#[test]
fn an_entry_s_line_is_its_level_and_its_message() {
    let entry = |level| LogEntry {
        level,
        category: "qml".to_owned(),
        message: "hello 2".to_owned(),
    };
    assert_eq!(entry(LogLevel::Debug).line(), "debug: hello 2");
    assert_eq!(entry(LogLevel::Info).line(), "info: hello 2");
    assert_eq!(entry(LogLevel::Warning).line(), "warning: hello 2");
    assert_eq!(entry(LogLevel::Critical).line(), "error: hello 2");
}

#[test]
fn a_built_application_keeps_its_log_unless_told_otherwise() {
    let (engine, _) = says_hello().build().unwrap();
    let kept: Vec<String> = engine.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(kept, ["hello 2"]);

    let (engine, _) = says_hello().log_to_stderr(false).build().unwrap();
    assert_eq!(engine.take_log().len(), 1, "kept, as asked");
}

#[test]
fn the_application_s_own_handler_hears_the_log() {
    let heard: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = Rc::clone(&heard);
    let (engine, _) = says_hello()
        .on_log(move |e| sink.borrow_mut().push(e.line()))
        .build()
        .unwrap();
    assert_eq!(*heard.borrow(), ["debug: hello 2"]);
    assert!(engine.take_log().is_empty(), "the handler took it");
}

#[test]
fn of_on_log_and_log_to_stderr_the_last_call_wins() {
    let heard: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = Rc::clone(&heard);
    let (engine, _) = says_hello()
        .on_log(move |e| sink.borrow_mut().push(e.line()))
        .log_to_stderr(false)
        .build()
        .unwrap();
    assert!(heard.borrow().is_empty());
    assert_eq!(engine.take_log().len(), 1);
}

#[test]
fn a_handler_a_module_sets_wins_over_the_builder_s() {
    let mine: Rc<RefCell<Vec<String>>> = Rc::default();
    let module: Rc<RefCell<Vec<String>>> = Rc::default();
    let (to_mine, to_module) = (Rc::clone(&mine), Rc::clone(&module));
    let (_engine, _) = says_hello()
        .on_log(move |e| to_mine.borrow_mut().push(e.line()))
        .register(move |engine| {
            let sink = Rc::clone(&to_module);
            engine.set_log_handler(move |e| sink.borrow_mut().push(e.line()));
        })
        .build()
        .unwrap();
    assert!(mine.borrow().is_empty(), "{:?}", mine.borrow());
    assert_eq!(*module.borrow(), ["debug: hello 2"]);
}
