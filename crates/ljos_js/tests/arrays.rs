//! Arrays that cross to Rust as the script's own array, for a host that
//! asks for it ([`HostObjects::arrays_by_reference`]).

use std::rc::Rc;

use ljos_js::{
    HostError, HostObjects, HostTypeId, JsArg, JsConfig, JsHost, JsOut, JsRef, JsValue,
    ModuleHandle, Scope,
};

/// A host with no objects, whose scripts give their arrays by reference.
struct ByReference;

impl HostObjects for ByReference {
    fn arrays_by_reference(&self) -> bool {
        true
    }

    fn type_of(&self, _obj: u64) -> Option<HostTypeId> {
        None
    }

    fn get<'js>(&self, _scope: &Scope<'js>, _obj: u64, _prop: u32) -> Result<JsOut, HostError> {
        Ok(JsValue::Undefined)
    }

    fn set<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _prop: u32,
        _value: JsArg,
    ) -> Result<(), HostError> {
        Ok(())
    }

    fn call<'js>(
        &self,
        _scope: &Scope<'js>,
        _obj: u64,
        _method: u32,
        _args: Vec<JsArg>,
    ) -> Result<JsOut, HostError> {
        Ok(JsValue::Undefined)
    }
}

const SOURCE: &str = "\
    const kept = [1, [2, 3]];\n\
    export function own() { return kept; }\n\
    export function fresh() { return [1, 2]; }\n\
    export function same(a, b) { return a === b; }\n\
    export function push(a, v) { a.push(v); return a.length; }\n\
    export function join(a) { return a.join('+'); }\n\
    export function isArray(a) { return Array.isArray(a); }\n\
    export function proxy() { return new Proxy([1, 2], {}); }\n";

fn host() -> (JsHost, ModuleHandle) {
    let host = JsHost::new(JsConfig::default()).unwrap();
    host.set_host_objects(Rc::new(ByReference));
    let m = host.load_module("arrays.js", SOURCE).unwrap();
    (host, m)
}

fn call(host: &JsHost, m: &ModuleHandle, name: &str, args: &[JsValue]) -> JsValue {
    host.call(m.function(name).unwrap(), args).unwrap()
}

fn array(v: JsValue) -> JsRef {
    match v {
        JsValue::Handle(r) if r.is_array() => r,
        other => panic!("expected the script's array, got {other:?}"),
    }
}

fn items(host: &JsHost, r: &JsRef) -> Vec<JsValue> {
    host.enter("items", host.config().slice, |s| s.array_items(r))
        .unwrap()
}

#[test]
fn a_host_that_does_not_ask_is_given_copies() {
    let host = JsHost::new(JsConfig::default()).unwrap();
    let m = host.load_module("arrays.js", SOURCE).unwrap();
    assert_eq!(
        call(&host, &m, "fresh", &[]),
        JsValue::Array(vec![1.into(), 2.into()])
    );
}

#[test]
fn an_array_read_twice_is_one_reference() {
    let (host, m) = host();
    let a = array(call(&host, &m, "own", &[]));
    let b = array(call(&host, &m, "own", &[]));
    assert!(JsRef::ptr_eq(&a, &b));
    assert_eq!(a.id(), b.id());
    let other = array(call(&host, &m, "fresh", &[]));
    assert!(!JsRef::ptr_eq(&a, &other));
    // Given back, it is the array the script holds.
    assert_eq!(
        call(
            &host,
            &m,
            "same",
            &[JsValue::Handle(a.clone()), JsValue::Handle(b)]
        ),
        JsValue::Bool(true)
    );
}

#[test]
fn what_a_script_changes_is_what_rust_reads() {
    let (host, m) = host();
    let a = array(call(&host, &m, "fresh", &[]));
    assert_eq!(items(&host, &a), vec![1.into(), 2.into()]);
    assert_eq!(
        call(&host, &m, "push", &[JsValue::Handle(a.clone()), 7.into()]),
        JsValue::Number(3.0)
    );
    assert_eq!(items(&host, &a), vec![1.into(), 2.into(), 7.into()]);
    let (len, last, past) = host
        .enter("entries", host.config().slice, |s| {
            Ok((
                s.array_len(&a)?,
                s.array_entry(&a, 2)?,
                s.array_entry(&a, 3)?,
            ))
        })
        .unwrap();
    assert_eq!(len, 3);
    assert_eq!(last, JsValue::Number(7.0));
    assert_eq!(past, JsValue::Undefined);
}

#[test]
fn an_array_inside_is_read_as_a_copy() {
    let (host, m) = host();
    let a = array(call(&host, &m, "own", &[]));
    assert_eq!(
        items(&host, &a),
        vec![1.into(), JsValue::Array(vec![2.into(), 3.into()])]
    );
}

#[test]
fn an_array_made_in_rust_is_built_once_when_a_script_sees_it() {
    let (host, m) = host();
    let a = JsRef::array(vec![1.into(), 2.into()]);
    assert!(a.is_array());
    assert_eq!(a.pending_items(), Some(vec![1.into(), 2.into()]));
    assert_eq!(a.with_pending(<[JsValue]>::len), Some(2));
    // Read without a script having seen it: the JS heap has no array yet.
    assert_eq!(items(&host, &a), vec![1.into(), 2.into()]);
    assert!(a.pending_items().is_some());
    let id = a.id();

    let arg = || JsValue::Handle(a.clone());
    assert_eq!(call(&host, &m, "isArray", &[arg()]), JsValue::Bool(true));
    assert_eq!(a.pending_items(), None);
    assert_eq!(a.with_pending(<[JsValue]>::len), None);
    assert_eq!(a.id(), id);
    // Every time it crosses it is that one array.
    assert_eq!(
        call(&host, &m, "same", &[arg(), arg()]),
        JsValue::Bool(true)
    );
    assert_eq!(
        call(&host, &m, "push", &[arg(), 9.into()]),
        JsValue::Number(3.0)
    );
    assert_eq!(
        call(&host, &m, "join", &[arg()]),
        JsValue::String("1+2+9".into())
    );
    assert_eq!(items(&host, &a), vec![1.into(), 2.into(), 9.into()]);
}

#[test]
fn a_proxy_of_an_array_is_an_object_not_an_array() {
    let (host, m) = host();
    let JsValue::Handle(p) = call(&host, &m, "proxy", &[]) else {
        panic!("expected a handle");
    };
    // Reading it entry by entry would run its traps.
    assert!(!p.is_array());
    let JsValue::Handle(q) = call(&host, &m, "proxy", &[]) else {
        panic!("expected a handle");
    };
    assert!(!JsRef::ptr_eq(&p, &q));
    assert_eq!(
        call(&host, &m, "isArray", &[JsValue::Handle(p)]),
        JsValue::Bool(true)
    );
}
