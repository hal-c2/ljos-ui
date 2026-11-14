//! An instanced model's copies as a frame draws them, and what is kept of
//! them from one frame to the next.
//!
//! A copy is at `global * entry * global⁻¹ * world`
//! ([`instance_worlds`](crate::instancing::instance_worlds)). The rows the
//! copies are drawn from hold the entries as the table has them; `global`
//! and its inverse go in the model's uniforms, in front of a row and behind
//! it (lit.wgsl's `copy_place`). A node above the model that moves then
//! changes one model's uniforms rather than a row per copy, and the rows are
//! written when the table changes. What a frame works out from all the
//! copies is kept in the table's space for the same reason: the sphere round
//! them, which only `global` takes into the scene, and the order copies that
//! blend are drawn in, sorted again only when the camera or `global` moved.

use std::collections::HashMap;

use crate::gpu_extra::{self, InstanceRow};
use crate::instancing::{Instance, Instancing};
use crate::math::{Mat4, Vec3};
use crate::scene::NodeId;

/// What is kept of each instanced model a target draws.
#[derive(Default)]
pub(crate) struct HeldCopies {
    held: HashMap<NodeId, Held>,
    frame: u64,
}

#[derive(Default)]
struct Held {
    /// The frame it was last drawn in: one not drawn in the last is let go.
    frame: u64,
    bounds: Option<(BoundsKey, (Vec3, f32))>,
    order: Option<(OrderKey, Vec<u32>)>,
}

/// The table, how many of it are drawn, and the spheres round the model in
/// the table's space, to the bit.
type BoundsKey = (u64, usize, Vec<[u32; 4]>);
/// The table, how many are drawn, where the table's space is seen from, and
/// a copy's origin in it, to the bit.
type OrderKey = (u64, usize, [[u32; 4]; 4], [u32; 3]);

/// An instanced model's copies this frame.
pub(crate) struct Copies {
    /// What the model's uniforms put in front of a row and behind it.
    pub(crate) global: Mat4,
    pub(crate) back: Mat4,
    /// The sphere round them all, in the scene.
    pub(crate) sphere: (Vec3, f32),
}

/// How a model's copies are pushed: from what a frame knows of the model.
pub(crate) struct Model<'a> {
    pub(crate) id: NodeId,
    pub(crate) instancing: &'a Instancing,
    /// [`Instancing::global`].
    pub(crate) global: Mat4,
    /// The spheres round the model's mesh in the table's space: placed by
    /// [`Instancing::local`], or by a skin's joints taken out of `global`.
    pub(crate) spheres: &'a [(Vec3, f32)],
    /// Where the model's origin is in the table's space.
    pub(crate) origin: Vec3,
    /// A copy's alpha is 1 unless the copies blend.
    pub(crate) opaque: bool,
    /// Where the scene is seen from, for copies drawn from the farthest.
    pub(crate) view: Option<&'a Mat4>,
}

impl HeldCopies {
    /// Starts a frame.
    pub(crate) fn begin(&mut self) {
        self.frame += 1;
    }

    /// Ends a frame: what is held of a model not drawn in it is let go.
    pub(crate) fn end(&mut self) {
        let frame = self.frame;
        self.held.retain(|_, h| h.frame == frame);
    }

    /// Pushes `model`'s copies onto `rows`, in the order they are drawn;
    /// `None` when there are none.
    pub(crate) fn push(
        &mut self,
        model: &Model<'_>,
        rows: &mut Vec<InstanceRow>,
    ) -> Option<Copies> {
        let entries = model.instancing.drawn();
        if entries.is_empty() {
            return None;
        }
        let held = self.held.entry(model.id).or_default();
        held.frame = self.frame;
        let table = model.instancing.table.id();
        let row = |e: &Instance| {
            let mut color = e.color;
            if model.opaque {
                color[3] = 1.0;
            }
            InstanceRow::new(&e.transform, color, e.custom)
        };
        // Sorting opaque copies changes no picture: the depth test has the
        // last word on them.
        match model.view.filter(|_| !model.opaque) {
            Some(view) => {
                let to_view = *view * model.global;
                let key = (table, entries.len(), bits4x4(&to_view), bits3(model.origin));
                if held.order.as_ref().is_none_or(|(k, _)| *k != key) {
                    let depth = |e: &Instance| {
                        -to_view
                            .transform_point(e.transform.transform_point(model.origin))
                            .z
                    };
                    let mut order: Vec<(f32, u32)> = entries
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (depth(e), i as u32))
                        .collect();
                    // Stable, so that copies as far as each other keep the
                    // table's order whatever order they were in before.
                    order.sort_by(|a, b| b.0.total_cmp(&a.0));
                    held.order = Some((key, order.into_iter().map(|(_, i)| i).collect()));
                }
                let (_, order) = held.order.as_ref().expect("sorted above");
                rows.extend(order.iter().map(|&i| row(&entries[i as usize])));
            }
            None => {
                held.order = None;
                rows.extend(entries.iter().map(row));
            }
        }
        let key = (
            table,
            entries.len(),
            model
                .spheres
                .iter()
                .map(|&(c, r)| bits_sphere(c, r))
                .collect(),
        );
        if held.bounds.as_ref().is_none_or(|(k, _)| *k != key) {
            let all = entries.iter().flat_map(|e| {
                let m = &e.transform;
                let scale = crate::gpu::largest_scale(m);
                model
                    .spheres
                    .iter()
                    .map(move |&(c, r)| (m.transform_point(c), r * scale))
            });
            held.bounds = Some((key, gpu_extra::union(all).unwrap_or_default()));
        }
        let (_, (centre, radius)) = held.bounds.as_ref().expect("found above");
        Some(Copies {
            global: model.global,
            back: model.global.inverse().unwrap_or(Mat4::IDENTITY),
            sphere: (
                model.global.transform_point(*centre),
                radius * crate::gpu::largest_scale(&model.global),
            ),
        })
    }
}

fn bits3(v: Vec3) -> [u32; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

fn bits_sphere(c: Vec3, r: f32) -> [u32; 4] {
    [c.x.to_bits(), c.y.to_bits(), c.z.to_bits(), r.to_bits()]
}

fn bits4x4(m: &Mat4) -> [[u32; 4]; 4] {
    m.0.map(|c| c.map(f32::to_bits))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::instancing::InstanceTable;
    use crate::math::Quat;

    fn at(x: f32, z: f32, alpha: f32) -> Instance {
        Instance {
            transform: Mat4::from_trs(Vec3::new(x, 0.0, z), Quat::IDENTITY, Vec3::ONE),
            color: [1.0, 1.0, 1.0, alpha],
            ..Instance::default()
        }
    }

    fn model<'a>(
        instancing: &'a Instancing,
        global: Mat4,
        spheres: &'a [(Vec3, f32)],
        view: Option<&'a Mat4>,
        opaque: bool,
    ) -> Model<'a> {
        Model {
            id: NodeId::default(),
            instancing,
            global,
            spheres,
            origin: Vec3::ZERO,
            opaque,
            view,
        }
    }

    #[test]
    fn the_rows_are_the_table_s_whatever_holds_it() {
        let instancing = Instancing::new(Arc::new(InstanceTable::new(vec![
            at(10.0, 0.0, 0.5),
            at(-10.0, 0.0, 0.5),
        ])));
        let spheres = [(Vec3::ZERO, 1.0)];
        let mut held = HeldCopies::default();
        let mut rows = |global: Mat4| {
            let mut rows = Vec::new();
            held.begin();
            let copies = held.push(&model(&instancing, global, &spheres, None, true), &mut rows);
            held.end();
            (rows, copies.unwrap())
        };
        let (still, _) = rows(Mat4::IDENTITY);
        let turned = Mat4::from_trs(
            Vec3::new(5.0, 0.0, 0.0),
            Quat::from_euler_degrees(Vec3::new(0.0, 90.0, 0.0)),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let (moved, copies) = rows(turned);
        assert!(still == moved, "only the uniforms say where the table is");
        assert_eq!(still[0].matrix(), at(10.0, 0.0, 1.0).transform);
        // Its alpha is 1: these copies do not blend.
        assert!(still[0] == InstanceRow::new(&at(10.0, 0.0, 1.0).transform, [1.0; 4], [0.0; 4]));
        // The sphere round the copies goes where they do: round the box
        // 11 either side of the middle and 1 across, doubled and moved to
        // x = 5.
        let (centre, radius) = copies.sphere;
        assert!(
            (centre - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-4,
            "{centre:?}"
        );
        assert!((radius - 2.0 * 123f32.sqrt()).abs() < 1e-3, "{radius}");
    }

    #[test]
    fn copies_that_blend_are_drawn_from_the_farthest_seen_from_where_the_camera_is() {
        let instancing = Instancing::new(Arc::new(InstanceTable::new(vec![
            at(0.0, 10.0, 0.5),
            at(0.0, -10.0, 0.5),
            at(0.0, 0.0, 0.5),
        ])));
        let spheres = [(Vec3::ZERO, 1.0)];
        let mut held = HeldCopies::default();
        let mut order = |global: Mat4, view: Mat4, opaque: bool| {
            let mut rows = Vec::new();
            held.begin();
            held.push(
                &model(&instancing, global, &spheres, Some(&view), opaque),
                &mut rows,
            );
            held.end();
            rows.iter()
                .map(|r| r.matrix().translation().z)
                .collect::<Vec<_>>()
        };
        // A camera at z = 100 looking down -z: the view takes z off it.
        let front = Mat4::from_trs(Vec3::new(0.0, 0.0, -100.0), Quat::IDENTITY, Vec3::ONE);
        assert_eq!(order(Mat4::IDENTITY, front, false), [-10.0, 0.0, 10.0]);
        // Turned half round by what holds them, the far one is the near.
        let half = Mat4::from_trs(
            Vec3::ZERO,
            Quat::from_euler_degrees(Vec3::new(0.0, 180.0, 0.0)),
            Vec3::ONE,
        );
        assert_eq!(order(half, front, false), [10.0, 0.0, -10.0]);
        // Opaque copies are drawn as the table has them.
        assert_eq!(order(half, front, true), [10.0, -10.0, 0.0]);
    }
}
