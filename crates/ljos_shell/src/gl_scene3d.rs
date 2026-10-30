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
    /// Kept for its report of what the device holds (the tests count the
    /// live textures by it).
    #[cfg_attr(not(test), allow(dead_code))]
    instance: wgpu::Instance,
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
        // No backend of its own: the adapter is the context's, and wgpu's
        // GL backend would start an EGL display of its own for nothing
        // (4.7 ms of the first frame on radeonsi).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // SAFETY: the instance has no backend whose adapters this one could
        // be confused with; wgpu-core only wraps it.
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
            instance,
            device,
            queue,
            renderer,
            vertex_array,
        })
    }

    /// Deletes what wgpu still holds of pictures let go of. A texture a
    /// submission drew into lives until wgpu sees that submission done,
    /// which it looks at only when it submits or is polled; the painter's
    /// 2D frames do neither, so without this the last views' textures
    /// would stay until a view is drawn again. Never waits: a submission
    /// not yet done is seen at the next one.
    pub(crate) fn release(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
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

    /// Whether drawing `primitives` needs the device: they show a view, or
    /// there are pictures that views gone may have to let go of.
    pub(crate) fn needs_device(&self, primitives: &[ClippedPrimitive]) -> bool {
        !self.pictures.is_empty() || primitives.iter().any(|p| view_of(p).is_some())
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
        let mut released = false;
        for k in gone.into_iter().flatten() {
            released |= self.pictures.remove(&k).is_some();
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
        if released {
            device.release();
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

    /// Lets go of every picture, while the context is current; whether
    /// there was one.
    pub(crate) fn clear(&mut self) -> bool {
        let had = !self.pictures.is_empty();
        self.pictures.clear();
        had
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use egui::epaint::{ClippedShape, PaintCallback, Primitive, Shape};
    use egui::{ClippedPrimitive, Rect, pos2, vec2};
    use glow::HasContext;
    use glutin::api::egl::context::PossiblyCurrentContext;
    use glutin::api::egl::device::Device;
    use glutin::api::egl::display::Display;
    use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
    use glutin::context::{ContextApi, ContextAttributesBuilder};
    use glutin::display::GlDisplay;
    use ljos_3d::math::Vec3;
    use ljos_3d::{
        Camera, Light, Material, MeshSource, Model, Node, NodeKind, Primitive as Mesh3d, Scene,
        Transform,
    };
    use ljos_render::scene3d::View3dPaint;

    use super::{Device3d, Views};

    /// A context with no surface on an EGL device, current on this thread,
    /// as the painter's is before its first window; Mesa has one for each
    /// GPU and one for llvmpipe.
    struct Headless {
        // Field order is drop order: the context goes before its display.
        glow: glow::Context,
        _context: PossiblyCurrentContext,
        display: Display,
        name: String,
    }

    fn headless(device: &Device) -> Result<Headless, String> {
        // SAFETY: no DRM display is handed over; the device is EGL's own.
        let display = unsafe { Display::with_device(device, None) }.map_err(|e| e.to_string())?;
        let template = ConfigTemplateBuilder::new()
            .with_surface_type(ConfigSurfaceTypes::empty())
            .build();
        // SAFETY: the template names no window.
        let config = unsafe { display.find_configs(template) }
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("no config")?;
        let attributes = |api| {
            ContextAttributesBuilder::new()
                .with_context_api(api)
                .build(None)
        };
        // SAFETY: the config is the display's.
        let context = unsafe {
            display
                .create_context(&config, &attributes(ContextApi::OpenGl(None)))
                .or_else(|_| display.create_context(&config, &attributes(ContextApi::Gles(None))))
        }
        .map_err(|e| e.to_string())?
        .make_current_surfaceless()
        .map_err(|e| e.to_string())?;
        // SAFETY: the context is current and its functions are the display's.
        let glow =
            unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
        // SAFETY: a plain query on the current context.
        let name = unsafe { glow.get_parameter_string(glow::RENDERER) };
        Ok(Headless {
            glow,
            _context: context,
            display,
            name,
        })
    }

    fn cube() -> Arc<Scene> {
        let mut scene = Scene::new();
        scene.add(
            None,
            Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
                position: Vec3::new(0.0, 0.0, 300.0),
                ..Transform::default()
            }),
        );
        scene.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(Mesh3d::Cube),
                material: Material::default(),
            })),
        );
        Arc::new(scene)
    }

    fn live_textures(device: &Device3d) -> usize {
        device
            .instance
            .generate_report()
            .map_or(0, |r| r.hub_report().textures.num_kept_from_user)
    }

    /// One frame of the painter showing `views` (key, size), as
    /// `GlPainter::paint` runs it: the shapes noted, then the views drawn.
    /// The pictures kept after it, by their GL names.
    fn frame(
        h: &Headless,
        device: &mut Device3d,
        views: &mut Views,
        scene: &Arc<Scene>,
        shown: &[(u64, [u32; 2])],
    ) -> Vec<glow::Texture> {
        let callback = |key, size_px: [u32; 2]| PaintCallback {
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(size_px[0] as f32, size_px[1] as f32)),
            callback: Arc::new(View3dPaint {
                key,
                scene: Arc::clone(scene),
                size_px,
            }),
        };
        let shapes: Vec<_> = shown
            .iter()
            .map(|&(k, size)| ClippedShape {
                clip_rect: Rect::EVERYTHING,
                shape: Shape::Callback(callback(k, size)),
                transform_after_tessellation: egui::emath::TSTransform::IDENTITY,
            })
            .collect();
        let primitives: Vec<_> = shown
            .iter()
            .map(|&(k, size)| ClippedPrimitive {
                clip_rect: Rect::EVERYTHING,
                primitive: Primitive::Callback(callback(k, size)),
            })
            .collect();
        views.shown(&shapes);
        // SAFETY: the context is current on this thread for the test.
        unsafe {
            views.draw(device, &h.glow, [320, 240], &primitives);
            // The frame is on the screen before the next is drawn, as
            // the painter's swap and the display's interval make it.
            h.glow.finish();
        }
        let mut pictures = Vec::new();
        views.pictures_into(&mut pictures);
        pictures.into_iter().map(|(_, t)| t).collect()
    }

    /// A frame with no view and no picture to let go of does not open
    /// wgpu: a program without a `View3D` never pays for the device.
    #[test]
    fn only_a_frame_with_a_view_needs_the_device() {
        let flat = ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(egui::Mesh::default()),
        };
        let view = ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Callback(PaintCallback {
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
                callback: Arc::new(View3dPaint {
                    key: 1,
                    scene: cube(),
                    size_px: [10, 10],
                }),
            }),
        };
        let views = Views::default();
        assert!(!views.needs_device(&[]));
        assert!(!views.needs_device(std::slice::from_ref(&flat)));
        assert!(views.needs_device(&[flat, view]));
    }

    /// Views that come and go on the GL painter, as a `Loader` toggling
    /// one makes them: each picture's GL texture is deleted once its view
    /// is no longer shown, on the context, and wgpu holds as many textures
    /// after any number of rounds as after the first. Runs on every EGL
    /// device the machine has (a GPU, llvmpipe), with no window.
    #[test]
    fn views_that_come_and_go_leave_no_gl_textures_behind() {
        let devices: Vec<_> = match Device::query_devices() {
            Ok(d) => d.collect(),
            Err(e) => {
                eprintln!("SKIPPED: EGL lists no devices: {e}");
                return;
            }
        };
        let mut ran = 0;
        for d in &devices {
            let h = match headless(d) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("an EGL device ({:?}) has no context: {e}", d.name());
                    continue;
                }
            };
            // SAFETY: the context is current on this thread, and stays so
            // while the device lives; it goes before the context.
            let made = unsafe { Device3d::new(&h.glow, |s| h.display.get_proc_address(s)) };
            let mut device = match made {
                Ok(device) => device,
                Err(e) => {
                    eprintln!("wgpu cannot use {}: {e}", h.name);
                    continue;
                }
            };
            run_rounds(&h, &mut device);
            drop(device);
            eprintln!("views came and went on {}", h.name);
            ran += 1;
        }
        if ran == 0 {
            eprintln!("SKIPPED: no EGL device wgpu can draw on");
        }
    }

    fn run_rounds(h: &Headless, device: &mut Device3d) {
        let on = &h.name;
        let scene = cube();
        let mut views = Views::default();
        // The renderer keeps what it shares between views once it first
        // draws; the level to come back to is after one view came and went.
        assert_eq!(
            frame(h, device, &mut views, &scene, &[(u64::MAX, [64, 64])]).len(),
            1
        );
        assert!(frame(h, device, &mut views, &scene, &[]).is_empty());
        let before = live_textures(device);
        let mut with_two = None;
        let mut released = HashSet::new();
        for round in 0..50u64 {
            // New keys each round, and a size of their own, as views laid
            // out again have.
            let size = [100 + (round % 7) as u32, 80];
            let shown = [(2 * round + 1, size), (2 * round + 2, size)];
            let pictures = frame(h, device, &mut views, &scene, &shown);
            assert_eq!(pictures.len(), 2, "round {round} on {on}: both views drawn");
            for &t in &pictures {
                // SAFETY: the context is current.
                assert!(
                    unsafe { h.glow.is_texture(t) },
                    "round {round} on {on}: a shown picture is a texture"
                );
            }
            let textures = live_textures(device);
            assert_eq!(
                *with_two.get_or_insert(textures),
                textures,
                "round {round} on {on}: two views hold as many textures as in the first round"
            );
            assert!(
                frame(h, device, &mut views, &scene, &[]).is_empty(),
                "round {round} on {on}: the views are gone"
            );
            assert!(
                !views.needs_device(&[]),
                "round {round} on {on}: with the views gone, frames leave wgpu be"
            );
            assert_eq!(
                live_textures(device),
                before,
                "round {round} on {on}: wgpu holds no texture of theirs"
            );
            released.extend(pictures);
        }
        // SAFETY: the context is current.
        let alive: Vec<_> = released
            .iter()
            .filter(|&&t| unsafe { h.glow.is_texture(t) })
            .collect();
        assert!(
            alive.is_empty(),
            "on {on}: {} of the {} GL textures of views gone are still textures",
            alive.len(),
            released.len()
        );
        let _ = views.clear();
    }
}
