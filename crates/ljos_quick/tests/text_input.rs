//! The editors from the host's side: what a frame paints, when the scene
//! wakes for the cursor, the input method seam and the clipboard. What an
//! editor does from QML is in `features/text-input/`.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::keys::Surrounding;
use ljos_quick::keys::key;
use ljos_quick::text_input::input_method_hint as hint;
use ljos_quick::{
    AccessibleTextAction, Clipboard, ImeState, KeyEvent, PlatformTextInput, QuickScene, to_rgba,
};
use ljos_render::{DrawOp, Paint, Rgba, kurbo};
use ljos_runtime::{Clock, Time, VirtualClock};

/// `Qt.Key_X`.
const KEY_X: i32 = key::A + 23;

fn ready(e: &Engine, source: &str) -> (ObjectId, QuickScene) {
    let root = load(e, source);
    let mut s = scene(e, root, 300.0, 200.0);
    frame(&mut s, e);
    (root, s)
}

#[test]
fn a_binding_on_the_cursor_may_ask_the_editor_where_a_point_is() {
    let e = engine();
    let (root, mut s) = ready(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 200
    property real at: 500
    TextInput {
        objectName: "input"
        text: "hello brave world"
        cursorPosition: positionAt(parent.at, 5)
    }
}
"#,
    );
    let input = find(&e, root, "input");
    assert_eq!(num(&e, input, "cursorPosition"), 17.0);
    set(&e, root, "at", Value::Real(3.0));
    frame(&mut s, &e);
    assert_eq!(num(&e, input, "cursorPosition"), 0.0);
    assert_eq!(e.take_warnings(), Vec::<String>::new());
}

#[test]
fn a_binding_installed_later_moves_the_cursor() {
    let e = engine();
    let (root, mut s) = ready(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 200
    function go() { input.cursorPosition = Qt.binding(function () { return input.near(3) }) }
    TextInput {
        id: input
        objectName: "input"
        text: "hello brave world"
        function near(x) { return positionAt(x, 5, TextInput.CursorBetweenCharacters) }
    }
}
"#,
    );
    let input = find(&e, root, "input");
    assert_eq!(num(&e, input, "cursorPosition"), 17.0);
    e.invoke_by_name(root, "go", &[]).unwrap();
    frame(&mut s, &e);
    assert_eq!(e.take_warnings(), Vec::<String>::new());
    assert_eq!(num(&e, input, "cursorPosition"), 0.0);
}

const EDITORS: &str = r##"
import QtQuick
Item {
    width: 300; height: 200
    TextInput {
        objectName: "input"; width: 200; height: 20
        text: "hello brave world"; selectionColor: "#ff0000"; font.pixelSize: 14
    }
    TextEdit { objectName: "edit"; y: 40; width: 200; height: 80; text: "one\ntwo" }
    TextInput {
        objectName: "secret"; y: 140; width: 200; height: 20
        echoMode: TextInput.Password; text: "hunter2"
    }
}
"##;

fn focus(e: &Engine, s: &mut QuickScene, item: ObjectId) {
    e.invoke_by_name(item, "forceActiveFocus", &[]).unwrap();
    frame(s, e);
}

fn string(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

/// The solid rectangles `item` paints: the selection, the line under a
/// composition, the cursor.
fn rects(s: &QuickScene, item: ObjectId) -> Vec<(kurbo::Rect, Rgba)> {
    let Some(list) = s
        .node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
    else {
        return Vec::new();
    };
    list.ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                rect,
                fill: Paint::Solid(c),
                ..
            } => Some((*rect, *c)),
            _ => None,
        })
        .collect()
}

#[test]
fn only_the_editor_with_the_keyboard_wakes_and_once_for_each_blink() {
    let clock = Arc::new(VirtualClock::new());
    let e = engine();
    e.set_clock(Rc::new(clock.clone()));
    let (root, mut s) = ready(&e, EDITORS);
    let input = find(&e, root, "input");
    // Three editors nobody is in: nothing to wake for.
    assert_eq!(e.next_timer_deadline(), None);
    assert!(!s.needs_frame());
    assert!(rects(&s, input).is_empty());

    focus(&e, &mut s, input);
    assert_eq!(rects(&s, input).len(), 1, "the cursor");
    let half = Duration::from_millis(500);
    let mut at = Time::ZERO;
    for shown in [false, true, false] {
        let due = e.next_timer_deadline().expect("the blink");
        // One wake for each half of a blink and none between.
        assert!(due >= at.saturating_add(half), "{due:?} after {at:?}");
        assert!(due < at.saturating_add(half + Duration::from_millis(20)));
        at = due;
        clock.set(at);
        e.advance_timers(clock.now());
        let out = s.frame(&e, at);
        assert!(out.changed);
        assert_eq!(rects(&s, input).len(), usize::from(shown));
        assert!(!s.needs_frame());
    }

    focus(&e, &mut s, root);
    assert_eq!(e.next_timer_deadline(), None);
    assert!(rects(&s, input).is_empty());
}

#[test]
fn a_selection_and_a_composition_are_painted_under_and_below_the_text() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    let input = find(&e, root, "input");
    focus(&e, &mut s, input);
    e.invoke_by_name(input, "select", &[Value::Int(0), Value::Int(5)])
        .unwrap();
    frame(&mut s, &e);
    let Value::Color(red) = get(&e, input, "selectionColor") else {
        panic!("a colour");
    };
    let width =
        ljos_quick::TextEngine::new().advance_width("hello", &ljos_quick::TextStyle::default());
    let selection: Vec<_> = rects(&s, input)
        .into_iter()
        .filter(|(_, c)| *c == to_rgba(red))
        .collect();
    assert_eq!(selection.len(), 1);
    let r = selection[0].0;
    assert_eq!((r.x0, r.y0, r.height()), (0.0, 0.0, 16.0));
    assert!(
        (r.width() - width).abs() < 0.01,
        "{r:?} is not {width} wide"
    );

    e.invoke_by_name(input, "deselect", &[]).unwrap();
    assert!(s.ime_preedit(&e, "ka", None));
    frame(&mut s, &e);
    let Value::Color(ink) = get(&e, input, "color") else {
        panic!("a colour");
    };
    // The line under what is being composed, one pixel below the baseline.
    let under: Vec<_> = rects(&s, input)
        .into_iter()
        .filter(|(r, c)| *c == to_rgba(ink) && r.height() == 1.0)
        .collect();
    assert_eq!(under.len(), 1, "{:?}", rects(&s, input));
    assert!(under[0].0.y0 > 12.0 && under[0].0.width() > 5.0);
}

/// A platform that writes down what it is told.
#[derive(Default)]
struct Told(Vec<String>);

impl PlatformTextInput for Told {
    fn enable(&mut self, hints: i32) {
        self.0.push(format!("enable {hints:#x}"));
    }
    fn disable(&mut self) {
        self.0.push("disable".to_owned());
    }
    fn set_cursor_area(&mut self, rect: kurbo::Rect) {
        self.0.push(format!("area {} {}", rect.x0.round(), rect.y0));
    }
    fn update(&mut self, around: &Surrounding) {
        self.0
            .push(format!("text {:?} {}", around.text, around.cursor));
    }
}

impl Told {
    fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.0)
    }
}

#[test]
fn the_platform_is_told_of_an_editor_only_what_changed() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    let (input, edit, secret) = (
        find(&e, root, "input"),
        find(&e, root, "edit"),
        find(&e, root, "secret"),
    );
    let mut ime = ImeState::new();
    let mut told = Told::default();
    ime.sync(&s, &e, &mut told);
    assert_eq!(told.take(), Vec::<String>::new());
    assert_eq!(ime.target(), None);

    focus(&e, &mut s, edit);
    ime.sync(&s, &e, &mut told);
    assert_eq!(ime.target(), Some(edit));
    let first = told.take();
    assert_eq!(first[0], format!("enable {:#x}", hint::MULTI_LINE));
    assert_eq!(first[2], "text \"one\\ntwo\" 0");
    assert_eq!(first.len(), 3, "{first:?}");
    // Nothing changed: nothing is said.
    ime.sync(&s, &e, &mut told);
    assert_eq!(told.take(), Vec::<String>::new());

    s.key_event(&e, &KeyEvent::press(KEY_X).with_text("x"));
    frame(&mut s, &e);
    ime.sync(&s, &e, &mut told);
    let typed = told.take();
    assert_eq!(typed.len(), 2, "{typed:?}");
    assert!(typed[0].starts_with("area "), "{typed:?}");
    assert_eq!(typed[1], "text \"xone\\ntwo\" 1");

    // A field that hides its text says so, whatever its author hinted.
    focus(&e, &mut s, secret);
    ime.sync(&s, &e, &mut told);
    let hidden = hint::HIDDEN_TEXT
        | hint::SENSITIVE_DATA
        | hint::NO_PREDICTIVE_TEXT
        | hint::NO_AUTO_UPPERCASE;
    assert_eq!(told.take()[0], format!("enable {hidden:#x}"));

    // An editor that cannot be typed into wants no input method.
    set(&e, secret, "readOnly", Value::Bool(true));
    frame(&mut s, &e);
    ime.sync(&s, &e, &mut told);
    assert_eq!(told.take(), ["disable"]);
    assert_eq!(ime.target(), None);

    focus(&e, &mut s, input);
    ime.sync(&s, &e, &mut told);
    assert_eq!(ime.target(), Some(input));
    told.take();
    focus(&e, &mut s, root);
    ime.sync(&s, &e, &mut told);
    assert_eq!(told.take(), ["disable"]);
}

#[test]
fn an_input_method_composes_in_the_editor_that_has_the_keyboard() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    let input = find(&e, root, "input");
    // Nobody to compose in.
    assert!(!s.ime_preedit(&e, "ka", None));
    assert!(!s.ime_commit(&e, "か"));

    focus(&e, &mut s, input);
    assert!(s.ime_preedit(&e, "ka", None));
    assert_eq!(string(&e, input, "preeditText"), "ka");
    assert_eq!(get(&e, input, "inputMethodComposing"), Value::Bool(true));
    // What is composed is shown and is not the text yet.
    assert_eq!(string(&e, input, "text"), "hello brave world");
    assert_eq!(string(&e, input, "displayText"), "hello brave worldka");

    assert!(s.ime_commit(&e, "か"));
    assert_eq!(string(&e, input, "text"), "hello brave worldか");
    assert_eq!(string(&e, input, "preeditText"), "");
    assert_eq!(get(&e, input, "inputMethodComposing"), Value::Bool(false));

    set(&e, input, "readOnly", Value::Bool(true));
    assert!(!s.ime_preedit(&e, "ka", None));
    assert!(!s.ime_commit(&e, "か"));
    assert_eq!(string(&e, input, "text"), "hello brave worldか");
}

#[test]
fn what_is_copied_waits_for_the_host_and_what_the_host_gives_is_pasted() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    let (input, edit, secret) = (
        find(&e, root, "input"),
        find(&e, root, "edit"),
        find(&e, root, "secret"),
    );
    let board = Clipboard::of(&e);
    assert_eq!(board.take_outgoing(), None);

    focus(&e, &mut s, input);
    e.invoke_by_name(input, "select", &[Value::Int(6), Value::Int(11)])
        .unwrap();
    e.invoke_by_name(input, "copy", &[]).unwrap();
    assert_eq!(board.take_outgoing().as_deref(), Some("brave"));
    // The host took it: it is not given twice.
    assert_eq!(board.take_outgoing(), None);

    // What a field hides does not leave it.
    e.invoke_by_name(secret, "selectAll", &[]).unwrap();
    e.invoke_by_name(secret, "copy", &[]).unwrap();
    e.invoke_by_name(secret, "cut", &[]).unwrap();
    assert_eq!(board.take_outgoing(), None);
    assert_eq!(string(&e, secret, "text"), "hunter2");

    // What the system holds comes from the host and is nothing to send
    // back to it.
    board.set_text(&e, "from elsewhere ");
    assert_eq!(board.take_outgoing(), None);
    assert_eq!(get(&e, edit, "canPaste"), Value::Bool(true));
    set(&e, edit, "cursorPosition", Value::Int(0));
    e.invoke_by_name(edit, "paste", &[]).unwrap();
    assert_eq!(string(&e, edit, "text"), "from elsewhere one\ntwo");
}

#[test]
fn an_assistive_technology_selects_and_types_in_an_editor() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    s.set_accessibility_active(&e, true);
    frame(&mut s, &e);
    let input = find(&e, root, "input");
    let key = s.accessible_node(input).expect("a node");
    let value = |s: &QuickScene| {
        let node = s.accessibility_tree().get(key).unwrap();
        node.text.clone().expect("a text value")
    };
    assert_eq!(value(&s).value, "hello brave world");
    assert!(!value(&s).read_only);

    let select = AccessibleTextAction::Select {
        anchor: 0,
        cursor: 5,
    };
    assert!(s.accessible_text_action(&e, key, select.clone()));
    assert_eq!(string(&e, input, "selectedText"), "hello");
    frame(&mut s, &e);
    assert_eq!((value(&s).anchor, value(&s).cursor), (0, 5));

    let typed = AccessibleTextAction::ReplaceSelection("howdy".to_owned());
    assert!(s.accessible_text_action(&e, key, typed.clone()));
    assert_eq!(string(&e, input, "text"), "howdy brave world");
    let all = AccessibleTextAction::SetValue("new".to_owned());
    assert!(s.accessible_text_action(&e, key, all.clone()));
    assert_eq!(string(&e, input, "text"), "new");

    // A read-only editor lets itself be selected in and nothing more.
    set(&e, input, "readOnly", Value::Bool(true));
    frame(&mut s, &e);
    assert!(value(&s).read_only);
    assert!(!s.accessible_text_action(&e, key, typed));
    assert!(!s.accessible_text_action(&e, key, all));
    assert!(s.accessible_text_action(&e, key, select.clone()));
    assert_eq!(string(&e, input, "text"), "new");

    set(&e, input, "enabled", Value::Bool(false));
    frame(&mut s, &e);
    assert!(!s.accessible_text_action(&e, key, select));
}

#[test]
fn an_assistive_technology_is_never_told_what_a_field_hides() {
    let e = engine();
    let (root, mut s) = ready(&e, EDITORS);
    s.set_accessibility_active(&e, true);
    frame(&mut s, &e);
    let secret = find(&e, root, "secret");
    let key = s.accessible_node(secret).expect("a node");
    let said = format!("{:?}", s.accessibility_tree().get(key).unwrap());
    assert!(!said.contains("hunter2"), "{said}");
    let value = s.accessibility_tree().get(key).unwrap().text.clone();
    // As many characters as it shows, and none of its own.
    assert_eq!(value.expect("a text value").value.chars().count(), 7);

    focus(&e, &mut s, secret);
    s.key_event(&e, &KeyEvent::press(KEY_X).with_text("x"));
    frame(&mut s, &e);
    let value = s.accessibility_tree().get(key).unwrap().text.clone();
    let value = value.expect("a text value").value;
    // What is typed is one more of the same sign.
    assert_eq!(value.chars().count(), 8);
    assert!(!value.contains(|c: char| c.is_alphanumeric()), "{value}");
}
