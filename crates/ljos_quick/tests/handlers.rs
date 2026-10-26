//! Seams the shell feeds: platform pinches and drags from other
//! applications. The pointer behaviour of the handlers is in
//! `features/input/`.

mod common;

use common::*;
use ljos_platform::{DragActions, DropAction, DropActions, ExternalDrag, external_drag};
use ljos_quick::{
    DragData, PinchGesture, PinchPhase, PlatformDrag, PlatformDragKind, platform_drag,
};
use ljos_render::kurbo;

/// Whether an area took the drag.
fn took(engine: &ljos_engine::Engine, drag: &ExternalDrag) -> bool {
    external_drag(engine, drag).is_some()
}

fn pinch(phase: PinchPhase, scale: f64, rotation: f64) -> PinchGesture {
    PinchGesture {
        scale,
        rotation,
        ..PinchGesture::new(phase, kurbo::Point::new(50.0, 50.0))
    }
}

const PINCH: &str = r#"
import QtQuick
Item {
    width: 200; height: 200
    Rectangle {
        objectName: "box"
        width: 100; height: 100
        PinchHandler {
            objectName: "pinch"
            scaleAxis.maximum: 3
        }
    }
}
"#;

#[test]
fn a_platform_pinch_scales_and_turns_the_target() {
    let e = engine();
    let root = load(&e, PINCH);
    let box_ = find(&e, root, "box");
    let h = find(&e, root, "pinch");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);

    assert!(s.pinch_event(&e, &pinch(PinchPhase::Begin, 1.0, 0.0)));
    assert_eq!(get(&e, h, "active"), ljos_engine::Value::Bool(true));
    assert!(s.pinch_event(&e, &pinch(PinchPhase::Update, 2.0, 10.0)));
    assert_eq!(num(&e, box_, "scale"), 2.0);
    assert_eq!(num(&e, box_, "rotation"), 10.0);
    assert_eq!(num(&e, h, "activeScale"), 2.0);
    assert!(s.pinch_event(&e, &pinch(PinchPhase::End, 1.0, 0.0)));
    assert_eq!(get(&e, h, "active"), ljos_engine::Value::Bool(false));
    assert_eq!(num(&e, h, "activeScale"), 1.0);
    assert_eq!(num(&e, h, "persistentScale"), 2.0);

    // A second gesture goes on from where the first left off, up to the
    // axis's maximum.
    s.pinch_event(&e, &pinch(PinchPhase::Begin, 1.0, 0.0));
    s.pinch_event(&e, &pinch(PinchPhase::Update, 2.0, 0.0));
    assert_eq!(num(&e, h, "persistentScale"), 3.0);
    assert_eq!(num(&e, box_, "scale"), 3.0);
    s.pinch_event(&e, &pinch(PinchPhase::Cancel, 1.0, 0.0));
    assert_eq!(get(&e, h, "active"), ljos_engine::Value::Bool(false));
}

#[test]
fn a_pinch_away_from_every_handler_is_not_taken() {
    let e = engine();
    let root = load(&e, PINCH);
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let away = PinchGesture::new(PinchPhase::Begin, kurbo::Point::new(150.0, 150.0));
    assert!(!s.pinch_event(&e, &away));
    assert!(!s.pinch_event(&e, &pinch(PinchPhase::Update, 2.0, 0.0)));
}

const DROP: &str = r#"
import QtQuick
Item {
    width: 200; height: 200
    property string log: ""
    DropArea {
        objectName: "files"
        width: 100; height: 100
        keys: ["text/uri-list"]
        onEntered: (drag) => parent.log += "entered(" + drag.x + "," + drag.y + " "
            + (drag.source === null) + " " + drag.hasUrls + " " + drag.proposedAction + ") "
        onPositionChanged: (drag) => parent.log += "pos(" + drag.x + "," + drag.y + ") "
        onExited: parent.log += "exited "
        onDropped: (drop) => {
            parent.log += "dropped(" + drop.urls.length + " " + drop.urls[0] + " "
                + drop.formats.join(",") + ") "
            drop.acceptProposedAction()
        }
    }
    DropArea {
        objectName: "text"
        x: 100; width: 100; height: 100
        keys: ["text/plain"]
        onEntered: parent.log += "text.entered "
    }
}
"#;

fn log(e: &ljos_engine::Engine, root: ljos_engine::ObjectId) -> String {
    get(e, root, "log").to_js_string().as_str().to_string()
}

fn files() -> PlatformDrag {
    PlatformDrag::new(DragData::files([
        std::path::PathBuf::from("/data/a.txt"),
        std::path::PathBuf::from("/data/b c.png"),
    ]))
}

#[test]
fn files_dragged_from_another_application_enter_move_and_drop() {
    let e = engine();
    let root = load(&e, DROP);
    let area = find(&e, root, "files");
    let drag = files();
    let at = |x, y| kurbo::Point::new(x, y);

    platform_drag(&e, &drag, &PlatformDragKind::Move(at(10.0, 20.0)));
    assert_eq!(log(&e, root), "entered(10,20 true true 1) ");
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(true)
    );
    platform_drag(&e, &drag, &PlatformDragKind::Move(at(30.0, 20.0)));
    let action = platform_drag(&e, &drag, &PlatformDragKind::Drop(at(30.0, 20.0)));
    assert_eq!(
        log(&e, root),
        "entered(10,20 true true 1) pos(30,20) \
         dropped(2 file:///data/a.txt text/uri-list) "
    );
    assert_eq!(action, 1, "the area took the proposed copy");
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(false)
    );
}

#[test]
fn files_that_leave_the_window_leave_the_area() {
    let e = engine();
    let root = load(&e, DROP);
    let drag = files();
    platform_drag(
        &e,
        &drag,
        &PlatformDragKind::Move(kurbo::Point::new(10.0, 10.0)),
    );
    platform_drag(&e, &drag, &PlatformDragKind::Leave);
    assert_eq!(log(&e, root), "entered(10,10 true true 1) exited ");
}

#[test]
fn files_are_not_offered_to_an_area_that_wants_text() {
    let e = engine();
    let root = load(&e, DROP);
    let drag = files();
    let p = kurbo::Point::new(150.0, 10.0);
    platform_drag(&e, &drag, &PlatformDragKind::Move(p));
    assert_eq!(platform_drag(&e, &drag, &PlatformDragKind::Drop(p)), 0);
    assert_eq!(log(&e, root), "");
}

/// The platform seam (`ljos_platform::external_drag`): DropArea installs
/// itself as the drop handler, so a host's file drags reach it with the
/// same log the platform drags above give. Whether an area took the drag
/// is the answer to each step.
#[test]
fn external_file_drags_reach_the_drop_area() {
    let e = engine();
    let root = load(&e, DROP);
    let area = find(&e, root, "files");
    let paths = || {
        vec![
            std::path::PathBuf::from("/data/a.txt"),
            std::path::PathBuf::from("/data/b c.png"),
        ]
    };

    // Came in where the host knows, moved, and was let go where it was
    // last seen (the drop itself carries no position).
    assert!(took(
        &e,
        &ExternalDrag::files_entered(paths(), Some((10.0, 20.0)))
    ));
    assert_eq!(log(&e, root), "entered(10,20 true true 1) ");
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(true)
    );
    assert!(took(&e, &ExternalDrag::moved((30.0, 20.0))));
    assert!(took(&e, &ExternalDrag::files_dropped(paths(), None)));
    assert_eq!(
        log(&e, root),
        "entered(10,20 true true 1) pos(30,20) \
         dropped(2 file:///data/a.txt text/uri-list) "
    );
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(false)
    );

    // A drag that comes in without a position is heard when it moves,
    // and leaving the window leaves the area.
    assert!(!took(&e, &ExternalDrag::files_entered(paths(), None)));
    assert!(took(&e, &ExternalDrag::moved((10.0, 10.0))));
    assert!(!took(&e, &ExternalDrag::Left));
    assert_eq!(
        log(&e, root),
        "entered(10,20 true true 1) pos(30,20) \
         dropped(2 file:///data/a.txt text/uri-list) \
         entered(10,10 true true 1) exited "
    );

    // A drop the host reports only at the end, over an area that wants
    // text, is not taken.
    assert!(!took(
        &e,
        &ExternalDrag::files_dropped(paths(), Some((150.0, 10.0)))
    ));
}

const LINKS: &str = r#"
import QtQuick
Item {
    width: 200; height: 200
    property string log: ""
    DropArea {
        objectName: "local"
        width: 200; height: 200
        // Only files on this machine.
        onEntered: (drag) => {
            drag.accepted = drag.hasUrls
                && drag.urls.every(u => u.toString().startsWith("file:"))
            parent.log += "entered(" + drag.urls + " " + drag.accepted + ") "
        }
        onDropped: (drop) => {
            parent.log += "dropped(" + drop.urls + " [" + drop.text + "] "
                + drop.formats.join(",") + " "
                + drop.getDataAsString("application/x-ljos-test") + ") "
            drop.accept()
        }
    }
}
"#;

/// #115: a drag from another application carries what that application
/// offers, not only files. A link is offered to the area, which turns it
/// down, and the host is told no area took it.
#[test]
fn an_area_that_takes_only_files_turns_down_a_dragged_link() {
    let e = engine();
    let root = load(&e, LINKS);
    let area = find(&e, root, "local");
    let link = DragData::urls(["https://example.com/page"]);
    assert!(!took(
        &e,
        &ExternalDrag::Entered {
            data: link.clone(),
            at: Some((10.0, 10.0)),
            actions: DragActions::default(),
        }
    ));
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(false)
    );
    assert!(!took(
        &e,
        &ExternalDrag::Dropped {
            data: link,
            at: None,
        }
    ));
    assert_eq!(log(&e, root), "entered(https://example.com/page false) ");
}

#[test]
fn a_drag_from_another_application_carries_text_and_any_format() {
    let e = engine();
    let root = load(&e, LINKS);
    let data = DragData::from_formats([
        (
            "text/uri-list",
            &b"# a comment\r\nfile:///data/a.txt\r\n"[..],
        ),
        ("UTF8_STRING", b"a.txt"),
        ("text/plain", b"not this one"),
        ("application/x-ljos-test", b"payload\0"),
    ]);
    assert!(took(
        &e,
        &ExternalDrag::Entered {
            data: data.clone(),
            at: Some((10.0, 10.0)),
            actions: DragActions::default(),
        }
    ));
    assert!(took(&e, &ExternalDrag::Dropped { data, at: None }));
    assert_eq!(
        log(&e, root),
        "entered(file:///data/a.txt true) \
         dropped(file:///data/a.txt [a.txt] \
         text/uri-list,text/plain,application/x-ljos-test payload) "
    );
}

#[test]
fn text_a_source_writes_as_utf_16_is_read() {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend("<b>ljós</b>".encode_utf16().flat_map(u16::to_le_bytes));
    let d = DragData::from_formats([("text/html", bytes)]);
    assert_eq!(d.html.as_deref(), Some("<b>ljós</b>"));
    assert_eq!(d.formats(), vec!["text/html"]);
}

#[test]
fn a_file_url_escapes_what_a_url_cannot_hold() {
    let d = DragData::files([
        std::path::PathBuf::from("/data/b c.png"),
        std::path::PathBuf::from("/data/ljós#1.txt"),
    ]);
    assert_eq!(
        d.urls,
        vec![
            "file:///data/b c.png".to_string(),
            "file:///data/ljós%231.txt".to_string()
        ]
    );
    assert_eq!(d.formats(), vec!["text/uri-list".to_string()]);
}

const ACTIONS: &str = r#"
import QtQuick
Item {
    width: 200; height: 200
    property int choose: -1
    property string log: ""
    DropArea {
        width: 200; height: 200
        onEntered: (drag) => {
            parent.log += "entered(" + drag.supportedActions + " " + drag.proposedAction + ") "
            if (parent.choose >= 0) drag.accept(parent.choose)
        }
        onDropped: (drop) => drop.accept()
    }
}
"#;

/// #115: the host is told the action the area chose, so the other
/// application knows whether to delete what it moved; one the source does
/// not allow is told as the one it proposed.
#[test]
fn the_host_is_told_the_action_the_area_chose() {
    let e = engine();
    let root = load(&e, ACTIONS);
    let actions = DragActions {
        allowed: [DropAction::Copy, DropAction::Move].into_iter().collect(),
        proposed: DropAction::Move,
    };
    let enter = || ExternalDrag::Entered {
        data: DragData::text("hello"),
        at: Some((10.0, 10.0)),
        actions,
    };
    assert_eq!(external_drag(&e, &enter()), Some(DropAction::Move));
    assert_eq!(log(&e, root), "entered(3 2) ");
    assert_eq!(external_drag(&e, &ExternalDrag::Left), None);

    set(&e, root, "choose", ljos_engine::Value::Int(1));
    assert_eq!(external_drag(&e, &enter()), Some(DropAction::Copy));
    assert_eq!(external_drag(&e, &ExternalDrag::Left), None);

    set(&e, root, "choose", ljos_engine::Value::Int(4));
    assert_eq!(external_drag(&e, &enter()), Some(DropAction::Move));

    // The source now proposes a copy.
    let moved = ExternalDrag::Moved {
        at: (12.0, 10.0),
        actions: Some(DragActions {
            allowed: DropActions::from(DropAction::Copy),
            proposed: DropAction::Copy,
        }),
    };
    assert_eq!(external_drag(&e, &moved), Some(DropAction::Copy));
}
