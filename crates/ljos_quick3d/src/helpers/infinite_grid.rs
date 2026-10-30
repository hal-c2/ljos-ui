//! `InfiniteGrid`: a grid of lines on the floor of a scene, declared in a
//! `SceneEnvironment`.
//!
//! Drawn as lines out to a fixed number of intervals each way, which is
//! far enough to look like the floor goes on in a scene of the sizes the
//! grid is used for, but is an edge to a camera that flies past it.

use std::sync::Arc;

use ljos::item::ljos_3d::CustomGeometry;
use ljos::prelude::*;

use super::mesh::Built;
use crate::node::{Geometry, Object3D};

/// Intervals from the origin to the edge of the lines, each way.
const REACH: u32 = 100;

#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "InfiniteGrid", extends = Object3D)]
pub struct InfiniteGrid {
    #[qml(base)]
    object3d: Object3D,
    /// Whether the X axis (red) and the Z axis (blue) are drawn over the
    /// grid.
    #[qml(notify)]
    grid_axes: bool,
    /// The distance between two lines.
    #[qml(notify)]
    grid_interval: f64,
    #[qml(notify)]
    visible: bool,
    /// The lines made for the interval they are for; kept so the renderer
    /// is handed the same mesh while nothing changes.
    #[qml(skip)]
    made: Option<Made>,
}

#[derive(Debug, Clone)]
pub(crate) struct Made {
    interval: f64,
    pub grid: Arc<CustomGeometry>,
    pub x_axis: Arc<CustomGeometry>,
    pub z_axis: Arc<CustomGeometry>,
}

impl Default for InfiniteGrid {
    fn default() -> Self {
        InfiniteGrid {
            object3d: Object3D::default(),
            grid_axes: true,
            grid_interval: 1.0,
            visible: true,
            made: None,
        }
    }
}

/// The meshes of the visible grid `obj`, with whether its axes are drawn;
/// `None` for a grid that is not shown. Tracked.
pub(crate) fn read(e: &Engine, obj: ObjectId) -> Option<(Made, bool)> {
    if !e.is::<InfiniteGrid>(obj) {
        return None;
    }
    let visible: bool = ljos::item::prop(e, obj, InfiniteGrid::VISIBLE);
    let axes: bool = ljos::item::prop(e, obj, InfiniteGrid::GRID_AXES);
    let interval: f64 = ljos::item::prop(e, obj, InfiniteGrid::GRID_INTERVAL);
    if !visible || !(interval > 0.0) || !interval.is_finite() {
        return None;
    }
    let kept = e
        .with(obj, |g: &InfiniteGrid| g.made.clone())
        .ok()
        .flatten()
        .filter(|m| m.interval == interval);
    let made = kept.unwrap_or_else(|| {
        let made = make(interval as f32);
        let copy = made.clone();
        let _ = e.with_mut(obj, |g: &mut InfiniteGrid| g.made = Some(copy));
        made
    });
    Some((made, axes))
}

fn make(interval: f32) -> Made {
    let reach = REACH as f32 * interval;
    let mut grid = Built {
        primitive: Some(ljos::item::ljos_3d::PrimitiveType::Lines),
        ..Built::default()
    };
    let line = |b: &mut Built, a: [f32; 3], z: [f32; 3]| {
        let first = b.vertex(a, [0.0, 1.0, 0.0], [0.0, 0.0]);
        let second = b.vertex(z, [0.0, 1.0, 0.0], [0.0, 0.0]);
        b.indices.extend([first, second]);
    };
    let axis = |from: [f32; 3], to: [f32; 3]| {
        let mut b = Built {
            primitive: Some(ljos::item::ljos_3d::PrimitiveType::Lines),
            ..Built::default()
        };
        line(&mut b, from, to);
        b
    };
    let (x_axis, z_axis) = (
        axis([-reach, 0.0, 0.0], [reach, 0.0, 0.0]),
        axis([0.0, 0.0, -reach], [0.0, 0.0, reach]),
    );
    // The lines through the origin are the axes' meshes, drawn grey when
    // the axes are not: left out here, because a line drawn twice in one
    // plane is a tie in depth and the first drawn would win.
    for i in (-(REACH as i32)..=REACH as i32).filter(|&i| i != 0) {
        let at = i as f32 * interval;
        line(&mut grid, [-reach, 0.0, at], [reach, 0.0, at]);
        line(&mut grid, [at, 0.0, -reach], [at, 0.0, reach]);
    }
    Made {
        interval: f64::from(interval),
        grid: snapshot(grid),
        x_axis: snapshot(x_axis),
        z_axis: snapshot(z_axis),
    }
}

fn snapshot(b: Built) -> Arc<CustomGeometry> {
    let mut g = Geometry::default();
    b.into_geometry(&mut g);
    g.into_snapshot()
}
