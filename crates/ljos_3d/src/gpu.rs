//! The renderer: a [`Scene`] drawn into a [`Target`] on a device someone
//! else owns.
//!
//! The device is the host's: egui-wgpu's natively, so the 3D pass and the
//! 2D pass share one device and one queue, and the browser's on the web.
//! The renderer never makes one of its own except in [`crate::offscreen`].
//!
//! Drawing has two halves, because a host's 2D pass is already open when
//! it reaches the view: [`Renderer::render`] records the 3D pass into the
//! host's encoder before the 2D pass begins, and [`Renderer::blit`] draws
//! the picture into the 2D pass over the view's rect. A target that holds
//! the scene's revision at its size is not drawn again, so a still 3D view
//! costs one textured triangle per 2D frame and nothing when no 2D frame
//! is drawn.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::scene::{
    AlphaMode, Background, Camera, CullMode, Light, Lighting, Material, MeshSource, NodeKind,
    Primitive, Scene,
};

/// What the picture is kept in: sRGB values written by the shader, so the
/// same bytes blend in a 2D pass that works in gamma space.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// The depth format every backend has, WebGL2 included.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// The most lights a scene is lit by; those after it in the tree are left
/// out. Sixteen keep the frame's uniforms near a kilobyte, well inside
/// WebGL2's smallest uniform block, and the shader's loop short.
pub const MAX_LIGHTS: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq)]
struct LightUniform {
    /// w: 0 directional, 1 point, 2 spot.
    position: [f32; 4],
    /// w: the cosine of half the cone.
    direction: [f32; 4],
    /// w: the cosine of half the inner cone.
    color: [f32; 4],
    fade: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    /// w is 1 for an orthographic camera.
    camera: [f32; 4],
    camera_dir: [f32; 4],
    ambient: [f32; 4],
    counts: [u32; 4],
    lights: [LightUniform; MAX_LIGHTS],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq)]
struct ModelUniform {
    world: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    base: [f32; 4],
    emissive: [f32; 4],
    /// Lit, metal-roughness, metalness, roughness.
    params: [f32; 4],
    /// Mask cutoff (below 0 for none), specular amount and roughness.
    params2: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

/// What a pipeline differs by.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    cull: CullMode,
    /// A blended model tests depth but leaves it for what is behind it.
    depth_write: bool,
    samples: u32,
}

/// The pipelines and meshes of one device. One per device, shared by every
/// view drawn on it.
pub struct Renderer {
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    frame_layout: wgpu::BindGroupLayout,
    model_layout: wgpu::BindGroupLayout,
    model_stride: u64,
    meshes: HashMap<Primitive, GpuMesh>,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    blits: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Sample counts both the picture's and the depth's format take,
    /// ascending; 1 always among them.
    sample_counts: Vec<u32>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("meshes", &self.meshes.len())
            .field("sample_counts", &self.sample_counts)
            .finish_non_exhaustive()
    }
}

/// The multisampled attachments a picture is drawn into before it is
/// resolved into the target's texture.
struct Multisample {
    samples: u32,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

/// Where one view's picture is drawn and kept.
pub struct Target {
    size: [u32; 2],
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    /// Made when a scene first asks for multisampling, and again when it
    /// asks for another count.
    multisample: Option<Multisample>,
    blit_group: wgpu::BindGroup,
    // The uniforms are the target's, not the renderer's: writes to a
    // buffer land before the submission that holds the passes, so two
    // views drawn in one submission from one buffer would both see the
    // last view's.
    frame_buffer: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    /// One slot per model, the renderer's stride apart; grown, never
    /// shrunk while the target lives.
    model_buffer: wgpu::Buffer,
    model_group: wgpu::BindGroup,
    model_capacity: u64,
    /// What the buffers hold, so a frame writes only what changed: one
    /// model moving in a hundred uploads one slot, not a hundred.
    frame_written: Option<FrameUniform>,
    slots_written: Vec<ModelUniform>,
    /// Where runs of changed slots are laid out, the stride apart, before
    /// they are written; kept to save allocating each frame.
    staging: Vec<u8>,
    /// The scene revision the picture shows; `None` until drawn.
    drawn: Option<u64>,
    /// How many models the last drawing drew.
    drawn_models: usize,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("size", &self.size)
            .field("drawn", &self.drawn)
            .field("drawn_models", &self.drawn_models)
            .finish_non_exhaustive()
    }
}

fn attachment(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    samples: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

impl Target {
    /// A picture `size` device pixels large; a side of 0 is taken as 1.
    pub fn new(device: &wgpu::Device, renderer: &Renderer, size: [u32; 2]) -> Self {
        let size = [size[0].max(1), size[1].max(1)];
        let color = attachment(
            device,
            "ljos_3d picture",
            size,
            1,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = attachment(
            device,
            "ljos_3d depth",
            size,
            1,
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let blit_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d blit"),
            layout: &renderer.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&renderer.blit_sampler),
                },
            ],
        });
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ljos_3d frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d frame"),
            layout: &renderer.frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let model_capacity = 16;
        let (model_buffer, model_group) = model_slots(
            device,
            &renderer.model_layout,
            model_capacity,
            renderer.model_stride,
        );
        Target {
            size,
            color,
            color_view,
            depth_view,
            multisample: None,
            blit_group,
            frame_buffer,
            frame_group,
            model_buffer,
            model_group,
            model_capacity,
            frame_written: None,
            slots_written: Vec::new(),
            staging: Vec::new(),
            drawn: None,
            drawn_models: 0,
        }
    }

    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The picture, for a host that reads it back (a test, a reference
    /// renderer) or samples it itself.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.color
    }

    /// How many models the last drawing drew: those the camera's frustum
    /// left in.
    pub fn drawn_models(&self) -> usize {
        self.drawn_models
    }

    /// The samples a pixel was drawn with last; 1 without multisampling.
    pub fn samples(&self) -> u32 {
        self.multisample.as_ref().map_or(1, |m| m.samples)
    }

    fn multisample(&mut self, device: &wgpu::Device, samples: u32) {
        if samples <= 1 {
            self.multisample = None;
            return;
        }
        if self
            .multisample
            .as_ref()
            .is_some_and(|m| m.samples == samples)
        {
            return;
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = attachment(
            device,
            "ljos_3d samples",
            self.size,
            samples,
            COLOR_FORMAT,
            usage,
        );
        let depth = attachment(
            device,
            "ljos_3d sampled depth",
            self.size,
            samples,
            DEPTH_FORMAT,
            usage,
        );
        self.multisample = Some(Multisample {
            samples,
            color: color.create_view(&wgpu::TextureViewDescriptor::default()),
            depth: depth.create_view(&wgpu::TextureViewDescriptor::default()),
        });
    }
}

fn uniform_layout(device: &wgpu::Device, label: &str, dynamic: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn model_slots(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: u64,
    stride: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ljos_3d models"),
        size: capacity * stride,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ljos_3d models"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(std::mem::size_of::<ModelUniform>() as u64),
            }),
        }],
    });
    (buffer, group)
}

/// The six planes of what `view_proj` sees, each `(normal, d)` with the
/// inside where `normal · p + d >= 0`; normals of unit length. Read off
/// the matrix's rows, with wgpu's depth of 0 to 1.
fn frustum(view_proj: &Mat4) -> [[f32; 4]; 6] {
    let m = &view_proj.0;
    let row = |r: usize| [m[0][r], m[1][r], m[2][r], m[3][r]];
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let add = |a: [f32; 4], b: [f32; 4], s: f32| {
        [
            a[0] + s * b[0],
            a[1] + s * b[1],
            a[2] + s * b[2],
            a[3] + s * b[3],
        ]
    };
    let mut planes = [
        add(r3, r0, 1.0),
        add(r3, r0, -1.0),
        add(r3, r1, 1.0),
        add(r3, r1, -1.0),
        r2,
        add(r3, r2, -1.0),
    ];
    for p in &mut planes {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        if l > 0.0 {
            for c in p.iter_mut() {
                *c /= l;
            }
        }
    }
    planes
}

/// Whether a sphere is wholly outside one of the planes.
fn outside(planes: &[[f32; 4]; 6], centre: Vec3, radius: f32) -> bool {
    planes
        .iter()
        .any(|p| p[0] * centre.x + p[1] * centre.y + p[2] * centre.z + p[3] < -radius)
}

/// The longest a unit vector becomes under `world`: what a bounding
/// sphere's radius is scaled by.
fn largest_scale(world: &Mat4) -> f32 {
    (0..3)
        .map(|c| {
            let v = world.0[c];
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        })
        .fold(0.0, f32::max)
}

fn transform_point(m: &Mat4, p: Vec3) -> Vec3 {
    m.transform_vector(p) + m.translation()
}

fn light_uniform(light: &Light, world: &Mat4) -> LightUniform {
    let (color, brightness, _) = light.common();
    let c = color.to_linear();
    let p = world.translation();
    let d = world.forward();
    let mut u = LightUniform {
        position: [p.x, p.y, p.z, 0.0],
        direction: [d.x, d.y, d.z, 0.0],
        color: [c[0] * brightness, c[1] * brightness, c[2] * brightness, 0.0],
        fade: [1.0, 0.0, 0.0, 0.0],
    };
    match *light {
        Light::Directional { .. } => {}
        Light::Point { fade, .. } => {
            u.position[3] = 1.0;
            u.fade = [
                fade.constant,
                fade.linear * 0.01,
                fade.quadratic * 0.0001,
                0.0,
            ];
        }
        Light::Spot {
            fade,
            cone_angle,
            inner_cone_angle,
            ..
        } => {
            u.position[3] = 2.0;
            u.fade = [
                fade.constant,
                fade.linear * 0.01,
                fade.quadratic * 0.0001,
                0.0,
            ];
            let outer = cone_angle.clamp(0.0, 180.0);
            let inner = inner_cone_angle.clamp(0.0, outer);
            u.direction[3] = (outer * 0.5).to_radians().cos();
            u.color[3] = (inner * 0.5).to_radians().cos();
        }
    }
    u
}

fn model_uniform(material: &Material, world: &Mat4) -> ModelUniform {
    let mut u = ModelUniform {
        world: world.0,
        normal: world.normal_matrix().0,
        base: [0.0; 4],
        emissive: [0.0; 4],
        params: [0.0; 4],
        params2: [-1.0, 0.0, 0.0, 0.0],
    };
    let lit = |l: Lighting| f32::from(u8::from(l == Lighting::Fragment));
    match material {
        Material::Default(m) => {
            u.base = m.diffuse_color.to_linear();
            u.params[0] = lit(m.lighting);
            u.params2[1] = m.specular_amount.clamp(0.0, 1.0);
            u.params2[2] = m.specular_roughness.clamp(0.0, 1.0);
        }
        Material::Principled(m) => {
            let c = m.base_color.to_linear();
            let alpha = match m.alpha_mode {
                AlphaMode::Opaque => 1.0,
                _ => material.alpha().clamp(0.0, 1.0),
            };
            u.base = [c[0], c[1], c[2], alpha];
            u.emissive = [m.emissive[0], m.emissive[1], m.emissive[2], 0.0];
            u.params = [
                lit(m.lighting),
                1.0,
                m.metalness.clamp(0.0, 1.0),
                m.roughness.clamp(0.0, 1.0),
            ];
            if m.alpha_mode == AlphaMode::Mask {
                u.params2[0] = m.alpha_cutoff.max(0.0);
            }
        }
    }
    u
}

fn pick_samples(counts: &[u32], wanted: u32) -> u32 {
    if wanted <= 1 {
        return 1;
    }
    let more = || counts.iter().copied().filter(|&n| n > 1);
    more()
        .filter(|&n| n <= wanted)
        .max()
        .or_else(|| more().min())
        .unwrap_or(1)
}

/// Writes the slots whose uniforms differ from what the buffer holds, a
/// run of neighbours in one write.
fn write_slots(queue: &wgpu::Queue, target: &mut Target, draws: &[Draw], stride: u64) {
    let size = std::mem::size_of::<ModelUniform>();
    let stride = stride as usize;
    let written = &mut target.slots_written;
    let changed = |i: usize, written: &[ModelUniform]| written.get(i) != Some(&draws[i].uniform);
    let mut i = 0;
    while i < draws.len() {
        if !changed(i, written) {
            i += 1;
            continue;
        }
        let start = i;
        while i < draws.len() && changed(i, written) {
            i += 1;
        }
        let staging = &mut target.staging;
        staging.clear();
        staging.resize((i - start - 1) * stride + size, 0);
        for (k, draw) in draws[start..i].iter().enumerate() {
            staging[k * stride..k * stride + size]
                .copy_from_slice(bytemuck::bytes_of(&draw.uniform));
        }
        queue.write_buffer(&target.model_buffer, (start * stride) as u64, staging);
    }
    written.clear();
    written.extend(draws.iter().map(|d| d.uniform));
}

struct Draw {
    mesh: Primitive,
    key: PipelineKey,
    uniform: ModelUniform,
    /// How far in front of the camera: blended models are drawn from the
    /// farthest.
    depth: f32,
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let frame_layout = uniform_layout(device, "ljos_3d frame", false);
        let model_layout = uniform_layout(device, "ljos_3d model", true);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d lit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/lit.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d lit"),
            bind_group_layouts: &[Some(&frame_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let size = std::mem::size_of::<ModelUniform>() as u64;
        let model_stride = size.div_ceil(align) * align;
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d blit"),
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
        // The picture is drawn at the view's size in device pixels, so it
        // lands one texel per pixel; linear only matters for a view placed
        // off the pixel grid.
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ljos_3d blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Renderer {
            shader,
            pipeline_layout,
            pipelines: HashMap::new(),
            frame_layout,
            model_layout,
            model_stride,
            meshes: HashMap::new(),
            blit_layout,
            blit_sampler,
            blits: HashMap::new(),
            // What WebGPU promises every device for these formats.
            sample_counts: vec![1, 4],
        }
    }

    /// Takes the sample counts `adapter` has for the picture's and the
    /// depth's formats, beyond the 1 and 4 every device has. Counts other
    /// than those need the device to have been asked for
    /// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`; without it, nothing
    /// changes.
    pub fn use_adapter_sample_counts(&mut self, adapter: &wgpu::Adapter, device: &wgpu::Device) {
        if !device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            return;
        }
        let takes = |format: wgpu::TextureFormat, n: u32| {
            let f = adapter.get_texture_format_features(format);
            f.flags.sample_count_supported(n)
                && (n == 1
                    || format.has_depth_aspect()
                    || f.flags
                        .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
        };
        self.sample_counts = [1, 2, 4, 8, 16]
            .into_iter()
            .filter(|&n| n == 1 || (takes(COLOR_FORMAT, n) && takes(DEPTH_FORMAT, n)))
            .collect();
    }

    /// The samples a pixel gets for a scene that asks for `wanted`: the
    /// most the device has up to it, or, where it has none between 1 and
    /// it, the fewest above 1, so that asking for antialiasing never draws
    /// without it on a device that can.
    pub fn samples_for(&self, wanted: u32) -> u32 {
        pick_samples(&self.sample_counts, wanted)
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: PipelineKey) {
        let (shader, layout) = (&self.shader, &self.pipeline_layout);
        self.pipelines.entry(key).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d lit"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    })],
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
                    format: DEPTH_FORMAT,
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
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }

    fn mesh(&mut self, device: &wgpu::Device, p: Primitive) -> &GpuMesh {
        self.meshes.entry(p).or_insert_with(|| {
            let mesh = Mesh::primitive(p);
            let vertices: Vec<Vertex> = mesh
                .positions
                .iter()
                .zip(&mesh.normals)
                .map(|(position, normal)| Vertex {
                    position: *position,
                    normal: *normal,
                })
                .collect();
            GpuMesh {
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ljos_3d vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ljos_3d indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: mesh.indices.len() as u32,
            }
        })
    }

    /// Records the 3D pass of `scene` into `encoder`, unless `target`
    /// already shows this revision. Returns whether it drew.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        target: &mut Target,
    ) -> bool {
        if target.drawn == Some(scene.revision()) {
            return false;
        }
        let [w, h] = target.size;
        let walked = scene.walk();
        let samples = self.samples_for(scene.environment().msaa_samples);
        target.multisample(device, samples);

        let camera = scene.active_camera();
        let (view, view_proj) = match camera {
            Some((camera, world)) => {
                let view = world.inverse().unwrap_or(Mat4::IDENTITY);
                (view, camera.projection(w as f32, h as f32) * view)
            }
            None => (Mat4::IDENTITY, Mat4::IDENTITY),
        };
        let mut frame = FrameUniform::zeroed();
        frame.view_proj = view_proj.0;
        if let Some((camera, world)) = camera {
            let p = world.translation();
            let d = world.forward();
            let ortho = matches!(camera, Camera::Orthographic { .. });
            frame.camera = [p.x, p.y, p.z, f32::from(u8::from(ortho))];
            frame.camera_dir = [d.x, d.y, d.z, 0.0];
        }
        let mut ambient = [0.0f32; 3];
        let mut count = 0;
        for (_, node, world) in &walked {
            let NodeKind::Light(light) = &node.kind else {
                continue;
            };
            if count == MAX_LIGHTS {
                break;
            }
            let a = light.common().2.to_linear();
            for (sum, c) in ambient.iter_mut().zip(a) {
                *sum += c;
            }
            frame.lights[count] = light_uniform(light, world);
            count += 1;
        }
        frame.ambient = [ambient[0], ambient[1], ambient[2], 0.0];
        frame.counts[0] = count as u32;
        if target.frame_written != Some(frame) {
            queue.write_buffer(&target.frame_buffer, 0, bytemuck::bytes_of(&frame));
            target.frame_written = Some(frame);
        }

        let mut opaque = Vec::new();
        let mut blended = Vec::new();
        if let Some((camera, _)) = camera {
            let planes = camera.frustum_culling().then(|| frustum(&view_proj));
            for (_, node, world) in &walked {
                let NodeKind::Model(model) = &node.kind else {
                    continue;
                };
                let MeshSource::Primitive(p) = model.mesh;
                let (centre, radius) = p.bounds();
                let centre = transform_point(world, centre);
                if let Some(planes) = &planes
                    && outside(planes, centre, radius * largest_scale(world))
                {
                    continue;
                }
                let blends = model.material.blends();
                let draw = Draw {
                    mesh: p,
                    key: PipelineKey {
                        cull: model.material.cull_mode(),
                        depth_write: !blends,
                        samples,
                    },
                    uniform: model_uniform(&model.material, world),
                    depth: -transform_point(&view, centre).z,
                };
                if blends {
                    blended.push(draw);
                } else {
                    opaque.push(draw);
                }
            }
        }
        // Over what is opaque, from the farthest in: each blends over what
        // is behind it. A stable sort keeps the tree's order where two are
        // as far.
        blended.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        let mut draws = opaque;
        draws.append(&mut blended);

        if draws.len() as u64 > target.model_capacity {
            target.model_capacity = (draws.len() as u64).next_power_of_two();
            (target.model_buffer, target.model_group) = model_slots(
                device,
                &self.model_layout,
                target.model_capacity,
                self.model_stride,
            );
            // A new buffer holds nothing yet.
            target.slots_written.clear();
        }
        write_slots(queue, target, &draws, self.model_stride);
        for draw in &draws {
            self.mesh(device, draw.mesh);
            self.pipeline(device, draw.key);
        }

        let clear = match scene.environment().background {
            Background::Transparent => wgpu::Color::TRANSPARENT,
            Background::Color(c) => wgpu::Color {
                r: f64::from(c.r * c.a),
                g: f64::from(c.g * c.a),
                b: f64::from(c.b * c.a),
                a: f64::from(c.a),
            },
        };
        {
            // Multisampled, the picture is drawn into the samples and
            // resolved into the target's texture, which is all the blit
            // and a read back ever see.
            let (view, resolve, depth) = match &target.multisample {
                Some(m) => (&m.color, Some(&target.color_view), &m.depth),
                None => (&target.color_view, None, &target.depth_view),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: if resolve.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &target.frame_group, &[]);
            let mut bound = None;
            for (i, draw) in draws.iter().enumerate() {
                if bound != Some(draw.key) {
                    pass.set_pipeline(&self.pipelines[&draw.key]);
                    bound = Some(draw.key);
                }
                let mesh = &self.meshes[&draw.mesh];
                let offset = (i as u64 * self.model_stride) as u32;
                pass.set_bind_group(1, &target.model_group, &[offset]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
        target.drawn_models = draws.len();
        target.drawn = Some(scene.revision());
        true
    }

    /// Makes the pipeline that draws pictures into a pass of `format`;
    /// needed once per format before [`Renderer::blit`].
    pub fn prepare_blit(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        let layout = &self.blit_layout;
        self.blits.entry(format).or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ljos_3d blit"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ljos_3d blit"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d blit"),
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
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }

    /// Draws `target`'s picture over the pass's viewport, which the host
    /// has set to the view's rect. Draws nothing for a format
    /// [`Renderer::prepare_blit`] was not given.
    pub fn blit(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        target: &Target,
    ) {
        let Some(pipeline) = self.blits.get(&format) else {
            return;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &target.blit_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_count_is_brought_to_one_the_device_has() {
        let webgpu = [1, 4];
        assert_eq!(pick_samples(&webgpu, 1), 1);
        assert_eq!(pick_samples(&webgpu, 2), 4, "up, not off");
        assert_eq!(pick_samples(&webgpu, 4), 4);
        assert_eq!(pick_samples(&webgpu, 8), 4, "down to the most there is");
        assert_eq!(pick_samples(&[1, 2, 4, 8], 2), 2);
        assert_eq!(pick_samples(&[1], 8), 1, "none without multisampling");
    }

    #[test]
    fn the_frame_fits_webgl2s_smallest_uniform_block() {
        assert!(std::mem::size_of::<FrameUniform>() <= 16384);
    }

    #[test]
    fn a_frustum_keeps_what_it_sees_and_drops_what_it_does_not() {
        let proj = Camera::PERSPECTIVE.projection(200.0, 100.0);
        let view = Mat4::from_trs(
            Vec3::new(0.0, 0.0, 300.0),
            crate::math::Quat::IDENTITY,
            Vec3::ONE,
        )
        .inverse()
        .unwrap();
        let planes = frustum(&(proj * view));
        assert!(!outside(&planes, Vec3::ZERO, 50.0));
        assert!(outside(&planes, Vec3::new(2000.0, 0.0, 0.0), 50.0));
        assert!(outside(&planes, Vec3::new(0.0, 0.0, 400.0), 50.0), "behind");
        assert!(
            outside(&planes, Vec3::new(0.0, 0.0, -20000.0), 50.0),
            "past far"
        );
        // Partly in is in.
        assert!(!outside(&planes, Vec3::new(0.0, 0.0, 320.0), 50.0));
    }
}
