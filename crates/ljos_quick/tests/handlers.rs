//! Seams the shell feeds: platform pinches and drags from other
//! applications. The pointer behaviour of the handlers is in
//! `features/input/`.

mod common;

use common::*;
use ljos_platform::{ExternalDrag, external_drag};
use ljos_quick::{
    DragData, PinchGesture, PinchPhase, PlatformDrag, PlatformDragKind, platform_drag,
};
use ljos_render::kurbo;

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
/// same log the platform drags above give.
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
    assert!(!external_drag(
        &e,
        &ExternalDrag::Entered {
            files: paths(),
            at: Some((10.0, 20.0)),
        }
    ));
    assert_eq!(log(&e, root), "entered(10,20 true true 1) ");
    assert_eq!(
        get(&e, area, "containsDrag"),
        ljos_engine::Value::Bool(true)
    );
    assert!(!external_drag(
        &e,
        &ExternalDrag::Moved { at: (30.0, 20.0) }
    ));
    assert!(external_drag(
        &e,
        &ExternalDrag::Dropped {
            files: paths(),
            at: None,
        }
    ));
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
    assert!(!external_drag(
        &e,
        &ExternalDrag::Entered {
            files: paths(),
            at: None,
        }
    ));
    assert!(!external_drag(
        &e,
        &ExternalDrag::Moved { at: (10.0, 10.0) }
    ));
    assert!(!external_drag(&e, &ExternalDrag::Left));
    assert_eq!(
        log(&e, root),
        "entered(10,20 true true 1) pos(30,20) \
         dropped(2 file:///data/a.txt text/uri-list) \
         entered(10,10 true true 1) exited "
    );

    // A drop the host reports only at the end, over an area that wants
    // text, is not taken.
    assert!(!external_drag(
        &e,
        &ExternalDrag::Dropped {
            files: paths(),
            at: Some((150.0, 10.0)),
        }
    ));
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
