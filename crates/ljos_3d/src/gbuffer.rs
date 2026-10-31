//! The surfaces screen-space reflections and global illumination read:
//! each pixel's normal, roughness, base colour and metalness, drawn by a
//! pass of their own with the lit shader's vertices, so that they lie
//! where the picture has them. Only a scene with either effect on draws
//! it; it writes the depth the other passes read too.

use std::collections::HashMap;

/// The normal in the scene's space and the roughness; past 1 the
/// roughness says the surface reflects nothing (2) or is not lit (3).
pub(crate) const NORMAL: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The base colour in linear light and the metalness.
pub(crate) const MATERIAL: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// What a pixel with nothing drawn holds: not lit, so neither effect
/// touches it.
pub(crate) const EMPTY: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 3.0,
};

/// By cull mode, a second vertex buffer, and whether the draw is a custom
/// material's, whose shading the pass cannot know.
type Key = (Option<wgpu::Face>, bool, bool);

#[derive(Default)]
pub(crate) struct Surfaces {
    pipelines: HashMap<Key, wgpu::RenderPipeline>,
    warned: bool,
}

impl Surfaces {
    /// Whether the device draws the pass: two pictures at once. Where it
    /// cannot, the effects are left out and that is said once.
    pub(crate) fn available(&mut self, device: &wgpu::Device) -> bool {
        let limits = device.limits();
        let fits =
            limits.max_color_attachments >= 2 && limits.max_color_attachment_bytes_per_sample >= 16;
        if !fits && !std::mem::replace(&mut self.warned, true) {
            tracing::warn!(
                "screen-space reflections and global illumination are left out: \
                 the device draws into fewer than two pictures at once"
            );
        }
        fits
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        vertex: &[Option<wgpu::VertexBufferLayout<'_>>],
        key: Key,
    ) {
        let (cull, extra, custom) = key;
        self.pipelines.entry(key).or_insert_with(|| {
            let target = |format| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d surfaces"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(if extra { "vs_extra" } else { "vs_main" }),
                    compilation_options: Default::default(),
                    buffers: vertex,
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::post::DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(if custom {
                        "fs_surface_unlit"
                    } else {
                        "fs_surface"
                    }),
                    compilation_options: Default::default(),
                    targets: &[target(NORMAL), target(MATERIAL)],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }

    pub(crate) fn pipeline(&self, key: Key) -> &wgpu::RenderPipeline {
        &self.pipelines[&key]
    }
}
