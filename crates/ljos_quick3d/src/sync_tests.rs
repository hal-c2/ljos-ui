//! A view's kept scene, patched where its objects changed, is the scene a
//! build makes anew: after each kind of change, and after what arrives
//! later than the first frame.

use std::cell::Cell;
use std::rc::Rc;

use ljos::item::ljos_3d::qml::Piece;
use ljos::item::ljos_3d::{Material, MeshSource, NodeKind, Scene};
use ljos::prelude::*;
use ljos::testing::TestWindow;

use super::*;

/// `qml` made, with its window when `window`: a window paints the view
/// as a host does, at once when what it reads changes; without one, only
/// the test syncs it, and what each sync read is the test's to count.
fn make(qml: &str, window: bool) -> (Engine, Option<TestWindow>, ObjectId) {
    let e = ljos::engine();
    register(&e);
    let mut project = ljos_compiler::Project::new(ljos::dev::EngineOracle::from_engine(&e));
    project.add("Main", format!("import QtQuick\nimport QtQuick3D\n{qml}\n"));
    let compiled = ljos_compiler::compile_file(&project, "Main").expect("compiles");
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    // Before the root: its timers start on the window's clock.
    let mut w = window.then(|| TestWindow::new(&e, 100.0, 100.0));
    let root = e
        .create_from_url("Main", None, Vec::new())
        .expect("created");
    if let Some(w) = &mut w {
        w.show(root).expect("shown");
    }
    (e, w, root)
}

fn show(qml: &str) -> (Engine, TestWindow, ObjectId) {
    let (e, w, root) = make(qml, true);
    (e, w.expect("a window"), root)
}

fn named(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    e.read_by_name(root, name)
        .as_object()
        .unwrap_or_else(|| panic!("no object in {name}"))
}

fn write(e: &Engine, obj: ObjectId, name: &str, v: impl Into<Value>) {
    e.write_by_name(obj, name, v.into()).expect("written");
}

/// Syncs the view and checks its kept scene is the tree built anew;
/// returns how many pieces were read.
fn synced(e: &Engine, view: ObjectId) -> usize {
    let s = view::sync(e, view);
    let built = scene::build(e, view);
    if let Some(said) = s.scene.difference(&built) {
        panic!("{said}\npatched {:#?}\nbuilt {built:#?}", s.scene);
    }
    s.read
}

fn kept(e: &Engine, view: ObjectId) -> std::sync::Arc<Scene> {
    view::sync(e, view).scene
}

const SCENE: &str = r##"
Item {
    id: root
    width: 100; height: 100
    property int count: 3
    property QtObject view_: view
    property QtObject other_: other
    property QtObject sun_: sun
    property QtObject group_: group
    property QtObject apart_: apart
    property QtObject cube_: cube
    property QtObject material_: material
    property QtObject entry_: entry
    View3D {
        id: view
        anchors.fill: parent
        PerspectiveCamera { id: camera; z: 300 }
        PerspectiveCamera { id: other; z: 500 }
        DirectionalLight { id: sun }
        Node {
            id: group
            Model {
                id: cube; source: "#Cube"
                materials: DefaultMaterial { id: material; diffuseColor: "red" }
            }
        }
        Node { id: apart; x: 30; eulerRotation.y: 30 }
        Repeater3D {
            model: root.count
            Model { source: "#Sphere"; x: index * 10; scale: Qt.vector3d(0.1, 0.1, 0.1) }
        }
        Model {
            source: "#Cube"
            instancing: InstanceList {
                instances: [ InstanceListEntry { id: entry; position: Qt.vector3d(0, 0, 0) } ]
            }
        }
    }
}
"##;

fn scene_view() -> (Engine, ObjectId, ObjectId) {
    let (e, _, root) = make(SCENE, false);
    let view = named(&e, root, "view_");
    // The first sync reads every piece.
    assert!(synced(&e, view) >= 10);
    (e, root, view)
}

#[test]
fn a_synced_view_has_nothing_left_to_read() {
    let (e, _root, view) = scene_view();
    assert_eq!(synced(&e, view), 0);
}

#[test]
fn a_painted_view_is_the_scene_built_anew() {
    let (e, mut w, root) = show(SCENE);
    let view = named(&e, root, "view_");
    // Each paint checks its scene against a build; these paint at once.
    write(&e, named(&e, root, "cube_"), "x", 40.0);
    write(&e, root, "count", 5);
    write(
        &e,
        named(&e, root, "cube_"),
        "parent",
        named(&e, root, "apart_"),
    );
    w.advance(16).expect("advanced");
    assert_eq!(synced(&e, view), 0);
}

#[test]
fn a_moved_model_is_read_alone() {
    let (e, root, view) = scene_view();
    write(&e, named(&e, root, "cube_"), "x", 40.0);
    assert_eq!(synced(&e, view), 1);
    let turned = e.read_by_name(named(&e, root, "apart_"), "eulerRotation");
    write(&e, named(&e, root, "cube_"), "eulerRotation", turned);
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_material_change_reads_its_model_alone() {
    let (e, root, view) = scene_view();
    write(&e, named(&e, root, "material_"), "diffuseColor", "blue");
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_light_change_reads_the_light_alone() {
    let (e, root, view) = scene_view();
    write(&e, named(&e, root, "sun_"), "brightness", 2.5);
    assert_eq!(synced(&e, view), 1);
    write(
        &e,
        named(&e, root, "sun_"),
        "scope",
        named(&e, root, "group_"),
    );
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_camera_chosen_reads_the_view_alone() {
    let (e, root, view) = scene_view();
    write(&e, view, "camera", named(&e, root, "other_"));
    assert_eq!(synced(&e, view), 1);
    write(&e, named(&e, root, "other_"), "fieldOfView", 30.0);
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_hidden_node_reads_itself_alone() {
    let (e, root, view) = scene_view();
    write(&e, named(&e, root, "group_"), "visible", false);
    assert_eq!(synced(&e, view), 1);
    write(&e, named(&e, root, "group_"), "visible", true);
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_reparented_node_reads_its_two_parents() {
    let (e, root, view) = scene_view();
    write(
        &e,
        named(&e, root, "cube_"),
        "parent",
        named(&e, root, "apart_"),
    );
    let read = synced(&e, view);
    assert!((1..=3).contains(&read), "{read}");
    write(&e, named(&e, root, "cube_"), "x", 5.0);
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_destroyed_node_is_gone_from_the_scene() {
    let (e, root, view) = scene_view();
    e.destroy(named(&e, root, "cube_"));
    let read = synced(&e, view);
    assert!(read <= 2, "{read}");
}

#[test]
fn a_repeater_model_change_reads_only_what_it_made() {
    let (e, root, view) = scene_view();
    write(&e, root, "count", 6);
    // A new model makes every delegate anew: the repeater and the six it
    // made are read, not the scene around them.
    assert_eq!(synced(&e, view), 7);
    write(&e, root, "count", 1);
    assert_eq!(synced(&e, view), 2);
}

#[test]
fn an_instance_table_change_reads_its_model_alone() {
    let (e, root, view) = scene_view();
    write(
        &e,
        named(&e, root, "entry_"),
        "position",
        e.read_by_name(named(&e, root, "apart_"), "position"),
    );
    assert_eq!(synced(&e, view), 1);
}

fn model_path(name: &str) -> String {
    format!(
        "file://{}/../../features/quick3d/models/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn custom_meshes(s: &Scene) -> usize {
    s.walk()
        .iter()
        .filter(|(_, n, _)| {
            matches!(&n.kind, NodeKind::Model(m) if matches!(m.mesh, MeshSource::Custom(_)))
        })
        .count()
}

#[test]
fn a_mesh_file_that_arrives_reaches_the_kept_scene() {
    let (e, mut w, root) = show(&format!(
        r##"View3D {{ width: 40; height: 40
               OrthographicCamera {{ z: 300 }}
               Model {{ source: "{}" }} }}"##,
        model_path("two-squares.mesh")
    ));
    w.advance(100).expect("advanced");
    assert_eq!(custom_meshes(&kept(&e, root)), 2);
    assert_eq!(synced(&e, root), 0);
}

#[test]
fn a_texture_that_arrives_reaches_the_kept_scene() {
    let (e, mut w, root) = show(&format!(
        r##"View3D {{ width: 40; height: 40
               OrthographicCamera {{ z: 300 }}
               Model {{ source: "#Rectangle"
                   materials: DefaultMaterial {{
                       diffuseMap: Texture {{ source: "{}" }} }} }} }}"##,
        model_path("quadrants.png")
    ));
    w.advance(100).expect("advanced");
    let s = kept(&e, root);
    let mapped = s.walk().iter().any(|(_, n, _)| match &n.kind {
        NodeKind::Model(m) => {
            matches!(&m.material, Material::Default(d) if d.diffuse_map.is_some())
        }
        _ => false,
    });
    assert!(mapped, "{s:#?}");
    assert_eq!(synced(&e, root), 0);
}

#[cfg(feature = "gltf")]
#[test]
fn a_runtime_loader_that_finishes_reaches_the_kept_scene() {
    let (e, mut w, root) = show(&format!(
        r##"View3D {{ width: 40; height: 40
               OrthographicCamera {{ z: 300 }}
               RuntimeLoader {{ source: "{}" }} }}"##,
        model_path("square.gltf")
    ));
    w.advance(100).expect("advanced");
    assert!(custom_meshes(&kept(&e, root)) >= 1);
    assert_eq!(synced(&e, root), 0);
}

#[test]
fn items_in_a_node_reach_the_kept_scene() {
    // Two pictures drawn in one frame: the first one's tell paints the
    // view at once, before the second is told.
    let (e, mut w, root) = show(
        r##"View3D { width: 40; height: 40
               OrthographicCamera { z: 300 }
               Node { Rectangle { width: 8; height: 4; color: "red" } }
               Node { y: -6
                   Rectangle { anchors.centerIn: parent; width: 4; height: 2 } } }"##,
    );
    w.advance(100).expect("advanced");
    let quads = kept(&e, root)
        .walk()
        .iter()
        .filter(|(_, n, _)| matches!(&n.kind, NodeKind::Model(_)))
        .count();
    assert_eq!(quads, 2);
    assert_eq!(synced(&e, root), 0);
}

#[test]
fn a_repeater_given_a_new_array_reaches_the_kept_scene() {
    // A spawner that gives its repeater a new array when its count
    // changes, as well as through the binding, and delegates that place
    // themselves once made, each drawing a mesh file: the file the old
    // delegates held goes with them, and the new ones read it again.
    const SPAWNER: &str = r##"
Item {
    id: root
    width: 100; height: 100
    property QtObject view_: view
    property QtObject spawner_: spawner
    View3D {
        id: view
        anchors.fill: parent
        PerspectiveCamera { z: 300 }
        Node {
            id: spawner
            property int count: 0
            onCountChanged: rep.model = new Array(spawner.count).fill({ "ts": 1 })
            Repeater3D {
                id: rep
                model: new Array(spawner.count).fill({ "ts": 1 })
                delegate: Node {
                    required property int index
                    Component.onCompleted: { position.x = index * 3; position.y = index }
                    Node { Model { source: "MESH" } }
                }
            }
        }
    }
}
"##;
    let (e, mut w, root) = show(&SPAWNER.replace("MESH", &model_path("two-squares.mesh")));
    let view = named(&e, root, "view_");
    let spawner = named(&e, root, "spawner_");
    for count in [3, 5, 2, 5] {
        write(&e, spawner, "count", count);
        w.advance(100).expect("advanced");
        assert_eq!(synced(&e, view), 0);
        // Two squares each.
        assert_eq!(custom_meshes(&kept(&e, view)), 2 * count as usize);
    }
}

const PAIR: &str = r##"
Item {
    id: root
    width: 100; height: 100
    property QtObject view_: view
    property QtObject a_: a
    property QtObject b_: b
    property QtObject away_: away
    // Outside the view: a node moved here leaves its scene.
    Node { id: away }
    View3D {
        id: view
        anchors.fill: parent
        PerspectiveCamera { z: 300 }
        Node { id: a }
        Node { id: b }
    }
}
"##;

fn pair() -> (Engine, ObjectId, ObjectId, ObjectId) {
    let (e, _, root) = make(PAIR, false);
    let view = named(&e, root, "view_");
    synced(&e, view);
    let (a, b) = (named(&e, root, "a_"), named(&e, root, "b_"));
    (e, view, a, b)
}

fn x(e: &Engine, o: ObjectId) -> f64 {
    e.read_by_name(o, "x").to_number()
}

#[test]
fn what_a_read_changes_is_patched_in_the_same_paint() {
    let (e, view, a, b) = pair();
    // Reading a writes b, as a LodManager that follows the camera just
    // read does: b, not told before the paint, is read in a later pass of
    // it.
    view::on_read(
        &e,
        view,
        Some(Rc::new(move |e: &Engine, piece| {
            if piece == Piece::Node(a) {
                write(e, b, "x", x(e, a) * 2.0);
            }
        })),
    );
    write(&e, a, "x", 5.0);
    assert_eq!(synced(&e, view), 2);
    assert_eq!(x(&e, b), 10.0);
    assert!(!view::unsettled(&e, view));
    assert!(!view::asked(&e, view));
}

#[test]
fn reads_that_write_each_other_are_cut_short_and_painted_again() {
    let (e, view, a, b) = pair();
    // Each read moves the other: a loop the graph does not see, as each
    // write is a read's, not a binding's.
    view::on_read(
        &e,
        view,
        Some(Rc::new(move |e: &Engine, piece| {
            let (from, to) = if piece == Piece::Node(a) {
                (a, b)
            } else {
                (b, a)
            };
            if matches!(piece, Piece::Node(_)) {
                write(e, to, "x", x(e, from) + 1.0);
            }
        })),
    );
    write(&e, a, "x", 1.0);
    // Not checked against a build: the scene is one write behind, and a
    // paint cut short does not check what it drew.
    let s = view::sync(&e, view);
    assert_eq!(s.read, view::PASSES + 1);
    assert!(view::unsettled(&e, view), "a write left for the next paint");
    assert!(view::asked(&e, view), "and that paint asked for");
    // Once the reads stop writing, the next paint ends where a build does.
    view::on_read(&e, view, None);
    synced(&e, view);
    assert!(!view::unsettled(&e, view));
}

#[test]
fn a_read_held_from_before_a_change_is_read_again() {
    let (e, view, a, _) = pair();
    // Held as the graph holds one it ran by itself, then a's change told:
    // the paint reads a again rather than draw the read from before.
    view::hold_read(&e, view, Piece::Node(a));
    write(&e, a, "x", 40.0);
    assert_eq!(synced(&e, view), 1);
}

#[test]
fn a_node_moved_out_of_the_view_leaves_its_scene() {
    let (e, _, root) = make(PAIR, false);
    let view = named(&e, root, "view_");
    synced(&e, view);
    let (b, away) = (named(&e, root, "b_"), named(&e, root, "away_"));
    // The view's lists tell nobody they changed; b's `parent` does.
    write(&e, b, "parent", away);
    let s = view::sync(&e, view);
    assert!(s.gone.contains(&Piece::Node(b)), "{:?}", s.gone);
    synced(&e, view);
}

#[test]
fn a_read_held_for_a_node_that_leaves_goes_with_it() {
    let (e, _, root) = make(PAIR, false);
    let view = named(&e, root, "view_");
    synced(&e, view);
    let (b, away) = (named(&e, root, "b_"), named(&e, root, "away_"));
    // b leaves through the view's own piece, whose children no longer
    // have it. Held while that piece is read, in the pass that takes b
    // out, as the graph holds b's when it runs b's tracking then; once,
    // as that tracking is gone after.
    let held = Rc::new(Cell::new(false));
    let hold = held.clone();
    view::on_read(
        &e,
        view,
        Some(Rc::new(move |e: &Engine, piece| {
            if piece == Piece::View && !hold.replace(true) {
                view::hold_read(e, view, Piece::Node(b));
            }
        })),
    );
    write(&e, b, "parent", away);
    let s = view::sync(&e, view);
    assert!(held.get(), "the view was read");
    assert!(s.gone.contains(&Piece::Node(b)), "{:?}", s.gone);
    assert!(
        !view::unsettled(&e, view),
        "b's read is not left pending: {} read",
        s.read
    );
    assert!(!view::asked(&e, view), "nor another paint asked for it");
    view::on_read(&e, view, None);
    synced(&e, view);
}

#[test]
fn the_arrivals_of_objects_gone_are_disposed() {
    let (e, view, ..) = pair();
    // Plain objects without a scope of their own: their arrivals are in
    // the view's, which outlives them.
    let objects: Vec<ObjectId> = (0..64)
        .map(|_| e.create_native(Node::default(), None).expect("made"))
        .collect();
    let nodes: Vec<_> = objects
        .iter()
        .map(|&o| {
            assert!(e.object_scope(o).is_none());
            arrival::wait(&e, view, o);
            arrival::node(&e, o).expect("an arrival")
        })
        .collect();
    for &o in &objects {
        e.destroy(o);
    }
    e.process_deferred_deletes();
    assert!(nodes.iter().all(|n| e.graph().is_alive(*n)));
    // The next one sweeps the map, which has doubled.
    let next = e.create_native(Node::default(), None).expect("made");
    arrival::wait(&e, view, next);
    assert!(nodes.iter().all(|n| !e.graph().is_alive(*n)));
    assert!(arrival::node(&e, next).is_some());
}
