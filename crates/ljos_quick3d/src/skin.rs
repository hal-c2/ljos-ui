//! `Skin`, `Skeleton` and `Joint`: the nodes a skinned model's vertices
//! follow, by the joint indices and weights its geometry gives them.
//!
//! A model names its joints by a `Skin` (any nodes, in index order) or by
//! a `Skeleton` of `Joint`s, each with its index. The scene's skin is made
//! once every node of the view has its place, as a joint may be declared
//! after the model it bends.

use std::cell::RefCell;
use std::sync::Arc;

use ljos::item::ljos_3d;
use ljos::prelude::*;
use ljos_3d::math::Mat4;

use crate::{Node, Object3D};

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
    #[qml(skip)]
    pub(crate) poses: Poses,
}

/// The matrices an object's `inverseBindPoses` list was read as, kept
/// while that list is the one it holds. The poses are written once, when
/// the mesh is bound, and each is sixteen reads of a script's value; a
/// model is read again whenever its joints move.
#[derive(Debug, Default)]
pub(crate) struct Poses(RefCell<Option<Kept>>);

/// A list, and the matrices it was read as.
type Kept = (Arc<Vec<Value>>, Arc<[Mat4]>);

impl Poses {
    fn kept(&self, list: &Arc<Vec<Value>>) -> Option<Arc<[Mat4]>> {
        let kept = self.0.borrow();
        let (read, poses) = kept.as_ref()?;
        Arc::ptr_eq(read, list).then(|| poses.clone())
    }
}

/// `o`'s inverse bind poses, the list at `index`, read again only when
/// the list is another one. `poses` is where `T` keeps them.
pub(crate) fn poses<T: QmlType>(
    e: &Engine,
    o: ObjectId,
    index: PropIndex,
    poses: fn(&T) -> &Poses,
) -> Vec<Mat4> {
    let Value::List(list) = e.read(o, index) else {
        return Vec::new();
    };
    if let Ok(Some(kept)) = e.with(o, |t: &T| poses(t).kept(&list)) {
        return kept.to_vec();
    }
    #[cfg(test)]
    counts::POSE_READS.with(|c| c.set(c.get() + 1));
    // Outside the borrow of `o`: a script's value is read through the
    // engine.
    let read: Arc<[Mat4]> = list.iter().map(|m| matrix(e, m)).collect();
    let _ = e.with(o, |t: &T| {
        *poses(t).0.borrow_mut() = Some((list, read.clone()))
    });
    read.to_vec()
}

#[cfg(test)]
pub(crate) mod counts {
    use std::cell::Cell;

    thread_local! {
        /// Inverse bind pose lists read from their script values.
        pub(crate) static POSE_READS: Cell<usize> = const { Cell::new(0) };
    }
}

/// A `Qt.matrix4x4`'s members, row by row.
const MEMBERS: [&str; 16] = [
    "m11", "m12", "m13", "m14", "m21", "m22", "m23", "m24", "m31", "m32", "m33", "m34", "m41",
    "m42", "m43", "m44",
];

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

/// A `Qt.matrix4x4` value (members `m11`..`m44`, row by row, or sixteen
/// numbers); the identity for anything else.
pub(crate) fn matrix(e: &Engine, v: &Value) -> Mat4 {
    let mut rows = [0.0f32; 16];
    match v {
        Value::List(l) if l.len() == 16 => {
            for (o, x) in rows.iter_mut().zip(l.iter()) {
                *o = x.to_number() as f32;
            }
        }
        Value::Js(_) | Value::Map(_) => {
            for (o, name) in rows.iter_mut().zip(MEMBERS) {
                match member(e, v, name) {
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
