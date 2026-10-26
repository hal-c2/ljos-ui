//! What the scenarios in `features/controls/` cannot see: that an idle
//! scene of controls schedules nothing, that the parts a style has and
//! the application replaced are never made, and what native code reads.

use ljos_a11y::Role;
use ljos_engine::{Engine, MetaRef, ObjectId, QmlType, Value};
use ljos_quick::{Accessible, PointerEvent, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::Time;

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e
}

/// Compiles `source` as `Main.qml` and creates it.
fn load(e: &Engine, source: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 300.0));
    s.set_root(e, root).unwrap();
    s
}

/// Frames until the scene asks for no more, and returns its next wake.
fn settle(s: &mut QuickScene, e: &Engine, now: Time) -> Option<Time> {
    ljos_drive::settle_at(e, s, now).unwrap()
}

const ALL: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    property int clicks: 0
    Column {
        Button { objectName: "button"; text: "Go"; onClicked: clicks++ }
        CheckBox { objectName: "check"; text: "Check" }
        RadioButton { text: "Radio" }
        Switch { text: "Switch" }
        ToolBar { ToolButton { text: "Tool" } }
        GroupBox { title: "Group"; Label { text: "Label" } }
        Frame { Pane { Label { text: "Deep" } } }
        Page { header: Label { text: "Head" } }
    }
}
"#;

#[test]
fn registering_twice_is_registering_once() {
    let e = engine();
    ljos_controls::register(&e);
    let root = load(&e, ALL);
    let width = e.read_by_name(find(&e, root, "button"), "width");
    assert_eq!(width.to_number(), 100.0);
}

#[test]
fn every_style_file_is_a_type_of_both_imports() {
    for import in ["QtQuick.Controls", "QtQuick.Controls.Basic"] {
        for ty in [
            "ApplicationWindow",
            "Button",
            "CheckBox",
            "Control",
            "Frame",
            "GroupBox",
            "Label",
            "Page",
            "Pane",
            "RadioButton",
            "Switch",
            "ToolBar",
            "ToolButton",
        ] {
            let e = engine();
            let root = load(
                &e,
                &format!(
                    "import QtQuick\nimport {import}\nItem {{ {ty} {{ objectName: \"x\" }} }}"
                ),
            );
            assert!(e.is_alive(find(&e, root, "x")), "{import} {ty}");
        }
    }
}

#[test]
fn an_idle_scene_of_controls_schedules_nothing() {
    let e = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    assert_eq!(e.next_timer_deadline(), None);
    // Nothing changed, so the next frame is the same one.
    assert!(!s.frame(&e, Time::ZERO).changed);
}

const HELD: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    property int clicks: 0
    property int holds: 0
    Button { objectName: "plain"; onClicked: clicks++ }
    Button { objectName: "listens"; y: 100; onPressAndHold: holds++ }
}
"#;

#[test]
fn a_held_button_nobody_listens_to_waits_on_nothing() {
    let e = engine();
    let root = load(&e, HELD);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);

    s.pointer_event(&e, &PointerEvent::press(20.0, 20.0, Time::ZERO));
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    let plain = find(&e, root, "plain");
    assert_eq!(e.read_by_name(plain, "pressed"), Value::Bool(true));
    // No handler of `pressAndHold`, so no deadline to wake up for.
    assert_eq!(e.next_timer_deadline(), None);

    s.pointer_event(&e, &PointerEvent::release(20.0, 20.0, Time::ZERO));
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    assert_eq!(e.read_by_name(root, "clicks"), Value::Int(1));
}

#[test]
fn a_held_button_somebody_listens_to_waits_on_one_deadline() {
    let e = engine();
    let root = load(&e, HELD);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);

    s.pointer_event(&e, &PointerEvent::press(20.0, 120.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    let listens = find(&e, root, "listens");
    assert_eq!(e.read_by_name(listens, "pressed"), Value::Bool(true));
    // The press-and-hold deadline, and nothing that polls before it.
    let deadline = e.next_timer_deadline().expect("a deadline");
    assert!(deadline > Time::ZERO);

    s.pointer_event(&e, &PointerEvent::release(20.0, 120.0, Time::ZERO));
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    assert_eq!(e.next_timer_deadline(), None);
    assert_eq!(e.read_by_name(root, "holds"), Value::Int(0));
}

/// Every object under `root`, itself included.
fn subtree(e: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut all = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        all.push(o);
        stack.extend(e.children(o));
    }
    all
}

/// The items under `root` that a file of the style declared: the parts
/// the style made.
fn made_by_the_style(e: &Engine, root: ObjectId) -> usize {
    subtree(e, root)
        .into_iter()
        .filter(|o| *o != root && ljos_quick::is_item(e, *o))
        .filter(|o| {
            e.declared_at(*o)
                .is_some_and(|at| at.starts_with("ljos-controls:/"))
        })
        .count()
}

#[test]
fn a_replaced_part_of_the_style_is_never_made() {
    let e = engine();
    let before = e.object_count();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    Button { objectName: "styled" }
    Button {
        objectName: "own"
        background: Rectangle { objectName: "bg" }
        contentItem: Text { objectName: "label" }
    }
}
"#,
    );
    let (styled, own) = (find(&e, root, "styled"), find(&e, root, "own"));
    // The style's button is a background and a content item; the picture
    // and the text in it are the content item's own doing.
    assert_eq!(made_by_the_style(&e, styled), 2);
    // Counting what was made: nothing of the style's is under the button
    // that has its own parts, and nothing was made to be destroyed at the
    // end of the turn, as a deferred `destroy` is.
    assert_eq!(made_by_the_style(&e, own), 0);
    let made = e.object_count() - before;
    assert_eq!(e.process_deferred_deletes(), 0);
    assert_eq!(e.object_count() - before, made);
    assert_eq!(e.visual_children(styled).len(), 2);
    let kids = e.visual_children(own);
    assert_eq!(kids.len(), 2, "{kids:?}");
    assert!(kids.contains(&find(&e, root, "bg")));
    assert!(kids.contains(&find(&e, root, "label")));
}

/// Each part of each control, replaced: none of the style's is made.
#[test]
fn no_part_of_the_style_is_made_where_the_application_gives_its_own() {
    let cases = [
        ("CheckBox", "indicator: Item {} contentItem: Item {}"),
        ("RadioButton", "indicator: Item {} contentItem: Item {}"),
        ("Switch", "indicator: Item {} contentItem: Item {}"),
        ("ToolButton", "background: Item {} contentItem: Item {}"),
        ("Pane", "background: Item {}"),
        ("Frame", "background: Item {}"),
        ("GroupBox", "background: Item {} label: Item {}"),
        ("ToolBar", "background: Item {}"),
        ("Page", "background: Item {}"),
        ("TextField", "background: Item {}"),
        ("TextArea", "background: Item {}"),
        ("ItemDelegate", "background: Item {} contentItem: Item {}"),
        ("ScrollBar", "contentItem: Item {}"),
        ("ScrollIndicator", "contentItem: Item {}"),
        (
            "SpinBox",
            "background: Item {} contentItem: TextInput {} up.indicator: Item {} \
             down.indicator: Item {}",
        ),
        (
            "ComboBox",
            "background: Item {} contentItem: Item {} indicator: Item {} popup: Popup {}",
        ),
    ];
    for (ty, parts) in cases {
        let e = engine();
        let parts = parts.replace("; ", "\n");
        let root = load(
            &e,
            &format!(
                "import QtQuick\nimport QtQuick.Controls.Basic\n\
                 Item {{ {ty} {{\nobjectName: \"own\"\n{parts}\n}} }}"
            ),
        );
        let own = find(&e, root, "own");
        assert_eq!(e.process_deferred_deletes(), 0, "{ty}");
        let kept = match ty {
            // The popup the application gave is the style's `Popup` type:
            // its background is that popup's own.
            "ComboBox" => continue,
            // The placeholder is no part: it is the style's text in the
            // field, whatever the background.
            "TextField" | "TextArea" => 1,
            _ => 0,
        };
        assert_eq!(made_by_the_style(&e, own), kept, "{ty}");
    }
}

#[test]
fn a_control_made_by_native_code_has_no_parts_and_no_size() {
    let e = engine();
    let c = e
        .create_native(ljos_controls::Control::default(), None)
        .unwrap();
    e.complete(c).unwrap();
    assert_eq!(e.read_by_name(c, "background"), Value::Null);
    assert_eq!(e.read_by_name(c, "contentItem"), Value::Null);
    assert_eq!(e.read_by_name(c, "implicitWidth").to_number(), 0.0);
    e.write_by_name(c, "padding", Value::from(7.0)).unwrap();
    assert_eq!(e.read_by_name(c, "leftPadding").to_number(), 7.0);
}

#[test]
fn the_default_role_is_on_the_attached_object_and_the_author_can_replace_it() {
    let e = engine();
    let root = load(&e, ALL);
    let check = find(&e, root, "check");
    let att = e
        .attached_object(check, &MetaRef::Static(Accessible::static_meta()))
        .unwrap();
    assert_eq!(
        e.read_by_name(att, "role"),
        Value::Int(Role::CheckBox.qt_value())
    );
    assert_eq!(e.read_by_name(att, "name").to_js_string().as_str(), "Check");
    e.write_by_name(att, "name", Value::string("Spoken"))
        .unwrap();
    e.write_by_name(check, "text", Value::string("Other"))
        .unwrap();
    assert_eq!(
        e.read_by_name(att, "name").to_js_string().as_str(),
        "Spoken"
    );
}

#[test]
fn destroying_a_pressed_button_leaves_nothing_behind() {
    let e = engine();
    let root = load(&e, ALL);
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let button = find(&e, root, "button");
    s.pointer_event(&e, &PointerEvent::press(20.0, 20.0, Time::ZERO));
    settle(&mut s, &e, Time::ZERO);
    e.destroy(button);
    // Its timer went with it, and the release finds nobody.
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    assert_eq!(e.next_timer_deadline(), None);
    s.pointer_event(&e, &PointerEvent::release(20.0, 20.0, Time::ZERO));
    assert_eq!(settle(&mut s, &e, Time::ZERO), None);
    assert_eq!(e.read_by_name(root, "clicks"), Value::Int(0));
}

/// A change of the application font writes each font that inherits it
/// once, the deepest too, and leaves alone a control whose font is all its
/// own.
#[test]
fn a_new_application_font_is_written_once_to_each_font_that_inherits_it() {
    use std::cell::Cell;
    use std::rc::Rc;
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
import QtQuick.Controls.Basic
Item {
    Pane {
        objectName: "outer"
        Pane {
            objectName: "inner"
            Label { objectName: "label"; text: "x" }
        }
    }
    Label { objectName: "own"; text: "x"; font.family: "Serif"; font.pixelSize: 30 }
}"#,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let control = ljos_controls::Control::FONT.index;
    let text = ljos_quick::Text::FONT.index;
    let count = |name: &str, prop| {
        let n = Rc::new(Cell::new(0));
        let seen = n.clone();
        e.watch(find(&e, root, name), prop, move |_, _| {
            seen.set(seen.get() + 1)
        })
        .unwrap();
        n
    };
    let writes = [
        count("outer", control),
        count("inner", control),
        count("label", text),
    ];
    let own = count("own", text);
    for size in [20, 24] {
        let font = ljos_engine::Font {
            pixel_size: size,
            point_size: -1.0,
            ..ljos_engine::Font::default()
        };
        ljos_controls::set_application_font(&e, Some(&font));
        settle(&mut s, &e, Time::ZERO);
        let label = e.read(find(&e, root, "label"), text);
        let Value::Font(label) = label else {
            panic!("{label:?}")
        };
        assert_eq!(label.pixel_size, size);
    }
    assert_eq!(writes.map(|n| n.get()), [2, 2, 2]);
    assert_eq!(own.get(), 0);
}

/// Makes `controls` Buttons with a font, palette and locale of their own
/// and `paths` icons drawn by `Shape`s between them, in one creation, and
/// shows them. What it cost: the effects run, how often an icon's path
/// was made again, and what was said about runs in one update.
fn controls_and_icons(controls: usize, paths: usize) -> (u64, Value, Vec<String>) {
    let button = |i: usize| {
        format!(
            r#"
    Button {{
        font.pixelSize: {}
        locale: Qt.locale("de_DE")
        palette.button: "red"
        text: "Go"
    }}"#,
            10 + i % 7
        )
    };
    let icon = r#"
    Shape {
        width: 16; height: 16
        ShapePath {
            onChanged: rebuilt++
            strokeColor: "black"
            PathLine { x: 16; y: 16 }
            PathLine { x: 0; y: 16 }
        }
    }"#;
    let items: String = (0..controls.max(paths))
        .flat_map(|i| {
            [
                (i < controls).then(|| button(i)),
                (i < paths).then(|| icon.to_owned()),
            ]
        })
        .flatten()
        .collect();
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Shapes
import QtQuick.Controls.Basic
Column {{
    property int rebuilt: 0{items}
}}
"#
        ),
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let reran = e
        .take_log()
        .into_iter()
        .filter(|l| l.message.contains("re-ran"))
        .map(|l| l.message)
        .collect();
    (
        e.graph().stats().effect_runs,
        e.read_by_name(root, "rebuilt"),
        reran,
    )
}

/// Each control with a font, palette or locale of its own moves what the
/// controls inherit along. Moved once for each as they were made, every
/// control made before looked again, and so did what was drawn from
/// them: a creation of many cost many times many. Moved once the
/// creation is complete, twice the controls cost twice as much, and the
/// icons' paths are made once.
#[test]
fn controls_with_fonts_made_together_cost_what_each_costs() {
    let (once, rebuilt, reran) = controls_and_icons(40, 40);
    assert_eq!(rebuilt, Value::Int(0));
    assert!(reran.is_empty(), "{reran:?}");
    let (twice, rebuilt, reran) = controls_and_icons(80, 80);
    assert_eq!(rebuilt, Value::Int(0));
    assert!(reran.is_empty(), "{reran:?}");
    assert!(
        twice * 10 < once * 22,
        "twice the controls ran {twice} effects, against {once}"
    );
}

/// The objects one more `control` of `style` makes: what a file of two
/// makes less what a file of one does, the root and the rest the same.
fn objects_of(style: &str, control: &str) -> usize {
    let e = engine();
    let made = |n: usize| {
        let all = format!("{control} {{}} ").repeat(n);
        let before = e.object_count();
        load(
            &e,
            &format!("import QtQuick\nimport QtQuick.Controls.{style}\nItem {{ {all}}}\n"),
        );
        e.object_count() - before
    };
    let one = made(1);
    made(2) - one
}

#[test]
fn fusion_makes_no_more_objects_than_it_draws() {
    // Every object costs memory and time to make, and Fusion draws in
    // gradients, each of which is three. What it holds back: the stripes
    // of a bar that knows how far it has come, and a path element per
    // line of an arrow or a tick. A knob's outline is an item of its own,
    // as on qml6. A dial is a shape lit by a radial gradient of four stops
    // over two shadow rings.
    for (control, most) in [
        ("Button", 7),
        ("CheckBox", 13),
        ("RadioButton", 6),
        ("Switch", 17),
        ("Slider", 14),
        ("ComboBox", 34),
        ("SpinBox", 21),
        ("TextField", 4),
        ("ProgressBar", 12),
        ("BusyIndicator", 12),
        ("GroupBox", 3),
        ("PageIndicator", 2),
        ("RangeSlider", 22),
        ("DelayButton", 15),
        ("Dial", 17),
        ("Tumbler", 10),
        ("ItemDelegate", 3),
        ("CheckDelegate", 13),
        ("RadioDelegate", 6),
        ("SwitchDelegate", 17),
    ] {
        let made = objects_of("Fusion", control);
        assert!(
            made <= most,
            "a Fusion {control} makes {made} objects, more than {most}"
        );
    }
}

/// Makes `rows` Material buttons one at a time, as a view does as it
/// scrolls, below an item that gives them an accent, and returns the
/// effects that ran.
fn material_rows(rows: usize) -> u64 {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Material
Column {
    id: column
    Material.accent: Material.Red
    property int rows: 0
    onRowsChanged: row.createObject(column)
    Component {
        id: row
        Button {}
    }
}
"#,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    for n in 1..=rows {
        e.write_by_name(root, "rows", Value::Int(n as i32)).unwrap();
        settle(&mut s, &e, Time::ZERO);
    }
    e.graph().stats().effect_runs
}

/// Makes `rows` Material buttons one at a time, each giving itself a
/// background, and returns the effects that ran.
fn material_rows_given(rows: usize) -> u64 {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Material
Column {
    id: column
    Material.accent: Material.Red
    property int rows: 0
    onRowsChanged: row.createObject(column)
    Component {
        id: row
        Button { Material.background: Material.Green }
    }
}
"#,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    for n in 1..=rows {
        e.write_by_name(root, "rows", Value::Int(n as i32)).unwrap();
        settle(&mut s, &e, Time::ZERO);
    }
    e.graph().stats().effect_runs
}

/// A Material value an author gave an item made every Material object
/// look again, so delegates that each give one cost many times many.
/// Only what was made below the item before it looks again.
#[test]
fn material_rows_that_each_give_a_value_cost_what_each_costs() {
    let once = material_rows_given(40);
    let twice = material_rows_given(80);
    assert!(
        twice * 10 < once * 22,
        "twice the rows ran {twice} effects, against {once}"
    );
}

/// Graph nodes per object made by a Repeater of `decl`, once settled.
fn nodes_each(decl: &str) -> f64 {
    let e = engine();
    let n0 = e.node_count();
    let root = load(
        &e,
        &format!(
            "import QtQuick\nimport QtQuick.Controls.Basic\n\
             Item {{ width: 400; height: 300; Repeater {{ model: 50; delegate: {decl} }} }}"
        ),
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    (e.node_count() - n0) as f64 / 50.0
}

/// A container hears a colour set on it later through the colour's
/// setter, not through a graph node per colour. A Pane holding an item
/// cost 86 nodes more than a bare Control holding one while it watched
/// each of its 21 colours, and 44 more without.
#[test]
fn a_container_has_no_node_per_colour() {
    let pane = nodes_each("Pane { Item {} }");
    let control = nodes_each("Control { contentItem: Item {} }");
    assert!(pane - control < 50.0, "Pane {pane}, Control {control}");
}

/// A button makes its mnemonic's `Shortcut` with the first text that has
/// one ([`features/controls/mnemonics.feature`]); one without costs
/// nothing for it.
#[test]
fn a_button_without_a_mnemonic_has_no_shortcut() {
    let plain = nodes_each(r#"Button { text: "File" }"#);
    let marked = nodes_each(r#"Button { text: "&File" }"#);
    assert!(marked - plain >= 3.0, "plain {plain}, marked {marked}");
}

/// An action makes its `Shortcut` with the first sequence it is given
/// (`features/controls/action-shortcut.feature`); a blank or null one is
/// none.
#[test]
fn an_action_without_a_sequence_has_no_shortcut() {
    let none = nodes_each("Item { Action {} }");
    let keyed = nodes_each(r#"Item { Action { shortcut: "Ctrl+K" } }"#);
    assert!(keyed - none >= 3.0, "none {none}, keyed {keyed}");
    for decl in [
        r#"Item { Action { shortcut: "" } }"#,
        "Item { Action { shortcut: null } }",
    ] {
        let blank = nodes_each(decl);
        assert!(blank - none < 1.0, "{decl}: {blank}, none {none}");
    }
}

/// The letters of a menu's rows are the menu's: a row with a mnemonic
/// makes no `Shortcut` of its own. Its marker costs what it costs a row
/// outside a menu, less the `Shortcut` (2 nodes against 6).
#[test]
fn a_row_of_a_menu_has_no_mnemonic_shortcut() {
    let mark = |menu: bool| {
        let row = |text: &str| {
            let item = format!(r#"MenuItem {{ text: "{text}" }}"#);
            if menu {
                nodes_each(&format!("Item {{ Menu {{ {item} }} }}"))
            } else {
                nodes_each(&format!("Item {{ {item} }}"))
            }
        };
        row("&Open") - row("Open")
    };
    let (inside, outside) = (mark(true), mark(false));
    assert!(
        inside + 3.0 <= outside,
        "in a menu {inside}, outside {outside}"
    );
}

/// Makes `rows` buttons one at a time, each with a colour of its own, as
/// a view does as it scrolls, and returns the effects that ran.
fn coloured_rows(rows: usize) -> u64 {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
import QtQuick.Controls.Basic
Column {
    id: column
    property int rows: 0
    onRowsChanged: row.createObject(column)
    Component {
        id: row
        Button { palette.buttonText: "red" }
    }
}
"#,
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    for n in 1..=rows {
        e.write_by_name(root, "rows", Value::Int(n as i32)).unwrap();
        settle(&mut s, &e, Time::ZERO);
    }
    e.graph().stats().effect_runs
}

/// A control made with a colour of its own had every control made before
/// it look again, so a page of buttons that each set one cost many times
/// many. What it sets is read by the objects made with it as they
/// resolve; only the first colour set anywhere tells everything to look.
#[test]
fn controls_made_one_by_one_with_colours_cost_what_each_costs() {
    let once = coloured_rows(40);
    let twice = coloured_rows(80);
    assert!(
        twice * 10 < once * 22,
        "twice the rows ran {twice} effects, against {once}"
    );
}

/// Each Material attached object made had every other one made before it
/// look again: opening a dialog of many delegates cost many times many.
/// What a new one is given reaches the rest by the writes that give it,
/// so twice the rows cost twice as much.
#[test]
fn material_items_made_one_by_one_cost_what_each_costs() {
    let once = material_rows(40);
    let twice = material_rows(80);
    assert!(
        twice * 10 < once * 22,
        "twice the rows ran {twice} effects, against {once}"
    );
}

/// Switches the colours bound on a pane of four buttons, beside `others`
/// buttons that set none, and returns the effects the switch ran and what
/// a button in the pane and a label in it read after.
fn switched_beside(others: usize) -> (u64, Value, Value) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
Column {{
    property bool dark: false
    Pane {{
        palette.button: dark ? "black" : "white"
        palette.text: dark ? "white" : "black"
        Row {{
            Repeater {{ model: 4; Button {{}} }}
            Button {{ objectName: "inside" }}
            Item {{ objectName: "plain" }}
        }}
    }}
    Repeater {{ model: {others}; Button {{}} }}
}}
"#
        ),
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let before = e.graph().stats().effect_runs;
    e.write_by_name(root, "dark", Value::Bool(true)).unwrap();
    settle(&mut s, &e, Time::ZERO);
    let ran = e.graph().stats().effect_runs - before;
    let inside = find(&e, root, "inside");
    let plain = find(&e, root, "plain");
    let button = e.read_by_name(inside, "palette.button");
    let text = e.read_by_name(plain, "palette.text");
    (ran, button, text)
}

/// A colour bound on a control changed its value, and every control in
/// the scene resolved its palette again: a theme bound on a menu bar cost
/// each switch milliseconds. The controls that took the colour read it as
/// they resolved and run again on their own; only a colour that was not
/// the author's before is news to the rest.
#[test]
fn a_bound_colour_that_changes_costs_what_is_below_it() {
    let (few, button, text) = switched_beside(10);
    let (many, ..) = switched_beside(80);
    assert_eq!(button, Value::Color(ljos_engine::Color::BLACK));
    assert_eq!(text, Value::Color(ljos_engine::Color::WHITE));
    assert!(
        many < few + 20,
        "beside 80 buttons the switch ran {many} effects, beside 10 {few}"
    );
}

/// Switches the colours bound on `menus` menus of four items each, once,
/// and returns the effects and the reads of the switch, and what an item
/// of a menu and the menu itself read after.
fn menus_switched(menus: usize) -> (u64, u64, Value, Value) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Basic
Item {{
    property bool dark: false
    Repeater {{
        model: {menus}
        Item {{
            Menu {{
                objectName: "menu"
                palette.base: dark ? "black" : "white"
                palette.text: dark ? "white" : "black"
                palette.highlight: dark ? "red" : "blue"
                Repeater {{ model: 4; MenuItem {{ objectName: "entry" }} }}
            }}
        }}
    }}
}}
"#
        ),
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let before = e.graph().stats();
    e.write_by_name(root, "dark", Value::Bool(true)).unwrap();
    settle(&mut s, &e, Time::ZERO);
    let after = e.graph().stats();
    let entry = find(&e, root, "entry");
    let menu = find(&e, root, "menu");
    (
        after.effect_runs - before.effect_runs,
        after.reads - before.reads,
        e.read_by_name(entry, "palette.text"),
        e.read_by_name(menu, "palette.inactive.base"),
    )
}

/// A menu handed each colour bound on it to its popup's item on its own,
/// and read all of the item's colours back for each: the controls in it
/// resolved their palettes once for each colour, and the first switch
/// had every menu tell the whole scene to look again. Now the colours go
/// down together and a menu costs what it holds.
#[test]
fn colours_bound_on_menus_cost_what_each_menu_holds() {
    let (few, few_reads, text, base) = menus_switched(4);
    let (many, many_reads, ..) = menus_switched(8);
    assert_eq!(text, Value::Color(ljos_engine::Color::WHITE));
    assert_eq!(base, Value::Color(ljos_engine::Color::BLACK));
    assert!(
        many * 10 < few * 22,
        "8 menus ran {many} effects, 4 ran {few}"
    );
    assert!(
        few_reads < 4 * 300 && many_reads < 8 * 300,
        "4 menus read {few_reads} times, 8 read {many_reads}"
    );
}

/// Changes the font size bound on a pane that holds `fields` text fields,
/// once, and returns the effects that ran and the field's height after.
fn fields_resized(fields: usize) -> (u64, f64) {
    fields_resized_to(fields, 20)
}

fn fields_resized_to(fields: usize, size: i32) -> (u64, f64) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
import QtQuick.Controls.Fusion
Pane {{
    property int size: 12
    font.pixelSize: size
    Column {{ Repeater {{ model: {fields}; TextField {{ objectName: "field" }} }} }}
}}
"#
        ),
    );
    let mut s = scene(&e, root);
    settle(&mut s, &e, Time::ZERO);
    let before = e.graph().stats().effect_runs;
    e.write_by_name(root, "size", Value::Int(size)).unwrap();
    settle(&mut s, &e, Time::ZERO);
    let ran = e.graph().stats().effect_runs - before;
    let height = e
        .read_by_name(find(&e, root, "field"), "height")
        .to_number();
    (ran, height)
}

/// A font a little larger leaves a field as high as its frame: the text
/// it lays out writes sizes the field reads, and they come out as they
/// were, so its text is laid out and placed once.
#[test]
fn a_field_whose_height_stays_with_a_new_font_lays_its_text_out_once() {
    let (one, height) = fields_resized_to(1, 13);
    let (two, _) = fields_resized_to(2, 13);
    assert_eq!(height, 24.0, "the frame gives the height");
    // It ran 7 when the sizes its layout wrote ran the layout again.
    assert!(
        two - one <= 5,
        "a field ran {} effects for a font change",
        two - one
    );
}

/// A field's height follows the font, so its text is placed twice: once
/// in the old height and once in the new. The first placing is not
/// painted, as the second follows at once.
#[test]
fn a_field_whose_height_follows_a_new_font_is_painted_once() {
    let (one, height) = fields_resized(1);
    let (two, _) = fields_resized(2);
    assert!(height > 30.0, "the field grew with its font: {height}");
    assert!(
        two - one <= 10,
        "a field ran {} effects for a font change",
        two - one
    );
}
