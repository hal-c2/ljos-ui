//! A view whose accessibility tree is grafted: one node in egui's tree,
//! and the scene's tree as an AccessKit tree of its own below it. Read
//! the way a platform adapter reads it, through `accesskit_consumer`
//! (which kittest wraps), fed the two updates of each pass in order.

use std::cell::Cell;
use std::rc::Rc;

use egui::accesskit::{Action, ActionRequest, NodeId, Role, TreeId, TreeUpdate, Uuid};
use egui::{Pos2, Rect, Vec2};
use egui_kittest::kittest::{AccessKitNode, State};
use ljos_a11y::Role as QtRole;
use ljos_egui::QmlView;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Accessible, Item, MouseArea, Rectangle, add_data};

const SUB: TreeId = TreeId(Uuid::from_u128(0x6c6a_6f73_0000_0000_0000_0000_0000_0001));

fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

fn accessible(e: &Engine, o: ObjectId) -> ObjectId {
    e.attached_object(o, &MetaRef::Static(Accessible::static_meta()))
        .unwrap()
}

/// A "Save" button of 60 by 30 at (15, 15), over a MouseArea.
struct Scene {
    engine: Rc<Engine>,
    root: ObjectId,
    save: ObjectId,
    clicks: Rc<Cell<u32>>,
}

fn scene() -> Scene {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);
    let save = make::<Rectangle>(&e, Some(root));
    e.set(save, Item::X, 15.0).unwrap();
    e.set(save, Item::Y, 15.0).unwrap();
    e.set(save, Item::WIDTH, 60.0).unwrap();
    e.set(save, Item::HEIGHT, 30.0).unwrap();
    let a = accessible(&e, save);
    set(&e, a, "role", Value::Int(QtRole::Button.qt_value()));
    set(&e, a, "name", Value::string("Save"));
    let area = make::<MouseArea>(&e, Some(save));
    set(&e, area, "anchors.fill", Value::Object(save));
    let clicks = Rc::new(Cell::new(0));
    let c = Rc::clone(&clicks);
    e.connect_by_name(area, "clicked", move |_, _| c.set(c.get() + 1))
        .unwrap();
    Scene {
        engine: e,
        root,
        save,
        clicks,
    }
}

/// Where the view is in the window: not at its corner, so that a node's
/// place says whether the graft moved it.
const AT: Pos2 = Pos2::new(40.0, 20.0);

/// A host that owns the adapter: an egui context, and what the adapter
/// was told.
struct Host {
    ctx: egui::Context,
    view: QmlView,
    told: bool,
    adapter: Option<State>,
    /// The updates of the last pass, as they were sent.
    last: Vec<TreeUpdate>,
    /// Every update that was sent, in order.
    sent: Vec<TreeUpdate>,
    /// How often the focus was kept off the graft node.
    held_back: u32,
}

impl Host {
    fn new(s: &Scene) -> Self {
        let view = QmlView::new(Rc::clone(&s.engine), s.root)
            .unwrap()
            .with_accessibility_graft(SUB);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Host {
            ctx,
            view,
            told: false,
            adapter: None,
            last: Vec::new(),
            sent: Vec::new(),
            held_back: 0,
        }
    }

    fn pass_with(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0))),
            focused: true,
            events,
            ..Default::default()
        };
        let view = &mut self.view;
        let mut out = self.ctx.run_ui(input, |ui| {
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(Rect::from_min_size(AT, Vec2::new(200.0, 100.0))),
                |ui| {
                    view.show(ui);
                },
            );
        });
        out.textures_delta.clear();
        self.last.clear();
        let Some(root) = out.platform_output.accesskit_update.take() else {
            return;
        };
        let updates = self.view.graft_updates(root, &mut self.told);
        self.held_back += u32::from(updates.again);
        for update in std::iter::once(updates.root).chain(updates.grafted) {
            self.last.push(update.clone());
            self.sent.push(update.clone());
            match &mut self.adapter {
                // The consumer panics on an update it cannot take: that a
                // pass gets here is the test of the order.
                Some(state) => state.update(update),
                None => self.adapter = Some(State::new(update)),
            }
        }
    }

    fn pass(&mut self) {
        self.pass_with(Vec::new());
    }

    fn root(&self) -> AccessKitNode<'_> {
        self.adapter.as_ref().expect("a pass ran").root()
    }
}

fn find<'a>(node: AccessKitNode<'a>, role: Role, label: &str) -> Option<AccessKitNode<'a>> {
    if node.role() == role && node.label().as_deref() == Some(label) {
        return Some(node);
    }
    node.children().find_map(|c| find(c, role, label))
}

fn key(s: &Scene, h: &Host) -> NodeId {
    h.view.scene().accessible_node(s.save).unwrap().node_id()
}

#[test]
fn the_scene_s_tree_hangs_at_one_node_of_egui_s() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();

    let graft = h
        .view
        .accessibility_graft_node()
        .expect("something listens");
    let (_, node) = h.last[0]
        .nodes
        .iter()
        .find(|(id, _)| *id == graft)
        .expect("the graft node is in egui's tree");
    assert_eq!(node.tree_id(), Some(SUB));
    assert!(node.children().is_empty());
    // egui's tree has no node of the scene: only the graft.
    assert!(
        h.last[0]
            .nodes
            .iter()
            .all(|(_, n)| n.label() != Some("Save"))
    );
    // The scene's tree came after it, whole.
    assert_eq!(h.last.len(), 2);
    assert_eq!(h.last[1].tree_id, SUB);
    assert!(h.last[1].tree.is_some());

    let save = find(h.root(), Role::Button, "Save").expect("found through the graft");
    assert_eq!(save.locate(), (key(&s, &h), SUB));
}

#[test]
fn a_grafted_node_is_where_its_item_is_painted() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    let save = find(h.root(), Role::Button, "Save").unwrap();
    let b = save.bounding_box().unwrap();
    // The item is at (15, 15) in a view at `AT`.
    assert_eq!((b.x0, b.y0), (55.0, 35.0));
    assert_eq!((b.x1, b.y1), (115.0, 65.0));
}

#[test]
fn a_pass_in_which_nothing_changed_tells_nothing_of_the_scene() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    h.pass();
    h.pass();
    // egui repeats its tree; the scene's is not repeated.
    assert_eq!(h.last.len(), 1);
    assert_eq!(h.last[0].tree_id, TreeId::ROOT);
}

#[test]
fn a_change_tells_the_node_that_changed() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    h.pass();
    set(
        &s.engine,
        accessible(&s.engine, s.save),
        "name",
        Value::string("Store"),
    );
    h.pass();
    assert_eq!(h.last.len(), 2);
    let delta = &h.last[1];
    assert_eq!(delta.tree_id, SUB);
    assert!(delta.tree.is_none(), "not the whole tree again");
    assert_eq!(delta.nodes.len(), 1);
    assert_eq!(delta.nodes[0].0, key(&s, &h));
    assert!(find(h.root(), Role::Button, "Store").is_some());
    assert!(find(h.root(), Role::Button, "Save").is_none());
}

#[test]
fn the_focus_is_not_on_the_graft_before_its_tree_is_there() {
    let s = scene();
    let mut h = Host::new(&s);
    // The view takes the free keyboard at once, so egui's focus is on the
    // graft node in the very pass that makes it. The consumer panics if
    // it is told so before the grafted tree: getting through the passes
    // is the test.
    for _ in 0..3 {
        h.pass();
    }
    assert_eq!(h.held_back, 1, "once, in the pass that made the graft");
    let graft = h.view.accessibility_graft_node().unwrap();
    assert_eq!(h.last[0].focus, graft, "afterwards it is said as it is");
}

/// What a platform adapter does with what it hears.
struct Quiet;

impl accesskit_consumer::TreeChangeHandler for Quiet {
    fn node_added(&mut self, _: &accesskit_consumer::Node) {}
    fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {}
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::Node>,
        _: Option<&accesskit_consumer::Node>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
}

/// The trees as an adapter of a focused window has them after `sent`.
fn adapter_after(sent: &[TreeUpdate]) -> accesskit_consumer::Tree {
    let mut tree = accesskit_consumer::Tree::new(sent[0].clone(), true);
    for update in &sent[1..] {
        tree.update_and_process_changes(update.clone(), &mut Quiet);
    }
    tree
}

/// Passes until the adapter was told that the focus is in the scene.
fn focus_in_the_scene(h: &mut Host) {
    for _ in 0..3 {
        h.pass();
    }
    let graft = h.view.accessibility_graft_node().unwrap();
    assert_eq!(h.last[0].focus, graft);
}

#[test]
fn the_focus_leaves_the_graft_while_the_window_s_focus_changes() {
    let s = scene();
    let mut h = Host::new(&s);
    focus_in_the_scene(&mut h);
    let mut adapter = adapter_after(&h.sent);
    let in_scene = adapter.state().focus_id().unwrap();
    assert_eq!(
        adapter.state().locate_node(in_scene).map(|(_, tree)| tree),
        Some(SUB),
        "the focus is a node of the scene's tree"
    );

    let release = h
        .view
        .release_graft_focus()
        .expect("the focus is on the graft");
    assert_eq!(release.tree_id, TreeId::ROOT);
    assert!(release.nodes.is_empty() && release.tree.is_none());
    adapter.update_and_process_changes(release, &mut Quiet);
    // The window lost the focus and got it back: what an adapter does
    // when it is told so.
    adapter.update_host_focus_state_and_process_changes(false, &mut Quiet);
    adapter.update_host_focus_state_and_process_changes(true, &mut Quiet);
    assert_ne!(adapter.state().focus_id(), Some(in_scene));
    assert!(
        h.view.release_graft_focus().is_none(),
        "it is released once"
    );

    // The next pass says where the focus is.
    let before = h.sent.len();
    h.pass();
    for update in &h.sent[before..] {
        adapter.update_and_process_changes(update.clone(), &mut Quiet);
    }
    assert_eq!(adapter.state().focus_id(), Some(in_scene));
}

/// Why [`QmlView::release_graft_focus`] is there. When this stops
/// panicking the adapters were mended (accesskit_consumer 0.39 is), and
/// a host need not release the focus any more.
#[test]
#[should_panic(expected = "is not in the node list")]
fn an_adapter_cannot_take_the_window_s_focus_changing_with_the_focus_in_a_graft() {
    let s = scene();
    let mut h = Host::new(&s);
    focus_in_the_scene(&mut h);
    let mut adapter = adapter_after(&h.sent);
    adapter.update_host_focus_state_and_process_changes(false, &mut Quiet);
}

#[test]
fn there_is_nothing_to_release_while_the_focus_is_elsewhere() {
    let s = scene();
    let mut h = Host::new(&s);
    // The pass that makes the graft keeps the focus off it.
    h.pass();
    assert_eq!(h.held_back, 1);
    assert!(h.view.release_graft_focus().is_none());
}

#[test]
fn the_press_action_names_the_scene_s_own_node() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    let request = ActionRequest {
        action: Action::Click,
        target_tree: SUB,
        target_node: key(&s, &h),
        data: None,
    };
    h.pass_with(vec![egui::Event::AccessKitActionRequest(request)]);
    h.pass();
    assert_eq!(s.clicks.get(), 1);
    assert_eq!(h.view.stats().accessible_presses, 1);
}

#[test]
fn an_action_for_egui_s_tree_is_not_the_scene_s() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    // The same number in egui's tree names another node, or none.
    let request = ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: key(&s, &h),
        data: None,
    };
    h.pass_with(vec![egui::Event::AccessKitActionRequest(request)]);
    h.pass();
    assert_eq!(s.clicks.get(), 0);
}

#[test]
fn an_adapter_that_starts_anew_is_told_everything() {
    let s = scene();
    let mut h = Host::new(&s);
    h.pass();
    h.pass();
    // The assistive technology went and came back: a new adapter.
    h.adapter = None;
    h.told = false;
    h.pass();
    assert_eq!(h.last.len(), 2);
    assert!(h.last[1].tree.is_some());
    assert!(find(h.root(), Role::Button, "Save").is_some());
}

#[test]
fn without_a_listener_there_is_no_graft_and_no_tree() {
    let s = scene();
    let mut view = QmlView::new(Rc::clone(&s.engine), s.root)
        .unwrap()
        .with_accessibility_graft(SUB);
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        view.show(ui);
    });
    out.textures_delta.clear();
    assert!(out.platform_output.accesskit_update.is_none());
    assert_eq!(view.accessibility_graft_node(), None);
    assert!(view.take_accessibility_update(true).is_none());
    assert!(!view.scene().accessibility_active());
}
