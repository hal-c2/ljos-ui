//! Keys and focus between egui and the scene: who has the keyboard, what
//! the scene is told, what is left in egui's input, and where an assistive
//! technology finds the focus.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::accesskit::Role;
use egui::{Event, Id, Key, Modifiers, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ljos_a11y::Role as QtRole;
use ljos_egui::{KeyboardPolicy, QmlView, ResizeMode};
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::keys::{KeyItem, Shortcut, key, register_key_item};
use ljos_quick::{Accessible, Item, KeyEvent, KeyKind, Rectangle, add_data};

thread_local! {
    /// What the Rectangles were handed, as `"press 65 a"`.
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The keys a Rectangle takes.
    static TAKE: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
}

fn log() -> Vec<String> {
    LOG.with(|l| l.borrow().clone())
}

fn take(keys: &[i32]) {
    TAKE.with(|t| *t.borrow_mut() = keys.to_vec());
}

fn note(e: &Engine, item: ObjectId, ev: &KeyEvent) -> bool {
    let kind = match ev.kind {
        KeyKind::Press => "press",
        KeyKind::Release => "release",
    };
    let name = e.read_by_name(item, "objectName").to_js_string();
    let repeat = if ev.auto_repeat { " repeat" } else { "" };
    LOG.with(|l| {
        l.borrow_mut().push(
            format!(
                "{name} {kind} {:#x} {:?} {}{repeat}",
                ev.key,
                ev.text,
                ev.modifiers.bits()
            )
            .trim_end()
            .to_owned(),
        );
    });
    TAKE.with(|t| t.borrow().contains(&ev.key))
}

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

fn flag(e: &Engine, o: ObjectId, name: &str) -> bool {
    e.read_by_name(o, name).to_boolean()
}

/// A 200 by 80 root with two fields, "one" (with focus) and "two", both
/// buttons to an assistive technology, and a Ctrl+S shortcut.
struct Scene {
    engine: Rc<Engine>,
    root: ObjectId,
    one: ObjectId,
    two: ObjectId,
    saves: Rc<Cell<u32>>,
}

fn field(e: &Engine, root: ObjectId, name: &str, label: &str, x: f64) -> ObjectId {
    let o = make::<Rectangle>(e, Some(root));
    e.set(o, Item::X, x).unwrap();
    e.set(o, Item::WIDTH, 80.0).unwrap();
    e.set(o, Item::HEIGHT, 30.0).unwrap();
    set(e, o, "objectName", Value::string(name));
    let a = e
        .attached_object(o, &MetaRef::Static(Accessible::static_meta()))
        .unwrap();
    set(e, a, "role", Value::Int(QtRole::Button.qt_value()));
    set(e, a, "name", Value::string(label));
    o
}

fn shortcut(e: &Engine, root: ObjectId, sequence: &str) -> Rc<Cell<u32>> {
    let s = make::<Shortcut>(e, Some(root));
    set(e, s, "sequence", Value::string(sequence));
    let count = Rc::new(Cell::new(0));
    let c = Rc::clone(&count);
    e.connect_by_name(s, "activated", move |_, _| c.set(c.get() + 1))
        .unwrap();
    count
}

fn scene() -> Scene {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    register_key_item(
        &e,
        MetaRef::Static(Rectangle::static_meta()),
        KeyItem {
            shortcut_override: |_, _, _| false,
            press: note,
            release: note,
        },
    );
    let root = make::<Item>(&e, None);
    e.set(root, Item::WIDTH, 200.0).unwrap();
    e.set(root, Item::HEIGHT, 80.0).unwrap();
    let one = field(&e, root, "one", "One", 10.0);
    let two = field(&e, root, "two", "Two", 100.0);
    set(&e, one, "focus", Value::Bool(true));
    let saves = shortcut(&e, root, "Ctrl+S");
    Scene {
        engine: e,
        root,
        one,
        two,
        saves,
    }
}

fn tab_stops(s: &Scene) {
    set(&s.engine, s.one, "activeFocusOnTab", Value::Bool(true));
    set(&s.engine, s.two, "activeFocusOnTab", Value::Bool(true));
}

/// What egui shows around the view.
#[derive(Clone, Copy, Default)]
struct Around {
    field_before: bool,
    button_after: bool,
    /// A modal dialog over everything, with nothing in it to focus.
    modal: bool,
}

struct State {
    view: QmlView,
    around: Around,
    text: String,
    /// The key, text and clipboard events left in egui's input after the
    /// view, over all passes.
    left: Vec<Event>,
    a_is_down: bool,
}

fn field_id() -> Id {
    Id::new("the field before")
}

fn harness(s: &Scene, policy: KeyboardPolicy, around: Around) -> Harness<'static, State> {
    let view = QmlView::new(Rc::clone(&s.engine), s.root)
        .unwrap()
        .with_resize_mode(ResizeMode::ViewToRoot)
        .with_keyboard_policy(policy);
    let mut h = Harness::builder()
        .with_size(Vec2::new(260.0, 200.0))
        .build_ui_state(
            |ui, st: &mut State| {
                if st.around.field_before {
                    ui.add(egui::TextEdit::singleline(&mut st.text).id(field_id()));
                }
                st.view.show(ui);
                ui.input(|i| {
                    st.left.extend(
                        i.events
                            .iter()
                            .filter(|e| {
                                matches!(
                                    e,
                                    Event::Key { .. }
                                        | Event::Text(_)
                                        | Event::Copy
                                        | Event::Paste(_)
                                )
                            })
                            .cloned(),
                    );
                    st.a_is_down = i.key_down(Key::A);
                });
                if st.around.button_after {
                    let _ = ui.button("After");
                }
                if st.around.modal {
                    egui::Modal::new(Id::new("a dialog")).show(ui.ctx(), |ui| {
                        ui.label("Sure?");
                    });
                }
            },
            State {
                view,
                around,
                text: String::new(),
                left: Vec::new(),
                a_is_down: false,
            },
        );
    h.run();
    h
}

fn alone(s: &Scene) -> Harness<'static, State> {
    harness(s, KeyboardPolicy::WhenFree, Around::default())
}

fn down(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn up(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    }
}

/// One pass with `events` in it, the way the platform layer delivers a
/// key and its text together.
fn pass(h: &mut Harness<'_, State>, events: impl IntoIterator<Item = Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

/// A key that types `text` goes down and comes up.
fn type_key(h: &mut Harness<'_, State>, k: Key, text: &str) {
    pass(h, [down(k, Modifiers::NONE), Event::Text(text.to_owned())]);
    pass(h, [up(k, Modifiers::NONE)]);
}

fn press(h: &mut Harness<'_, State>, k: Key, modifiers: Modifiers) {
    pass(h, [down(k, modifiers)]);
    pass(h, [up(k, modifiers)]);
}

fn egui_focus(h: &Harness<'_, State>) -> Option<Id> {
    h.ctx.memory(|m| m.focused())
}

fn left_keys(h: &Harness<'_, State>) -> Vec<(Key, bool)> {
    h.state()
        .left
        .iter()
        .filter_map(|e| match e {
            Event::Key { key, pressed, .. } => Some((*key, *pressed)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_view_that_is_the_content_has_the_keyboard_from_the_start() {
    let s = scene();
    let h = alone(&s);
    let view = &h.state().view;
    assert!(view.has_keyboard());
    assert_eq!(egui_focus(&h), Some(view.focus_id()));
    assert!(view.scene().is_window_active(&s.engine));
    assert!(flag(&s.engine, s.one, "activeFocus"));
    assert_eq!(view.scene().active_focus_item(&s.engine), Some(s.one));
}

#[test]
fn a_key_and_its_text_reach_the_item_with_active_focus() {
    let s = scene();
    let mut h = alone(&s);
    type_key(&mut h, Key::A, "a");
    assert_eq!(
        log(),
        ["one press 0x41 \"a\" 0", "one release 0x41 \"a\" 0"]
    );
    let stats = h.state().view.stats();
    assert_eq!((stats.key_events, stats.keys_accepted), (2, 0));
}

#[test]
fn a_held_key_and_its_modifiers_are_told() {
    let s = scene();
    let mut h = alone(&s);
    pass(&mut h, [down(Key::ArrowDown, Modifiers::SHIFT)]);
    let repeat = Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: Modifiers::SHIFT,
    };
    pass(&mut h, [repeat]);
    let shift = ljos_quick::Modifiers::SHIFT.bits();
    assert_eq!(
        log(),
        [
            format!("one press {:#x} \"\" {shift}", key::DOWN),
            format!("one press {:#x} \"\" {shift} repeat", key::DOWN),
        ]
    );
}

#[test]
fn an_accepted_key_is_gone_from_egui_s_input_and_the_others_stay() {
    let s = scene();
    take(&[key::A]);
    let mut h = alone(&s);
    pass(
        &mut h,
        [
            down(Key::A, Modifiers::NONE),
            Event::Text("a".into()),
            down(Key::B, Modifiers::NONE),
            Event::Text("b".into()),
        ],
    );
    assert_eq!(left_keys(&h), [(Key::B, true)]);
    let texts: Vec<&Event> = h
        .state()
        .left
        .iter()
        .filter(|e| matches!(e, Event::Text(_)))
        .collect();
    assert_eq!(texts, [&Event::Text("b".into())]);
    // egui does not think the key is held either.
    assert!(!h.state().a_is_down);
    pass(
        &mut h,
        [up(Key::A, Modifiers::NONE), up(Key::B, Modifiers::NONE)],
    );
    assert_eq!(left_keys(&h), [(Key::B, true), (Key::B, false)]);
    let stats = h.state().view.stats();
    assert_eq!((stats.key_events, stats.keys_accepted), (4, 2));
}

#[test]
fn a_shortcut_takes_its_chord_out_of_egui_s_input() {
    let s = scene();
    let mut h = alone(&s);
    press(&mut h, Key::S, Modifiers::CTRL);
    assert_eq!(s.saves.get(), 1);
    // The press went to the shortcut; the release went past the item.
    assert_eq!(left_keys(&h), [(Key::S, false)]);
    let ctrl = ljos_quick::Modifiers::CONTROL.bits();
    assert_eq!(log(), [format!("one release 0x53 \"\" {ctrl}")]);
}

#[test]
fn the_clipboard_events_are_chords_a_shortcut_can_take() {
    let s = scene();
    let copies = shortcut(&s.engine, s.root, "Ctrl+C");
    let mut h = alone(&s);
    pass(&mut h, [Event::Copy]);
    assert_eq!(copies.get(), 1);
    assert!(h.state().left.is_empty(), "{:?}", h.state().left);
    // Nothing in the scene pastes: the text stays for whoever does.
    pass(&mut h, [Event::Paste("words".into())]);
    assert_eq!(h.state().left, [Event::Paste("words".into())]);
}

#[test]
fn text_with_no_key_is_left_for_a_text_item() {
    let s = scene();
    let mut h = alone(&s);
    pass(&mut h, [Event::Text("né".into())]);
    assert!(log().is_empty());
    assert_eq!(h.state().left, [Event::Text("né".into())]);
}

#[test]
fn tab_moves_active_focus_inside_the_scene_and_egui_s_focus_stays() {
    let s = scene();
    tab_stops(&s);
    let around = Around {
        button_after: true,
        ..Around::default()
    };
    let mut h = harness(&s, KeyboardPolicy::WhenFree, around);
    let id = h.state().view.focus_id();
    press(&mut h, Key::Tab, Modifiers::NONE);
    assert!(flag(&s.engine, s.two, "activeFocus"));
    assert_eq!(egui_focus(&h), Some(id));
    press(&mut h, Key::Tab, Modifiers::SHIFT);
    assert!(flag(&s.engine, s.one, "activeFocus"));
    assert_eq!(egui_focus(&h), Some(id));
    // The scene took every Tab press.
    assert_eq!(left_keys(&h), [(Key::Tab, false), (Key::Tab, false)]);
}

#[test]
fn a_tab_the_scene_leaves_moves_egui_s_focus_on() {
    let s = scene();
    let around = Around {
        button_after: true,
        ..Around::default()
    };
    let mut h = harness(&s, KeyboardPolicy::WhenFocused, around);
    h.state().view.request_keyboard(&h.ctx);
    h.run();
    assert!(h.state().view.has_keyboard());
    assert!(flag(&s.engine, s.one, "activeFocus"));

    // No item is a Tab stop, so the key is egui's.
    press(&mut h, Key::Tab, Modifiers::NONE);
    h.run();
    assert!(h.get_by_role_and_label(Role::Button, "After").is_focused());
    let view = &h.state().view;
    assert!(!view.has_keyboard());
    // Without the keyboard the scene's window is not the active one.
    assert!(!view.scene().is_window_active(&s.engine));
    assert!(!flag(&s.engine, s.one, "activeFocus"));
    assert!(flag(&s.engine, s.one, "focus"));

    // Shift+Tab on the button comes back, and so does active focus.
    press(&mut h, Key::Tab, Modifiers::SHIFT);
    h.run();
    assert!(h.state().view.has_keyboard());
    assert!(flag(&s.engine, s.one, "activeFocus"));
    // The Tab that brought the keyboard was not typed into the scene.
    let presses = log().iter().filter(|l| l.contains("press")).count();
    assert_eq!(presses, 1, "{:?}", log());
}

#[test]
fn a_view_among_widgets_gets_the_keyboard_by_a_click() {
    let s = scene();
    let around = Around {
        field_before: true,
        ..Around::default()
    };
    let mut h = harness(&s, KeyboardPolicy::WhenFocused, around);
    assert!(!h.state().view.has_keyboard());
    assert_eq!(egui_focus(&h), None);
    assert!(!h.state().view.scene().is_window_active(&s.engine));
    assert!(!flag(&s.engine, s.one, "activeFocus"));
    type_key(&mut h, Key::A, "a");
    assert!(log().is_empty());

    h.get_by_role_and_label(Role::Button, "Two").click();
    h.run();
    assert!(h.state().view.has_keyboard());
    assert!(flag(&s.engine, s.one, "activeFocus"));
    type_key(&mut h, Key::A, "a");
    assert_eq!(log().len(), 2);
}

#[test]
fn a_focused_egui_widget_keeps_its_keys() {
    let s = scene();
    let around = Around {
        field_before: true,
        ..Around::default()
    };
    let mut h = harness(&s, KeyboardPolicy::WhenFree, around);
    assert!(h.state().view.has_keyboard());
    h.ctx.memory_mut(|m| m.request_focus(field_id()));
    h.run();
    assert!(!h.state().view.has_keyboard());
    assert!(!flag(&s.engine, s.one, "activeFocus"));
    type_key(&mut h, Key::A, "a");
    press(&mut h, Key::S, Modifiers::CTRL);
    assert_eq!(h.state().text, "a");
    assert!(log().is_empty());
    assert_eq!(s.saves.get(), 0);
    assert_eq!(h.state().view.stats().key_events, 0);

    // Escape lets go of the field. The view takes the free keyboard, but
    // that Escape was the field's.
    press(&mut h, Key::Escape, Modifiers::NONE);
    h.run();
    assert!(h.state().view.has_keyboard());
    assert!(flag(&s.engine, s.one, "activeFocus"));
    assert!(!log().iter().any(|l| l.contains("press")), "{:?}", log());
}

#[test]
fn escape_is_the_scene_s_first() {
    let s = scene();
    take(&[key::ESCAPE]);
    let mut h = harness(&s, KeyboardPolicy::WhenFocused, Around::default());
    let at = h.get_by_role_and_label(Role::Button, "Two").rect().center();
    pass(&mut h, [Event::PointerMoved(at)]);
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    pass(&mut h, [button(true)]);
    assert!(h.state().view.has_keyboard());
    // The very next pass: egui has not yet been told to leave Escape to
    // the view, and has dropped the focus for it before the view ran.
    pass(&mut h, [down(Key::Escape, Modifiers::NONE)]);
    assert!(h.state().view.has_keyboard());
    pass(&mut h, [up(Key::Escape, Modifiers::NONE), button(false)]);
    assert!(h.state().view.has_keyboard());
    // Later ones egui leaves alone.
    press(&mut h, Key::Escape, Modifiers::NONE);
    assert!(h.state().view.has_keyboard());
    let presses = log().iter().filter(|l| l.contains("press")).count();
    assert_eq!(presses, 2, "{:?}", log());
    assert_eq!(egui_focus(&h), Some(h.state().view.focus_id()));
    assert!(left_keys(&h).is_empty());

    // An Escape nobody in the scene wants gives the keyboard up, as it
    // does for any egui widget.
    take(&[]);
    press(&mut h, Key::Escape, Modifiers::NONE);
    assert!(!h.state().view.has_keyboard());
    assert_eq!(egui_focus(&h), None);
    assert!(!flag(&s.engine, s.one, "activeFocus"));
}

#[test]
fn a_content_view_keeps_the_keyboard_through_keys_nobody_takes() {
    let s = scene();
    let mut h = alone(&s);
    let active = Rc::new(Cell::new(0));
    let a = Rc::clone(&active);
    s.engine
        .connect_by_name(s.one, "activeFocusChanged", move |_, _| a.set(a.get() + 1))
        .unwrap();
    for k in [Key::Escape, Key::Tab, Key::ArrowDown] {
        press(&mut h, k, Modifiers::NONE);
        h.run();
        assert!(h.state().view.has_keyboard(), "{k:?}");
    }
    press(&mut h, Key::Tab, Modifiers::SHIFT);
    type_key(&mut h, Key::A, "a");
    assert_eq!(egui_focus(&h), Some(h.state().view.focus_id()));
    // Every one of them reached the item, and it never blinked.
    assert_eq!(log().len(), 10, "{:?}", log());
    assert_eq!(active.get(), 0);
}

#[test]
fn a_modal_layer_over_the_view_takes_the_keyboard_away() {
    let s = scene();
    let mut h = alone(&s);
    h.state_mut().around.modal = true;
    h.run();
    assert!(!h.state().view.has_keyboard());
    assert!(!flag(&s.engine, s.one, "activeFocus"));
    type_key(&mut h, Key::A, "a");
    press(&mut h, Key::S, Modifiers::CTRL);
    assert!(log().is_empty());
    assert_eq!(s.saves.get(), 0);
    // The dialog closes: the keyboard is free again.
    h.state_mut().around.modal = false;
    h.run();
    h.step();
    assert!(h.state().view.has_keyboard());
    assert!(flag(&s.engine, s.one, "activeFocus"));
}

#[test]
fn the_scene_s_window_is_active_while_the_viewport_has_the_focus() {
    let s = scene();
    let mut h = alone(&s);
    h.input_mut().focused = false;
    pass(&mut h, [Event::WindowFocused(false)]);
    assert!(!h.state().view.scene().is_window_active(&s.engine));
    assert!(!flag(&s.engine, s.one, "activeFocus"));
    h.input_mut().focused = true;
    pass(&mut h, [Event::WindowFocused(true)]);
    assert!(h.state().view.scene().is_window_active(&s.engine));
    assert!(flag(&s.engine, s.one, "activeFocus"));
}

#[test]
fn an_assistive_technology_finds_the_focus_on_the_item() {
    let s = scene();
    tab_stops(&s);
    let mut h = alone(&s);
    assert!(h.get_by_role_and_label(Role::Button, "One").is_focused());
    assert!(!h.get_by_role_and_label(Role::Button, "Two").is_focused());
    press(&mut h, Key::Tab, Modifiers::NONE);
    h.run();
    assert!(h.get_by_role_and_label(Role::Button, "Two").is_focused());
    assert!(!h.get_by_role_and_label(Role::Button, "One").is_focused());
    // egui's own focus never left the view's widget.
    assert_eq!(egui_focus(&h), Some(h.state().view.focus_id()));
}

#[test]
fn the_focus_action_gives_the_item_active_focus_and_the_view_the_keyboard() {
    let s = scene();
    let around = Around {
        field_before: true,
        ..Around::default()
    };
    let mut h = harness(&s, KeyboardPolicy::WhenFocused, around);
    assert!(!h.state().view.has_keyboard());
    h.get_by_role_and_label(Role::Button, "Two").focus();
    h.run();
    let view = &h.state().view;
    assert_eq!(view.stats().accessible_focuses, 1);
    assert!(view.has_keyboard());
    assert!(flag(&s.engine, s.two, "activeFocus"));
    assert!(!flag(&s.engine, s.one, "focus"));
    assert!(h.get_by_role_and_label(Role::Button, "Two").is_focused());
}

#[test]
fn an_idle_view_with_the_keyboard_runs_no_frames() {
    let s = scene();
    tab_stops(&s);
    let mut h = alone(&s);
    press(&mut h, Key::Tab, Modifiers::NONE);
    h.run();
    let before = h.state().view.stats();
    h.run_steps(5);
    let after = h.state().view.stats();
    assert_eq!(after.frames, before.frames);
    assert_eq!(after.repaint_requests, before.repaint_requests);
    assert_eq!(after.key_events, before.key_events);
}
