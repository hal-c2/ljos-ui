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
    let Material::Default(d) = &m.material else {
        panic!("{:?}", m.material);
    };
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

const M3: &str = r##"
View3D {
    property QtObject env_: env
    width: 100; height: 100
    environment: SceneEnvironment {
        id: env; antialiasingMode: SceneEnvironment.MSAA
    }
    PerspectiveCamera {
        z: 300; fieldOfViewOrientation: PerspectiveCamera.Horizontal
        frustumCullingEnabled: true
    }
    PointLight { linearFade: 2; brightness: 3; ambientColor: "#808080" }
    SpotLight { coneAngle: 50; innerConeAngle: 10; quadraticFade: 0 }
    Model {
        source: "#Sphere"
        materials: PrincipledMaterial {
            baseColor: "red"; metalness: 0.5; roughness: 0.25
            emissiveFactor: Qt.vector3d(0, 1, 0); opacity: 0.5
            alphaMode: PrincipledMaterial.Mask; alphaCutoff: 0.3
            cullMode: Material.NoCulling; lighting: PrincipledMaterial.NoLighting
        }
    }
    Model {
        source: "#Cone"
        materials: DefaultMaterial {
            specularAmount: 0.75; specularRoughness: 0.5
            cullMode: Material.FrontFaceCulling
        }
    }
    Model { source: "#Cylinder" }
}
"##;

#[test]
fn lights_materials_and_cameras_reach_the_scene() {
    use ljos::item::ljos_3d::{AlphaMode, CullMode, Fade, Light, MeshSource, Primitive};
    let (e, view) = load(M3);
    let s = scene::build(&e, view);
    let kinds: Vec<_> = s.walk().iter().map(|(_, n, _)| n.kind.clone()).collect();
    assert_eq!(
        kinds[0],
        NodeKind::Camera(Camera::Perspective {
            fov_degrees: 60.0,
            fov_horizontal: true,
            clip_near: 10.0,
            clip_far: 10000.0,
            frustum_culling: true,
        })
    );
    let NodeKind::Light(Light::Point {
        fade,
        brightness,
        ambient_color,
        ..
    }) = kinds[1]
    else {
        panic!("{:?}", kinds[1]);
    };
    assert_eq!(
        fade,
        Fade {
            constant: 1.0,
            linear: 2.0,
            quadratic: 1.0
        }
    );
    assert_eq!(brightness, 3.0);
    assert!((ambient_color.r - 128.0 / 255.0).abs() < 1e-3);
    let NodeKind::Light(Light::Spot {
        fade,
        cone_angle,
        inner_cone_angle,
        ..
    }) = kinds[2]
    else {
        panic!("{:?}", kinds[2]);
    };
    assert_eq!(
        (fade.quadratic, cone_angle, inner_cone_angle),
        (0.0, 50.0, 10.0)
    );

    let NodeKind::Model(sphere) = &kinds[3] else {
        panic!("{:?}", kinds[3]);
    };
    assert_eq!(sphere.mesh, MeshSource::Primitive(Primitive::Sphere));
    let Material::Principled(p) = &sphere.material else {
        panic!("{:?}", sphere.material);
    };
    assert_eq!((p.base_color.r, p.base_color.g), (1.0, 0.0));
    assert_eq!((p.metalness, p.roughness, p.opacity), (0.5, 0.25, 0.5));
    assert_eq!(p.emissive, [0.0, 1.0, 0.0]);
    assert_eq!((p.alpha_mode, p.alpha_cutoff), (AlphaMode::Mask, 0.3));
    assert_eq!((p.cull_mode, p.lighting), (CullMode::None, Lighting::None));

    let NodeKind::Model(cone) = &kinds[4] else {
        panic!("{:?}", kinds[4]);
    };
    assert_eq!(cone.mesh, MeshSource::Primitive(Primitive::Cone));
    let Material::Default(d) = &cone.material else {
        panic!("{:?}", cone.material);
    };
    assert_eq!((d.specular_amount, d.specular_roughness), (0.75, 0.5));
    assert_eq!(d.cull_mode, CullMode::Front);
    assert!(matches!(&kinds[5], NodeKind::Model(m)
        if m.mesh == MeshSource::Primitive(Primitive::Cylinder)));

    // High, the default quality, is 4 samples; Medium 2, VeryHigh 8.
    assert_eq!(s.environment().msaa_samples, 4);
    let env = named(&e, view, "env_");
    for (quality, samples) in [(0, 2), (2, 8)] {
        e.write_by_name(env, "antialiasingQuality", Value::from(quality))
            .expect("quality");
        assert_eq!(scene::build(&e, view).environment().msaa_samples, samples);
    }
    e.write_by_name(env, "antialiasingMode", Value::from(0))
        .expect("mode");
    assert_eq!(scene::build(&e, view).environment().msaa_samples, 1);
}

#[test]
fn the_new_types_start_from_the_documented_values() {
    let (e, root) = load(
        r##"View3D {
               property QtObject point_: point
               property QtObject spot_: spot
               property QtObject pbr_: pbr
               property QtObject plain_: plain
               property QtObject env_: env
               property QtObject camera_: camera
               environment: SceneEnvironment { id: env }
               PerspectiveCamera { id: camera }
               PointLight { id: point }
               SpotLight { id: spot }
               PrincipledMaterial { id: pbr }
               DefaultMaterial { id: plain } }"##,
    );
    let point = named(&e, root, "point_");
    assert_eq!(e.get(point, PointLight::CONSTANT_FADE).ok(), Some(1.0));
    assert_eq!(e.get(point, PointLight::LINEAR_FADE).ok(), Some(0.0));
    assert_eq!(e.get(point, PointLight::QUADRATIC_FADE).ok(), Some(1.0));
    let spot = named(&e, root, "spot_");
    assert_eq!(e.get(spot, SpotLight::CONE_ANGLE).ok(), Some(40.0));
    assert_eq!(e.get(spot, SpotLight::INNER_CONE_ANGLE).ok(), Some(30.0));
    assert_eq!(e.get(spot, SpotLight::QUADRATIC_FADE).ok(), Some(1.0));
    let pbr = named(&e, root, "pbr_");
    assert_eq!(e.get(pbr, PrincipledMaterial::METALNESS).ok(), Some(0.0));
    assert_eq!(e.get(pbr, PrincipledMaterial::ROUGHNESS).ok(), Some(0.0));
    assert_eq!(e.get(pbr, PrincipledMaterial::ALPHA_CUTOFF).ok(), Some(0.5));
    assert_eq!(e.get(pbr, crate::Material::CULL_MODE).ok(), Some(0));
    let plain = named(&e, root, "plain_");
    assert_eq!(
        e.get(plain, DefaultMaterial::SPECULAR_AMOUNT).ok(),
        Some(0.0)
    );
    let env = named(&e, root, "env_");
    assert_eq!(
        e.get(env, SceneEnvironment::ANTIALIASING_QUALITY).ok(),
        Some(1)
    );
    let camera = named(&e, root, "camera_");
    assert_eq!(
        e.get(camera, crate::Camera::FRUSTUM_CULLING_ENABLED).ok(),
        Some(false)
    );
}

/// What an application writes: a geometry type of its own, a square of
/// `size` facing +Z, as a fan.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Square", extends = Geometry)]
struct Square {
    #[qml(base)]
    geometry: Geometry,
}

impl Square {
    fn fill(e: &Engine, me: ObjectId, size: f32) {
        let h = size / 2.0;
        e.with_mut(me, |s: &mut Square| {
            let g = &mut s.geometry;
            g.clear();
            g.set_vertex_data(
                [-h, -h, 0.0, h, -h, 0.0, h, h, 0.0, -h, h, 0.0]
                    .iter()
                    .flat_map(|f: &f32| f.to_le_bytes())
                    .collect(),
            );
            g.set_stride(12);
            g.add_attribute(Semantic::Position, 0, ComponentType::F32);
            g.set_primitive_type(PrimitiveType::TriangleFan);
        })
        .expect("filled");
        Geometry::update(e, me);
    }
}

fn load_square(qml: &str) -> (Engine, ObjectId) {
    let e = ljos::engine();
    register(&e);
    e.register_module(ljos::module!("Test" 1.0 => [Square]));
    let mut project = ljos_compiler::Project::new(ljos::dev::EngineOracle::from_engine(&e));
    project.add(
        "Main",
        format!("import QtQuick\nimport QtQuick3D\nimport Test\n{qml}\n"),
    );
    let compiled = ljos_compiler::compile_file(&project, "Main").expect("compiles");
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    e.set_component_source(source);
    let root = e
        .create_from_url("Main", None, Vec::new())
        .expect("created");
    (e, root)
}

const SQUARE: &str = r##"
Rectangle {
    property QtObject square_: square
    width: 100; height: 100; color: "black"
    View3D {
        anchors.fill: parent
        environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
        }
        OrthographicCamera { z: 300 }
        Model {
            geometry: Square { id: square }
            materials: DefaultMaterial {
                lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
        }
    }
}
"##;

fn view_of(e: &Engine, root: ObjectId) -> ObjectId {
    e.children(root)
        .into_iter()
        .find(|c| e.is::<View3D>(*c))
        .expect("a view")
}

#[test]
fn a_geometry_reaches_the_scene_once_it_is_updated() {
    use ljos::item::ljos_3d::MeshSource;
    let (e, root) = load_square(SQUARE);
    let view = view_of(&e, root);
    let square = named(&e, root, "square_");
    let model_mesh = |s: &ljos::item::ljos_3d::Scene| {
        s.walk().iter().find_map(|(_, n, _)| match &n.kind {
            NodeKind::Model(m) => Some(m.mesh.clone()),
            _ => None,
        })
    };
    assert_eq!(
        model_mesh(&scene::build(&e, view)),
        None,
        "nothing committed yet: the model is a plain node"
    );

    Square::fill(&e, square, 60.0);
    let a = scene::build(&e, view);
    let Some(MeshSource::Custom(first)) = model_mesh(&a) else {
        panic!("{:?}", model_mesh(&a));
    };
    assert_eq!(first.vertex_data().len(), 48);
    assert!(a.same_as(&scene::build(&e, view)), "no update, no change");

    Square::fill(&e, square, 80.0);
    let b = scene::build(&e, view);
    assert!(!a.same_as(&b));
    let Some(MeshSource::Custom(second)) = model_mesh(&b) else {
        panic!();
    };
    assert_eq!(second.id(), first.id(), "the same geometry, revised");
    assert_ne!(second.revision(), first.revision());
}

#[test]
fn a_view_is_drawn_again_when_its_geometry_is_updated() {
    use ljos_render::{FrameView, RasterConfig, VelloCpuReference};
    if ljos::item::ljos_3d::offscreen::Offscreen::new().is_err() {
        eprintln!("SKIPPED: no adapter");
        return;
    }
    let (e, root) = load_square(SQUARE);
    let square = named(&e, root, "square_");
    Square::fill(&e, square, 30.0);
    let pic = ljos::render::Picture::new(100, 100).settle(0);
    let mut scene = pic.scene(&e, root).expect("laid out");
    let mut r = VelloCpuReference::new(RasterConfig::default());
    let first = scene.frame(&e, pic.now());
    assert!(
        !scene.frame(&e, pic.now()).changed,
        "a still geometry costs nothing"
    );
    r.render(&FrameView::new(&first.snapshot, 1.0))
        .expect("drawn");

    Square::fill(&e, square, 80.0);
    let second = scene.frame(&e, pic.now());
    assert!(second.changed, "update() dirtied the view");
    r.render(&FrameView::new(&second.snapshot, 1.0))
        .expect("drawn");
    assert_eq!(r.view3d_pictures(), 1);

    let rgba = picture(&e, root, 100, 100);
    assert_eq!(pixel(&rgba, 100, 50, 50), [255, 0, 0, 255]);
    assert_eq!(
        pixel(&rgba, 100, 20, 50),
        [255, 0, 0, 255],
        "the larger one"
    );
    assert_eq!(pixel(&rgba, 100, 5, 50), [0x20, 0x20, 0x20, 255]);
}

const SHADOWS: &str = r##"
View3D {
    width: 100; height: 100
    PerspectiveCamera { z: 300 }
    DirectionalLight { castsShadow: false }
    DirectionalLight {
        castsShadow: true; shadowFactor: 50; shadowBias: 4; shadowMapFar: 800
        shadowMapQuality: Light.ShadowMapQualityHigh
        softShadowQuality: Light.PCF64; pcfFactor: 3
        csmNumSplits: 2; csmSplit1: 0.2; csmBlendRatio: 0.1; lockShadowmapTexels: true
    }
    SpotLight { castsShadow: true; softShadowQuality: Light.Hard }
    Model { source: "#Cube"; castsShadows: false; receivesShadows: false }
    Model { source: "#Cube" }
}
"##;

#[test]
fn shadow_properties_reach_the_scene() {
    use ljos::item::ljos_3d::{Cascades, Shadow};
    let (e, view) = load(SHADOWS);
    let s = scene::build(&e, view);
    let nodes: Vec<_> = s.walk().iter().map(|(_, n, _)| (*n).clone()).collect();
    assert_eq!(nodes[1].shadow, None, "a light that casts none");
    assert_eq!(
        nodes[2].shadow,
        Some(Shadow {
            map_size: 1024,
            bias: 4.0,
            factor: 0.5,
            far: 800.0,
            samples: 16,
            pcf_radius: 3.0,
            cascades: Cascades {
                splits: 2,
                at: [0.2, 0.25, 0.5],
                blend: 0.1,
                lock_texels: true,
            },
        })
    );
    let spot = nodes[3].shadow.as_ref().expect("the spot light casts");
    assert_eq!((spot.samples, spot.map_size), (1, 256));
    assert_eq!(spot.cascades, Cascades::default());
    assert!(!nodes[4].casts_shadows && !nodes[4].receives_shadows);
    assert!(nodes[5].casts_shadows && nodes[5].receives_shadows);
}

#[test]
fn the_shadow_properties_start_from_the_documented_values() {
    let (e, view) = load("View3D { property QtObject sun_: sun\nDirectionalLight { id: sun } }");
    let sun = named(&e, view, "sun_");
    assert_eq!(e.get(sun, Light::CASTS_SHADOW).ok(), Some(false));
    assert_eq!(e.get(sun, Light::SHADOW_FACTOR).ok(), Some(75.0));
    assert_eq!(e.get(sun, Light::SHADOW_BIAS).ok(), Some(10.0));
    assert_eq!(e.get(sun, Light::SHADOW_MAP_FAR).ok(), Some(5000.0));
    assert_eq!(e.get(sun, Light::PCF_FACTOR).ok(), Some(2.0));
    assert_eq!(e.get(sun, Light::SHADOW_MAP_QUALITY).ok(), Some(0));
    assert_eq!(e.get(sun, Light::SOFT_SHADOW_QUALITY).ok(), Some(1));
    assert_eq!(e.get(sun, DirectionalLight::CSM_NUM_SPLITS).ok(), Some(0));
    assert_eq!(e.get(sun, DirectionalLight::CSM_SPLIT2).ok(), Some(0.25));
    assert_eq!(
        e.get(sun, DirectionalLight::CSM_BLEND_RATIO).ok(),
        Some(0.05)
    );
}
