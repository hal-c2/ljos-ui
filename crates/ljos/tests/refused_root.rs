//! A root the window cannot show leaves the root it showed: the scene
//! checks the new one before it lets go of the old one.

use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::TestWindow;
use ljos::window::{RootError, replace_root};

fn red(engine: &Engine) -> ObjectId {
    let root = engine.create_by_name("QtQuick", "Rectangle", None).unwrap();
    for (p, v) in [("width", 100.0), ("height", 100.0)] {
        engine.write_by_name(root, p, Value::Real(v)).unwrap();
    }
    engine
        .write_by_name(root, "color", Value::string("red"))
        .unwrap();
    root
}

#[test]
fn a_refused_root_leaves_the_one_shown() {
    let engine = ljos::engine();
    let mut w = TestWindow::new(&engine, 200.0, 200.0);
    let root = red(&engine);
    w.show(root).unwrap();
    assert!(w.painted(root).is_some());

    let not_an_item = engine.create_by_name("QtQml", "QtObject", None).unwrap();
    assert!(w.show(not_an_item).is_err());
    w.settle().unwrap();
    assert!(engine.is_alive(root));
    assert!(w.painted(root).is_some(), "the old root is still shown");
}

#[test]
fn a_root_destroyed_before_it_is_shown_leaves_the_one_shown() {
    let engine = ljos::engine();
    let mut w = TestWindow::new(&engine, 200.0, 200.0);
    let root = red(&engine);
    w.show(root).unwrap();

    let gone = red(&engine);
    engine.destroy(gone);
    assert!(w.show(gone).is_err());
    w.settle().unwrap();
    assert!(w.painted(root).is_some(), "the old root is still shown");
}

#[test]
fn replace_root_refuses_what_is_not_an_item_where_it_is_asked() {
    let engine = ljos::engine();
    let mut w = TestWindow::new(&engine, 200.0, 200.0);
    let root = red(&engine);
    w.show(root).unwrap();

    let not_an_item = engine.create_by_name("QtQml", "QtObject", None).unwrap();
    assert_eq!(
        replace_root(&engine, root, not_an_item),
        Err(RootError::NotAnItem(not_an_item))
    );
}
