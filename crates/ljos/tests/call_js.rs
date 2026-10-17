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

fn thrown(engine: &Engine, body: &str) -> JsError {
    let f = engine
        .eval_js(&format!("(function () {{ {body} }})"))
        .unwrap();
    engine.try_call_js(&f, &[]).unwrap_err()
}

fn value_of(err: &JsError) -> &Value {
    let JsError::Thrown { value, .. } = err else {
        panic!("not a throw: {err:?}");
    };
    value
}

#[test]
fn an_error_s_own_message_can_be_had() {
    let engine = ljos::engine();
    let err = thrown(&engine, "throw new Error('no')");
    assert_eq!(err.to_string(), "Error: no");
    assert_eq!(err.message(&engine), "no");
    let value = value_of(&err);
    assert_eq!(engine.js_member(value, "name"), Some(Value::from("Error")));
}

#[test]
fn a_subclass_keeps_its_name_and_message() {
    let engine = ljos::engine();
    let err = thrown(
        &engine,
        "class NotFound extends Error { constructor(m) { super(m); this.name = 'NotFound'; this.code = 404 } }
         throw new NotFound('gone')",
    );
    assert_eq!(err.to_string(), "NotFound: gone");
    assert_eq!(err.message(&engine), "gone");
    let value = value_of(&err);
    assert_eq!(
        engine.js_member(value, "name"),
        Some(Value::from("NotFound"))
    );
    assert_eq!(engine.js_member(value, "code"), Some(Value::Real(404.0)));
}

#[test]
fn a_thrown_object_keeps_its_fields() {
    let engine = ljos::engine();
    let err = thrown(&engine, "throw { message: 'no', code: 7 }");
    assert_eq!(err.to_string(), "[object Object]");
    assert_eq!(err.message(&engine), "no");
    let value = value_of(&err);
    assert_eq!(engine.js_member(value, "code"), Some(Value::Real(7.0)));

    // An object with no message of its own is told by what it prints as.
    let err = thrown(&engine, "throw { code: 7 }");
    assert_eq!(err.message(&engine), "[object Object]");
}

#[test]
fn a_thrown_primitive_is_the_value() {
    let engine = ljos::engine();
    let err = thrown(&engine, "throw 'plain words'");
    assert_eq!(value_of(&err), &Value::from("plain words"));
    assert_eq!(err.message(&engine), "plain words");

    let err = thrown(&engine, "throw 42");
    assert_eq!(value_of(&err), &Value::Real(42.0));
    assert_eq!(err.message(&engine), "42");
}

#[test]
fn a_thrown_value_may_outlive_its_engine() {
    let engine = ljos::engine();
    let err = thrown(&engine, "throw { message: 'no' }");
    drop(engine);
    // The object went with the engine's JS heap; what printed is kept.
    assert_eq!(err.to_string(), "[object Object]");
    drop(err);
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
