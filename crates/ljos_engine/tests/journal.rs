//! What a turn did, in order (`Engine::take_journal`): the handlers and
//! bindings that ran, and the values that changed inside them.

mod support;

use ljos_engine::{JournalKind, Value};
use support::engine;

const SRC: &str = r#"
import QtQuick
Item {
  objectName: "root"
  property int base: 1
  property int doubled: base * 2
  property int told: 0
  onBaseChanged: told += 1
}
"#;

fn listed(j: &ljos_engine::Journal) -> Vec<String> {
    j.entries
        .iter()
        .map(|e| {
            let what = match (&e.old, &e.new) {
                (Some(Value::Int(a)), Some(Value::Int(b))) => format!(" {a}->{b}"),
                _ => String::new(),
            };
            format!(
                "{}{} {}{what}",
                "  ".repeat(e.depth as usize),
                e.kind.name(),
                e.name
            )
        })
        .collect()
}

#[test]
fn nothing_is_journaled_until_asked() {
    let h = engine();
    let root = h.load(SRC);
    h.set(root, "base", Value::Int(2));
    assert!(h.engine.take_journal().entries.is_empty());
}

#[test]
fn a_journal_lists_changes_inside_the_runs_that_made_them() {
    let h = engine();
    let root = h.load(SRC);
    assert_eq!(h.num(root, "doubled"), 2.0);
    h.engine.set_journaled(true);
    h.set(root, "base", Value::Int(2));
    assert_eq!(h.num(root, "doubled"), 4.0);
    let j = h.engine.take_journal();
    assert_eq!(
        listed(&j),
        [
            "changed base 1->2",
            "handler onBaseChanged",
            "  changed told 0->1",
            "binding doubled",
            "changed doubled 2->4",
        ]
    );
    let handler = &j.entries[1];
    assert_eq!(handler.kind, JournalKind::Handler);
    assert_eq!(handler.at.as_deref(), Some("file:///Main.qml:8:3"));
    assert!(handler.took.is_some());
    assert_eq!(j.dropped, 0);
    // A take starts afresh.
    assert!(h.engine.take_journal().entries.is_empty());
}

#[test]
fn turning_the_journal_off_forgets_what_it_kept() {
    let h = engine();
    let root = h.load(SRC);
    h.engine.set_journaled(true);
    h.set(root, "base", Value::Int(2));
    h.engine.set_journaled(false);
    assert!(h.engine.take_journal().entries.is_empty());
    h.engine.set_journaled(true);
    assert!(h.engine.take_journal().entries.is_empty());
}
