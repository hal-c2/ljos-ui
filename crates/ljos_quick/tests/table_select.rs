//! A `CellSelector` selects the cells of a table view with the pointer.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{
    CellBlock, CellSelector, MouseButtons, PointerEvent, cell_block_rect, select_by,
    set_cell_selector,
};
use ljos_render::kurbo;
use ljos_runtime::Time;

/// Says how it selects, and writes down what the view tells it.
struct Recorder {
    mode: i32,
    log: RefCell<Vec<String>>,
}

impl CellSelector for Recorder {
    fn mode(&self, _: &Engine) -> i32 {
        self.mode
    }

    fn handles(&self, _: &Engine) -> [Option<ObjectId>; 2] {
        [None; 2]
    }

    fn selected(&self, _: &Engine, b: CellBlock, dragging: bool, handle: Option<usize>) {
        self.log.borrow_mut().push(format!(
            "{},{}-{},{} {dragging} {handle:?}",
            b.top, b.left, b.bottom, b.right
        ));
    }

    fn dropped(&self, _: &Engine) {
        self.log.borrow_mut().push("dropped".into());
    }
}

const VIEW: &str = r#"
import QtQuick
import QtQml.Models
Item {
    width: 200; height: 200
    TableView {
        objectName: "tv"; width: 200; height: 200
        selectionModel: ItemSelectionModel {}
        model: ListModel { ListElement { a: 1 } ListElement { a: 2 } ListElement { a: 3 } ListElement { a: 4 } }
        delegate: Rectangle { implicitWidth: 80; implicitHeight: 30 }
    }
}
"#;

fn ms(n: u64) -> Time {
    Time::from_millis(n)
}

#[test]
fn a_drag_selects_the_block_for_the_selector_and_the_view_does_not_flick() {
    let e = engine();
    let root = load(&e, VIEW);
    let tv = find(&e, root, "tv");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let rec = Rc::new(Recorder {
        mode: select_by::DRAG,
        log: RefCell::default(),
    });
    set_cell_selector(&e, tv, Some(rec.clone()));

    s.pointer_event(&e, &PointerEvent::press(10.0, 10.0, ms(0)));
    for (i, y) in [40.0, 70.0].into_iter().enumerate() {
        let t = ms(10 * (i as u64 + 1));
        s.pointer_event(&e, &PointerEvent::moved(10.0, y, MouseButtons::LEFT, t));
    }
    s.pointer_event(&e, &PointerEvent::release(10.0, 70.0, ms(40)));
    frame(&mut s, &e);

    assert_eq!(
        *rec.log.borrow(),
        ["0,0-1,0 true None", "0,0-2,0 true None", "dropped"]
    );
    assert_eq!(num(&e, tv, "contentY"), 0.0);
    assert_eq!(num(&e, tv, "currentRow"), 2.0);
    let block = CellBlock {
        top: 1,
        left: 0,
        bottom: 2,
        right: 0,
    };
    assert_eq!(
        cell_block_rect(&e, tv, block),
        Some(kurbo::Rect::new(0.0, 30.0, 80.0, 90.0))
    );
}

#[test]
fn without_a_selector_the_view_is_its_own_again() {
    let e = engine();
    let root = load(&e, VIEW);
    let tv = find(&e, root, "tv");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let rec = Rc::new(Recorder {
        mode: select_by::DRAG,
        log: RefCell::default(),
    });
    set_cell_selector(&e, tv, Some(rec.clone()));
    set_cell_selector(&e, tv, None);

    s.pointer_event(&e, &PointerEvent::press(10.0, 10.0, ms(0)));
    s.pointer_event(
        &e,
        &PointerEvent::moved(10.0, 40.0, MouseButtons::LEFT, ms(10)),
    );
    s.pointer_event(&e, &PointerEvent::release(10.0, 40.0, ms(20)));
    frame(&mut s, &e);

    assert!(rec.log.borrow().is_empty());
}
