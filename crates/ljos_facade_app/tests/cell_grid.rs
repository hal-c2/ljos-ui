//! The terminal-like item, used through the facade alone: shown in the
//! facade's window for tests and handed input as a host hands it on.

use std::sync::Arc;

use ljos::item::{DisplayList, DrawOp, Font, MouseButtons, Paint, PointerEvent, Rgba, kurbo};
use ljos::prelude::*;
use ljos::testing::{self, ScreenSpec, TestWindow};
use ljos_facade_app::{CellGrid, hover_colour, register, selection_colour};

fn grid(engine: &Engine, owner: Option<ObjectId>) -> ObjectId {
    let g = engine.create_by_name("Example", "CellGrid", owner).unwrap();
    let font = Font {
        pixel_size: 10,
        ..Font::default()
    };
    engine.set(g, CellGrid::FONT, font).unwrap();
    engine.set(g, Item::WIDTH, 200.0).unwrap();
    engine.set(g, Item::HEIGHT, 100.0).unwrap();
    g
}

fn rects(list: &DisplayList, colour: Rgba) -> Vec<kurbo::Rect> {
    list.ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                rect,
                fill: Paint::Solid(c),
                ..
            } if *c == colour => Some(*rect),
            _ => None,
        })
        .collect()
}

fn glyph_runs(list: &DisplayList) -> usize {
    list.ops()
        .iter()
        .filter(|op| matches!(op, DrawOp::Glyphs { .. }))
        .count()
}

fn painted(window: &TestWindow, item: ObjectId) -> Arc<DisplayList> {
    window.painted(item).expect("the grid paints")
}

#[test]
fn the_grid_counts_its_cells_before_it_is_painted() {
    let engine = ljos::engine();
    register(&engine);
    let g = grid(&engine, None);
    let columns = engine.get(g, CellGrid::COLUMNS).unwrap();
    let rows = engine.get(g, CellGrid::ROWS).unwrap();
    assert!(columns > 0 && rows > 0, "{columns} x {rows}");
    // The cells follow the system's text size, as a Text's letters do.
    testing::update_system_prefs(&engine, |p| p.font_scale = 2.0);
    let wider = engine.get(g, CellGrid::COLUMNS).unwrap();
    assert!(
        wider < columns,
        "{wider} columns at twice the size, {columns} before"
    );
}

#[test]
fn what_the_input_method_commits_is_painted() {
    let engine = ljos::engine();
    register(&engine);
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    let g = grid(&engine, None);
    window.show(g).unwrap();
    assert_eq!(glyph_runs(&painted(&window, g)), 0);
    assert!(window.pointer(&PointerEvent::press(1.0, 1.0, window.now())));
    assert!(window.pointer(&PointerEvent::release(1.0, 1.0, window.now())));
    assert_eq!(window.input_method_item(), Some(g));
    let at_start = window.input_method_cursor().unwrap();
    assert!(window.commit("ab"));
    assert_eq!(glyph_runs(&painted(&window, g)), 1);
    // Its window goes two cells on, where the next letter will.
    let next = window.input_method_cursor().unwrap();
    assert_eq!(next.y0, at_start.y0);
    assert!((next.x0 - 2.0 * at_start.width()).abs() < 1e-9, "{next:?}");
}

#[test]
fn a_drag_selects_and_a_pointer_with_no_button_hovers() {
    let engine = ljos::engine();
    register(&engine);
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    let g = grid(&engine, None);
    window.show(g).unwrap();
    let cell = window.input_method_cursor();
    // Without the focus there is no input method; find the cell size by it.
    assert_eq!(cell, None);
    let now = window.now();
    assert!(window.pointer(&PointerEvent::press(1.0, 1.0, now)));
    let cell = window.input_method_cursor().unwrap();
    let across = 2.5 * cell.width();
    window.pointer(&PointerEvent::moved(across, 1.0, MouseButtons::LEFT, now));
    window.pointer(&PointerEvent::release(across, 1.0, now));
    assert_eq!(rects(&painted(&window, g), selection_colour()).len(), 3);
    // A move with no button held is hover: the cell under it is marked.
    let below = 1.5 * cell.height();
    window.pointer(&PointerEvent::moved(1.0, below, MouseButtons::empty(), now));
    let marked = rects(&painted(&window, g), hover_colour());
    assert_eq!(marked.len(), 1);
    assert!(marked[0].y1 > cell.height() && marked[0].y1 <= 2.0 * cell.height() + 1e-9);
    window.pointer(&PointerEvent::moved(
        across,
        below,
        MouseButtons::empty(),
        now,
    ));
    let moved = rects(&painted(&window, g), hover_colour());
    assert!(moved[0].x0 > marked[0].x0, "{moved:?} after {marked:?}");
}

#[test]
fn the_cursor_is_one_device_pixel_on_the_screen_the_window_is_on() {
    let engine = ljos::engine();
    register(&engine);
    let fine = ScreenSpec {
        name: "fine".into(),
        scale: 2.0,
        ..ScreenSpec::default()
    };
    let coarse = ScreenSpec {
        name: "coarse".into(),
        position: [800.0, 0.0],
        ..ScreenSpec::default()
    };
    testing::set_screens(&engine, &[fine, coarse]);
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    let root = engine.create_by_name("QtQuick", "Window", None).unwrap();
    let content = engine
        .read_by_name(root, "contentItem")
        .as_object()
        .unwrap();
    let g = grid(&engine, Some(root));
    engine
        .write_by_name(g, "parent", Value::Object(content))
        .unwrap();
    window.show(root).unwrap();
    window.pointer(&PointerEvent::press(1.0, 1.0, window.now()));
    let cursor = |window: &TestWindow| {
        let ink = Rgba::BLACK;
        let r = rects(&painted(window, g), ink);
        r.last().copied().expect("a cursor")
    };
    assert_eq!(cursor(&window).width(), 0.5);
    testing::set_window_screen(&engine, root, 1);
    window.settle().unwrap();
    assert_eq!(cursor(&window).width(), 1.0);
    assert_eq!(ljos::item::device_pixel_ratio(&engine, g), 1.0);
}

#[test]
fn the_grid_follows_the_colour_scheme() {
    let engine = ljos::engine();
    register(&engine);
    let mut window = TestWindow::new(&engine, 200.0, 100.0);
    let g = grid(&engine, None);
    window.show(g).unwrap();
    assert_eq!(rects(&painted(&window, g), Rgba::BLACK).len(), 0);
    testing::update_system_prefs(&engine, |p| p.color_scheme = ColorScheme::Dark);
    window.settle().unwrap();
    assert_eq!(rects(&painted(&window, g), Rgba::BLACK).len(), 1);
    let hints = ljos::style_hints(&engine).unwrap();
    assert_eq!(
        engine.get(hints, StyleHints::COLOR_SCHEME),
        Ok(ColorScheme::Dark.qt_value())
    );
    assert!(!ljos::reduced_motion(&engine));
    testing::update_system_prefs(&engine, |p| p.reduced_motion = true);
    assert!(ljos::reduced_motion(&engine));
}

#[test]
fn standard_locations_are_named_through_the_prelude() {
    let env = PathEnv::sandbox("/home/user", "/nowhere");
    let data = Location::AppData.writable(&env, "Example", "Grid");
    assert!(data.starts_with("/home/user"), "{data:?}");
    // The QML singleton is the same type.
    let engine = ljos::engine();
    assert!(engine.singleton::<StandardPaths>().is_ok());
}
