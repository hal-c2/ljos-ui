//! Rust calls a JS function it holds and hears why the call failed, as
//! `QJSValue::isCallable` and an error value from `QJSValue::call` tell.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

fn warnings(engine: &Engine) -> Rc<RefCell<Vec<String>>> {
    let logged = Rc::new(RefCell::new(Vec::new()));
    let l = Rc::clone(&logged);
    engine.set_log_handler(move |e| {
        if matches!(e.level, LogLevel::Warning | LogLevel::Critical) {
            l.borrow_mut().push(e.message.to_string());
        }
    });
    logged
}

#[test]
fn only_a_function_is_callable() {
    let engine = ljos::engine();
    let f = engine
        .eval_js("(function (a, b) { return a + b })")
        .unwrap();
    let object = engine.eval_js("({})").unwrap();
    assert!(engine.is_callable(&f));
    assert!(!engine.is_callable(&object));
    assert!(!engine.is_callable(&Value::Int(3)));
    assert!(!engine.is_callable(&Value::from("f")));
}

#[test]
fn a_call_that_returns_gives_its_value() {
    let engine = ljos::engine();
    let f = engine
        .eval_js("(function (a, b) { return a + b })")
        .unwrap();
    let got = engine.try_call_js(&f, &[Value::Int(2), Value::Int(3)]);
    assert_eq!(got.ok().and_then(|v| v.as_number()), Some(5.0));
}

#[test]
fn a_throw_comes_back_with_its_message_and_is_not_logged() {
    let engine = ljos::engine();
    let logged = warnings(&engine);
    let throws = engine
        .eval_js("(function () { throw new Error('why it failed') })")
        .unwrap();

    let err = engine.try_call_js(&throws, &[]).unwrap_err();

    let JsError::Thrown { message, line, .. } = &err else {
        panic!("not a throw: {err:?}");
    };
    assert_eq!(message, "Error: why it failed");
    assert_eq!(*line, Some(1));
    assert_eq!(err.to_string(), "Error: why it failed");
    assert!(logged.borrow().is_empty(), "logged: {:?}", logged.borrow());
}

#[test]
fn what_is_not_a_function_is_refused() {
    let engine = ljos::engine();
    let object = engine.eval_js("({})").unwrap();
    assert_eq!(engine.try_call_js(&object, &[]), Err(JsError::NotCallable));
    assert_eq!(
        engine.try_call_js(&Value::Int(3), &[]),
        Err(JsError::NotCallable)
    );
}

#[test]
fn call_js_still_logs_and_gives_none() {
    let engine = ljos::engine();
    let logged = warnings(&engine);
    let throws = engine
        .eval_js("(function () { throw new Error('why it failed') })")
        .unwrap();
    assert_eq!(engine.call_js(&throws, &[]), None);
    assert!(
        logged.borrow().iter().any(|m| m.contains("why it failed")),
        "logged: {:?}",
        logged.borrow()
    );
}

#[test]
fn try_call_value_binds_this() {
    let engine = ljos::engine();
    let obj = engine
        .create_by_name("QtQuick", "Item", None)
        .expect("an Item");
    let f = engine
        .eval_js("(function () { return this === null ? 'null' : typeof this })")
        .unwrap();
    assert_eq!(
        engine.try_call_value(obj, &f, &[]),
        Ok(Value::from("object"))
    );
    let throws = engine
        .eval_js("(function () { throw new TypeError('bad ' + typeof this) })")
        .unwrap();
    let err = engine.try_call_value(obj, &throws, &[]).unwrap_err();
    assert_eq!(err.to_string(), "TypeError: bad object");
}
