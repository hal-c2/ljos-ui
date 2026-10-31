//! A graph of objects whose members are kept by their QML names: the
//! shape of the web's object graph, and what the mapping is tested with.

use std::collections::HashMap;

use super::{Prop3d, SceneSource, Type3d};
use crate::math::{Quat, Vec3};

/// A member's value in a [`MapGraph`].
#[derive(Clone, Debug, PartialEq)]
pub enum MapValue {
    /// A real, an integer or an enumeration's number.
    Number(f64),
    Bool(bool),
    /// sRGB `[r, g, b, a]`, each 0 to 1.
    Color([f32; 4]),
    Vector3d(Vec3),
    Quaternion(Quat),
    Url(String),
    Object(usize),
    Objects(Vec<usize>),
}

/// Objects, each a type and the members it was given, by name. A member
/// it was not given reads as `None`: the mapping takes Qt's default.
#[derive(Clone, Debug, Default)]
pub struct MapGraph {
    objects: Vec<(Type3d, HashMap<String, MapValue>)>,
}

impl MapGraph {
    pub fn new() -> Self {
        MapGraph::default()
    }

    /// Adds an object of type `ty` with no members given.
    pub fn add(&mut self, ty: Type3d) -> usize {
        self.objects.push((ty, HashMap::new()));
        self.objects.len() - 1
    }

    /// Gives `o`'s member `name`, as QML names it.
    pub fn set(&mut self, o: usize, name: &str, value: MapValue) -> &mut Self {
        if let Some((_, members)) = self.objects.get_mut(o) {
            members.insert(name.to_owned(), value);
        }
        self
    }

    /// Appends `child` to `parent`'s `data`.
    pub fn append(&mut self, parent: usize, child: usize) -> &mut Self {
        if let Some((_, members)) = self.objects.get_mut(parent) {
            match members
                .entry(Prop3d::Data.qml_name().to_owned())
                .or_insert_with(|| MapValue::Objects(Vec::new()))
            {
                MapValue::Objects(list) => list.push(child),
                other => *other = MapValue::Objects(vec![child]),
            }
        }
        self
    }

    fn get(&self, o: usize, p: Prop3d) -> Option<&MapValue> {
        self.objects.get(o)?.1.get(p.qml_name())
    }
}

impl SceneSource for MapGraph {
    type Object = usize;

    fn is(&self, o: usize, ty: Type3d) -> bool {
        self.objects.get(o).is_some_and(|(t, _)| t.is_a(ty))
    }

    fn number(&self, o: usize, p: Prop3d) -> Option<f64> {
        match self.get(o, p)? {
            MapValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    fn int(&self, o: usize, p: Prop3d) -> Option<i32> {
        self.number(o, p).map(|n| n as i32)
    }

    fn boolean(&self, o: usize, p: Prop3d) -> Option<bool> {
        match self.get(o, p)? {
            MapValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn color(&self, o: usize, p: Prop3d) -> Option<[f32; 4]> {
        match self.get(o, p)? {
            MapValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    fn vector3d(&self, o: usize, p: Prop3d) -> Option<Vec3> {
        match self.get(o, p)? {
            MapValue::Vector3d(v) => Some(*v),
            _ => None,
        }
    }

    fn quaternion(&self, o: usize, p: Prop3d) -> Option<Quat> {
        match self.get(o, p)? {
            MapValue::Quaternion(q) => Some(*q),
            _ => None,
        }
    }

    fn url(&self, o: usize, p: Prop3d) -> Option<String> {
        match self.get(o, p)? {
            MapValue::Url(u) => Some(u.clone()),
            _ => None,
        }
    }

    fn object(&self, o: usize, p: Prop3d) -> Option<usize> {
        match self.get(o, p)? {
            MapValue::Object(x) if *x < self.objects.len() => Some(*x),
            _ => None,
        }
    }

    fn objects(&self, o: usize, p: Prop3d) -> Vec<usize> {
        match self.get(o, p) {
            Some(MapValue::Objects(list)) => list
                .iter()
                .copied()
                .filter(|x| *x < self.objects.len())
                .collect(),
            // A list member given one object, as QML lets a binding do.
            Some(MapValue::Object(x)) if *x < self.objects.len() => vec![*x],
            _ => Vec::new(),
        }
    }

    fn authored(&self, o: usize, p: Prop3d) -> bool {
        self.get(o, p).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qml::{build, defaults, enums};
    use crate::scene::{Camera, CullMode, Lighting, Material, NodeKind, Tonemap};

    /// A View3D with a camera at z 300, a light and a cube with one
    /// DefaultMaterial; the material is returned for the test to dress.
    fn cube() -> (MapGraph, usize, usize, usize) {
        let mut g = MapGraph::new();
        let view = g.add(Type3d::View3D);
        let camera = g.add(Type3d::PerspectiveCamera);
        g.set(camera, "z", MapValue::Number(300.0));
        let light = g.add(Type3d::DirectionalLight);
        let model = g.add(Type3d::Model);
        g.set(model, "source", MapValue::Url("#Cube".into()));
        let material = g.add(Type3d::DefaultMaterial);
        g.set(model, "materials", MapValue::Objects(vec![material]));
        g.append(view, camera)
            .append(view, light)
            .append(view, model);
        (g, view, model, material)
    }

    #[test]
    fn a_member_left_out_takes_qts_default() {
        let (g, view, model, material) = cube();
        let built = build(&g, view);
        let id = built.ids[&model];
        let node = built.scene.node(id).unwrap();
        assert!(node.visible);
        assert_eq!(node.casts_shadows, defaults::model::CASTS_SHADOWS);
        assert_eq!(node.transform.scale, Vec3::ONE);
        let NodeKind::Model(m) = &node.kind else {
            panic!("the cube is a model")
        };
        let Material::Default(dm) = &m.material else {
            panic!("its material is a DefaultMaterial")
        };
        assert_eq!(dm.lighting, Lighting::Fragment);
        assert_eq!(dm.cull_mode, CullMode::Back);
        assert_eq!(dm.diffuse_color, crate::scene::Color::WHITE);
        let _ = material;
        let (camera, _) = built.scene.active_camera().unwrap();
        assert_eq!(
            camera,
            Camera::Perspective {
                fov_degrees: 60.0,
                fov_horizontal: false,
                clip_near: 10.0,
                clip_far: 10000.0,
                frustum_culling: false,
            }
        );
        assert_eq!(built.scene.environment().tonemap, Tonemap::Linear);
    }

    #[test]
    fn an_enumeration_maps_by_its_number() {
        let (mut g, view, model, material) = cube();
        g.set(
            material,
            "lighting",
            MapValue::Number(f64::from(enums::lighting::NO_LIGHTING)),
        );
        g.set(
            material,
            "cullMode",
            MapValue::Number(f64::from(enums::cull::NONE)),
        );
        let built = build(&g, view);
        let node = built.scene.node(built.ids[&model]).unwrap();
        let NodeKind::Model(m) = &node.kind else {
            panic!("the cube is a model")
        };
        assert_eq!(m.material.cull_mode(), CullMode::None);
        let Material::Default(dm) = &m.material else {
            panic!("its material is a DefaultMaterial")
        };
        assert_eq!(dm.lighting, Lighting::None);
    }

    #[test]
    fn a_node_placed_by_x_is_where_position_puts_it() {
        let (mut by_x, view, model, _) = cube();
        by_x.set(model, "x", MapValue::Number(60.0));
        let (mut by_position, ..) = cube();
        by_position.set(
            model,
            "position",
            MapValue::Vector3d(Vec3::new(60.0, 0.0, 0.0)),
        );
        let a = build(&by_x, view);
        let b = build(&by_position, view);
        assert!(a.scene.same_as(&b.scene));
        let at = a.scene.node(a.ids[&model]).unwrap().transform.position;
        assert_eq!(at, Vec3::new(60.0, 0.0, 0.0));
    }

    #[test]
    fn defaults_written_out_build_the_scene_left_out_builds() {
        let (left_out, view, model, material) = cube();
        let (mut written, ..) = cube();
        written
            .set(model, "visible", MapValue::Bool(defaults::node::VISIBLE))
            .set(model, "scale", MapValue::Vector3d(Vec3::ONE))
            .set(
                material,
                "lighting",
                MapValue::Number(f64::from(defaults::default_material::LIGHTING)),
            );
        assert!(
            build(&left_out, view)
                .scene
                .same_as(&build(&written, view).scene)
        );
    }

    #[test]
    fn a_type_is_what_it_derives_from() {
        assert!(Type3d::PerspectiveCamera.is_a(Type3d::Node));
        assert!(Type3d::ExtendedSceneEnvironment.is_a(Type3d::SceneEnvironment));
        assert!(!Type3d::Texture.is_a(Type3d::Node));
        assert!(!Type3d::View3D.is_a(Type3d::Object3D));
    }

    #[test]
    fn every_member_is_found_by_its_qml_name() {
        for &p in Prop3d::ALL {
            assert_eq!(Prop3d::from_qml_name(p.qml_name()), Some(p));
        }
    }
}
