//! QmlView's accessibility nodes, read the way an assistive technology
//! reads them: through egui's AccessKit tree, queried with egui_kittest.

use std::cell::Cell;
use std::rc::Rc;

use egui::accesskit::Role;
use egui::{Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ljos_a11y::Role as QtRole;
use ljos_egui::QmlView;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Accessible, Item, MouseArea, Rectangle, TapHandler, Text, add_data};

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

/// Gives `o` an `Accessible` object, as `Accessible.role: ...` in QML does.
fn expose(e: &Engine, o: ObjectId, role: QtRole, name: &str) -> ObjectId {
    let a = e
        .attached_object(o, &MetaRef::Static(Accessible::static_meta()))
        .unwrap();
    set(e, a, "role", Value::Int(role.qt_value()));
    set(e, a, "name", Value::string(name));
    a
}

/// A toolbar group at (10,10) holding a "Save" button over a MouseArea and
/// an "Open" button over a TapHandler, and a "Ready" status text below.
struct Scene {
    engine: Rc<Engine>,
    root: ObjectId,
    bar: ObjectId,
    save: ObjectId,
    open: ObjectId,
    status: ObjectId,
    clicks: Rc<Cell<u32>>,
    taps: Rc<Cell<u32>>,
}

fn scene() -> Scene {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);

    let bar = make::<Item>(&e, Some(root));
    geom(&e, bar, 10.0, 10.0, 180.0, 40.0);
    expose(&e, bar, QtRole::ToolBar, "Tools");

    let save = make::<Rectangle>(&e, Some(bar));
    geom(&e, save, 5.0, 5.0, 60.0, 30.0);
    set(&e, save, "objectName", Value::string("save"));
    expose(&e, save, QtRole::Button, "Save");
    let area = make::<MouseArea>(&e, Some(save));
    set(&e, area, "anchors.fill", Value::Object(save));
    let clicks = Rc::new(Cell::new(0));
    let c = Rc::clone(&clicks);
    e.connect_by_name(area, "clicked", move |_, _| c.set(c.get() + 1))
        .unwrap();

    let open = make::<Rectangle>(&e, Some(bar));
    geom(&e, open, 75.0, 5.0, 60.0, 30.0);
    expose(&e, open, QtRole::Button, "Open");
    let tap = make::<TapHandler>(&e, Some(open));
    let taps = Rc::new(Cell::new(0));
    let t = Rc::clone(&taps);
    e.connect_by_name(tap, "tapped", move |_, _| t.set(t.get() + 1))
        .unwrap();

    let status = make::<Text>(&e, Some(root));
    e.set(status, Item::X, 10.0).unwrap();
    e.set(status, Item::Y, 70.0).unwrap();
    set(&e, status, "text", Value::string("Ready"));
    expose(&e, status, QtRole::StaticText, "");

    Scene {
        engine: e,
        root,
        bar,
        save,
        open,
        status,
        clicks,
        taps,
    }
}

struct State {
    view: QmlView,
    rect: Rect,
}

fn harness(s: &Scene) -> Harness<'static, State> {
    let view = QmlView::new(Rc::clone(&s.engine), s.root).unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(220.0, 120.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.rect = st.view.show(ui).rect;
            },
            State {
                view,
                rect: Rect::NOTHING,
            },
        );
    h.run();
    h
}

fn accessible(s: &Scene, o: ObjectId) -> ObjectId {
    s.engine
        .attached_object(o, &MetaRef::Static(Accessible::static_meta()))
        .unwrap()
}

#[test]
fn a_button_is_found_by_role_and_name() {
    let s = scene();
    let h = harness(&s);
    let save = h.get_by_role_and_label(Role::Button, "Save");
    // Scene coordinates (15,15)-(75,45), moved to where the view is.
    let min = h.state().rect.min + Vec2::new(15.0, 15.0);
    assert_eq!(save.rect(), Rect::from_min_size(min, Vec2::new(60.0, 30.0)));
    assert_eq!(save.accesskit_node().author_id(), Some("save"));
    assert_eq!(h.state().view.stats().accessible_nodes, 4);
}

#[test]
fn nodes_nest_as_the_items_do() {
    let s = scene();
    let h = harness(&s);
    let bar = h.get_by_role_and_label(Role::Toolbar, "Tools");
    let inside: Vec<String> = bar
        .children()
        .map(|n| n.accesskit_node().label().unwrap_or_default())
        .collect();
    assert_eq!(inside, ["Save", "Open"]);
    // The status text is a sibling of the toolbar, not inside it.
    let status = h.get_by_role(Role::Label);
    let above = status.parent().unwrap();
    assert_eq!(
        above.accesskit_node().id(),
        bar.parent().unwrap().accesskit_node().id()
    );
}

#[test]
fn a_text_with_a_role_is_a_label_that_reads_its_text() {
    let s = scene();
    let mut h = harness(&s);
    assert_eq!(h.get_by_role(Role::Label).value().as_deref(), Some("Ready"));
    set(&s.engine, s.status, "text", Value::string("Saved"));
    h.run();
    assert_eq!(h.get_by_role(Role::Label).value().as_deref(), Some("Saved"));
}

#[test]
fn the_press_action_clicks_the_mouse_area() {
    let s = scene();
    let mut h = harness(&s);
    h.get_by_role_and_label(Role::Button, "Save")
        .click_accesskit();
    h.run();
    assert_eq!(s.clicks.get(), 1);
    assert_eq!(s.taps.get(), 0);
    let stats = h.state().view.stats();
    assert_eq!(stats.accessible_presses, 1);
    // The action is not a pointer event of the window.
    assert_eq!(stats.pointer_events, 0);
}

#[test]
fn the_press_action_taps_the_tap_handler() {
    let s = scene();
    let mut h = harness(&s);
    h.get_by_role_and_label(Role::Button, "Open")
        .click_accesskit();
    h.run();
    assert_eq!(s.taps.get(), 1);
    assert_eq!(s.clicks.get(), 0);
}

#[test]
fn a_pointer_click_on_the_node_s_rect_reaches_the_item_too() {
    let s = scene();
    let mut h = harness(&s);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run();
    assert_eq!(s.clicks.get(), 1);
}

#[test]
fn a_disabled_button_is_not_pressed() {
    let s = scene();
    set(&s.engine, s.save, "enabled", Value::Bool(false));
    let mut h = harness(&s);
    let save = h.get_by_role_and_label(Role::Button, "Save");
    assert!(save.accesskit_node().is_disabled());
    save.click_accesskit();
    h.run();
    assert_eq!(s.clicks.get(), 0);
    assert_eq!(h.state().view.stats().accessible_presses, 0);
}

#[test]
fn a_new_name_is_the_same_node() {
    let s = scene();
    let mut h = harness(&s);
    let before = h
        .get_by_role_and_label(Role::Button, "Save")
        .accesskit_node()
        .id();
    set(
        &s.engine,
        accessible(&s, s.save),
        "name",
        Value::string("Store"),
    );
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "Save").is_none(),
        "the old name is gone"
    );
    let after = h
        .get_by_role_and_label(Role::Button, "Store")
        .accesskit_node()
        .id();
    assert_eq!(before, after);
}

#[test]
fn ids_come_from_the_view_and_the_item() {
    let s = scene();
    let mut h = harness(&s);
    let id = h.state().view.accessible_id(s.save).unwrap();
    let node = h.get_by_role_and_label(Role::Button, "Save");
    assert_eq!(node.accesskit_node().locate().0, id.accesskit_id());
    // Moving the item to another parent keeps its id.
    ljos_quick::set_parent_item(&s.engine, s.save, Some(s.root)).unwrap();
    h.run();
    assert_eq!(h.state().view.accessible_id(s.save), Some(id));
    let node = h.get_by_role_and_label(Role::Button, "Save");
    assert_eq!(node.accesskit_node().locate().0, id.accesskit_id());
    let bar = h.get_by_role_and_label(Role::Toolbar, "Tools");
    assert_eq!(bar.children().count(), 1);
    // An item that is not exposed has none; neither has another view's.
    assert_eq!(h.state().view.accessible_id(s.root), None);
    let other = QmlView::new(Rc::clone(&s.engine), s.root)
        .unwrap()
        .with_id(egui::Id::new("another view"));
    assert_eq!(other.accessible_id(s.save), None);
}

#[test]
fn hidden_and_ignored_items_leave_the_tree() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.open, "visible", Value::Bool(false));
    set(
        &s.engine,
        accessible(&s, s.bar),
        "ignored",
        Value::Bool(true),
    );
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "Open").is_none());
    assert!(h.query_by_role_and_label(Role::Toolbar, "Tools").is_none());
    // The ignored toolbar's button moves up; it is still there.
    h.get_by_role_and_label(Role::Button, "Save");
    assert_eq!(h.state().view.stats().accessible_nodes, 2);
}

#[test]
fn an_idle_view_with_a_listener_runs_no_frames() {
    let s = scene();
    let mut h = harness(&s);
    let before = h.state().view.stats();
    h.run_steps(5);
    let after = h.state().view.stats();
    assert_eq!(after.frames, before.frames);
    assert_eq!(after.repaint_requests, before.repaint_requests);
    // A rename costs one frame and then it is quiet again.
    set(
        &s.engine,
        accessible(&s, s.save),
        "name",
        Value::string("Store"),
    );
    h.run();
    h.run_steps(5);
    assert_eq!(h.state().view.stats().frames, before.frames + 1);
}

/// One pass of a bare egui context, without kittest (which always turns
/// AccessKit on).
fn pass(ctx: &egui::Context, view: &mut QmlView) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(220.0, 120.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        view.show(ui);
    });
    // No renderer here to take the font atlas; egui insists it is not lost
    // by accident.
    out.textures_delta.clear();
    out
}

#[test]
fn nothing_is_built_while_nothing_listens() {
    let s = scene();
    let mut view = QmlView::new(Rc::clone(&s.engine), s.root).unwrap();
    let ctx = egui::Context::default();

    for _ in 0..2 {
        let out = pass(&ctx, &mut view);
        assert!(out.platform_output.accesskit_update.is_none());
    }
    assert!(!view.scene().accessibility_active());
    assert!(view.scene().accessibility_tree().is_empty());
    assert_eq!(view.stats().accessible_nodes, 0);
    assert_eq!(view.accessible_id(s.save), None);
    let frames = view.stats().frames;

    // A listener arrives: the same pass builds and writes the tree.
    ctx.enable_accesskit();
    let out = pass(&ctx, &mut view);
    assert!(view.scene().accessibility_active());
    assert_eq!(view.stats().accessible_nodes, 4);
    assert_eq!(view.stats().frames, frames + 1);
    let update = out.platform_output.accesskit_update.unwrap();
    let id = view.accessible_id(s.save).unwrap().accesskit_id();
    let (_, node) = update.nodes.iter().find(|(n, _)| *n == id).unwrap();
    assert_eq!(node.role(), Role::Button);
    assert_eq!(node.label(), Some("Save"));

    // It leaves: the tree and its bookkeeping go away.
    ctx.disable_accesskit();
    let out = pass(&ctx, &mut view);
    assert!(out.platform_output.accesskit_update.is_none());
    assert!(!view.scene().accessibility_active());
    assert!(view.scene().accessibility_tree().is_empty());
    assert_eq!(view.stats().accessible_nodes, 0);
}
