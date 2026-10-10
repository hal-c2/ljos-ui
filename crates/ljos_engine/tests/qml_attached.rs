//! Attached properties read and written in expressions: `Keys.enabled` is
//! what `Keys` attached to the object the expression is written in, and
//! `other.Keys.enabled` what it attached to `other`.

mod support;

use ljos_engine::{QmlObject, QmlType, Value};
use support::{Harness, engine};

/// The compiler reads Qt's type table, where `Keys.priority` holds an enum:
/// a plain number for it is an error there, so the files below write `+3`,
/// which is a script.
#[derive(Debug, QmlObject)]
#[qml(name = "KeysAttached")]
pub struct KeysAttached {
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub priority: i32,
}

impl Default for KeysAttached {
    fn default() -> Self {
        KeysAttached {
            enabled: true,
            priority: 0,
        }
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Keys", attached = KeysAttached, uncreatable)]
pub struct Keys {}

fn with_keys() -> Harness {
    let h = engine();
    h.engine
        .register_module(ljos_engine::module!("QtQuick" 2.0 => [Keys]));
    h
}

#[test]
fn the_types_name_alone_is_what_it_attached_to_the_scope_object() {
    let h = with_keys();
    let root = h.load(
        r#"import QtQuick
Item {
  Item {
    objectName: "a"
    Keys.priority: +3
    property int mine: Keys.priority * 2
    property bool on: Keys.enabled
    function raise() { Keys.priority = 5 }
    function inner() { return [1].map(() => Keys.priority)[0] }
  }
  Item { objectName: "b"; property int mine: Keys.priority }
}
"#,
    );
    let (a, b) = (h.find(root, "a"), h.find(root, "b"));
    assert_eq!(h.num(a, "mine"), 6.0);
    assert_eq!(h.get(a, "on"), Value::Bool(true));
    assert_eq!(h.num(b, "mine"), 0.0);
    h.engine.invoke_by_name(a, "raise", &[]).unwrap();
    assert_eq!(h.num(a, "mine"), 10.0);
    assert_eq!(h.num(b, "mine"), 0.0);
    assert_eq!(
        h.engine
            .invoke_by_name(a, "inner", &[])
            .unwrap()
            .to_number(),
        5.0
    );
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn after_an_object_it_is_what_it_attached_to_that_object() {
    let h = with_keys();
    let root = h.load(
        r#"import QtQuick
Item {
  id: root
  Item { id: first; objectName: "first"; Keys.priority: +1 }
  Item {
    objectName: "second"
    Keys.priority: +4
    property int theirs: first.Keys.priority
    property int sum: first.Keys.priority + Keys.priority
    property int ofParent: parent.Keys.priority
    property var plain: ({ Keys: { priority: 7 } })
    property int ofPlain: plain.Keys.priority
    function push() { first.Keys.priority = 6 }
    function pushParent() { parent.Keys.priority = 8 }
  }
}
"#,
    );
    let (first, second) = (h.find(root, "first"), h.find(root, "second"));
    assert_eq!(h.num(second, "theirs"), 1.0);
    assert_eq!(h.num(second, "sum"), 5.0);
    assert_eq!(h.num(second, "ofParent"), 0.0);
    // A member of a JavaScript object is still that member.
    assert_eq!(h.num(second, "ofPlain"), 7.0);
    h.engine.invoke_by_name(second, "push", &[]).unwrap();
    assert_eq!(h.num(second, "theirs"), 6.0);
    assert_eq!(h.num(second, "sum"), 10.0);
    h.engine.invoke_by_name(second, "pushParent", &[]).unwrap();
    assert_eq!(h.num(second, "ofParent"), 8.0);
    // What a script reached is the object a native reader finds.
    let keys = <Keys as QmlType>::static_meta();
    let attached = h
        .engine
        .existing_attached_object(first, &ljos_engine::MetaRef::Static(keys))
        .expect("first has Keys attached");
    assert_eq!(h.num(attached, "priority"), 6.0);
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}
