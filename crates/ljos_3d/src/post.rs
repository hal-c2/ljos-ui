//! Post-processing: the passes a scene drawn in linear light goes through
//! before it is the target's picture.
//!
//! A scene with effects, ambient occlusion or an ExtendedSceneEnvironment
//! is drawn into half floats with its light as it is (tone mode 5). Then,
//! in order: ambient occlusion darkens it, the environment's effects run
//! their passes, global illumination, reflections, depth of field, glow
//! and the lens flare follow, and the last pass tonemaps and grades it
//! into the target, with FXAA after it where asked. The depth those passes
//! read is drawn by a pass of its own, with the surfaces' normals and
//! materials where reflections or global illumination read them.
//!
//! Each pass's uniforms and bind group are made when the scene is drawn,
//! which is only when it changed: a still scene runs none of this.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::custom::{
    BufferFormat, Effect, GlowBlend, Illumination, LensFlare, PassTexture, PostProcess,
};
use crate::maps::{self, Maps};
use crate::programs::{self, Header, Programs};
use crate::scene::{Environment, Tonemap};
use crate::timing::Timer;

/// What a post-processed scene is drawn into: light past 1 is kept for
/// the tonemapper, and the format renders and filters everywhere.
pub(crate) const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The depth the passes read, drawn by the shadow maps' pipeline.
pub(crate) const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const POST_WGSL: &str = concat!(
    include_str!("shaders/tone.wgsl"),
    include_str!("shaders/post.wgsl")
);

/// Where each picture of the chain is kept in [`Buffers`].
pub(crate) const SCENE: u32 = 0;
const PING: u32 = 1;
const PONG: u32 = 2;
const OCCLUSION: u32 = 3;
const FINISHED: u32 = 4;
pub(crate) const DEPTH_KEY: u32 = 5;
pub(crate) const NORMAL_KEY: u32 = 6;
pub(crate) const MATERIAL_KEY: u32 = 7;
const REFLECTED: u32 = 8;
const GATHERED: u32 = 40;
const SOFTENED: u32 = 41;
const SOFTENED_FIRST: u32 = 42;
const FLARE_BRIGHT: u32 = 50;
const FLARE: u32 = 51;
const FLARE_BLUR: u32 = 52;
const GLOW: u32 = 10;
const GLOW_BLUR: u32 = 20;
const GLOW_SUM: u32 = 30;
const EFFECT_BUFFERS: u32 = 1000;

/// The levels glow has, each half the size of the one before.
const GLOW_LEVELS: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    inverse: [[f32; 4]; 4],
    projection: [[f32; 4]; 4],
    texel: [f32; 4],
    tone: [f32; 4],
    ao: [f32; 4],
    ao2: [f32; 4],
    dof: [f32; 4],
    glow: [f32; 4],
    glow2: [f32; 4],
    adjust: [f32; 4],
    vignette: [f32; 4],
    vignette2: [f32; 4],
    lut: [f32; 4],
    view: [[f32; 4]; 4],
    flare: [f32; 4],
    flare2: [f32; 4],
    flare3: [f32; 4],
    ssr: [f32; 4],
    gi: [f32; 4],
    gi2: [f32; 4],
}

/// The built-in passes' pipelines on one device.
pub(crate) struct Pipes {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    pipelines: HashMap<(&'static str, wgpu::TextureFormat, bool), wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    /// What a pass binds where it reads nothing.
    none: Option<wgpu::TextureView>,
}

impl Pipes {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let texture = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d post"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(1, true),
                texture(2, true),
                // Depth is read as the float it is, never filtered.
                texture(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(5, true),
                texture(6, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d post"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d post"),
            source: wgpu::ShaderSource::Wgsl(POST_WGSL.into()),
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ljos_3d post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Pipes {
            layout,
            pipeline_layout,
            module,
            pipelines: HashMap::new(),
            sampler,
            none: None,
        }
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        entry: &'static str,
        format: wgpu::TextureFormat,
        add: bool,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.pipelines
            .entry((entry, format, add))
            .or_insert_with(|| {
                let blend = add.then_some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::OVER,
                });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module,
                        entry_point: Some("vs_full"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
    }
}

struct Pooled {
    view: wgpu::TextureView,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

/// A target's pictures between the passes, kept while their size and
/// format stay.
#[derive(Default)]
pub(crate) struct Buffers {
    pool: HashMap<u32, Pooled>,
}

impl Buffers {
    pub(crate) fn view(
        &mut self,
        device: &wgpu::Device,
        key: u32,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        let size = [size[0].max(1), size[1].max(1)];
        let pooled = self
            .pool
            .entry(key)
            .or_insert_with(|| make(device, size, format));
        if pooled.size != size || pooled.format != format {
            *pooled = make(device, size, format);
        }
        pooled.view.clone()
    }

    /// Lets go of the pictures a scene drawn without post-processing does
    /// not need.
    pub(crate) fn clear(&mut self) {
        self.pool.clear();
    }
}

fn make(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> Pooled {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ljos_3d post"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    Pooled {
        view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        size,
        format,
    }
}

/// Whether the passes read the scene's depth.
pub(crate) fn needs_depth(environment: &Environment) -> bool {
    environment.ambient_occlusion.is_some_and(|ao| ao.shows())
        || environment.post.depth_of_field.is_some()
        || environment.effects.iter().any(Effect::reads_depth)
        || needs_surfaces(environment)
}

/// Whether the passes read the surfaces' normals and materials too.
pub(crate) fn needs_surfaces(environment: &Environment) -> bool {
    let post = &environment.post;
    post.on && (post.reflections.is_some() || post.illumination.is_some())
}

/// The starburst's turn for a camera looking `d`, in its own space: none
/// looking straight ahead, and as much as it turns or tilts away.
fn starburst_turn(d: [f32; 3]) -> f32 {
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= f32::EPSILON {
        return 0.0;
    }
    let [x, y, z] = d.map(|c| c / len);
    // Not -z: looking straight up that is -0, whose angle is a half turn.
    x.atan2(0.0 - z) + y.clamp(-1.0, 1.0).asin()
}

/// The camera the passes see the scene with.
pub(crate) struct View {
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
    pub inverse_projection: [[f32; 4]; 4],
    pub view_projection: [[f32; 4]; 4],
    pub camera: [f32; 4],
    pub direction: [f32; 4],
    pub near: f32,
    pub far: f32,
}

/// What one run of the chain works with.
pub(crate) struct Chain<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub pipes: &'a mut Pipes,
    pub programs: &'a mut Programs,
    pub maps: &'a mut Maps,
    pub buffers: &'a mut Buffers,
    pub environment: &'a Environment,
    pub view: View,
    pub size: [u32; 2],
    /// The target's picture, which the last pass writes.
    pub output: &'a wgpu::TextureView,
    pub output_format: wgpu::TextureFormat,
    /// How many times the target was drawn, the effects' FRAME.
    pub frame: f32,
    /// Where the chain's passes write their GPU time, when it is timed.
    pub timer: Option<&'a Timer>,
    /// Whether a pass before the chain (the camera's depth) started the
    /// time already.
    pub started: bool,
    /// Whether the surfaces' normals and materials were drawn, without
    /// which reflections and global illumination are left out.
    pub surfaces: bool,
}

struct Pass<'v> {
    entry: &'static str,
    a: &'v wgpu::TextureView,
    b: Option<&'v wgpu::TextureView>,
    out: &'v wgpu::TextureView,
    format: wgpu::TextureFormat,
    add: bool,
    clear: bool,
    /// The chain's last pass, which ends its GPU time.
    last: bool,
}

impl<'a> Chain<'a> {
    /// Where a pass writes its timestamps: the chain's first its start,
    /// its last its end.
    fn stamp(&mut self, last: bool) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
        let first = !std::mem::replace(&mut self.started, true);
        self.timer.and_then(|t| t.post_pass(first, last))
    }

    fn uniform(&self) -> PostUniform {
        let env = self.environment;
        let post = &env.post;
        let mut u = PostUniform::zeroed();
        u.inverse = self.view.inverse_projection;
        u.projection = self.view.projection;
        let mode = match env.tonemap {
            Tonemap::None => 0.0,
            Tonemap::Linear => 1.0,
            Tonemap::Aces => 2.0,
            Tonemap::HejlDawson => 3.0,
            Tonemap::Filmic => 4.0,
        };
        u.tone = [
            mode,
            env.exposure.max(0.0),
            env.white_point,
            f32::from(u8::from(post.on && post.dithering)),
        ];
        if let Some(ao) = env.ambient_occlusion {
            u.ao = [
                (ao.strength / 100.0).clamp(0.0, 1.0),
                ao.distance.max(0.0),
                ao.softness.clamp(0.0, 50.0) / 10.0,
                (ao.sample_rate.clamp(2, 4) * 6) as f32,
            ];
            u.ao2 = [ao.bias, f32::from(u8::from(ao.dither)), 0.0, 0.0];
        }
        if let Some(d) = post.depth_of_field {
            u.dof = [d.focus_distance, d.focus_range, d.blur_amount.max(0.0), 0.0];
        }
        if let Some(g) = post.glow {
            let mode = match g.blend {
                GlowBlend::Additive => 0.0,
                GlowBlend::Screen => 1.0,
                GlowBlend::SoftLight => 2.0,
                GlowBlend::Replace => 3.0,
            };
            u.glow = [g.strength, g.intensity, g.bloom, mode];
            u.glow2 = [
                g.hdr_minimum,
                g.hdr_maximum,
                g.hdr_scale,
                f32::from(u8::from(g.quality_high)),
            ];
        }
        if let Some(a) = post.adjustments {
            u.adjust = [a.brightness, a.contrast, a.saturation, 1.0];
        }
        if let Some(v) = post.vignette {
            u.vignette = [v.color[0], v.color[1], v.color[2], 1.0];
            u.vignette2 = [v.radius, v.strength, 0.0, 0.0];
        }
        if let Some(l) = &post.lut {
            u.lut = [
                l.size.max(2.0),
                l.alpha,
                f32::from(u8::from(l.texture.is_some())),
                0.0,
            ];
        }
        u.lut[3] = post.sharpness.clamp(0.0, 1.0);
        u.view = self.view.view;
        if let Some(f) = &post.lens_flare {
            let from = |t: &Option<Option<_>>| match t {
                None => 0.0,
                Some(None) => 1.0,
                Some(Some(_)) => 2.0,
            };
            u.flare = [
                f.bloom_scale,
                f.bloom_bias,
                f.ghost_dispersal,
                f.ghost_count as f32,
            ];
            u.flare2 = [
                f.halo_width,
                f.stretch_to_aspect,
                f.distortion,
                f.blur_amount,
            ];
            u.flare3 = [
                0.0,
                from(&f.dirt),
                from(&f.starburst),
                starburst_turn(f.camera_direction),
            ];
        }
        if let Some(r) = post.reflections {
            u.ssr = [
                r.max_steps as f32,
                r.thickness,
                self.view.near,
                self.view.far,
            ];
        }
        if let Some(g) = post.illumination {
            u.gi = [
                g.hit_thickness,
                g.indirect_boost,
                f32::from(u8::from(g.indirect)),
                g.sample_count as f32,
            ];
            u.gi2 = [
                g.sample_radius,
                g.bounce.unwrap_or(0.0),
                g.slice_count as f32,
                g.buffer_size,
            ];
        }
        u
    }

    fn none(&mut self) -> wgpu::TextureView {
        let (device, queue) = (self.device, self.queue);
        self.pipes
            .none
            .get_or_insert_with(|| {
                let level = (1, 1, vec![0, 0, 0, 0]);
                maps::upload(device, queue, "ljos_3d post none", false, &[level])
            })
            .clone()
    }

    /// Draws one built-in pass. `texel` is a texel of what it reads and
    /// `dir` the blur's direction.
    fn pass(
        &mut self,
        p: Pass<'_>,
        base: &PostUniform,
        texel: [u32; 2],
        dir: [f32; 2],
        depth: Option<&wgpu::TextureView>,
    ) {
        self.pass_reading(p, base, texel, dir, depth, [None, None]);
    }

    /// [`Chain::pass`] with a third and a fourth picture.
    fn pass_reading(
        &mut self,
        p: Pass<'_>,
        base: &PostUniform,
        texel: [u32; 2],
        dir: [f32; 2],
        depth: Option<&wgpu::TextureView>,
        [c, d]: [Option<&wgpu::TextureView>; 2],
    ) {
        let mut u = *base;
        u.texel = [1.0 / texel[0] as f32, 1.0 / texel[1] as f32, dir[0], dir[1]];
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ljos_3d post"),
                contents: bytemuck::bytes_of(&u),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let none = self.none();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d post"),
            layout: &self.pipes.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(p.a),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(p.b.unwrap_or(&none)),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth.unwrap_or(&none)),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.pipes.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(c.unwrap_or(&none)),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(d.unwrap_or(&none)),
                },
            ],
        });
        let timestamp_writes = self.stamp(p.last);
        let pipeline = self.pipes.pipeline(self.device, p.entry, p.format, p.add);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(p.entry),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: p.out,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if p.clear || !p.add {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Runs the chain from the scene drawn into [`SCENE`].
    pub(crate) fn run(mut self) {
        let size = self.size;
        let base = self.uniform();
        let env = self.environment;
        let depth =
            needs_depth(env).then(|| self.buffers.view(self.device, DEPTH_KEY, size, DEPTH));
        let mut current = SCENE;
        let next = |current: u32| if current == PING { PONG } else { PING };

        if env.ambient_occlusion.is_some_and(|ao| ao.shows()) {
            let occlusion = self.buffers.view(
                self.device,
                OCCLUSION,
                size,
                wgpu::TextureFormat::Rgba8Unorm,
            );
            let none = self.none();
            self.pass(
                Pass {
                    entry: "fs_ao",
                    a: &none,
                    b: None,
                    out: &occlusion,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    add: false,
                    clear: true,
                    last: false,
                },
                &base,
                size,
                [0.0; 2],
                depth.as_ref(),
            );
            let out = next(current);
            let a = self.buffers.view(self.device, current, size, HDR);
            let o = self.buffers.view(self.device, out, size, HDR);
            self.pass(
                Pass {
                    entry: "fs_ao_apply",
                    a: &a,
                    b: Some(&occlusion),
                    out: &o,
                    format: HDR,
                    add: false,
                    clear: true,
                    last: false,
                },
                &base,
                size,
                [0.0; 2],
                None,
            );
            current = out;
        }

        for (e, effect) in env.effects.iter().enumerate() {
            if let Some(out) = self.effect(e as u32, effect, current, next(current), depth.as_ref())
            {
                current = out;
            }
        }

        if self.surfaces
            && let Some(depth) = &depth
        {
            if let Some(gi) = env.post.illumination {
                let out = next(current);
                self.illumination(&base, gi, depth, current, out);
                current = out;
            }
            if env.post.reflections.is_some() {
                let out = next(current);
                self.reflections(&base, depth, current, out);
                current = out;
            }
        }

        if env.post.on && env.post.depth_of_field.is_some() {
            let out = next(current);
            let a = self.buffers.view(self.device, current, size, HDR);
            let o = self.buffers.view(self.device, out, size, HDR);
            self.pass(
                Pass {
                    entry: "fs_dof",
                    a: &a,
                    b: None,
                    out: &o,
                    format: HDR,
                    add: false,
                    clear: true,
                    last: false,
                },
                &base,
                size,
                [0.0; 2],
                depth.as_ref(),
            );
            current = out;
        }

        if env.post.on
            && let Some(glow) = env.post.glow
            && glow.levels & ((1 << GLOW_LEVELS) - 1) != 0
        {
            let out = next(current);
            self.glow(&base, glow.levels, current, out);
            current = out;
        }

        if env.post.on
            && let Some(flare) = &env.post.lens_flare
        {
            let out = next(current);
            self.lens_flare(&base, flare, current, out);
            current = out;
        }

        self.finish(&base, &env.post, current);
    }

    fn glow(&mut self, base: &PostUniform, levels: u32, current: u32, out: u32) {
        let size = self.size;
        let deepest = (32 - (levels & ((1 << GLOW_LEVELS) - 1)).leading_zeros()).max(1);
        let scene = self.buffers.view(self.device, current, size, HDR);
        let mut from = (scene.clone(), size);
        let mut level_views = Vec::new();
        for k in 1..=deepest {
            let s = [(size[0] >> k).max(1), (size[1] >> k).max(1)];
            let level = self.buffers.view(self.device, GLOW + k, s, HDR);
            let blur = self.buffers.view(self.device, GLOW_BLUR + k, s, HDR);
            let entry = if k == 1 { "fs_bright" } else { "fs_down" };
            let simple = |entry, a, out| Pass {
                entry,
                a,
                b: None,
                out,
                format: HDR,
                add: false,
                clear: true,
                last: false,
            };
            self.pass(simple(entry, &from.0, &level), base, from.1, [0.0; 2], None);
            self.pass(simple("fs_blur", &level, &blur), base, s, [1.0, 0.0], None);
            self.pass(simple("fs_blur", &blur, &level), base, s, [0.0, 1.0], None);
            level_views.push((k, level.clone(), s));
            from = (level, s);
        }
        let sum_size = [(size[0] >> 1).max(1), (size[1] >> 1).max(1)];
        let sum = self.buffers.view(self.device, GLOW_SUM, sum_size, HDR);
        let mut first = true;
        for (k, view, s) in &level_views {
            if levels & (1 << (k - 1)) == 0 {
                continue;
            }
            self.pass(
                Pass {
                    entry: "fs_add",
                    a: view,
                    b: None,
                    out: &sum,
                    format: HDR,
                    add: true,
                    clear: first,
                    last: false,
                },
                base,
                *s,
                [0.0; 2],
                None,
            );
            first = false;
        }
        let o = self.buffers.view(self.device, out, size, HDR);
        self.pass(
            Pass {
                entry: "fs_glow",
                a: &scene,
                b: Some(&sum),
                out: &o,
                format: HDR,
                add: false,
                clear: true,
                last: false,
            },
            base,
            size,
            [0.0; 2],
            None,
        );
    }

    /// The surfaces' normals and materials, which the surface pass drew.
    fn surfaces(&mut self) -> [wgpu::TextureView; 2] {
        let size = self.size;
        [
            self.buffers
                .view(self.device, NORMAL_KEY, size, crate::gbuffer::NORMAL),
            self.buffers
                .view(self.device, MATERIAL_KEY, size, crate::gbuffer::MATERIAL),
        ]
    }

    fn illumination(
        &mut self,
        base: &PostUniform,
        gi: Illumination,
        depth: &wgpu::TextureView,
        current: u32,
        out: u32,
    ) {
        let size = self.size;
        let small = [
            ((size[0] as f32 * gi.buffer_size).round() as u32).max(1),
            ((size[1] as f32 * gi.buffer_size).round() as u32).max(1),
        ];
        let [normal, material] = self.surfaces();
        let scene = self.buffers.view(self.device, current, size, HDR);
        let gathered = self.buffers.view(self.device, GATHERED, small, HDR);
        let softened = self.buffers.view(self.device, SOFTENED, small, HDR);
        let first = gi
            .bounce
            .map(|_| self.buffers.view(self.device, SOFTENED_FIRST, small, HDR));
        let simple = |entry, a, b, out| Pass {
            entry,
            a,
            b,
            out,
            format: HDR,
            add: false,
            clear: true,
            last: false,
        };
        // The bounce gathers again from what the first gathering lit, in the
        // same frame: a still scene draws one frame, so a bounce kept from the
        // last one would show only once something moved.
        if let Some(first) = &first {
            let mut u = *base;
            u.gi2[1] = 0.0;
            self.pass_reading(
                simple("fs_gi", &scene, None, &gathered),
                &u,
                size,
                [0.0; 2],
                Some(depth),
                [Some(&normal), Some(&material)],
            );
            self.pass_reading(
                simple("fs_gi_blur", &gathered, None, first),
                &u,
                small,
                [0.0; 2],
                Some(depth),
                [Some(&normal), None],
            );
        }
        self.pass_reading(
            simple("fs_gi", &scene, first.as_ref(), &gathered),
            base,
            size,
            [0.0; 2],
            Some(depth),
            [Some(&normal), Some(&material)],
        );
        self.pass_reading(
            simple("fs_gi_blur", &gathered, None, &softened),
            base,
            small,
            [0.0; 2],
            Some(depth),
            [Some(&normal), None],
        );
        let o = self.buffers.view(self.device, out, size, HDR);
        self.pass_reading(
            simple("fs_gi_apply", &scene, Some(&softened), &o),
            base,
            size,
            [0.0; 2],
            Some(depth),
            [Some(&normal), Some(&material)],
        );
    }

    fn reflections(
        &mut self,
        base: &PostUniform,
        depth: &wgpu::TextureView,
        current: u32,
        out: u32,
    ) {
        let size = self.size;
        let [normal, material] = self.surfaces();
        let scene = self.buffers.view(self.device, current, size, HDR);
        let reflected = self.buffers.view(self.device, REFLECTED, size, HDR);
        let simple = |entry, a, b, out| Pass {
            entry,
            a,
            b,
            out,
            format: HDR,
            add: false,
            clear: true,
            last: false,
        };
        self.pass_reading(
            simple("fs_ssr", &scene, None, &reflected),
            base,
            size,
            [0.0; 2],
            Some(depth),
            [Some(&normal), None],
        );
        let o = self.buffers.view(self.device, out, size, HDR);
        self.pass_reading(
            simple("fs_ssr_apply", &scene, Some(&reflected), &o),
            base,
            size,
            [0.0; 2],
            Some(depth),
            [Some(&normal), Some(&material)],
        );
    }

    fn lens_flare(&mut self, base: &PostUniform, flare: &LensFlare, current: u32, out: u32) {
        let size = self.size;
        let half = [(size[0] >> 1).max(1), (size[1] >> 1).max(1)];
        let (device, queue) = (self.device, self.queue);
        let mut single = |t: Option<&crate::texture::Texture>| {
            t.and_then(|t| self.maps.single(device, queue, t))
                .map(|(view, _)| view)
        };
        let colour = single(flare.color_texture.as_ref());
        let dirt = single(flare.dirt.as_ref().and_then(Option::as_ref));
        let starburst = single(flare.starburst.as_ref().and_then(Option::as_ref));
        let mut u = *base;
        u.flare3[0] = f32::from(u8::from(colour.is_some()));
        // A texture asked for and not had is the built-in one.
        if dirt.is_none() && u.flare3[1] > 1.5 {
            u.flare3[1] = 1.0;
        }
        if starburst.is_none() && u.flare3[2] > 1.5 {
            u.flare3[2] = 1.0;
        }
        let scene = self.buffers.view(device, current, size, HDR);
        let bright = self.buffers.view(device, FLARE_BRIGHT, half, HDR);
        let ghosts = self.buffers.view(device, FLARE, half, HDR);
        let blur = self.buffers.view(device, FLARE_BLUR, half, HDR);
        let simple = |entry, a, b, out| Pass {
            entry,
            a,
            b,
            out,
            format: HDR,
            add: false,
            clear: true,
            last: false,
        };
        self.pass(
            simple("fs_flare_bright", &scene, None, &bright),
            &u,
            size,
            [0.0; 2],
            None,
        );
        self.pass(
            simple("fs_flare_ghosts", &bright, colour.as_ref(), &ghosts),
            &u,
            half,
            [0.0; 2],
            None,
        );
        let spread = flare.blur_amount * 0.5;
        if spread > 0.0 {
            self.pass(
                simple("fs_blur", &ghosts, None, &blur),
                &u,
                half,
                [spread, 0.0],
                None,
            );
            self.pass(
                simple("fs_blur", &blur, None, &ghosts),
                &u,
                half,
                [0.0, spread],
                None,
            );
        }
        let o = self.buffers.view(device, out, size, HDR);
        self.pass_reading(
            simple("fs_flare_apply", &scene, Some(&ghosts), &o),
            &u,
            size,
            [0.0; 2],
            None,
            [dirt.as_ref(), starburst.as_ref()],
        );
    }

    fn finish(&mut self, base: &PostUniform, post: &PostProcess, current: u32) {
        let size = self.size;
        let a = self.buffers.view(self.device, current, size, HDR);
        let lut = post
            .lut
            .as_ref()
            .and_then(|l| l.texture.as_ref())
            .and_then(|t| self.maps.single(self.device, self.queue, t))
            .map(|(view, _)| view);
        let fxaa = post.on && post.fxaa;
        let format = self.output_format;
        let finished = fxaa.then(|| self.buffers.view(self.device, FINISHED, size, format));
        let output = self.output;
        self.pass(
            Pass {
                entry: "fs_final",
                a: &a,
                b: lut.as_ref(),
                out: finished.as_ref().unwrap_or(output),
                format,
                add: false,
                clear: true,
                last: !fxaa,
            },
            base,
            size,
            [0.0; 2],
            None,
        );
        if let Some(finished) = finished {
            self.pass(
                Pass {
                    entry: "fs_fxaa",
                    a: &finished,
                    b: None,
                    out: output,
                    format,
                    add: false,
                    clear: true,
                    last: true,
                },
                base,
                size,
                [0.0; 2],
                None,
            );
        }
    }

    /// Runs `effect`'s passes from `input`; returns where its output is,
    /// unless no pass wrote it.
    fn effect(
        &mut self,
        e: u32,
        effect: &Effect,
        input: u32,
        out: u32,
        depth: Option<&wgpu::TextureView>,
    ) -> Option<u32> {
        let (device, queue) = (self.device, self.queue);
        let size = self.size;
        let buffer_key = |b: usize| EFFECT_BUFFERS + e * 64 + b as u32;
        let mut buffers = Vec::with_capacity(effect.buffers.len());
        for (b, buffer) in effect.buffers.iter().enumerate() {
            let m = if buffer.size_multiplier > 0.0 {
                buffer.size_multiplier
            } else {
                1.0
            };
            let s = [
                ((size[0] as f32 * m).round() as u32).max(1),
                ((size[1] as f32 * m).round() as u32).max(1),
            ];
            buffers.push((
                self.buffers
                    .view(device, buffer_key(b), s, buffer_format(buffer.format)),
                s,
                buffer_format(buffer.format),
            ));
        }
        let input_view = self.buffers.view(device, input, size, HDR);
        let output_view = self.buffers.view(device, out, size, HDR);
        let mut wrote = false;
        for pass in &effect.passes {
            let (in_view, in_size) = match pass.input.and_then(|b| buffers.get(b)) {
                Some((v, s, _)) => (v.clone(), *s),
                None => (input_view.clone(), size),
            };
            let (out_view, out_size, format) = match pass.output.and_then(|b| buffers.get(b)) {
                Some((v, s, f)) => (v.clone(), *s, *f),
                None => {
                    wrote = true;
                    (output_view.clone(), size, HDR)
                }
            };
            let program = &pass.program;
            let mut textures = vec![(in_view, self.programs.sampler().clone())];
            for t in &pass.textures {
                let bound = match t {
                    PassTexture::Buffer(b) => buffers
                        .get(*b)
                        .map(|(v, _, _)| (v.clone(), self.programs.sampler().clone())),
                    PassTexture::Texture(Some(t)) => self.maps.single(device, queue, t),
                    PassTexture::Texture(None) => None,
                };
                textures.push(bound.unwrap_or_else(|| {
                    (
                        self.programs.black(device, queue),
                        self.programs.sampler().clone(),
                    )
                }));
            }
            let header = Header {
                model: IDENTITY,
                normal: IDENTITY,
                view: self.view.view,
                projection: self.view.projection,
                inverse_projection: self.view.inverse_projection,
                view_projection: self.view.view_projection,
                camera: self.view.camera,
                direction: self.view.direction,
                clip: [self.view.near, self.view.far, self.frame, 0.0],
                sizes: [
                    in_size[0] as f32,
                    in_size[1] as f32,
                    out_size[0] as f32,
                    out_size[1] as f32,
                ],
            };
            let bytes = programs::block(program, &header, &pass.values);
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ljos_3d effect"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            // Made first, as the group is made from the program's layout.
            self.programs.effect_pipeline(device, program, format);
            let group = self
                .programs
                .group(device, program, &buffer, &textures, depth, None);
            // The final pass comes after every effect, so none ends the time.
            let timestamp_writes = self.stamp(false);
            let pipeline = self.programs.effect_pipeline(device, program, format);
            let mut rp = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d effect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                timestamp_writes,
                ..Default::default()
            });
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, &group, &[]);
            rp.draw(0..3, 0..1);
        }
        wrote.then_some(out)
    }
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// A Buffer's format on the device. Thirty-two-bit floats are kept in
/// sixteen, as WebGPU filters them only with an optional feature, and
/// normalised sixteen-bit red needs one too.
fn buffer_format(f: BufferFormat) -> wgpu::TextureFormat {
    match f {
        BufferFormat::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
        BufferFormat::Rgba16F | BufferFormat::Rgba32F => wgpu::TextureFormat::Rgba16Float,
        BufferFormat::R8 => wgpu::TextureFormat::R8Unorm,
        BufferFormat::R16 | BufferFormat::R16F | BufferFormat::R32F => {
            wgpu::TextureFormat::R16Float
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starburst_turns_with_the_camera() {
        assert_eq!(starburst_turn([0.0, 0.0, -1.0]), 0.0);
        assert_eq!(starburst_turn([0.0; 3]), 0.0);
        let right = starburst_turn([1.0, 0.0, 0.0]);
        assert!((right - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let up = starburst_turn([0.0, 1.0, 0.0]);
        assert!((up - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }
}
