//! A ListView whose rows settle their height after they are made (made at
//! 40, then the height of their wrapped text), and one whose rows are of
//! two heights and reused: where `positionViewAtEnd`, `positionViewAtIndex`,
//! a wheel and a `contentY` write leave it, with and without an attached
//! ScrollBar.

use ljos::item::{ScrollPhase, WheelEvent};
use ljos::prelude::{Engine, ObjectId};
use ljos::testing::TestWindow;

const SETTLING: &str = r##"
import QtQuick
import QtQuick.Controls.Basic

Item {
    width: 400
    height: 300
    property alias view: view
    ListView {
        id: view
        anchors.fill: parent
        model: 200
        delegate: Item {
            id: row
            required property int index
            width: ListView.view.width
            // Made at 40, then the height of its text.
            property bool made: false
            height: made ? text.implicitHeight : 40
            Component.onCompleted: made = true
            Text {
                id: text
                width: parent.width
                wrapMode: Text.Wrap
                // One to twenty lines.
                text: row.index + " " + "word ".repeat(1 + (row.index * 37) % 200)
            }
        }
        BAR
    }
}
"##;

const REUSED: &str = r##"
import QtQuick

Item {
    width: 900
    height: 375
    property alias view: view
    ListView {
        id: view
        anchors.fill: parent
        clip: true
        model: 116
        reuseItems: true
        delegate: Rectangle {
            required property int index
            width: ListView.view.width
            height: index % 65 == 0 ? 36 : 18
            color: index % 2 ? "red" : "blue"
        }
    }
}
"##;

struct Shown {
    engine: Engine,
    window: TestWindow,
    view: ObjectId,
    _dir: tempfile::TempDir,
}

fn show(qml: &str, width: f64, height: f64) -> Shown {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), qml).unwrap();
    let engine = ljos::engine();
    let compiled = ljos::dev::compile_quiet(&[dir.path()], Some(&engine)).unwrap();
    assert!(!compiled.has_errors(), "{:?}", compiled.diagnostics);
    let mut window = TestWindow::new(&engine, width, height);
    let root = ljos::create_root(&engine, compiled.source, "Main").unwrap();
    window.show(root).unwrap();
    window.settle().unwrap();
    let view = engine.read_by_name(root, "view").as_object().unwrap();
    Shown {
        engine,
        window,
        view,
        _dir: dir,
    }
}

fn settling(bar: bool) -> Shown {
    let bar = if bar {
        "ScrollBar.vertical: ScrollBar {}"
    } else {
        ""
    };
    show(&SETTLING.replace("BAR", bar), 400.0, 300.0)
}

impl Shown {
    fn read(&self, name: &str) -> f64 {
        self.engine.read_by_name(self.view, name).to_number()
    }

    fn call(&mut self, name: &str, args: &[ljos::prelude::Value]) {
        self.engine.invoke_by_name(self.view, name, args).unwrap();
        self.window.settle().unwrap();
    }

    /// contentY after each of `n` touchpad steps of `dy`, from where it is.
    fn wheel(&mut self, n: usize, dy: f64) -> Vec<f64> {
        let mut out = vec![self.read("contentY")];
        let ev = |dy: f64, phase, w: &TestWindow| {
            WheelEvent::pixels(200.0, 150.0, 0.0, dy, w.now()).with_phase(phase)
        };
        self.window
            .wheel(&ev(0.0, ScrollPhase::Begin, &self.window));
        for _ in 0..n {
            self.window
                .wheel(&ev(dy, ScrollPhase::Update, &self.window));
            self.window.advance(8).unwrap();
            out.push(self.read("contentY"));
        }
        self.window.wheel(&ev(0.0, ScrollPhase::End, &self.window));
        out
    }
}

/// Steps that moved contentY toward the top: (step, before, after).
fn backward(ys: &[f64]) -> Vec<(usize, f64, f64)> {
    ys.windows(2)
        .enumerate()
        .filter(|(_, w)| w[1] < w[0] - 1e-6)
        .map(|(i, w)| (i, w[0], w[1]))
        .collect()
}

fn ends_at_the_end(bar: bool) {
    let mut s = settling(bar);
    let mut seen = Vec::new();
    for _ in 0..3 {
        s.call("positionViewAtEnd", &[]);
        seen.push((
            s.read("contentY"),
            s.read("originY"),
            s.read("contentHeight"),
        ));
    }
    // (contentY, originY, contentHeight) after each call: the end is at
    // originY + contentHeight - height.
    for (y, origin, length) in &seen {
        assert!(
            (y - (origin + length - 300.0)).abs() < 0.5,
            "not at the end: {seen:?}"
        );
    }
    let lengths: Vec<f64> = seen.iter().map(|s| s.2).collect();
    assert!(
        lengths.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5),
        "the length moves: {seen:?}"
    );
}

#[test]
fn position_view_at_end_shows_the_last_row() {
    ends_at_the_end(false);
}

#[test]
fn position_view_at_end_with_a_scroll_bar_shows_the_last_row() {
    ends_at_the_end(true);
}

fn scrolls_toward_the_end(bar: bool) {
    let mut s = settling(bar);
    let ys = s.wheel(1500, -6.0);
    assert!(
        ys[ys.len() - 1] - ys[0] > 1000.0,
        "it scrolled: {:?}",
        &ys[ys.len() - 5..]
    );
    assert_eq!(backward(&ys), vec![]);
}

#[test]
fn a_plain_view_scrolls_only_toward_the_end() {
    scrolls_toward_the_end(false);
}

#[test]
fn a_view_with_a_scroll_bar_scrolls_only_toward_the_end() {
    scrolls_toward_the_end(true);
}

fn keeps_a_content_y_write(bar: bool) {
    let mut s = settling(bar);
    for to in [2884.0, 6000.0, 1200.0] {
        s.engine
            .write_by_name(s.view, "contentY", ljos::prelude::Value::Real(to))
            .unwrap();
        s.window.settle().unwrap();
        assert_eq!(s.read("contentY"), to, "bar {bar}");
    }
}

#[test]
fn a_content_y_write_is_kept() {
    keeps_a_content_y_write(false);
}

#[test]
fn a_content_y_write_is_kept_with_a_scroll_bar() {
    keeps_a_content_y_write(true);
}

#[test]
fn position_view_at_index_reaches_a_far_row_of_reused_delegates() {
    let mut s = show(REUSED, 900.0, 375.0);
    s.window.advance(16).unwrap();
    // ListView.Beginning is 0.
    s.call(
        "positionViewAtIndex",
        &[ljos::prelude::Value::Int(65), ljos::prelude::Value::Int(0)],
    );
    s.window.advance(16).unwrap();
    // Row 0 is 36 and rows 1..64 are 18: row 65 starts at 1188.
    assert_eq!(s.read("contentY"), 1188.0);
}
