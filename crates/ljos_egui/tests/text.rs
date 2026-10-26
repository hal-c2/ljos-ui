//! Text between the platform and the editors of a scene: keys that type,
//! text that comes without a key, an input method that composes, the
//! clipboard, what the platform's input method is told, and what an
//! assistive technology reads and asks of an editor's text.

use std::rc::Rc;
use std::sync::Arc;

use egui::accesskit::{
    Action, ActionData, ActionRequest, Role, TextPosition, TextSelection, TreeId,
};
use egui::{Event, IMEPurpose, ImeEvent, Key, Modifiers, OutputCommand, Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ljos_egui::QmlView;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Accessible, Item, TextEdit, TextInput, add_data};
use ljos_runtime::VirtualClock;

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

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    e.read_by_name(o, name).to_js_string().to_string()
}

fn name(e: &Engine, o: ObjectId, name: &str) {
    let a = e
        .attached_object(o, &MetaRef::Static(Accessible::static_meta()))
        .unwrap();
    set(e, a, "name", Value::string(name));
}

/// A field "Name" holding "hello" with focus, a field "Secret" that hides
/// "hunter2", and a `TextEdit` "Notes" below them.
struct Scene {
    engine: Rc<Engine>,
    root: ObjectId,
    one: ObjectId,
    secret: ObjectId,
    notes: ObjectId,
    /// The one clock of the engine's timers and of the view.
    clock: Arc<VirtualClock>,
}

fn scene() -> Scene {
    let e = Rc::new(Engine::new());
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(Arc::clone(&clock)));
    ljos_quick::register(&e);
    let root = make::<Item>(&e, None);

    let one = make::<TextInput>(&e, Some(root));
    geom(&e, one, 10.0, 10.0, 150.0, 20.0);
    set(&e, one, "text", Value::string("hello"));
    name(&e, one, "Name");

    let secret = make::<TextInput>(&e, Some(root));
    geom(&e, secret, 10.0, 40.0, 150.0, 20.0);
    set(&e, secret, "echoMode", Value::Int(2));
    set(&e, secret, "text", Value::string("hunter2"));
    name(&e, secret, "Secret");

    let notes = make::<TextEdit>(&e, Some(root));
    geom(&e, notes, 10.0, 70.0, 150.0, 40.0);
    name(&e, notes, "Notes");

    set(&e, one, "focus", Value::Bool(true));
    Scene {
        engine: e,
        root,
        one,
        secret,
        notes,
        clock,
    }
}

struct State {
    view: QmlView,
    rect: Rect,
    /// The key, text and input method events left in egui's input after
    /// the view, over all passes.
    left: Vec<Event>,
}

fn harness(s: &Scene) -> Harness<'static, State> {
    let view = QmlView::new(Rc::clone(&s.engine), s.root)
        .unwrap()
        .with_clock(Arc::clone(&s.clock) as Arc<dyn ljos_runtime::Clock>);
    let mut h = Harness::builder()
        .with_size(Vec2::new(220.0, 140.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.rect = st.view.show(ui).rect;
                ui.input(|i| {
                    st.left.extend(
                        i.events
                            .iter()
                            .filter(|e| {
                                matches!(
                                    e,
                                    Event::Key { .. }
                                        | Event::Text(_)
                                        | Event::Ime(_)
                                        | Event::Copy
                                        | Event::Cut
                                        | Event::Paste(_)
                                )
                            })
                            .cloned(),
                    );
                });
            },
            State {
                view,
                rect: Rect::NOTHING,
                left: Vec::new(),
            },
        );
    h.run();
    h
}

fn down(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn up(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn pass(h: &mut Harness<'_, State>, events: impl IntoIterator<Item = Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

fn preedit(text: &str) -> Event {
    Event::Ime(ImeEvent::Preedit {
        text: text.to_owned(),
        active_range_chars: Some(text.chars().count()..text.chars().count()),
    })
}

fn copied(h: &Harness<'_, State>) -> Vec<String> {
    h.output()
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_key_types_what_its_text_says() {
    let s = scene();
    let mut h = harness(&s);
    pass(&mut h, [down(Key::A), Event::Text("a".into())]);
    pass(&mut h, [up(Key::A)]);
    assert_eq!(text(&s.engine, s.one, "text"), "helloa");
    // The editor took the press and its text; nothing else types it again.
    assert!(
        !h.state()
            .left
            .iter()
            .any(|e| matches!(e, Event::Text(_) | Event::Key { pressed: true, .. })),
        "{:?}",
        h.state().left
    );
}

#[test]
fn text_without_a_key_is_typed_too() {
    let s = scene();
    let mut h = harness(&s);
    // A key of a layout egui has no name for: only its text arrives.
    pass(&mut h, [Event::Text("é".into())]);
    assert_eq!(text(&s.engine, s.one, "text"), "helloé");
    let stats = h.state().view.stats();
    assert_eq!((stats.text_events, stats.texts_accepted), (1, 1));
    assert!(h.state().left.is_empty(), "{:?}", h.state().left);
    // It comes in the order it was typed between keys.
    pass(
        &mut h,
        [
            down(Key::B),
            Event::Text("b".into()),
            Event::Text("ñ".into()),
            down(Key::C),
            Event::Text("c".into()),
        ],
    );
    assert_eq!(text(&s.engine, s.one, "text"), "helloébñc");
}

#[test]
fn text_nobody_takes_stays_in_egui_s_input() {
    let s = scene();
    set(&s.engine, s.one, "focus", Value::Bool(false));
    let mut h = harness(&s);
    pass(&mut h, [Event::Text("é".into()), preedit("に")]);
    assert_eq!(text(&s.engine, s.one, "text"), "hello");
    let stats = h.state().view.stats();
    assert_eq!((stats.text_events, stats.texts_accepted), (2, 0));
    assert_eq!(h.state().left.len(), 2);
}

#[test]
fn an_input_method_composes_and_commits() {
    let s = scene();
    let mut h = harness(&s);
    pass(&mut h, [preedit("に")]);
    pass(&mut h, [preedit("にほ")]);
    assert_eq!(text(&s.engine, s.one, "preeditText"), "にほ");
    assert!(
        s.engine
            .read_by_name(s.one, "inputMethodComposing")
            .to_boolean()
    );
    // What is being composed is shown, and is not the text yet.
    assert_eq!(text(&s.engine, s.one, "text"), "hello");
    assert_eq!(text(&s.engine, s.one, "displayText"), "helloにほ");

    pass(
        &mut h,
        [
            preedit(""),
            Event::Ime(ImeEvent::Commit("日本".into())),
            preedit(""),
        ],
    );
    assert_eq!(text(&s.engine, s.one, "text"), "hello日本");
    assert_eq!(text(&s.engine, s.one, "preeditText"), "");
    assert!(
        !s.engine
            .read_by_name(s.one, "inputMethodComposing")
            .to_boolean()
    );
    assert!(h.state().left.is_empty(), "{:?}", h.state().left);
}

#[test]
fn an_input_method_takes_back_characters_around_the_cursor() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.one, "text", Value::string("a𝄞bcd"));
    set(&s.engine, s.one, "cursorPosition", Value::Int(4));
    h.run();
    // Characters, where the editor counts UTF-16 units: the clef is two.
    pass(
        &mut h,
        [Event::Ime(ImeEvent::DeleteSurrounding {
            before_chars: 2,
            after_chars: 1,
        })],
    );
    assert_eq!(text(&s.engine, s.one, "text"), "ad");
}

#[test]
fn the_platform_is_told_where_the_cursor_is() {
    let s = scene();
    let mut h = harness(&s);
    let rect = h.state().rect;
    let ime = h.output().platform_output.ime.expect("an editor has focus");
    assert_eq!(ime.purpose, IMEPurpose::Normal);
    assert!(h.state().view.text_input_active());
    // After "hello", inside the field at (10, 10).
    let c = ime.cursor_rect;
    assert!(
        c.min.x > rect.min.x + 20.0 && c.max.x < rect.min.x + 160.0,
        "{c:?}"
    );
    assert!(
        c.min.y >= rect.min.y + 10.0 && c.max.y <= rect.min.y + 30.0,
        "{c:?}"
    );
    assert!(c.height() > 8.0, "{c:?}");

    // It follows the cursor.
    pass(&mut h, [down(Key::Home)]);
    let home = h.output().platform_output.ime.unwrap().cursor_rect;
    assert!((home.min.x - (rect.min.x + 10.0)).abs() < 1.5, "{home:?}");

    // It is said in every pass: egui forgets it between them.
    h.run_steps(3);
    assert!(h.output().platform_output.ime.is_some());
}

#[test]
fn a_field_that_hides_its_text_tells_the_input_method_so() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.secret, "focus", Value::Bool(true));
    h.run();
    let ime = h.output().platform_output.ime.unwrap();
    assert_eq!(ime.purpose, IMEPurpose::Password);
    // The view has settled: the composition was interrupted in the pass
    // the keyboard came, and is not interrupted again in every pass after.
    assert!(!ime.should_interrupt_composition);
}

#[test]
fn moving_between_editors_interrupts_the_composition() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.notes, "focus", Value::Bool(true));
    h.step();
    let ime = h.output().platform_output.ime.unwrap();
    assert!(ime.should_interrupt_composition);
    h.step();
    assert!(
        !h.output()
            .platform_output
            .ime
            .unwrap()
            .should_interrupt_composition
    );
}

#[test]
fn no_input_method_without_an_editor_that_takes_text() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.one, "readOnly", Value::Bool(true));
    h.run();
    assert!(h.output().platform_output.ime.is_none());
    assert!(!h.state().view.text_input_active());
    set(&s.engine, s.one, "readOnly", Value::Bool(false));
    h.run();
    assert!(h.output().platform_output.ime.is_some());
    set(&s.engine, s.one, "focus", Value::Bool(false));
    h.run();
    assert!(h.output().platform_output.ime.is_none());
}

#[test]
fn copy_and_cut_go_to_the_platform_and_paste_comes_from_it() {
    let s = scene();
    let mut h = harness(&s);
    s.engine.invoke_by_name(s.one, "selectAll", &[]).unwrap();
    pass(&mut h, [Event::Copy]);
    assert_eq!(copied(&h), ["hello"]);
    assert_eq!(text(&s.engine, s.one, "text"), "hello");
    // Said once.
    h.step();
    assert!(copied(&h).is_empty());

    // The platform read its clipboard; that is what is pasted.
    pass(&mut h, [Event::Paste("from elsewhere".into())]);
    assert_eq!(text(&s.engine, s.one, "text"), "from elsewhere");

    s.engine
        .invoke_by_name(s.one, "select", &[Value::Int(0), Value::Int(5)])
        .unwrap();
    pass(&mut h, [Event::Cut]);
    assert_eq!(copied(&h), ["from "]);
    assert_eq!(text(&s.engine, s.one, "text"), "elsewhere");
    assert!(h.state().left.is_empty(), "{:?}", h.state().left);
}

#[test]
fn a_copy_made_by_a_script_reaches_the_platform_too() {
    let s = scene();
    let mut h = harness(&s);
    s.engine.invoke_by_name(s.one, "selectAll", &[]).unwrap();
    s.engine.invoke_by_name(s.one, "copy", &[]).unwrap();
    h.step();
    assert_eq!(copied(&h), ["hello"]);
}

#[test]
fn a_hidden_text_is_not_copied() {
    let s = scene();
    let mut h = harness(&s);
    set(&s.engine, s.secret, "focus", Value::Bool(true));
    h.run();
    s.engine.invoke_by_name(s.secret, "selectAll", &[]).unwrap();
    pass(&mut h, [Event::Copy]);
    assert!(copied(&h).is_empty());
}

#[test]
fn tab_types_into_a_text_edit_and_leaves_a_field() {
    let s = scene();
    let mut h = harness(&s);
    pass(&mut h, [down(Key::Tab)]);
    assert_eq!(text(&s.engine, s.one, "text"), "hello");

    set(&s.engine, s.notes, "focus", Value::Bool(true));
    h.run();
    pass(&mut h, [down(Key::Tab)]);
    assert_eq!(text(&s.engine, s.notes, "text"), "\t");
    assert!(s.engine.read_by_name(s.notes, "activeFocus").to_boolean());
}

#[test]
fn the_cursor_wakes_the_view_once_per_blink() {
    let s = scene();
    let clock = Arc::clone(&s.clock);
    let mut h = harness(&s);
    let frames = h.state().view.stats().frames;
    // Between blinks nothing runs, however many passes egui makes.
    h.run_steps(5);
    assert_eq!(h.state().view.stats().frames, frames);
    assert!(s.engine.read_by_name(s.one, "cursorVisible").to_boolean());
    for blink in 1..=3 {
        // A little over the half second of a blink: the engine's timers
        // fire on its own coarse ticks.
        clock.advance_ms(510);
        h.run_steps(3);
        assert_eq!(h.state().view.stats().frames, frames + blink);
    }

    // Without focus there is no cursor and nothing to wake for.
    set(&s.engine, s.one, "focus", Value::Bool(false));
    h.run_steps(2);
    let quiet = h.state().view.stats();
    clock.advance_ms(5000);
    h.run_steps(5);
    let after = h.state().view.stats();
    assert_eq!(after.frames, quiet.frames);
    assert_eq!(after.repaint_requests, quiet.repaint_requests);
}

#[test]
fn an_editor_s_text_is_read_with_its_selection() {
    let s = scene();
    s.engine
        .invoke_by_name(s.one, "select", &[Value::Int(1), Value::Int(3)])
        .unwrap();
    let h = harness(&s);
    let field = h.get_by_role_and_label(Role::TextInput, "Name");
    let node = field.accesskit_node();
    assert_eq!(node.value().as_deref(), Some("hello"));
    assert!(node.supports_text_ranges());
    assert_eq!(
        node.text_selection().map(|r| r.text()).as_deref(),
        Some("el")
    );
    assert_eq!(node.document_range().text(), "hello");

    // The runs are not items: they are not counted as the view's nodes.
    assert_eq!(h.state().view.stats().accessible_nodes, 3);

    let notes = h.get_by_role_and_label(Role::MultilineTextInput, "Notes");
    assert!(notes.accesskit_node().supports_text_ranges());
}

#[test]
fn a_hidden_text_is_never_read() {
    let s = scene();
    let h = harness(&s);
    let field = h.get_by_role_and_label(Role::PasswordInput, "Secret");
    let node = field.accesskit_node();
    let shown = node.document_range().text();
    assert_eq!(shown.chars().count(), 7);
    assert!(!shown.contains("hunter2"));
    assert_ne!(node.value().as_deref(), Some("hunter2"));
}

#[test]
fn a_screen_reader_selects_and_replaces_text() {
    let s = scene();
    let mut h = harness(&s);
    let field = h.get_by_role_and_label(Role::TextInput, "Name");
    let node = field.accesskit_node();
    let target = node.locate().0;
    let run = node.children().next().expect("a text run").locate().0;
    let at = |character_index| TextPosition {
        node: run,
        character_index,
    };
    pass(
        &mut h,
        [Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor: at(4),
                focus: at(1),
            })),
        })],
    );
    assert_eq!(text(&s.engine, s.one, "selectedText"), "ell");
    assert_eq!(
        s.engine.read_by_name(s.one, "cursorPosition").to_number(),
        1.0
    );

    pass(
        &mut h,
        [Event::AccessKitActionRequest(ActionRequest {
            action: Action::ReplaceSelectedText,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: Some(ActionData::Value("ipp".into())),
        })],
    );
    assert_eq!(text(&s.engine, s.one, "text"), "hippo");

    pass(
        &mut h,
        [Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: Some(ActionData::Value("all new".into())),
        })],
    );
    assert_eq!(text(&s.engine, s.one, "text"), "all new");
    assert_eq!(h.state().view.stats().accessible_actions, 3);
    // What is read has followed.
    h.run();
    let field = h.get_by_role_and_label(Role::TextInput, "Name");
    assert_eq!(field.accesskit_node().document_range().text(), "all new");
}

#[test]
fn a_run_of_another_editor_names_no_place() {
    let s = scene();
    let mut h = harness(&s);
    let target = h
        .get_by_role_and_label(Role::TextInput, "Name")
        .accesskit_node()
        .locate()
        .0;
    let other = h
        .get_by_role_and_label(Role::PasswordInput, "Secret")
        .accesskit_node()
        .children()
        .next()
        .unwrap()
        .locate()
        .0;
    let at = |character_index| TextPosition {
        node: other,
        character_index,
    };
    pass(
        &mut h,
        [Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor: at(0),
                focus: at(2),
            })),
        })],
    );
    assert_eq!(text(&s.engine, s.one, "selectedText"), "");
    assert_eq!(h.state().view.stats().accessible_actions, 0);
}

/// A line, and a second one given later: lower case fits a 256² glyph
/// atlas beside the first line alone; the first line's own letters fit
/// in any case.
const TWO_LINES: &str = r#"
import QtQuick
Item {
    objectName: "root"
    width: 500; height: 200
    property string second
    Text { font.pixelSize: 64; text: "ABCDEFGHIJ" }
    Text { y: 100; font.pixelSize: 64; text: parent.second }
}
"#;

/// The repaints a settled view asks for once its second line is
/// `second`, on a GPU that takes textures no wider than 256, and whether
/// it then stays idle.
fn repaints_for_second_line(second: &str) -> (u64, bool) {
    use ljos_compiler::{Project, compile_file};
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", TWO_LINES);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = QmlView::new(Rc::clone(&e), root).unwrap();
    // Building the harness runs passes until egui asks for no more.
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 200.0))
        .build_ui_state(
            |ui, view: &mut QmlView| {
                ljos_render::b0::set_max_texture_side(ui.ctx(), 256);
                view.show(ui);
            },
            view,
        );
    let before = h.state().stats().repaint_requests;
    set(&e, root, "second", Value::string(second));
    h.run();
    let settled = h.state().stats().repaint_requests;
    h.run_steps(3);
    let idle = h.state().stats().repaint_requests == settled;
    (settled - before, idle)
}

/// Text drawn without glyphs a full atlas had no room for asks for one
/// more pass, in case it finds room; while the atlas stays full it asks
/// no more, so the window goes idle.
#[test]
fn text_that_misses_glyphs_asks_for_one_more_pass() {
    let (crowded, idle) = repaints_for_second_line("abcdefghij");
    assert!(idle, "a full atlas keeps the view drawing");
    let (roomy, idle) = repaints_for_second_line("ABCDEFGHIJ");
    assert!(idle);
    assert_eq!(crowded, roomy + 1);
}
