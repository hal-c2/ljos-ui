//! A 3D view in egui's own pass, on egui-wgpu's device.
//!
//! The host installs [`Scene3dResources`] once on the painter's renderer;
//! each frame a view paints a [`Scene3dPaint`] callback over its rect.
//! egui-wgpu runs the callback's `prepare` before its pass opens, and the
//! view's scene is drawn there into a picture of its own (only when its
//! revision or size changed); in the pass, `paint` draws that picture over
//! the rect egui has set as the viewport, clipped as any shape is. Nothing
//! here makes a device or a second queue: the 3D pass is one more pass in
//! egui's submission.

use std::collections::HashMap;
use std::sync::Arc;

use egui_wgpu::{CallbackResources, CallbackTrait, RenderState, ScreenDescriptor, wgpu};
use ljos_3d::{Renderer, Scene, Target};

/// Which view a picture belongs to; the host's own numbering (a render
/// node's key), stable while the view lives.
pub type ViewKey = u64;

/// The renderer and every view's picture, kept in egui-wgpu's callback
/// resources.
pub struct Scene3dResources {
    renderer: Renderer,
    format: wgpu::TextureFormat,
    targets: HashMap<ViewKey, Target>,
}

impl std::fmt::Debug for Scene3dResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene3dResources")
            .field("format", &self.format)
            .field("views", &self.targets.len())
            .finish_non_exhaustive()
    }
}

impl Scene3dResources {
    /// Puts the renderer on `state`'s painter, for the format it draws
    /// into. Again on the same painter it changes nothing.
    pub fn install(state: &RenderState) {
        let mut renderer = state.renderer.write();
        if renderer.callback_resources.contains::<Scene3dResources>() {
            return;
        }
        let mut r = Renderer::new(&state.device);
        r.use_adapter_formats(&state.adapter);
        r.prepare_blit(&state.device, state.target_format);
        renderer.callback_resources.insert(Scene3dResources {
            renderer: r,
            format: state.target_format,
            targets: HashMap::new(),
        });
    }

    /// Lets go of the pictures of views `keep` says are gone.
    pub fn retain(state: &RenderState, mut keep: impl FnMut(ViewKey) -> bool) {
        let mut renderer = state.renderer.write();
        if let Some(r) = renderer.callback_resources.get_mut::<Scene3dResources>() {
            r.targets.retain(|k, _| keep(*k));
        }
    }

    /// How long the GPU took to draw the views, added up over the readings
    /// that came back since the last time this was asked; `None` when none
    /// did, or the device cannot time ([`ljos_3d::timing_features`]).
    pub fn take_gpu_time(state: &RenderState) -> Option<std::time::Duration> {
        let mut renderer = state.renderer.write();
        let r = renderer.callback_resources.get_mut::<Scene3dResources>()?;
        r.targets
            .values_mut()
            .filter_map(Target::take_gpu_time)
            .reduce(|a, b| a + b)
    }

    /// [`Self::take_gpu_time`] of the passes after the views' scene passes
    /// (post-processing), which that time leaves out.
    pub fn take_gpu_post_time(state: &RenderState) -> Option<std::time::Duration> {
        let mut renderer = state.renderer.write();
        let r = renderer.callback_resources.get_mut::<Scene3dResources>()?;
        r.targets
            .values_mut()
            .filter_map(Target::take_gpu_post_time)
            .reduce(|a, b| a + b)
    }

    pub fn views(&self) -> usize {
        self.targets.len()
    }
}

/// One view's frame: its scene, as of this frame, at its size in device
/// pixels.
#[derive(Debug)]
pub struct Scene3dPaint {
    pub view: ViewKey,
    pub scene: Arc<Scene>,
    pub size_px: [u32; 2],
}

impl Scene3dPaint {
    /// The shape that paints `self` over `rect`, in points.
    pub fn shape(self, rect: egui::Rect) -> egui::Shape {
        egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(rect, self))
    }
}

impl CallbackTrait for Scene3dPaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Not installed: the host draws no 3D, and the view shows what is
        // under it, as with any painter that cannot draw a shape.
        let Some(r) = resources.get_mut::<Scene3dResources>() else {
            return Vec::new();
        };
        let Scene3dResources {
            renderer, targets, ..
        } = r;
        let size = [self.size_px[0].max(1), self.size_px[1].max(1)];
        let target = targets
            .entry(self.view)
            .and_modify(|t| {
                if t.size() != size {
                    *t = Target::new(device, renderer, size);
                }
            })
            .or_insert_with(|| Target::new(device, renderer, size));
        renderer.render(device, queue, encoder, &self.scene, target);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(r) = resources.get::<Scene3dResources>() else {
            return;
        };
        if let Some(target) = r.targets.get(&self.view) {
            r.renderer.blit(pass, r.format, target);
        }
    }
}
