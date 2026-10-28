//! How long a turn of the UI thread takes: a turn past its budget warns
//! naming what ran.

mod support;

use std::time::Duration;

use ljos_engine::{JournalKind, TurnCause, Value};
use support::engine;

const SRC: &str = r#"
import QtQuick
Item {
  objectName: "root"
  property int base: 1
  property int told: 0
  onBaseChanged: told += 1
}
"#;

const LOAD: TurnCause = TurnCause::Message {
    mailbox: "application",
    kind: Some("Load"),
};

#[test]
fn a_turn_within_its_budget_says_nothing() {
    let h = engine();
    h.engine.set_turn_budget(Some(Duration::from_secs(60)));
    h.engine.turn(None, || ());
    assert!(h.warnings().is_empty());
}

#[test]
fn host_code_past_the_budget_warns_how_long_it_took() {
    let h = engine();
    h.engine.set_turn_budget(Some(Duration::from_millis(5)));
    h.engine
        .turn(None, || std::thread::sleep(Duration::from_millis(20)));
    let warnings = h.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert!(
        w.starts_with("a turn of the UI thread took ")
            && w.ends_with(
                " ms, longer than its budget of 5 ms: host code run with `Engine::turn`"
            ),
        "{w}"
    );
}

#[test]
fn a_message_past_the_budget_is_named_by_its_kind() {
    let h = engine();
    h.engine.set_turn_budget(Some(Duration::from_millis(5)));
    h.engine
        .turn_for(LOAD, None, || std::thread::sleep(Duration::from_millis(20)));
    let warnings = h.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].ends_with("message `Load` of mailbox `application`"),
        "{}",
        warnings[0]
    );
}

#[test]
fn a_handler_past_the_budget_is_named_with_its_object_and_place() {
    let h = engine();
    let root = h.load(SRC);
    // Any handler takes longer than a nanosecond: QML has no way to sleep.
    h.engine.set_turn_budget(Some(Duration::from_nanos(1)));
    h.set(root, "base", Value::Int(2));
    let warnings = h.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert!(w.contains("longer than its budget of 0 ms: "), "{w}");
    assert!(w.contains("handler onBaseChanged of Main \"root\""), "{w}");
    assert!(w.ends_with(" declared at file:///Main.qml:3:1"), "{w}");
}

#[test]
fn only_the_outermost_of_nested_turns_is_timed() {
    let h = engine();
    h.engine.set_turn_budget(Some(Duration::from_millis(5)));
    h.engine.turn_for(LOAD, None, || {
        h.engine
            .turn(None, || std::thread::sleep(Duration::from_millis(20)));
    });
    let warnings = h.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].ends_with("message `Load` of mailbox `application`"));
}

#[test]
fn warnings_of_turns_close_together_are_counted_into_the_next() {
    let h = engine();
    h.engine.set_turn_budget(Some(Duration::from_nanos(1)));
    for _ in 0..3 {
        h.engine.turn(None, || ());
    }
    // The first warns; the two after it within a second are not said
    // apart, and the next one said counts them.
    assert_eq!(h.warnings().len(), 1);
}

#[test]
fn the_journal_lists_host_work_with_its_time() {
    let h = engine();
    h.engine.set_journaled(true);
    h.engine
        .turn_for(LOAD, None, || std::thread::sleep(Duration::from_millis(2)));
    let j = h.engine.take_journal();
    let host: Vec<_> = j
        .entries
        .iter()
        .filter(|e| e.kind == JournalKind::Host)
        .collect();
    assert_eq!(host.len(), 1, "{:?}", j.entries);
    assert_eq!(host[0].type_name.as_ref() as &str, "application");
    assert_eq!(host[0].name.as_ref() as &str, "Load");
    assert!(host[0].took.unwrap() >= Duration::from_millis(2));
}
