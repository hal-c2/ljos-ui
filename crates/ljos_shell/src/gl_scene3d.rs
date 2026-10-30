//! 3D views on the GL painter: wgpu's GL backend on the painter's own
//! context (ARCHITECTURE.md §7.8).
//!
//! GL stays the painter where EGL is (the painter bench in
//! docs/quick3d.md: egui-wgpu cost an animating window up to three times
//! the processor time), so a window that shows a `View3D` does not open a
//! second device for it. wgpu is handed the context the painter draws
//! with: one driver, one set of pictures, nothing read back. Each frame,
//! before egui's meshes, every view whose scene changed is drawn into a
//! picture of its own, and its paint callback becomes a mesh that samples
//! that picture, so it is clipped, blended and drawn in part as any mesh.
//!
//! The two share GL's state. wgpu sets what a pass needs at its start but
//! keeps some state from when its device was opened (its vertex array,
//! the pixel store) and leaves the rest as its last command did; the
//! painter sets its own before its meshes but not, for example, the
//! sampler objects or the buffer pixels are unpacked from. So wgpu gets its
//! state back before it works ([`Device3d::enter`]) and the painter after
//! ([`Device3d::leave`]).

use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_void};
use std::num::NonZeroU32;

use egui::epaint::{ClippedShape, Primitive, Shape};
use egui::{ClippedPrimitive, Color32, Mesh, Rect, TextureId, pos2};
use glow::HasContext;
use ljos_3d::{Renderer, Target};
use ljos_render::scene3d::{View3dPaint, ViewKey};

/// The texture units whose sampler objects are unbound after wgpu works:
/// as many as GL promises a fragment shader at least.
const UNITS: u32 = 16;

/// A wgpu device on the GL context, made while it is current.
pub(crate) struct Device3d {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    /// The vertex array wgpu bound when its device was opened, which it
    /// draws with from then on.
    vertex_array: glow::VertexArray,
}

impl Device3d {
    /// A device on the context `gl` is current with, whose functions
    /// `lookup` finds.
    ///
    /// # Safety
    ///
    /// The context must be current now, and whenever the device or
    /// anything made with it is used or dropped.
    pub(crate) unsafe fn new(
        gl: &glow::Context,
        mut lookup: impl FnMut(&std::ffi::CStr) -> *const c_void,
    ) -> Result<Device3d, String> {
        let find = |name: &str| {
            CString::new(name).map_or(std::ptr::null(), |name| lookup(name.as_c_str()))
        };
        // SAFETY: the caller made the context current, and keeps it so for
        // everything the adapter makes.
        let exposed = unsafe {
            wgpu::hal::gles::Adapter::new_external(find, wgpu::GlBackendOptions::default())
        }
        .ok_or("wgpu cannot use the GL context (it needs GL 3.3 or GLES 3)")?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // SAFETY: the adapter is of the GL backend the instance has.
        let adapter = unsafe { instance.create_adapter_from_hal::<wgpu::hal::api::Gles>(exposed) };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ljos_shell 3D on GL"),
            required_limits:
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        // SAFETY: the context is current; a plain query.
        let bound = unsafe { gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) };
        let vertex_array = NonZeroU32::new(bound as u32)
            .map(glow::NativeVertexArray)
            .ok_or("wgpu opened its device without a vertex array")?;
        let mut renderer = Renderer::new(&device);
        // WebGL2's limits ask for no float targets; the context may still
        // have them, as a light probe's maps need.
        renderer.use_adapter_formats(&adapter);
        Ok(Device3d {
            device,
            queue,
            renderer,
            vertex_array,
        })
    }

    /// GL's state as wgpu left it when it opened its device.
    unsafe fn enter(&self, gl: &glow::Context) {
        // SAFETY: the caller made `gl` current.
        unsafe {
            gl.bind_vertex_array(Some(self.vertex_array));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        }
    }

    /// GL's state as the painter expects it, the window's framebuffer
    /// bound.
    unsafe fn leave(&self, gl: &glow::Context, [w, h]: [u32; 2]) {
        // SAFETY: the caller made `gl` current.
        unsafe {
            // Off wgpu's array, so that nothing the painter binds lands
            // in it.
            gl.bind_vertex_array(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            for unit in 0..UNITS {
                gl.bind_sampler(unit, None);
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            gl.use_program(None);
            for cap in [
                glow::DEPTH_TEST,
                glow::STENCIL_TEST,
                glow::SCISSOR_TEST,
                glow::BLEND,
                glow::CULL_FACE,
            ] {
                gl.disable(cap);
            }
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.viewport(0, 0, w as i32, h as i32);
        }
    }
}

/// One window's views: a picture each, kept while the view is shown.
#[derive(Default)]
pub(crate) struct Views {
    pictures: HashMap<ViewKey, Picture>,
    /// The views the last frame showed, drawn in part or not.
    shown: Option<HashSet<ViewKey>>,
    /// Hands out the ids pictures are sampled by; egui's own are counted
    /// up from zero, these down from the top.
    ids: u64,
}

struct Picture {
    target: Target,
    texture: glow::Texture,
    id: TextureId,
}

impl Views {
    /// Notes the views `shapes` (the whole frame, before it is cut to the
    /// areas it draws) show: the pictures of the others go at the next
    /// paint.
    pub(crate) fn shown(&mut self, shapes: &[ClippedShape]) {
        let shown = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Callback(cb) => cb.callback.downcast_ref::<View3dPaint>().map(|v| v.key),
                _ => None,
            })
            .collect();
        self.shown = Some(shown);
    }

    /// The pictures meshes sample, by the ids they sample them by.
    pub(crate) fn pictures_into(&self, out: &mut Vec<(TextureId, glow::Texture)>) {
        out.clear();
        out.extend(self.pictures.values().map(|p| (p.id, p.texture)));
    }

    /// Draws the views among `primitives` whose scene changed into their
    /// pictures, and gives back `primitives` with each view a mesh that
    /// samples its picture; `None` when there is no view.
    ///
    /// # Safety
    ///
    /// `gl` and `device`'s context must be current.
    pub(crate) unsafe fn draw(
        &mut self,
        device: &mut Device3d,
        gl: &glow::Context,
        px: [u32; 2],
        primitives: &[ClippedPrimitive],
    ) -> Option<Vec<ClippedPrimitive>> {
        let gone = self.shown.take().map(|shown| {
            let gone: Vec<_> = self
                .pictures
                .keys()
                .filter(|k| !shown.contains(k))
                .copied()
                .collect();
            gone
        });
        let has_views = primitives.iter().any(|p| view_of(p).is_some());
        if !has_views && gone.as_ref().is_none_or(Vec::is_empty) {
            return None;
        }
        // SAFETY: the caller made the context current.
        unsafe { device.enter(gl) };
        for k in gone.into_iter().flatten() {
            self.pictures.remove(&k);
        }
        let mut encoder = device
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ljos_shell 3D views"),
            });
        let mut drawn = false;
        for p in primitives {
            let Some(view) = view_of(p) else {
                continue;
            };
            let picture = match self.pictures.get_mut(&view.key) {
                Some(picture) if picture.target.size() == view.size_px => picture,
                _ => {
                    let target = Target::new(&device.device, &device.renderer, view.size_px);
                    let Some(texture) = texture_of(&target) else {
                        continue;
                    };
                    self.ids += 1;
                    let id = TextureId::User(u64::MAX - self.ids);
                    self.pictures.entry(view.key).insert_entry(Picture {
                        target,
                        texture,
                        id,
                    });
                    self.pictures.get_mut(&view.key).expect("just put in")
                }
            };
            drawn |= device.renderer.render(
                &device.device,
                &device.queue,
                &mut encoder,
                &view.scene,
                &mut picture.target,
            );
        }
        if drawn {
            device.queue.submit([encoder.finish()]);
        }
        // SAFETY: as above.
        unsafe { device.leave(gl, px) };
        let out = primitives
            .iter()
            .map(|p| match (view_of(p), &p.primitive) {
                (Some(view), Primitive::Callback(cb)) => {
                    let mut mesh = Mesh::with_texture(
                        self.pictures
                            .get(&view.key)
                            .map_or(TextureId::default(), |p| p.id),
                    );
                    if self.pictures.contains_key(&view.key) {
                        mesh.add_rect_with_uv(
                            cb.rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ClippedPrimitive {
                        clip_rect: p.clip_rect,
                        primitive: Primitive::Mesh(mesh),
                    }
                }
                _ => p.clone(),
            })
            .collect();
        Some(out)
    }

    /// Lets go of every picture, while the context is current.
    pub(crate) fn clear(&mut self) {
        self.pictures.clear();
    }
}

fn view_of(p: &ClippedPrimitive) -> Option<&View3dPaint> {
    match &p.primitive {
        Primitive::Callback(cb) => cb.callback.downcast_ref::<View3dPaint>(),
        Primitive::Mesh(_) => None,
    }
}

/// The GL texture behind `target`'s picture. wgpu made it with nearest
/// filtering, which a picture the size of its view on the screen samples
/// texel for pixel; the meshes bind no sampler object, so that is how
/// they sample it.
fn texture_of(target: &Target) -> Option<glow::Texture> {
    // SAFETY: the texture is only read for its name here, on the device's
    // own backend.
    let raw = unsafe { target.texture().as_hal::<wgpu::hal::api::Gles>() }?;
    match raw.inner {
        wgpu::hal::gles::TextureInner::Texture { raw, .. } => Some(raw),
        _ => None,
    }
}
