use ljos::item::ljos_3d::{Background, Camera, Lighting, Material, NodeKind};
use ljos::prelude::*;

use super::*;

fn load(qml: &str) -> (Engine, ObjectId) {
    let e = ljos::engine();
    register(&e);
    let mut project = ljos_compiler::Project::new(ljos::dev::EngineOracle::from_engine(&e));
    project.add("Main", format!("import QtQuick\nimport QtQuick3D\n{qml}\n"));
    let compiled = ljos_compiler::compile_file(&project, "Main").expect("compiles");
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    let root = e
        .create_from_url("Main", None, Vec::new())
        .expect("created");
    (e, root)
}

/// The object the root's property `name` holds.
fn named(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    e.read_by_name(root, name)
        .as_object()
        .unwrap_or_else(|| panic!("no object in {name}"))
}

const SCENE: &str = r##"
View3D {
    property QtObject camera_: camera
    property QtObject sun_: sun
    property QtObject cube_: cube
    width: 100; height: 100
    environment: SceneEnvironment {
        clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
    }
    PerspectiveCamera { id: camera; z: 300 }
    DirectionalLight { id: sun }
    Node {
        Model {
            id: cube; source: "#Cube"
            materials: DefaultMaterial {
                lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
        }
    }
}
"##;

#[test]
fn the_nodes_declared_in_a_view_are_its_scene() {
    let (e, view) = load(SCENE);
    let s = scene::build(&e, view);
    let kinds: Vec<_> = s.walk().iter().map(|(_, n, _)| n.kind.clone()).collect();
    assert_eq!(kinds.len(), 4, "{kinds:?}");
    assert!(matches!(
        kinds[0],
        NodeKind::Camera(Camera::Perspective { .. })
    ));
    assert!(matches!(kinds[1], NodeKind::Light(_)));
    assert!(matches!(kinds[2], NodeKind::Group));
    let NodeKind::Model(m) = &kinds[3] else {
        panic!("{:?}", kinds[3]);
    };
    let Material::Default(d) = m.material;
    assert_eq!(d.lighting, Lighting::None);
    assert_eq!((d.diffuse_color.r, d.diffuse_color.g), (1.0, 0.0));
    let (_, camera) = s.active_camera().expect("a camera");
    assert_eq!(camera.translation().z, 300.0);
    assert!(matches!(s.environment().background, Background::Color(_)));
}

#[test]
fn a_walk_that_comes_out_the_same_keeps_its_scene() {
    let (e, view) = load(SCENE);
    let a = scene::build(&e, view);
    let b = scene::build(&e, view);
    assert!(a.same_as(&b));
    e.write_by_name(named(&e, view, "cube_"), "x", Value::from(10.0))
        .expect("written");
    assert!(!a.same_as(&scene::build(&e, view)));
}

#[test]
fn x_and_position_follow_each_other() {
    let (e, view) = load(SCENE);
    let cube = named(&e, view, "cube_");
    e.write_by_name(cube, "x", Value::from(5.0)).expect("x");
    assert_eq!(
        e.get(cube, Node::POSITION).expect("position"),
        Vector3d {
            x: 5.0,
            y: 0.0,
            z: 0.0
        }
    );
    e.set(
        cube,
        Node::POSITION,
        Vector3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
    )
    .expect("position");
    let xyz = [Node::X, Node::Y, Node::Z].map(|p| e.get(cube, p).expect("axis"));
    assert_eq!(xyz, [1.0, 2.0, 3.0]);
}

#[test]
fn euler_rotation_and_rotation_follow_each_other() {
    let (e, view) = load(SCENE);
    let cube = named(&e, view, "cube_");
    e.set(
        cube,
        Node::EULER_ROTATION,
        Vector3d {
            x: 0.0,
            y: 90.0,
            z: 0.0,
        },
    )
    .expect("euler");
    let q = e.get(cube, Node::ROTATION).expect("rotation");
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (q.scalar - half).abs() < 1e-5 && (q.y - half).abs() < 1e-5,
        "{q:?}"
    );
    // About X by 90°: (cos 45°, sin 45°, 0, 0).
    e.set(
        cube,
        Node::ROTATION,
        Quaternion {
            scalar: half,
            x: half,
            y: 0.0,
            z: 0.0,
        },
    )
    .expect("rotation");
    let d = e.get(cube, Node::EULER_ROTATION).expect("euler");
    assert!(
        (d.x - 90.0).abs() < 1e-3 && d.y.abs() < 1e-3 && d.z.abs() < 1e-3,
        "{d:?}"
    );
}

#[test]
fn a_binding_on_x_stays_when_position_follows() {
    let (e, root) = load(
        r#"View3D { property real at: 4
               property QtObject m_: m
               Model { id: m; x: at } }"#,
    );
    let m = named(&e, root, "m_");
    assert_eq!(e.get(m, Node::POSITION).expect("position").x, 4.0);
    e.write_by_name(root, "at", Value::from(7.0)).expect("at");
    assert_eq!(e.get(m, Node::POSITION).expect("position").x, 7.0);
}

#[test]
fn the_types_start_from_the_documented_values() {
    let (e, view) = load(SCENE);
    let camera = named(&e, view, "camera_");
    assert_eq!(
        e.get(camera, PerspectiveCamera::FIELD_OF_VIEW).ok(),
        Some(60.0)
    );
    assert_eq!(e.get(camera, PerspectiveCamera::CLIP_NEAR).ok(), Some(10.0));
    assert_eq!(
        e.get(camera, PerspectiveCamera::CLIP_FAR).ok(),
        Some(10000.0)
    );
    let sun = named(&e, view, "sun_");
    assert_eq!(e.get(sun, Light::BRIGHTNESS).ok(), Some(1.0));
    let cube = named(&e, view, "cube_");
    assert_eq!(
        e.get(cube, Node::SCALE).ok(),
        Some(Vector3d {
            x: 1.0,
            y: 1.0,
            z: 1.0
        })
    );
    assert_eq!(e.get(cube, Model::CASTS_SHADOWS).ok(), Some(true));
}

/// The picture of `root`, `w` by `h`, as straight RGBA rows.
fn picture(e: &Engine, root: ObjectId, w: u32, h: u32) -> Vec<u8> {
    let png = ljos::render::Picture::new(w, h)
        .settle(0)
        .png(e, root)
        .expect("drawn");
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().expect("a png");
    let mut buf = vec![0; reader.output_buffer_size().expect("sized")];
    reader.next_frame(&mut buf).expect("a frame");
    buf
}

fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn a_view_is_drawn_in_the_picture() {
    let (e, root) = load(&format!(
        "Rectangle {{ width: 100; height: 100; color: \"black\"\n{SCENE}\n}}"
    ));
    let Some(_) = ljos::item::ljos_3d::offscreen::Offscreen::new().ok() else {
        eprintln!("SKIPPED: no adapter");
        return;
    };
    let rgba = picture(&e, root, 100, 100);
    assert_eq!(pixel(&rgba, 100, 2, 2), [0x20, 0x20, 0x20, 255]);
    assert_eq!(pixel(&rgba, 100, 50, 50), [255, 0, 0, 255]);
}

/// A view that goes lets go of its picture, so a window that makes and
/// drops views over and over keeps one picture, not one for each.
#[test]
fn a_destroyed_view_lets_go_of_its_picture() {
    use ljos_render::{FrameView, RasterConfig, VelloCpuReference};
    let (e, root) = load(
        r##"Item { width: 40; height: 40
               property bool shown: true
               Loader { active: shown; anchors.fill: parent
                        sourceComponent: View3D {
                            PerspectiveCamera { z: 300 }
                            Model { source: "#Cube" } } } }"##,
    );
    if ljos::item::ljos_3d::offscreen::Offscreen::new().is_err() {
        eprintln!("SKIPPED: no adapter");
        return;
    }
    let pic = ljos::render::Picture::new(40, 40).settle(0);
    let mut scene = pic.scene(&e, root).expect("laid out");
    let mut r = VelloCpuReference::new(RasterConfig::default());
    let objects = e.object_count();
    for round in 0..20 {
        for shown in [false, true] {
            e.write_by_name(root, "shown", Value::from(shown))
                .expect("shown");
            let out = scene.frame(&e, pic.now());
            r.render(&FrameView::new(&out.snapshot, 1.0))
                .expect("drawn");
            assert_eq!(
                r.view3d_pictures(),
                usize::from(shown),
                "round {round}, shown {shown}"
            );
        }
    }
    assert_eq!(e.object_count(), objects, "the views made are all gone");
}
