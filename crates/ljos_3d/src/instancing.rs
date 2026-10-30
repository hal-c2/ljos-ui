//! Many copies of one model drawn at once (`Model.instancing`), and meshes
//! bent by the joints of a skin (`Model.skin`).
//!
//! Both are data on a [`Node`](crate::Node) rather than on its
//! [`Model`](crate::Model): a model built anywhere else stays as it was.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::math::{Mat4, Vec3};
use crate::scene::{NodeId, Scene};

/// The most joints a skin moves its mesh by. 256 matrices are 16 KiB,
/// WebGL2's smallest uniform block; the documentation names no limit.
pub const MAX_JOINTS: usize = 256;

/// One copy of an instanced model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// Where the copy is, in the instancing's space.
    pub transform: Mat4,
    /// Linear and not premultiplied; it multiplies the material's colour.
    pub color: [f32; 4],
    /// Read only by custom materials.
    pub custom: [f32; 4],
}

impl Default for Instance {
    fn default() -> Self {
        Instance {
            transform: Mat4::IDENTITY,
            color: [1.0; 4],
            custom: [0.0; 4],
        }
    }
}

/// An instancing's entries as one value. A changed table is a new one with
/// a new id, so that comparing scenes compares two numbers rather than
/// thousands of matrices.
#[derive(Debug)]
pub struct InstanceTable {
    id: u64,
    entries: Vec<Instance>,
}

impl InstanceTable {
    pub fn new(entries: Vec<Instance>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        InstanceTable {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            entries,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn entries(&self) -> &[Instance] {
        &self.entries
    }
}

impl PartialEq for InstanceTable {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A model drawn once for each entry of a table.
#[derive(Clone, Debug, PartialEq)]
pub struct Instancing {
    pub table: Arc<InstanceTable>,
    /// The node whose space the entries are in (`instanceRoot`); `None`
    /// for the model itself.
    pub root: Option<NodeId>,
    /// Draws the copies from the farthest; among themselves only.
    pub depth_sort: bool,
    /// Blends the copies by their colours' alpha (`hasTransparency`).
    pub transparent: bool,
    /// How many entries are drawn (`instanceCountOverride`); `None` for
    /// all.
    pub count: Option<usize>,
}

impl Instancing {
    pub fn new(table: Arc<InstanceTable>) -> Self {
        Instancing {
            table,
            root: None,
            depth_sort: false,
            transparent: false,
            count: None,
        }
    }

    /// The entries drawn.
    pub fn drawn(&self) -> &[Instance] {
        let all = self.table.entries();
        &all[..self.count.map_or(all.len(), |n| n.min(all.len()))]
    }

    /// The matrix an entry's transform is put inside of, for the model
    /// `model` whose world transform is `world`: the root's place without
    /// its own turn and scale. The model's are then applied inside each
    /// entry's, so a copy is turned and scaled as the model is, and moved
    /// by the entry within the root's space.
    pub fn global(&self, scene: &Scene, model: NodeId, world: &Mat4) -> Mat4 {
        let root = self.root.unwrap_or(model);
        let root_world = if root == model {
            Some(*world)
        } else {
            scene.world(root)
        };
        let (Some(root_world), Some(node)) = (root_world, scene.node(root)) else {
            return *world;
        };
        let turn = Mat4::from_trs(Vec3::ZERO, node.transform.rotation, node.transform.scale);
        root_world * turn.inverse().unwrap_or(Mat4::IDENTITY)
    }
}

/// Every copy's world transform: `global * entry * global⁻¹ * world`.
pub fn instance_worlds<'a>(
    global: Mat4,
    world: Mat4,
    entries: &'a [Instance],
) -> impl Iterator<Item = Mat4> + 'a {
    let local = global.inverse().unwrap_or(Mat4::IDENTITY) * world;
    entries.iter().map(move |e| global * e.transform * local)
}

/// The joints a skinned mesh follows, in the order its vertices' joint
/// indices count them, and each joint's inverse transform when the mesh
/// was bound to it. A skinned model's own transform is not used.
#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub joints: Vec<NodeId>,
    /// Missing ones are the identity.
    pub inverse_bind: Vec<Mat4>,
}

impl Skin {
    /// What each joint does to a vertex of the mesh: its place now times
    /// its inverse bind pose. A joint no longer in the scene stays as it
    /// was bound.
    pub fn palette(&self, scene: &Scene) -> Vec<Mat4> {
        self.joints
            .iter()
            .take(MAX_JOINTS)
            .enumerate()
            .map(|(i, j)| {
                let ibm = self.inverse_bind.get(i).copied().unwrap_or(Mat4::IDENTITY);
                scene.world(*j).map_or(Mat4::IDENTITY, |w| w * ibm)
            })
            .collect()
    }
}

/// `p` moved by the joints `joints` by `weights`.
pub fn skin_point(palette: &[Mat4], joints: [u16; 4], weights: [f32; 4], p: Vec3) -> Vec3 {
    let mut out = Vec3::ZERO;
    for (j, w) in joints.iter().zip(weights) {
        if w == 0.0 {
            continue;
        }
        let m = palette.get(usize::from(*j)).copied().unwrap_or(Mat4::IDENTITY);
        out = out + (m.transform_vector(p) + m.translation()) * w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Quat;
    use crate::scene::{Node, NodeKind, Transform};

    #[test]
    fn a_copy_is_moved_in_the_models_parent_and_turned_as_the_model() {
        let mut scene = Scene::new();
        let model = scene.add(
            None,
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(0.0, 40.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::new(2.0, 2.0, 2.0),
            }),
        );
        let world = scene.world(model).unwrap();
        let entry = Instance {
            transform: Mat4::from_trs(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE),
            ..Instance::default()
        };
        let instancing = Instancing::new(Arc::new(InstanceTable::new(vec![entry])));
        let global = instancing.global(&scene, model, &world);
        let w = instance_worlds(global, world, instancing.drawn())
            .next()
            .unwrap();
        // Moved 10, not 20: the model's scale is inside the entry's move.
        assert_eq!(w.translation(), Vec3::new(10.0, 40.0, 0.0));
        assert_eq!(w.transform_vector(Vec3::new(1.0, 0.0, 0.0)).x, 2.0);
    }

    #[test]
    fn a_count_override_draws_fewer_and_never_more() {
        let table = Arc::new(InstanceTable::new(vec![Instance::default(); 3]));
        let mut i = Instancing::new(table);
        i.count = Some(1);
        assert_eq!(i.drawn().len(), 1);
        i.count = Some(10);
        assert_eq!(i.drawn().len(), 3);
    }

    #[test]
    fn a_skinned_point_follows_its_joints_by_their_weights() {
        let palette = [
            Mat4::IDENTITY,
            Mat4::from_trs(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE),
        ];
        let p = skin_point(&palette, [0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0], Vec3::ZERO);
        assert_eq!(p, Vec3::new(5.0, 0.0, 0.0));
    }
}
