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
use std::sync::{Arc, Weak};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::custom::CustomMaterial;
use crate::geometry::CustomGeometry;
use crate::gpu_extra::{self, InstanceRow, JOINT_SLOT, JointSlot, Rows};
use crate::ibl::{self, Bound, Ibl};
use crate::instancing::{Instancing, instance_worlds};
use crate::maps::{self, GroupKey, Maps};
use crate::math::{Mat4, Quat, Vec3};
use crate::mesh::{Mesh, Topology};
use crate::post::{self, Buffers, Chain, Pipes};
use crate::programs::{Header, MaterialKey, Programs, Slot};
use crate::scene::{
    AlphaMode, Background, Camera, CullMode, Environment, Light, LightProbe, Lighting, Material,
    MeshSource, NodeKind, Primitive, Scene, Tonemap,
};
use crate::shadow::{self, Layer as ShadowLayer, Plan};

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
    /// The first shadow map (below 0 for none), how many, the shadow's
    /// factor and its bias.
    shadow: [f32; 4],
    /// The samples' radius, their count and the cascades' blend.
    shadow2: [f32; 4],
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
    /// The inverse of the camera's projection and turn: a pixel's ray.
    sky: [[f32; 4]; 4],
    /// Turns a direction in the scene into the probe's.
    probe_rot: [[f32; 4]; 4],
    /// The probe's exposure, its horizon, the prefiltered map's last level
    /// and 1 where there is a probe.
    env: [f32; 4],
    /// The tone mapping mode, the exposure, the white point and the sky
    /// box's blur.
    tone: [f32; 4],
    lights: [LightUniform; MAX_LIGHTS],
    shadow_layers: [ShadowLayer; shadow::MAX_LAYERS],
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
    /// Mask cutoff (below 0 for none), specular amount and roughness, and
    /// 1 where shadows fall on the model.
    params2: [f32; 4],
    /// Per map, two rows taking a mesh UV to the image's.
    uv: [[f32; 4]; maps::MAPS * 2],
    /// A bit per map that is there; the channels the maps of one value
    /// read, two bits each; a bit per map read through the second UV set;
    /// 1 where the model's joints bend its mesh.
    maps: [u32; 4],
    /// The normal map's strength and the occlusion map's amount.
    map_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    /// Along the image's u; w says which way its v runs.
    tangent: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    /// Whether its vertices name joints: only then does a skin bend it.
    has_joints: bool,
    /// The second UV set and the joints, for a mesh that has either.
    extra: Option<wgpu::Buffer>,
    indices: wgpu::Buffer,
    count: u32,
    topology: Topology,
}

/// A custom geometry as last uploaded. `None` for one that cannot be
/// drawn: it is not decoded again until it is revised.
struct CustomMesh {
    revision: u64,
    source: Weak<CustomGeometry>,
    mesh: Option<GpuMesh>,
}

/// What a pipeline differs by.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    cull: CullMode,
    /// A blended model tests depth but leaves it for what is behind it.
    depth_write: bool,
    samples: u32,
    /// Drawn with a second vertex buffer, by `vs_extra`.
    extra: bool,
    /// The picture's, or the half floats a post-processed scene is drawn
    /// into.
    format: wgpu::TextureFormat,
    topology: Topology,
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
    /// An application's geometries by id: uploaded once per revision, and
    /// gone once no scene holds the geometry any more.
    custom: HashMap<u64, CustomMesh>,
    custom_uploads: u64,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    blits: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    maps: Maps,
    ibl: Ibl,
    sky_shader: wgpu::ShaderModule,
    skies: HashMap<(u32, wgpu::TextureFormat), wgpu::RenderPipeline>,
    /// The authors' programs: custom materials and effects.
    programs: Programs,
    post: Pipes,
    /// Whether the device renders into and filters half floats, which the
    /// probe's maps are; without, the scene draws with no probe.
    probes: bool,
    /// Sample counts both the picture's and the depth's format take,
    /// ascending; 1 always among them.
    sample_counts: Vec<u32>,
    shadow_shader: wgpu::ShaderModule,
    shadow_layout: wgpu::PipelineLayout,
    layer_layout: wgpu::BindGroupLayout,
    layer_stride: u64,
    /// By cull mode, and whether the mesh has a second vertex buffer.
    shadow_pipelines: HashMap<(CullMode, bool), wgpu::RenderPipeline>,
    shadow_sampler: wgpu::Sampler,
    /// What a frame without shadows binds where the maps go.
    no_shadows: shadow::Maps,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("meshes", &self.meshes.len())
            .field("custom_meshes", &self.custom.len())
            .field("probes", &self.probes)
            .field("sample_counts", &self.sample_counts)
            .finish_non_exhaustive()
    }
}

/// The multisampled attachments a picture is drawn into before it is
/// resolved into the target's texture.
struct Multisample {
    samples: u32,
    format: wgpu::TextureFormat,
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
    /// The copies every draw reads a range of; row 0 is the identity, a
    /// model drawn once.
    instances: Rows<InstanceRow>,
    /// A slot per skinned model, bound in the model's group; slot 0 for
    /// every other.
    joints: Rows<JointSlot>,
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
    /// The shadow maps, while a light casts shadows.
    shadows: Option<Shadows>,
    /// Shadow maps drawn, a pass each, since the target was made.
    shadow_passes: u64,
    /// The GPU's timestamps of its frames, where the device has them.
    timer: Option<crate::timing::Timer>,
    /// What each custom material drawn binds, in the order drawn.
    custom_slots: Vec<Slot>,
    /// The pictures between the post-processing passes.
    post: Buffers,
    /// How many times the picture was drawn: the programs' FRAME.
    frames: u64,
}

/// A target's shadow maps and what they were last drawn from.
struct Shadows {
    maps: shadow::Maps,
    /// A slot per layer, the renderer's layer stride apart.
    layer_buffer: wgpu::Buffer,
    layer_group: wgpu::BindGroup,
    /// `None` until drawn, or when the maps were made again.
    drawn: Option<ShadowKey>,
}

/// Everything a scene's shadow maps show: while it stays the same, so do
/// they, and they are not drawn again.
/// A caster as its shadow shows it: mesh, mesh revision, world, culling
/// and copies.
type CasterKey = (MeshKey, u64, [[f32; 4]; 4], CullMode, Copies);

#[derive(PartialEq)]
struct ShadowKey {
    layers: Vec<ShadowLayer>,
    casters: Vec<CasterKey>,
    /// Every copy and joint of the frame, as the casters' ranges and
    /// slots point into them.
    instances: Vec<InstanceRow>,
    joints: Vec<JointSlot>,
}

/// Which rows of the instance buffer a draw reads, and which joint slot.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Copies {
    first: u32,
    count: u32,
    joints: u32,
}

impl Copies {
    const ONE: Copies = Copies {
        first: 0,
        count: 1,
        joints: 0,
    };
}

/// A model drawn into the shadow maps.
struct Caster {
    mesh: MeshKey,
    cull: CullMode,
    copies: Copies,
    extra: bool,
    world: Mat4,
    centre: Vec3,
    radius: f32,
    /// Its uniforms' slot.
    slot: usize,
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
        let frame_group = frame_group(device, renderer, &frame_buffer, &renderer.no_shadows);
        let model_capacity = 16;
        let instances = Rows::new(device, "ljos_3d copies", wgpu::BufferUsages::VERTEX, 16);
        let joints = Rows::new(device, "ljos_3d joints", wgpu::BufferUsages::UNIFORM, 1);
        let (model_buffer, model_group) = model_slots(
            device,
            &renderer.model_layout,
            model_capacity,
            renderer.model_stride,
            &joints.buffer,
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
            instances,
            joints,
            frame_written: None,
            slots_written: Vec::new(),
            staging: Vec::new(),
            drawn: None,
            drawn_models: 0,
            shadows: None,
            shadow_passes: 0,
            timer: crate::timing::Timer::new(device),
            custom_slots: Vec::new(),
            post: Buffers::default(),
            frames: 0,
        }
    }

    /// How long the GPU took to draw the picture (its shadow maps and its
    /// scene pass) the last time that was read back: one or two drawings
    /// behind, as nothing waits for the GPU. `None` where the device was
    /// not asked for [`crate::timing_features`], or before a reading.
    pub fn last_gpu_time(&self) -> Option<std::time::Duration> {
        self.timer.as_ref().and_then(crate::timing::Timer::last)
    }

    /// [`Self::last_gpu_time`], only if it was read since the last time
    /// this was asked: a report of frames counts each reading once, while a
    /// view that is not drawn again keeps its last.
    pub fn take_gpu_time(&mut self) -> Option<std::time::Duration> {
        self.timer.as_mut().and_then(crate::timing::Timer::take)
    }

    /// How many shadow maps were drawn since the target was made: a
    /// scene that keeps its casters, receivers and lights where they are
    /// draws none again.
    pub fn shadow_passes(&self) -> u64 {
        self.shadow_passes
    }

    /// Makes the shadow maps the plan needs, unless they are there, or
    /// drops them for a scene without shadows.
    fn shadows(&mut self, device: &wgpu::Device, renderer: &Renderer, plan: &Plan) {
        if plan.layers.is_empty() {
            if self.shadows.take().is_some() {
                self.frame_group =
                    frame_group(device, renderer, &self.frame_buffer, &renderer.no_shadows);
            }
            return;
        }
        let layers = shadow::layer_count(plan.layers.len());
        if self
            .shadows
            .as_ref()
            .is_some_and(|s| s.maps.size == plan.size && s.maps.layers == layers)
        {
            return;
        }
        let maps = shadow::maps(device, plan.size, layers);
        self.frame_group = frame_group(device, renderer, &self.frame_buffer, &maps);
        let (layer_buffer, layer_group) = match self.shadows.take() {
            Some(s) => (s.layer_buffer, s.layer_group),
            None => {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ljos_3d shadow layers"),
                    size: shadow::MAX_LAYERS as u64 * renderer.layer_stride,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ljos_3d shadow layers"),
                    layout: &renderer.layer_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<ShadowLayer>() as u64),
                        }),
                    }],
                });
                (buffer, group)
            }
        };
        self.shadows = Some(Shadows {
            maps,
            layer_buffer,
            layer_group,
            drawn: None,
        });
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

    fn multisample(&mut self, device: &wgpu::Device, samples: u32, format: wgpu::TextureFormat) {
        if samples <= 1 {
            self.multisample = None;
            return;
        }
        if self
            .multisample
            .as_ref()
            .is_some_and(|m| m.samples == samples && m.format == format)
        {
            return;
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = attachment(device, "ljos_3d samples", self.size, samples, format, usage);
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
            format,
            color: color.create_view(&wgpu::TextureViewDescriptor::default()),
            depth: depth.create_view(&wgpu::TextureViewDescriptor::default()),
        });
    }
}

fn frame_group(
    device: &wgpu::Device,
    renderer: &Renderer,
    buffer: &wgpu::Buffer,
    maps: &shadow::Maps,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ljos_3d frame"),
        layout: &renderer.frame_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&maps.array),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&renderer.shadow_sampler),
            },
        ],
    })
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
    joints: &wgpu::Buffer,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ljos_3d models"),
        size: capacity * stride,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = model_group(device, layout, &buffer, joints);
    (buffer, group)
}

fn model_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    models: &wgpu::Buffer,
    joints: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ljos_3d models"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: models,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<ModelUniform>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: joints,
                    offset: 0,
                    size: wgpu::BufferSize::new(JOINT_SLOT),
                }),
            },
        ],
    })
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
        shadow: [-1.0, 0.0, 0.0, 0.0],
        shadow2: [0.0; 4],
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
        uv: [[0.0; 4]; maps::MAPS * 2],
        maps: [0; 4],
        map_params: [1.0, 1.0, 0.0, 0.0],
    };
    for (i, slot) in maps::slots(material).iter().enumerate() {
        let Some((texture, _)) = slot else {
            continue;
        };
        u.maps[0] |= 1 << i;
        if texture.uv_set > 0 {
            u.maps[2] |= 1 << i;
        }
        let [r0, r1] = texture.uv.rows();
        u.uv[i * 2] = [r0[0], r0[1], r0[2], 0.0];
        u.uv[i * 2 + 1] = [r1[0], r1[1], r1[2], 0.0];
    }
    let lit = |l: Lighting| f32::from(u8::from(l == Lighting::Fragment));
    match material {
        Material::Default(m) => {
            u.base = m.diffuse_color.to_linear();
            u.params[0] = lit(m.lighting);
            u.params2[1] = m.specular_amount.clamp(0.0, 1.0);
            u.params2[2] = m.specular_roughness.clamp(0.0, 1.0);
            u.maps[1] = 3 << 6;
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
            let maps = &m.maps;
            u.maps[1] = maps.metalness_channel.index()
                | maps.roughness_channel.index() << 2
                | maps.occlusion_channel.index() << 4
                | maps.opacity_channel.index() << 6;
            u.map_params = [
                maps.normal_strength,
                maps.occlusion_amount.clamp(0.0, 1.0),
                0.0,
                0.0,
            ];
        }
        // Its snippets read only the matrices and whether it takes shadows.
        Material::Custom(_) => u.base = [1.0; 4],
    }
    u
}

/// The camera's near and far clip planes.
fn clip_planes(camera: &Camera) -> (f32, f32) {
    match *camera {
        Camera::Perspective {
            clip_near,
            clip_far,
            ..
        }
        | Camera::Orthographic {
            clip_near,
            clip_far,
            ..
        } => (clip_near, clip_far),
    }
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
fn write_slots(queue: &wgpu::Queue, target: &mut Target, draws: &[ModelUniform], stride: u64) {
    let size = std::mem::size_of::<ModelUniform>();
    let stride = stride as usize;
    let written = &mut target.slots_written;
    let changed = |i: usize, written: &[ModelUniform]| written.get(i) != Some(&draws[i]);
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
            staging[k * stride..k * stride + size].copy_from_slice(bytemuck::bytes_of(draw));
        }
        queue.write_buffer(&target.model_buffer, (start * stride) as u64, staging);
    }
    written.clear();
    written.extend_from_slice(draws);
}

#[derive(Clone, Copy, PartialEq)]
enum MeshKey {
    Primitive(Primitive),
    Custom(u64),
}

struct Draw<'a> {
    mesh: MeshKey,
    /// A custom material and its pipeline, which bind in place of the
    /// maps.
    custom: Option<(&'a CustomMaterial, MaterialKey)>,
    maps: GroupKey,
    key: PipelineKey,
    uniform: ModelUniform,
    copies: Copies,
    /// How far in front of the camera: blended models are drawn from the
    /// farthest.
    depth: f32,
    /// Its entry among the shadow casters.
    caster: Option<usize>,
}

/// The lit shader, with the tone mapping it shares with the sky box.
const LIT_WGSL: &str = concat!(
    include_str!("shaders/tone.wgsl"),
    include_str!("shaders/lit.wgsl")
);
const SHADOW_WGSL: &str = include_str!("shaders/shadow.wgsl");
const SKY_WGSL: &str = concat!(
    include_str!("shaders/tone.wgsl"),
    include_str!("shaders/sky.wgsl")
);

/// Bytes a [`Renderer`]'s textures take on its device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureBytes {
    /// The materials' maps, mipmaps included.
    pub maps: u64,
    /// The light probes' prefiltered radiance and irradiance.
    pub probes: u64,
}

fn environment_uniforms(
    frame: &mut FrameUniform,
    environment: &Environment,
    probe: Option<&LightProbe>,
    bound: Bound,
    last_level: f32,
) {
    let mode = match environment.tonemap {
        Tonemap::None => 0.0,
        Tonemap::Linear => 1.0,
        Tonemap::Aces => 2.0,
        Tonemap::HejlDawson => 3.0,
        Tonemap::Filmic => 4.0,
    };
    // Without tone mapping the light is written as it is: no exposure,
    // the probe's included.
    frame.tone = [
        mode,
        environment.exposure.max(0.0),
        environment.white_point,
        0.0,
    ];
    frame.probe_rot = Mat4::IDENTITY.0;
    let Some(probe) = probe else {
        return;
    };
    let exposure = if environment.tonemap == Tonemap::None {
        1.0
    } else {
        probe.exposure.max(0.0)
    };
    frame.env = [
        exposure,
        probe.horizon.clamp(0.0, 1.0),
        last_level,
        f32::from(u8::from(bound != Bound::None)),
    ];
    frame.tone[3] = probe.skybox_blur.clamp(0.0, 1.0);
    let turn = Mat4::from_trs(
        Vec3::ZERO,
        Quat::from_euler_degrees(probe.orientation),
        Vec3::ONE,
    );
    frame.probe_rot = turn.inverse().unwrap_or(Mat4::IDENTITY).0;
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let frame_layout = shadow::frame_layout(device);
        let layer_layout = uniform_layout(device, "ljos_3d shadow layer", true);
        let model_layout = gpu_extra::model_layout(device);
        let maps = Maps::new(device);
        let ibl = Ibl::new(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d lit"),
            source: wgpu::ShaderSource::Wgsl(LIT_WGSL.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d sky"),
            source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
        });
        // The sky box shares the layout, so that switching to it and back
        // keeps every group bound.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d lit"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&model_layout),
                Some(&maps.layout),
                Some(&ibl.layout),
            ],
            immediate_size: 0,
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let size = std::mem::size_of::<ModelUniform>() as u64;
        let model_stride = size.div_ceil(align) * align;
        let size = std::mem::size_of::<ShadowLayer>() as u64;
        let layer_stride = size.div_ceil(align) * align;
        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d shadow"),
            source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d shadow"),
            bind_group_layouts: &[Some(&layer_layout), Some(&model_layout)],
            immediate_size: 0,
        });
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
            custom: HashMap::new(),
            custom_uploads: 0,
            blit_layout,
            blit_sampler,
            blits: HashMap::new(),
            maps,
            ibl,
            sky_shader,
            skies: HashMap::new(),
            programs: Programs::new(device),
            post: Pipes::new(device),
            // WebGPU has half floats to draw into and filter; a host on
            // another backend says whether it has with
            // `use_adapter_formats`.
            probes: true,
            // What WebGPU promises every device for these formats.
            sample_counts: vec![1, 4],
            shadow_shader,
            shadow_layout,
            layer_layout,
            layer_stride,
            shadow_pipelines: HashMap::new(),
            shadow_sampler: shadow::sampler(device),
            no_shadows: shadow::maps(device, 1, shadow::layer_count(0)),
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

    /// Takes whether `adapter` renders into and filters the half floats a
    /// light probe's maps are made of. WebGPU always does; WebGL2 only
    /// with `EXT_color_buffer_float`. Without them a light probe neither
    /// lights the scene nor shows as its sky box. Returns whether probes
    /// are drawn, for the host to say so where they are not.
    pub fn use_adapter_formats(&mut self, adapter: &wgpu::Adapter) -> bool {
        let f = adapter.get_texture_format_features(ibl::FORMAT);
        self.probes = f.allowed_usages.contains(
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        ) && f
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE);
        self.probes
    }

    /// Bytes the renderer's textures take on the device: the materials'
    /// maps and the light probes'.
    pub fn texture_bytes(&self) -> TextureBytes {
        TextureBytes {
            maps: self.maps.bytes(),
            probes: self.ibl.bytes(),
        }
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
                    entry_point: Some(if key.extra { "vs_extra" } else { "vs_main" }),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(vertex_layout()),
                        Some(gpu_extra::instance_layout()),
                        key.extra.then(gpu_extra::extra_layout),
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: key.topology.wgpu(),
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
                        format: key.format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }

    /// The depth pass of casters culled `cull`: the faces the picture
    /// shows are the ones the maps hold, so a surface's own depth is in
    /// them and the bias is what keeps it from shadowing itself.
    fn shadow_pipeline(&mut self, device: &wgpu::Device, cull: CullMode, extra: bool) {
        let (shader, layout) = (&self.shadow_shader, &self.shadow_layout);
        self.shadow_pipelines
            .entry((cull, extra))
            .or_insert_with(|| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ljos_3d shadow"),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: shader,
                        entry_point: Some(if extra { "vs_extra" } else { "vs_main" }),
                        compilation_options: Default::default(),
                        buffers: &[
                            Some(wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                            }),
                            Some(gpu_extra::instance_layout()),
                            extra.then(gpu_extra::extra_layout),
                        ],
                    },
                    primitive: wgpu::PrimitiveState {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: match cull {
                            CullMode::Back => Some(wgpu::Face::Back),
                            CullMode::Front => Some(wgpu::Face::Front),
                            CullMode::None => None,
                        },
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: shadow::FORMAT,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::Less),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            });
    }

    /// Primitives are triangles; a custom geometry says. Only called for
    /// geometries `custom_mesh` found drawable.
    fn topology_of(&self, key: MeshKey) -> Topology {
        match key {
            MeshKey::Primitive(_) => Topology::Triangles,
            MeshKey::Custom(id) => self.gpu_mesh(MeshKey::Custom(id)).topology,
        }
    }

    fn gpu_mesh(&self, key: MeshKey) -> &GpuMesh {
        match key {
            MeshKey::Primitive(p) => &self.meshes[&p],
            MeshKey::Custom(id) => self.custom[&id]
                .mesh
                .as_ref()
                .expect("only drawable geometries are drawn"),
        }
    }

    fn sky(&mut self, device: &wgpu::Device, samples: u32, format: wgpu::TextureFormat) {
        let (shader, layout) = (&self.sky_shader, &self.pipeline_layout);
        self.skies.entry((samples, format)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ljos_3d sky"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                // Behind everything, as it is drawn first and leaves the
                // depth as it was cleared.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
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
        });
    }

    fn mesh(&mut self, device: &wgpu::Device, p: Primitive) -> &GpuMesh {
        self.meshes
            .entry(p)
            .or_insert_with(|| upload(device, &Mesh::primitive(p)))
    }

    /// Uploads `geometry` unless this revision of it is there already.
    /// Whether it can be drawn.
    fn custom_mesh(&mut self, device: &wgpu::Device, geometry: &Arc<CustomGeometry>) -> bool {
        let id = geometry.id();
        match self.custom.get(&id) {
            Some(c) if c.revision == geometry.revision() => return c.mesh.is_some(),
            _ => {}
        }
        self.custom_uploads += 1;
        // Nothing to draw is as good as a geometry that fails.
        let mesh = geometry
            .to_mesh()
            .ok()
            .filter(|m| !m.indices.is_empty())
            .map(|m| upload(device, &m));
        let drawable = mesh.is_some();
        self.custom.insert(
            id,
            CustomMesh {
                revision: geometry.revision(),
                source: Arc::downgrade(geometry),
                mesh,
            },
        );
        drawable
    }

    /// How many custom geometries are held.
    pub fn custom_meshes(&self) -> usize {
        self.custom.len()
    }

    /// How many times a custom geometry was decoded and uploaded.
    pub fn custom_uploads(&self) -> u64 {
        self.custom_uploads
    }
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Float32x4,
];

impl Topology {
    pub(crate) fn wgpu(self) -> wgpu::PrimitiveTopology {
        match self {
            Topology::Triangles => wgpu::PrimitiveTopology::TriangleList,
            Topology::Lines => wgpu::PrimitiveTopology::LineList,
            Topology::Points => wgpu::PrimitiveTopology::PointList,
        }
    }
}

/// Whether a draw is in the depth the post-processing passes read: the
/// depth pass draws triangles, and segments and points have no surface.
fn writes_camera_depth(draw: &Draw<'_>) -> bool {
    draw.key.depth_write && draw.key.topology == Topology::Triangles
}

/// The vertices the lit shader and the authors' programs read.
fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRIBUTES,
    }
}

fn upload(device: &wgpu::Device, mesh: &Mesh) -> GpuMesh {
    let tangents = mesh.tangents();
    let vertices: Vec<Vertex> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .zip(&tangents)
        .enumerate()
        .map(|(i, ((position, normal), tangent))| Vertex {
            position: *position,
            normal: *normal,
            uv: mesh.uvs.get(i).copied().unwrap_or_default(),
            tangent: *tangent,
        })
        .collect();
    GpuMesh {
        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ljos_3d vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        extra: gpu_extra::upload_extra(device, mesh),
        has_joints: !mesh.joints.is_empty(),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ljos_3d indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: mesh.indices.len() as u32,
        topology: mesh.topology,
    }
}

/// Binds a mesh's buffers and the rows of its copies.
fn bind_mesh(pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMesh, rows: &wgpu::Buffer, copies: Copies) {
    let row = std::mem::size_of::<InstanceRow>() as u64;
    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
    pass.set_vertex_buffer(1, rows.slice(u64::from(copies.first) * row..));
    if let Some(extra) = &mesh.extra {
        pass.set_vertex_buffer(2, extra.slice(..));
    }
    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
}

/// Pushes the rows of an instanced model's copies onto `rows`, the
/// farthest from the camera first if the instancing sorts them. A copy's
/// alpha is 1 unless the copies blend.
fn copy_rows(
    scene: &Scene,
    id: crate::scene::NodeId,
    world: &Mat4,
    instancing: &Instancing,
    opaque: bool,
    view: &Mat4,
    rows: &mut Vec<InstanceRow>,
) {
    let global = instancing.global(scene, id, world);
    let back = global.inverse().unwrap_or(Mat4::IDENTITY);
    let first = rows.len();
    let entries = instancing.drawn();
    rows.extend(entries.iter().map(|e| {
        let mut color = e.color;
        if opaque {
            color[3] = 1.0;
        }
        InstanceRow::new(&(global * e.transform * back), color)
    }));
    if instancing.depth_sort {
        let origin = world.translation();
        let depth =
            |r: &InstanceRow| -transform_point(view, transform_point(&r.matrix(), origin)).z;
        rows[first..].sort_by(|a, b| depth(b).total_cmp(&depth(a)));
    }
    debug_assert_eq!(
        instance_worlds(global, *world, entries).count(),
        rows.len() - first
    );
}

impl Renderer {
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
        if let Some(timer) = &mut target.timer {
            timer.begin(queue);
        }
        let [w, h] = target.size;
        let walked = scene.walk();
        let environment = scene.environment();
        let samples = self.samples_for(environment.msaa_samples);
        // Post-processing works on the light as it is, kept in half
        // floats; a device without them draws the scene plain.
        let processed = environment.post_processed() && self.probes;
        let format = if processed { post::HDR } else { COLOR_FORMAT };
        target.multisample(device, samples, format);
        target.frames += 1;
        if !processed {
            target.post.clear();
        }
        self.maps.sweep();
        self.programs.sweep();
        self.ibl.sweep();
        self.ibl.ready(device, queue);
        let probe = environment.light_probe.as_ref();
        let bound = match probe {
            Some(p) if self.probes => self.ibl.prepare(device, queue, encoder, p),
            _ => Bound::None,
        };

        let camera = scene.active_camera();
        let (view, proj, view_proj, sky) = match camera {
            Some((camera, world)) => {
                let view = world.inverse().unwrap_or(Mat4::IDENTITY);
                let proj = camera.projection(w as f32, h as f32);
                let mut turn = view;
                turn.0[3] = [0.0, 0.0, 0.0, 1.0];
                let sky = (proj * turn).inverse().unwrap_or(Mat4::IDENTITY);
                (view, proj, proj * view, sky)
            }
            None => (
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
            ),
        };
        let mut frame = FrameUniform::zeroed();
        frame.view_proj = view_proj.0;
        frame.sky = sky.0;
        let last_level = self.ibl.last_level(bound);
        environment_uniforms(&mut frame, environment, probe, bound, last_level);
        if processed {
            // Tone mapping is the last pass's.
            frame.tone[0] = 5.0;
        }
        if let Some((camera, world)) = camera {
            let p = world.translation();
            let d = world.forward();
            let ortho = matches!(camera, Camera::Orthographic { .. });
            frame.camera = [p.x, p.y, p.z, f32::from(u8::from(ortho))];
            frame.camera_dir = [d.x, d.y, d.z, 0.0];
        }
        let mut ambient = [0.0f32; 3];
        let mut count = 0;
        // The lights casting shadows, and where each is among the lights.
        let mut shadowing = Vec::new();
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
            if let Some(shadow) = &node.shadow
                && shadowing.len() < shadow::MAX_SHADOW_LIGHTS
            {
                shadowing.push((count, light, world, shadow));
            }
            count += 1;
        }
        frame.ambient = [ambient[0], ambient[1], ambient[2], 0.0];
        frame.counts[0] = count as u32;

        let mut opaque = Vec::new();
        let mut blended = Vec::new();
        // Models the camera leaves out still cast shadows into what it
        // sees; their uniforms go after the drawn models'.
        let mut casters: Vec<Caster> = Vec::new();
        let mut unseen = Vec::new();
        let mut bounds = Vec::new();
        let shadows = !shadowing.is_empty();
        // Row 0 and joint slot 0 are what a plain model reads.
        let mut rows = vec![InstanceRow::IDENTITY];
        let mut joint_slots: Vec<JointSlot> = vec![gpu_extra::joint_slot(&[])];
        if let Some((camera, _)) = camera {
            let planes = camera.frustum_culling().then(|| frustum(&view_proj));
            for (id, node, world) in &walked {
                let NodeKind::Model(model) = &node.kind else {
                    continue;
                };
                let key = match &model.mesh {
                    MeshSource::Primitive(p) => MeshKey::Primitive(*p),
                    MeshSource::Custom(g) => {
                        if !self.custom_mesh(device, g) {
                            continue;
                        }
                        MeshKey::Custom(g.id())
                    }
                };
                let (extra, has_joints) = match key {
                    MeshKey::Primitive(_) => (false, false),
                    MeshKey::Custom(_) => {
                        let mesh = self.gpu_mesh(key);
                        (mesh.extra.is_some(), mesh.has_joints)
                    }
                };
                // An author's program reads one copy and no joints (its
                // vertex stage has no INSTANCE_ or skin built-ins yet), so
                // it is drawn, cast and culled as one unbent model.
                let plain = matches!(model.material, Material::Custom(_));
                // A skinned mesh is placed by its joints alone: their
                // matrices take it into the scene.
                let palette = node
                    .skin
                    .as_ref()
                    .filter(|_| has_joints && !plain)
                    .map(|s| s.palette(scene));
                let placed = if palette.is_some() {
                    Mat4::IDENTITY
                } else {
                    *world
                };
                let (local_centre, local_radius) = model.mesh.bounds();
                let sphere = |m: &Mat4| {
                    (
                        transform_point(m, local_centre),
                        local_radius * largest_scale(m),
                    )
                };
                // Each vertex is a blend of its joints' places, so it is
                // within the spheres the joints carry the mesh's to.
                let spheres: Vec<(Vec3, f32)> = match &palette {
                    Some(p) if !p.is_empty() => p.iter().map(sphere).collect(),
                    _ => vec![sphere(&placed)],
                };
                let mut copies = Copies::ONE;
                let mut transparent = false;
                let (centre, radius) = match node.instancing.as_ref().filter(|_| !plain) {
                    None => gpu_extra::union(spheres.iter().copied()).unwrap_or_default(),
                    Some(instancing) => {
                        let first = rows.len();
                        transparent = instancing.transparent;
                        let opaque = !(transparent || model.material.blends());
                        copy_rows(scene, *id, world, instancing, opaque, &view, &mut rows);
                        if rows.len() == first {
                            continue;
                        }
                        copies.first = first as u32;
                        copies.count = (rows.len() - first) as u32;
                        let places = rows[first..].iter().map(InstanceRow::matrix);
                        let all = places.flat_map(|m| {
                            spheres
                                .iter()
                                .map(move |&(c, r)| (transform_point(&m, c), r * largest_scale(&m)))
                        });
                        gpu_extra::union(all).unwrap_or_default()
                    }
                };
                let mut uniform = model_uniform(&model.material, &placed);
                uniform.params2[3] = f32::from(u8::from(node.receives_shadows));
                if let Some(palette) = &palette {
                    copies.joints = joint_slots.len() as u32;
                    joint_slots.push(gpu_extra::joint_slot(palette));
                    uniform.maps[3] = 1;
                }
                // A segment or a point has no volume to shadow with.
                let casts = node.casts_shadows && self.topology_of(key) == Topology::Triangles;
                let caster = (shadows && casts).then(|| {
                    casters.push(Caster {
                        mesh: key,
                        cull: model.material.cull_mode(),
                        copies,
                        extra,
                        world: placed,
                        centre,
                        radius,
                        slot: 0,
                    });
                    casters.len() - 1
                });
                if shadows && (node.casts_shadows || node.receives_shadows) {
                    bounds.push((centre, radius));
                }
                if let Some(planes) = &planes
                    && outside(planes, centre, radius)
                {
                    if let Some(c) = caster {
                        unseen.push((c, uniform));
                    }
                    continue;
                }
                let blends = model.material.blends() || transparent;
                let custom = match &model.material {
                    Material::Custom(m) => Some((
                        m,
                        MaterialKey {
                            program: m.program.id(),
                            cull: m.cull_mode,
                            depth_write: m.depth_draw.writes(blends),
                            samples,
                            format,
                            blend: blends.then_some((m.source_blend, m.destination_blend)),
                            topology: self.topology_of(key),
                        },
                    )),
                    _ => None,
                };
                let draw = Draw {
                    mesh: key,
                    custom,
                    maps: self.maps.prepare(device, queue, &model.material),
                    key: PipelineKey {
                        cull: model.material.cull_mode(),
                        depth_write: custom.map_or(!blends, |(_, k)| k.depth_write),
                        samples,
                        extra,
                        format,
                        topology: self.topology_of(key),
                    },
                    uniform,
                    copies,
                    depth: -transform_point(&view, centre).z,
                    caster,
                };
                if blends {
                    blended.push(draw);
                } else {
                    opaque.push(draw);
                }
            }
        }

        let lights: Vec<shadow::Caster> = shadowing
            .iter()
            .map(|&(_, light, world, shadow)| shadow::Caster {
                light,
                world,
                shadow,
            })
            .collect();
        let plan = if shadows {
            Plan::new(
                &lights,
                &bounds,
                camera.as_ref().map(|(camera, _)| shadow::View {
                    camera,
                    view_proj: &view_proj,
                }),
                device.limits().max_texture_dimension_2d,
            )
        } else {
            Plan::default()
        };
        for (&(i, _, _, s), &(first, n)) in shadowing.iter().zip(&plan.lights) {
            frame.lights[i].shadow = [
                first as f32,
                n as f32,
                s.factor.clamp(0.0, 1.0),
                s.bias.max(0.0),
            ];
            frame.lights[i].shadow2 = [
                s.pcf_radius.max(0.0),
                s.samples.clamp(1, 16) as f32,
                s.cascades.blend.clamp(0.0, 1.0),
                0.0,
            ];
        }
        frame.shadow_layers[..plan.layers.len()].copy_from_slice(&plan.layers);
        target.shadows(device, self, &plan);
        if target.frame_written != Some(frame) {
            queue.write_buffer(&target.frame_buffer, 0, bytemuck::bytes_of(&frame));
            target.frame_written = Some(frame);
        }

        // Over what is opaque, from the farthest in: each blends over what
        // is behind it. A stable sort keeps the tree's order where two are
        // as far.
        blended.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        let mut draws = opaque;
        draws.append(&mut blended);

        for (i, draw) in draws.iter().enumerate() {
            if let Some(c) = draw.caster {
                casters[c].slot = i;
            }
        }
        let mut uniforms: Vec<ModelUniform> = draws.iter().map(|d| d.uniform).collect();
        for (c, uniform) in unseen {
            casters[c].slot = uniforms.len();
            uniforms.push(uniform);
        }
        target.instances.write(device, queue, &rows);
        if target.joints.write(device, queue, &joint_slots) {
            target.model_group = model_group(
                device,
                &self.model_layout,
                &target.model_buffer,
                &target.joints.buffer,
            );
        }
        if uniforms.len() as u64 > target.model_capacity {
            target.model_capacity = (uniforms.len() as u64).next_power_of_two();
            (target.model_buffer, target.model_group) = model_slots(
                device,
                &self.model_layout,
                target.model_capacity,
                self.model_stride,
                &target.joints.buffer,
            );
            // A new buffer holds nothing yet.
            target.slots_written.clear();
        }
        write_slots(queue, target, &uniforms, self.model_stride);
        let (near, far) = camera.map_or((0.0, 1.0), |(c, _)| clip_planes(&c));
        let inverse_projection = proj.inverse().unwrap_or(Mat4::IDENTITY);
        let mut slot = 0;
        for draw in &draws {
            if let MeshKey::Primitive(p) = draw.mesh {
                self.mesh(device, p);
            }
            match draw.custom {
                Some((material, key)) => {
                    self.programs.material_pipeline(
                        device,
                        &material.program,
                        key,
                        [&self.frame_layout, &self.model_layout, &self.ibl.layout],
                        vertex_layout(),
                        DEPTH_FORMAT,
                    );
                    let header = Header {
                        model: draw.uniform.world,
                        normal: draw.uniform.normal,
                        view: view.0,
                        projection: proj.0,
                        inverse_projection: inverse_projection.0,
                        view_projection: view_proj.0,
                        camera: frame.camera,
                        direction: frame.camera_dir,
                        clip: [near, far, target.frames as f32, 0.0],
                        sizes: [w as f32, h as f32, w as f32, h as f32],
                    };
                    self.programs.bind_material(
                        device,
                        queue,
                        &mut self.maps,
                        &mut target.custom_slots,
                        slot,
                        material,
                        &header,
                    );
                    slot += 1;
                }
                None => self.pipeline(device, draw.key),
            }
        }
        target.custom_slots.truncate(slot);
        let depth_pass = processed && post::needs_depth(environment);
        if depth_pass {
            for draw in draws.iter().filter(|d| writes_camera_depth(d)) {
                self.shadow_pipeline(device, draw.key.cull, draw.key.extra);
            }
        }
        for caster in &casters {
            if let MeshKey::Primitive(p) = caster.mesh {
                self.mesh(device, p);
            }
            self.shadow_pipeline(device, caster.cull, caster.extra);
        }
        let passes = target.shadow_passes;
        self.draw_shadows(
            queue,
            encoder,
            target,
            &plan,
            &casters,
            (&rows, &joint_slots),
        );
        let shadows_drawn = target.shadow_passes != passes;
        // A geometry no scene holds is dropped with its buffers.
        self.custom.retain(|_, c| c.source.strong_count() > 0);

        let plain_maps = self.maps.plain(device, queue);
        let sky = environment.background == Background::SkyBox && bound != Bound::None;
        if sky {
            self.sky(device, samples, format);
        }
        let clear = match environment.background {
            Background::Transparent | Background::SkyBox => wgpu::Color::TRANSPARENT,
            // Drawn for the tonemapper, the colour is light, as the
            // models' are.
            Background::Color(c) if processed => {
                let l = c.to_linear();
                wgpu::Color {
                    r: f64::from(l[0] * c.a),
                    g: f64::from(l[1] * c.a),
                    b: f64::from(l[2] * c.a),
                    a: f64::from(c.a),
                }
            }
            Background::Color(c) => wgpu::Color {
                r: f64::from(c.r * c.a),
                g: f64::from(c.g * c.a),
                b: f64::from(c.b * c.a),
                a: f64::from(c.a),
            },
        };
        let scene_view =
            processed.then(|| target.post.view(device, post::SCENE, [w, h], post::HDR));
        {
            // Multisampled, the picture is drawn into the samples and
            // resolved into the target's texture, which is all the blit
            // and a read back ever see. Post-processed, it is drawn into
            // half floats the passes read.
            let picture = scene_view.as_ref().unwrap_or(&target.color_view);
            let (view, resolve, depth) = match &target.multisample {
                Some(m) => (&m.color, Some(picture), &m.depth),
                None => (picture, None, &target.depth_view),
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
                timestamp_writes: target
                    .timer
                    .as_ref()
                    .and_then(crate::timing::Timer::scene_pass),
                ..Default::default()
            });
            pass.set_bind_group(0, &target.frame_group, &[]);
            pass.set_bind_group(3, self.ibl.group(bound), &[]);
            if sky {
                // Its layout is the materials', so every group is bound
                // for it, though it reads only the frame and the probe.
                pass.set_pipeline(&self.skies[&(samples, format)]);
                pass.set_bind_group(1, &target.model_group, &[0, 0]);
                pass.set_bind_group(2, self.maps.group(&plain_maps), &[]);
                pass.draw(0..3, 0..1);
            }
            let mut bound = None;
            let mut bound_maps = None;
            let mut slot = 0;
            for (i, draw) in draws.iter().enumerate() {
                if let Some((_, key)) = draw.custom {
                    // A program's pipeline and block are its own, so the
                    // next lit draw binds its own again.
                    pass.set_pipeline(self.programs.material(&key));
                    pass.set_bind_group(2, &target.custom_slots[slot].group, &[]);
                    slot += 1;
                    bound = None;
                    bound_maps = None;
                } else {
                    if bound != Some(draw.key) {
                        pass.set_pipeline(&self.pipelines[&draw.key]);
                        bound = Some(draw.key);
                    }
                    if bound_maps != Some(&draw.maps) {
                        pass.set_bind_group(2, self.maps.group(&draw.maps), &[]);
                        bound_maps = Some(&draw.maps);
                    }
                }
                let mesh = self.gpu_mesh(draw.mesh);
                let offset = (i as u64 * self.model_stride) as u32;
                let joints = (u64::from(draw.copies.joints) * JOINT_SLOT) as u32;
                pass.set_bind_group(1, &target.model_group, &[offset, joints]);
                bind_mesh(&mut pass, mesh, &target.instances.buffer, draw.copies);
                pass.draw_indexed(0..mesh.count, 0, 0..draw.copies.count);
            }
        }
        if processed {
            if depth_pass {
                self.draw_depth(device, encoder, target, &draws, view_proj);
            }
            Chain {
                device,
                queue,
                encoder,
                pipes: &mut self.post,
                programs: &mut self.programs,
                maps: &mut self.maps,
                buffers: &mut target.post,
                environment,
                view: post::View {
                    view: view.0,
                    projection: proj.0,
                    inverse_projection: inverse_projection.0,
                    view_projection: view_proj.0,
                    camera: frame.camera,
                    direction: frame.camera_dir,
                    near,
                    far,
                },
                size: [w, h],
                output: &target.color_view,
                output_format: COLOR_FORMAT,
                frame: target.frames as f32,
            }
            .run();
        }
        if let Some(timer) = &mut target.timer {
            timer.end(encoder, shadows_drawn);
        }
        target.drawn_models = draws.len();
        crate::stats::record_render(
            (draws.len() + usize::from(sky)) as u32,
            target.shadow_passes,
        );
        target.drawn = Some(scene.revision());
        true
    }

    /// Draws the depth the post-processing passes read: what writes depth
    /// in the picture, seen from the camera.
    fn draw_depth(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &mut Target,
        draws: &[Draw<'_>],
        view_proj: Mat4,
    ) {
        let [w, h] = target.size;
        let depth = target
            .post
            .view(device, post::DEPTH_KEY, [w, h], post::DEPTH);
        let layer = ShadowLayer {
            view_proj: view_proj.0,
            params: [0.0; 4],
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ljos_3d camera depth"),
            contents: bytemuck::bytes_of(&layer),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d camera depth"),
            layout: &self.layer_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<ShadowLayer>() as u64),
                }),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ljos_3d camera depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &group, &[0]);
        let mut bound = None;
        for (i, draw) in draws.iter().enumerate() {
            if !writes_camera_depth(draw) {
                continue;
            }
            let pipeline = (draw.key.cull, draw.key.extra);
            if bound != Some(pipeline) {
                pass.set_pipeline(&self.shadow_pipelines[&pipeline]);
                bound = Some(pipeline);
            }
            let mesh = self.gpu_mesh(draw.mesh);
            let offset = (i as u64 * self.model_stride) as u32;
            let joints = (u64::from(draw.copies.joints) * JOINT_SLOT) as u32;
            pass.set_bind_group(1, &target.model_group, &[offset, joints]);
            bind_mesh(&mut pass, mesh, &target.instances.buffer, draw.copies);
            pass.draw_indexed(0..mesh.count, 0, 0..draw.copies.count);
        }
    }

    /// Draws the shadow maps, unless they already show what the plan
    /// and the casters would draw into them.
    fn draw_shadows(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &mut Target,
        plan: &Plan,
        casters: &[Caster],
        (instances, joints): (&[InstanceRow], &[JointSlot]),
    ) {
        let Some(shadows) = target.shadows.as_mut() else {
            return;
        };
        let key = ShadowKey {
            layers: plan.layers.clone(),
            casters: casters
                .iter()
                .map(|c| {
                    let revision = match c.mesh {
                        MeshKey::Primitive(_) => 0,
                        MeshKey::Custom(id) => self.custom[&id].revision,
                    };
                    (c.mesh, revision, c.world.0, c.cull, c.copies)
                })
                .collect(),
            instances: instances.to_vec(),
            joints: joints.to_vec(),
        };
        if shadows.drawn.as_ref() == Some(&key) {
            return;
        }
        let stride = self.layer_stride as usize;
        let size = std::mem::size_of::<ShadowLayer>();
        let mut bytes = vec![0u8; (plan.layers.len() - 1) * stride + size];
        for (i, layer) in plan.layers.iter().enumerate() {
            bytes[i * stride..i * stride + size].copy_from_slice(bytemuck::bytes_of(layer));
        }
        queue.write_buffer(&shadows.layer_buffer, 0, &bytes);
        let n = plan.layers.len();
        for (i, layer) in plan.layers.iter().enumerate() {
            let planes = frustum(&Mat4(layer.view_proj));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadows.maps.views[i],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: target.timer.as_ref().and_then(|t| t.shadow_pass(i, n)),
                ..Default::default()
            });
            let offset = (i as u64 * self.layer_stride) as u32;
            pass.set_bind_group(0, &shadows.layer_group, &[offset]);
            let mut bound = None;
            for c in casters {
                if outside(&planes, c.centre, c.radius) {
                    continue;
                }
                if bound != Some((c.cull, c.extra)) {
                    pass.set_pipeline(&self.shadow_pipelines[&(c.cull, c.extra)]);
                    bound = Some((c.cull, c.extra));
                }
                let mesh = self.gpu_mesh(c.mesh);
                let offset = (c.slot as u64 * self.model_stride) as u32;
                let joints = (u64::from(c.copies.joints) * JOINT_SLOT) as u32;
                pass.set_bind_group(1, &target.model_group, &[offset, joints]);
                bind_mesh(&mut pass, mesh, &target.instances.buffer, c.copies);
                pass.draw_indexed(0..mesh.count, 0, 0..c.copies.count);
            }
            target.shadow_passes += 1;
        }
        shadows.drawn = Some(key);
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
