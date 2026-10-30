//! The meshes of a frame, drawn from one upload.
//!
//! egui_glow uploads every mesh into its buffers before drawing it. A GL
//! driver gives a buffer that is written again while a draw still reads it
//! new storage, so a frame of twenty pictures was forty allocations, and
//! the driver's flush of them the largest cost of a frame that moves a few
//! clock hands. Here the frame's vertices and indices go up once, and each
//! mesh is a draw of its range; consecutive meshes with one picture and one
//! clip are one draw. A mesh of flat colour samples only egui's white texel,
//! so it is drawn without a picture and joins the draw before or after it
//! whatever that one samples: a row's background and its text, one after
//! the other down a list, were a draw each. The shader is otherwise
//! egui's: the same colours, in gamma, without its dithering
//! ([`crate::surface::DITHER`]).
//!
//! Textures stay egui_glow's. egui_glow sets its whole state up before and
//! after each paint callback, which cost a list as much as a draw a row,
//! so the callbacks of Ljós (the clears of [`crate::gl::fill_rects`] and
//! the shader effects of [`crate::gl_shader`]) are run here, and only
//! another one goes through egui_glow.

use egui::epaint::{Primitive, Vertex, WHITE_UV};
use egui::{ClippedPrimitive, Rect};
use glow::HasContext;

const VERTEX: &str = r"
#if NEW_SHADER_INTERFACE
#define I in
#define O out
#else
#define I attribute
#define O varying
#endif
#ifdef GL_ES
#if defined(GL_FRAGMENT_PRECISION_HIGH) && GL_FRAGMENT_PRECISION_HIGH == 1
precision highp float;
#else
precision mediump float;
#endif
#endif
uniform vec2 u_screen_size;
I vec2 a_pos;
I vec2 a_tc;
I vec4 a_srgba;
O vec4 v_rgba;
O vec2 v_tc;
void main() {
    gl_Position = vec4(2.0 * a_pos.x / u_screen_size.x - 1.0,
                       1.0 - 2.0 * a_pos.y / u_screen_size.y, 0.0, 1.0);
    v_rgba = a_srgba / 255.0;
    v_tc = a_tc;
}
";

const FRAGMENT: &str = r"
#ifdef GL_ES
#if defined(GL_FRAGMENT_PRECISION_HIGH) && GL_FRAGMENT_PRECISION_HIGH == 1
precision highp float;
#else
precision mediump float;
#endif
#endif
uniform sampler2D u_sampler;
#if NEW_SHADER_INTERFACE
in vec4 v_rgba;
in vec2 v_tc;
out vec4 f_color;
#define gl_FragColor f_color
#define texture2D texture
#else
varying vec4 v_rgba;
varying vec2 v_tc;
#endif
void main() {
    // A flat colour has no picture to sample (`FLAT`).
    gl_FragColor = v_tc.x < 0.0 ? v_rgba : v_rgba * texture2D(u_sampler, v_tc);
}
";

/// Where the attributes are bound: on the painter's own vertex array where
/// the context has them (a core profile has no default one), else on the
/// default array, which is all a GLES 2 context has.
const POS: u32 = 0;
const TC: u32 = 1;
const SRGBA: u32 = 2;

/// The texture coordinates of a vertex of flat colour, which the shader
/// does not sample at: egui's white texel would give the same colour.
const FLAT: egui::Pos2 = egui::pos2(-1.0, -1.0);

pub(crate) struct Meshes {
    program: glow::Program,
    screen_size: glow::UniformLocation,
    sampler: glow::UniformLocation,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    srgb_framebuffer: bool,
    vertex_array: Option<glow::VertexArray>,
    /// Pictures that are not egui_glow's (a 3D view's), by the ids meshes
    /// sample them by; set before each paint.
    pub(crate) pictures: Vec<(egui::TextureId, glow::Texture)>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<Draw>,
}

enum Draw {
    Mesh {
        scissor: [i32; 4],
        /// None while every mesh of it is of flat colour.
        texture: Option<glow::Texture>,
        /// Into the frame's indices.
        start: usize,
        count: usize,
    },
    Clear {
        scissor: [i32; 4],
        color: [f32; 4],
    },
    Callback(ClippedPrimitive),
}

impl Meshes {
    /// The program and buffers, on `gl` while it is current.
    pub(crate) fn new(gl: &glow::Context) -> Result<Self, String> {
        let version = egui_glow::ShaderVersion::get(gl);
        let head = format!(
            "{}\n#define NEW_SHADER_INTERFACE {}\n",
            version.version_declaration(),
            i32::from(version.is_new_shader_interface())
        );
        let srgb_framebuffer = gl
            .supported_extensions()
            .iter()
            .any(|e| e.ends_with("ARB_framebuffer_sRGB"));
        // SAFETY: the caller made `gl` current.
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
            ] {
                let s = gl.create_shader(kind)?;
                gl.shader_source(s, &format!("{head}{src}"));
                gl.compile_shader(s);
                if !gl.get_shader_compile_status(s) {
                    let log = gl.get_shader_info_log(s);
                    gl.delete_shader(s);
                    gl.delete_program(program);
                    return Err(format!("the mesh shader does not compile: {log}"));
                }
                gl.attach_shader(program, s);
                shaders.push(s);
            }
            gl.bind_attrib_location(program, POS, "a_pos");
            gl.bind_attrib_location(program, TC, "a_tc");
            gl.bind_attrib_location(program, SRGBA, "a_srgba");
            gl.link_program(program);
            for s in shaders {
                gl.detach_shader(program, s);
                gl.delete_shader(s);
            }
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("the mesh shader does not link: {log}"));
            }
            let uniform = |name| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("the mesh shader has no {name}"))
            };
            Ok(Self {
                screen_size: uniform("u_screen_size")?,
                sampler: uniform("u_sampler")?,
                program,
                vbo: gl.create_buffer()?,
                ebo: gl.create_buffer()?,
                srgb_framebuffer,
                vertex_array: if gl.version().major >= 3 {
                    Some(gl.create_vertex_array()?)
                } else {
                    None
                },
                pictures: Vec::new(),
                vertices: Vec::new(),
                indices: Vec::new(),
                draws: Vec::new(),
            })
        }
    }

    /// Draws `primitives` into the `px` framebuffer, whose pictures are
    /// already up.
    pub(crate) fn paint<'a>(
        &mut self,
        painter: &mut egui_glow::Painter,
        px: [u32; 2],
        pixels_per_point: f32,
        primitives: impl IntoIterator<Item = &'a ClippedPrimitive>,
    ) {
        self.gather(painter, px, pixels_per_point, primitives);
        let gl = std::sync::Arc::clone(painter.gl());
        // SAFETY: the caller made the painter's context current.
        unsafe {
            // Binds the buffers, on the painter's vertex array.
            self.prepare(&gl, px, pixels_per_point);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&self.vertices), glow::STREAM_DRAW);
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytes(&self.indices),
                glow::STREAM_DRAW,
            );
            let (mut scissor, mut texture) = (None, None);
            for d in &self.draws {
                match d {
                    Draw::Mesh {
                        scissor: s,
                        texture: t,
                        start,
                        count,
                    } => {
                        if scissor != Some(*s) {
                            gl.scissor(s[0], s[1], s[2], s[3]);
                            scissor = Some(*s);
                        }
                        if let Some(t) = *t
                            && texture != Some(t)
                        {
                            gl.bind_texture(glow::TEXTURE_2D, Some(t));
                            texture = Some(t);
                        }
                        gl.draw_elements(
                            glow::TRIANGLES,
                            *count as i32,
                            glow::UNSIGNED_INT,
                            (*start * size_of::<u32>()) as i32,
                        );
                    }
                    Draw::Clear { scissor: s, color } => {
                        if scissor != Some(*s) {
                            gl.scissor(s[0], s[1], s[2], s[3]);
                            scissor = Some(*s);
                        }
                        gl.clear_color(color[0], color[1], color[2], color[3]);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }
                    Draw::Callback(p) => {
                        self.call(painter, px, pixels_per_point, p);
                        self.prepare(&gl, px, pixels_per_point);
                        (scissor, texture) = (None, None);
                    }
                }
            }
            if self.vertex_array.is_some() {
                gl.bind_vertex_array(None);
            } else {
                for a in [POS, TC, SRGBA] {
                    gl.disable_vertex_attrib_array(a);
                }
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
            gl.disable(glow::SCISSOR_TEST);
        }
        self.draws.clear();
    }

    /// Runs the callback of `p`: one of ours in its scissor and viewport,
    /// any other through egui_glow, which sets its whole state for it.
    fn call(
        &self,
        painter: &mut egui_glow::Painter,
        px: [u32; 2],
        pixels_per_point: f32,
        p: &ClippedPrimitive,
    ) {
        let Primitive::Callback(cb) = &p.primitive else {
            return;
        };
        let Some(shaded) = cb.callback.downcast_ref::<crate::gl_shader::Shaded>() else {
            painter.paint_primitives(px, pixels_per_point, std::slice::from_ref(p));
            return;
        };
        if !cb.rect.is_positive() {
            return;
        }
        let info = egui::PaintCallbackInfo {
            viewport: cb.rect,
            clip_rect: p.clip_rect,
            pixels_per_point,
            screen_size_px: px,
        };
        let gl = painter.gl();
        let s = scissor(px, pixels_per_point, p.clip_rect);
        let v = info.viewport_in_pixels();
        // SAFETY: the caller made the painter's context current.
        unsafe {
            gl.scissor(s[0], s[1], s[2], s[3]);
            gl.viewport(v.left_px, v.from_bottom_px, v.width_px, v.height_px);
        }
        shaded.call(&info, painter);
    }

    /// The frame's vertices, indices and draws.
    fn gather<'a>(
        &mut self,
        painter: &egui_glow::Painter,
        px: [u32; 2],
        pixels_per_point: f32,
        primitives: impl IntoIterator<Item = &'a ClippedPrimitive>,
    ) {
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
        // The painter's map hashes with SipHash; meshes in a row mostly
        // sample one picture (the font atlas), so the last answer is kept.
        let mut last: Option<(egui::TextureId, glow::Texture)> = None;
        for p in primitives {
            let mesh = match &p.primitive {
                Primitive::Mesh(m) => m,
                Primitive::Callback(cb) => {
                    if let Some(crate::gl::Clear(color)) = cb.callback.downcast_ref() {
                        let scissor = scissor(px, pixels_per_point, p.clip_rect.intersect(cb.rect));
                        if scissor[2] > 0 && scissor[3] > 0 {
                            self.draws.push(Draw::Clear {
                                scissor,
                                color: *color,
                            });
                        }
                        continue;
                    }
                    self.draws.push(Draw::Callback(p.clone()));
                    continue;
                }
            };
            let scissor = scissor(px, pixels_per_point, p.clip_rect);
            if mesh.indices.is_empty() || scissor[2] == 0 || scissor[3] == 0 {
                continue;
            }
            let flat = mesh.texture_id == egui::TextureId::default()
                && mesh.vertices.iter().all(|v| v.uv == WHITE_UV);
            let texture = match last {
                _ if flat => None,
                Some((id, t)) if id == mesh.texture_id => Some(t),
                _ => {
                    let own = || {
                        self.pictures
                            .iter()
                            .find(|(id, _)| *id == mesh.texture_id)
                            .map(|(_, t)| *t)
                    };
                    let Some(t) = painter.texture(mesh.texture_id).or_else(own) else {
                        continue;
                    };
                    last = Some((mesh.texture_id, t));
                    Some(t)
                }
            };
            let base = self.vertices.len() as u32;
            let start = self.indices.len();
            if flat {
                self.vertices
                    .extend(mesh.vertices.iter().map(|v| Vertex { uv: FLAT, ..*v }));
            } else {
                self.vertices.extend_from_slice(&mesh.vertices);
            }
            self.indices.extend(mesh.indices.iter().map(|i| i + base));
            match self.draws.last_mut() {
                Some(Draw::Mesh {
                    scissor: s,
                    texture: t,
                    count,
                    ..
                }) if *s == scissor && (t.is_none() || texture.is_none() || *t == texture) => {
                    *t = t.or(texture);
                    *count += mesh.indices.len();
                }
                _ => self.draws.push(Draw::Mesh {
                    scissor,
                    texture,
                    start,
                    count: mesh.indices.len(),
                }),
            }
        }
    }

    /// The state the meshes draw in, which a callback may have changed.
    unsafe fn prepare(&self, gl: &glow::Context, [w, h]: [u32; 2], pixels_per_point: f32) {
        // SAFETY: the caller made `gl` current.
        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            // egui's meshes wind both ways.
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.color_mask(true, true, true, true);
            gl.enable(glow::BLEND);
            gl.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
            // Premultiplied colour; the alpha kept for a see-through window.
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE_MINUS_DST_ALPHA,
                glow::ONE,
            );
            if self.srgb_framebuffer {
                gl.disable(glow::FRAMEBUFFER_SRGB);
            }
            gl.viewport(0, 0, w as i32, h as i32);
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(
                Some(&self.screen_size),
                w as f32 / pixels_per_point,
                h as f32 / pixels_per_point,
            );
            gl.uniform_1_i32(Some(&self.sampler), 0);
            gl.active_texture(glow::TEXTURE0);
            if let Some(v) = self.vertex_array {
                gl.bind_vertex_array(Some(v));
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
            let stride = size_of::<Vertex>() as i32;
            for (a, size, kind, offset) in [
                (POS, 2, glow::FLOAT, std::mem::offset_of!(Vertex, pos)),
                (TC, 2, glow::FLOAT, std::mem::offset_of!(Vertex, uv)),
                (
                    SRGBA,
                    4,
                    glow::UNSIGNED_BYTE,
                    std::mem::offset_of!(Vertex, color),
                ),
            ] {
                gl.enable_vertex_attrib_array(a);
                gl.vertex_attrib_pointer_f32(a, size, kind, false, stride, offset as i32);
            }
        }
    }

    /// Deletes the program and buffers, while `gl` is current.
    pub(crate) fn destroy(&self, gl: &glow::Context) {
        // SAFETY: the caller made `gl` current.
        unsafe {
            gl.delete_program(self.program);
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.ebo);
            if let Some(v) = self.vertex_array {
                gl.delete_vertex_array(v);
            }
        }
    }
}

/// `clip` in the framebuffer's pixels, from its foot, as egui_glow cuts it.
fn scissor([w, h]: [u32; 2], pixels_per_point: f32, clip: Rect) -> [i32; 4] {
    let (w, h) = (w as i32, h as i32);
    let at = |v: f32| (v * pixels_per_point).round() as i32;
    let x0 = at(clip.min.x).clamp(0, w);
    let y0 = at(clip.min.y).clamp(0, h);
    let x1 = at(clip.max.x).clamp(x0, w);
    let y1 = at(clip.max.y).clamp(y0, h);
    [x0, h - y1, x1 - x0, y1 - y0]
}

/// The bytes of vertices or indices, which have no padding.
fn bytes<T: Copy>(v: &[T]) -> &[u8] {
    // SAFETY: `Vertex` is `repr(C)` of floats and bytes with no padding
    // between or after them, and `u32` has none either, so every byte of
    // the slice is initialised.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), size_of_val(v)) }
}
