//! The device side of the author's programs: a CustomMaterial's pipelines
//! and the uniforms and samplers each draw binds, and the layouts and
//! pipelines of an Effect's passes.
//!
//! A program is compiled once per device and shared by every material,
//! pass and view that runs it; it is let go when no scene holds it. Its
//! uniform block starts with [`Header`], which the renderer fills, and
//! goes on with the author's properties where [`Program::write_values`]
//! puts them.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use bytemuck::{Pod, Zeroable};

use crate::custom::{
    Blend, CustomMaterial, EFFECT_FRAGMENT, EFFECT_VERTEX, HEADER_ROWS, MATERIAL_FRAGMENT,
    MATERIAL_FRAGMENT_STRAIGHT, MATERIAL_VERTEX, Program, ProgramKind,
};
use crate::maps::{self, Maps};
use crate::scene::CullMode;
use crate::texture::Sampling;

/// The binding of a program's uniform block; its textures follow it, each
/// with its sampler after it, and the depth is at 30 and 31.
const BLOCK: u32 = 32;
const DEPTH: u32 = 30;

/// What the renderer tells every program, in the block's first rows.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq)]
pub(crate) struct Header {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
    pub inverse_projection: [[f32; 4]; 4],
    pub view_projection: [[f32; 4]; 4],
    /// w is 1 for an orthographic camera.
    pub camera: [f32; 4],
    pub direction: [f32; 4],
    /// The clip planes and the frame's number.
    pub clip: [f32; 4],
    /// The input's size and the output's, in pixels.
    pub sizes: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Header>() == HEADER_ROWS * 16);

/// The block of `program` with `header` and `values`, ready to upload.
pub(crate) fn block(
    program: &Program,
    header: &Header,
    values: &[crate::custom::UniformValue],
) -> Vec<u8> {
    let mut bytes = vec![0; program.block_size()];
    bytes[..HEADER_ROWS * 16].copy_from_slice(bytemuck::bytes_of(header));
    program.write_values(values, &mut bytes);
    bytes
}

struct Compiled {
    program: Weak<Program>,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
}

/// What a custom material's pipeline differs by.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MaterialKey {
    pub program: u64,
    pub cull: CullMode,
    pub depth_write: bool,
    pub samples: u32,
    pub format: wgpu::TextureFormat,
    /// The author's factors, where both are given; the picture's own
    /// premultiplied blending otherwise.
    pub blend: Option<(Blend, Blend)>,
}

/// What one custom draw binds on a target, kept while the material keeps
/// its program and textures, so that a still material uploads nothing.
pub(crate) struct Slot {
    program: u64,
    textures: Vec<Option<(u64, Sampling)>>,
    buffer: wgpu::Buffer,
    pub(crate) group: wgpu::BindGroup,
    written: Vec<u8>,
}

pub(crate) struct Programs {
    compiled: HashMap<u64, Compiled>,
    materials: HashMap<MaterialKey, wgpu::RenderPipeline>,
    effects: HashMap<(u64, wgpu::TextureFormat), wgpu::RenderPipeline>,
    /// What a texture that is missing or disabled reads: opaque black.
    black: Option<wgpu::TextureView>,
    sampler: wgpu::Sampler,
    depth_sampler: wgpu::Sampler,
}

impl Programs {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Programs {
            compiled: HashMap::new(),
            materials: HashMap::new(),
            effects: HashMap::new(),
            black: None,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ljos_3d program"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            depth_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ljos_3d program depth"),
                ..Default::default()
            }),
        }
    }

    /// The linear, clamping sampler an effect reads its input and buffers
    /// with.
    pub(crate) fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub(crate) fn black(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> wgpu::TextureView {
        self.black
            .get_or_insert_with(|| {
                let level = (1, 1, vec![0, 0, 0, 255]);
                maps::upload(device, queue, "ljos_3d no texture", false, &[level])
            })
            .clone()
    }

    /// Lets go of the programs no scene holds any more, and their
    /// pipelines.
    pub(crate) fn sweep(&mut self) {
        let before = self.compiled.len();
        self.compiled.retain(|_, c| c.program.strong_count() > 0);
        if self.compiled.len() != before {
            let compiled = &self.compiled;
            self.materials
                .retain(|k, _| compiled.contains_key(&k.program));
            self.effects.retain(|k, _| compiled.contains_key(&k.0));
        }
    }

    fn compile(
        &mut self,
        device: &wgpu::Device,
        program: &Arc<Program>,
        shared: &[&wgpu::BindGroupLayout],
    ) {
        self.compiled.entry(program.id()).or_insert_with(|| {
            let mut entries = vec![wgpu::BindGroupLayoutEntry {
                binding: BLOCK,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }];
            for i in 0..texture_count(program) as u32 {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: BLOCK + 1 + 2 * i,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: BLOCK + 2 + 2 * i,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                });
            }
            if program.reads_depth() {
                // Depth is never filtered: it is read as the float it is.
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: DEPTH,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: DEPTH + 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                });
            }
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ljos_3d program"),
                entries: &entries,
            });
            // A material's block is its third group, beside the frame,
            // the model and the probe the lit shader binds; an effect's
            // is its only one.
            let groups: Vec<Option<&wgpu::BindGroupLayout>> = match program.kind() {
                ProgramKind::Effect => vec![Some(&layout)],
                _ => vec![
                    Some(shared[0]),
                    Some(shared[1]),
                    Some(&layout),
                    Some(shared[2]),
                ],
            };
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ljos_3d program"),
                bind_group_layouts: &groups,
                immediate_size: 0,
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ljos_3d program"),
                source: wgpu::ShaderSource::Wgsl(program.wgsl().into()),
            });
            Compiled {
                program: Arc::downgrade(program),
                layout,
                pipeline_layout,
                module,
            }
        });
    }

    /// Makes the pipeline of a custom material, unless it is there.
    /// `shared` are the frame's, the model's and the probe's layouts.
    pub(crate) fn material_pipeline(
        &mut self,
        device: &wgpu::Device,
        program: &Arc<Program>,
        key: MaterialKey,
        shared: [&wgpu::BindGroupLayout; 3],
        vertex: wgpu::VertexBufferLayout<'_>,
        depth_format: wgpu::TextureFormat,
    ) {
        if self.materials.contains_key(&key) {
            return;
        }
        self.compile(device, program, &shared);
        let c = &self.compiled[&key.program];
        let (entry, blend) = match key.blend {
            None => (
                MATERIAL_FRAGMENT,
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            ),
            Some((src, dst)) => {
                let component = wgpu::BlendComponent {
                    src_factor: factor(src),
                    dst_factor: factor(dst),
                    operation: wgpu::BlendOperation::Add,
                };
                (
                    MATERIAL_FRAGMENT_STRAIGHT,
                    wgpu::BlendState {
                        color: component,
                        alpha: component,
                    },
                )
            }
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ljos_3d custom material"),
            layout: Some(&c.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &c.module,
                entry_point: Some(MATERIAL_VERTEX),
                compilation_options: Default::default(),
                buffers: &[Some(vertex)],
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: match key.cull {
                    CullMode::Back => Some(wgpu::Face::Back),
                    CullMode::Front => Some(wgpu::Face::Front),
                    CullMode::None => None,
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(key.depth_write),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: key.samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &c.module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: key.format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.materials.insert(key, pipeline);
    }

    pub(crate) fn material(&self, key: &MaterialKey) -> &wgpu::RenderPipeline {
        &self.materials[key]
    }

    /// Makes the pipeline of an effect's pass writing `format`, unless it
    /// is there.
    pub(crate) fn effect_pipeline(
        &mut self,
        device: &wgpu::Device,
        program: &Arc<Program>,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        self.compile(device, program, &[]);
        let c = &self.compiled[&program.id()];
        self.effects
            .entry((program.id(), format))
            .or_insert_with(|| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ljos_3d effect"),
                    layout: Some(&c.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &c.module,
                        entry_point: Some(EFFECT_VERTEX),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &c.module,
                        entry_point: Some(EFFECT_FRAGMENT),
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
            })
    }

    /// A group of `program` binding `buffer` as its block, `textures` in
    /// their order and `depth` where it reads the depth.
    pub(crate) fn group(
        &self,
        device: &wgpu::Device,
        program: &Program,
        buffer: &wgpu::Buffer,
        textures: &[(wgpu::TextureView, wgpu::Sampler)],
        depth: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: BLOCK,
            resource: buffer.as_entire_binding(),
        }];
        for (i, (view, sampler)) in textures.iter().enumerate() {
            let i = i as u32;
            entries.push(wgpu::BindGroupEntry {
                binding: BLOCK + 1 + 2 * i,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: BLOCK + 2 + 2 * i,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        if program.reads_depth() {
            let view = depth.expect("a program that reads the depth is given it");
            entries.push(wgpu::BindGroupEntry {
                binding: DEPTH,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: DEPTH + 1,
                resource: wgpu::BindingResource::Sampler(&self.depth_sampler),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d program"),
            layout: &self.compiled[&program.id()].layout,
            entries: &entries,
        })
    }

    /// Makes `slots[i]` bind `material` with `header`, uploading what
    /// changed since it last did.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind_material(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        maps: &mut Maps,
        slots: &mut Vec<Slot>,
        i: usize,
        material: &CustomMaterial,
        header: &Header,
    ) {
        let program = &material.program;
        let mut textures = Vec::with_capacity(material.textures.len());
        let mut keys = Vec::with_capacity(material.textures.len());
        for t in &material.textures {
            match t
                .as_ref()
                .and_then(|t| Some((t, maps.single(device, queue, t)?)))
            {
                Some((t, bound)) => {
                    keys.push(Some((t.image.id(), t.sampling)));
                    textures.push(bound);
                }
                None => {
                    keys.push(None);
                    textures.push((self.black(device, queue), self.sampler.clone()));
                }
            }
        }
        let bytes = block(program, header, &material.values);
        let fresh = slots
            .get(i)
            .is_none_or(|s| s.program != program.id() || s.textures != keys);
        if fresh {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ljos_3d custom material"),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = self.group(device, program, &buffer, &textures, None);
            let slot = Slot {
                program: program.id(),
                textures: keys,
                buffer,
                group,
                written: Vec::new(),
            };
            if i < slots.len() {
                slots[i] = slot;
            } else {
                slots.push(slot);
            }
        }
        let slot = &mut slots[i];
        if slot.written != bytes {
            queue.write_buffer(&slot.buffer, 0, &bytes);
            slot.written = bytes;
        }
    }
}

/// An effect binds its INPUT before the author's textures.
fn texture_count(program: &Program) -> usize {
    program.textures().len() + usize::from(program.kind() == ProgramKind::Effect)
}

fn factor(b: Blend) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as F;
    match b {
        Blend::NoBlend | Blend::Zero => F::Zero,
        Blend::One => F::One,
        Blend::SrcColor => F::Src,
        Blend::OneMinusSrcColor => F::OneMinusSrc,
        Blend::DstColor => F::Dst,
        Blend::OneMinusDstColor => F::OneMinusDst,
        Blend::SrcAlpha => F::SrcAlpha,
        Blend::OneMinusSrcAlpha => F::OneMinusSrcAlpha,
        Blend::DstAlpha => F::DstAlpha,
        Blend::OneMinusDstAlpha => F::OneMinusDstAlpha,
        // WebGPU has one constant, and no blend constant is ever set, so
        // it is transparent black either way.
        Blend::ConstantColor | Blend::ConstantAlpha => F::Constant,
        Blend::OneMinusConstantColor | Blend::OneMinusConstantAlpha => F::OneMinusConstant,
        Blend::SrcAlphaSaturate => F::SrcAlphaSaturated,
    }
}
