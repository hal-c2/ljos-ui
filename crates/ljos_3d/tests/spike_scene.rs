// The scene of this QML, built by hand until the QtQuick3D types build it:
//
//     View3D {
//         width: 320; height: 240
//         environment: SceneEnvironment {
//             backgroundMode: SceneEnvironment.Color; clearColor: "#202020"
//         }
//         PerspectiveCamera { z: 300 }
//         DirectionalLight { eulerRotation: Qt.vector3d(-30, -30, 0) }
//         Model {
//             source: "#Cube"
//             eulerRotation: Qt.vector3d(30, 45, 0)
//             materials: DefaultMaterial { diffuseColor: "#4080ff" }
//         }
//     }

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Light, Material, MeshSource, Model,
    Node, NodeKind, Primitive, Scene, Transform,
};

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;
pub const CLEAR: [u8; 4] = [0x20, 0x20, 0x20, 0xff];

pub fn lit_cube(background: Background) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background,
        ..Environment::default()
    });
    scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        }),
    );
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -30.0, 0.0)),
            ..Transform::default()
        }),
    );
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Cube),
            material: Material::Default(DefaultMaterial {
                diffuse_color: Color::rgb(0x40 as f32 / 255.0, 0x80 as f32 / 255.0, 1.0),
                ..DefaultMaterial::default()
            }),
        }))
        .with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(30.0, 45.0, 0.0)),
            ..Transform::default()
        }),
    );
    scene
}

pub fn grey() -> Background {
    Background::Color(Color::rgb(
        0x20 as f32 / 255.0,
        0x20 as f32 / 255.0,
        0x20 as f32 / 255.0,
    ))
}
