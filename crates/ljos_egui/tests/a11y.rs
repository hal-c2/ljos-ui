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
use ljos_quick::{Accessible, Item, MouseArea, Rectangle, TapHandler, Text};

mod common;
use common::{geom, make, set};

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
        .with_id(egui::Id::unique("another view"));
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

/// A list at (10,10), 150 by 100, that scrolls 400 of content.
fn scrolling() -> (Scene, ObjectId) {
    let s = scene();
    let e = &s.engine;
    geom(e, s.bar, 10.0, 130.0, 180.0, 40.0);
    let list = make::<ljos_quick::Flickable>(e, Some(s.root));
    geom(e, list, 10.0, 10.0, 150.0, 100.0);
    set(e, list, "contentWidth", Value::Real(150.0));
    set(e, list, "contentHeight", Value::Real(400.0));
    expose(e, list, QtRole::List, "Rows");
    (s, list)
}

#[test]
fn a_scroll_asked_of_a_node_scrolls_the_item() {
    let (s, list) = scrolling();
    let mut h = harness(&s);
    let rows = h.get_by_role_and_label(Role::List, "Rows");
    let node = rows.accesskit_node();
    assert_eq!(node.scroll_y(), Some(0.0));
    assert_eq!(node.scroll_y_max(), Some(300.0));
    rows.scroll_down();
    h.run();
    let y = s.engine.read_by_name(list, "contentY").to_number();
    assert_eq!(y, 72.0);
    assert_eq!(h.state().view.stats().accessible_actions, 1);
    // What the screen reader is told has moved with it.
    let rows = h.get_by_role_and_label(Role::List, "Rows");
    assert_eq!(rows.accesskit_node().scroll_y(), Some(72.0));
}

#[test]
fn a_scroll_with_nowhere_to_go_is_not_counted() {
    let (s, list) = scrolling();
    let mut h = harness(&s);
    h.get_by_role_and_label(Role::List, "Rows").scroll_up();
    h.run();
    assert_eq!(s.engine.read_by_name(list, "contentY").to_number(), 0.0);
    assert_eq!(h.state().view.stats().accessible_actions, 0);
}

#[test]
fn the_state_of_an_item_is_told() {
    let s = scene();
    let a = accessible(&s, s.save);
    set(&s.engine, a, "checkable", Value::Bool(true));
    set(&s.engine, a, "checked", Value::Bool(true));
    set(&s.engine, a, "expandable", Value::Bool(true));
    set(&s.engine, a, "selected", Value::Bool(true));
    set(&s.engine, a, "readOnly", Value::Bool(true));
    let h = harness(&s);
    let save = h.get_by_role_and_label(Role::Button, "Save");
    let node = save.accesskit_node();
    assert_eq!(node.toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(node.data().is_expanded(), Some(false));
    assert!(node.supports_expand_collapse());
    assert_eq!(node.is_selected(), Some(true));
    assert!(node.is_read_only());
    // An item that said nothing of the kind says nothing.
    let open = h.get_by_role_and_label(Role::Button, "Open");
    assert_eq!(open.accesskit_node().toggled(), None);
    assert_eq!(open.accesskit_node().data().is_expanded(), None);
}

#[test]
fn a_label_names_the_item_it_is_for() {
    let s = scene();
    let a = accessible(&s, s.save);
    set(&s.engine, a, "labelledBy", Value::Object(s.status));
    let h = harness(&s);
    let label = h.state().view.accessible_id(s.status).unwrap();
    let save = h.get_by_role_and_label(Role::Button, "Save");
    let named_by: Vec<_> = save
        .accesskit_node()
        .labelled_by()
        .map(|n| n.locate().0)
        .collect();
    assert_eq!(named_by, vec![label.accesskit_id()]);
}

#[test]
fn what_is_announced_is_a_live_node() {
    let s = scene();
    let mut h = harness(&s);
    let a = accessible(&s, s.save);
    s.engine
        .invoke_by_name(a, "announce", &[Value::string("Saved")])
        .unwrap();
    h.run();
    let said = h.get_by_role_and_label(Role::Status, "Saved");
    assert_eq!(said.accesskit_node().live(), egui::accesskit::Live::Polite);
}

#[test]
fn the_view_says_that_something_listens() {
    let s = scene();
    assert!(!ljos_quick::system_prefs(&s.engine).screen_reader_active);
    let _h = harness(&s);
    assert!(ljos_quick::system_prefs(&s.engine).screen_reader_active);
}

#[test]
fn the_scheme_of_the_system_is_the_application_s() {
    use ljos_a11y::ColorScheme;
    let s = scene();
    let ctx = egui::Context::default();
    let mut view = QmlView::new(Rc::clone(&s.engine), s.root).unwrap();
    let mut pass_with = |theme| {
        let input = egui::RawInput {
            system_theme: theme,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            view.show(ui);
        });
        out.textures_delta.clear();
    };
    // egui does not know: the scheme is whatever it was.
    let before = ljos_quick::color_scheme(&s.engine);
    assert_ne!(before, ColorScheme::Dark);
    pass_with(None);
    assert_eq!(ljos_quick::color_scheme(&s.engine), before);
    pass_with(Some(egui::Theme::Dark));
    assert_eq!(ljos_quick::color_scheme(&s.engine), ColorScheme::Dark);
    // A scheme set over the system's stays while the system's changes.
    ljos_quick::update_system_prefs(&s.engine, |p| p.color_scheme = ColorScheme::Light);
    pass_with(Some(egui::Theme::Dark));
    assert_eq!(ljos_quick::color_scheme(&s.engine), ColorScheme::Light);
    pass_with(Some(egui::Theme::Light));
    pass_with(Some(egui::Theme::Dark));
    assert_eq!(ljos_quick::color_scheme(&s.engine), ColorScheme::Light);
}
