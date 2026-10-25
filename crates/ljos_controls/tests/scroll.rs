//! Scroll bars, indicators and views against the scene: what the
//! scenarios in `features/controls/` cannot see.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
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

fn num(e: &Engine, o: ObjectId, name: &str) -> f64 {
    e.read_by_name(o, name).to_number()
}

const ATTACHED: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    Flickable {
        objectName: "f"
        width: 150; height: 100
        contentWidth: 300; contentHeight: 400
        ScrollBar.vertical: ScrollBar { objectName: "v" }
        ScrollBar.horizontal: ScrollBar { objectName: "h"; policy: ScrollBar.AlwaysOn }
    }
}
"#;

#[test]
fn an_attached_bar_shows_the_share_of_the_content_in_view() {
    let e = engine();
    let root = load(&e, ATTACHED);
    let (f, v) = (find(&e, root, "f"), find(&e, root, "v"));
    let mut s = scene(&e, root);
    s.frame(&e, Time::from_millis(0));
    assert_eq!(num(&e, v, "size"), 0.25);
    assert_eq!(num(&e, v, "x"), 140.0);
    assert_eq!(num(&e, v, "height"), 100.0);
    e.write_by_name(f, "contentY", Value::Real(100.0)).unwrap();
    assert_eq!(num(&e, v, "position"), 0.25);
}

const POLICIES: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 300; height: 250
    ScrollView {
        objectName: "view"
        width: 200; height: 100
        ScrollBar.vertical.policy: ScrollBar.AlwaysOn
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        Rectangle { implicitWidth: 500; implicitHeight: 300 }
    }
}
"#;

#[test]
fn what_is_attached_is_written_through_the_object_it_names() {
    let e = engine();
    let root = load(&e, POLICIES);
    assert!(e.take_warnings().is_empty());
    let view = find(&e, root, "view");
    let mut s = scene(&e, root);
    s.frame(&e, Time::from_millis(0));
    assert_eq!(num(&e, view, "effectiveScrollBarWidth"), 10.0);
    assert_eq!(num(&e, view, "effectiveScrollBarHeight"), 0.0);
}

const VIEWS: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    ScrollView {
        objectName: "made"
        width: 200; height: 100
        Accessible.name: "Notes"
        Rectangle { objectName: "page"; implicitWidth: 500; implicitHeight: 300 }
    }
    ScrollView {
        objectName: "given"
        y: 150; width: 200; height: 100
        Flickable { objectName: "own"; contentWidth: 500; contentHeight: 300 }
    }
}
"#;

fn content_item(e: &Engine, view: ObjectId) -> ObjectId {
    e.read_by_name(view, "contentItem").as_object().unwrap()
}

#[test]
fn a_view_makes_a_flickable_or_takes_the_one_declared_in_it() {
    let e = engine();
    let root = load(&e, VIEWS);
    assert!(e.take_warnings().is_empty());
    let (made, given) = (find(&e, root, "made"), find(&e, root, "given"));
    let mut s = scene(&e, root);
    s.frame(&e, Time::from_millis(0));

    let f = content_item(&e, made);
    assert!(ljos_quick::flickable::is_flickable(&e, f));
    assert_eq!(num(&e, f, "contentWidth"), 500.0);
    assert_eq!(num(&e, f, "contentHeight"), 300.0);
    // What was declared in the view is in what the Flickable scrolls.
    let page = find(&e, root, "page");
    assert_eq!(
        e.visual_parent(page),
        ljos_quick::flickable::content_item(&e, f)
    );

    assert_eq!(content_item(&e, given), find(&e, root, "own"));
    assert_eq!(num(&e, given, "contentHeight"), 300.0);
}

#[test]
fn a_view_at_rest_asks_for_no_frame() {
    let e = engine();
    let root = load(&e, VIEWS);
    let mut s = scene(&e, root);
    s.frame(&e, Time::from_millis(0));
    // The bars fade out on a timer; nothing is left to do after it.
    s.frame(&e, Time::from_millis(2_000));
    s.frame(&e, Time::from_millis(4_000));
    assert!(!s.needs_frame());
}

mod assistive {
    use ljos_a11y::accesskit::Role as AkRole;
    use ljos_a11y::{NodeKey, Range, Scroll};
    use ljos_quick::AccessibleAction;

    use super::*;

    const BARS: &str = r#"
import QtQuick
import QtQuick.Controls.Basic
Item {
    width: 400; height: 300
    Flickable {
        objectName: "f"
        width: 150; height: 100
        contentWidth: 300; contentHeight: 400
        ScrollBar.vertical: ScrollBar { objectName: "v"; Accessible.name: "Down" }
        ScrollBar.horizontal: ScrollBar {
            objectName: "h"
            stepSize: 0.25
            Accessible.name: "Across"
        }
        ScrollIndicator.vertical: ScrollIndicator { Accessible.name: "Mark" }
    }
}
"#;

    fn listening(e: &Engine, root: ObjectId) -> QuickScene {
        let mut s = scene(e, root);
        s.set_accessibility_active(e, true);
        s.frame(e, Time::from_millis(0));
        s
    }

    fn node(s: &QuickScene, role: AkRole, name: &str) -> NodeKey {
        s.accessibility_tree()
            .find(role, name)
            .unwrap_or_else(|| panic!("no {role:?} named {name}"))
    }

    fn range(s: &QuickScene, key: NodeKey) -> Range {
        s.accessibility_tree().get(key).unwrap().range.unwrap()
    }

    fn scroll(s: &QuickScene, key: NodeKey) -> Scroll {
        s.accessibility_tree().get(key).unwrap().scroll.unwrap()
    }

    #[test]
    fn a_bar_says_where_it_is_and_how_far_it_can_go() {
        let e = engine();
        let root = load(&e, BARS);
        let s = listening(&e, root);
        let down = range(&s, node(&s, AkRole::ScrollBar, "Down"));
        assert_eq!((down.value, down.min, down.max), (0.0, 0.0, 0.75));
        assert_eq!(down.step, 0.1);
        let across = range(&s, node(&s, AkRole::ScrollBar, "Across"));
        assert_eq!((across.max, across.step), (0.5, 0.25));
    }

    #[test]
    fn a_bar_takes_the_steps_it_can_and_an_indicator_none() {
        let e = engine();
        let root = load(&e, BARS);
        let s = listening(&e, root);
        let tree = s.accessibility_tree();
        let bar = tree
            .accesskit_node(node(&s, AkRole::ScrollBar, "Down"))
            .unwrap();
        use ljos_a11y::accesskit::Action;
        assert_eq!(bar.numeric_value(), Some(0.0));
        assert_eq!(bar.max_numeric_value(), Some(0.75));
        assert!(bar.supports_action(Action::Increment));
        assert!(bar.supports_action(Action::SetValue));
        let mark = tree
            .accesskit_node(node(&s, AkRole::Meter, "Mark"))
            .unwrap();
        assert_eq!(mark.max_numeric_value(), Some(0.75));
        assert!(!mark.supports_action(Action::Increment));
        assert!(!mark.supports_action(Action::SetValue));
    }

    #[test]
    fn a_step_asked_of_a_bar_moves_the_content_and_is_read_back() {
        let e = engine();
        let root = load(&e, BARS);
        let f = find(&e, root, "f");
        let mut s = listening(&e, root);
        let down = node(&s, AkRole::ScrollBar, "Down");

        assert!(s.accessible_action(&e, down, AccessibleAction::Increase));
        assert!((num(&e, f, "contentY") - 40.0).abs() < 1e-9);
        assert!(s.needs_frame());
        s.frame(&e, Time::from_millis(16));
        assert!((range(&s, down).value - 0.1).abs() < 1e-9);

        assert!(s.accessible_action(&e, down, AccessibleAction::SetValue(5.0)));
        assert_eq!(num(&e, f, "contentY"), 300.0);
        // At the end there is nowhere further to go.
        assert!(!s.accessible_action(&e, down, AccessibleAction::Increase));

        let mark = node(&s, AkRole::Meter, "Mark");
        assert!(!s.accessible_action(&e, mark, AccessibleAction::Decrease));
        assert_eq!(num(&e, f, "contentY"), 300.0);
    }

    #[test]
    fn a_view_says_how_far_it_is_scrolled_and_scrolls_when_asked() {
        let e = engine();
        let root = load(&e, VIEWS);
        let made = find(&e, root, "made");
        let mut s = listening(&e, root);
        let view = node(&s, AkRole::ScrollView, "Notes");
        let at = scroll(&s, view);
        assert_eq!((at.x, at.x_min, at.x_max), (0.0, 0.0, 300.0));
        assert_eq!((at.y, at.y_min, at.y_max), (0.0, 0.0, 200.0));
        assert!(s.accessibility_tree().get(view).unwrap().clips_children);

        let page = AccessibleAction::ScrollDown { page: true };
        assert!(s.accessible_action(&e, view, page));
        assert_eq!(num(&e, content_item(&e, made), "contentY"), 100.0);
        s.frame(&e, Time::from_millis(16));
        assert_eq!(scroll(&s, view).y, 100.0);

        // Nothing is above the beginning.
        let back = AccessibleAction::ScrollLeft { page: false };
        assert!(!s.accessible_action(&e, view, back));
    }

    #[test]
    fn a_view_takes_only_the_scrolls_that_have_somewhere_to_go() {
        let e = engine();
        let root = load(&e, VIEWS);
        let mut s = listening(&e, root);
        let view = node(&s, AkRole::ScrollView, "Notes");
        use ljos_a11y::accesskit::Action;
        let n = s.accessibility_tree().accesskit_node(view).unwrap();
        assert!(n.supports_action(Action::ScrollDown));
        assert!(n.supports_action(Action::ScrollRight));
        assert!(!n.supports_action(Action::ScrollUp));
        assert!(!n.supports_action(Action::ScrollLeft));
        assert!(n.clips_children());

        let end = AccessibleAction::ScrollDown { page: true };
        for _ in 0..3 {
            s.accessible_action(&e, view, end);
        }
        s.frame(&e, Time::from_millis(16));
        let n = s.accessibility_tree().accesskit_node(view).unwrap();
        assert!(!n.supports_action(Action::ScrollDown));
        assert!(n.supports_action(Action::ScrollUp));
    }
}
