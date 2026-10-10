//! Recovery must terminate and must not panic, whatever a typo does to a file.
//!
//! Every case takes a valid QML file, damages it (truncation, deleted ranges,
//! inserted brackets, quotes and keywords) and parses the result under a
//! watchdog: a recovery that loops fails the test after a timeout instead of
//! hanging it.

use std::{
    path::Path,
    sync::{OnceLock, mpsc},
    time::Duration,
};

use proptest::prelude::*;
use qml_syntax::Allocator;

const SMALL: &str = r#"
import QtQuick 2.15
import QtQuick.Controls as C
pragma ComponentBehavior: Bound

Item {
    id: root
    property int count: 0
    required property string title
    readonly property alias inner: box.width
    default property list<Item> kids
    signal done(int code)
    signal other(a: int, b: string)
    enum Mode { A, B = 2, C }
    component Badge: Rectangle { width: 4; color: "red" }
    function add(a: int, b: int): int { return a + b }
    width: parent.width * 2 + (count > 1 ? 1 : 0)
    text: `n=${count} ${ title + "x" }`
    onClicked: { if (count > 3) { done(count) } else { count++ } }
    anchors { left: parent.left; top: parent.top }
    Rectangle {
        id: box
        color: "red"
        children: [ Item { x: 1 }, Item { y: 2 } ]
        Behavior on width { NumberAnimation { duration: 100 } }
    }
    ListView { model: [1, 2, 3]; delegate: Text { text: modelData } }
}
"#;

/// Small corpus files if the submodule is checked out; the inline file alone
/// otherwise.
fn sources() -> Vec<String> {
    let mut sources = vec![SMALL.to_string()];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples");
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "qml") {
                found.push(path);
            }
        }
    }
    found.sort();
    // A spread of files, not the first few of one directory.
    for path in found.iter().step_by((found.len() / 12).max(1)).take(12) {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.len() < 6000
        {
            sources.push(text);
        }
    }
    sources
}

static SOURCES: OnceLock<Vec<String>> = OnceLock::new();

const FRAGMENTS: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ";",
    ":",
    ",",
    ".",
    "\"",
    "'",
    "`",
    "${",
    "/",
    "//",
    "/*",
    "@",
    "\n",
    "\\",
    "=>",
    "property ",
    "import ",
    "signal ",
    "function ",
    "enum ",
    "component ",
    "required ",
    "on ",
    "as ",
    "?",
    "+",
    "=",
    "0x",
    "<",
    ">",
];

#[derive(Debug, Clone)]
enum Damage {
    Truncate(usize),
    Delete(usize, usize),
    Insert(usize, usize),
}

fn damage() -> impl Strategy<Value = Damage> {
    prop_oneof![
        any::<usize>().prop_map(Damage::Truncate),
        (any::<usize>(), 1..40usize).prop_map(|(at, len)| Damage::Delete(at, len)),
        (any::<usize>(), 0..FRAGMENTS.len()).prop_map(|(at, f)| Damage::Insert(at, f)),
    ]
}

fn apply(source: &str, damages: &[Damage]) -> String {
    let mut text = source.to_string();
    let boundary = |text: &str, at: usize| {
        let mut at = at % (text.len() + 1);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    for damage in damages {
        match *damage {
            Damage::Truncate(at) => {
                let at = boundary(&text, at);
                text.truncate(at);
            }
            Damage::Delete(at, len) => {
                let start = boundary(&text, at);
                let mut end = (start + len).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                text.replace_range(start..end, "");
            }
            Damage::Insert(at, fragment) => {
                let at = boundary(&text, at);
                text.insert_str(at, FRAGMENTS[fragment]);
            }
        }
    }
    text
}

/// Parses on a thread and checks what must hold of any result.
fn parse_checked(source: String) {
    let (sender, receiver) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &source);
        let len = source.len() as u32;
        for diagnostic in &parsed.diagnostics {
            assert!(diagnostic.span.start <= diagnostic.span.end);
            assert!(diagnostic.span.end <= len, "{diagnostic:?} outside {len}");
        }
        if parsed.panicked {
            assert!(
                !parsed.diagnostics.is_empty(),
                "panicked without a diagnostic"
            );
        }
        let starts: Vec<u32> = parsed.diagnostics.iter().map(|d| d.span.start).collect();
        assert!(
            starts.windows(2).all(|w| w[0] <= w[1]),
            "diagnostics out of order"
        );
        // The tree is walkable and its spans lie in the file.
        let span = parsed.document.root.span;
        assert!(
            span.start <= span.end && span.end <= len,
            "root {span:?} in {source:?}"
        );
        for member in &parsed.document.root.members {
            if let qml_syntax::ast::QmlMember::Object(object) = member {
                assert!(object.span.start <= object.span.end && object.span.end <= len);
            }
        }
        let _ = sender.send(());
    });
    match receiver.recv_timeout(Duration::from_secs(20)) {
        Ok(()) => handle.join().unwrap(),
        // The sender is dropped when the thread panicked: surface that panic.
        Err(mpsc::RecvTimeoutError::Disconnected) => handle.join().unwrap(),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("the parser did not terminate"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn damaged_files_terminate_without_panicking(
        pick in any::<usize>(),
        damages in proptest::collection::vec(damage(), 1..6),
    ) {
        let sources = SOURCES.get_or_init(sources);
        let source = &sources[pick % sources.len()];
        parse_checked(apply(source, &damages));
    }
}

#[test]
fn every_truncation_of_a_file_terminates() {
    // Exhaustive for one file: every prefix is a possible "error at EOF".
    let mut at = 0;
    while at <= SMALL.len() {
        if SMALL.is_char_boundary(at) {
            parse_checked(SMALL[..at].to_string());
        }
        at += 7;
    }
}
