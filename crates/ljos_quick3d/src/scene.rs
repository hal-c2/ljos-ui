//! A view's objects, walked into an `ljos_3d` scene.

use std::collections::HashMap;

use ljos::item::{self, ljos_3d};
use ljos::prelude::*;
use ljos_3d::{
    Background, Environment, Lighting, MeshSource, NodeId, NodeKind, Primitive, Scene, Transform,
};

use crate::node::{self, background, lighting};
use crate::{
    DefaultMaterial, DirectionalLight, Light, Model, Node, Object3D, OrthographicCamera,
    PerspectiveCamera, SceneEnvironment, View3D,
};

/// The scene of the view `view`, read now. Tracked.
pub(crate) fn build(e: &Engine, view: ObjectId) -> Scene {
    let mut walk = Walk {
        e,
        scene: Scene::new(),
        ids: HashMap::new(),
    };
    for o in item::resources(e, view) {
        walk.node(None, o);
    }
    if let Some(imported) = item::prop(e, view, View3D::IMPORT_SCENE) {
        walk.node(None, imported);
    }
    let env = item::prop(e, view, View3D::ENVIRONMENT)
        .map_or_else(Environment::default, |env| environment(e, env));
    walk.scene.set_environment(env);
    // A camera that is not in the scene leaves the choice to the scene:
    // its first camera.
    let camera = item::prop(e, view, View3D::CAMERA).and_then(|c| walk.ids.get(&c).copied());
    walk.scene.set_camera(camera);
    walk.scene
}

struct Walk<'e> {
    e: &'e Engine,
    scene: Scene,
    ids: HashMap<ObjectId, NodeId>,
}

impl Walk<'_> {
    fn node(&mut self, parent: Option<NodeId>, obj: ObjectId) {
        let e = self.e;
        // Materials, environments and anything else declared among the
        // nodes are not drawn by themselves.
        if !e.is::<Node>(obj) || self.ids.contains_key(&obj) {
            return;
        }
        let transform = Transform {
            position: node::vec3(item::prop(e, obj, Node::POSITION)),
            rotation: node::quat(e.get(obj, Node::ROTATION).unwrap_or(Quaternion {
                scalar: 1.0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            })),
            scale: node::vec3(e.get(obj, Node::SCALE).unwrap_or(Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            })),
        };
        let mut n = ljos_3d::Node::new(kind(e, obj)).with_transform(transform);
        n.visible = item::prop(e, obj, Node::VISIBLE);
        let id = self.scene.add(parent, n);
        self.ids.insert(obj, id);
        for child in list(e, obj, Object3D::DATA.index) {
            self.node(Some(id), child);
        }
    }
}

fn kind(e: &Engine, obj: ObjectId) -> NodeKind {
    if e.is::<Model>(obj) {
        return model(e, obj).map_or(NodeKind::Group, NodeKind::Model);
    }
    if e.is::<PerspectiveCamera>(obj) {
        return NodeKind::Camera(ljos_3d::Camera::Perspective {
            fov_y_degrees: item::prop(e, obj, PerspectiveCamera::FIELD_OF_VIEW) as f32,
            clip_near: item::prop(e, obj, PerspectiveCamera::CLIP_NEAR) as f32,
            clip_far: item::prop(e, obj, PerspectiveCamera::CLIP_FAR) as f32,
        });
    }
    if e.is::<OrthographicCamera>(obj) {
        return NodeKind::Camera(ljos_3d::Camera::Orthographic {
            clip_near: item::prop(e, obj, OrthographicCamera::CLIP_NEAR) as f32,
            clip_far: item::prop(e, obj, OrthographicCamera::CLIP_FAR) as f32,
            horizontal_magnification: item::prop(
                e,
                obj,
                OrthographicCamera::HORIZONTAL_MAGNIFICATION,
            ) as f32,
            vertical_magnification: item::prop(e, obj, OrthographicCamera::VERTICAL_MAGNIFICATION)
                as f32,
        });
    }
    if e.is::<DirectionalLight>(obj) {
        return NodeKind::Light(ljos_3d::Light::Directional {
            color: color(item::prop(e, obj, Light::COLOR)),
            ambient_color: color(item::prop(e, obj, Light::AMBIENT_COLOR)),
            brightness: item::prop(e, obj, Light::BRIGHTNESS) as f32,
        });
    }
    NodeKind::Group
}

/// `None` for a mesh this renderer does not build yet: the node still
/// places its children.
fn model(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Model> {
    let source = item::prop(e, obj, Model::SOURCE);
    // A relative `#Cube` comes resolved against the file's URL; the
    // built-in meshes are named by the fragment.
    let name = source
        .0
        .as_str()
        .rfind('#')
        .map(|i| &source.0.as_str()[i..])?;
    let mesh = MeshSource::Primitive(Primitive::from_source(name)?);
    // A built-in mesh has one part, drawn with the first material.
    let material = list(e, obj, Model::MATERIALS.index)
        .into_iter()
        .next()
        .and_then(|m| material(e, m))
        .unwrap_or_default();
    Some(ljos_3d::Model { mesh, material })
}

fn material(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Material> {
    if !e.is::<DefaultMaterial>(obj) {
        return None;
    }
    Some(ljos_3d::Material::Default(ljos_3d::DefaultMaterial {
        diffuse_color: color(item::prop(e, obj, DefaultMaterial::DIFFUSE_COLOR)),
        lighting: if item::prop(e, obj, DefaultMaterial::LIGHTING) == lighting::NO_LIGHTING {
            Lighting::None
        } else {
            Lighting::Fragment
        },
    }))
}

fn environment(e: &Engine, env: ObjectId) -> Environment {
    if !e.is::<SceneEnvironment>(env) {
        return Environment::default();
    }
    let background = match item::prop(e, env, SceneEnvironment::BACKGROUND_MODE) {
        background::COLOR => {
            Background::Color(color(item::prop(e, env, SceneEnvironment::CLEAR_COLOR)))
        }
        // The sky boxes are not drawn yet: the 2D scene shows through.
        _ => Background::Transparent,
    };
    Environment { background }
}

fn color(c: Color) -> ljos_3d::Color {
    let [r, g, b, a] = c.to_rgba_f();
    ljos_3d::Color {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: a as f32,
    }
}

/// An object list property's live objects; tracked.
fn list(e: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match e.read(obj, p) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| e.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}
