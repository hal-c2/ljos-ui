//! `View3D.pick` and its kin, and the `PickResult` they give.
//!
//! A pick reads the scene afresh, untracked, so a binding that calls one
//! does not follow every node of the scene; the scene is the one the
//! view draws, so a model is picked where it is drawn.

use std::collections::HashMap;
use std::sync::Arc;

use ljos::item::ljos_3d::pick::{self, Hit, Ray, Viewport};
use ljos::item::ljos_3d::{NodeId, Scene};
use ljos::item::{self, ljos_3d::math::Vec3};
use ljos::prelude::*;

use crate::space::{from_value, to_value};
use crate::{Model, View3D};

fn pick_result_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "HitType",
        vec![
            ("Null", hit_type::NULL),
            ("Model", hit_type::MODEL),
            ("Item", hit_type::ITEM),
        ],
    )]
}

pub(crate) mod hit_type {
    pub const NULL: i64 = 0;
    pub const MODEL: i64 = 1;
    pub const ITEM: i64 = 2;
}

/// `PickResult`: what a pick met. A pick gives a plain value with these
/// members; the type is there for its `HitType` enum.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "PickResult",
    extends = QtObject,
    enums = pick_result_enums,
    uncreatable
)]
pub struct PickResult {
    #[qml(base)]
    pub object: QtObject,
}

/// The view's scene, read now without tracking, and which object each of
/// its pickable models is.
struct Picking {
    scene: Scene,
    pickable: HashMap<NodeId, ObjectId>,
}

impl Picking {
    fn read(e: &Engine, view: ObjectId) -> Picking {
        e.untrack(|| {
            let (scene, ids) = crate::scene::build_with_ids(e, view);
            let pickable = ids
                .into_iter()
                .filter(|(obj, _)| e.is::<Model>(*obj) && item::prop(e, *obj, Model::PICKABLE))
                .map(|(obj, id)| (id, obj))
                .collect();
            Picking { scene, pickable }
        })
    }

    fn hits(&self, ray: Option<Ray>) -> Vec<Value> {
        let Some(ray) = ray else {
            return Vec::new();
        };
        pick::pick(&self.scene, &ray, |id| self.pickable.contains_key(&id))
            .iter()
            .filter_map(|h| Some(result(Some((self.pickable.get(&h.node)?, h)))))
            .collect()
    }

    fn viewport(&self, e: &Engine, view: ObjectId) -> Option<Viewport> {
        let (w, h) = crate::view::size(e, view);
        Viewport::of_scene(&self.scene, w, h)
    }
}

fn result(hit: Option<(&ObjectId, &Hit)>) -> Value {
    let v2 = |[x, y]: [f32; 2]| Value::Vector2d(ljos::ljos_engine::Vector2d { x, y });
    let Some((obj, h)) = hit else {
        let zero = to_value(Vec3::ZERO);
        return Value::map([
            ("objectHit", Value::Null),
            ("distance", Value::Real(0.0)),
            ("scenePosition", zero.clone()),
            ("position", zero.clone()),
            ("sceneNormal", zero.clone()),
            ("normal", zero),
            ("uvPosition", v2([0.0, 0.0])),
            ("instanceIndex", Value::Int(-1)),
            ("hitType", Value::Int(hit_type::NULL as i32)),
            ("itemHit", Value::Null),
        ]);
    };
    Value::map([
        ("objectHit", Value::Object(*obj)),
        ("distance", Value::Real(f64::from(h.distance))),
        ("scenePosition", to_value(h.scene_position)),
        ("position", to_value(h.position)),
        ("sceneNormal", to_value(h.scene_normal)),
        ("normal", to_value(h.normal)),
        ("uvPosition", v2(h.uv)),
        ("instanceIndex", Value::Int(-1)),
        ("hitType", Value::Int(hit_type::MODEL as i32)),
        ("itemHit", Value::Null),
    ])
}

fn first(mut hits: Vec<Value>) -> Value {
    if hits.is_empty() {
        result(None)
    } else {
        hits.swap_remove(0)
    }
}

fn ray_of(origin: &Value, direction: &Value) -> Option<Ray> {
    Ray::new(from_value(origin), from_value(direction))
}

#[qml_impl]
impl View3D {
    /// The nearest pickable model under a point of the view.
    #[qml(invokable)]
    fn pick(cx: &Cx, x: f64, y: f64) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let p = Picking::read(e, me);
        let ray = p.viewport(e, me).and_then(|v| v.ray(x as f32, y as f32));
        first(p.hits(ray))
    }

    /// Every pickable model under a point of the view, nearest first.
    #[qml(invokable, name = "pickAll")]
    fn pick_all(cx: &Cx, x: f64, y: f64) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let p = Picking::read(e, me);
        let ray = p.viewport(e, me).and_then(|v| v.ray(x as f32, y as f32));
        Value::List(Arc::new(p.hits(ray)))
    }

    /// The nearest pickable model on a ray from a scene position.
    #[qml(invokable, name = "rayPick")]
    fn ray_pick(cx: &Cx, origin: Value, direction: Value) -> Value {
        first(Picking::read(cx.engine, cx.this).hits(ray_of(&origin, &direction)))
    }

    #[qml(invokable, name = "rayPickAll")]
    fn ray_pick_all(cx: &Cx, origin: Value, direction: Value) -> Value {
        let hits = Picking::read(cx.engine, cx.this).hits(ray_of(&origin, &direction));
        Value::List(Arc::new(hits))
    }

    /// A scene position as a point of the view, its z the distance past
    /// the camera's near clip plane. The origin where it cannot be mapped.
    #[qml(invokable, name = "mapFrom3DScene")]
    fn map_from_3d_scene(cx: &Cx, scene: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let at = Picking::read(e, me)
            .viewport(e, me)
            .and_then(|v| v.from_scene(from_value(&scene)));
        to_value(at.unwrap_or(Vec3::ZERO))
    }

    #[qml(invokable, name = "mapTo3DScene")]
    fn map_to_3d_scene(cx: &Cx, view: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let at = Picking::read(e, me)
            .viewport(e, me)
            .and_then(|v| v.to_scene(from_value(&view)));
        to_value(at.unwrap_or(Vec3::ZERO))
    }
}
