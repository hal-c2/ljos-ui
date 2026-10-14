//! `PropertyMap` (ARCHITECTURE.md §17): a map the application makes and
//! hands to QML, as Qt's documentation gives a `QQmlPropertyMap` to the
//! UI layer as the context property `owner`.
//! `features/models/property-map.feature` holds the behaviour measured
//! on Qt 6.11.2; its scenarios wait for a step that can hand a scenario
//! a map, so these tests pin the behaviour meanwhile.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{ObjectId, PropertyMap, Value};
use support::{Harness, engine};

const MAIN: &str = r#"import QtQuick
Item {
  id: root
  objectName: "root"
  property string shown: owner.name + " " + owner.phone
  property string nameOnly: owner.name
  property int nameChanges: 0
  onNameOnlyChanged: nameChanges++
  property string nameKind: typeof owner.name
  property string changed: ""
  property string keyList: ""
  function rename(v) { owner.name = v }
  function listKeys() { keyList = owner.keys().join(",") }
  Connections {
    target: owner
    function onValueChanged(key, value) { root.changed = key + "=" + value }
  }
}
"#;

/// A map holding the Qt documentation example's two keys, given to the
/// QML that `MAIN` is loaded as `owner`.
fn given(h: &Harness) -> (PropertyMap, ObjectId) {
    let map = PropertyMap::new(&h.engine, None).expect("map creates");
    map.insert(&h.engine, "name", Value::from("John Smith"));
    map.insert(&h.engine, "phone", Value::from("555-5555"));
    h.engine
        .set_context_id(h.engine.root_context(), "owner", map.object());
    let root = h.load(MAIN);
    (map, root)
}

fn call(h: &Harness, obj: ObjectId, method: &str, args: &[Value]) {
    h.engine
        .invoke_by_name(obj, method, args)
        .unwrap_or_else(|e| panic!("{method}: {e}"));
}

#[test]
fn bindings_read_keys_and_follow_each_on_its_own() {
    let h = engine();
    let (map, root) = given(&h);
    assert_eq!(h.get(root, "shown"), Value::from("John Smith 555-5555"));
    // The change handler already ran for the binding's first value.
    let runs = h.num(root, "nameChanges");
    map.insert(&h.engine, "phone", Value::from("111"));
    assert_eq!(h.get(root, "shown"), Value::from("John Smith 111"));
    // The binding on `name` was not re-run by `phone`'s change.
    assert_eq!(h.num(root, "nameChanges"), runs);
    // The same value again changes nothing.
    map.insert(&h.engine, "name", Value::from("John Smith"));
    assert_eq!(h.num(root, "nameChanges"), runs);
    map.insert(&h.engine, "name", Value::from("Zed"));
    assert_eq!(h.num(root, "nameChanges"), runs + 1.0);
    assert_eq!(h.get(root, "nameOnly"), Value::from("Zed"));
}

#[test]
fn a_qml_write_is_told_and_an_application_write_is_not() {
    let h = engine();
    let (map, root) = given(&h);
    let told: Rc<RefCell<Vec<(String, Value)>>> = Rc::default();
    let seen = told.clone();
    map.on_value_changed(&h.engine, move |key, value| {
        seen.borrow_mut().push((key.to_owned(), value.clone()));
    })
    .expect("connects");

    map.insert(&h.engine, "phone", Value::from("111"));
    assert!(told.borrow().is_empty());
    assert_eq!(h.get(root, "changed"), Value::from(""));

    call(&h, root, "rename", &[Value::from("Zed")]);
    assert_eq!(
        *told.borrow(),
        vec![("name".to_owned(), Value::from("Zed"))]
    );
    assert_eq!(h.get(root, "changed"), Value::from("name=Zed"));
    assert_eq!(map.value(&h.engine, "name"), Value::from("Zed"));

    // Writing the value the key already has tells nobody.
    call(&h, root, "rename", &[Value::from("Zed")]);
    assert_eq!(told.borrow().len(), 1);
}

#[test]
fn keys_keep_their_order_and_a_cleared_key_stays() {
    let h = engine();
    let (map, root) = given(&h);
    call(&h, root, "listKeys", &[]);
    assert_eq!(h.get(root, "keyList"), Value::from("name,phone"));
    assert_eq!(
        map.keys().iter().map(|k| k.to_string()).collect::<Vec<_>>(),
        ["name", "phone"]
    );
    assert_eq!(map.size(), 2);
    assert!(!map.is_empty());

    map.clear(&h.engine, "name");
    assert!(map.contains("name"));
    assert_eq!(map.value(&h.engine, "name"), Value::Undefined);
    assert_eq!(h.get(root, "nameKind"), Value::from("undefined"));
    call(&h, root, "listKeys", &[]);
    assert_eq!(h.get(root, "keyList"), Value::from("name,phone"));

    // Clearing a key the map has not got adds it, with no value.
    map.clear(&h.engine, "email");
    assert!(map.contains("email"));
    assert_eq!(map.size(), 3);
}

#[test]
fn a_frozen_map_takes_no_new_keys() {
    let h = engine();
    let (map, root) = given(&h);
    map.freeze();
    map.insert(&h.engine, "email", Value::from("z@x.is"));
    assert!(!map.contains("email"));
    map.clear(&h.engine, "email");
    assert!(!map.contains("email"));
    map.insert(&h.engine, "name", Value::from("Zed"));
    assert_eq!(h.get(root, "shown"), Value::from("Zed 555-5555"));
    // An existing key still takes a write from QML.
    call(&h, root, "rename", &[Value::from("Ann")]);
    assert_eq!(map.value(&h.engine, "name"), Value::from("Ann"));
}

#[test]
fn whole_map_insert_and_the_handle_of_an_object() {
    let h = engine();
    let map = PropertyMap::new(&h.engine, None).expect("map creates");
    assert!(map.is_empty());
    map.insert_many(
        &h.engine,
        [
            ("a".to_owned(), Value::Int(1)),
            ("b".to_owned(), Value::from("two")),
        ],
    );
    assert_eq!(map.size(), 2);
    assert_eq!(map.value(&h.engine, "b"), Value::from("two"));
    assert_eq!(map.value(&h.engine, "missing"), Value::Undefined);

    let again = PropertyMap::from_object(&h.engine, map.object()).expect("the same map");
    again.insert(&h.engine, "a", Value::Int(3));
    assert_eq!(map.value(&h.engine, "a"), Value::Int(3));

    let keys = h
        .engine
        .invoke_by_name(map.object(), "keys", &[])
        .expect("keys() is invokable");
    // A list value compares by identity, so read its items out.
    let Value::List(items) = h.engine.plain(&keys) else {
        panic!("keys() gave {keys:?}");
    };
    assert_eq!(
        items
            .iter()
            .map(|v| v.to_js_string().to_string())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}
