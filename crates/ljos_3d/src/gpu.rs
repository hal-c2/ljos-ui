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

use crate::batch;
use crate::copies::{self, HeldCopies};
use crate::custom::{CustomMaterial, Screen};
use crate::gbuffer;
use crate::geometry::CustomGeometry;
use crate::gpu_extra::{self, InstanceRow, JOINT_SLOT, JointSlot, Rows};
use crate::ibl::{self, Bound, Ibl};
use crate::maps::{self, GroupKey, Maps};
use crate::math::{Mat4, Quat, Vec3};
use crate::mesh::{Mesh, Topology};
use crate::post::{self, Buffers, Chain, Pipes};
use crate::programs::{self, Header, MaterialKey, Programs, Slot};
use crate::scene::{
    AlphaMode, Background, BlendMode, Camera, CullMode, Environment, Light, LightProbe, Lighting,
    Material, MeshSource, Node, NodeId, NodeKind, Primitive, ReflectionProbe, Scene, Tonemap,
};
use crate::screen::ScreenTexture;
use crate::shadow::{self, Layer as ShadowLayer, Plan};
use crate::uniform_slots::UniformSlots;

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
    /// The fog's linear colour and density.
    fog: [f32; 4],
    /// Where the distance's fog starts and is whole, its curve, 1 when on.
    fog_depth: [f32; 4],
    /// The y of the least and of the most, the curve, 1 when on.
    fog_height: [f32; 4],
    /// The transmission's curve, 1 when on, and 1 where highlights are
    /// widened against aliasing.
    fog_transmit: [f32; 4],
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
    /// A dielectric's reflection: its amount, its tint, what it is head on,
    /// and the Fresnel power; a `DefaultMaterial` has the power alone.
    shine: [f32; 4],
    /// The Fresnel scale and bias, the clearcoat's amount and its roughness.
    edge: [f32; 4],
    /// The clearcoat's Fresnel power, scale and bias, and the transmission.
    coat_edge: [f32; 4],
    /// A `DefaultMaterial`'s linear specular tint; w the height amount.
    tint: [f32; 4],
    /// A bit per light that lights it, 1 to use the vertices' colours, the
    /// fewest and most height samples (low and high half), and the channels
    /// of transmission (bits 0-1) and height (2-3).
    flags: [u32; 4],
    /// The ambient of the lights scoped to it.
    ambient: [f32; 4],
    /// Its own probe's last level, 1 where it has one, and its exposure.
    probe: [f32; 4],
    /// Where a reflection probe looked from, 1 where what is read of it is
    /// put right; and the corners of its box.
    mirror_at: [f32; 4],
    mirror_min: [f32; 4],
    mirror_max: [f32; 4],
    /// An instanced model's copies are each placed by their entry inside
    /// these, the instancing's place in front and its inverse behind: the
    /// rows then hold the table as it is and need writing only when it
    /// changes, not when what holds it moves. Identities for a model drawn
    /// once.
    copies_global: [[f32; 4]; 4],
    copies_back: [[f32; 4]; 4],
}

/// A reflection probe of this frame, as the models near it read it.
#[derive(Clone, Copy)]
struct Mirror {
    key: u64,
    at: Vec3,
    min: Vec3,
    max: Vec3,
    parallax: bool,
}

/// What is kept of a reflection probe between frames.
struct HeldMirror {
    across: u32,
    /// A side each, each with its own uniforms: those are written ahead of
    /// the commands, so one target could not be drawn through six times.
    targets: Vec<Target>,
    /// What it last saw, for one that looks once.
    there: Option<(u64, bool, usize)>,
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
    blend: BlendMode,
    /// Off, a surface is drawn over all that was drawn before it.
    depth_test: bool,
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
    /// Copies the picture for the materials that read the screen.
    screen: crate::screen::Pipes,
    /// Whether the device renders into and filters half floats, which the
    /// probe's maps are; without, the scene draws with no probe.
    probes: bool,
    /// Whether models alike are drawn as one ([`Renderer::set_batching`]).
    batching: bool,
    mirrors: HashMap<u64, HeldMirror>,
    /// Sample counts both the picture's and the depth's format take,
    /// ascending; 1 always among them.
    sample_counts: Vec<u32>,
    shadow_shader: wgpu::ShaderModule,
    shadow_layout: wgpu::PipelineLayout,
    layer_layout: wgpu::BindGroupLayout,
    layer_stride: u64,
    /// By cull mode, and whether the mesh has a second vertex buffer.
    shadow_pipelines: HashMap<(CullMode, bool), wgpu::RenderPipeline>,
    surfaces: gbuffer::Surfaces,
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
    /// Which model's uniforms are in which of those slots.
    model_slots: UniformSlots<UniformKey>,
    /// What is kept of the instanced models' copies between frames.
    copies: HeldCopies,
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
    /// Drawn into in place of the half floats of a post-processed scene:
    /// a side of a reflection probe's cube.
    capture: Option<wgpu::TextureView>,
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
    /// What a material's DEPTH_TEXTURE reads, also when nothing is post-processed.
    material_depth: Buffers,
    /// The scene behind the materials that read it, while one does.
    screen: Option<ScreenTexture>,
    /// Draws the picture `screen` holds, while a material reads it.
    behind: Option<Box<Target>>,
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
/// A caster as its shadow shows it: mesh, mesh revision, world, culling,
/// copies, and what places the copies' rows.
type CasterKey = (
    MeshKey,
    u64,
    [[f32; 4]; 4],
    CullMode,
    Copies,
    [[[f32; 4]; 4]; 2],
);

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
    /// What its uniforms put in front of a copy's row and behind it.
    places: (Mat4, Mat4),
    extra: bool,
    world: Mat4,
    centre: Vec3,
    radius: f32,
    /// Its uniforms' slot.
    slot: u32,
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
    crate::counted::create_texture(
        device,
        &wgpu::TextureDescriptor {
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
        },
    )
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
        let blit_group = crate::counted::create_bind_group(
            device,
            &wgpu::BindGroupDescriptor {
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
            },
        );
        let frame_buffer = crate::counted::create_buffer(
            device,
            &wgpu::BufferDescriptor {
                label: Some("ljos_3d frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
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
            capture: None,
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
            model_slots: UniformSlots::default(),
            copies: HeldCopies::default(),
            slots_written: Vec::new(),
            staging: Vec::new(),
            drawn: None,
            drawn_models: 0,
            shadows: None,
            shadow_passes: 0,
            timer: crate::timing::Timer::new(device),
            custom_slots: Vec::new(),
            post: Buffers::default(),
            material_depth: Buffers::default(),
            screen: None,
            behind: None,
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

    /// How long the GPU took for the passes after the scene pass the last
    /// time that was read back: the camera's depth, ambient occlusion,
    /// effects, depth of field, glow, the final tonemap and FXAA, where
    /// the environment post-processes. Not in [`Self::last_gpu_time`].
    pub fn last_gpu_post_time(&self) -> Option<std::time::Duration> {
        self.timer
            .as_ref()
            .and_then(crate::timing::Timer::last_post)
    }

    /// [`Self::last_gpu_post_time`], only if it was read since the last
    /// time this was asked.
    pub fn take_gpu_post_time(&mut self) -> Option<std::time::Duration> {
        self.timer
            .as_mut()
            .and_then(crate::timing::Timer::take_post)
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
                let buffer = crate::counted::create_buffer(
                    device,
                    &wgpu::BufferDescriptor {
                        label: Some("ljos_3d shadow layers"),
                        size: shadow::MAX_LAYERS as u64 * renderer.layer_stride,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    },
                );
                let group = crate::counted::create_bind_group(
                    device,
                    &wgpu::BindGroupDescriptor {
                        label: Some("ljos_3d shadow layers"),
                        layout: &renderer.layer_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &buffer,
                                offset: 0,
                                size: wgpu::BufferSize::new(
                                    std::mem::size_of::<ShadowLayer>() as u64
                                ),
                            }),
                        }],
                    },
                );
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
    crate::counted::create_bind_group(
        device,
        &wgpu::BindGroupDescriptor {
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
        },
    )
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
    let buffer = crate::counted::create_buffer(
        device,
        &wgpu::BufferDescriptor {
            label: Some("ljos_3d models"),
            size: capacity * stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
    );
    let group = model_group(device, layout, &buffer, joints);
    (buffer, group)
}

fn model_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    models: &wgpu::Buffer,
    joints: &wgpu::Buffer,
) -> wgpu::BindGroup {
    crate::counted::create_bind_group(
        device,
        &wgpu::BindGroupDescriptor {
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
        },
    )
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
pub(crate) fn largest_scale(world: &Mat4) -> f32 {
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
        map_params: [1.0, 1.0, 1.0, 0.0],
        shine: [1.0, 0.0, 0.04, 5.0],
        edge: [1.0, 0.0, 0.0, 0.0],
        coat_edge: [5.0, 1.0, 0.0, 0.0],
        tint: [1.0, 1.0, 1.0, 0.0],
        flags: [u32::MAX, 0, 0, 0],
        ambient: [0.0; 4],
        probe: [0.0; 4],
        mirror_at: [0.0; 4],
        mirror_min: [0.0; 4],
        mirror_max: [0.0; 4],
        copies_global: Mat4::IDENTITY.0,
        copies_back: Mat4::IDENTITY.0,
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
            u.maps[1] = m.opacity_channel.index() << 6 | u32::from(m.invert_opacity_map_value) << 8;
            u.map_params[0] = m.bump_amount;
            u.emissive = [m.emissive[0], m.emissive[1], m.emissive[2], 0.0];
            let tint = m.specular_tint.to_linear();
            u.tint = [tint[0], tint[1], tint[2], 0.0];
            u.shine[2] = ((m.index_of_refraction - 1.0) / (m.index_of_refraction + 1.0)).powi(2);
            u.shine[3] = m.fresnel_power;
            u.flags[1] = u32::from(m.vertex_colors);
            u.base[3] = material.alpha().clamp(0.0, 1.0);
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
                | maps.opacity_channel.index() << 6
                | u32::from(maps.invert_opacity_map_value) << 8;
            u.map_params = [
                maps.normal_strength,
                // With no picture nothing is taken away, whatever the amount.
                maps.occlusion
                    .as_ref()
                    .map_or(1.0, |_| maps.occlusion_amount.clamp(0.0, 1.0)),
                1.0,
                0.0,
            ];
            let s = &maps.surface;
            let ior = s.index_of_refraction;
            let f0 = ((ior - 1.0) / (ior + 1.0)).powi(2);
            u.shine = [
                s.specular_amount.clamp(0.0, 1.0),
                s.specular_tint.clamp(0.0, 1.0),
                f0,
                s.fresnel.power,
            ];
            u.edge = [
                s.fresnel.scale,
                s.fresnel.bias,
                s.clearcoat_amount.clamp(0.0, 1.0),
                s.clearcoat_roughness.clamp(0.0, 1.0),
            ];
            u.coat_edge = [
                s.clearcoat_fresnel.power,
                s.clearcoat_fresnel.scale,
                s.clearcoat_fresnel.bias,
                s.transmission_factor.clamp(0.0, 1.0),
            ];
            u.tint[3] = s.height_amount;
            u.tint[0] = maps.clearcoat_normal_strength;
            u.flags[1] = u32::from(s.vertex_colors);
            u.flags[2] = s.height_samples[0].min(0xffff) | s.height_samples[1].min(0xffff) << 16;
            u.flags[3] = maps.transmission_channel.index()
                | maps.height_channel.index() << 2
                | maps.clearcoat_channel.index() << 4
                | maps.clearcoat_roughness_channel.index() << 6;
        }
        // Its snippets read only the matrices and whether it takes shadows.
        Material::Custom(_) => u.base = [1.0; 4],
    }
    u
}

/// How a surface is laid over what is behind it. The shader's colours are
/// premultiplied, which is what these factors take.
fn blend_state(mode: BlendMode) -> wgpu::BlendState {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation, BlendState};
    let color = |src_factor, dst_factor| BlendComponent {
        src_factor,
        dst_factor,
        operation: BlendOperation::Add,
    };
    let alpha = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    };
    match mode {
        BlendMode::SourceOver => BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        BlendMode::Screen => BlendState {
            color: color(BlendFactor::One, BlendFactor::OneMinusSrc),
            alpha,
        },
        BlendMode::Multiply => BlendState {
            color: color(BlendFactor::Dst, BlendFactor::OneMinusSrcAlpha),
            alpha,
        },
    }
}

/// Writes the environment's fog and whether highlights are widened.
/// A depth fog with no far of its own is whole at the camera's far clip.
fn fog_uniforms(frame: &mut FrameUniform, environment: &Environment, clip_far: f32) {
    frame.fog_transmit[2] = f32::from(u8::from(environment.specular_aa));
    let Some(fog) = &environment.fog else {
        return;
    };
    let c = fog.color.to_linear();
    frame.fog = [c[0], c[1], c[2], fog.density];
    if let Some([near, far, curve]) = fog.depth {
        let far = if far > 0.0 { far } else { clip_far };
        frame.fog_depth = [near, far, curve, 1.0];
    }
    if let Some([least, most, curve]) = fog.height {
        frame.fog_height = [least, most, curve, 1.0];
    }
    if let Some(curve) = fog.transmit {
        frame.fog_transmit[0] = curve;
        frame.fog_transmit[1] = 1.0;
    }
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
fn write_slots(queue: &wgpu::Queue, target: &mut Target, uniforms: &[ModelUniform], stride: u64) {
    let size = std::mem::size_of::<ModelUniform>();
    let stride = stride as usize;
    let written = &mut target.slots_written;
    let changed = |i: usize, written: &[ModelUniform]| written.get(i) != Some(&uniforms[i]);
    let mut i = 0;
    while i < uniforms.len() {
        if !changed(i, written) {
            i += 1;
            continue;
        }
        let start = i;
        while i < uniforms.len() && changed(i, written) {
            i += 1;
        }
        let staging = &mut target.staging;
        staging.clear();
        staging.resize((i - start - 1) * stride + size, 0);
        for (k, uniform) in uniforms[start..i].iter().enumerate() {
            staging[k * stride..k * stride + size].copy_from_slice(bytemuck::bytes_of(uniform));
        }
        crate::counted::write_buffer(
            queue,
            &target.model_buffer,
            (start * stride) as u64,
            staging,
        );
    }
    written.clear();
    written.extend_from_slice(uniforms);
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Where its uniforms are in the target's buffer.
    slot: u32,
    copies: Copies,
    /// How far in front of the camera: blended models are drawn from the
    /// farthest.
    depth: f32,
    /// The box it takes in the scene: where it may meet another.
    reach: batch::Reach,
    /// The material's own light probe, bound for it in place of the scene's.
    probe: Option<Bound>,
    /// Whether a depth bias orders it: drawn last of those at one depth,
    /// it is the one seen there.
    biased: bool,
    /// Where it is in the scene's declaration order, which the depth sort
    /// keeps among those at one depth.
    order: u32,
    /// The model drawn, or the first of those drawn with it as one.
    node: NodeId,
    /// How many models it draws.
    models: u32,
}

/// What a slot in a target's model buffer holds the uniforms of: a model,
/// or the models drawn as one with it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum UniformKey {
    Model(NodeId),
    Batch(NodeId),
}

/// What draws are drawn as one by: the same mesh, pipeline and maps, and
/// uniforms the same but for where each model is and its colour, those
/// hashed ([`unplaced`]).
type BatchKey = (MeshKey, PipelineKey, GroupKey, u64);

/// A model's uniforms but for its place and colour, which a row of the
/// instance buffer carries for the models drawn as one: the shader puts
/// the row's place in front of the uniforms' and multiplies the row's
/// colour into the uniforms' base, so the same figures come out.
fn unplaced(u: &ModelUniform) -> ModelUniform {
    ModelUniform {
        world: Mat4::IDENTITY.0,
        normal: Mat4::IDENTITY.0,
        base: [1.0; 4],
        ..*u
    }
}

/// Where in a model's uniforms what [`unplaced`] keeps begins: the place
/// and colour come first, so what models drawn as one share is the rest,
/// read where it is rather than copied out.
const SHARED: usize = std::mem::offset_of!(ModelUniform, emissive);
const _: () = assert!(
    std::mem::offset_of!(ModelUniform, world) < SHARED
        && std::mem::offset_of!(ModelUniform, normal) < SHARED
        && std::mem::offset_of!(ModelUniform, base) < SHARED
        && SHARED.is_multiple_of(8)
        && size_of::<ModelUniform>().is_multiple_of(8)
);

/// What models drawn as one share of `u`'s uniforms.
fn shared(u: &ModelUniform) -> &[u8] {
    &bytemuck::bytes_of(u)[SHARED..]
}

/// What `draw` is drawn as one with others by, or `None` for a draw drawn
/// alone. The models of a batch are drawn in one order, so only those whose
/// order the depth test has the last word on are drawn together. It passes
/// at equal depth, so of two surfaces in one place the one drawn last shows,
/// and a depth bias is how a scene says which: a biased model draws alone.
/// What a row cannot carry draws alone too: a model's own copies, a skin,
/// morphs or vertex colours, a light probe of its own, a program reading
/// its own uniforms.
fn batch_key(draw: &Draw<'_>) -> Option<BatchKey> {
    let alone = draw.custom.is_some()
        || draw.copies != Copies::ONE
        || draw.key.extra
        || draw.probe.is_some()
        || draw.biased
        || !draw.key.depth_write
        || !draw.key.depth_test;
    if alone {
        return None;
    }
    // FNV-1a over four lanes of eight bytes: a thousand models' uniforms are
    // hashed each frame, the lanes do not wait on each other, and a
    // collision only costs the comparison `batched` makes.
    let mut lanes = [0xcbf2_9ce4_8422_2325_u64; 4];
    for chunk in shared(&draw.uniform).chunks(32) {
        for (lane, word) in lanes.iter_mut().zip(chunk.chunks(8)) {
            let word = u64::from_le_bytes(word.try_into().expect("eight bytes"));
            *lane = (*lane ^ word).wrapping_mul(0x0100_0000_01b3);
        }
    }
    let hash = lanes.iter().fold(0, |h: u64, &l| h.rotate_left(17) ^ l);
    Some((draw.mesh, draw.key, draw.maps, hash))
}

/// `draws` with those [`batch_key`] finds alike drawn as one, where the
/// nearest of them is: each model's place and colour in a row of `rows`, and
/// the uniforms they share in a slot of their own.
fn batched<'a>(
    mut draws: Vec<Draw<'a>>,
    rows: &mut Vec<InstanceRow>,
    slots: &mut UniformSlots<UniformKey>,
    uniforms: &mut Vec<(u32, ModelUniform)>,
) -> Vec<Draw<'a>> {
    // A batch is drawn where its nearest is, before what is drawn between
    // its models, and its rows are in their declaration order; the depth
    // test lets the one drawn last show where two are as deep. So no batch
    // reaches across a draw drawn alone, which may be the one seen there (a
    // depth bias says so without moving it, a draw that writes no depth is
    // seen by its order), nor turns round two models that may meet.
    let keys: Vec<Option<BatchKey>> = draws.iter().map(batch_key).collect();
    let alike = |a: usize, b: usize| {
        keys[a] == keys[b] && shared(&draws[a].uniform) == shared(&draws[b].uniform)
    };
    let reach: Vec<batch::Reach> = draws.iter().map(|d| d.reach).collect();
    let order: Vec<u32> = draws.iter().map(|d| d.order).collect();
    let firsts = batch::firsts(&keys, alike, &reach, &order);
    let keyed = keys
        .iter()
        .zip(&firsts)
        .map(|(key, &first)| key.map(|key| (key, first)));
    let mut groups = batch::groups(keyed, alike);
    if groups.len() == draws.len() {
        return draws;
    }
    // A group is drawn where its nearest is; its rows are laid out in the
    // scene's declaration order, which a camera that moves changes neither
    // within a batch nor between batches. The rows are then written again
    // only when a model moves or a batch gains or loses one. Two that may
    // meet and came in the other order are not in one batch (`firsts`).
    let nearest: Vec<usize> = groups.iter().map(|g| g[0]).collect();
    for group in &mut groups {
        group.sort_by_key(|&i| draws[i].order);
    }
    let mut laid_out: Vec<usize> = (0..groups.len()).filter(|&g| groups[g].len() > 1).collect();
    laid_out.sort_by_key(|&g| draws[groups[g][0]].order);
    let mut first = vec![0; groups.len()];
    for g in laid_out {
        first[g] = rows.len() as u32;
        rows.extend(groups[g].iter().map(|&i| {
            let u = &draws[i].uniform;
            InstanceRow::new(&Mat4(u.world), u.base, [0.0; 4])
        }));
    }
    let leads: Vec<NodeId> = groups.iter().map(|g| draws[g[0]].node).collect();
    // A group's nearest is its first draw, so the groups' nearest come in
    // the draws' order: each draw kept is a group's, in place.
    let (mut at, mut g) = (0, 0);
    draws.retain_mut(|draw| {
        let kept = nearest.get(g) == Some(&at);
        at += 1;
        if !kept {
            return false;
        }
        let group = &groups[g];
        if group.len() > 1 {
            draw.uniform = unplaced(&draw.uniform);
            draw.slot = slots.slot(UniformKey::Batch(leads[g]));
            uniforms.push((draw.slot, draw.uniform));
            draw.copies = Copies {
                first: first[g],
                count: group.len() as u32,
                joints: 0,
            };
            draw.models = group.len() as u32;
        }
        g += 1;
        true
    });
    draws
}

/// Masks the lights scoped away from model `id` and adds the ambient of the
/// ones it is inside.
fn scope_uniform(
    u: &mut ModelUniform,
    scene: &Scene,
    id: NodeId,
    scoped: &[(NodeId, u32, [f32; 4])],
) {
    for &(scope, bit, ambient) in scoped {
        let mut at = Some(id);
        let mut inside = false;
        while let Some(n) = at {
            if n == scope {
                inside = true;
                break;
            }
            at = scene.node(n).and_then(Node::parent);
        }
        if inside {
            for (sum, c) in u.ambient.iter_mut().zip(ambient) {
                *sum += c;
            }
        } else {
            u.flags[0] &= !bit;
        }
    }
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
    // The way looked along is turned by it, not back: as Qt reads a probe.
    frame.probe_rot = turn.0;
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let frame_layout = shadow::frame_layout(device);
        let layer_layout = uniform_layout(device, "ljos_3d shadow layer", true);
        let model_layout = gpu_extra::model_layout(device);
        let maps = Maps::new(device);
        let ibl = Ibl::new(device);
        let shader = crate::counted::create_shader_module(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("ljos_3d lit"),
                source: wgpu::ShaderSource::Wgsl(LIT_WGSL.into()),
            },
        );
        let sky_shader = crate::counted::create_shader_module(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("ljos_3d sky"),
                source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
            },
        );
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
        let shadow_shader = crate::counted::create_shader_module(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("ljos_3d shadow"),
                source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
            },
        );
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
            // Half floats, where they turn out to be there, on the first frame.
            screen: crate::screen::Pipes::new(device, false),
            // WebGPU has half floats to draw into and filter; a host on
            // another backend says whether it has with
            // `use_adapter_formats`.
            probes: true,
            batching: true,
            mirrors: HashMap::new(),
            // What WebGPU promises every device for these formats.
            sample_counts: vec![1, 4],
            shadow_shader,
            shadow_layout,
            layer_layout,
            layer_stride,
            shadow_pipelines: HashMap::new(),
            surfaces: gbuffer::Surfaces::default(),
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

    /// Whether models that differ only in where they are and their colour
    /// are drawn as one, the copies of one instanced draw: on unless turned
    /// off, which draws each on its own, as to compare the two.
    pub fn set_batching(&mut self, on: bool) {
        self.batching = on;
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
            crate::counted::create_render_pipeline(
                device,
                &wgpu::RenderPipelineDescriptor {
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
                        depth_write_enabled: Some(key.depth_write && key.depth_test),
                        depth_compare: Some(if key.depth_test {
                            wgpu::CompareFunction::LessEqual
                        } else {
                            wgpu::CompareFunction::Always
                        }),
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
                            blend: Some(blend_state(key.blend)),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                },
            )
        });
    }

    /// The depth pass of casters culled `cull`: the faces the picture
    /// shows are the ones the maps hold, so a surface's own depth is in
    /// them, and its slope and the bias are what keep it from shadowing itself.
    fn shadow_pipeline(&mut self, device: &wgpu::Device, cull: CullMode, extra: bool) {
        let (shader, layout) = (&self.shadow_shader, &self.shadow_layout);
        self.shadow_pipelines
            .entry((cull, extra))
            .or_insert_with(|| {
                crate::counted::create_render_pipeline(
                    device,
                    &wgpu::RenderPipelineDescriptor {
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
                            depth_compare: Some(wgpu::CompareFunction::LessEqual),
                            stencil: Default::default(),
                            // A surface turned from the light covers more depth per texel of
                            // the map than any fixed bias clears, the flatter the more.
                            bias: wgpu::DepthBiasState {
                                constant: 0,
                                slope_scale: 1.5,
                                clamp: 0.0,
                            },
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
                    },
                )
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
            crate::counted::create_render_pipeline(
                device,
                &wgpu::RenderPipelineDescriptor {
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
                },
            )
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
fn material_screen(material: &Material) -> Screen {
    match material {
        Material::Custom(m) => m.program.screen(),
        _ => Screen::None,
    }
}

fn screen_of(draw: &Draw<'_>) -> Screen {
    draw.custom
        .map_or(Screen::None, |(m, _)| m.program.screen())
}

fn surface_key(draw: &Draw<'_>) -> (Option<wgpu::Face>, bool, bool) {
    let cull = match draw.key.cull {
        CullMode::Back => Some(wgpu::Face::Back),
        CullMode::Front => Some(wgpu::Face::Front),
        CullMode::None => None,
    };
    (cull, draw.key.extra, draw.custom.is_some())
}

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
        vertices: crate::counted::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("ljos_3d vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ),
        extra: gpu_extra::upload_extra(device, mesh),
        has_joints: !mesh.joints.is_empty(),
        indices: crate::counted::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("ljos_3d indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ),
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

/// The way each side of a cube is looked through, and which way is up on
/// it, in the order of the cube's layers.
const CUBE_LOOKS: [([f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
];

impl Renderer {
    /// Draws what is round each reflection probe that something mirrors by,
    /// and says which that is of each model: of those whose box its own
    /// meets, the one whose middle is nearest.
    fn reflections(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        walked: &[(NodeId, &Node, Mat4)],
    ) -> HashMap<NodeId, Mirror> {
        let mut found: Vec<(Mirror, ReflectionProbe)> = Vec::new();
        for (id, node, world) in walked {
            let NodeKind::ReflectionProbe(probe) = &node.kind else {
                continue;
            };
            let at = world.translation();
            let middle = at + probe.box_offset;
            let half = probe.box_size * 0.5;
            found.push((
                Mirror {
                    key: slotmap::Key::data(id).as_ffi(),
                    at,
                    min: middle - half,
                    max: middle + half,
                    parallax: probe.parallax,
                },
                *probe,
            ));
        }
        let mut chosen = HashMap::new();
        if found.is_empty() {
            self.forget_mirrors(&[]);
            return chosen;
        }
        for (id, node, world) in walked {
            let NodeKind::Model(model) = &node.kind else {
                continue;
            };
            if !node.receives_reflections {
                continue;
            }
            let (centre, radius) = model.mesh.bounds();
            let centre = transform_point(world, centre);
            let radius = radius * largest_scale(world);
            let reach = Vec3::new(radius, radius, radius);
            let least = centre - reach;
            let most = centre + reach;
            let mut nearest = f32::INFINITY;
            for (mirror, _) in &found {
                let apart = mirror.min.x > most.x
                    || least.x > mirror.max.x
                    || mirror.min.y > most.y
                    || least.y > mirror.max.y
                    || mirror.min.z > most.z
                    || least.z > mirror.max.z;
                if apart {
                    continue;
                }
                let middle = (mirror.min + mirror.max) * 0.5;
                let d = (centre - middle).length();
                if d < nearest {
                    nearest = d;
                    chosen.insert(*id, *mirror);
                }
            }
        }
        let used: Vec<u64> = chosen.values().map(|m| m.key).collect();
        self.forget_mirrors(&used);
        let mut drawing: Vec<u64> = used;
        drawing.sort_unstable();
        drawing.dedup();
        let mut held = std::mem::take(&mut self.mirrors);
        for key in drawing {
            let Some((mirror, probe)) = found.iter().find(|(m, _)| m.key == key) else {
                continue;
            };
            if held.get(&key).is_some_and(|h| h.across != probe.across) {
                held.remove(&key);
                self.ibl.forget_mirror(key);
            }
            let across = probe.across;
            let renderer = &*self;
            let entry = held.entry(key).or_insert_with(|| HeldMirror {
                across,
                targets: (0..6)
                    .map(|_| Target::new(device, renderer, [across, across]))
                    .collect(),
                there: None,
            });
            let models = walked
                .iter()
                .filter(|(_, n, _)| n.casts_reflections && matches!(n.kind, NodeKind::Model(_)))
                .count();
            let sky = scene.environment().background == Background::SkyBox;
            let there = (probe.turn, sky, models);
            if probe.once && entry.there == Some(there) {
                continue;
            }
            entry.there = Some(there);
            let sides = self.ibl.mirror(device, queue, key, across).sides.clone();
            let mut seen = scene.mirrored(probe.clear);
            for (side, (target, (way, up))) in entry.targets.iter_mut().zip(CUBE_LOOKS).enumerate()
            {
                let way = Vec3::new(way[0], way[1], way[2]);
                let up = Vec3::new(up[0], up[1], up[2]);
                let right = way.cross(up);
                let at = mirror.at;
                seen.look(
                    Camera::Perspective {
                        fov_degrees: 90.0,
                        fov_horizontal: false,
                        clip_near: 1.0,
                        clip_far: 10000.0,
                        frustum_culling: false,
                    },
                    Mat4([
                        [right.x, right.y, right.z, 0.0],
                        [up.x, up.y, up.z, 0.0],
                        [-way.x, -way.y, -way.z, 0.0],
                        [at.x, at.y, at.z, 1.0],
                    ]),
                );
                target.capture = Some(sides[side].clone());
                target.drawn = None;
                self.render(device, queue, encoder, &seen, target);
            }
            self.ibl.finish_mirror(encoder, key);
        }
        self.mirrors = held;
        chosen
    }

    fn forget_mirrors(&mut self, used: &[u64]) {
        let gone: Vec<u64> = self
            .mirrors
            .keys()
            .filter(|k| !used.contains(k))
            .copied()
            .collect();
        for key in gone {
            self.mirrors.remove(&key);
            self.ibl.forget_mirror(key);
        }
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
        let mut clock = crate::stats::Stopwatch::start();
        if let Some(timer) = &mut target.timer {
            timer.begin(queue);
        }
        let [w, h] = target.size;
        let walked = scene.walk();
        let environment = scene.environment();
        let samples = self.samples_for(environment.msaa_samples);
        // Post-processing works on the light as it is, kept in half
        // floats; a device without them draws the scene plain.
        let captured = environment.capture.is_some();
        let processed = (environment.post_processed() || captured) && self.probes;
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

        let mirrored = if captured || !self.probes {
            HashMap::new()
        } else {
            self.reflections(device, queue, encoder, scene, &walked)
        };

        // What reads the picture of what is behind it reads light as it is,
        // as Qt has it: what is opaque is drawn for it apart from what the
        // view shows, with the view's own tone mapping left out.
        let screen = if environment.behind {
            Screen::None
        } else {
            walked
                .iter()
                .filter_map(|(_, node, _)| match &node.kind {
                    NodeKind::Model(model) => Some(material_screen(&model.material)),
                    _ => None,
                })
                .max()
                .unwrap_or(Screen::None)
        };
        if screen != Screen::None && self.screen.hdr() != self.probes {
            self.screen = crate::screen::Pipes::new(device, self.probes);
        }
        self.screen.fit(device, &mut target.screen, [w, h], screen);
        if let Some(texture) = &target.screen {
            let mut held = target.behind.take().filter(|b| b.size == [w, h]);
            let behind = held.get_or_insert_with(|| Box::new(Target::new(device, self, [w, h])));
            behind.capture = Some(texture.first_level().clone());
            behind.drawn = None;
            self.render(device, queue, encoder, &scene.behind(), behind);
            self.screen.make_levels(encoder, texture);
            target.behind = held;
        } else {
            target.behind = None;
        }

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
        let clip_far = camera.map_or(1.0, |(c, _)| clip_planes(&c).1);
        fog_uniforms(&mut frame, environment, clip_far);
        if processed && (!captured || environment.behind) {
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
        // The lights that light only what is inside a node: the node, the
        // light's bit and its ambient.
        let mut scoped = Vec::new();
        for (_, node, world) in &walked {
            let NodeKind::Light(light) = &node.kind else {
                continue;
            };
            if count == MAX_LIGHTS {
                break;
            }
            let a = light.common().2.to_linear();
            if let Some(scope) = node.scope {
                scoped.push((scope, 1u32 << count, a));
            } else {
                for (sum, c) in ambient.iter_mut().zip(a) {
                    *sum += c;
                }
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
        // sees, so every model's uniforms are kept, drawn or not, by slot.
        let mut casters: Vec<Caster> = Vec::new();
        let mut uniforms: Vec<(u32, ModelUniform)> = Vec::new();
        target.model_slots.begin();
        target.copies.begin();
        let mut bounds = Vec::new();
        let shadows = !shadowing.is_empty();
        // Row 0 and joint slot 0 are what a plain model reads.
        let mut rows = vec![InstanceRow::IDENTITY];
        let mut joint_slots: Vec<JointSlot> = vec![gpu_extra::joint_slot(&[])];
        if let Some((camera, _)) = camera {
            let planes = camera.frustum_culling().then(|| frustum(&view_proj));
            let faded = scene.faded();
            for (id, node, world) in &walked {
                let NodeKind::Model(model) = &node.kind else {
                    continue;
                };
                let opacity = faded
                    .as_ref()
                    .map_or(1.0, |f| f.get(*id).copied().unwrap_or(1.0));
                if opacity <= 0.0 || model.material.is_clear() {
                    continue;
                }
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
                // A skinned mesh is placed by its joints alone: their
                // matrices take it into the scene.
                let palette = node
                    .skin
                    .as_ref()
                    .filter(|_| has_joints)
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
                // Of the shapes, all but the rectangle are closed round
                // what their box holds, and show only their outside where
                // their back faces are culled. Culling front faces or none
                // shows the inside, as a mirroring place does by turning
                // the winding, and an inside face on the box's face is seen
                // with what touches it there.
                let solid = matches!(model.mesh, MeshSource::Primitive(p) if p != Primitive::Rectangle)
                    && model.material.cull_mode() == CullMode::Back
                    && crate::pick::determinant3(&placed) > 0.0;
                let reach = match &palette {
                    Some(p) if !p.is_empty() => batch::Reach::round(spheres.iter().copied()),
                    _ => batch::Reach::placed(&placed, model.mesh.bounding_box(), solid),
                };
                let mut copies = Copies::ONE;
                let mut places = (Mat4::IDENTITY, Mat4::IDENTITY);
                // A custom material's shader writes its own alpha, so it has
                // no share of the node's to take.
                let mut transparent =
                    opacity < 1.0 && !matches!(model.material, Material::Custom(_));
                let (centre, radius) = match &node.instancing {
                    None => gpu_extra::union(spheres.iter().copied()).unwrap_or_default(),
                    Some(instancing) => {
                        let first = rows.len();
                        transparent |= instancing.transparent;
                        let opaque = !(transparent || model.material.blends());
                        let global = instancing.global(scene, *id, world);
                        let local = instancing.local(scene, *id, world, &global);
                        // The spheres in the table's space: the mesh's
                        // placed there as the node has it, to the bit while
                        // only what is above moves, or the joints' taken
                        // out of the scene.
                        let in_table: Vec<(Vec3, f32)> = match &palette {
                            Some(p) if !p.is_empty() => {
                                let back = global.inverse().unwrap_or(Mat4::IDENTITY);
                                let scale = largest_scale(&back);
                                spheres
                                    .iter()
                                    .map(|&(c, r)| (transform_point(&back, c), r * scale))
                                    .collect()
                            }
                            _ => vec![sphere(&local)],
                        };
                        let held = target.copies.push(
                            &copies::Model {
                                id: *id,
                                instancing,
                                global,
                                spheres: &in_table,
                                origin: local.translation(),
                                opaque,
                                depth_decides: opaque
                                    && model.material.depth_draw().writes(false)
                                    && environment.depth_test,
                                view: instancing.depth_sort.then_some(&view),
                            },
                            &mut rows,
                        );
                        let Some(held) = held else {
                            continue;
                        };
                        copies.first = first as u32;
                        copies.count = (rows.len() - first) as u32;
                        places = (held.global, held.back);
                        held.sphere
                    }
                };
                let mut uniform = model_uniform(&model.material, &placed);
                uniform.copies_global = places.0.0;
                uniform.copies_back = places.1.0;
                uniform.params2[3] = f32::from(u8::from(node.receives_shadows));
                uniform.map_params[2] = opacity;
                // Lit by its own shader, a copy's colour is part of its surface.
                if node.instancing.is_some() && matches!(model.material, Material::Custom(_)) {
                    uniform.map_params[3] = 1.0;
                }
                if !scoped.is_empty() {
                    scope_uniform(&mut uniform, scene, *id, &scoped);
                }
                let own_probe = match &model.material {
                    Material::Principled(m) if self.probes => {
                        m.maps.surface.light_probe.as_ref().map(|texture| {
                            self.ibl.prepare_texture(device, queue, encoder, texture)
                        })
                    }
                    _ => None,
                }
                .filter(|b| *b != Bound::None);
                if let Some(own) = own_probe {
                    uniform.probe = [self.ibl.last_level(own), 1.0, frame.tone[1].signum(), 0.0];
                }
                let mirror = mirrored.get(id);
                if let Some(mirror) = mirror {
                    uniform.probe = [
                        self.ibl.last_level(Bound::Mirror(mirror.key)),
                        2.0,
                        0.0,
                        0.0,
                    ];
                    uniform.mirror_at = [
                        mirror.at.x,
                        mirror.at.y,
                        mirror.at.z,
                        f32::from(u8::from(mirror.parallax)),
                    ];
                    uniform.mirror_min = [mirror.min.x, mirror.min.y, mirror.min.z, 0.0];
                    uniform.mirror_max = [mirror.max.x, mirror.max.y, mirror.max.z, 0.0];
                }
                let own_probe = mirror.map(|m| Bound::Mirror(m.key)).or(own_probe);
                if let Some(palette) = &palette {
                    copies.joints = joint_slots.len() as u32;
                    joint_slots.push(gpu_extra::joint_slot(palette));
                    uniform.maps[3] = 1;
                }
                let slot = target.model_slots.slot(UniformKey::Model(*id));
                uniforms.push((slot, uniform));
                // A segment or a point has no volume to shadow with.
                let casts = node.casts_shadows && self.topology_of(key) == Topology::Triangles;
                if shadows && casts {
                    casters.push(Caster {
                        mesh: key,
                        cull: model.material.cull_mode(),
                        copies,
                        places,
                        extra,
                        world: placed,
                        centre,
                        radius,
                        slot,
                    });
                }
                if shadows && (node.casts_shadows || node.receives_shadows) {
                    bounds.push((centre, radius));
                }
                if let Some(planes) = &planes
                    && outside(planes, centre, radius)
                {
                    crate::stats::count(|c| c.models_culled += 1);
                    continue;
                }
                let blends = model.material.blends() || transparent;
                let reads = material_screen(&model.material) != Screen::None;
                if environment.behind && (blends || reads) {
                    continue;
                }
                let custom = match &model.material {
                    Material::Custom(m) => Some((
                        m,
                        MaterialKey {
                            program: m.program.id(),
                            cull: m.cull_mode,
                            // What reads hides as what is opaque does, blended or not.
                            depth_write: m.depth_draw.writes(blends && !reads),
                            samples,
                            format,
                            blend: blends.then_some(programs::Blending {
                                source: m.source_blend,
                                destination: m.destination_blend,
                                source_alpha: m.source_alpha_blend,
                                destination_alpha: m.destination_alpha_blend,
                            }),
                            topology: self.topology_of(key),
                            extra,
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
                        depth_write: custom
                            .map_or(model.material.depth_draw().writes(blends), |(_, k)| {
                                k.depth_write
                            }),
                        samples,
                        extra,
                        format,
                        topology: self.topology_of(key),
                        blend: model.material.blend_mode(),
                        depth_test: environment.depth_test,
                    },
                    uniform,
                    slot,
                    copies,
                    // Qt counts a bias times itself, and below nothing as nearer.
                    depth: -transform_point(&view, centre).z
                        + node.depth_bias.signum() * node.depth_bias * node.depth_bias,
                    reach,
                    probe: own_probe,
                    biased: node.depth_bias != 0.0,
                    order: (opaque.len() + blended.len()) as u32,
                    node: *id,
                    models: 1,
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
            crate::counted::write_buffer(
                queue,
                &target.frame_buffer,
                0,
                bytemuck::bytes_of(&frame),
            );
            target.frame_written = Some(frame);
        }

        // Over what is opaque, from the farthest in: each blends over what
        // is behind it. A stable sort keeps the tree's order where two are
        // as far. What reads the picture goes before all that is seen
        // through, however far each is, as Qt has it.
        let (mut draws, mut reading): (Vec<_>, Vec<_>) = opaque
            .into_iter()
            .partition(|d| screen_of(d) == Screen::None);
        let (mut read_blended, mut blended): (Vec<_>, Vec<_>) = blended
            .into_iter()
            .partition(|d| screen_of(d) != Screen::None);
        reading.append(&mut read_blended);
        // What nothing is seen through goes nearest first, as Qt has it: where
        // one does not hide, the farther is drawn over it.
        draws.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        // The shadow casters read the rows before the batches': a batched
        // model's colour is no reason to draw a shadow map again.
        let cast_rows = rows.len();
        if self.batching {
            draws = batched(draws, &mut rows, &mut target.model_slots, &mut uniforms);
        }
        reading.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        blended.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        draws.append(&mut reading);
        draws.append(&mut blended);

        target.copies.end();
        target.instances.write(device, queue, &rows);
        if target.joints.write(device, queue, &joint_slots) {
            target.model_group = model_group(
                device,
                &self.model_layout,
                &target.model_buffer,
                &target.joints.buffer,
            );
        }
        let slots = target.model_slots.len();
        if slots as u64 > target.model_capacity {
            target.model_capacity = (slots as u64).next_power_of_two();
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
        // A slot no model has this frame keeps what it holds.
        let mut by_slot: Vec<ModelUniform> = (0..slots)
            .map(|i| {
                target
                    .slots_written
                    .get(i)
                    .copied()
                    .unwrap_or_else(ModelUniform::zeroed)
            })
            .collect();
        for (slot, uniform) in uniforms {
            by_slot[slot as usize] = uniform;
        }
        write_slots(queue, target, &by_slot, self.model_stride);
        let (near, far) = camera.map_or((0.0, 1.0), |(c, _)| clip_planes(&c));
        let inverse_projection = proj.inverse().unwrap_or(Mat4::IDENTITY);
        // Materials read it while they draw, so it is drawn before the scene;
        // the post passes' own is drawn after, and only when they ask.
        let material_depth = draws
            .iter()
            .any(|d| d.custom.is_some_and(|(m, _)| m.program.reads_depth()))
            .then(|| {
                target
                    .material_depth
                    .view(device, post::DEPTH_KEY, [w, h], post::DEPTH)
            });
        if material_depth.is_none() {
            target.material_depth.clear();
        }
        // A view changes only with its size, as it is kept otherwise.
        let depth_generation = (u64::from(w) << 32) | u64::from(h);
        if material_depth.is_some() {
            for draw in draws.iter().filter(|d| writes_camera_depth(d)) {
                self.shadow_pipeline(device, draw.key.cull, draw.key.extra);
            }
        }
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
                        (material.program.screen() != Screen::None)
                            .then_some(target.screen.as_ref())
                            .flatten()
                            .map(|s| (&s.view, s.generation)),
                        material_depth
                            .as_ref()
                            .filter(|_| material.program.reads_depth())
                            .map(|d| (d, depth_generation)),
                    );
                    slot += 1;
                }
                None => self.pipeline(device, draw.key),
            }
        }
        target.custom_slots.truncate(slot);
        // What is before this is preparing the frame, what is after it
        // recording its passes; the pipelines the passes need are made
        // here too, so they are part of preparing.
        let surface_pass =
            processed && post::needs_surfaces(environment) && self.surfaces.available(device);
        let depth_pass = processed && !surface_pass && post::needs_depth(environment);
        if depth_pass {
            for draw in draws.iter().filter(|d| writes_camera_depth(d)) {
                self.shadow_pipeline(device, draw.key.cull, draw.key.extra);
            }
        }
        if surface_pass {
            for draw in draws.iter().filter(|d| writes_camera_depth(d)) {
                self.surfaces.prepare(
                    device,
                    &self.shader,
                    &self.pipeline_layout,
                    &[
                        Some(vertex_layout()),
                        Some(gpu_extra::instance_layout()),
                        draw.key.extra.then(gpu_extra::extra_layout),
                    ],
                    surface_key(draw),
                );
            }
        }
        for caster in &casters {
            if let MeshKey::Primitive(p) = caster.mesh {
                self.mesh(device, p);
            }
            self.shadow_pipeline(device, caster.cull, caster.extra);
        }
        let prepared = clock.lap();
        let passes = target.shadow_passes;
        self.draw_shadows(
            queue,
            encoder,
            target,
            &plan,
            &casters,
            (&rows[..cast_rows], &joint_slots),
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
            _ if captured => {
                let c = environment.capture.unwrap_or_default();
                wgpu::Color {
                    r: f64::from(c[0] * c[3]),
                    g: f64::from(c[1] * c[3]),
                    b: f64::from(c[2] * c[3]),
                    a: f64::from(c[3]),
                }
            }
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
            // A colour of the screen's, brought to linear light and back as
            // everything drawn is, which is not where it started: a
            // SceneEnvironment shows #101010 as 11.
            Background::Color(c) => {
                let [r, g, b, _] = c.to_linear();
                let l = environment.tonemap.shown([r, g, b]);
                wgpu::Color {
                    r: f64::from(l[0] * c.a),
                    g: f64::from(l[1] * c.a),
                    b: f64::from(l[2] * c.a),
                    a: f64::from(c.a),
                }
            }
        };
        let scene_view = target.capture.clone().or_else(|| {
            processed.then(|| target.post.view(device, post::SCENE, [w, h], post::HDR))
        });
        // Multisampled, the picture is drawn into the samples and resolved
        // into the target's texture, which is all the blit and a read back
        // ever see. Post-processed, it is drawn into half floats the
        // passes read.
        let picture = scene_view.as_ref().unwrap_or(&target.color_view);
        if let Some(depth) = &material_depth {
            self.draw_depth(device, encoder, target, &draws, view_proj, depth, false);
        }
        let mut slot = 0;
        {
            let (view, resolve, depth) = match &target.multisample {
                Some(m) => (&m.color, Some(picture), &m.depth),
                None => (picture, None, &target.depth_view),
            };
            let mut pass = crate::counted::Pass::begin(
                encoder,
                &wgpu::RenderPassDescriptor {
                    label: Some("ljos_3d scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: resolve,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: if resolve.is_none() {
                                wgpu::StoreOp::Store
                            } else {
                                wgpu::StoreOp::Discard
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
                    timestamp_writes: target.timer.as_ref().and_then(|t| t.scene_part(true, true)),
                    ..Default::default()
                },
            );
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
            let scene_probe = bound;
            let mut bound_probe = scene_probe;
            let mut bound = None;
            let mut bound_maps = None;
            // Models alike that are not drawn as one still share a mesh, and
            // plain ones the instance buffer's first row.
            let mut bound_model = None;
            let mut bound_mesh = None;
            for draw in &draws {
                let probe = draw.probe.unwrap_or(scene_probe);
                if probe != bound_probe {
                    pass.set_bind_group(3, self.ibl.group(probe), &[]);
                    bound_probe = probe;
                }
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
                let offset = (u64::from(draw.slot) * self.model_stride) as u32;
                let joints = (u64::from(draw.copies.joints) * JOINT_SLOT) as u32;
                if bound_model != Some((offset, joints)) {
                    pass.set_bind_group(1, &target.model_group, &[offset, joints]);
                    bound_model = Some((offset, joints));
                }
                if bound_mesh != Some((draw.mesh, draw.copies.first)) {
                    bind_mesh(&mut pass, mesh, &target.instances.buffer, draw.copies);
                    bound_mesh = Some((draw.mesh, draw.copies.first));
                }
                pass.draw_indexed(0..mesh.count, 0, 0..draw.copies.count);
            }
        }
        if processed && !environment.behind {
            if depth_pass {
                let depth = target
                    .post
                    .view(device, post::DEPTH_KEY, [w, h], post::DEPTH);
                self.draw_depth(device, encoder, target, &draws, view_proj, &depth, true);
            }
            if surface_pass {
                self.draw_surfaces(device, encoder, target, &draws, bound, &plain_maps);
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
                timer: target.timer.as_ref(),
                started: depth_pass || surface_pass,
                surfaces: surface_pass,
            }
            .run();
        }
        if let Some(timer) = &mut target.timer {
            timer.end(encoder, shadows_drawn, processed);
        }
        let models: usize = draws.iter().map(|d| d.models as usize).sum();
        target.drawn_models = models;
        let encoded = clock.lap();
        crate::stats::count(|c| {
            c.models_drawn += models as u64;
            c.prepare_seconds += prepared;
            c.encode_seconds += encoded;
        });
        crate::stats::record_render(
            (draws.len() + usize::from(sky)) as u32,
            target.shadow_passes,
        );
        target.drawn = Some(scene.revision());
        true
    }

    /// Draws the depth the post-processing passes read: what writes depth
    /// in the picture, seen from the camera.
    #[allow(clippy::too_many_arguments)]
    fn draw_depth(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &Target,
        draws: &[Draw<'_>],
        view_proj: Mat4,
        depth: &wgpu::TextureView,
        timed: bool,
    ) {
        let layer = ShadowLayer {
            view_proj: view_proj.0,
            params: [0.0; 4],
        };
        let buffer = crate::counted::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("ljos_3d camera depth"),
                contents: bytemuck::bytes_of(&layer),
                usage: wgpu::BufferUsages::UNIFORM,
            },
        );
        let group = crate::counted::create_bind_group(
            device,
            &wgpu::BindGroupDescriptor {
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
            },
        );
        let mut pass = crate::counted::Pass::begin(
            encoder,
            &wgpu::RenderPassDescriptor {
                label: Some("ljos_3d camera depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                // The post passes' time starts here, as they read this depth.
                timestamp_writes: target
                    .timer
                    .as_ref()
                    .filter(|_| timed)
                    .and_then(|t| t.post_pass(true, false)),
                ..Default::default()
            },
        );
        pass.set_bind_group(0, &group, &[0]);
        let mut bound = None;
        for draw in draws {
            if !writes_camera_depth(draw) {
                continue;
            }
            let pipeline = (draw.key.cull, draw.key.extra);
            if bound != Some(pipeline) {
                pass.set_pipeline(&self.shadow_pipelines[&pipeline]);
                bound = Some(pipeline);
            }
            let mesh = self.gpu_mesh(draw.mesh);
            let offset = (u64::from(draw.slot) * self.model_stride) as u32;
            let joints = (u64::from(draw.copies.joints) * JOINT_SLOT) as u32;
            pass.set_bind_group(1, &target.model_group, &[offset, joints]);
            bind_mesh(&mut pass, mesh, &target.instances.buffer, draw.copies);
            pass.draw_indexed(0..mesh.count, 0, 0..draw.copies.count);
        }
    }

    /// Draws the depth with each surface's normal and material, for the
    /// passes that read all three.
    fn draw_surfaces(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &mut Target,
        draws: &[Draw<'_>],
        bound: Bound,
        plain_maps: &GroupKey,
    ) {
        let size = target.size;
        let depth = target.post.view(device, post::DEPTH_KEY, size, post::DEPTH);
        let normal = target
            .post
            .view(device, post::NORMAL_KEY, size, gbuffer::NORMAL);
        let material = target
            .post
            .view(device, post::MATERIAL_KEY, size, gbuffer::MATERIAL);
        let attachment = |view, clear| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = crate::counted::Pass::begin(
            encoder,
            &wgpu::RenderPassDescriptor {
                label: Some("ljos_3d surfaces"),
                color_attachments: &[
                    attachment(&normal, gbuffer::EMPTY),
                    attachment(&material, wgpu::Color::TRANSPARENT),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                // The post passes' time starts here, as they read these.
                timestamp_writes: target.timer.as_ref().and_then(|t| t.post_pass(true, false)),
                ..Default::default()
            },
        );
        pass.set_bind_group(0, &target.frame_group, &[]);
        pass.set_bind_group(3, self.ibl.group(bound), &[]);
        for draw in draws {
            if !writes_camera_depth(draw) {
                continue;
            }
            pass.set_pipeline(self.surfaces.pipeline(surface_key(draw)));
            let maps = if draw.custom.is_some() {
                plain_maps
            } else {
                &draw.maps
            };
            pass.set_bind_group(2, self.maps.group(maps), &[]);
            let mesh = self.gpu_mesh(draw.mesh);
            let offset = (u64::from(draw.slot) * self.model_stride) as u32;
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
                    (
                        c.mesh,
                        revision,
                        c.world.0,
                        c.cull,
                        c.copies,
                        [c.places.0.0, c.places.1.0],
                    )
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
        crate::counted::write_buffer(queue, &shadows.layer_buffer, 0, &bytes);
        let n = plan.layers.len();
        for (i, layer) in plan.layers.iter().enumerate() {
            let planes = frustum(&Mat4(layer.view_proj));
            let mut pass = crate::counted::Pass::begin(
                encoder,
                &wgpu::RenderPassDescriptor {
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
                },
            );
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
                let offset = (u64::from(c.slot) * self.model_stride) as u32;
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
            let shader = crate::counted::create_shader_module(
                device,
                wgpu::ShaderModuleDescriptor {
                    label: Some("ljos_3d blit"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
                },
            );
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ljos_3d blit"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            crate::counted::create_render_pipeline(
                device,
                &wgpu::RenderPipelineDescriptor {
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
                },
            )
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
        // The pass is the host's, so what is set and drawn on it is
        // counted here.
        crate::counted::set_on_host_pass(1, 1);
        crate::counted::drawn(3, 1);
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
