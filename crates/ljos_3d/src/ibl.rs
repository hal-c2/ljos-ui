//! A light probe on the device: what a material's indirect light and the
//! sky box read, made once per probe image on the GPU.
//!
//! The image is laid on a cube with a full chain of mipmaps (the
//! environment), from which two maps are kept: the radiance prefiltered
//! for six roughnesses, a level each, and the irradiance each normal
//! gathers, 32 texels a side, as the lit shader's diffuse light varies
//! slowly. The environment and the uploaded image are dropped as soon as
//! the passes that read them are recorded.
//!
//! The prefiltered map is half floats, 512 texels a side at most: for a
//! 2048 by 1024 probe that is 512² × 6 faces × 8 bytes × 4/3 for the
//! mipmaps, 16.8 MB, which a rough surface's blur and the sky box share.
//! A side of 512 is about one image texel per cube texel for a 2k probe, so
//! the sky box is as sharp as the image; a larger image is read at that
//! size. Making the maps also needs the environment (as much again) and
//! the image (8 MB) until the GPU has run the passes.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use bytemuck::{Pod, Zeroable};

use crate::scene::LightProbe;
use crate::texture::{PixelEncoding, TexturePixels};

/// The format of the probe's maps: light beyond 1 needs more than a byte.
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const IRRADIANCE_SIDE: u32 = 32;
const MOST_SIDE: u32 = 512;
const LEAST_SIDE: u32 = 64;
const SPECULAR_LEVELS: u32 = 6;
const SPECULAR_SAMPLES: u32 = 64;
const IRRADIANCE_SAMPLES: u32 = 128;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    face: u32,
    encoding: u32,
    samples: u32,
    _pad: u32,
    roughness: f32,
    texel: f32,
    last_level: f32,
    _pad2: f32,
}

struct Probe {
    image: Weak<dyn TexturePixels>,
    group: wgpu::BindGroup,
    last_level: f32,
    bytes: u64,
}

/// What a frame binds for the probe it is lit by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    None,
    Probe(u64),
}

pub(crate) struct Ibl {
    /// Group 3 of the lit and sky pipelines.
    pub(crate) layout: wgpu::BindGroupLayout,
    /// Bound where there is no probe; made with the first frame, as the
    /// renderer is made without a queue.
    none: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    source_layout: wgpu::BindGroupLayout,
    cube_layout: wgpu::BindGroupLayout,
    equirect: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    specular: wgpu::RenderPipeline,
    irradiance: wgpu::RenderPipeline,
    probes: HashMap<u64, Probe>,
    params_stride: u64,
    max_side: u32,
}

fn cube_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn params_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn cube_texture(
    device: &wgpu::Device,
    label: &str,
    side: u32,
    levels: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 6,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn cube_view(texture: &wgpu::Texture, base: u32, levels: Option<u32>) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("ljos_3d probe cube"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        base_mip_level: base,
        mip_level_count: levels,
        ..Default::default()
    })
}

fn face_view(texture: &wgpu::Texture, level: u32, face: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("ljos_3d probe face"),
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: level,
        mip_level_count: Some(1),
        base_array_layer: face,
        array_layer_count: Some(1),
        ..Default::default()
    })
}

/// The side of the cube a probe image `width` texels around is laid on:
/// a quarter of the way around per face, a power of two within bounds.
fn cube_side(width: u32, max_side: u32) -> u32 {
    (width / 4)
        .max(1)
        .next_power_of_two()
        .clamp(LEAST_SIDE, MOST_SIDE)
        .min(max_side)
}

fn levels_of(side: u32) -> u32 {
    32 - side.leading_zeros()
}

/// Bytes a cube of `side` with `levels` levels of half floats takes.
fn cube_bytes(side: u32, levels: u32) -> u64 {
    (0..levels)
        .map(|l| {
            let s = u64::from((side >> l).max(1));
            s * s * 6 * 8
        })
        .sum()
}

impl Ibl {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d probe"),
            entries: &[cube_entry(0), cube_entry(1), sampler_entry(2)],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ljos_3d probe"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d probe image"),
            entries: &[
                params_entry(),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let cube_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d probe environment"),
            entries: &[params_entry(), cube_entry(2), sampler_entry(3)],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d probe"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ibl.wgsl").into()),
        });
        let pipeline = |layout: &wgpu::BindGroupLayout, entry: &str| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ljos_3d probe"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d probe"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let equirect = pipeline(&source_layout, "fs_equirect");
        let downsample = pipeline(&cube_layout, "fs_downsample");
        let specular = pipeline(&cube_layout, "fs_specular");
        let irradiance = pipeline(&cube_layout, "fs_irradiance");
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let size = std::mem::size_of::<Params>() as u64;
        Ibl {
            layout,
            none: None,
            sampler,
            source_layout,
            cube_layout,
            equirect,
            downsample,
            specular,
            irradiance,
            probes: HashMap::new(),
            params_stride: size.div_ceil(align) * align,
            max_side: device.limits().max_texture_dimension_2d,
        }
    }

    /// Makes what is bound without a probe; before any frame is drawn.
    pub(crate) fn ready(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.none.is_some() {
            return;
        }
        // Without a probe the shaders skip the maps, but a group is bound
        // all the same: black cubes of bytes, which every device has.
        let black = cube_texture(
            device,
            "ljos_3d no probe",
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &black,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0; 24],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
        );
        let black_view = cube_view(&black, 0, None);
        self.none = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d no probe"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&black_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&black_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    pub(crate) fn group(&self, bound: Bound) -> &wgpu::BindGroup {
        let none = self.none.as_ref().expect("ready before any frame");
        match bound {
            Bound::Probe(id) => self.probes.get(&id).map_or(none, |p| &p.group),
            Bound::None => none,
        }
    }

    /// The prefiltered map's last level for the probe, which a roughness
    /// of 1 reads.
    pub(crate) fn last_level(&self, bound: Bound) -> f32 {
        match bound {
            Bound::Probe(id) => self.probes.get(&id).map_or(0.0, |p| p.last_level),
            Bound::None => 0.0,
        }
    }

    /// Makes the maps of `probe` if they are not made yet, recording the
    /// passes into `encoder`, ahead of the scene's that read them.
    /// [`Bound::None`] for an image that holds no pixels.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        probe: &LightProbe,
    ) -> Bound {
        let image = &probe.texture.image;
        let id = image.id();
        if self.probes.contains_key(&id) {
            return Bound::Probe(id);
        }
        let Some((group, last_level, bytes)) = self.make(device, queue, encoder, &**image) else {
            return Bound::None;
        };
        self.probes.insert(
            id,
            Probe {
                image: Arc::downgrade(&image.0),
                group,
                last_level,
                bytes,
            },
        );
        image.uploaded();
        Bound::Probe(id)
    }

    /// Lets go of the maps of probes no scene holds any more.
    pub(crate) fn sweep(&mut self) {
        self.probes.retain(|_, p| p.image.strong_count() > 0);
    }

    /// Bytes the probes' maps take on the device.
    pub(crate) fn bytes(&self) -> u64 {
        self.probes.values().map(|p| p.bytes).sum()
    }

    fn make(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        image: &dyn TexturePixels,
    ) -> Option<(wgpu::BindGroup, f32, u64)> {
        let (w, h, pixels) = self.source_pixels(image)?;
        let encoding = match image.encoding() {
            PixelEncoding::Srgb8Premultiplied => 0,
            PixelEncoding::Rgbe => 1,
            PixelEncoding::Srgb8Straight => 2,
        };
        let source = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ljos_3d probe image"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());

        let side = cube_side(w, self.max_side);
        let env_levels = levels_of(side);
        let spec_levels = SPECULAR_LEVELS.min(env_levels);
        let target = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let env = cube_texture(
            device,
            "ljos_3d probe environment",
            side,
            env_levels,
            FORMAT,
            target,
        );
        let spec = cube_texture(
            device,
            "ljos_3d probe radiance",
            side,
            spec_levels,
            FORMAT,
            target,
        );
        let irr = cube_texture(
            device,
            "ljos_3d probe irradiance",
            IRRADIANCE_SIDE,
            1,
            FORMAT,
            target,
        );

        // Every pass's parameters in one buffer, a slot each.
        let texel = 4.0 * std::f32::consts::PI / (6.0 * (side * side) as f32);
        let last_env = (env_levels - 1) as f32;
        let mut params = Vec::new();
        let base = Params {
            face: 0,
            encoding,
            samples: 1,
            _pad: 0,
            roughness: 0.0,
            texel,
            last_level: last_env,
            _pad2: 0.0,
        };
        for face in 0..6 {
            params.push(Params { face, ..base });
        }
        for _ in 1..env_levels {
            for face in 0..6 {
                params.push(Params { face, ..base });
            }
        }
        for level in 0..spec_levels {
            let roughness = if spec_levels > 1 {
                level as f32 / (spec_levels - 1) as f32
            } else {
                0.0
            };
            for face in 0..6 {
                params.push(Params {
                    face,
                    samples: SPECULAR_SAMPLES,
                    roughness,
                    ..base
                });
            }
        }
        for face in 0..6 {
            params.push(Params {
                face,
                samples: IRRADIANCE_SAMPLES,
                ..base
            });
        }
        let stride = self.params_stride as usize;
        let mut bytes = vec![0u8; params.len() * stride];
        for (i, p) in params.iter().enumerate() {
            bytes[i * stride..i * stride + std::mem::size_of::<Params>()]
                .copy_from_slice(bytemuck::bytes_of(p));
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ljos_3d probe passes"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, &bytes);
        let slot = |i: usize| {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: (i * stride) as u64,
                size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
            })
        };
        let cube_group = |i: usize, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ljos_3d probe pass"),
                layout: &self.cube_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: slot(i),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let mut pass = |target: &wgpu::TextureView,
                        pipeline: &wgpu::RenderPipeline,
                        group: &wgpu::BindGroup| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d probe pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };

        let mut i = 0;
        for face in 0..6 {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ljos_3d probe pass"),
                layout: &self.source_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: slot(i),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&source_view),
                    },
                ],
            });
            pass(&face_view(&env, 0, face), &self.equirect, &group);
            i += 1;
        }
        // Each level from the one above: one bilinear tap of the level
        // twice the size is the mean of four texels.
        for level in 1..env_levels {
            let above = cube_view(&env, level - 1, Some(1));
            for face in 0..6 {
                let group = cube_group(i, &above);
                pass(&face_view(&env, level, face), &self.downsample, &group);
                i += 1;
            }
        }
        let whole = cube_view(&env, 0, None);
        for level in 0..spec_levels {
            for face in 0..6 {
                let group = cube_group(i, &whole);
                pass(&face_view(&spec, level, face), &self.specular, &group);
                i += 1;
            }
        }
        for face in 0..6 {
            let group = cube_group(i, &whole);
            pass(&face_view(&irr, 0, face), &self.irradiance, &group);
            i += 1;
        }

        let spec_view = cube_view(&spec, 0, None);
        let irr_view = cube_view(&irr, 0, None);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d probe"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&spec_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&irr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Some((
            group,
            (spec_levels - 1) as f32,
            cube_bytes(side, spec_levels) + cube_bytes(IRRADIANCE_SIDE, 1),
        ))
    }

    /// The image's pixels, every other one dropped until it fits the
    /// device, as the cube it is laid on is smaller still.
    fn source_pixels(&self, image: &dyn TexturePixels) -> Option<(u32, u32, Vec<u8>)> {
        let [w, h] = image.size();
        let len = w as usize * h as usize * 4;
        let mut data = Vec::new();
        image.with_pixels(&mut |p| {
            if p.len() == len {
                data = p.to_vec();
            }
        });
        if data.is_empty() || len == 0 {
            return None;
        }
        let step = w.max(h).div_ceil(self.max_side).max(1);
        if step == 1 {
            return Some((w, h, data));
        }
        let (nw, nh) = ((w / step).max(1), (h / step).max(1));
        let mut out = Vec::with_capacity(nw as usize * nh as usize * 4);
        for y in 0..nh {
            for x in 0..nw {
                let i = ((y * step) as usize * w as usize + (x * step) as usize) * 4;
                out.extend_from_slice(&data[i..i + 4]);
            }
        }
        Some((nw, nh, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_2k_probe_lies_on_a_cube_of_512() {
        assert_eq!(cube_side(2048, 8192), 512);
        assert_eq!(cube_side(64, 8192), 64);
        assert_eq!(cube_side(8192, 8192), 512);
        assert_eq!(levels_of(512), 10);
    }

    #[test]
    fn a_2k_probes_maps_take_under_17_megabytes() {
        let bytes = cube_bytes(512, SPECULAR_LEVELS) + cube_bytes(IRRADIANCE_SIDE, 1);
        assert_eq!(bytes, 16_822_272);
    }
}
