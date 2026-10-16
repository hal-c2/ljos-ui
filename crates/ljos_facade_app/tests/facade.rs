//! The module above, used as a host uses it: through the facade alone.

use ljos::prelude::*;
use ljos_facade_app::{Counter, Greeter, register};

#[test]
fn a_native_type_works_through_the_facade_alone() {
    let engine = ljos::engine();
    register(&engine);
    let counter = engine.create_by_name("Example", "Counter", None).unwrap();
    engine.invoke_by_name(counter, "increment", &[]).unwrap();
    assert_eq!(engine.get(counter, Counter::COUNT), Ok(1));
}

#[test]
fn a_native_singleton_works_through_the_facade_alone() {
    let engine = ljos::engine();
    register(&engine);
    let greeter = engine.singleton::<Greeter>().unwrap();
    assert_eq!(engine.singleton_by_name("Example", "Greeter"), Some(greeter));
    engine
        .set(greeter, Greeter::GREETING, Atom::from("Hello"))
        .unwrap();
    let said = engine
        .invoke_by_name(greeter, "greet", &[Value::from("world")])
        .unwrap();
    assert_eq!(said, Value::from("Hello world"));
}
