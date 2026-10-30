//! `CustomMaterial`, `Effect` and the objects they are made of: shader
//! snippets an author writes, read into the programs `ljos_3d` draws.
//!
//! An object's own properties (those its QML declares) are the snippets'
//! uniforms, in the order they are declared; a `TextureInput` property is
//! a sampler. The snippets are read and translated when a view first draws
//! with them and again only when a file name or that list changes, so a
//! property animating its value costs a block of numbers, not a compile.

use std::cell::RefCell;
use std::sync::Arc;

use ljos::item;
use ljos::item::ljos_3d::{
    self, Blend, BufferFormat, CullMode, DepthDraw, EffectBuffer, Filter, PassTexture, Program,
    Tiling, UniformKind, UniformValue, snippet,
};
use ljos::ljos_engine::TypeTag;
use ljos::prelude::*;

use crate::node::{Material, cull};
use crate::scene::{color, list};
use crate::{Object3D, texture};

pub(crate) mod shading {
    pub const UNSHADED: i32 = 0;
    pub const SHADED: i32 = 1;
}

fn custom_material_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ShadingMode",
            vec![
                ("Unshaded", i64::from(shading::UNSHADED)),
                ("Shaded", i64::from(shading::SHADED)),
            ],
        ),
        (
            "BlendMode",
            [
                "NoBlend",
                "Zero",
                "One",
                "SrcColor",
                "OneMinusSrcColor",
                "DstColor",
                "OneMinusDstColor",
                "SrcAlpha",
                "OneMinusSrcAlpha",
                "DstAlpha",
                "OneMinusDstAlpha",
                "ConstantColor",
                "OneMinusConstantColor",
                "ConstantAlpha",
                "OneMinusConstantAlpha",
                "SrcAlphaSaturate",
            ]
            .into_iter()
            .zip(0..)
            .collect(),
        ),
    ]
}

/// `CustomMaterial`: a model drawn with the author's snippets.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "CustomMaterial",
    extends = Material,
    enums = custom_material_enums
)]
pub struct CustomMaterial {
    #[qml(base)]
    pub material: Material,
    #[qml(notify, enumeration = "ShadingMode")]
    pub shading_mode: i32,
    #[qml(notify)]
    pub vertex_shader: Url,
    #[qml(notify)]
    pub fragment_shader: Url,
    #[qml(notify, enumeration = "BlendMode")]
    pub source_blend: i32,
    #[qml(notify, enumeration = "BlendMode")]
    pub destination_blend: i32,
    /// Kept: alpha blends with the colour's factors.
    #[qml(notify, enumeration = "BlendMode")]
    pub source_alpha_blend: i32,
    /// Kept: alpha blends with the colour's factors.
    #[qml(notify, enumeration = "BlendMode")]
    pub destination_alpha_blend: i32,
    /// Kept: a view draws again when a value it read changes, and a
    /// snippet's time comes from a property an animation drives.
    #[qml(notify)]
    pub always_dirty: bool,
    /// Kept: lines are not drawn.
    #[qml(notify)]
    pub line_width: f64,
    #[qml(skip)]
    program: Cache,
}

impl Default for CustomMaterial {
    fn default() -> Self {
        CustomMaterial {
            material: Material::default(),
            shading_mode: shading::SHADED,
            vertex_shader: Url::default(),
            fragment_shader: Url::default(),
            source_blend: 0,
            destination_blend: 0,
            source_alpha_blend: 0,
            destination_alpha_blend: 0,
            always_dirty: false,
            line_width: 1.0,
            program: Cache::default(),
        }
    }
}

/// `TextureInput`: a texture a snippet samples by the property's name.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "TextureInput", extends = Object3D)]
pub struct TextureInput {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify, object = "Texture")]
    pub texture: Option<ObjectId>,
    /// Off, the sampler reads opaque black.
    #[qml(notify)]
    pub enabled: bool,
}

impl Default for TextureInput {
    fn default() -> Self {
        TextureInput {
            object3d: Object3D::default(),
            texture: None,
            enabled: true,
        }
    }
}

/// `Effect`: passes run over the picture once the scene is drawn.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Effect", extends = Object3D)]
pub struct Effect {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify, list)]
    pub passes: QmlList,
}

/// `Pass`: one run of a fragment snippet over the whole output.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Pass", extends = Object3D)]
pub struct Pass {
    #[qml(base)]
    pub object3d: Object3D,
    /// `BufferInput`s and `SetUniformValue`s.
    #[qml(notify, list)]
    pub commands: QmlList,
    /// Where it writes; without one, the effect's output.
    #[qml(notify, object = "Buffer")]
    pub output: Option<ObjectId>,
    #[qml(notify, list)]
    pub shaders: QmlList,
    #[qml(skip)]
    program: Cache,
}

pub(crate) mod stage {
    pub const VERTEX: i32 = 0;
    pub const FRAGMENT: i32 = 1;
}

fn shader_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Stage",
        vec![
            ("Vertex", i64::from(stage::VERTEX)),
            ("Fragment", i64::from(stage::FRAGMENT)),
        ],
    )]
}

/// `Shader`: a pass's snippet for one stage.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Shader", extends = Object3D, enums = shader_enums)]
pub struct Shader {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub shader: Url,
    #[qml(notify, enumeration = "Stage")]
    pub stage: i32,
}

impl Default for Shader {
    fn default() -> Self {
        Shader {
            object3d: Object3D::default(),
            shader: Url::default(),
            stage: stage::FRAGMENT,
        }
    }
}

fn buffer_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "TextureFormat",
            ["RGBA8", "RGBA16F", "RGBA32F", "R8", "R16", "R16F", "R32F"]
                .into_iter()
                .zip(0..)
                .collect(),
        ),
        (
            "TextureFilterOperation",
            vec![("Nearest", 0), ("Linear", 1)],
        ),
        (
            "TextureCoordOperation",
            vec![("ClampToEdge", 0), ("Repeat", 1), ("MirroredRepeat", 2)],
        ),
        (
            "AllocateBufferFlagValues",
            vec![("None", 0), ("SceneLifetime", 1)],
        ),
    ]
}

/// `Buffer`: a picture an effect's passes write and read.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D", name = "Buffer", extends = Object3D, enums = buffer_enums)]
pub struct Buffer {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub name: String,
    /// RGBA32F is kept as RGBA16F, and R16 and R32F as R16F: the
    /// formats every target can draw into and filter.
    #[qml(notify, enumeration = "TextureFormat")]
    pub format: i32,
    #[qml(notify)]
    pub size_multiplier: f64,
    #[qml(notify, enumeration = "TextureFilterOperation")]
    pub texture_filter_operation: i32,
    #[qml(notify, enumeration = "TextureCoordOperation")]
    pub texture_coord_operation: i32,
    /// Kept: a buffer starts each frame empty.
    #[qml(notify, enumeration = "AllocateBufferFlagValues")]
    pub buffer_flags: i32,
}

impl Default for Buffer {
    fn default() -> Self {
        Buffer {
            object3d: Object3D::default(),
            name: String::new(),
            format: 0,
            size_multiplier: 1.0,
            texture_filter_operation: 1,
            texture_coord_operation: 0,
            buffer_flags: 0,
        }
    }
}

/// `Command`: what a pass does besides running its snippets.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Command", extends = Object3D, uncreatable)]
pub struct Command {
    #[qml(base)]
    pub object3d: Object3D,
}

/// `BufferInput`: a buffer a pass reads, as INPUT or by `sampler`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "BufferInput", extends = Command)]
pub struct BufferInput {
    #[qml(base)]
    pub command: Command,
    #[qml(notify, object = "Buffer")]
    pub buffer: Option<ObjectId>,
    #[qml(notify)]
    pub sampler: String,
}

/// `SetUniformValue`: a uniform's value for its pass alone.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "SetUniformValue", extends = Command)]
pub struct SetUniformValue {
    #[qml(base)]
    pub command: Command,
    #[qml(notify)]
    pub target: String,
    #[qml(notify)]
    pub value: Value,
}

/// What a program was made from: it is made again only when this
/// changes.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    vertex: String,
    fragment: String,
    shaded: bool,
    uniforms: Vec<(String, UniformKind)>,
    textures: Vec<String>,
}

/// The program last made for an object; `None` when its snippets did not
/// make one, so a broken file is reported once.
#[derive(Debug, Default)]
struct Cache(RefCell<Option<(Key, Option<Arc<Program>>)>>);

impl Cache {
    fn get(&self, key: &Key) -> Option<Option<Arc<Program>>> {
        match &*self.0.borrow() {
            Some((k, p)) if k == key => Some(p.clone()),
            _ => None,
        }
    }
}

/// One of an object's own properties, as a snippet reads it.
pub(crate) enum Input {
    Uniform(String, UniformKind, PropIndex),
    Texture(String, PropIndex),
}

/// The properties `obj`'s QML declares beyond the native type's `native`
/// ones, in their order: those a snippet can read. A `var` or a string
/// has no uniform to be.
pub(crate) fn inputs(e: &Engine, obj: ObjectId, native: u32) -> Vec<Input> {
    let Some(meta) = e.meta_of(obj) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, p) in meta.properties.iter().enumerate().skip(native as usize) {
        let index = PropIndex(i as u32);
        let name = p.name.to_string();
        let kind = match &p.ty {
            TypeTag::Bool => UniformKind::Bool,
            TypeTag::Int | TypeTag::Enum(_) => UniformKind::Int,
            TypeTag::Real => UniformKind::Float,
            TypeTag::Point | TypeTag::Size | TypeTag::Vector2d => UniformKind::Vec2,
            TypeTag::Vector3d => UniformKind::Vec3,
            TypeTag::Color | TypeTag::Rect | TypeTag::Vector4d | TypeTag::Quaternion => {
                UniformKind::Vec4
            }
            TypeTag::Object(Some(t)) if t.as_str() == "TextureInput" => {
                out.push(Input::Texture(name, index));
                continue;
            }
            _ => continue,
        };
        out.push(Input::Uniform(name, kind, index));
    }
    out
}

/// `v` as a uniform of `kind`. A colour is made linear, as the built-in
/// materials' colours are; a quaternion is (x, y, z, scalar).
fn uniform(v: &Value, kind: UniformKind) -> UniformValue {
    let n = |v: &Value| v.to_number() as f32;
    match (kind, v) {
        (UniformKind::Bool, v) => UniformValue::Bool(v.to_boolean()),
        (UniformKind::Int, v) => UniformValue::Int(v.to_number() as i32),
        (UniformKind::Float, v) => UniformValue::Float(n(v)),
        (UniformKind::Vec2, Value::Vector2d(p)) => UniformValue::Vec2([p.x, p.y]),
        (UniformKind::Vec2, Value::Point(p)) => UniformValue::Vec2([p.x as f32, p.y as f32]),
        (UniformKind::Vec2, Value::Size(s)) => {
            UniformValue::Vec2([s.width as f32, s.height as f32])
        }
        (UniformKind::Vec3, Value::Vector3d(p)) => UniformValue::Vec3([p.x, p.y, p.z]),
        (UniformKind::Vec4, Value::Color(c)) => UniformValue::color(color(*c)),
        (UniformKind::Vec4, Value::Vector4d(p)) => UniformValue::Vec4([p.x, p.y, p.z, p.w]),
        (UniformKind::Vec4, Value::Quaternion(q)) => UniformValue::Vec4([q.x, q.y, q.z, q.scalar]),
        (UniformKind::Vec4, Value::Rect(r)) => {
            UniformValue::Vec4([r.x as f32, r.y as f32, r.width as f32, r.height as f32])
        }
        (kind, _) => UniformValue::zero(kind),
    }
}

/// The texture of the TextureInput `input`: `None` reads opaque black,
/// as a disabled one does.
fn texture_input(e: &Engine, view: ObjectId, input: Option<ObjectId>) -> Option<ljos_3d::Texture> {
    let input = input.filter(|i| e.is::<TextureInput>(*i))?;
    if !item::prop(e, input, TextureInput::ENABLED) {
        return None;
    }
    item::prop(e, input, TextureInput::TEXTURE).and_then(|t| texture::read(e, view, t))
}

/// The text of the snippet at `url`. Read on the spot: a snippet is a
/// few lines, read once per change of its name.
fn read_snippet(e: &Engine, url: &str) -> Result<String, String> {
    let platform = Platform::of(e);
    let bytes = if let Some(bytes) = platform.resource(url) {
        bytes.to_vec()
    } else if is_remote(url) {
        return Err("a snippet on the network is not read".to_owned());
    } else {
        let path = url_path(url).ok_or_else(|| "no such file".to_owned())?;
        let path = platform.env().real(&path).map_err(|err| err.to_string())?;
        std::fs::read(path).map_err(|err| err.to_string())?
    };
    String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_owned())
}

/// The program `key` names, made with `make` from the snippets' text the
/// first time; a failure is warned of, naming the file, and remembered.
fn program<T: QmlType>(
    e: &Engine,
    obj: ObjectId,
    who: &str,
    cache: fn(&T) -> &Cache,
    key: Key,
    make: impl FnOnce(Option<&str>, Option<&str>) -> Result<Program, snippet::SnippetError>,
) -> Option<Arc<Program>> {
    if let Some(had) = e.with(obj, |t: &T| cache(t).get(&key)).ok().flatten() {
        return had;
    }
    let text = |url: &str| (!url.is_empty()).then(|| read_snippet(e, url)).transpose();
    let made = match (text(&key.vertex), text(&key.fragment)) {
        (Err(err), _) => Err(format!("{}: {err}", key.vertex)),
        (_, Err(err)) => Err(format!("{}: {err}", key.fragment)),
        (Ok(v), Ok(f)) => make(v.as_deref(), f.as_deref()).map_err(|err| {
            let file = match &err {
                snippet::SnippetError::Compile { stage, .. }
                | snippet::SnippetError::Unsupported { stage, .. } => {
                    if *stage == "vertex" {
                        key.vertex.clone()
                    } else {
                        key.fragment.clone()
                    }
                }
                snippet::SnippetError::Program(_) => {
                    format!("{} and {}", key.vertex, key.fragment)
                }
            };
            format!("{file}: {err}")
        }),
    };
    let made = match made {
        Ok(p) => Some(Arc::new(p)),
        Err(why) => {
            e.warn(format!("QtQuick3D {who}: {why}"));
            None
        }
    };
    let _ = e.with(obj, |t: &T| {
        *cache(t).0.borrow_mut() = Some((key, made.clone()));
    });
    made
}

/// `url` resolved against the file that wrote `obj`; empty for none.
fn resolved(e: &Engine, obj: ObjectId, url: &Url) -> String {
    if url.0.as_str().is_empty() {
        return String::new();
    }
    e.resolve_url(obj, url.0.as_str())
}

/// The CustomMaterial `obj` as the view `view` draws it; `None` when its
/// snippets make no program, which leaves its model out.
pub(crate) fn material(
    e: &Engine,
    view: ObjectId,
    obj: ObjectId,
) -> Option<ljos_3d::CustomMaterial> {
    let inputs = inputs(e, obj, CustomMaterial::PROPERTY_COUNT);
    let (uniforms, textures) = layout(&inputs);
    let shaded = item::prop(e, obj, CustomMaterial::SHADING_MODE) != shading::UNSHADED;
    let key = Key {
        vertex: resolved(e, obj, &item::prop(e, obj, CustomMaterial::VERTEX_SHADER)),
        fragment: resolved(e, obj, &item::prop(e, obj, CustomMaterial::FRAGMENT_SHADER)),
        shaded,
        uniforms,
        textures,
    };
    let (u, t) = (key.uniforms.clone(), key.textures.clone());
    let program = program(
        e,
        obj,
        "CustomMaterial",
        |m: &CustomMaterial| &m.program,
        key,
        |v, f| snippet::material(v, f, shaded, &u, &t),
    )?;
    let mut m = ljos_3d::CustomMaterial::new(program);
    m.values = values(e, obj, &inputs);
    m.textures = inputs
        .iter()
        .filter_map(|i| match i {
            Input::Texture(_, p) => Some(texture_input(e, view, e.read(obj, *p).as_object())),
            Input::Uniform(..) => None,
        })
        .collect();
    let blend = |p| Blend::from_index(i64::from(item::prop(e, obj, p)));
    m.source_blend = blend(CustomMaterial::SOURCE_BLEND);
    m.destination_blend = blend(CustomMaterial::DESTINATION_BLEND);
    m.depth_draw = DepthDraw::from_index(i64::from(item::prop(e, obj, Material::DEPTH_DRAW_MODE)));
    m.cull_mode = match item::prop(e, obj, Material::CULL_MODE) {
        cull::FRONT => CullMode::Front,
        cull::NONE => CullMode::None,
        _ => CullMode::Back,
    };
    Some(m)
}

/// The uniforms' names and kinds and the samplers' names, in order.
fn layout(inputs: &[Input]) -> (Vec<(String, UniformKind)>, Vec<String>) {
    let mut uniforms = Vec::new();
    let mut textures = Vec::new();
    for i in inputs {
        match i {
            Input::Uniform(n, k, _) => uniforms.push((n.clone(), *k)),
            Input::Texture(n, _) => textures.push(n.clone()),
        }
    }
    (uniforms, textures)
}

/// The uniforms' values now; tracked, so a change draws again.
fn values(e: &Engine, obj: ObjectId, inputs: &[Input]) -> Vec<UniformValue> {
    inputs
        .iter()
        .filter_map(|i| match i {
            Input::Uniform(_, k, p) => Some(uniform(&e.read(obj, *p), *k)),
            Input::Texture(..) => None,
        })
        .collect()
}

/// What a pass without a fragment snippet runs: its input as it is.
const COPY: &str = "void MAIN()\n{\n    FRAGCOLOR = texture(INPUT, INPUT_UV);\n}\n";

/// The Effect `obj` as the view `view` runs it; `None` when a pass's
/// snippets make no program, which leaves the effect out.
pub(crate) fn effect(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<ljos_3d::Effect> {
    if !e.is::<Effect>(obj) {
        return None;
    }
    let inputs = inputs(e, obj, Effect::PROPERTY_COUNT);
    let (uniforms, textures) = layout(&inputs);
    let values = values(e, obj, &inputs);
    let own_textures: Vec<Option<ljos_3d::Texture>> = inputs
        .iter()
        .filter_map(|i| match i {
            Input::Texture(_, p) => Some(texture_input(e, view, e.read(obj, *p).as_object())),
            Input::Uniform(..) => None,
        })
        .collect();
    let mut buffers: Vec<ObjectId> = Vec::new();
    let mut index_of = |b: ObjectId| {
        buffers.iter().position(|x| *x == b).unwrap_or_else(|| {
            buffers.push(b);
            buffers.len() - 1
        })
    };
    let mut passes = Vec::new();
    for pass in list(e, obj, Effect::PASSES.index) {
        if !e.is::<Pass>(pass) {
            continue;
        }
        let (mut vertex, mut fragment) = (String::new(), String::new());
        for s in list(e, pass, Pass::SHADERS.index) {
            if !e.is::<Shader>(s) {
                continue;
            }
            let url = resolved(e, s, &item::prop(e, s, Shader::SHADER));
            match item::prop(e, s, Shader::STAGE) {
                stage::VERTEX => vertex = url,
                _ => fragment = url,
            }
        }
        let mut names = textures.clone();
        let mut read: Vec<PassTexture> = own_textures
            .iter()
            .cloned()
            .map(PassTexture::Texture)
            .collect();
        let mut pass_values = values.clone();
        let mut input = None;
        for c in list(e, pass, Pass::COMMANDS.index) {
            if e.is::<BufferInput>(c) {
                let Some(b) = item::prop(e, c, BufferInput::BUFFER).filter(|b| e.is::<Buffer>(*b))
                else {
                    continue;
                };
                let i = index_of(b);
                let sampler = item::prop(e, c, BufferInput::SAMPLER);
                if sampler.is_empty() {
                    input = Some(i);
                } else if let Some(at) = names.iter().position(|n| *n == sampler) {
                    read[at] = PassTexture::Buffer(i);
                } else {
                    names.push(sampler);
                    read.push(PassTexture::Buffer(i));
                }
            } else if e.is::<SetUniformValue>(c) {
                let target = item::prop(e, c, SetUniformValue::TARGET);
                if let Some(at) = uniforms.iter().position(|(n, _)| *n == target) {
                    let v = e.read(c, SetUniformValue::VALUE.index);
                    pass_values[at] = uniform(&v, uniforms[at].1);
                }
            }
        }
        let output = item::prop(e, pass, Pass::OUTPUT)
            .filter(|b| e.is::<Buffer>(*b))
            .map(&mut index_of);
        let key = Key {
            vertex,
            fragment,
            shaded: false,
            uniforms: uniforms.clone(),
            textures: names,
        };
        let (u, t) = (key.uniforms.clone(), key.textures.clone());
        let program = program(
            e,
            pass,
            "Effect",
            |p: &Pass| &p.program,
            key,
            |v, f| snippet::effect(v, f.unwrap_or(COPY), &u, &t),
        )?;
        passes.push(ljos_3d::Pass {
            program,
            values: pass_values,
            textures: read,
            input,
            output,
        });
    }
    let buffers = buffers.iter().map(|b| buffer(e, *b)).collect();
    Some(ljos_3d::Effect { buffers, passes })
}

fn buffer(e: &Engine, b: ObjectId) -> EffectBuffer {
    EffectBuffer {
        format: BufferFormat::from_index(i64::from(item::prop(e, b, Buffer::FORMAT))),
        size_multiplier: item::prop(e, b, Buffer::SIZE_MULTIPLIER) as f32,
        filter: match item::prop(e, b, Buffer::TEXTURE_FILTER_OPERATION) {
            0 => Filter::Nearest,
            _ => Filter::Linear,
        },
        tiling: match item::prop(e, b, Buffer::TEXTURE_COORD_OPERATION) {
            1 => Tiling::Repeat,
            2 => Tiling::MirroredRepeat,
            _ => Tiling::ClampToEdge,
        },
    }
}
