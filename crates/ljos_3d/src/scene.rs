//! The retained scene: what a `View3D` shows, as plain data.
//!
//! The QML types write it and a renderer reads it; neither knows the
//! other. Each change bumps [`Scene::revision`], so a renderer that drew a
//! revision at a size draws nothing again until one of the two changes:
//! an idle 3D view costs the blit of the picture it already has.

use std::sync::atomic::{AtomicU64, Ordering};

use slotmap::{SlotMap, new_key_type};

use crate::math::{Mat4, Quat, Vec3};

new_key_type! {
    /// A node of one [`Scene`].
    pub struct NodeId;
}

/// A colour as QML gives it: sRGB, not premultiplied, each channel 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b, a: 1.0 }
    }

    /// The channels in linear light, where the lighting is summed.
    pub fn to_linear(self) -> [f32; 4] {
        fn lin(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        [lin(self.r), lin(self.g), lin(self.b), self.a]
    }
}

/// A node's own placement: `position`, `rotation` and `scale`, relative to
/// its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(self.position, self.rotation, self.scale)
    }
}

/// Qt Quick 3D's built-in meshes (`source: "#Cube"`), in scene units: the
/// cube is 100 a side and centred on its origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    Cube,
    Rectangle,
}

impl Primitive {
    /// From a `Model.source` such as `#Cube`; `None` for a name this
    /// renderer does not build yet.
    pub fn from_source(source: &str) -> Option<Self> {
        match source {
            "#Cube" => Some(Primitive::Cube),
            "#Rectangle" => Some(Primitive::Rectangle),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeshSource {
    Primitive(Primitive),
}

/// `DefaultMaterial`, as far as it is built: a diffuse colour lit by the
/// scene's lights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefaultMaterial {
    pub diffuse_color: Color,
}

impl Default for DefaultMaterial {
    fn default() -> Self {
        DefaultMaterial {
            diffuse_color: Color::WHITE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Material {
    Default(DefaultMaterial),
}

impl Default for Material {
    fn default() -> Self {
        Material::Default(DefaultMaterial::default())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub mesh: MeshSource,
    pub material: Material,
}

/// A camera looks down its local -Z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Camera {
    /// `PerspectiveCamera`; Qt's defaults are a 60° vertical field of view
    /// and clip planes at 10 and 10000.
    Perspective {
        fov_y_degrees: f32,
        clip_near: f32,
        clip_far: f32,
    },
    /// `OrthographicCamera`: one scene unit is one pixel of the view at a
    /// magnification of 1.
    Orthographic {
        clip_near: f32,
        clip_far: f32,
        horizontal_magnification: f32,
        vertical_magnification: f32,
    },
}

impl Camera {
    pub const PERSPECTIVE: Camera = Camera::Perspective {
        fov_y_degrees: 60.0,
        clip_near: 10.0,
        clip_far: 10000.0,
    };

    /// View space to clip space for a view `width` by `height` pixels.
    pub fn projection(&self, width: f32, height: f32) -> Mat4 {
        let aspect = if height > 0.0 { width / height } else { 1.0 };
        match *self {
            Camera::Perspective {
                fov_y_degrees,
                clip_near,
                clip_far,
            } => Mat4::perspective(fov_y_degrees.to_radians(), aspect, clip_near, clip_far),
            Camera::Orthographic {
                clip_near,
                clip_far,
                horizontal_magnification,
                vertical_magnification,
            } => Mat4::orthographic(
                width / horizontal_magnification.max(f32::EPSILON),
                height / vertical_magnification.max(f32::EPSILON),
                clip_near,
                clip_far,
            ),
        }
    }
}

/// A light shines along its local -Z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    Directional {
        color: Color,
        ambient_color: Color,
        brightness: f32,
    },
}

impl Light {
    pub const DIRECTIONAL: Light = Light::Directional {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
    };
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A plain `Node`: a transform for its children.
    Group,
    Model(Model),
    Camera(Camera),
    Light(Light),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub transform: Transform,
    pub visible: bool,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Node {
            kind,
            transform: Transform::default(),
            visible: true,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

/// `SceneEnvironment.backgroundMode`, as far as it is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Background {
    /// Qt's default: the 2D scene shows through where nothing is drawn.
    Transparent,
    Color(Color),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub background: Background,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            background: Background::Transparent,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    nodes: SlotMap<NodeId, Node>,
    roots: Vec<NodeId>,
    environment: Environment,
    camera: Option<NodeId>,
    revision: u64,
}

impl Scene {
    pub fn new() -> Self {
        Scene::default()
    }

    /// Changes whenever anything a renderer reads changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    // One counter for every scene: a view handed a scene built anew must
    // not take it for the one it drew because both counted to the same
    // number.
    fn touch(&mut self) {
        static REVISIONS: AtomicU64 = AtomicU64::new(1);
        self.revision = REVISIONS.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether `other` draws the same: the same nodes under the same ids,
    /// environment and camera, whatever the revisions. A host that builds
    /// its scene again from its items keeps the one it had when nothing
    /// changed, so the picture is not drawn again.
    pub fn same_as(&self, other: &Scene) -> bool {
        self.roots == other.roots
            && self.environment == other.environment
            && self.camera == other.camera
            && self.nodes.len() == other.nodes.len()
            && self
                .nodes
                .iter()
                .all(|(id, n)| other.nodes.get(id) == Some(n))
    }

    /// Adds `node` as the last child of `parent`, or as a root. A parent
    /// that is not in the scene makes it a root.
    pub fn add(&mut self, parent: Option<NodeId>, mut node: Node) -> NodeId {
        let parent = parent.filter(|p| self.nodes.contains_key(*p));
        node.parent = parent;
        node.children.clear();
        let id = self.nodes.insert(node);
        match parent {
            Some(p) => self.nodes[p].children.push(id),
            None => self.roots.push(id),
        }
        self.touch();
        id
    }

    /// Removes `id` and everything under it.
    pub fn remove(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        match node.parent {
            Some(p) => self.nodes[p].children.retain(|c| *c != id),
            None => self.roots.retain(|c| *c != id),
        }
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.remove(n) {
                stack.extend(node.children);
            }
        }
        if self.camera.is_some_and(|c| !self.nodes.contains_key(c)) {
            self.camera = None;
        }
        self.touch();
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// The node to change; the scene counts it as changed.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if self.nodes.contains_key(id) {
            self.touch();
        }
        self.nodes.get_mut(id)
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn set_environment(&mut self, environment: Environment) {
        if self.environment != environment {
            self.environment = environment;
            self.touch();
        }
    }

    /// `View3D.camera`. Without one the first camera in the tree, depth
    /// first in declaration order, is used, as Qt does.
    pub fn set_camera(&mut self, camera: Option<NodeId>) {
        if self.camera != camera {
            self.camera = camera;
            self.touch();
        }
    }

    /// Every visible node with its world transform, parents before
    /// children, in declaration order. An invisible node hides its subtree.
    pub fn walk(&self) -> Vec<(NodeId, &Node, Mat4)> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<(NodeId, Mat4)> = self
            .roots
            .iter()
            .rev()
            .map(|r| (*r, Mat4::IDENTITY))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let node = &self.nodes[id];
            if !node.visible {
                continue;
            }
            let world = parent * node.transform.matrix();
            stack.extend(node.children.iter().rev().map(|c| (*c, world)));
            out.push((id, node, world));
        }
        out
    }

    /// The camera the view is drawn through, with its world transform.
    pub fn active_camera(&self) -> Option<(Camera, Mat4)> {
        let walked = self.walk();
        let pick = |want: Option<NodeId>| {
            walked
                .iter()
                .find_map(|(id, node, world)| match &node.kind {
                    NodeKind::Camera(c) if want.is_none_or(|w| w == *id) => Some((*c, *world)),
                    _ => None,
                })
        };
        match self.camera {
            Some(c) => pick(Some(c)),
            None => pick(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_moves_with_its_parent() {
        let mut s = Scene::new();
        let parent = s.add(
            None,
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(100.0, 0.0, 0.0),
                ..Transform::default()
            }),
        );
        let child = s.add(
            Some(parent),
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(0.0, 50.0, 0.0),
                ..Transform::default()
            }),
        );
        let world = s
            .walk()
            .into_iter()
            .find(|(id, ..)| *id == child)
            .unwrap()
            .2;
        assert_eq!(world.translation(), Vec3::new(100.0, 50.0, 0.0));
    }

    #[test]
    fn scenes_built_apart_never_share_a_revision() {
        let mut a = Scene::new();
        let mut b = Scene::new();
        a.add(None, Node::new(NodeKind::Group));
        b.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
        assert_ne!(a.revision(), b.revision());
        assert!(!a.same_as(&b));
        let mut c = Scene::new();
        c.add(None, Node::new(NodeKind::Group));
        assert!(a.same_as(&c), "the same nodes draw the same");
    }

    #[test]
    fn every_change_is_a_new_revision() {
        let mut s = Scene::new();
        let a = s.add(None, Node::new(NodeKind::Group));
        let r = s.revision();
        s.node_mut(a).unwrap().visible = false;
        assert_ne!(s.revision(), r);
        let r = s.revision();
        s.set_environment(Environment::default());
        assert_eq!(
            s.revision(),
            r,
            "setting what is already there changes nothing"
        );
    }

    #[test]
    fn without_a_camera_set_the_first_one_is_used() {
        let mut s = Scene::new();
        s.add(None, Node::new(NodeKind::Group));
        s.add(None, Node::new(NodeKind::Camera(Camera::PERSPECTIVE)));
        let second = s.add(
            None,
            Node::new(NodeKind::Camera(Camera::Perspective {
                fov_y_degrees: 30.0,
                clip_near: 1.0,
                clip_far: 100.0,
            })),
        );
        assert_eq!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
        s.set_camera(Some(second));
        assert_ne!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
        s.remove(second);
        assert_eq!(s.active_camera().unwrap().0, Camera::PERSPECTIVE);
    }
}
