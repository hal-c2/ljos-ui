//! Item properties, inheritance, positioners, Text sizing and Loader
//! registration. Positioner numbers were observed on Qt 6.11.2 (qml6).

mod common;

use common::*;
use ljos_engine::{Atom, Value};
use ljos_quick::enums::{layout_direction, loader_status, wrap_mode};
use ljos_quick::{Column, Item, Loader, Rectangle, RectanglePaint, Row, Text};

#[test]
fn row_right_to_left_with_padding_spacing_and_skipped_children() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let row = make::<Row>(&e, Some(root));
    set(&e, row, "spacing", Value::Real(5.0));
    set(&e, row, "padding", Value::Real(2.0));
    set(
        &e,
        row,
        "layoutDirection",
        Value::Int(layout_direction::RIGHT_TO_LEFT),
    );
    let sizes = [
        (10.0, 10.0),
        (20.0, 20.0),
        (30.0, 10.0),
        (0.0, 10.0),
        (5.0, 5.0),
    ];
    let kids: Vec<_> = sizes
        .iter()
        .map(|&(w, h)| {
            let k = make::<Item>(&e, Some(row));
            geom(&e, k, 100.0, 100.0, w, h);
            k
        })
        .collect();
    set(&e, kids[2], "visible", Value::Bool(false));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(
        (num(&e, row, "width"), num(&e, row, "height")),
        (49.0, 24.0)
    );
    assert_eq!((num(&e, kids[0], "x"), num(&e, kids[0], "y")), (37.0, 2.0));
    assert_eq!((num(&e, kids[1], "x"), num(&e, kids[1], "y")), (12.0, 2.0));
    // Invisible and zero-sized children are not positioned.
    assert_eq!(num(&e, kids[2], "x"), 100.0);
    assert_eq!(num(&e, kids[3], "x"), 100.0);
    assert_eq!((num(&e, kids[4], "x"), num(&e, kids[4], "y")), (2.0, 2.0));
}

#[test]
fn column_stacks_with_spacing_and_top_padding_and_relayouts() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let col = make::<Column>(&e, Some(root));
    set(&e, col, "spacing", Value::Real(3.0));
    set(&e, col, "topPadding", Value::Real(1.0));
    let a = make::<Item>(&e, Some(col));
    geom(&e, a, 0.0, 0.0, 10.0, 10.0);
    let b = make::<Item>(&e, Some(col));
    geom(&e, b, 0.0, 0.0, 20.0, 20.0);
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(num(&e, a, "y"), 1.0);
    assert_eq!(num(&e, b, "y"), 14.0);
    assert_eq!(
        (num(&e, col, "width"), num(&e, col, "height")),
        (20.0, 34.0)
    );
    // A child growing re-runs the layout on the next frame.
    e.set(a, Item::HEIGHT, 15.0).unwrap();
    assert!(s.needs_frame());
    frame(&mut s, &e);
    assert_eq!(num(&e, b, "y"), 19.0);
    assert_eq!(num(&e, col, "height"), 39.0);
}

#[test]
fn invisible_parent_hides_children_and_restores_them() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let p = make::<Item>(&e, Some(root));
    let c = make::<Item>(&e, Some(p));
    let hidden = make::<Item>(&e, Some(p));
    set(&e, hidden, "visible", Value::Bool(false));
    let _s = scene(&e, root, 100.0, 100.0);
    set(&e, p, "visible", Value::Bool(false));
    assert!(!get(&e, c, "visible").to_boolean());
    set(&e, p, "visible", Value::Bool(true));
    assert!(get(&e, c, "visible").to_boolean());
    // An item hidden on its own stays hidden.
    assert!(!get(&e, hidden, "visible").to_boolean());
}

#[test]
fn disabled_parent_disables_children() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let p = make::<Item>(&e, Some(root));
    let c = make::<Item>(&e, Some(p));
    let _s = scene(&e, root, 100.0, 100.0);
    set(&e, p, "enabled", Value::Bool(false));
    assert!(!get(&e, c, "enabled").to_boolean());
    set(&e, p, "enabled", Value::Bool(true));
    assert!(get(&e, c, "enabled").to_boolean());
}

#[test]
fn children_rect_and_contains() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let a = make::<Item>(&e, Some(root));
    geom(&e, a, 10.0, 5.0, 20.0, 20.0);
    let b = make::<Item>(&e, Some(root));
    geom(&e, b, -4.0, 30.0, 8.0, 2.0);
    let Value::Rect(r) = get(&e, root, "childrenRect") else {
        panic!("childrenRect is not a rect");
    };
    assert_eq!((r.x, r.y, r.width, r.height), (-4.0, 5.0, 34.0, 27.0));
    let pt = |x, y| Value::Point(std::sync::Arc::new(ljos_engine::Point { x, y }));
    let inside = e.invoke_by_name(a, "contains", &[pt(19.0, 0.0)]).unwrap();
    let outside = e.invoke_by_name(a, "contains", &[pt(20.0, 0.0)]).unwrap();
    assert!(inside.to_boolean());
    assert!(!outside.to_boolean());
}

#[test]
fn map_to_and_from_item() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let a = make::<Item>(&e, Some(root));
    geom(&e, a, 10.0, 20.0, 50.0, 50.0);
    let b = make::<Item>(&e, Some(a));
    geom(&e, b, 3.0, 4.0, 10.0, 10.0);
    let c = make::<Item>(&e, Some(root));
    geom(&e, c, 100.0, 0.0, 10.0, 10.0);
    let to = e
        .invoke_by_name(
            b,
            "mapToItem",
            &[Value::Object(c), Value::Real(1.0), Value::Real(1.0)],
        )
        .unwrap();
    let Value::Point(p) = to else {
        panic!("{to:?}")
    };
    assert_eq!((p.x, p.y), (-86.0, 25.0));
    let from = e
        .invoke_by_name(
            b,
            "mapFromItem",
            &[Value::Null, Value::Real(14.0), Value::Real(25.0)],
        )
        .unwrap();
    let Value::Point(p) = from else {
        panic!("{from:?}")
    };
    assert_eq!((p.x, p.y), (1.0, 1.0));
}

#[test]
fn text_implicit_size_from_the_measurer() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = make::<Text>(&e, Some(root));
    set(&e, t, "text", Value::String(Atom::from("hello")));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    // FixedAdvanceMeasurer: 0.5 px per character per pixel of font size,
    // line height = ascent + descent = font size (14 px by default).
    assert_eq!(num(&e, t, "implicitWidth"), 35.0);
    assert_eq!(num(&e, t, "implicitHeight"), 14.0);
    assert_eq!((num(&e, t, "width"), num(&e, t, "height")), (35.0, 14.0));
    assert_eq!(get(&e, t, "lineCount"), Value::Int(1));
}

#[test]
fn text_wraps_to_its_width() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let t = make::<Text>(&e, Some(root));
    set(&e, t, "text", Value::String(Atom::from("aaaa bbbb cccc")));
    set(&e, t, "wrapMode", Value::Int(wrap_mode::WORD_WRAP));
    set(&e, t, "width", Value::Real(70.0));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    assert_eq!(get(&e, t, "lineCount"), Value::Int(2));
    assert_eq!(num(&e, t, "height"), 28.0);
}

#[test]
fn rectangle_properties_and_border_group() {
    let e = engine();
    let r = make::<Rectangle>(&e, None);
    set(&e, r, "radius", Value::Real(4.0));
    set(&e, r, "border.width", Value::Real(2.0));
    assert_eq!(num(&e, r, "border.width"), 2.0);
    // Unassigned antialiasing follows the radius when painting.
    assert!(RectanglePaint::read(&e, r).antialiasing);
    set(&e, r, "antialiasing", Value::Bool(false));
    assert!(!RectanglePaint::read(&e, r).antialiasing);
    let Value::Color(c) = get(&e, r, "color") else {
        panic!("color")
    };
    assert_eq!(c, ljos_engine::Color::WHITE);
}

#[test]
fn loader_is_registered_with_its_properties() {
    let e = engine();
    assert!(e.lookup_type("QtQuick", "Loader").is_some());
    let l = e.create_by_name("QtQuick", "Loader", None).unwrap();
    assert!(get(&e, l, "active").to_boolean());
    assert_eq!(get(&e, l, "status"), Value::Int(loader_status::NULL));
    assert_eq!(get(&e, l, "item"), Value::Null);
    // Its own content is the loaded item; `item` is read-only.
    assert!(e.write_by_name(l, "item", Value::Null).is_err());
    let _ = Loader::ACTIVE;
}

#[test]
fn polish_stops_after_the_pass_limit_and_warns() {
    // A child whose width follows the Row's never settles: each layout
    // grows the Row, which grows the child.
    let e = engine();
    let root = make::<Item>(&e, None);
    let row = make::<Row>(&e, Some(root));
    let k = make::<Item>(&e, Some(row));
    e.set(k, Item::HEIGHT, 5.0).unwrap();
    e.bind(k, Item::WIDTH.index, move |e| {
        Value::Real(e.read(row, Item::WIDTH.index).to_number() + 1.0)
    })
    .unwrap();
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let log = e.take_log();
    assert!(
        log.iter()
            .any(|l| l.category == ljos_quick::POLISH_CATEGORY),
        "{log:?}"
    );
    // The scene does not spin: the next frame is idle again.
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
}

#[test]
fn rectangle_border_is_drawn_only_once_assigned() {
    // Qt 6.11.2: border.width reads 1, but no border is drawn until the
    // width or the colour is assigned.
    let e = engine();
    let r = make::<Rectangle>(&e, None);
    assert_eq!(num(&e, r, "border.width"), 1.0);
    assert_eq!(RectanglePaint::read(&e, r).border_width, 0.0);
    set(
        &e,
        r,
        "border.color",
        Value::Color(ljos_engine::Color::BLACK),
    );
    assert_eq!(RectanglePaint::read(&e, r).border_width, 1.0);
}
