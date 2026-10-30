//! 3D views on the wgpu painter.
//!
//! B0 hands a `View3D` over as a paint callback holding a [`View3dPaint`]
//! (`QmlView::set_draws_scenes3d`, on for this painter). Here it becomes
//! egui-wgpu's own callback, which draws the scene on the window's device
//! in egui's pass: no second device, no read-back. A view no longer drawn
//! lets go of its picture in the same frame.

use std::collections::HashSet;
use std::sync::Arc;

use egui::epaint::{ClippedShape, Shape};
use ljos_egui::scene3d::{Scene3dPaint, Scene3dResources};
use ljos_render::scene3d::View3dPaint;

/// `shapes` with every 3D view drawn by egui-wgpu.
pub(crate) fn paint_views(
    painter: &egui_wgpu::winit::Painter,
    shapes: &[ClippedShape],
) -> Vec<ClippedShape> {
    // Before the first frame there is no device yet, and nothing to draw.
    let Some(state) = painter.render_state() else {
        return shapes.to_vec();
    };
    let mut drawn = HashSet::new();
    let out = shapes
        .iter()
        .map(|cs| match &cs.shape {
            Shape::Callback(cb) => match cb.callback.downcast_ref::<View3dPaint>() {
                Some(v) => {
                    drawn.insert(v.key);
                    let paint = Scene3dPaint {
                        view: v.key,
                        scene: Arc::clone(&v.scene),
                        size_px: v.size_px,
                    };
                    ClippedShape {
                        shape: paint.shape(cb.rect),
                        ..cs.clone()
                    }
                }
                None => cs.clone(),
            },
            _ => cs.clone(),
        })
        .collect();
    // The renderer is made with the first view a window shows: a window
    // without 3D never builds its pipelines.
    if !drawn.is_empty() {
        Scene3dResources::install(&state);
    }
    Scene3dResources::retain(&state, |k| drawn.contains(&k));
    out
}
