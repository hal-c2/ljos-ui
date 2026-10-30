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

use crate::math::Mat4;
use crate::mesh::Mesh;
use crate::scene::{Background, Light, Lighting, Material, MeshSource, NodeKind, Primitive, Scene};

/// What the picture is kept in: sRGB values written by the shader, so the
/// same bytes blend in a 2D pass that works in gamma space.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// The depth format every backend has, WebGL2 included.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelUniform {
    world: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    diffuse: [f32; 4],
    /// x is 1 where the material is lit; the rest unused.
    params: [f32; 4],
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

/// The pipelines and meshes of one device. One per device, shared by every
/// view drawn on it.
pub struct Renderer {
    lit: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    model_layout: wgpu::BindGroupLayout,
    model_stride: u64,
    meshes: HashMap<Primitive, GpuMesh>,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    blits: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("meshes", &self.meshes.len())
            .finish_non_exhaustive()
    }
}

/// Where one view's picture is drawn and kept.
pub struct Target {
    size: [u32; 2],
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
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
    /// The scene revision the picture shows; `None` until drawn.
    drawn: Option<u64>,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("size", &self.size)
            .field("drawn", &self.drawn)
            .finish_non_exhaustive()
    }
}

impl Target {
    /// A picture `size` device pixels large; a side of 0 is taken as 1.
    pub fn new(device: &wgpu::Device, renderer: &Renderer, size: [u32; 2]) -> Self {
        let size = [size[0].max(1), size[1].max(1)];
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ljos_3d picture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ljos_3d depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
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
            blit_group,
            frame_buffer,
            frame_group,
            model_buffer,
            model_group,
            model_capacity,
            drawn: None,
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

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let frame_layout = uniform_layout(device, "ljos_3d frame", false);
        let model_layout = uniform_layout(device, "ljos_3d model", true);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ljos_3d lit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/lit.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ljos_3d lit"),
            bind_group_layouts: &[Some(&frame_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let lit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ljos_3d lit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
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
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
            lit,
            frame_layout,
            model_layout,
            model_stride,
            meshes: HashMap::new(),
            blit_layout,
            blit_sampler,
            blits: HashMap::new(),
        }
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

        let (view_proj, has_camera) = match scene.active_camera() {
            Some((camera, world)) => {
                let view = world.inverse().unwrap_or(Mat4::IDENTITY);
                (camera.projection(w as f32, h as f32) * view, true)
            }
            None => (Mat4::IDENTITY, false),
        };
        let mut frame = FrameUniform {
            view_proj: view_proj.0,
            light_dir: [0.0, 0.0, -1.0, 0.0],
            light_color: [0.0; 4],
            ambient: [0.0; 4],
        };
        // One light for now: the first directional light in the tree.
        if let Some((world, light)) = walked.iter().find_map(|(_, node, world)| match &node.kind {
            NodeKind::Light(l) => Some((*world, *l)),
            _ => None,
        }) {
            let Light::Directional {
                color,
                ambient_color,
                brightness,
            } = light;
            let d = world.forward();
            let c = color.to_linear();
            let a = ambient_color.to_linear();
            frame.light_dir = [d.x, d.y, d.z, 0.0];
            frame.light_color = [c[0] * brightness, c[1] * brightness, c[2] * brightness, 1.0];
            frame.ambient = [a[0], a[1], a[2], 0.0];
        }
        queue.write_buffer(&target.frame_buffer, 0, bytemuck::bytes_of(&frame));

        let mut draws = Vec::new();
        if has_camera {
            for (_, node, world) in &walked {
                let NodeKind::Model(model) = &node.kind else {
                    continue;
                };
                let MeshSource::Primitive(p) = model.mesh;
                let Material::Default(m) = model.material;
                draws.push((
                    p,
                    ModelUniform {
                        world: world.0,
                        normal: world.normal_matrix().0,
                        diffuse: m.diffuse_color.to_linear(),
                        params: [
                            f32::from(u8::from(m.lighting == Lighting::Fragment)),
                            0.0,
                            0.0,
                            0.0,
                        ],
                    },
                ));
            }
        }
        if draws.len() as u64 > target.model_capacity {
            target.model_capacity = (draws.len() as u64).next_power_of_two();
            (target.model_buffer, target.model_group) = model_slots(
                device,
                &self.model_layout,
                target.model_capacity,
                self.model_stride,
            );
        }
        for (i, (p, uniform)) in draws.iter().enumerate() {
            queue.write_buffer(
                &target.model_buffer,
                i as u64 * self.model_stride,
                bytemuck::bytes_of(uniform),
            );
            self.mesh(device, *p);
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.lit);
            pass.set_bind_group(0, &target.frame_group, &[]);
            for (i, (p, _)) in draws.iter().enumerate() {
                let mesh = &self.meshes[p];
                let offset = (i as u64 * self.model_stride) as u32;
                pass.set_bind_group(1, &target.model_group, &[offset]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
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
