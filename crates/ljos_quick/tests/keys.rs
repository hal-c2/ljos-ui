//! Key delivery from the host's side: what `key_event` returns, the hooks
//! an item type gets between its `Keys` handlers, shortcuts across two
//! windows of one engine, and the text input seam. What keys look like
//! from QML is in `features/input/`.

mod common;

use std::ops::Range;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::keys::{
    KeyItem, Surrounding, TextInputClient, key, register_key_item, register_text_input_client,
};
use ljos_quick::{KeyEvent, Modifiers, QuickScene, Rectangle};
use ljos_render::kurbo;

fn ready(e: &Engine, source: &str) -> (ObjectId, QuickScene) {
    let root = load(e, source);
    let mut s = scene(e, root, 200.0, 200.0);
    frame(&mut s, e);
    (root, s)
}

fn log(e: &Engine, root: ObjectId) -> String {
    get(e, root, "log").to_js_string().trim_end().to_owned()
}

fn note(e: &Engine, item: ObjectId, what: &str) {
    // The hooks have nothing but the item: the log is on the root.
    let mut root = item;
    while let Some(p) = e.visual_parent(root) {
        root = p;
    }
    let so_far = get(e, root, "log").to_js_string().to_string();
    set(
        e,
        root,
        "log",
        Value::from(format!("{so_far}{what} ").as_str()),
    );
}

fn ctrl(k: i32) -> KeyEvent {
    KeyEvent::press(k).with_modifiers(Modifiers::CONTROL)
}

/// Press and release; whether the press was accepted.
fn hit(s: &mut QuickScene, e: &Engine, ev: KeyEvent) -> bool {
    let mut up = ev.clone();
    up.kind = ljos_quick::KeyKind::Release;
    let accepted = s.key_event(e, &ev);
    s.key_event(e, &up);
    accepted
}

#[test]
fn key_event_says_whether_anything_took_the_key() {
    let e = engine();
    let (root, mut s) = ready(
        &e,
        r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property string log: ""
    Item {
        focus: true
        Keys.onReturnPressed: root.log += "return "
        Keys.onPressed: (event) => { root.log += "pressed "; event.accepted = event.key === Qt.Key_A }
        Keys.onReleased: (event) => { root.log += "released "; event.accepted = event.key === Qt.Key_A }
    }
}
"#,
    );
    assert!(s.key_event(&e, &KeyEvent::press(key::A).with_text("a")));
    assert!(s.key_event(&e, &KeyEvent::release(key::A).with_text("a")));
    assert!(!s.key_event(&e, &KeyEvent::press(key::A + 1).with_text("b")));
    assert!(!s.key_event(&e, &KeyEvent::release(key::A + 1).with_text("b")));
    // A per-key handler accepts by being there.
    assert!(s.key_event(&e, &KeyEvent::press(key::RETURN).with_text("\r")));
    assert_eq!(log(&e, root), "pressed released pressed released return");
    assert!(e.take_warnings().is_empty());
}

#[test]
fn a_scene_without_a_root_takes_no_keys() {
    let e = engine();
    let mut s = QuickScene::new(kurbo::Size::new(10.0, 10.0));
    assert!(!s.key_event(&e, &KeyEvent::press(key::A)));
    s.set_window_active(&e, false);
    assert!(!s.is_window_active(&e));
    assert_eq!(s.active_focus_item(&e), None);
}

const HOOKED: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property string log: ""
    property bool take: false
    property bool claim: false
    Shortcut { sequence: "Backspace"; onActivated: root.log += "shortcut " }
    Rectangle {
        objectName: "field"
        focus: true
        property bool take: root.take
        property bool claim: root.claim
        Keys.onPressed: root.log += "keys "
        Keys.onReleased: root.log += "keys-up "
    }
}
"#;

fn hooks() -> KeyItem {
    KeyItem {
        shortcut_override: |e, item, _| get(e, item, "claim").to_boolean(),
        press: |e, item, _| {
            note(e, item, "item");
            get(e, item, "take").to_boolean()
        },
        release: |e, item, _| {
            note(e, item, "item-up");
            get(e, item, "take").to_boolean()
        },
    }
}

fn hooked(e: &Engine) -> (ObjectId, QuickScene) {
    register_key_item(e, MetaRef::Static(Rectangle::static_meta()), hooks());
    ready(e, HOOKED)
}

#[test]
fn an_item_type_sees_keys_after_the_before_item_handlers() {
    let e = engine();
    let (root, mut s) = hooked(&e);
    assert!(!hit(&mut s, &e, KeyEvent::press(key::A).with_text("a")));
    assert_eq!(log(&e, root), "keys item keys-up item-up");
}

#[test]
fn an_item_type_sees_keys_before_the_after_item_handlers() {
    let e = engine();
    let (root, mut s) = hooked(&e);
    let field = find(&e, root, "field");
    let keys = e
        .existing_attached_object(
            field,
            &MetaRef::Static(ljos_quick::keys::Keys::static_meta()),
        )
        .expect("the Keys object of the field");
    set(
        &e,
        keys,
        "priority",
        Value::Int(ljos_quick::keys::priority::AFTER_ITEM),
    );
    assert!(!hit(&mut s, &e, KeyEvent::press(key::A).with_text("a")));
    assert_eq!(log(&e, root), "item keys item-up keys-up");

    // What the item takes, the handlers after it never see.
    set(&e, root, "log", Value::from(""));
    set(&e, root, "take", Value::Bool(true));
    assert!(hit(&mut s, &e, KeyEvent::press(key::A).with_text("a")));
    assert_eq!(log(&e, root), "item item-up");
}

#[test]
fn an_item_type_can_keep_a_key_from_the_shortcuts() {
    let e = engine();
    let (root, mut s) = hooked(&e);
    assert!(hit(&mut s, &e, KeyEvent::press(key::BACKSPACE)));
    assert_eq!(log(&e, root), "shortcut keys-up item-up");

    set(&e, root, "log", Value::from(""));
    set(&e, root, "claim", Value::Bool(true));
    hit(&mut s, &e, KeyEvent::press(key::BACKSPACE));
    assert_eq!(log(&e, root), "keys item keys-up item-up");
}

const TWO_CHORDS: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    focus: true
    property string log: ""
    Keys.onPressed: (event) => root.log += "pressed "
    Shortcut { sequence: "Ctrl+K, Ctrl+C"; onActivated: root.log += "comment " }
    Shortcut { sequence: "F5"; autoRepeat: false; onActivated: root.log += "reload " }
    Shortcut { sequence: "F6"; onActivated: root.log += "next " }
}
"#;

#[test]
fn a_sequence_under_way_owns_every_press_until_it_ends() {
    let e = engine();
    let (root, mut s) = ready(&e, TWO_CHORDS);
    assert!(s.key_event(&e, &ctrl(key::A + 10)));
    // Ctrl going down again for the second chord is no wrong key.
    assert!(s.key_event(&e, &KeyEvent::press(key::CONTROL)));
    assert!(s.key_event(&e, &ctrl(key::A + 2)));
    assert_eq!(log(&e, root), "comment");

    // A wrong key ends the sequence and is used up by it.
    assert!(s.key_event(&e, &ctrl(key::A + 10)));
    assert!(s.key_event(&e, &KeyEvent::press(key::A + 23).with_text("x")));
    assert_eq!(log(&e, root), "comment");
    // The second chord alone is nothing.
    assert!(!s.key_event(&e, &ctrl(key::A + 2)));
    assert_eq!(log(&e, root), "comment pressed");
}

#[test]
fn losing_the_window_ends_a_sequence_under_way() {
    let e = engine();
    let (root, mut s) = ready(&e, TWO_CHORDS);
    assert!(s.key_event(&e, &ctrl(key::A + 10)));
    s.set_window_active(&e, false);
    s.set_window_active(&e, true);
    assert!(!s.key_event(&e, &ctrl(key::A + 2)));
    assert_eq!(log(&e, root), "pressed");
}

#[test]
fn a_held_key_acts_again_only_where_auto_repeat_is_on() {
    let e = engine();
    let (root, mut s) = ready(&e, TWO_CHORDS);
    let f5 = key::F1 + 4;
    assert!(s.key_event(&e, &KeyEvent::press(f5)));
    // The shortcut still owns the key while it is held.
    assert!(s.key_event(&e, &KeyEvent::press(f5).repeated()));
    assert!(s.key_event(&e, &KeyEvent::press(f5).repeated()));
    assert_eq!(log(&e, root), "reload");
    let f6 = key::F1 + 5;
    assert!(s.key_event(&e, &KeyEvent::press(f6)));
    assert!(s.key_event(&e, &KeyEvent::press(f6).repeated()));
    assert_eq!(log(&e, root), "reload next next");
}

fn window(e: &Engine, context: &str) -> (ObjectId, QuickScene) {
    let source = format!(
        r#"
import QtQuick
Item {{
    id: root
    objectName: "root"
    property string log: ""
    Shortcut {{ sequence: "F2"; context: {context}; onActivated: root.log += "f2 " }}
}}
"#
    );
    ready(e, &source)
}

#[test]
fn a_window_shortcut_hears_only_its_own_window() {
    let e = engine();
    let (first, mut s1) = window(&e, "Qt.WindowShortcut");
    let (second, mut s2) = window(&e, "Qt.WindowShortcut");
    assert!(s2.key_event(&e, &KeyEvent::press(key::F1 + 1)));
    assert_eq!((log(&e, first), log(&e, second)), ("".into(), "f2".into()));
    assert!(s1.key_event(&e, &KeyEvent::press(key::F1 + 1)));
    assert_eq!(
        (log(&e, first), log(&e, second)),
        ("f2".into(), "f2".into())
    );
}

#[test]
fn an_application_shortcut_hears_every_window() {
    let e = engine();
    let (first, _s1) = window(&e, "Qt.ApplicationShortcut");
    let (second, mut s2) = {
        let root = load_again(&e);
        let mut s = scene(&e, root, 200.0, 200.0);
        frame(&mut s, &e);
        (root, s)
    };
    assert!(s2.key_event(&e, &KeyEvent::press(key::F1 + 1)));
    assert_eq!(log(&e, first), "f2");
    assert_eq!(log(&e, second), "");
}

/// A second window with no shortcut of its own, from the component
/// already loaded.
fn load_again(e: &Engine) -> ObjectId {
    let root = e
        .create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"));
    // Its own shortcut would make the press ambiguous.
    let shortcut = e
        .children(root)
        .into_iter()
        .next()
        .expect("the Shortcut of the second window");
    e.destroy_now(shortcut);
    root
}

#[test]
fn a_destroyed_shortcut_takes_no_keys() {
    let e = engine();
    let (root, mut s) = window(&e, "Qt.WindowShortcut");
    let shortcut = e.children(root)[0];
    e.destroy_now(shortcut);
    assert!(!s.key_event(&e, &KeyEvent::press(key::F1 + 1)));
    assert_eq!(log(&e, root), "");
}

struct Field;

impl TextInputClient for Field {
    fn surrounding(&self, e: &Engine, item: ObjectId) -> Surrounding {
        let text = get(e, item, "objectName").to_js_string().to_string();
        let end = text.encode_utf16().count();
        Surrounding {
            text,
            anchor: end,
            cursor: end,
        }
    }
    fn replace(&self, _: &Engine, _: ObjectId, _: Range<usize>, _: &str) {}
    fn set_selection(&self, _: &Engine, _: ObjectId, _: usize, _: usize) {}
    fn cursor_rect(&self, _: &Engine, _: ObjectId) -> kurbo::Rect {
        kurbo::Rect::ZERO
    }
    fn hints(&self, _: &Engine, _: ObjectId) -> i32 {
        0
    }
}

#[test]
fn the_text_input_target_is_the_focus_item_when_it_edits_text() {
    let e = engine();
    register_text_input_client(
        &e,
        MetaRef::Static(Rectangle::static_meta()),
        Rc::new(Field),
    );
    let (root, mut s) = ready(
        &e,
        r#"
import QtQuick
Item {
    Rectangle { objectName: "field"; focus: true; activeFocusOnTab: true }
    Item { objectName: "plain"; activeFocusOnTab: true }
}
"#,
    );
    let (item, client) = s.text_input_target(&e).expect("a text input target");
    assert_eq!(item, find(&e, root, "field"));
    assert_eq!(client.surrounding(&e, item).cursor, 5);

    // Focus on something that edits no text: nothing for an input method.
    assert!(s.key_event(&e, &KeyEvent::press(key::TAB)));
    assert_eq!(s.active_focus_item(&e), Some(find(&e, root, "plain")));
    assert!(s.text_input_target(&e).is_none());

    s.key_event(&e, &KeyEvent::press(key::TAB));
    s.set_window_active(&e, false);
    assert!(s.text_input_target(&e).is_none());
}
