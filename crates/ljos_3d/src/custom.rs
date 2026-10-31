//! What an author's shader snippets make of a scene: a [`CustomMaterial`]
//! draws a model with them, an [`Effect`] runs them over the picture.
//!
//! A snippet is GLSL written against Qt Quick 3D's documented keywords.
//! The `snippets` feature translates one into a [`Program`]; the scene
//! only holds programs and the values their uniforms take, so it compares
//! and clones cheaply, and the renderer needs no shader compiler of its
//! own.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::scene::{Color, CullMode};
use crate::texture::{Filter, Texture, Tiling};

/// The type of one of a program's uniforms: an author's property, as the
/// GLSL it is read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UniformKind {
    Bool,
    Int,
    Float,
    Vec2,
    Vec3,
    Vec4,
    Mat4,
}

impl UniformKind {
    /// Its alignment and size in a std140 block; a bool is an int there,
    /// as a shader's bool is not something a buffer can hold.
    fn std140(self) -> (usize, usize) {
        match self {
            UniformKind::Bool | UniformKind::Int | UniformKind::Float => (4, 4),
            UniformKind::Vec2 => (8, 8),
            UniformKind::Vec3 => (16, 12),
            UniformKind::Vec4 => (16, 16),
            UniformKind::Mat4 => (16, 64),
        }
    }
}

/// A uniform's value. Colours are linear by now; a matrix is in columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UniformValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat4([[f32; 4]; 4]),
}

impl UniformValue {
    pub fn kind(&self) -> UniformKind {
        match self {
            UniformValue::Bool(_) => UniformKind::Bool,
            UniformValue::Int(_) => UniformKind::Int,
            UniformValue::Float(_) => UniformKind::Float,
            UniformValue::Vec2(_) => UniformKind::Vec2,
            UniformValue::Vec3(_) => UniformKind::Vec3,
            UniformValue::Vec4(_) => UniformKind::Vec4,
            UniformValue::Mat4(_) => UniformKind::Mat4,
        }
    }

    /// A colour as a shader reads it: linear, not premultiplied.
    pub fn color(c: Color) -> Self {
        UniformValue::Vec4(c.to_linear())
    }

    /// The zero of `kind`, for a uniform whose property is missing.
    pub fn zero(kind: UniformKind) -> Self {
        match kind {
            UniformKind::Bool => UniformValue::Bool(false),
            UniformKind::Int => UniformValue::Int(0),
            UniformKind::Float => UniformValue::Float(0.0),
            UniformKind::Vec2 => UniformValue::Vec2([0.0; 2]),
            UniformKind::Vec3 => UniformValue::Vec3([0.0; 3]),
            UniformKind::Vec4 => UniformValue::Vec4([0.0; 4]),
            UniformKind::Mat4 => UniformValue::Mat4([[0.0; 4]; 4]),
        }
    }

    /// Writes it at the start of `out`, as the block holds it.
    pub fn write(&self, out: &mut [u8]) {
        let mut put = |i: usize, bytes: [u8; 4]| out[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        match *self {
            UniformValue::Bool(b) => put(0, i32::from(b).to_ne_bytes()),
            UniformValue::Int(i) => put(0, i.to_ne_bytes()),
            UniformValue::Float(f) => put(0, f.to_ne_bytes()),
            UniformValue::Vec2(v) => v
                .iter()
                .enumerate()
                .for_each(|(i, f)| put(i, f.to_ne_bytes())),
            UniformValue::Vec3(v) => v
                .iter()
                .enumerate()
                .for_each(|(i, f)| put(i, f.to_ne_bytes())),
            UniformValue::Vec4(v) => v
                .iter()
                .enumerate()
                .for_each(|(i, f)| put(i, f.to_ne_bytes())),
            UniformValue::Mat4(m) => m
                .iter()
                .flatten()
                .enumerate()
                .for_each(|(i, f)| put(i, f.to_ne_bytes())),
        }
    }
}

/// What a program is run as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProgramKind {
    /// A CustomMaterial that writes FRAGCOLOR as it is.
    Unshaded,
    /// A CustomMaterial lit like the built-in materials.
    Shaded,
    /// A pass of an Effect.
    Effect,
}

/// What a material reads of the scene drawn before it: `SCREEN_TEXTURE`,
/// or `SCREEN_MIP_TEXTURE`, which makes the texture's mip levels too.
/// Ordered by what it costs: a view with both makes the mip levels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Screen {
    #[default]
    None,
    Texture,
    Mipmapped,
}

/// Snippets translated for the renderer. Made by
/// `snippet::material` and `snippet::effect`; two programs are the same
/// only when they are one.
#[derive(Debug)]
pub struct Program {
    id: u64,
    kind: ProgramKind,
    wgsl: String,
    uniforms: Vec<(String, UniformKind)>,
    textures: Vec<String>,
    reads_depth: bool,
    screen: Screen,
    /// Where each uniform is in the block, after the header.
    offsets: Vec<usize>,
    /// The bytes the uniforms take, a whole number of rows and at least
    /// one, as a block may not end inside one.
    size: usize,
}

static PROGRAMS: AtomicU64 = AtomicU64::new(1);

impl Program {
    #[cfg_attr(not(feature = "snippets"), allow(dead_code))]
    pub(crate) fn new(
        kind: ProgramKind,
        wgsl: String,
        uniforms: Vec<(String, UniformKind)>,
        textures: Vec<String>,
        reads_depth: bool,
    ) -> Self {
        let mut offsets = Vec::with_capacity(uniforms.len());
        let mut end = 0usize;
        for (_, kind) in &uniforms {
            let (align, size) = kind.std140();
            let at = end.next_multiple_of(align);
            offsets.push(at);
            end = at + size;
        }
        Program {
            id: PROGRAMS.fetch_add(1, Ordering::Relaxed),
            kind,
            wgsl,
            uniforms,
            textures,
            reads_depth,
            screen: Screen::None,
            offsets,
            size: end.max(1).next_multiple_of(16),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn kind(&self) -> ProgramKind {
        self.kind
    }

    /// The whole shader module, the renderer's own code included.
    pub fn wgsl(&self) -> &str {
        &self.wgsl
    }

    /// The uniforms, in the order their values are given.
    pub fn uniforms(&self) -> &[(String, UniformKind)] {
        &self.uniforms
    }

    /// The samplers, in the order their textures are given.
    pub fn textures(&self) -> &[String] {
        &self.textures
    }

    /// Whether it samples DEPTH_TEXTURE, which needs a depth pass.
    pub fn reads_depth(&self) -> bool {
        self.reads_depth
    }

    /// Whether, and how, it samples the scene drawn behind it.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    #[cfg_attr(not(feature = "snippets"), allow(dead_code))]
    pub(crate) fn reading_screen(mut self, screen: Screen) -> Self {
        self.screen = screen;
        self
    }

    #[cfg(all(test, feature = "snippets"))]
    pub(crate) fn offsets(&self) -> &[usize] {
        &self.offsets
    }

    /// The bytes of its whole uniform block.
    pub fn block_size(&self) -> usize {
        HEADER_ROWS * 16 + self.size
    }

    /// Writes `values` after the header of `block`. A value of another
    /// kind than its uniform's (a property changed type) is left zero.
    pub fn write_values(&self, values: &[UniformValue], block: &mut [u8]) {
        let area = &mut block[HEADER_ROWS * 16..];
        area.fill(0);
        for ((value, (_, kind)), &at) in values.iter().zip(&self.uniforms).zip(&self.offsets) {
            if value.kind() == *kind {
                value.write(&mut area[at..]);
            }
        }
    }
}

/// Rows of every program's uniform block before its own values: the
/// model's, the view's and the projection's matrices, then the camera, the
/// clip planes and frame, and the input's and output's sizes. `snippet`
/// declares the block and the renderer fills it.
pub const HEADER_ROWS: usize = 28;

/// The entry points a program has, by kind.
pub const MATERIAL_VERTEX: &str = "cm_vs";
/// For a mesh with a second vertex buffer: a second UV set or joints.
pub const MATERIAL_VERTEX_EXTRA: &str = "cm_vs_extra";
/// Premultiplied, as the built-in materials blend.
pub const MATERIAL_FRAGMENT: &str = "cm_fs";
/// Straight, for the blend factors a CustomMaterial names.
pub const MATERIAL_FRAGMENT_STRAIGHT: &str = "cm_fs_straight";
pub const EFFECT_VERTEX: &str = "cm_effect_vs";
pub const EFFECT_FRAGMENT: &str = "cm_effect_fs";

impl PartialEq for Program {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// `CustomMaterial.sourceBlend` and `destinationBlend`, in the
/// documentation's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blend {
    #[default]
    NoBlend,
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    ConstantColor,
    OneMinusConstantColor,
    ConstantAlpha,
    OneMinusConstantAlpha,
    SrcAlphaSaturate,
}

impl Blend {
    /// The documentation's number of each factor.
    pub fn from_index(i: i64) -> Self {
        use Blend::*;
        const ALL: [Blend; 16] = [
            NoBlend,
            Zero,
            One,
            SrcColor,
            OneMinusSrcColor,
            DstColor,
            OneMinusDstColor,
            SrcAlpha,
            OneMinusSrcAlpha,
            DstAlpha,
            OneMinusDstAlpha,
            ConstantColor,
            OneMinusConstantColor,
            ConstantAlpha,
            OneMinusConstantAlpha,
            SrcAlphaSaturate,
        ];
        usize::try_from(i)
            .ok()
            .and_then(|i| ALL.get(i).copied())
            .unwrap_or(NoBlend)
    }
}

/// `Material.depthDrawMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DepthDraw {
    /// Opaque materials write depth, blended ones do not.
    #[default]
    OpaqueOnly,
    Always,
    Never,
    /// Drawn as `Always`: the depth a pre-pass would lay is the one the
    /// draw itself writes.
    OpaquePrePass,
}

impl DepthDraw {
    pub fn from_index(i: i64) -> Self {
        match i {
            1 => DepthDraw::Always,
            2 => DepthDraw::Never,
            3 => DepthDraw::OpaquePrePass,
            _ => DepthDraw::OpaqueOnly,
        }
    }

    /// Whether a material drawn this way writes depth.
    pub fn writes(self, blends: bool) -> bool {
        match self {
            DepthDraw::OpaqueOnly => !blends,
            DepthDraw::Always | DepthDraw::OpaquePrePass => true,
            DepthDraw::Never => false,
        }
    }
}

/// `CustomMaterial`: a model drawn with a program.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomMaterial {
    pub program: Arc<Program>,
    /// The program's uniforms' values, in its order.
    pub values: Vec<UniformValue>,
    /// The program's samplers' textures, in its order; `None` reads
    /// opaque black, as a disabled TextureInput does.
    pub textures: Vec<Option<Texture>>,
    pub source_blend: Blend,
    pub destination_blend: Blend,
    pub depth_draw: DepthDraw,
    pub cull_mode: CullMode,
}

impl CustomMaterial {
    /// A material with `program`'s uniforms at zero and no textures.
    pub fn new(program: Arc<Program>) -> Self {
        let values = program
            .uniforms()
            .iter()
            .map(|(_, k)| UniformValue::zero(*k))
            .collect();
        let textures = vec![None; program.textures().len()];
        CustomMaterial {
            program,
            values,
            textures,
            source_blend: Blend::NoBlend,
            destination_blend: Blend::NoBlend,
            depth_draw: DepthDraw::OpaqueOnly,
            cull_mode: CullMode::Back,
        }
    }

    /// Blending needs both factors; one alone draws as opaque.
    pub fn blends(&self) -> bool {
        self.source_blend != Blend::NoBlend && self.destination_blend != Blend::NoBlend
    }
}

/// `Buffer.format`, in the documentation's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BufferFormat {
    #[default]
    Rgba8,
    Rgba16F,
    Rgba32F,
    R8,
    R16,
    R16F,
    R32F,
}

impl BufferFormat {
    pub fn from_index(i: i64) -> Self {
        match i {
            1 => BufferFormat::Rgba16F,
            2 => BufferFormat::Rgba32F,
            3 => BufferFormat::R8,
            4 => BufferFormat::R16,
            5 => BufferFormat::R16F,
            6 => BufferFormat::R32F,
            _ => BufferFormat::Rgba8,
        }
    }
}

/// A `Buffer` an effect's passes write and read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectBuffer {
    pub format: BufferFormat,
    /// Of the effect's input's size.
    pub size_multiplier: f32,
    pub filter: Filter,
    pub tiling: Tiling,
}

impl Default for EffectBuffer {
    fn default() -> Self {
        EffectBuffer {
            format: BufferFormat::Rgba8,
            size_multiplier: 1.0,
            filter: Filter::Linear,
            tiling: Tiling::ClampToEdge,
        }
    }
}

/// What a sampler of a pass reads.
#[derive(Clone, Debug, PartialEq)]
pub enum PassTexture {
    /// A TextureInput's; `None` reads opaque black.
    Texture(Option<Texture>),
    /// One of the effect's buffers, as a BufferInput names it.
    Buffer(usize),
}

/// One `Pass` of an effect.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub program: Arc<Program>,
    /// The uniforms' values for this pass, its SetUniformValue commands
    /// applied.
    pub values: Vec<UniformValue>,
    /// What the program's samplers read, in its order.
    pub textures: Vec<PassTexture>,
    /// The buffer INPUT reads; `None` for the effect's input.
    pub input: Option<usize>,
    /// The buffer it writes; `None` for the effect's output.
    pub output: Option<usize>,
}

/// `Effect`: passes run over the picture in their order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub buffers: Vec<EffectBuffer>,
    pub passes: Vec<Pass>,
}

impl Effect {
    /// Whether a pass samples DEPTH_TEXTURE.
    pub fn reads_depth(&self) -> bool {
        self.passes.iter().any(|p| p.program.reads_depth())
    }
}

/// `ExtendedSceneEnvironment.glowBlendMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlowBlend {
    Additive,
    Screen,
    #[default]
    SoftLight,
    Replace,
}

impl GlowBlend {
    pub fn from_index(i: i64) -> Self {
        match i {
            0 => GlowBlend::Additive,
            1 => GlowBlend::Screen,
            3 => GlowBlend::Replace,
            _ => GlowBlend::SoftLight,
        }
    }
}

/// Glow and bloom: bright light spread past where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub strength: f32,
    pub intensity: f32,
    pub bloom: f32,
    pub hdr_minimum: f32,
    pub hdr_maximum: f32,
    pub hdr_scale: f32,
    /// A bit per blur level, the first the finest; seven levels.
    pub levels: u32,
    pub blend: GlowBlend,
    pub quality_high: bool,
}

impl Default for Glow {
    fn default() -> Self {
        Glow {
            strength: 1.0,
            intensity: 1.0,
            bloom: 0.0,
            hdr_minimum: 1.0,
            hdr_maximum: 12.0,
            hdr_scale: 2.0,
            levels: 1,
            blend: GlowBlend::SoftLight,
            quality_high: false,
        }
    }
}

/// Depth of field: what is not near the focus distance is blurred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthOfField {
    pub focus_distance: f32,
    pub focus_range: f32,
    pub blur_amount: f32,
}

/// Brightness, contrast and saturation, each 1 for none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjustments {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vignette {
    /// Linear.
    pub color: [f32; 3],
    pub radius: f32,
    pub strength: f32,
}

/// Colour grading through a look-up table `size` cells a side, laid out
/// `size * size` wide and `size` high; without a texture, the identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut {
    pub texture: Option<Texture>,
    pub size: f32,
    pub alpha: f32,
}

/// ExtendedSceneEnvironment's built-in post-processing, each effect
/// there when it is enabled. They run after the environment's effects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PostProcess {
    /// On for an ExtendedSceneEnvironment: it always draws through the
    /// post-processing passes, with nothing else enabled too.
    pub on: bool,
    pub glow: Option<Glow>,
    pub depth_of_field: Option<DepthOfField>,
    pub adjustments: Option<Adjustments>,
    pub vignette: Option<Vignette>,
    pub lut: Option<Lut>,
    pub fxaa: bool,
    pub dithering: bool,
    /// 0 to 1.
    pub sharpness: f32,
}

/// Screen-space ambient occlusion, `SceneEnvironment.ao*`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientOcclusion {
    /// 0 to 100; 0 is none.
    pub strength: f32,
    /// How far, in scene units, a surface shades another.
    pub distance: f32,
    /// 0 to 50.
    pub softness: f32,
    /// 2 to 4.
    pub sample_rate: u32,
    pub bias: f32,
    pub dither: bool,
}

impl AmbientOcclusion {
    /// Whether it darkens anything: neither its strength nor its distance
    /// is 0.
    pub fn shows(&self) -> bool {
        self.strength > 0.0 && self.distance > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_needs_both_factors() {
        let program = Arc::new(Program::new(
            ProgramKind::Unshaded,
            String::new(),
            vec![("level".into(), UniformKind::Float)],
            Vec::new(),
            false,
        ));
        let mut m = CustomMaterial::new(program);
        assert_eq!(m.values, [UniformValue::Float(0.0)]);
        m.source_blend = Blend::One;
        assert!(!m.blends());
        m.destination_blend = Blend::from_index(2);
        assert!(m.blends());
    }

    #[test]
    fn values_are_laid_out_as_std140_has_them() {
        let empty = Program::new(
            ProgramKind::Effect,
            String::new(),
            Vec::new(),
            Vec::new(),
            false,
        );
        assert_eq!(empty.block_size(), HEADER_ROWS * 16 + 16);
        let uniforms = [
            UniformKind::Float,
            UniformKind::Vec3,
            UniformKind::Int,
            UniformKind::Vec2,
            UniformKind::Mat4,
        ];
        let p = Program::new(
            ProgramKind::Effect,
            String::new(),
            uniforms.iter().map(|k| (String::new(), *k)).collect(),
            Vec::new(),
            false,
        );
        // The vec3 starts a row and the int fills its last float.
        assert_eq!(p.offsets, [0, 16, 28, 32, 48]);
        assert_eq!(p.block_size(), HEADER_ROWS * 16 + 112);
        let mut block = vec![1; p.block_size()];
        p.write_values(
            &[UniformValue::Float(2.0), UniformValue::Bool(true)],
            &mut block,
        );
        let area = &block[HEADER_ROWS * 16..];
        assert_eq!(area[..4], 2.0f32.to_ne_bytes());
        // A bool where a vec3 is wanted is not written.
        assert!(area[16..28].iter().all(|&b| b == 0));
    }
}
