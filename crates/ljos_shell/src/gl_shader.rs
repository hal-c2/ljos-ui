//! A `ShaderEffect`'s program, run by the GL painter (ARCHITECTURE.md
//! section 7.1, "As built: shader effects").
//!
//! B0 hands the effect over as a paint callback holding a [`ShaderPaint`];
//! [`programs`] turns each into the [`Shaded`] callback that draws it. The
//! programs are linked the first time one is drawn and kept, with their
//! grid, while the op that holds them lives; the uniform buffer is written
//! at each draw, since `qt_Matrix` depends on the viewport egui gives it.
//!
//! The mesh painter sets its own state again after a callback, but not
//! the sampler objects, the uniform buffer binding or culling, which this
//! leaves as it found them.
//!
//! An effect drawn a second time with nothing changed (the same
//! [`ShaderPaint`], at the same place) is drawn once more into a picture
//! of its own, which later frames copy: a program that is costly per pixel
//! (a background drawn under everything) is then run once, not again for
//! each frame something over it changes. One that changes each frame is
//! never kept.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use egui::epaint::{ClippedShape, PaintCallbackInfo, Shape};
use glow::HasContext;
use ljos_render::b0::ShaderPaint;
use ljos_render::kurbo::Rect;
use ljos_render::ljos_shader::{Compiled, Program};
use ljos_render::{Cull, Viewport};

/// The programs one GL painter linked, shared by the callbacks it draws.
#[derive(Clone, Default)]
pub(crate) struct Programs(Arc<Mutex<Cache>>);

#[derive(Default)]
struct Cache {
    /// By the address of the program's `Arc`, which the entry holds so the
    /// address is not given to another.
    linked: HashMap<usize, Linked>,
    /// Bound for a sampler that has no picture: transparent black.
    empty: Option<glow::Texture>,
    /// By how they sample: [`Sampling`].
    samplers: HashMap<Sampling, glow::Sampler>,
    ubo: Option<glow::Buffer>,
    /// Programs that failed to link, said once each.
    failed: HashMap<usize, Arc<Program>>,
    /// By the address of the paint's `Arc`, which the entry holds.
    kept: HashMap<usize, Kept>,
    /// Draws a kept picture: its program and the vertex array it needs.
    copy: Option<(glow::Program, glow::VertexArray)>,
}

/// An effect seen drawn, and once drawn again unchanged, its picture.
struct Kept {
    paint: Arc<ShaderPaint>,
    /// Left, bottom, width and height in the window's pixels.
    viewport: [i32; 4],
    picture: Option<(glow::Framebuffer, glow::Texture)>,
}

/// Smooth, from mipmaps, and repeating across and down.
type Sampling = (bool, bool, [bool; 2]);

struct Linked {
    program: Arc<Program>,
    gl: glow::Program,
    /// The sampler uniforms by texture unit, as
    /// [`Program::texture_names`] orders them.
    units: Vec<Vec<glow::UniformLocation>>,
    grid: Option<Grid>,
}

struct Grid {
    rect: Rect,
    cells: (u32, u32),
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    count: i32,
}

impl Programs {
    /// Deletes everything linked, while the painter's context is current.
    pub(crate) fn destroy(&self, gl: &glow::Context) {
        let mut cache = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let cache = std::mem::take(&mut *cache);
        // SAFETY: the caller made the context current.
        unsafe {
            for l in cache.linked.into_values() {
                if let Some(g) = &l.grid {
                    delete_grid(gl, g);
                }
                gl.delete_program(l.gl);
            }
            for k in cache.kept.into_values() {
                delete_picture(gl, &k);
            }
            if let Some((p, vao)) = cache.copy {
                gl.delete_program(p);
                gl.delete_vertex_array(vao);
            }
            if let Some(t) = cache.empty {
                gl.delete_texture(t);
            }
            for s in cache.samplers.into_values() {
                gl.delete_sampler(s);
            }
            if let Some(b) = cache.ubo {
                gl.delete_buffer(b);
            }
        }
    }
}

/// A callback the mesh painter runs itself, in the scissor of its clip and
/// the viewport of its rectangle. egui_glow's own sets up the whole of its
/// state before and again after one, on top of the painter's.
pub(crate) struct Shaded {
    programs: Programs,
    paint: Arc<ShaderPaint>,
}

impl Shaded {
    pub(crate) fn call(&self, info: &PaintCallbackInfo, painter: &egui_glow::Painter) {
        let mut cache = self
            .programs
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        draw(&mut cache, &self.paint, info, painter);
    }
}

/// Turns every shader callback of `shapes` into one the GL painter runs.
pub(crate) fn programs(programs: &Programs, shapes: &mut [ClippedShape]) {
    for shape in shapes {
        let Shape::Callback(cb) = &mut shape.shape else {
            continue;
        };
        let Ok(paint) = Arc::clone(&cb.callback).downcast::<ShaderPaint>() else {
            continue;
        };
        cb.callback = Arc::new(Shaded {
            programs: programs.clone(),
            paint,
        });
    }
}

fn draw(
    cache: &mut Cache,
    paint: &Arc<ShaderPaint>,
    info: &PaintCallbackInfo,
    painter: &egui_glow::Painter,
) {
    let gl = painter.gl();
    let key = Arc::as_ptr(&paint.op.program) as usize;
    forget_dead(cache, gl);
    if cache.failed.contains_key(&key) {
        return;
    }
    if let std::collections::hash_map::Entry::Vacant(slot) = cache.linked.entry(key) {
        // SAFETY: the painter calls this while its context is current.
        match unsafe { link(gl, &paint.op.program) } {
            Ok(linked) => {
                slot.insert(linked);
            }
            Err(log) => {
                eprintln!("ShaderEffect: the program does not link: {log}");
                cache.failed.insert(key, Arc::clone(&paint.op.program));
                return;
            }
        }
    }
    let v = info.viewport_in_pixels();
    let top = info.screen_size_px[1] as i32 - v.from_bottom_px - v.height_px;
    let on_screen = Viewport {
        left: v.left_px as f32,
        top: top as f32,
        width: v.width_px as f32,
        height: v.height_px as f32,
    };
    let viewport = [v.left_px, v.from_bottom_px, v.width_px, v.height_px];
    let es = gl.version().is_embedded;
    let at = Arc::as_ptr(paint) as usize;
    if !keeps(paint, viewport, info.screen_size_px) {
        // SAFETY: as above.
        unsafe { run(cache, painter, key, paint, on_screen) };
        return;
    }
    let picture = match cache.kept.get(&at) {
        Some(k) if k.viewport == viewport => k.picture,
        _ => {
            if let Some(old) = cache.kept.insert(
                at,
                Kept {
                    paint: Arc::clone(paint),
                    viewport,
                    picture: None,
                },
            ) {
                // SAFETY: as above.
                unsafe { delete_picture(gl, &old) };
            }
            // SAFETY: as above.
            unsafe { run(cache, painter, key, paint, on_screen) };
            return;
        }
    };
    // SAFETY: as above; the framebuffer, viewport and scissor test the
    // painter set are set back before the copy.
    unsafe {
        let texture = match picture {
            Some((_, t)) => t,
            None => {
                let (w, h) = (viewport[2], viewport[3]);
                let shown = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
                let (fb, t) = picture_of(gl, w, h);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fb));
                gl.viewport(0, 0, w, h);
                gl.disable(glow::SCISSOR_TEST);
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                run(cache, painter, key, paint, on_screen);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, shown);
                gl.viewport(viewport[0], viewport[1], w, h);
                gl.enable(glow::SCISSOR_TEST);
                if let Some(k) = cache.kept.get_mut(&at) {
                    k.picture = Some((fb, t));
                }
                t
            }
        };
        let (program, vao) = *cache.copy.get_or_insert_with(|| copier(gl, es));
        gl.use_program(Some(program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.bind_sampler(0, None);
        // The picture holds the effect over transparent black, blended
        // as the effect itself is, so it is put down as the effect would be.
        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.disable(glow::CULL_FACE);
        gl.bind_vertex_array(Some(vao));
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.bind_vertex_array(None);
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
}

/// Whether an effect is worth a picture of its own and drawn the same
/// into it. One that is not blended writes even where it is transparent,
/// which a copy blended over what is there would not. A picture is at
/// most the window's size; and one placed away from the window's corner
/// is not kept if its program reads where on the window it is.
fn keeps(paint: &ShaderPaint, viewport: [i32; 4], screen: [u32; 2]) -> bool {
    let [left, bottom, w, h] = viewport;
    let fits = w > 0 && h > 0 && (w as i64 * h as i64) <= screen[0] as i64 * screen[1] as i64;
    let placed = (left == 0 && bottom == 0) || !reads_where(&paint.op.program);
    paint.op.blending && fits && placed
}

fn reads_where(program: &Program) -> bool {
    [&program.vertex, &program.fragment]
        .iter()
        .any(|c| c.gl330.contains("gl_FragCoord") || c.es300.contains("gl_FragCoord"))
}

/// Runs the effect's program into the viewport the painter set.
///
/// # Safety
/// The painter's context is current.
unsafe fn run(
    cache: &mut Cache,
    painter: &egui_glow::Painter,
    key: usize,
    paint: &ShaderPaint,
    viewport: Viewport,
) {
    // SAFETY: the caller's context is current; everything bound here is
    // bound back to none.
    unsafe {
        let gl = painter.gl();
        let empty = *cache.empty.get_or_insert_with(|| empty_texture(gl));
        let ubo = *cache
            .ubo
            .get_or_insert_with(|| gl.create_buffer().expect("a GL buffer"));
        let Some(linked) = cache.linked.get_mut(&key) else {
            return;
        };
        let op = &paint.op;
        let grid = match &linked.grid {
            Some(g) if g.rect == paint.rect && g.cells == op.grid => g,
            _ => {
                if let Some(old) = linked.grid.take() {
                    delete_grid(gl, &old);
                }
                &*linked.grid.insert(grid(gl, paint))
            }
        };

        let uniforms = paint.uniforms(viewport);

        gl.use_program(Some(linked.gl));
        if !uniforms.is_empty() {
            gl.bind_buffer(glow::UNIFORM_BUFFER, Some(ubo));
            gl.buffer_data_u8_slice(glow::UNIFORM_BUFFER, &uniforms, glow::STREAM_DRAW);
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(ubo));
        }
        for (unit, locations) in linked.units.iter().enumerate() {
            let texture = op.textures.get(unit);
            let id = paint.textures.get(unit).copied().flatten();
            let tex = id.and_then(|id| painter.texture(id)).unwrap_or(empty);
            gl.active_texture(glow::TEXTURE0 + unit as u32);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            // The empty texture has no mipmaps to sample.
            let how = match texture {
                Some(t) if id.is_some() => (t.smooth, t.mipmap, t.wrap),
                _ => (true, false, [false; 2]),
            };
            let s = *cache
                .samplers
                .entry(how)
                .or_insert_with(|| sampler(gl, how));
            gl.bind_sampler(unit as u32, Some(s));
            for at in locations {
                gl.uniform_1_i32(Some(at), unit as i32);
            }
        }
        if op.blending {
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        } else {
            gl.disable(glow::BLEND);
        }
        match op.cull {
            Cull::None => gl.disable(glow::CULL_FACE),
            Cull::Back | Cull::Front => {
                gl.enable(glow::CULL_FACE);
                gl.front_face(glow::CCW);
                gl.cull_face(if op.cull == Cull::Back {
                    glow::BACK
                } else {
                    glow::FRONT
                });
            }
        }
        gl.bind_vertex_array(Some(grid.vao));
        gl.draw_elements(glow::TRIANGLES, grid.count, glow::UNSIGNED_INT, 0);
        gl.bind_vertex_array(None);

        gl.disable(glow::CULL_FACE);
        for unit in 0..linked.units.len() {
            gl.bind_sampler(unit as u32, None);
            gl.active_texture(glow::TEXTURE0 + unit as u32);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        gl.active_texture(glow::TEXTURE0);
        gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, None);
        gl.bind_buffer(glow::UNIFORM_BUFFER, None);
    }
}

/// Deletes what was linked for programs nothing draws any more.
/// Pictures go first: they hold the programs they were drawn with.
fn forget_dead(cache: &mut Cache, gl: &glow::Context) {
    cache.kept.retain(|_, k| {
        let live = Arc::strong_count(&k.paint) > 1;
        if !live {
            // SAFETY: the context is current (see `draw`).
            unsafe { delete_picture(gl, k) };
        }
        live
    });
    cache.failed.retain(|_, p| Arc::strong_count(p) > 1);
    cache.linked.retain(|_, l| {
        let live = Arc::strong_count(&l.program) > 1;
        if !live {
            // SAFETY: the context is current (see `draw`).
            unsafe {
                if let Some(g) = l.grid.take() {
                    delete_grid(gl, &g);
                }
                gl.delete_program(l.gl);
            }
        }
        live
    });
}

unsafe fn link(gl: &glow::Context, program: &Arc<Program>) -> Result<Linked, String> {
    let es = gl.version().is_embedded;
    let text = |c: &Compiled| {
        if es { c.es300.clone() } else { c.gl330.clone() }
    };
    // SAFETY: the caller's context is current.
    unsafe {
        let p = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, source) in [
            (glow::VERTEX_SHADER, text(&program.vertex)),
            (glow::FRAGMENT_SHADER, text(&program.fragment)),
        ] {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, &source);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                let log = gl.get_shader_info_log(s);
                gl.delete_shader(s);
                for s in shaders {
                    gl.delete_shader(s);
                }
                gl.delete_program(p);
                return Err(log);
            }
            gl.attach_shader(p, s);
            shaders.push(s);
        }
        gl.bind_attrib_location(p, 0, "_p2vs_location0");
        gl.bind_attrib_location(p, 1, "_p2vs_location1");
        gl.link_program(p);
        for s in shaders {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        if !gl.get_program_link_status(p) {
            let log = gl.get_program_info_log(p);
            gl.delete_program(p);
            return Err(log);
        }
        for block in [&program.vertex.block, &program.fragment.block]
            .into_iter()
            .flatten()
        {
            if let Some(i) = gl.get_uniform_block_index(p, &block.gl_name) {
                gl.uniform_block_binding(p, i, 0);
            }
        }
        let units = program
            .texture_names()
            .into_iter()
            .map(|name| {
                program
                    .samplers()
                    .filter(|s| s.name == name)
                    .filter_map(|s| gl.get_uniform_location(p, &s.gl_name))
                    .collect()
            })
            .collect();
        Ok(Linked {
            program: Arc::clone(program),
            gl: p,
            units,
            grid: None,
        })
    }
}

unsafe fn grid(gl: &glow::Context, paint: &ShaderPaint) -> Grid {
    let (vertices, indices) = paint.op.grid_for(paint.rect);
    let vb: Vec<u8> = vertices.iter().flat_map(|f| f.to_ne_bytes()).collect();
    let ib: Vec<u8> = indices.iter().flat_map(|i| i.to_ne_bytes()).collect();
    // SAFETY: the caller's context is current.
    unsafe {
        let vao = gl.create_vertex_array().expect("a GL vertex array");
        let vbo = gl.create_buffer().expect("a GL buffer");
        let ebo = gl.create_buffer().expect("a GL buffer");
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &vb, glow::STATIC_DRAW);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &ib, glow::STATIC_DRAW);
        let stride = 6 * 4;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 4 * 4);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Grid {
            rect: paint.rect,
            cells: paint.op.grid,
            vao,
            vbo,
            ebo,
            count: indices.len() as i32,
        }
    }
}

/// A framebuffer drawing into a texture of `w` by `h` pixels.
unsafe fn picture_of(gl: &glow::Context, w: i32, h: i32) -> (glow::Framebuffer, glow::Texture) {
    // SAFETY: the caller's context is current.
    unsafe {
        let t = gl.create_texture().expect("a GL texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        for (what, how) in [
            (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
            (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, what, how as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        let fb = gl.create_framebuffer().expect("a GL framebuffer");
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fb));
        gl.framebuffer_texture_2d(
            glow::DRAW_FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(t),
            0,
        );
        (fb, t)
    }
}

unsafe fn delete_picture(gl: &glow::Context, k: &Kept) {
    if let Some((fb, t)) = k.picture {
        // SAFETY: the caller's context is current.
        unsafe {
            gl.delete_framebuffer(fb);
            gl.delete_texture(t);
        }
    }
}

/// The program that puts a kept picture over the viewport, its corners
/// made from the vertex's index.
unsafe fn copier(gl: &glow::Context, es: bool) -> (glow::Program, glow::VertexArray) {
    let head = if es {
        "#version 300 es\nprecision mediump float;\n"
    } else {
        "#version 330 core\n"
    };
    let vertex = format!(
        "{head}out vec2 uv;\nvoid main() {{\n  vec2 p = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));\n  uv = p;\n  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n}}\n"
    );
    let fragment = format!(
        "{head}in vec2 uv;\nuniform sampler2D picture;\nout vec4 color;\nvoid main() {{ color = texture(picture, uv); }}\n"
    );
    // SAFETY: the caller's context is current.
    unsafe {
        let p = gl.create_program().expect("a GL program");
        let mut shaders = Vec::new();
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let s = gl.create_shader(kind).expect("a GL shader");
            gl.shader_source(s, &source);
            gl.compile_shader(s);
            debug_assert!(
                gl.get_shader_compile_status(s),
                "{}",
                gl.get_shader_info_log(s)
            );
            gl.attach_shader(p, s);
            shaders.push(s);
        }
        gl.link_program(p);
        for s in shaders {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        debug_assert!(
            gl.get_program_link_status(p),
            "{}",
            gl.get_program_info_log(p)
        );
        gl.use_program(Some(p));
        gl.uniform_1_i32(gl.get_uniform_location(p, "picture").as_ref(), 0);
        let vao = gl.create_vertex_array().expect("a GL vertex array");
        (p, vao)
    }
}

unsafe fn delete_grid(gl: &glow::Context, g: &Grid) {
    // SAFETY: the caller's context is current.
    unsafe {
        gl.delete_vertex_array(g.vao);
        gl.delete_buffer(g.vbo);
        gl.delete_buffer(g.ebo);
    }
}

unsafe fn empty_texture(gl: &glow::Context) -> glow::Texture {
    // SAFETY: the caller's context is current.
    unsafe {
        let t = gl.create_texture().expect("a GL texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&[0, 0, 0, 0])),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        t
    }
}

/// Sampler objects, so the filter and the wrapping a ShaderEffectSource
/// asks for do not change the texture egui draws the same picture with
/// elsewhere.
unsafe fn sampler(gl: &glow::Context, (smooth, mipmap, wrap): Sampling) -> glow::Sampler {
    let mag = if smooth { glow::LINEAR } else { glow::NEAREST };
    let min = match (smooth, mipmap) {
        (true, true) => glow::LINEAR_MIPMAP_LINEAR,
        (false, true) => glow::NEAREST_MIPMAP_NEAREST,
        _ => mag,
    };
    let edge = |repeat: bool| {
        if repeat {
            glow::REPEAT
        } else {
            glow::CLAMP_TO_EDGE
        }
    };
    // SAFETY: the caller's context is current.
    unsafe {
        let s = gl.create_sampler().expect("a GL sampler");
        gl.sampler_parameter_i32(s, glow::TEXTURE_MIN_FILTER, min as i32);
        gl.sampler_parameter_i32(s, glow::TEXTURE_MAG_FILTER, mag as i32);
        gl.sampler_parameter_i32(s, glow::TEXTURE_WRAP_S, edge(wrap[0]) as i32);
        gl.sampler_parameter_i32(s, glow::TEXTURE_WRAP_T, edge(wrap[1]) as i32);
        s
    }
}

#[cfg(test)]
mod tests {
    use ljos_render::kurbo::Affine;
    use ljos_render::ljos_shader::{default_fragment, default_vertex};
    use ljos_render::{ShaderOp, ShaderTexture};

    use super::*;

    fn paint(blending: bool, fragment: Compiled) -> ShaderPaint {
        let program = Program::new(default_vertex(), Arc::new(fragment)).expect("a program");
        let op = ShaderOp {
            program: Arc::new(program),
            uniforms: Vec::new(),
            textures: Vec::<ShaderTexture>::new(),
            grid: (1, 1),
            blending,
            cull: Cull::None,
        };
        ShaderPaint {
            op: Arc::new(op),
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            to_screen: Affine::IDENTITY,
            opacity: 1.0,
            ppp: 1.0,
            textures: Vec::new(),
        }
    }

    const SCREEN: [u32; 2] = [200, 100];

    #[test]
    fn a_blended_effect_on_the_window_is_kept() {
        let p = paint(true, (*default_fragment()).clone());
        assert!(keeps(&p, [0, 0, 200, 100], SCREEN));
        assert!(keeps(&p, [50, 20, 100, 50], SCREEN));
    }

    #[test]
    fn an_effect_that_replaces_what_is_below_is_drawn_each_time() {
        let p = paint(false, (*default_fragment()).clone());
        assert!(!keeps(&p, [0, 0, 200, 100], SCREEN));
    }

    #[test]
    fn a_picture_larger_than_the_window_is_not_kept() {
        let p = paint(true, (*default_fragment()).clone());
        assert!(!keeps(&p, [-100, 0, 400, 100], SCREEN));
        assert!(!keeps(&p, [0, 0, 0, 100], SCREEN));
    }

    #[test]
    fn a_program_that_reads_where_it_is_is_kept_only_at_the_corner() {
        let mut f = (*default_fragment()).clone();
        f.gl330.push_str("\n// gl_FragCoord\n");
        f.es300.push_str("\n// gl_FragCoord\n");
        let p = paint(true, f);
        assert!(keeps(&p, [0, 0, 100, 50], SCREEN));
        assert!(!keeps(&p, [10, 0, 100, 50], SCREEN));
    }
}
