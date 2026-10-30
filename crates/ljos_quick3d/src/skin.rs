//! `Skin`, `Skeleton` and `Joint`: the nodes a skinned model's vertices
//! follow, by the joint indices and weights its geometry gives them.
//!
//! A model names its joints by a `Skin` (any nodes, in index order) or by
//! a `Skeleton` of `Joint`s, each with its index. The scene's skin is made
//! once every node of the view has its place, as a joint may be declared
//! after the model it bends.

use std::collections::HashMap;

use ljos::item::{self, ljos_3d};
use ljos::prelude::*;
use ljos_3d::NodeId;
use ljos_3d::math::Mat4;

use crate::{Model, Node, Object3D};

/// `Skin`: a skinned model's joints, any nodes, with the inverse of each
/// one's transform when the mesh was bound to it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Skin", extends = Object3D)]
pub struct Skin {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify, list)]
    pub joints: QmlList,
    /// `Qt.matrix4x4` values; a missing one is the identity.
    #[qml(notify)]
    pub inverse_bind_poses: Value,
}

/// `Skeleton`: the top of a tree of `Joint`s.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Skeleton", extends = Node)]
pub struct Skeleton {
    #[qml(base)]
    pub node: Node,
}

/// `Joint`: a node of a skeleton, the `index`th a model's vertices name.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Joint", extends = Node)]
pub struct Joint {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub index: i32,
    #[qml(notify, object = "Skeleton")]
    pub skeleton_root: Option<ObjectId>,
}

/// The skin of the model `obj`, its joints as the scene's nodes; `None`
/// for a model with neither a skin nor a skeleton, or one whose joints
/// are not all in the scene. Tracked.
pub(crate) fn skin(
    e: &Engine,
    obj: ObjectId,
    ids: &HashMap<ObjectId, NodeId>,
) -> Option<ljos_3d::Skin> {
    let (joints, poses) = if let Some(s) = item::prop(e, obj, Model::SKIN)
        && e.is::<Skin>(s)
    {
        (
            objects(e, s, Skin::JOINTS.index),
            e.read(s, Skin::INVERSE_BIND_POSES.index),
        )
    } else if let Some(s) = item::prop(e, obj, Model::SKELETON)
        && e.is::<Skeleton>(s)
    {
        (
            skeleton_joints(e, s),
            e.read(obj, Model::INVERSE_BIND_POSES.index),
        )
    } else {
        return None;
    };
    let joints = joints
        .into_iter()
        .map(|j| ids.get(&j?).copied())
        .collect::<Option<Vec<_>>>()?;
    let inverse_bind = match &poses {
        Value::List(l) => l.iter().map(|m| matrix(e, m)).collect(),
        _ => Vec::new(),
    };
    Some(ljos_3d::Skin {
        joints,
        inverse_bind,
    })
}

fn objects(e: &Engine, obj: ObjectId, p: PropIndex) -> Vec<Option<ObjectId>> {
    match e.read(obj, p) {
        Value::List(l) => l
            .iter()
            .map(|v| v.as_object().filter(|o| e.is_alive(*o)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The joints under `skeleton`, placed by their indices; an index no joint
/// has is a hole, which leaves the skin out.
fn skeleton_joints(e: &Engine, skeleton: ObjectId) -> Vec<Option<ObjectId>> {
    let mut found = Vec::new();
    let mut stack = vec![skeleton];
    while let Some(n) = stack.pop() {
        for child in objects(e, n, Object3D::DATA.index).into_iter().flatten() {
            if e.is::<Joint>(child) {
                let root = item::prop(e, child, Joint::SKELETON_ROOT);
                if root.is_none_or(|r| r == skeleton)
                    && let Ok(i) = usize::try_from(item::prop(e, child, Joint::INDEX))
                {
                    found.push((i, child));
                }
            }
            if e.is::<Node>(child) {
                stack.push(child);
            }
        }
    }
    let len = found.iter().map(|(i, _)| i + 1).max().unwrap_or(0);
    let mut joints = vec![None; len.min(ljos_3d::instancing::MAX_JOINTS)];
    for (i, j) in found {
        if let Some(slot) = joints.get_mut(i) {
            *slot = Some(j);
        }
    }
    joints
}

/// A `Qt.matrix4x4` value (members `m11`..`m44`, row by row, or sixteen
/// numbers); the identity for anything else.
fn matrix(e: &Engine, v: &Value) -> Mat4 {
    let mut rows = [0.0f32; 16];
    match v {
        Value::List(l) if l.len() == 16 => {
            for (o, x) in rows.iter_mut().zip(l.iter()) {
                *o = x.to_number() as f32;
            }
        }
        Value::Js(_) | Value::Map(_) => {
            for (i, o) in rows.iter_mut().enumerate() {
                let name = format!("m{}{}", i / 4 + 1, i % 4 + 1);
                match member(e, v, &name) {
                    Some(n) => *o = n as f32,
                    None => return Mat4::IDENTITY,
                }
            }
        }
        _ => return Mat4::IDENTITY,
    }
    Mat4(std::array::from_fn(|c| {
        std::array::from_fn(|r| rows[r * 4 + c])
    }))
}

fn member(e: &Engine, v: &Value, name: &str) -> Option<f64> {
    match v {
        Value::Map(m) => m
            .iter()
            .find(|(k, _)| k.as_str() == name)
            .map(|(_, x)| x.to_number()),
        _ => e.js_member(v, name).map(|x| x.to_number()),
    }
}
