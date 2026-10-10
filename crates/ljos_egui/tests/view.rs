//! QmlView inside egui_kittest: painting, clicks, hover, timers and the
//! zero-idle guarantee (nothing runs and no repaint is asked for when the
//! scene has no work).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, ViewportId};
use egui_kittest::Harness;
use ljos_egui::{QmlView, ResizeMode};
use ljos_engine::{Color, Engine, ObjectId, QmlType, Value};
use ljos_quick::{Item, MouseArea, Rectangle, Text, add_data};
use ljos_runtime::{Time, VirtualClock};

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const BLUE: Color = Color::from_rgba8(0, 0, 255, 255);
const GREEN: Color = Color::from_rgba8(0, 255, 0, 255);

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

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

/// A red square at (10,10) and a blue 60x40 button at (100,10) whose
/// MouseArea turns it green on click.
struct Scene {
    engine: Rc<Engine>,
    root: ObjectId,
    button: ObjectId,
    area: ObjectId,
    text: ObjectId,
    clicks: Rc<Cell<u32>>,
}

fn scene() -> Scene {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);
    let red = make::<Rectangle>(&e, Some(root));
    geom(&e, red, 10.0, 10.0, 40.0, 40.0);
    set(&e, red, "color", Value::Color(RED));
    let button = make::<Rectangle>(&e, Some(root));
    geom(&e, button, 100.0, 10.0, 60.0, 40.0);
    set(&e, button, "color", Value::Color(BLUE));
    let area = make::<MouseArea>(&e, Some(button));
    set(&e, area, "anchors.fill", Value::Object(button));
    set(&e, area, "hoverEnabled", Value::Bool(true));
    let clicks = Rc::new(Cell::new(0));
    let c = Rc::clone(&clicks);
    e.connect_by_name(area, "clicked", move |e, _| {
        c.set(c.get() + 1);
        e.write_by_name(button, "color", Value::Color(GREEN))
            .unwrap();
    })
    .unwrap();
    let text = make::<Text>(&e, Some(root));
    e.set(text, Item::X, 10.0).unwrap();
    e.set(text, Item::Y, 70.0).unwrap();
    set(&e, text, "text", Value::string("Hello"));
    Scene {
        engine: e,
        root,
        button,
        area,
        text,
        clicks,
    }
}

struct State {
    view: QmlView,
    rect: Rect,
}

fn harness(s: &Scene, clock: Option<Arc<VirtualClock>>) -> Harness<'static, State> {
    let mut view = QmlView::new(Rc::clone(&s.engine), s.root).unwrap();
    if let Some(c) = clock {
        view = view.with_clock(c);
    }
    Harness::builder()
        .with_size(Vec2::new(200.0, 120.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.rect = st.view.show(ui).rect;
            },
            State {
                view,
                rect: Rect::NOTHING,
            },
        )
}

/// egui_kittest's default `step_dt`, which egui uses as the predicted frame
/// time.
const STEP_DT: Duration = Duration::from_millis(250);

fn repaint_delay(h: &Harness<'_, State>) -> Duration {
    h.output().viewport_output[&ViewportId::ROOT].repaint_delay
}

/// Screen position of a point in scene coordinates.
fn at(h: &Harness<'_, State>, x: f32, y: f32) -> Pos2 {
    h.state().rect.min + Vec2::new(x, y)
}

fn color(e: &Engine, o: ObjectId) -> Color {
    match e.read_by_name(o, "color") {
        Value::Color(c) => c,
        v => panic!("not a colour: {v:?}"),
    }
}

fn click(h: &mut Harness<'_, State>, p: Pos2) {
    h.hover_at(p);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

#[test]
fn root_follows_the_view_size() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let r = h.state().rect;
    assert!(r.width() > 100.0 && r.height() > 50.0, "{r:?}");
    assert_eq!(
        s.engine.get(s.root, Item::WIDTH).unwrap(),
        f64::from(r.width())
    );
    assert_eq!(
        s.engine.get(s.root, Item::HEIGHT).unwrap(),
        f64::from(r.height())
    );
}

#[test]
fn view_to_root_allocates_the_root_size() {
    let s = scene();
    s.engine.set(s.root, Item::WIDTH, 120.0).unwrap();
    s.engine.set(s.root, Item::HEIGHT, 30.0).unwrap();
    let view = QmlView::new(Rc::clone(&s.engine), s.root)
        .unwrap()
        .with_resize_mode(ResizeMode::ViewToRoot);
    let mut h = Harness::builder()
        .with_size(Vec2::new(300.0, 200.0))
        .build_ui_state(
            |ui, st: &mut State| st.rect = st.view.show(ui).rect,
            State {
                view,
                rect: Rect::NOTHING,
            },
        );
    h.run();
    assert_eq!(h.state().rect.size(), Vec2::new(120.0, 30.0));
}

#[test]
fn renders_the_rectangles() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let red = at(&h, 30.0, 30.0);
    let blue = at(&h, 130.0, 30.0);
    match h.render() {
        Ok(img) => {
            let px = |p: Pos2| img.get_pixel(p.x as u32, p.y as u32).0;
            assert_eq!(px(red), [255, 0, 0, 255]);
            assert_eq!(px(blue), [0, 0, 255, 255]);
            // Between the two rectangles is egui's panel, not ours.
            assert_ne!(px(at(&h, 75.0, 30.0)), [255, 0, 0, 255]);
        }
        Err(err) => {
            // No GPU adapter here: check the shapes egui was given instead.
            eprintln!("render unavailable ({err}); checking shapes");
            let has_colour_at = |c: egui::Color32, p: Pos2| {
                h.output().shapes.iter().any(|cs| match &cs.shape {
                    egui::Shape::Mesh(m) => {
                        m.vertices.iter().any(|v| v.color == c)
                            && cs.clip_rect.contains(p)
                            && m.calc_bounds().contains(p)
                    }
                    _ => false,
                })
            };
            assert!(has_colour_at(egui::Color32::RED, red));
            assert!(has_colour_at(egui::Color32::BLUE, blue));
        }
    }
}

#[test]
fn text_is_sized_by_the_text_engine_and_painted_from_the_atlas() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    // The size comes from the scene's text engine, not from egui's fonts.
    let (advance, line) = {
        let te = ljos_quick::text_engine();
        let mut te = te.borrow_mut();
        let style = ljos_quick::TextStyle::default();
        (
            te.advance_width("Hello", &style),
            te.font_metrics(&style).line_height,
        )
    };
    let w = s.engine.read_by_name(s.text, "implicitWidth").to_number();
    let ht = s.engine.read_by_name(s.text, "implicitHeight").to_number();
    assert_eq!(w, advance);
    assert_eq!(ht, line);
    // The glyphs are quads that sample a texture of their own, inside the
    // Text's box (a glyph's ink may reach a pixel past its advance).
    let text_rect = Rect::from_min_size(at(&h, 10.0, 70.0), Vec2::new(w as f32, ht as f32));
    let painted = h.output().shapes.iter().any(|cs| match &cs.shape {
        egui::Shape::Mesh(m) => {
            m.texture_id != egui::TextureId::default()
                && !m.vertices.is_empty()
                && text_rect.expand(2.0).contains_rect(m.calc_bounds())
        }
        _ => false,
    });
    assert!(painted, "no glyph mesh inside {text_rect:?}");
    assert!(
        !h.output()
            .shapes
            .iter()
            .any(|cs| matches!(cs.shape, egui::Shape::Text(_))),
        "text must not go through egui's own text layout"
    );
}

#[test]
fn a_click_reaches_the_mouse_area() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    assert_eq!(color(&s.engine, s.button), BLUE);
    let p = at(&h, 130.0, 30.0);
    click(&mut h, p);
    assert_eq!(s.clicks.get(), 1);
    assert_eq!(color(&s.engine, s.button), GREEN);
    // A click elsewhere does not.
    let p = at(&h, 30.0, 30.0);
    click(&mut h, p);
    assert_eq!(s.clicks.get(), 1);
    // And a click outside the view is egui's business.
    click(&mut h, Pos2::new(2.0, 2.0));
    assert_eq!(s.clicks.get(), 1);
}

#[test]
fn a_press_dragged_off_the_area_does_not_click() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let inside = at(&h, 130.0, 30.0);
    let outside = at(&h, 30.0, 90.0);
    h.hover_at(inside);
    h.drag_at(inside);
    h.run();
    assert_eq!(s.engine.read_by_name(s.area, "pressed"), Value::Bool(true));
    h.hover_at(outside);
    h.event(Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert_eq!(s.engine.read_by_name(s.area, "pressed"), Value::Bool(false));
    assert_eq!(s.clicks.get(), 0);
}

#[test]
fn hover_enters_and_exits() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let contains = |s: &Scene| s.engine.read_by_name(s.area, "containsMouse");
    h.hover_at(at(&h, 130.0, 30.0));
    h.run();
    assert_eq!(contains(&s), Value::Bool(true));
    assert_eq!(s.engine.read_by_name(s.area, "mouseX").to_number(), 30.0);
    h.hover_at(at(&h, 30.0, 30.0));
    h.run();
    assert_eq!(contains(&s), Value::Bool(false));
    h.hover_at(at(&h, 131.0, 31.0));
    h.run();
    assert_eq!(contains(&s), Value::Bool(true));
    h.remove_cursor();
    h.run();
    assert_eq!(contains(&s), Value::Bool(false));
}

#[test]
fn touch_is_a_pointer_with_its_own_id() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let p = at(&h, 130.0, 30.0);
    let touch = |phase| Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId(7),
        phase,
        pos: p,
        force: None,
    };
    // As egui-winit sends it: the touch, then the emulated mouse events.
    h.event(touch(egui::TouchPhase::Start));
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert_eq!(s.engine.read_by_name(s.area, "pressed"), Value::Bool(true));
    h.event(touch(egui::TouchPhase::End));
    h.event(Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.event(Event::PointerGone);
    h.run();
    assert_eq!(
        s.clicks.get(),
        1,
        "one click, not a touch plus a mouse click"
    );
    assert_eq!(
        s.engine.read_by_name(s.area, "containsMouse"),
        Value::Bool(false)
    );
}

#[test]
fn idle_runs_no_frames_and_requests_no_repaint() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    let settled = h.state().view.stats();
    assert!(settled.frames >= 1);
    for _ in 0..200 {
        h.step();
        assert_eq!(
            repaint_delay(&h),
            Duration::MAX,
            "an idle view asked for a repaint"
        );
    }
    let after = h.state().view.stats();
    assert_eq!(after.frames, settled.frames, "frame() ran while idle");
    assert_eq!(after.repaint_requests, settled.repaint_requests);

    // Moving the mouse over static content costs no frame either.
    h.hover_at(at(&h, 30.0, 30.0));
    h.run();
    assert_eq!(h.state().view.stats().frames, settled.frames);
}

#[test]
fn a_change_outside_the_pass_wakes_egui_once() {
    let s = scene();
    let mut h = harness(&s, None);
    h.run();
    h.step();
    assert!(!h.ctx.has_requested_repaint());
    let frames = h.state().view.stats().frames;
    // As if a message handler or a binding outside the pass ran.
    s.engine
        .write_by_name(s.button, "color", Value::Color(GREEN))
        .unwrap();
    assert!(
        h.ctx.has_requested_repaint(),
        "the scene waker did not reach egui"
    );
    h.run();
    assert_eq!(h.state().view.stats().frames, frames + 1);
    assert_eq!(repaint_delay(&h), Duration::MAX);
}

#[test]
fn a_timer_wakes_exactly_when_due() {
    let s = scene();
    let clock = Arc::new(VirtualClock::new());
    let mut h = harness(&s, Some(Arc::clone(&clock)));
    // Three ticks, 1 s apart, each toggling the button colour.
    let fired = Rc::new(Cell::new(0u32));
    let f = Rc::clone(&fired);
    let button = s.button;
    let mut due = Time::from_millis(1000);
    h.state_mut().view.set_timer_hook(Box::new(move |e, now| {
        if f.get() < 3 && now >= due {
            f.set(f.get() + 1);
            let c = if f.get() % 2 == 1 { GREEN } else { BLUE };
            e.write_by_name(button, "color", Value::Color(c)).unwrap();
            due += Duration::from_millis(1000);
        }
        (f.get() < 3).then_some(due)
    }));
    h.run();
    assert_eq!(fired.get(), 0);
    // egui reports the delay minus the predicted frame time, so the frame
    // is ready on time.
    assert_eq!(repaint_delay(&h), Duration::from_millis(1000) - STEP_DT);

    let frames_before = h.state().view.stats().frames;
    // Passes before the deadline do not run the pipeline but keep the
    // delayed repaint alive.
    clock.advance_ms(400);
    h.step();
    assert_eq!(repaint_delay(&h), Duration::from_millis(600) - STEP_DT);
    assert_eq!(h.state().view.stats().frames, frames_before);

    // Behave like an event loop: sleep until egui's deadline, then run a
    // pass. Every wake must be a due tick, and then the loop must stop.
    let mut wakes = 0u32;
    while repaint_delay(&h) != Duration::MAX && wakes < 10 {
        clock.advance(repaint_delay(&h) + STEP_DT);
        h.step();
        wakes += 1;
        assert_eq!(fired.get(), wakes, "a wake that fired no tick");
        let want = if wakes % 2 == 1 { GREEN } else { BLUE };
        assert_eq!(color(&s.engine, s.button), want);
    }
    assert_eq!(wakes, 3);
    assert_eq!(h.state().view.stats().frames, frames_before + 3);
    h.run_steps(20);
    assert_eq!(h.state().view.stats().frames, frames_before + 3);
}
