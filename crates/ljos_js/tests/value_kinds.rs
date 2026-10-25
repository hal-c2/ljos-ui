//! Value kinds an embedder defines: a JS object the host knows by its
//! prototype and carries across as its fields, so that what comes back is
//! the value and not a handle to some object.

use ljos_js::{JsConfig, JsHost, JsValue, ModuleHandle, ValueKind};

/// A pair that is one frozen object per distinct value, as an embedder
/// that wants `===` by value writes it.
const PAIR: &str = r#"(() => {
  const proto = { toString() { return `Pair(${this.a}, ${this.b})`; } };
  const seen = new Map();
  globalThis.made = 0;
  const make = (a, b) => {
    globalThis.made += 1;
    const key = a + "," + b;
    let it = seen.get(key);
    if (!it) {
      it = Object.freeze(Object.assign(Object.create(proto), { a, b }));
      seen.set(key, it);
    }
    return it;
  };
  globalThis.pair = make;
  return { make, proto, fields: ["a", "b"] };
})()"#;

/// A second kind, to see that the two are told apart.
const NAMED: &str = r#"(() => {
  const proto = {};
  const make = (name) => Object.assign(Object.create(proto), { name });
  globalThis.named = make;
  return { make, proto, fields: ["name"] };
})()"#;

fn host() -> (JsHost, ValueKind, ValueKind) {
    let host = JsHost::new(JsConfig::default()).unwrap();
    let pair = host.define_value_kind(PAIR).unwrap();
    let named = host.define_value_kind(NAMED).unwrap();
    (host, pair, named)
}

fn module(host: &JsHost, source: &str) -> ModuleHandle {
    host.load_module("m.js", source).unwrap()
}

fn call(host: &JsHost, m: &ModuleHandle, name: &str, args: &[JsValue]) -> JsValue {
    host.call(m.function(name).unwrap(), args).unwrap()
}

#[test]
fn kinds_are_numbered_in_the_order_they_are_defined() {
    let (_host, pair, named) = host();
    assert_eq!(pair, ValueKind::nth(0));
    assert_eq!(named, ValueKind::nth(1));
    assert_eq!((pair.index(), named.index()), (0, 1));
}

#[test]
fn a_value_crosses_both_ways_as_its_fields() {
    let (host, pair, named) = host();
    let m = module(
        &host,
        "export function id(x) { return x; }
         export function made() { return pair(3, 'x'); }
         export function who() { return [named('n'), pair(1, 2)]; }
         export function text(x) { return '' + x; }",
    );
    let v = JsValue::Typed(pair, vec![JsValue::Number(1.0), JsValue::Number(2.5)]);
    assert_eq!(call(&host, &m, "id", std::slice::from_ref(&v)), v);
    assert_eq!(
        call(&host, &m, "made", &[]),
        JsValue::Typed(
            pair,
            vec![JsValue::Number(3.0), JsValue::String("x".into())]
        )
    );
    assert_eq!(
        call(&host, &m, "who", &[]),
        JsValue::Array(vec![
            JsValue::Typed(named, vec![JsValue::String("n".into())]),
            JsValue::Typed(pair, vec![JsValue::Number(1.0), JsValue::Number(2.0)]),
        ])
    );
    // What arrives is an object of the kind, with the kind's methods.
    assert_eq!(
        call(&host, &m, "text", &[v]),
        JsValue::String("Pair(1, 2.5)".into())
    );
}

#[test]
fn a_kind_that_keeps_one_object_per_value_compares_by_value() {
    let (host, pair, named) = host();
    let m = module(
        &host,
        "export function same(a, b) { return a === b; }
         export function mine(a) { return a === pair(1, 2); }",
    );
    let one = || JsValue::Typed(pair, vec![JsValue::Number(1.0), JsValue::Number(2.0)]);
    let other = JsValue::Typed(pair, vec![JsValue::Number(1.0), JsValue::Number(3.0)]);
    assert_eq!(
        call(&host, &m, "same", &[one(), one()]),
        JsValue::Bool(true)
    );
    assert_eq!(
        call(&host, &m, "same", &[one(), other]),
        JsValue::Bool(false)
    );
    assert_eq!(call(&host, &m, "mine", &[one()]), JsValue::Bool(true));
    // The second kind makes a new object each time, so it does not.
    let n = || JsValue::Typed(named, vec![JsValue::String("n".into())]);
    assert_eq!(call(&host, &m, "same", &[n(), n()]), JsValue::Bool(false));
}

#[test]
fn an_object_that_only_looks_like_one_stays_an_object() {
    let (host, _, _) = host();
    let m = module(
        &host,
        "export function plain() { return { a: 1, b: 2 }; }
         export function child() { return Object.create(pair(1, 2)); }",
    );
    assert!(matches!(call(&host, &m, "plain", &[]), JsValue::Handle(_)));
    // Only the kind's own prototype counts, not one further up.
    assert!(matches!(call(&host, &m, "child", &[]), JsValue::Handle(_)));
}

#[test]
fn a_definition_that_is_not_a_kind_is_refused() {
    let host = JsHost::new(JsConfig::default()).unwrap();
    assert!(host.define_value_kind("({ make: 1 })").is_err());
    assert!(host.define_value_kind("nothing here").is_err());
}

#[test]
fn a_frozen_value_that_crosses_again_is_not_made_again() {
    let (host, pair, named) = host();
    let m = module(
        &host,
        "export function id(x) { return x; }
         export function made() { return globalThis.made; }
         export function back() { return pair(5, 6); }",
    );
    let v = JsValue::Typed(pair, vec![JsValue::Number(1.0), JsValue::Number(2.0)]);
    for _ in 0..3 {
        assert_eq!(call(&host, &m, "id", std::slice::from_ref(&v)), v);
    }
    assert_eq!(call(&host, &m, "made", &[]), JsValue::Number(1.0));
    // One that came from JS goes back as the object it was.
    let w = call(&host, &m, "back", &[]);
    assert_eq!(call(&host, &m, "id", std::slice::from_ref(&w)), w);
    assert_eq!(call(&host, &m, "made", &[]), JsValue::Number(2.0));
    // An object a script may still change is made each time.
    let m2 = module(
        &host,
        "export function grow(x) { x.name += '!'; return x; }",
    );
    let n = JsValue::Typed(named, vec![JsValue::String("n".into())]);
    let once = JsValue::Typed(named, vec![JsValue::String("n!".into())]);
    assert_eq!(call(&host, &m2, "grow", std::slice::from_ref(&n)), once);
    assert_eq!(call(&host, &m2, "grow", &[n]), once);
}
