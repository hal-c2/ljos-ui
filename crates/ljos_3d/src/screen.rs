//! `SCREEN_TEXTURE` and `SCREEN_MIP_TEXTURE`: the scene a view has drawn
//! before the materials that read it, as a texture those materials sample.
//!
//! The view's opaque models that do not read the screen are drawn here on
//! their own, ahead of the view: so neither a transparent model nor one that
//! reads the screen is in it, as the documentation says, and it holds light
//! as it is, ahead of the tone mapping, as Qt's does. A view whose materials
//! do not read the screen keeps no texture and draws nothing twice.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::custom::Screen;

const WGSL: &str = concat!(
    include_str!("shaders/tone.wgsl"),
    include_str!("shaders/screen.wgsl")
);

/// What the scene is drawn into where the device can, or else the picture's.
fn format(hdr: bool) -> wgpu::TextureFormat {
    if hdr {
        crate::post::HDR
    } else {
        crate::gpu::COLOR_FORMAT
    }
}

/// So that a material's group, made for one texture, is made again for
/// the next.
static GENERATIONS: AtomicU64 = AtomicU64::new(1);

/// The pipelines that copy the picture and make its mip levels.
pub(crate) struct Pipes {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    copy: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
}

/// One target's screen texture.
pub(crate) struct ScreenTexture {
    size: [u32; 2],
    levels: u32,
    /// Every level, as the materials sample it.
    pub(crate) view: wgpu::TextureView,
    /// A level each, as the copy and the mip passes draw into them.
    level_views: Vec<wgpu::TextureView>,
    /// Level `i` read by the pass that makes level `i + 1`.
    mip_groups: Vec<wgpu::BindGroup>,
    pub(crate) generation: u64,
}

impl Pipes {
    pub(crate) fn new(device: &wgpu::Device, hdr: bool) -> Self {
        let format = self::format(hdr);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d screen"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d screen"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d screen"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d screen"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Pipes {
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ljos_3d screen"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            copy: pipeline("fs_copy"),
            format,
            layout,
        }
    }

    fn group(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d screen"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Whether it draws half floats.
    pub(crate) fn hdr(&self) -> bool {
        self.format == crate::post::HDR
    }

    /// Makes `slot` a texture of `size` with mip levels where `screen`
    /// asks for them, unless it is one already; `None` frees it.
    pub(crate) fn fit(
        &self,
        device: &wgpu::Device,
        slot: &mut Option<ScreenTexture>,
        size: [u32; 2],
        screen: Screen,
    ) {
        let levels = match screen {
            Screen::None => {
                *slot = None;
                return;
            }
            Screen::Texture => 1,
            Screen::Mipmapped => 32 - size[0].max(size[1]).max(1).leading_zeros(),
        };
        if slot
            .as_ref()
            .is_some_and(|s| s.size == size && s.levels == levels)
        {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ljos_3d screen"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let level_views: Vec<_> = (0..levels)
            .map(|level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let mip_groups = level_views[..level_views.len() - 1]
            .iter()
            .map(|v| self.group(device, v))
            .collect();
        *slot = Some(ScreenTexture {
            size,
            levels,
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            level_views,
            mip_groups,
            generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
        });
    }

    /// Makes the levels below the first from it.
    pub(crate) fn make_levels(&self, encoder: &mut wgpu::CommandEncoder, screen: &ScreenTexture) {
        for (group, out) in screen.mip_groups.iter().zip(&screen.level_views[1..]) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.copy);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

impl ScreenTexture {
    /// What the scene is drawn into for it.
    pub(crate) fn first_level(&self) -> &wgpu::TextureView {
        &self.level_views[0]
    }
}
