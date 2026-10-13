//! Files dragged onto a window from another application, through
//! `QmlView`: what egui reports (egui-winit's file events) reaches the
//! scene's DropAreas once, where the pointer is, and only in the window the
//! view shows.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use egui::{HoveredFile, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use ljos_egui::QmlView;
use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{DropArea, Item, add_data};

fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

fn geom(e: &Engine, o: ObjectId, x: f64, y: f64, w: f64, h: f64) {
    e.set(o, Item::X, x).unwrap();
    e.set(o, Item::Y, y).unwrap();
    e.set(o, Item::WIDTH, w).unwrap();
    e.set(o, Item::HEIGHT, h).unwrap();
}

/// What the DropAreas heard, in order.
type Log = Rc<RefCell<Vec<String>>>;

/// A DropArea at (100,20) 80x60 in a 200x120 root, and one at the same
/// place in a second tree no view shows (another window's).
fn scene(log: &Log) -> (Rc<Engine>, ObjectId) {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 120.0);
    let other = make::<Item>(&e, None);
    geom(&e, other, 0.0, 0.0, 200.0, 120.0);
    for (top, name) in [(root, "area"), (other, "elsewhere")] {
        let area = make::<DropArea>(&e, Some(top));
        geom(&e, area, 100.0, 20.0, 80.0, 60.0);
        for signal in ["entered", "positionChanged", "exited", "dropped"] {
            let log = Rc::clone(log);
            e.connect_by_name(area, signal, move |e, args| {
                let mut line = format!("{name} {signal}");
                if let Some(Value::Object(ev)) = args.first() {
                    let x = e.read_by_name(*ev, "x").to_number();
                    let y = e.read_by_name(*ev, "y").to_number();
                    line += &format!(" {x},{y}");
                }
                if signal == "dropped"
                    && let Some(Value::Object(ev)) = args.first()
                {
                    line += &format!(" {}", e.read_by_name(*ev, "urls").to_js_string().as_str());
                }
                log.borrow_mut().push(line);
            })
            .unwrap();
        }
    }
    (e, root)
}

struct State {
    view: QmlView,
    rect: Rect,
}

fn harness(e: &Rc<Engine>, root: ObjectId) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(240.0, 160.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.rect = st.view.show(ui).rect;
            },
            State {
                view: QmlView::new(Rc::clone(e), root).unwrap(),
                rect: Rect::NOTHING,
            },
        );
    h.run();
    h
}

fn at(h: &Harness<'_, State>, x: f32, y: f32) -> Pos2 {
    h.state().rect.min + Vec2::new(x, y)
}

/// A file as egui-winit hands a dropped one on.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

/// What egui-winit does for winit's `HoveredFile`, then `DroppedFile`, of
/// one file: the file hovers while the pointer moves, then is dropped.
fn drag_and_drop(h: &mut Harness<'_, State>, path: &str, from: Pos2, to: Pos2) {
    h.input_mut().hovered_files.push(HoveredFile {
        path: Some(PathBuf::from(path)),
        ..HoveredFile::default()
    });
    // egui repaints while files hover, so a pass is taken at a time.
    h.hover_at(from);
    h.run_steps(3);
    h.hover_at(to);
    h.run_steps(3);
    h.input_mut().hovered_files.clear();
    h.input_mut()
        .dropped_files
        .push(Arc::new(Dropped(PathBuf::from(path))));
    h.run();
}

#[test]
fn a_file_dropped_on_the_window_reaches_the_drop_area_once_where_it_was_let_go() {
    let log = Log::default();
    let (e, root) = scene(&log);
    let mut h = harness(&e, root);
    let (from, to) = (at(&h, 110.0, 30.0), at(&h, 150.0, 60.0));
    drag_and_drop(&mut h, "/tmp/a.txt", from, to);
    // More passes, as a host takes after a drop: they bring nothing new.
    h.run();
    h.run();
    assert_eq!(
        *log.borrow(),
        vec![
            "area entered 10,10",
            "area positionChanged 50,40",
            "area dropped 50,40 file:///tmp/a.txt",
        ]
    );
}

#[test]
fn a_file_that_hovers_without_moving_is_told_once() {
    let log = Log::default();
    let (e, root) = scene(&log);
    let mut h = harness(&e, root);
    h.input_mut().hovered_files.push(HoveredFile {
        path: Some(PathBuf::from("/tmp/a.txt")),
        ..HoveredFile::default()
    });
    h.hover_at(at(&h, 110.0, 30.0));
    h.run_steps(6);
    assert_eq!(*log.borrow(), vec!["area entered 10,10"]);
    // The drag is given up: winit's HoveredFileCancelled.
    h.input_mut().hovered_files.clear();
    h.run();
    assert_eq!(*log.borrow(), vec!["area entered 10,10", "area exited"]);
}

#[test]
fn a_file_dropped_outside_every_area_reaches_none() {
    let log = Log::default();
    let (e, root) = scene(&log);
    let mut h = harness(&e, root);
    let (from, to) = (at(&h, 10.0, 10.0), at(&h, 20.0, 100.0));
    drag_and_drop(&mut h, "/tmp/a.txt", from, to);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}
