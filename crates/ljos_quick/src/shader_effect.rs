//! `ShaderEffect`, `GridMesh` and `ShaderEffectSource` (doc.qt.io,
//! `qml-qtquick-shadereffect.html`, `qml-qtquick-gridmesh.html`,
//! `qml-qtquick-shadereffectsource.html`; ARCHITECTURE.md section 7.1, "As
//! built: shader effects").
//!
//! A ShaderEffect's programs come from `ljos_shader`: a `.ljsh` beside the
//! shader is read at once, a GLSL source is compiled on the compute pool
//! and the answer comes back through the platform's inbox, so `status` is
//! `Uncompiled` until it does. What the effect paints is a
//! [`ljos_render::DrawOp::Shader`], which only the GL painter draws; with
//! no shaders at all it draws its `source` as Qt's default shaders do, as
//! a picture, which every renderer draws.
//!
//! The uniforms are read under the scene's tracking, so a ShaderEffect is
//! painted again only when a property its program reads, or a texture,
//! changed: an effect whose inputs stay put costs nothing between frames.
//!
//! A ShaderEffectSource is a picture of its source item, made on the CPU
//! by the scene (`QuickScene::draw_effect_sources`) when what the source
//! shows changed.

use std::sync::Arc;

use crate::off_thread::off_thread;
use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, Rect, Size, Url, Value, qml_impl,
};
use ljos_platform::{Platform, Reply, url_path};
use ljos_reactive::NodeId;
use ljos_render::{Cull, DisplayListBuilder, DrawOp, ImageHandle, ShaderOp, ShaderTexture, kurbo};
use ljos_shader::{Compiled, Located, Program, Stage, Type, write_member};

use crate::enums::EnumTable;
use crate::item::{Item, is_a, write_computed};

const CATEGORY: &str = "qt.quick.shadereffect";

pub mod cull_mode {
    pub const NO_CULLING: i32 = 0;
    pub const BACK_FACE_CULLING: i32 = 1;
    pub const FRONT_FACE_CULLING: i32 = 2;
}

pub mod status {
    pub const COMPILED: i32 = 0;
    pub const UNCOMPILED: i32 = 1;
    pub const ERROR: i32 = 2;
}

fn shader_effect_enums() -> EnumTable {
    vec![
        (
            "CullMode",
            vec![
                ("NoCulling", i64::from(cull_mode::NO_CULLING)),
                ("BackFaceCulling", i64::from(cull_mode::BACK_FACE_CULLING)),
                ("FrontFaceCulling", i64::from(cull_mode::FRONT_FACE_CULLING)),
            ],
        ),
        (
            "Status",
            vec![
                ("Compiled", i64::from(status::COMPILED)),
                ("Uncompiled", i64::from(status::UNCOMPILED)),
                ("Error", i64::from(status::ERROR)),
            ],
        ),
    ]
}

fn source_enums() -> EnumTable {
    vec![
        ("Format", vec![("RGBA8", 1), ("RGBA16F", 2), ("RGBA32F", 3)]),
        (
            "WrapMode",
            vec![
                ("ClampToEdge", 0),
                ("RepeatHorizontally", 1),
                ("RepeatVertically", 2),
                ("Repeat", 3),
            ],
        ),
        (
            "TextureMirroring",
            vec![
                ("NoMirroring", 0),
                ("MirrorHorizontally", 1),
                ("MirrorVertically", 2),
            ],
        ),
    ]
}

/// `ShaderEffect`. Every property declared on it besides these is a
/// uniform of the same name, or a texture.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "ShaderEffect", extends = Item, enums = shader_effect_enums)]
pub struct ShaderEffect {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub fragment_shader: Url,
    #[qml(notify)]
    pub vertex_shader: Url,
    #[qml(notify)]
    pub blending: bool,
    #[qml(notify, enumeration = "CullMode")]
    pub cull_mode: i32,
    /// Ljós has no texture atlas, so every texture is whole whatever this
    /// says.
    #[qml(notify)]
    pub supports_atlas_textures: bool,
    /// A `GridMesh`, or the size of the default one cell.
    #[qml(notify)]
    pub mesh: Value,
    /// One of [`status`].
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(notify, readonly)]
    pub log: Atom,
    /// The program, once both stages are had; `None` with no shaders.
    #[qml(skip)]
    program: Option<Arc<Program>>,
    /// Counts the preparations, so the answer to one that was overtaken
    /// is dropped.
    #[qml(skip)]
    generation: u64,
}

impl Default for ShaderEffect {
    fn default() -> Self {
        Self {
            item: Item::default(),
            fragment_shader: Url::default(),
            vertex_shader: Url::default(),
            blending: true,
            cull_mode: cull_mode::NO_CULLING,
            supports_atlas_textures: false,
            mesh: Value::Size(Arc::new(Size {
                width: 1.0,
                height: 1.0,
            })),
            status: status::UNCOMPILED,
            log: Atom::default(),
            program: None,
            generation: 0,
        }
    }
}

#[qml_impl]
impl ShaderEffect {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        prepare(e, me);
        e.watch(me, ShaderEffect::FRAGMENT_SHADER.index, move |e, _| {
            prepare(e, me);
        })?;
        e.watch(me, ShaderEffect::VERTEX_SHADER.index, move |e, _| {
            prepare(e, me);
        })?;
        Ok(())
    }
}

/// `GridMesh`: cuts a ShaderEffect's rectangle into `resolution` columns
/// and rows, for a vertex shader that bends it.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "GridMesh")]
pub struct GridMesh {
    #[qml(notify)]
    pub resolution: Size,
}

impl Default for GridMesh {
    fn default() -> Self {
        Self {
            resolution: Size {
                width: 1.0,
                height: 1.0,
            },
        }
    }
}

/// `ShaderEffectSource`. `format` and `samples` are kept and do nothing:
/// the picture is 8-bit RGBA, and drawn with the scene's antialiasing.
/// `recursive` does nothing either: a source that holds the
/// ShaderEffectSource is drawn again until its picture stops changing,
/// whether it is set or not, as Qt does on GL.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "ShaderEffectSource", extends = Item, enums = source_enums)]
pub struct ShaderEffectSource {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub source_item: Option<ObjectId>,
    #[qml(notify)]
    pub hide_source: bool,
    #[qml(notify)]
    pub live: bool,
    #[qml(notify)]
    pub recursive: bool,
    #[qml(notify, enumeration = "Format")]
    pub format: i32,
    #[qml(notify)]
    pub mipmap: bool,
    #[qml(notify, enumeration = "WrapMode")]
    pub wrap_mode: i32,
    #[qml(notify, enumeration = "TextureMirroring")]
    pub texture_mirroring: i32,
    #[qml(notify)]
    pub samples: i32,
    #[qml(notify)]
    pub source_rect: Rect,
    #[qml(notify)]
    pub texture_size: Size,
    /// Changed by `scheduleUpdate()`, so a picture that is not live is
    /// taken again.
    #[qml(skip)]
    update_node: Option<NodeId>,
}

impl Default for ShaderEffectSource {
    fn default() -> Self {
        Self {
            item: Item::default(),
            source_item: None,
            hide_source: false,
            live: true,
            recursive: false,
            format: 1,
            mipmap: false,
            wrap_mode: 0,
            texture_mirroring: 2,
            samples: 0,
            source_rect: Rect::default(),
            texture_size: Size::default(),
            update_node: None,
        }
    }
}

#[qml_impl]
impl ShaderEffectSource {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if let Some(scope) = e.object_scope(me) {
            let node = e.graph().property(scope, Value::Int(0))?;
            let _ = e.with_mut::<ShaderEffectSource, _>(me, |s| s.update_node = Some(node));
        }
        Ok(())
    }

    /// `scheduleUpdate()`: take the picture again in the coming frame,
    /// which is what a source that is not `live` shows from then on.
    #[qml(invokable, name = "scheduleUpdate")]
    fn schedule_update(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        if let Ok(Some(node)) = e.with::<ShaderEffectSource, _>(me, |s| s.update_node) {
            let next = match e.graph().get_untracked(node) {
                Value::Int(n) => n.wrapping_add(1),
                _ => 1,
            };
            e.graph().set(node, Value::Int(next));
        }
    }
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        ShaderEffect, GridMesh, ShaderEffectSource,
    ]));
}

// ---------------------------------------------------------------- programs

/// One stage: the program had, or where it is to be compiled from.
enum Want {
    Ready(Arc<Compiled>),
    Compile(Located),
}

/// Finds both shaders of `me` and makes its program: at once when each is
/// built in, cached or a `.ljsh`; on the compute pool when one is GLSL
/// source not compiled before.
fn prepare(e: &Engine, me: ObjectId) {
    let Ok(generation) = e.with_mut::<ShaderEffect, _>(me, |s| {
        s.generation += 1;
        s.generation
    }) else {
        return;
    };
    let url_of = |p| e.untrack(|| e.read(me, p).to_js_string());
    let urls = [
        (Stage::Vertex, url_of(ShaderEffect::VERTEX_SHADER.index)),
        (Stage::Fragment, url_of(ShaderEffect::FRAGMENT_SHADER.index)),
    ];
    if urls.iter().all(|(_, u)| u.is_empty()) {
        // Qt's default shaders: the scene draws `source` as a picture.
        set_program(e, me, None);
        set_status(e, me, status::COMPILED, "");
        return;
    }
    let mut wants = Vec::with_capacity(2);
    for (stage, url) in &urls {
        if url.is_empty() {
            wants.push(Want::Ready(match stage {
                Stage::Vertex => ljos_shader::default_vertex(),
                Stage::Fragment => ljos_shader::default_fragment(),
            }));
            continue;
        }
        let resolved = e.resolve_url(me, url.as_str());
        let located = match url_path(&resolved) {
            // A sandboxed application's `/` is a directory of its own.
            Some(path) => Platform::of(e)
                .env()
                .real(&path)
                .map_err(|err| ljos_shader::Error::Missing(format!("{resolved}: {err}")))
                .and_then(|path| ljos_shader::locate(&path)),
            None => Err(ljos_shader::Error::Missing(resolved.clone())),
        };
        let want = match located {
            Ok(Located::Artifact(p)) => {
                ljos_shader::load(&Located::Artifact(p), *stage).map(Want::Ready)
            }
            Ok(source) => Ok(match ljos_shader::cached(&source, *stage) {
                Some(c) => Want::Ready(c),
                None => Want::Compile(source),
            }),
            Err(err) => Err(err),
        };
        match want {
            Ok(w) => wants.push(w),
            Err(err) => return fail(e, me, &resolved, &err),
        }
    }
    let shown: Vec<String> = urls
        .iter()
        .map(|(_, u)| e.resolve_url(me, u.as_str()))
        .collect();
    if wants.iter().all(|w| matches!(w, Want::Ready(_))) {
        let [v, f] = ready(wants);
        return finish(e, me, v, f, &shown);
    }
    set_program(e, me, None);
    set_status(e, me, status::UNCOMPILED, "");
    let reply: Reply = Platform::of(e).expect_for("ShaderEffect shaders", Some(me));
    off_thread(move || {
        let stages = [Stage::Vertex, Stage::Fragment];
        let done: Vec<Result<Arc<Compiled>, ljos_shader::Error>> = wants
            .into_iter()
            .zip(stages)
            .map(|(w, stage)| match w {
                Want::Ready(c) => Ok(c),
                Want::Compile(at) => ljos_shader::load(&at, stage),
            })
            .collect();
        reply.send(move |e| {
            let current = e
                .with::<ShaderEffect, _>(me, |s| s.generation == generation)
                .unwrap_or(false);
            if !current {
                return;
            }
            let mut had = Vec::with_capacity(2);
            for (r, url) in done.into_iter().zip(&shown) {
                match r {
                    Ok(c) => had.push(Want::Ready(c)),
                    Err(err) => return fail(e, me, url, &err),
                }
            }
            let [v, f] = ready(had);
            finish(e, me, v, f, &shown);
        });
    });
}

fn ready(wants: Vec<Want>) -> [Arc<Compiled>; 2] {
    let mut it = wants.into_iter().map(|w| match w {
        Want::Ready(c) => c,
        Want::Compile(_) => unreachable!("every stage is had"),
    });
    let v = it.next().unwrap_or_else(ljos_shader::default_vertex);
    let f = it.next().unwrap_or_else(ljos_shader::default_fragment);
    [v, f]
}

fn finish(e: &Engine, me: ObjectId, v: Arc<Compiled>, f: Arc<Compiled>, urls: &[String]) {
    match Program::new(v, f) {
        Ok(p) => {
            set_program(e, me, Some(Arc::new(p)));
            set_status(e, me, status::COMPILED, "");
        }
        Err(err) => {
            let url = urls
                .iter()
                .find(|u| !u.is_empty())
                .cloned()
                .unwrap_or_default();
            fail(e, me, &url, &err);
        }
    }
}

/// Says why, in `log` and as Qt warns.
fn fail(e: &Engine, me: ObjectId, url: &str, err: &ljos_shader::Error) {
    let message = err.to_string();
    if let ljos_shader::Error::Missing(_) = err {
        e.log(LogLevel::Warning, CATEGORY, message.clone());
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("ShaderEffect: shader preparation failed for {url}"),
        );
    } else {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("ShaderEffect: shader preparation failed for {url}: {message}"),
        );
    }
    set_program(e, me, None);
    set_status(e, me, status::ERROR, &message);
}

fn set_program(e: &Engine, me: ObjectId, program: Option<Arc<Program>>) {
    let _ = e.with_mut::<ShaderEffect, _>(me, |s| s.program = program);
}

/// The log first: what watches `status` reads it.
fn set_status(e: &Engine, me: ObjectId, status: i32, log: &str) {
    let _ = write_computed(
        e,
        me,
        ShaderEffect::LOG.index,
        Value::String(Atom::from(log)),
    );
    let _ = write_computed(e, me, ShaderEffect::STATUS.index, Value::Int(status));
}

// ---------------------------------------------------------------- paint

/// What a sampler property names.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Texture {
    None,
    Image(ImageHandle, bool),
    /// A ShaderEffectSource, whose picture the scene has.
    Source(ObjectId),
    /// An item whose layer is enabled, and how its picture is taken; the
    /// scene has the picture.
    Layer(ObjectId, Box<SourcePaint>),
}

/// What a ShaderEffect paints, read under tracking.
#[derive(Clone, Debug)]
pub struct ShaderEffectPaint {
    compiled: bool,
    /// `None`: the default shaders, drawn as the `source` picture.
    program: Option<Arc<Program>>,
    uniforms: Vec<u8>,
    textures: Vec<Texture>,
    grid: (u32, u32),
    blending: bool,
    cull: Cull,
}

impl ShaderEffectPaint {
    pub(crate) fn read(e: &Engine, obj: ObjectId) -> Self {
        let st = e.read(obj, ShaderEffect::STATUS.index).to_number() as i32;
        let blending = e.read(obj, ShaderEffect::BLENDING.index).to_boolean();
        let cull = match e.read(obj, ShaderEffect::CULL_MODE.index).to_number() as i32 {
            cull_mode::BACK_FACE_CULLING => Cull::Back,
            cull_mode::FRONT_FACE_CULLING => Cull::Front,
            _ => Cull::None,
        };
        let grid = mesh_resolution(e, &e.read(obj, ShaderEffect::MESH.index));
        let program = e
            .with::<ShaderEffect, _>(obj, |s| s.program.clone())
            .ok()
            .flatten();
        let compiled = st == status::COMPILED;
        let (uniforms, textures) = match (&program, compiled) {
            (Some(p), true) => {
                let mut buf = vec![0u8; p.size as usize];
                let shown = crate::animation::shown::shown(e);
                for m in &p.members {
                    if m.name == "qt_Matrix" || m.name == "qt_Opacity" {
                        continue;
                    }
                    let mut v = e.read_by_name(obj, &m.name);
                    // A UniformAnimator on its way shows its own value.
                    if !shown.is_empty()
                        && let Ok(prop) = e.property_index(obj, &m.name)
                        && let Some(n) = shown.get(obj, prop)
                    {
                        v = Value::Real(n);
                    }
                    write_member(&mut buf, m, &numbers(&v, m.ty));
                }
                let textures = p
                    .texture_names()
                    .into_iter()
                    .map(|n| texture(e, &e.read_by_name(obj, n)))
                    .collect();
                (buf, textures)
            }
            (None, true) => (Vec::new(), vec![texture(e, &e.read_by_name(obj, "source"))]),
            _ => (Vec::new(), Vec::new()),
        };
        Self {
            compiled,
            program,
            uniforms,
            textures,
            grid,
            blending,
            cull,
        }
    }

    /// The ShaderEffectSources it samples.
    pub(crate) fn sources(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.textures.iter().filter_map(|t| match t {
            Texture::Source(s) | Texture::Layer(s, _) => Some(*s),
            _ => None,
        })
    }

    /// The items whose layer it samples, and how each is taken.
    pub(crate) fn layers(&self) -> impl Iterator<Item = (ObjectId, &SourcePaint)> + '_ {
        self.textures.iter().filter_map(|t| match t {
            Texture::Layer(s, how) => Some((*s, &**how)),
            _ => None,
        })
    }

    /// Paints it over `size`; `picture` gives a ShaderEffectSource's
    /// texture.
    pub(crate) fn paint(
        &self,
        size: kurbo::Size,
        picture: impl Fn(ObjectId) -> Option<ShaderTexture>,
        out: &mut DisplayListBuilder,
    ) {
        if !self.compiled || size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let textures: Vec<ShaderTexture> = self
            .textures
            .iter()
            .map(|t| match t {
                Texture::None => ShaderTexture::new(None, true),
                Texture::Image(i, smooth) => ShaderTexture::new(Some(i.clone()), *smooth),
                Texture::Source(s) | Texture::Layer(s, _) => {
                    picture(*s).unwrap_or_else(|| ShaderTexture::new(None, true))
                }
            })
            .collect();
        let rect = kurbo::Rect::from_origin_size((0.0, 0.0), size);
        let Some(program) = &self.program else {
            // `source` times the opacity, which the tree applies.
            if let Some(ShaderTexture {
                image: Some(image),
                smooth,
                ..
            }) = textures.into_iter().next()
            {
                out.push(DrawOp::Image {
                    src: image.rect(),
                    image,
                    dst: rect,
                    tile: None,
                    mirror: [false; 2],
                    smooth,
                    opacity: 1.0,
                    tint: None,
                });
            }
            return;
        };
        out.shader(
            rect,
            Arc::new(ShaderOp {
                program: Arc::clone(program),
                uniforms: self.uniforms.clone(),
                textures,
                grid: self.grid,
                blending: self.blending,
                cull: self.cull,
            }),
        );
    }
}

fn mesh_resolution(e: &Engine, mesh: &Value) -> (u32, u32) {
    let size = match mesh {
        Value::Object(m) if is_a::<GridMesh>(e, *m) => {
            match e.read(*m, GridMesh::RESOLUTION.index) {
                Value::Size(s) => (s.width, s.height),
                _ => (1.0, 1.0),
            }
        }
        Value::Size(s) => (s.width, s.height),
        _ => (1.0, 1.0),
    };
    let cells = |n: f64| {
        if n.is_finite() {
            n.clamp(1.0, 4096.0) as u32
        } else {
            1
        }
    };
    (cells(size.0), cells(size.1))
}

/// A QML value as the numbers of a uniform of type `ty`. A colour is
/// premultiplied, as Qt gives it.
fn numbers(v: &Value, ty: Type) -> Vec<f64> {
    let f = f64::from;
    let g = |x: f32| f64::from(x);
    match v {
        Value::Color(c) => {
            let a = f(c.a) / 65535.0;
            vec![
                f(c.r) / 65535.0 * a,
                f(c.g) / 65535.0 * a,
                f(c.b) / 65535.0 * a,
                a,
            ]
        }
        Value::Point(p) => vec![p.x, p.y],
        Value::IntPoint(p) => vec![f64::from(p.x), f64::from(p.y)],
        Value::Size(s) => vec![s.width, s.height],
        Value::Rect(r) => vec![r.x, r.y, r.width, r.height],
        Value::Vector2d(p) => vec![g(p.x), g(p.y)],
        Value::Vector3d(p) => vec![g(p.x), g(p.y), g(p.z)],
        Value::Vector4d(p) => vec![g(p.x), g(p.y), g(p.z), g(p.w)],
        Value::Quaternion(q) => vec![g(q.x), g(q.y), g(q.z), g(q.scalar)],
        Value::List(items) => items.iter().map(Value::to_number).collect(),
        _ => match ty {
            Type::Int | Type::Uint | Type::IVec(_) => vec![v.to_number().trunc()],
            _ => vec![v.to_number()],
        },
    }
}

/// What a sampler property gives: an Image's or Canvas's pixels, or a
/// ShaderEffectSource's picture. Other items give none, as in Qt, where
/// only they are texture providers.
fn texture(e: &Engine, v: &Value) -> Texture {
    let Value::Object(o) = v else {
        return Texture::None;
    };
    if is_a::<ShaderEffectSource>(e, *o) {
        return Texture::Source(*o);
    }
    if crate::item::is_item(e, *o) && e.read_by_name(*o, "layer.enabled").to_boolean() {
        return Texture::Layer(*o, Box::new(SourcePaint::read_layer(e, *o)));
    }
    let smooth = e.read(*o, Item::SMOOTH.index).to_boolean();
    if is_a::<crate::image::ImageBase>(e, *o) || is_a::<crate::canvas::Canvas>(e, *o) {
        return match crate::effects::read_item_image(e, *o) {
            Some(i) => Texture::Image(i, smooth),
            None => Texture::None,
        };
    }
    Texture::None
}

/// What a ShaderEffectSource shows, or how an item's layer is taken, read
/// under tracking; the picture is the scene's to take.
#[derive(Clone, Debug, PartialEq)]
pub struct SourcePaint {
    pub(crate) source: Option<ObjectId>,
    pub(crate) hide: bool,
    pub(crate) live: bool,
    /// How many times `scheduleUpdate()` was called.
    pub(crate) update: i32,
    rect: Option<Rect>,
    texture_size: Option<(u32, u32)>,
    smooth: bool,
    mipmap: bool,
    /// `wrapMode`, across and down.
    wrap: [bool; 2],
    /// `textureMirroring`, as what a sampler is given is flipped: left to
    /// right, and top to bottom.
    flip: [bool; 2],
    /// The picture as the ShaderEffectSource draws it.
    pub(crate) image: Option<ImageHandle>,
    /// The same, as a ShaderEffect samples it.
    sampled: Option<ImageHandle>,
}

impl SourcePaint {
    pub(crate) fn read(e: &Engine, obj: ObjectId) -> Self {
        let source = e
            .read(obj, ShaderEffectSource::SOURCE_ITEM.index)
            .as_object()
            .filter(|s| crate::item::is_item(e, *s));
        let rect = match e.read(obj, ShaderEffectSource::SOURCE_RECT.index) {
            Value::Rect(r) if r.width > 0.0 && r.height > 0.0 => Some(*r),
            _ => None,
        };
        let texture_size = match e.read(obj, ShaderEffectSource::TEXTURE_SIZE.index) {
            Value::Size(s) if s.width >= 1.0 && s.height >= 1.0 => {
                Some((s.width.round() as u32, s.height.round() as u32))
            }
            _ => None,
        };
        let update = e
            .with::<ShaderEffectSource, _>(obj, |s| s.update_node)
            .ok()
            .flatten()
            .map_or(0, |n| match e.graph().get(n) {
                Value::Int(n) => n,
                _ => 0,
            });
        let wrap = e.read(obj, ShaderEffectSource::WRAP_MODE.index).to_number() as i32;
        // Qt's default, MirrorVertically, is what reads the right way up
        // through `qt_TexCoord0`; it is the bits that are not set that
        // flip, as qml6 on GL shows.
        let mirror = e
            .read(obj, ShaderEffectSource::TEXTURE_MIRRORING.index)
            .to_number() as i32;
        Self {
            source,
            hide: e
                .read(obj, ShaderEffectSource::HIDE_SOURCE.index)
                .to_boolean(),
            live: e.read(obj, ShaderEffectSource::LIVE.index).to_boolean(),
            update,
            rect,
            texture_size,
            smooth: e.read(obj, Item::SMOOTH.index).to_boolean(),
            mipmap: e.read(obj, ShaderEffectSource::MIPMAP.index).to_boolean(),
            wrap: [wrap & 1 != 0, wrap & 2 != 0],
            flip: [mirror & 1 != 0, mirror & 2 == 0],
            image: None,
            sampled: None,
        }
    }

    /// How the layer of `item` is taken: as a ShaderEffectSource of it
    /// with the layer's settings would take it.
    pub(crate) fn read_layer(e: &Engine, item: ObjectId) -> Self {
        let read = |name: &str| e.read_by_name(item, &format!("layer.{name}"));
        let rect = match read("sourceRect") {
            Value::Rect(r) if r.width > 0.0 && r.height > 0.0 => Some(*r),
            _ => None,
        };
        let texture_size = match read("textureSize") {
            Value::Size(s) if s.width >= 1.0 && s.height >= 1.0 => {
                Some((s.width.round() as u32, s.height.round() as u32))
            }
            _ => None,
        };
        let wrap = read("wrapMode").to_number() as i32;
        let mirror = read("textureMirroring").to_number() as i32;
        Self {
            source: Some(item),
            hide: false,
            live: read("live").to_boolean(),
            update: 0,
            rect,
            texture_size,
            smooth: read("smooth").to_boolean(),
            mipmap: read("mipmap").to_boolean(),
            wrap: [wrap & 1 != 0, wrap & 2 != 0],
            flip: [mirror & 1 != 0, mirror & 2 == 0],
            image: None,
            sampled: None,
        }
    }

    /// The same, showing `picture`, the source at `scale` pixels per
    /// logical pixel: cut to `sourceRect` and resampled to `textureSize`,
    /// which is in pixels.
    pub(crate) fn with_picture(&self, picture: Option<ImageHandle>, scale: f32) -> Self {
        let scale = f64::from(scale);
        let image = picture.and_then(|p| {
            let p = match self.rect {
                Some(r) => {
                    let at = |v: f64| (v * scale).max(0.0).round() as u32;
                    p.cropped(at(r.x), at(r.y), at(r.width), at(r.height))?
                }
                None => p,
            };
            match self.texture_size {
                Some((w, h)) if (w, h) != (p.width(), p.height()) => {
                    if self.smooth {
                        p.resized(w, h)
                    } else {
                        nearest(&p, w, h)
                    }
                }
                _ => Some(p),
            }
        });
        let sampled = image
            .as_ref()
            .map(|i| i.flipped(self.flip[0], self.flip[1]));
        Self {
            image,
            sampled,
            ..self.clone()
        }
    }

    /// What a ShaderEffect that names it samples.
    pub(crate) fn texture(&self) -> ShaderTexture {
        ShaderTexture {
            image: self.sampled.clone(),
            smooth: self.smooth,
            mipmap: self.mipmap,
            wrap: self.wrap,
        }
    }

    /// Paints it over `size`, at `scale` device pixels per logical pixel.
    pub(crate) fn paint(&self, size: kurbo::Size, scale: f32, out: &mut DisplayListBuilder) {
        let Some(image) = &self.image else { return };
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        // A mipmapped picture drawn smaller than it is averages what each
        // pixel covers, as sampling from the mipmaps does.
        let fit = |side: f64| (side * f64::from(scale)).ceil().max(1.0) as u32;
        let (w, h) = (fit(size.width), fit(size.height));
        let image = match (self.mipmap, image.width() > w || image.height() > h) {
            (true, true) => image
                .resized(w.min(image.width()), h.min(image.height()))
                .unwrap_or_else(|| image.clone()),
            _ => image.clone(),
        };
        out.push(DrawOp::Image {
            src: image.rect(),
            image,
            dst: kurbo::Rect::from_origin_size((0.0, 0.0), size),
            tile: None,
            mirror: [false; 2],
            smooth: self.smooth,
            opacity: 1.0,
            tint: None,
        });
    }
}

/// `p` at `w` by `h`, each pixel the source's at its centre.
fn nearest(p: &ImageHandle, w: u32, h: u32) -> Option<ImageHandle> {
    let (sw, sh) = (p.width(), p.height());
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return None;
    }
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let sy = ((f64::from(y) + 0.5) * f64::from(sh) / f64::from(h)) as u32;
        for x in 0..w {
            let sx = ((f64::from(x) + 0.5) * f64::from(sw) / f64::from(w)) as u32;
            data.extend_from_slice(&p.pixel(sx.min(sw - 1), sy.min(sh - 1)));
        }
    }
    ImageHandle::from_premultiplied(w, h, data).ok()
}
