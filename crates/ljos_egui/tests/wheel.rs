//! The wheel and the touchpad through `QmlView`: what egui reports becomes
//! a scroll event of the scene, the scene's Flickable moves, and an egui
//! scroll area around the view only gets what the scene did not use.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use egui::{Event, Modifiers, MouseWheelUnit, Pos2, Rect, TouchPhase, Vec2};
use egui_kittest::Harness;
use ljos_egui::{QmlView, ResizeMode};
use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{Flickable, Item, add_data};
use ljos_runtime::VirtualClock;

fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

fn num(e: &Engine, o: ObjectId, name: &str) -> f64 {
    match e.read_by_name(o, name) {
        Value::Real(v) => v,
        Value::Int(v) => f64::from(v),
        v => panic!("{name} is not a number: {v:?}"),
    }
}

/// A 200x100 Flickable with content of 200x500 at the top left of a root of
/// 300x150.
fn scene() -> (Rc<Engine>, ObjectId, ObjectId) {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);
    e.set(root, Item::WIDTH, 300.0).unwrap();
    e.set(root, Item::HEIGHT, 150.0).unwrap();
    let f = make::<Flickable>(&e, Some(root));
    e.set(f, Item::WIDTH, 200.0).unwrap();
    e.set(f, Item::HEIGHT, 100.0).unwrap();
    e.write_by_name(f, "contentWidth", Value::Real(200.0))
        .unwrap();
    e.write_by_name(f, "contentHeight", Value::Real(500.0))
        .unwrap();
    (e, root, f)
}

struct State {
    view: QmlView,
    clock: VirtualClock,
    rect: Rect,
    /// How far the egui scroll area around the view has scrolled.
    outer: f32,
}

fn state(e: &Rc<Engine>, root: ObjectId) -> State {
    // The engine's timers and the view's input have to tell the same time.
    let clock = VirtualClock::new();
    e.set_clock(Rc::new(clock.clone()));
    State {
        view: QmlView::new(Rc::clone(e), root)
            .unwrap()
            .with_clock(Arc::new(clock.clone()))
            .with_resize_mode(ResizeMode::ViewToRoot),
        clock,
        rect: Rect::NOTHING,
        outer: 0.0,
    }
}

/// The passes egui takes to hand a turn of the wheel on, then the time
/// after which a wheel that sent nothing more has stopped, then rest.
fn settle(h: &mut Harness<'_, State>) {
    h.run_steps(3);
    h.state().clock.advance(Duration::from_millis(500));
    h.run();
}

fn plain(e: &Rc<Engine>, root: ObjectId) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.rect = st.view.show(ui).rect;
            },
            state(e, root),
        );
    h.run();
    h
}

/// The view at the top of an egui scroll area whose content is 1000 high.
fn in_scroll_area(e: &Rc<Engine>, root: ObjectId) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_ui_state(
            |ui, st: &mut State| {
                let out = egui::ScrollArea::vertical().show(ui, |ui| {
                    st.rect = st.view.show(ui).rect;
                    ui.allocate_space(Vec2::new(100.0, 850.0));
                });
                st.outer = out.state.offset.y;
            },
            state(e, root),
        );
    h.run();
    h
}

fn at(h: &Harness<'_, State>, x: f32, y: f32) -> Pos2 {
    h.state().rect.min + Vec2::new(x, y)
}

fn wheel(unit: MouseWheelUnit, x: f32, y: f32, phase: TouchPhase) -> Event {
    Event::MouseWheel {
        unit,
        delta: Vec2::new(x, y),
        phase,
        modifiers: Modifiers::NONE,
    }
}

fn notch_down() -> Event {
    wheel(MouseWheelUnit::Line, 0.0, -1.0, TouchPhase::Move)
}

#[test]
fn a_notch_scrolls_the_flickable_under_the_pointer() {
    let (e, root, f) = scene();
    let mut h = plain(&e, root);
    let before = h.state().view.stats();

    h.hover_at(at(&h, 100.0, 50.0));
    h.event(notch_down());
    settle(&mut h);

    assert_eq!(num(&e, f, "contentY"), 72.0);
    let after = h.state().view.stats();
    assert_eq!(after.wheel_events - before.wheel_events, 1);
    assert_eq!(after.wheels_accepted - before.wheels_accepted, 1);
    // One turn of the wheel is one frame of the scene, however many passes
    // egui takes to settle.
    assert_eq!(after.changed - before.changed, 1);
}

#[test]
fn the_wheel_is_for_what_the_pointer_is_over() {
    let (e, root, f) = scene();
    let mut h = plain(&e, root);

    // Over the root, beside the Flickable.
    h.hover_at(at(&h, 250.0, 120.0));
    h.event(notch_down());
    settle(&mut h);
    assert_eq!(num(&e, f, "contentY"), 0.0);
    assert_eq!(h.state().view.stats().wheels_accepted, 0);

    // Outside the view.
    let below = Pos2::new(100.0, h.state().rect.max.y + 20.0);
    h.hover_at(below);
    h.event(notch_down());
    settle(&mut h);
    assert_eq!(num(&e, f, "contentY"), 0.0);
    assert_eq!(h.state().view.stats().wheel_events, 1);
}

#[test]
fn a_touchpad_scrolls_by_the_pixel_and_says_when_it_is_done() {
    let (e, root, f) = scene();
    let mut h = plain(&e, root);
    let pad = |y: f32, phase| wheel(MouseWheelUnit::Point, 0.0, y, phase);

    h.hover_at(at(&h, 100.0, 50.0));
    h.event(pad(0.0, TouchPhase::Start));
    h.event(pad(-30.0, TouchPhase::Move));
    h.event(pad(-12.0, TouchPhase::Move));
    h.run_steps(2);
    assert_eq!(num(&e, f, "contentY"), 42.0);
    assert_eq!(e.read_by_name(f, "moving"), Value::Bool(true));

    // No time passes: it is the device that says so, not a timer.
    h.event(pad(0.0, TouchPhase::End));
    h.run_steps(2);
    assert_eq!(e.read_by_name(f, "moving"), Value::Bool(false));
    assert_eq!(num(&e, f, "contentY"), 42.0);
    settle(&mut h);
}

#[test]
fn a_scroll_the_scene_used_does_not_scroll_egui_around_it() {
    let (e, root, f) = scene();
    let mut h = in_scroll_area(&e, root);

    h.hover_at(at(&h, 100.0, 50.0));
    h.event(notch_down());
    settle(&mut h);
    assert_eq!(num(&e, f, "contentY"), 72.0);
    assert_eq!(h.state().outer, 0.0);
}

#[test]
fn a_scroll_the_scene_has_no_use_for_scrolls_egui_around_it() {
    let (e, root, f) = scene();
    let mut h = in_scroll_area(&e, root);

    // The Flickable is at its end: the wheel is egui's.
    e.write_by_name(f, "contentY", Value::Real(400.0)).unwrap();
    h.run();
    h.hover_at(at(&h, 100.0, 50.0));
    h.event(notch_down());
    settle(&mut h);
    assert_eq!(num(&e, f, "contentY"), 400.0);
    assert!(h.state().outer > 0.0, "{}", h.state().outer);

    // And so is a wheel beside the Flickable.
    let (e, root, f) = scene();
    let mut h = in_scroll_area(&e, root);
    h.hover_at(at(&h, 250.0, 120.0));
    h.event(notch_down());
    settle(&mut h);
    assert_eq!(num(&e, f, "contentY"), 0.0);
    assert!(h.state().outer > 0.0, "{}", h.state().outer);
}
