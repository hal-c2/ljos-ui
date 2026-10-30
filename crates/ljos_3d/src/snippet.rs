//! An author's GLSL snippets translated into a [`Program`] the renderer
//! draws with.
//!
//! A snippet is not a shader: it is a `MAIN` (and, for a shaded material,
//! light processors) written against Qt Quick 3D's keywords. Each stage's
//! snippet is wrapped in GLSL that declares the keywords as globals, the
//! author's properties as reads of one uniform block, and functions that
//! fill the keywords, call the snippet and hand back what it wrote. naga
//! turns that into WGSL, every name prefixed so that the two stages and
//! the renderer's own code share one module, and Rust-written WGSL glues
//! it to the renderer's lighting (lit.wgsl) and entry points.
//!
//! The wrappers' own names are letters only and start with `ljos`, as
//! naga's WGSL writer keeps such names as they are; the glue can then call
//! them by name.

use std::fmt::Write as _;

use naga::{Module, ShaderStage};

use crate::custom::{
    EFFECT_FRAGMENT, EFFECT_VERTEX, HEADER_ROWS, MATERIAL_FRAGMENT, MATERIAL_FRAGMENT_STRAIGHT,
    MATERIAL_VERTEX, Program, ProgramKind, UniformKind,
};

/// At most this many VARYINGs, each one location: WebGL2 guarantees few
/// more inter-stage components than the material's own and these take.
pub const MAX_VARYINGS: usize = 8;
/// At most this many TextureInputs, WebGL2's sixteen samplers less the
/// renderer's own.
pub const MAX_TEXTURES: usize = 8;

/// Why snippets were not made into a program.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SnippetError {
    /// The snippet does not compile; the line is the snippet's own.
    #[error("{stage} shader, line {line}: {message}")]
    Compile {
        stage: &'static str,
        line: u32,
        message: String,
    },
    /// It uses something this translation does not have.
    #[error("{stage} shader: {message}")]
    Unsupported {
        stage: &'static str,
        message: String,
    },
    /// The translation did not validate: a fault here, or a snippet that
    /// compiles as GLSL but breaks a rule WGSL keeps (sampling in
    /// non-uniform control flow, say).
    #[error("{0}")]
    Program(String),
}

const LIT: &str = concat!(
    include_str!("shaders/tone.wgsl"),
    "\n",
    include_str!("shaders/lit.wgsl")
);

/// A CustomMaterial's snippets: `shaded` lights it, otherwise FRAGCOLOR is
/// drawn as it is. `uniforms` are its properties in the order their
/// values are given, `textures` its TextureInput properties.
pub fn material(
    vertex: Option<&str>,
    fragment: Option<&str>,
    shaded: bool,
    uniforms: &[(String, UniformKind)],
    textures: &[String],
) -> Result<Program, SnippetError> {
    check_names(uniforms, textures)?;
    let vertex_src = vertex.map(strip_comments).unwrap_or_default();
    let fragment_src = fragment.map(strip_comments).unwrap_or_default();
    let out = varyings(&vertex_src, "vertex")?;
    let ins = varyings(&fragment_src, "fragment")?;
    if fragment_src.split(is_break).any(|w| w == "SCREEN_TEXTURE") {
        return Err(SnippetError::Unsupported {
            stage: "fragment",
            message: "SCREEN_TEXTURE is not available".into(),
        });
    }

    let set = 2;
    let header = header(set, uniforms, textures, &[], false);
    let vs = stage_module(
        "vertex",
        ShaderStage::Vertex,
        material_vertex(&header, vertex.unwrap_or(""), &vertex_src, &out),
        "cmv_",
    )?;
    let fs = stage_module(
        "fragment",
        ShaderStage::Fragment,
        material_fragment(&header, fragment.unwrap_or(""), &fragment_src, &ins, shaded),
        "cmf_",
    )?;
    let functions = Functions::of(&fragment_src);
    let mut wgsl = String::from(LIT);
    wgsl.push_str(&vs);
    wgsl.push_str(&fs);
    wgsl.push_str(&material_glue(&out, &ins, shaded, &functions));
    validate(&wgsl)?;
    let kind = if shaded {
        ProgramKind::Shaded
    } else {
        ProgramKind::Unshaded
    };
    Ok(Program::new(
        kind,
        wgsl,
        uniforms.to_vec(),
        textures.to_vec(),
        false,
    ))
}

/// A pass of an Effect: its fragment snippet runs over the whole output,
/// reading the picture so far as INPUT; a vertex snippet may move the
/// triangle that covers it. `textures` are its own samplers after INPUT.
pub fn effect(
    vertex: Option<&str>,
    fragment: &str,
    uniforms: &[(String, UniformKind)],
    textures: &[String],
) -> Result<Program, SnippetError> {
    check_names(uniforms, textures)?;
    let vertex_src = vertex.map(strip_comments).unwrap_or_default();
    let fragment_src = strip_comments(fragment);
    let out = varyings(&vertex_src, "vertex")?;
    let ins = varyings(&fragment_src, "fragment")?;
    let depth = fragment_src
        .split(is_break)
        .chain(vertex_src.split(is_break))
        .any(|w| w == "DEPTH_TEXTURE");

    let set = 0;
    let header = header(set, uniforms, textures, &["INPUT"], depth);
    let vs = stage_module(
        "vertex",
        ShaderStage::Vertex,
        effect_vertex(&header, vertex.unwrap_or(""), &vertex_src, &out),
        "cmv_",
    )?;
    let fs = stage_module(
        "fragment",
        ShaderStage::Fragment,
        effect_fragment(&header, fragment, &fragment_src, &ins),
        "cmf_",
    )?;
    let mut wgsl = vs;
    wgsl.push_str(&fs);
    wgsl.push_str(&effect_glue(&out, &ins));
    validate(&wgsl)?;
    Ok(Program::new(
        ProgramKind::Effect,
        wgsl,
        uniforms.to_vec(),
        textures.to_vec(),
        depth,
    ))
}

fn check_names(
    uniforms: &[(String, UniformKind)],
    textures: &[String],
) -> Result<(), SnippetError> {
    if textures.len() > MAX_TEXTURES {
        return Err(SnippetError::Unsupported {
            stage: "fragment",
            message: format!("more than {MAX_TEXTURES} textures"),
        });
    }
    let bad = uniforms
        .iter()
        .map(|(n, _)| n)
        .chain(textures)
        .find(|n| !is_identifier(n));
    match bad {
        Some(n) => Err(SnippetError::Unsupported {
            stage: "fragment",
            message: format!("\"{n}\" is not a GLSL name"),
        }),
        None => Ok(()),
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_break(c: char) -> bool {
    !(c.is_ascii_alphanumeric() || c == '_')
}

/// `src` with its comments blanked out, lines kept, for the scans that
/// look for functions, VARYINGs and keywords.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    }
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// The identifiers and the punctuation of `src`, in order.
fn tokens(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in src.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' || (c == '.' && start.is_some()) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            out.push(&src[s..i]);
        }
        if !c.is_whitespace() {
            out.push(&src[i..i + c.len_utf8()]);
        }
    }
    if let Some(s) = start {
        out.push(&src[s..]);
    }
    out
}

/// Whether `src` defines `void name(`.
fn defines(src: &str, name: &str) -> bool {
    tokens(src)
        .windows(3)
        .any(|w| w[0] == "void" && w[1] == name && w[2] == "(")
}

/// Which of the documented functions a fragment snippet defines.
#[derive(Default)]
struct Functions {
    main: bool,
    ambient: bool,
    directional: bool,
    point: bool,
    spot: bool,
    specular: bool,
    post: bool,
}

impl Functions {
    fn of(src: &str) -> Self {
        Functions {
            main: defines(src, "MAIN"),
            ambient: defines(src, "AMBIENT_LIGHT"),
            directional: defines(src, "DIRECTIONAL_LIGHT"),
            point: defines(src, "POINT_LIGHT"),
            spot: defines(src, "SPOT_LIGHT"),
            specular: defines(src, "SPECULAR_LIGHT"),
            post: defines(src, "POST_PROCESS"),
        }
    }
}

/// A `VARYING type name;` of a snippet: its name and how many floats.
struct Varying {
    name: String,
    width: usize,
}

fn varyings(src: &str, stage: &'static str) -> Result<Vec<Varying>, SnippetError> {
    let tokens = tokens(src);
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if *t != "VARYING" {
            continue;
        }
        let (ty, name) = match (tokens.get(i + 1), tokens.get(i + 2)) {
            (Some(ty), Some(name)) => (*ty, *name),
            _ => break,
        };
        let width = match ty {
            "float" => 1,
            "vec2" => 2,
            "vec3" => 3,
            "vec4" => 4,
            _ => {
                return Err(SnippetError::Unsupported {
                    stage,
                    message: format!("VARYING {ty} {name}: only float and vec types pass"),
                });
            }
        };
        out.push(Varying {
            name: name.to_owned(),
            width,
        });
    }
    if out.len() > MAX_VARYINGS {
        return Err(SnippetError::Unsupported {
            stage,
            message: format!("more than {MAX_VARYINGS} VARYINGs"),
        });
    }
    Ok(out)
}

/// The letters naming the `i`th of something in the wrappers, as names
/// that end in a digit are changed by naga's writer.
fn letters(i: usize) -> String {
    let mut s = String::new();
    let mut n = i;
    loop {
        s.insert(0, char::from(b'a' + (n % 26) as u8));
        n /= 26;
        if n == 0 {
            return s;
        }
        n -= 1;
    }
}

fn pack(v: &Varying) -> String {
    match v.width {
        1 => format!("vec4({}, 0.0, 0.0, 0.0)", v.name),
        2 => format!("vec4({}, 0.0, 0.0)", v.name),
        3 => format!("vec4({}, 0.0)", v.name),
        _ => v.name.clone(),
    }
}

fn unpack(v: &Varying, from: &str) -> String {
    let swizzle = match v.width {
        1 => ".x",
        2 => ".xy",
        3 => ".xyz",
        _ => "",
    };
    format!("    {} = {from}{swizzle};\n", v.name)
}

/// What both stages' wrappers start with: the uniform block, the
/// author's properties read from it, the samplers and the matrices.
/// `fixed` are samplers before the author's (an Effect's INPUT).
fn header(
    set: u32,
    uniforms: &[(String, UniformKind)],
    textures: &[String],
    fixed: &[&str],
    depth: bool,
) -> String {
    debug_assert_eq!(HEADER_ROWS, 28);
    // The author's properties are members of the block, which has no
    // instance name, so that a snippet reads them by their own names.
    // Bools are ints in the block and read through a test.
    let mut members = String::new();
    let mut flags = String::new();
    for (i, (name, kind)) in uniforms.iter().enumerate() {
        let ty = match kind {
            UniformKind::Bool => {
                let l = letters(i);
                let _ = writeln!(flags, "#define {name} (ljosflag{l} != 0)");
                let _ = write!(members, "int ljosflag{l}; ");
                continue;
            }
            UniformKind::Int => "int",
            UniformKind::Float => "float",
            UniformKind::Vec2 => "vec2",
            UniformKind::Vec3 => "vec3",
            UniformKind::Vec4 => "vec4",
            UniformKind::Mat4 => "mat4",
        };
        let _ = write!(members, "{ty} {name}; ");
    }
    let mut h = format!(
        "#version 440\n\
         layout(std140, set = {set}, binding = 32) uniform LjosCustom {{\n\
         mat4 ljosmodel; mat4 ljosnormal; mat4 ljosview; mat4 ljosprojection;\n\
         mat4 ljosinverse; mat4 ljosviewprojection;\n\
         vec4 ljoscamera; vec4 ljosdirection; vec4 ljosclip; vec4 ljossizes;\n\
         {members}\n\
         }};\n\
         {flags}\
         #define MODEL_MATRIX ljosmodel\n\
         #define NORMAL_MATRIX mat3(ljosnormal)\n\
         #define VIEW_MATRIX ljosview\n\
         #define PROJECTION_MATRIX ljosprojection\n\
         #define INVERSE_PROJECTION_MATRIX ljosinverse\n\
         #define VIEWPROJECTION_MATRIX ljosviewprojection\n\
         #define MODELVIEWPROJECTION_MATRIX (ljosviewprojection * ljosmodel)\n\
         #define CAMERA_POSITION ljoscamera.xyz\n\
         #define CAMERA_DIRECTION ljosdirection.xyz\n\
         #define CAMERA_PROPERTIES ljosclip.xy\n\
         #define FRAME ljosclip.z\n\
         #define INPUT_SIZE ljossizes.xy\n\
         #define OUTPUT_SIZE ljossizes.zw\n\
         #define FRAMEBUFFER_Y_UP (-1.0)\n\
         #define NDC_Y_UP 1.0\n\
         #define NEAR_CLIP_VALUE 0.0\n\
         #define VARYING\n"
    );
    for (i, name) in fixed
        .iter()
        .copied()
        .chain(textures.iter().map(String::as_str))
        .enumerate()
    {
        let l = letters(i);
        let _ = writeln!(
            h,
            "layout(set = {set}, binding = {}) uniform texture2D ljostex{l};\n\
             layout(set = {set}, binding = {}) uniform sampler ljossampler{l};\n\
             #define {name} sampler2D(ljostex{l}, ljossampler{l})",
            33 + 2 * i,
            34 + 2 * i
        );
    }
    if depth {
        let _ = writeln!(
            h,
            "layout(set = {set}, binding = 30) uniform texture2D ljosdepth;\n\
             layout(set = {set}, binding = 31) uniform sampler ljosdepthsampler;\n\
             #define DEPTH_TEXTURE sampler2D(ljosdepth, ljosdepthsampler)"
        );
    }
    h
}

fn lines(s: &str) -> u32 {
    s.matches('\n').count() as u32
}

fn material_vertex(header: &str, user: &str, src: &str, out: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str(
        "vec3 VERTEX; vec3 NORMAL; vec2 UV0; vec2 UV1; vec3 TANGENT; vec3 BINORMAL;\n\
         vec4 COLOR; vec4 POSITION;\n",
    );
    let before = lines(&s);
    s.push_str(user);
    s.push_str("\nstruct LjosVertex {\n    vec4 ljosclip; vec3 ljosworld; vec3 ljosnormal; vec4 ljosuvs;\n    vec3 ljostangent; vec3 ljosbinormal; vec4 ljoscolor;\n");
    for i in 0..out.len() {
        let _ = writeln!(s, "    vec4 ljosvary{};", letters(i));
    }
    s.push_str(
        "};\n\
         LjosVertex ljosvertex(vec3 ljosposition, vec3 ljosnormalin, vec2 ljosuv, vec4 ljostangentin) {\n\
         \x20   VERTEX = ljosposition; NORMAL = ljosnormalin; UV0 = ljosuv; UV1 = ljosuv;\n\
         \x20   TANGENT = ljostangentin.xyz;\n\
         \x20   BINORMAL = cross(ljosnormalin, ljostangentin.xyz) * ljostangentin.w;\n\
         \x20   COLOR = vec4(1.0); POSITION = vec4(0.0);\n",
    );
    for v in out {
        let _ = writeln!(s, "    {} = {}(0.0);", v.name, glsl_type(v.width));
    }
    if defines(src, "MAIN") {
        s.push_str("    MAIN();\n");
    }
    if !mentions(src, "POSITION") {
        s.push_str("    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);\n");
    }
    s.push_str(
        "    LjosVertex ljosout;\n\
         \x20   ljosout.ljosclip = POSITION;\n\
         \x20   ljosout.ljosworld = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;\n\
         \x20   ljosout.ljosnormal = NORMAL_MATRIX * NORMAL;\n\
         \x20   ljosout.ljosuvs = vec4(UV0, UV1);\n\
         \x20   ljosout.ljostangent = (MODEL_MATRIX * vec4(TANGENT, 0.0)).xyz;\n\
         \x20   ljosout.ljosbinormal = (MODEL_MATRIX * vec4(BINORMAL, 0.0)).xyz;\n\
         \x20   ljosout.ljoscolor = COLOR;\n",
    );
    for (i, v) in out.iter().enumerate() {
        let _ = writeln!(s, "    ljosout.ljosvary{} = {};", letters(i), pack(v));
    }
    s.push_str("    return ljosout;\n}\nvoid main() {}\n");
    (s, before)
}

fn glsl_type(width: usize) -> &'static str {
    match width {
        1 => "float",
        2 => "vec2",
        3 => "vec3",
        _ => "vec4",
    }
}

fn mentions(src: &str, word: &str) -> bool {
    src.split(is_break).any(|w| w == word)
}

const FRAGMENT_KEYWORDS: &str = "\
vec4 BASE_COLOR; vec3 EMISSIVE_COLOR; float METALNESS; float ROUGHNESS; float SPECULAR_AMOUNT;\n\
float OCCLUSION_AMOUNT; float IOR; float FRESNEL_POWER; float FRESNEL_SCALE; float FRESNEL_BIAS;\n\
float TRANSMISSION_FACTOR; float THICKNESS_FACTOR; vec3 ATTENUATION_COLOR; float ATTENUATION_DISTANCE;\n\
float CLEARCOAT_AMOUNT; float CLEARCOAT_ROUGHNESS; vec3 CLEARCOAT_NORMAL; float CLEARCOAT_FRESNEL_POWER;\n\
float CLEARCOAT_FRESNEL_SCALE; float CLEARCOAT_FRESNEL_BIAS;\n\
vec3 NORMAL; vec3 TANGENT; vec3 BINORMAL; vec2 UV0; vec2 UV1;\n\
vec3 VAR_WORLD_NORMAL; vec3 VAR_WORLD_TANGENT; vec3 VAR_WORLD_BINORMAL; vec3 VAR_WORLD_POSITION;\n\
vec4 VAR_COLOR; vec3 VIEW_VECTOR; vec4 FRAGCOORD;\n\
vec3 DIFFUSE; vec3 SPECULAR; vec3 EMISSIVE; vec3 LIGHT_COLOR; float LIGHT_ATTENUATION;\n\
float SPOT_FACTOR; float SHADOW_CONTRIB; vec3 TO_LIGHT_DIR; vec3 TOTAL_AMBIENT_COLOR;\n\
vec3 FRESNEL_CONTRIB; float AO_FACTOR; vec4 COLOR_SUM; vec4 FRAGCOLOR;\n";

fn material_fragment(
    header: &str,
    user: &str,
    src: &str,
    ins: &[Varying],
    shaded: bool,
) -> (String, u32) {
    let f = Functions::of(src);
    let mut s = String::from(header);
    s.push_str(FRAGMENT_KEYWORDS);
    let before = lines(&s);
    s.push_str(user);
    s.push_str(
        "\nstruct LjosSurface {\n\
         \x20   vec4 ljosbase; vec3 ljosemissive; vec3 ljosnormal; vec3 ljosview;\n\
         \x20   float ljosmetalness; float ljosroughness; float ljosspecular; float ljosocclusion;\n\
         };\n\
         LjosSurface ljossurface(vec3 ljosnormalin, vec3 ljosworld, vec4 ljosuvs, vec3 ljostangent,\n\
         \x20   vec3 ljosbinormal, vec4 ljoscolor, vec4 ljosfragcoord, bool ljosfront",
    );
    for i in 0..ins.len() {
        let _ = write!(s, ", vec4 ljosvary{}", letters(i));
    }
    s.push_str(
        ") {\n\
         \x20   float ljosside = ljosfront ? 1.0 : -1.0;\n\
         \x20   VAR_WORLD_NORMAL = ljosnormalin; VAR_WORLD_TANGENT = ljostangent;\n\
         \x20   VAR_WORLD_BINORMAL = ljosbinormal; VAR_WORLD_POSITION = ljosworld; VAR_COLOR = ljoscolor;\n\
         \x20   NORMAL = normalize(ljosnormalin) * ljosside; CLEARCOAT_NORMAL = NORMAL;\n\
         \x20   TANGENT = ljostangent * ljosside; BINORMAL = ljosbinormal * ljosside;\n\
         \x20   UV0 = ljosuvs.xy; UV1 = ljosuvs.zw; FRAGCOORD = ljosfragcoord;\n\
         \x20   VIEW_VECTOR = ljoscamera.w > 0.5 ? -ljosdirection.xyz\n\
         \x20       : normalize(ljoscamera.xyz - ljosworld);\n\
         \x20   BASE_COLOR = vec4(1.0); EMISSIVE_COLOR = vec3(0.0); METALNESS = 0.0; ROUGHNESS = 0.0;\n\
         \x20   SPECULAR_AMOUNT = 0.5; OCCLUSION_AMOUNT = 1.0; IOR = 1.5;\n\
         \x20   FRESNEL_POWER = 5.0; FRESNEL_SCALE = 1.0; FRESNEL_BIAS = 0.0;\n\
         \x20   TRANSMISSION_FACTOR = 0.0; THICKNESS_FACTOR = 0.0; ATTENUATION_COLOR = vec3(1.0);\n\
         \x20   ATTENUATION_DISTANCE = 0.0; CLEARCOAT_AMOUNT = 0.0; CLEARCOAT_ROUGHNESS = 0.0;\n\
         \x20   CLEARCOAT_FRESNEL_POWER = 5.0; CLEARCOAT_FRESNEL_SCALE = 1.0; CLEARCOAT_FRESNEL_BIAS = 0.0;\n\
         \x20   FRAGCOLOR = vec4(1.0); AO_FACTOR = 1.0;\n",
    );
    for (i, v) in ins.iter().enumerate() {
        s.push_str(&unpack(v, &format!("ljosvary{}", letters(i))));
    }
    if f.main {
        s.push_str("    MAIN();\n");
    }
    s.push_str(
        "    LjosSurface ljosout;\n\
         \x20   ljosout.ljosbase = BASE_COLOR; ljosout.ljosemissive = EMISSIVE_COLOR;\n\
         \x20   ljosout.ljosnormal = normalize(NORMAL); ljosout.ljosview = VIEW_VECTOR;\n\
         \x20   ljosout.ljosmetalness = METALNESS; ljosout.ljosroughness = ROUGHNESS;\n\
         \x20   ljosout.ljosspecular = SPECULAR_AMOUNT; ljosout.ljosocclusion = OCCLUSION_AMOUNT;\n\
         \x20   return ljosout;\n\
         }\n\
         vec4 ljosfragcolor() { return FRAGCOLOR; }\n",
    );
    if shaded {
        s.push_str(
            "void ljosbegin(vec3 ljosambient) {\n\
             \x20   DIFFUSE = vec3(0.0); SPECULAR = vec3(0.0); TOTAL_AMBIENT_COLOR = ljosambient;\n\
             }\n\
             void ljosadd(vec3 ljosdiffuse, vec3 ljosspecular) {\n\
             \x20   DIFFUSE += ljosdiffuse; SPECULAR += ljosspecular;\n\
             }\n\
             void ljoslight(vec3 ljoscolor, vec3 ljosdir, float ljosattenuation, float ljosspot,\n\
             \x20   float ljosshadow, vec3 ljosfresnel) {\n\
             \x20   LIGHT_COLOR = ljoscolor; TO_LIGHT_DIR = ljosdir; LIGHT_ATTENUATION = ljosattenuation;\n\
             \x20   SPOT_FACTOR = ljosspot; SHADOW_CONTRIB = ljosshadow; FRESNEL_CONTRIB = ljosfresnel;\n\
             }\n",
        );
        let call = |s: &mut String, name: &str, there: bool, function: &str| {
            let body = if there {
                format!("{function}();")
            } else {
                String::new()
            };
            let _ = writeln!(s, "void {name}() {{ {body} }}");
        };
        call(&mut s, "ljosambientlight", f.ambient, "AMBIENT_LIGHT");
        call(
            &mut s,
            "ljosdirectional",
            f.directional,
            "DIRECTIONAL_LIGHT",
        );
        call(&mut s, "ljospoint", f.point, "POINT_LIGHT");
        call(&mut s, "ljosspot", f.spot, "SPOT_LIGHT");
        call(&mut s, "ljosspecularlight", f.specular, "SPECULAR_LIGHT");
        s.push_str(
            "vec4 ljosfinish() {\n\
             \x20   EMISSIVE = EMISSIVE_COLOR;\n\
             \x20   COLOR_SUM = vec4(DIFFUSE + SPECULAR + EMISSIVE, BASE_COLOR.a);\n",
        );
        if f.post {
            s.push_str("    POST_PROCESS();\n");
        }
        s.push_str("    return COLOR_SUM;\n}\n");
    }
    s.push_str("void main() {}\n");
    (s, before)
}

fn effect_vertex(header: &str, user: &str, src: &str, out: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str("vec3 VERTEX; vec4 POSITION; vec2 INPUT_UV; vec2 TEXTURE_UV;\n");
    let before = lines(&s);
    s.push_str(user);
    s.push_str("\nstruct LjosVertex {\n    vec4 ljosclip; vec4 ljosuvs;\n");
    for i in 0..out.len() {
        let _ = writeln!(s, "    vec4 ljosvary{};", letters(i));
    }
    s.push_str(
        "};\n\
         LjosVertex ljosvertex(vec2 ljosuv) {\n\
         \x20   VERTEX = vec3(ljosuv.x * 2.0 - 1.0, 1.0 - ljosuv.y * 2.0, 0.0);\n\
         \x20   INPUT_UV = ljosuv; TEXTURE_UV = ljosuv; POSITION = vec4(0.0);\n",
    );
    for v in out {
        let _ = writeln!(s, "    {} = {}(0.0);", v.name, glsl_type(v.width));
    }
    if defines(src, "MAIN") {
        s.push_str("    MAIN();\n");
    }
    if !mentions(src, "POSITION") {
        s.push_str("    POSITION = vec4(VERTEX, 1.0);\n");
    }
    s.push_str(
        "    LjosVertex ljosout;\n\
         \x20   ljosout.ljosclip = POSITION; ljosout.ljosuvs = vec4(INPUT_UV, TEXTURE_UV);\n",
    );
    for (i, v) in out.iter().enumerate() {
        let _ = writeln!(s, "    ljosout.ljosvary{} = {};", letters(i), pack(v));
    }
    s.push_str("    return ljosout;\n}\nvoid main() {}\n");
    (s, before)
}

fn effect_fragment(header: &str, user: &str, src: &str, ins: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str("vec2 INPUT_UV; vec2 TEXTURE_UV; vec4 FRAGCOLOR; vec4 FRAGCOORD;\n");
    let before = lines(&s);
    s.push_str(user);
    s.push_str("\nvec4 ljoseffect(vec4 ljosuvs, vec4 ljosfragcoord");
    for i in 0..ins.len() {
        let _ = write!(s, ", vec4 ljosvary{}", letters(i));
    }
    s.push_str(
        ") {\n\
         \x20   INPUT_UV = ljosuvs.xy; TEXTURE_UV = ljosuvs.zw; FRAGCOORD = ljosfragcoord;\n\
         \x20   FRAGCOLOR = vec4(0.0);\n",
    );
    for (i, v) in ins.iter().enumerate() {
        s.push_str(&unpack(v, &format!("ljosvary{}", letters(i))));
    }
    if defines(src, "MAIN") {
        s.push_str("    MAIN();\n");
    }
    s.push_str("    return FRAGCOLOR;\n}\nvoid main() {}\n");
    (s, before)
}

/// One stage's wrapper as WGSL, every name `prefix`ed. `before` is how
/// many lines precede the snippet, so that an error names its own line.
fn stage_module(
    stage: &'static str,
    shader_stage: ShaderStage,
    (source, before): (String, u32),
    prefix: &str,
) -> Result<String, SnippetError> {
    let source = source.as_str();
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(shader_stage);
    let mut module = frontend.parse(&options, source).map_err(|e| {
        let first = e.errors.first();
        let line = first
            .and_then(|e| e.meta.to_range())
            .map(|r| source[..r.start].matches('\n').count() as u32 + 1)
            .unwrap_or(0);
        SnippetError::Compile {
            stage,
            line: line.saturating_sub(before).max(1),
            message: first.map(|e| e.kind.to_string()).unwrap_or_default(),
        }
    })?;
    // The wrapper's `main` is only there for the front end to take it as a
    // whole shader; the program's entry points are the glue's.
    module.entry_points.clear();
    rename(&mut module, prefix);
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| SnippetError::Compile {
        stage,
        line: 1,
        message: e.into_inner().to_string(),
    })?;
    naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
        .map_err(|e| SnippetError::Program(e.to_string()))
}

fn rename(module: &mut Module, prefix: &str) {
    // Unnamed things (the block with no instance name) would be named
    // alike by both stages' writers.
    let named = |name: &mut Option<String>| {
        let n = name.as_deref().unwrap_or("unnamed");
        *name = Some(format!("{prefix}{n}"));
    };
    for (_, f) in module.functions.iter_mut() {
        named(&mut f.name);
    }
    for (_, g) in module.global_variables.iter_mut() {
        named(&mut g.name);
    }
    for (_, c) in module.constants.iter_mut() {
        named(&mut c.name);
    }
    for (_, o) in module.overrides.iter_mut() {
        named(&mut o.name);
    }
    let structs: Vec<_> = module
        .types
        .iter()
        .filter(|(_, t)| t.name.is_some() && matches!(t.inner, naga::TypeInner::Struct { .. }))
        .map(|(h, _)| h)
        .collect();
    for h in structs {
        let mut ty = module.types[h].clone();
        named(&mut ty.name);
        module.types.replace(h, ty);
    }
}

fn validate(wgsl: &str) -> Result<(), SnippetError> {
    let module = naga::front::wgsl::parse_str(wgsl)
        .map_err(|e| SnippetError::Program(e.emit_to_string(wgsl)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| SnippetError::Program(e.emit_to_string(wgsl)))?;
    Ok(())
}

/// The vertex output both material entry points share.
fn material_io(out: &[Varying]) -> String {
    let mut s = String::from(
        "\nstruct CmVsOut {\n\
         \x20   @builtin(position) clip: vec4<f32>,\n\
         \x20   @location(0) normal: vec3<f32>,\n\
         \x20   @location(1) world: vec3<f32>,\n\
         \x20   @location(2) uvs: vec4<f32>,\n\
         \x20   @location(3) tangent: vec3<f32>,\n\
         \x20   @location(4) binormal: vec3<f32>,\n\
         \x20   @location(5) color: vec4<f32>,\n",
    );
    for i in 0..out.len() {
        let _ = writeln!(s, "    @location({}) vary{}: vec4<f32>,", 6 + i, letters(i));
    }
    s.push_str("};\n");
    s
}

/// The fragment stage's varyings, each from the vertex stage's of its
/// name, or zero where the vertex stage has none.
fn varying_args(out: &[Varying], ins: &[Varying]) -> String {
    let mut s = String::new();
    for v in ins {
        match out.iter().position(|o| o.name == v.name) {
            Some(i) => {
                let _ = write!(s, ", in.vary{}", letters(i));
            }
            None => s.push_str(", vec4<f32>(0.0)"),
        }
    }
    s
}

fn material_glue(out: &[Varying], ins: &[Varying], shaded: bool, f: &Functions) -> String {
    let mut s = material_io(out);
    let _ = write!(
        s,
        "@vertex\n\
         fn {MATERIAL_VERTEX}(\n\
         \x20   @location(0) position: vec3<f32>,\n\
         \x20   @location(1) normal: vec3<f32>,\n\
         \x20   @location(2) uv: vec2<f32>,\n\
         \x20   @location(3) tangent: vec4<f32>,\n\
         ) -> CmVsOut {{\n\
         \x20   let v = cmv_ljosvertex(position, normal, uv, tangent);\n\
         \x20   var out: CmVsOut;\n\
         \x20   out.clip = v.ljosclip;\n\
         \x20   out.normal = v.ljosnormal;\n\
         \x20   out.world = v.ljosworld;\n\
         \x20   out.uvs = v.ljosuvs;\n\
         \x20   out.tangent = v.ljostangent;\n\
         \x20   out.binormal = v.ljosbinormal;\n\
         \x20   out.color = v.ljoscolor;\n"
    );
    for i in 0..out.len() {
        let l = letters(i);
        let _ = writeln!(s, "    out.vary{l} = v.ljosvary{l};");
    }
    let _ = write!(
        s,
        "    return out;\n\
         }}\n\
         fn cm_surface(in: CmVsOut, front: bool) -> cmf_LjosSurface {{\n\
         \x20   return cmf_ljossurface(in.normal, in.world, in.uvs, in.tangent, in.binormal, in.color,\n\
         \x20       in.clip, front{});\n\
         }}\n",
        varying_args(out, ins)
    );
    if shaded {
        s.push_str(&shaded_glue(f));
    } else {
        // Written as it is. A scene drawn for post-processing holds linear
        // light, which the last pass encodes, so FRAGCOLOR is decoded to
        // come out of it as written.
        s.push_str(
            "fn cm_shade(in: CmVsOut, front: bool) -> vec4<f32> {\n\
             \x20   _ = cm_surface(in, front);\n\
             \x20   let c = cmf_ljosfragcolor();\n\
             \x20   if u32(frame.tone.x + 0.5) == 5u {\n\
             \x20       return vec4<f32>(cm_linear(c.rgb), c.a);\n\
             \x20   }\n\
             \x20   return c;\n\
             }\n\
             fn cm_linear(c: vec3<f32>) -> vec3<f32> {\n\
             \x20   let k = max(c, vec3<f32>(0.0));\n\
             \x20   return select(pow((k + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), k / 12.92,\n\
             \x20       k <= vec3<f32>(0.04045));\n\
             }\n\
             fn cm_out(c: vec4<f32>) -> vec3<f32> {\n\
             \x20   return c.rgb;\n\
             }\n",
        );
    }
    let _ = write!(
        s,
        "@fragment\n\
         fn {MATERIAL_FRAGMENT}(in: CmVsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {{\n\
         \x20   let c = cm_shade(in, front);\n\
         \x20   let a = clamp(c.a, 0.0, 1.0);\n\
         \x20   return vec4<f32>(cm_out(c) * a, a);\n\
         }}\n\
         @fragment\n\
         fn {MATERIAL_FRAGMENT_STRAIGHT}(in: CmVsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {{\n\
         \x20   let c = cm_shade(in, front);\n\
         \x20   return vec4<f32>(cm_out(c), clamp(c.a, 0.0, 1.0));\n\
         }}\n"
    );
    s
}

/// The light loop of lit.wgsl, with the author's processors called in
/// place of the built-in terms they replace. Nothing the snippet runs is
/// inside a branch on the pixel, so that it may sample textures.
fn shaded_glue(f: &Functions) -> String {
    let builtin_diffuse = "cmf_ljosadd((vec3<f32>(1.0) - fresnel) * (1.0 - metal) * base * radiance * max(nl, 0.0), vec3<f32>(0.0));";
    let pick = |there: bool, call: &str| -> String {
        if there {
            format!(
                "cmf_ljoslight(light.color.rgb, l, attenuation, spot, shadow, fresnel);\n            {call}();"
            )
        } else {
            builtin_diffuse.to_owned()
        }
    };
    let ambient = if f.ambient {
        "cmf_ljosambientlight();"
    } else {
        "cmf_ljosadd(base * frame.ambient.rgb * s.ljosocclusion, vec3<f32>(0.0));"
    };
    let specular = if f.specular {
        "cmf_ljoslight(light.color.rgb, l, attenuation * spot, spot, shadow, fresnel);\n        cmf_ljosspecularlight();"
    } else {
        "if nl > 0.0 {\n\
         \x20           let spec = ggx(max(dot(n, h), 0.0), squared(roughness)) * smith(nl, nv, roughness)\n\
         \x20               / (4.0 * nl * nv);\n\
         \x20           cmf_ljosadd(vec3<f32>(0.0), PI * spec * fresnel * radiance * nl);\n\
         \x20       }"
    };
    format!(
        "fn cm_shade(in: CmVsOut, front: bool) -> vec4<f32> {{\n\
         \x20   let s = cm_surface(in, front);\n\
         \x20   let base = s.ljosbase.rgb;\n\
         \x20   let n = s.ljosnormal;\n\
         \x20   let v = s.ljosview;\n\
         \x20   let nv = max(dot(n, v), 1e-4);\n\
         \x20   let metal = clamp(s.ljosmetalness, 0.0, 1.0);\n\
         \x20   let roughness = clamp(s.ljosroughness, 0.0, 1.0);\n\
         \x20   let f0 = mix(vec3<f32>(0.08 * s.ljosspecular), base, metal);\n\
         \x20   cmf_ljosbegin(frame.ambient.rgb);\n\
         \x20   {ambient}\n\
         \x20   for (var i = 0u; i < min(frame.counts.x, MAX_LIGHTS); i++) {{\n\
         \x20       let light = frame.lights[i];\n\
         \x20       var l = -light.direction.xyz;\n\
         \x20       var attenuation = 1.0;\n\
         \x20       var spot = 1.0;\n\
         \x20       if light.position.w > 0.5 {{\n\
         \x20           let to_light = light.position.xyz - in.world;\n\
         \x20           let d = length(to_light);\n\
         \x20           l = to_light / max(d, 1e-6);\n\
         \x20           attenuation = 1.0 / max(light.fade.x + light.fade.y * d + light.fade.z * d * d, 1e-6);\n\
         \x20           if light.position.w > 1.5 {{\n\
         \x20               let c = dot(-l, light.direction.xyz);\n\
         \x20               let outer = light.direction.w;\n\
         \x20               let inner = light.color.w;\n\
         \x20               spot = select(select(1.0, 0.0, c < outer), smoothstep(outer, inner, c), inner > outer);\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20       let nl = dot(n, l);\n\
         \x20       let shadow = shadowed(light, in.world, l);\n\
         \x20       let h = normalize(l + v);\n\
         \x20       let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(v, h), 0.0), 5.0);\n\
         \x20       let radiance = light.color.rgb * attenuation * spot * shadow;\n\
         \x20       if light.position.w < 0.5 {{\n\
         \x20           {directional}\n\
         \x20       }} else if light.position.w < 1.5 {{\n\
         \x20           {point}\n\
         \x20       }} else {{\n\
         \x20           {spot_light}\n\
         \x20       }}\n\
         \x20       {specular}\n\
         \x20   }}\n\
         \x20   if frame.env.w > 0.5 {{\n\
         \x20       let horizon = frame.env.y;\n\
         \x20       let irradiance = textureSampleLevel(irradiance_cube, cube_sampler, probe_dir(n), 0.0).rgb\n\
         \x20           * (1.0 - horizon * (1.0 - n.y) * 0.5);\n\
         \x20       let r = reflect(-v, n);\n\
         \x20       let below = mix(select(0.0, 1.0, r.y < 0.0), (1.0 - r.y) * 0.5, roughness);\n\
         \x20       let prefiltered = textureSampleLevel(specular_cube, cube_sampler, probe_dir(r),\n\
         \x20           roughness * frame.env.z).rgb * (1.0 - horizon * below);\n\
         \x20       let specular = env_brdf(f0, roughness, nv);\n\
         \x20       let kd = (vec3<f32>(1.0) - specular) * (1.0 - metal);\n\
         \x20       let k = s.ljosocclusion * frame.env.x;\n\
         \x20       cmf_ljosadd(kd * base * irradiance * k, prefiltered * specular * k);\n\
         \x20   }}\n\
         \x20   return cmf_ljosfinish();\n\
         }}\n\
         fn cm_out(c: vec4<f32>) -> vec3<f32> {{\n\
         \x20   return tonemap(c.rgb, frame.tone);\n\
         }}\n",
        directional = pick(f.directional, "cmf_ljosdirectional"),
        point = pick(f.point, "cmf_ljospoint"),
        spot_light = pick(f.spot, "cmf_ljosspot"),
    )
}

fn effect_glue(out: &[Varying], ins: &[Varying]) -> String {
    let mut s = String::from(
        "\nstruct CmEffectOut {\n\
         \x20   @builtin(position) clip: vec4<f32>,\n\
         \x20   @location(0) uvs: vec4<f32>,\n",
    );
    for i in 0..out.len() {
        let _ = writeln!(s, "    @location({}) vary{}: vec4<f32>,", 1 + i, letters(i));
    }
    // One triangle over the output, as blit.wgsl draws.
    let _ = write!(
        s,
        "}};\n\
         @vertex\n\
         fn {EFFECT_VERTEX}(@builtin(vertex_index) i: u32) -> CmEffectOut {{\n\
         \x20   let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));\n\
         \x20   let v = cmv_ljosvertex(uv);\n\
         \x20   var out: CmEffectOut;\n\
         \x20   out.clip = v.ljosclip;\n\
         \x20   out.uvs = v.ljosuvs;\n"
    );
    for i in 0..out.len() {
        let l = letters(i);
        let _ = writeln!(s, "    out.vary{l} = v.ljosvary{l};");
    }
    let _ = write!(
        s,
        "    return out;\n\
         }}\n\
         @fragment\n\
         fn {EFFECT_FRAGMENT}(in: CmEffectOut) -> @location(0) vec4<f32> {{\n\
         \x20   return cmf_ljoseffect(in.uvs, in.clip{});\n\
         }}\n",
        varying_args(out, ins)
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniforms(list: &[(&str, UniformKind)]) -> Vec<(String, UniformKind)> {
        list.iter().map(|(n, k)| ((*n).to_owned(), *k)).collect()
    }

    #[test]
    fn an_unshaded_material_with_properties_and_a_texture_translates() {
        let p = material(
            Some("void MAIN() { VERTEX.x += offset; POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0); }"),
            Some("void MAIN() { FRAGCOLOR = vec4(tint.rgb * level, 1.0) * texture(picture, UV0); }"),
            false,
            &uniforms(&[
                ("tint", UniformKind::Vec4),
                ("level", UniformKind::Float),
                ("offset", UniformKind::Float),
            ]),
            &["picture".to_owned()],
        )
        .unwrap();
        assert_eq!(p.kind(), ProgramKind::Unshaded);
        assert!(p.wgsl().contains("fn cm_fs("));
    }

    #[test]
    fn a_shaded_material_calls_its_light_processors() {
        let p = material(
            None,
            Some(
                "void MAIN() { BASE_COLOR = vec4(0.5); }\n\
                 void DIRECTIONAL_LIGHT() { DIFFUSE += LIGHT_COLOR * 0.25; }\n\
                 void SPECULAR_LIGHT() { }\n\
                 void POST_PROCESS() { COLOR_SUM = vec4(DIFFUSE, 1.0); }",
            ),
            true,
            &[],
            &[],
        )
        .unwrap();
        assert!(p.wgsl().contains("cmf_DIRECTIONAL_LIGHT"));
        assert!(p.wgsl().contains("cmf_POST_PROCESS"));
    }

    #[test]
    fn varyings_pass_from_the_vertex_snippet_to_the_fragment_snippet() {
        material(
            Some("VARYING vec3 pos;\nVARYING float k;\nvoid MAIN() { pos = VERTEX; k = 1.0; }"),
            Some("VARYING vec3 pos;\nVARYING float k;\nvoid MAIN() { FRAGCOLOR = vec4(pos * k, 1.0); }"),
            false,
            &[],
            &[],
        )
        .unwrap();
    }

    #[test]
    fn a_broken_snippet_names_its_own_line() {
        let e = material(
            None,
            Some("void MAIN()\n{\n    FRAGCOLOR = vec4(1.0;\n}\n"),
            false,
            &[],
            &[],
        )
        .unwrap_err();
        match e {
            SnippetError::Compile { stage, line, .. } => {
                assert_eq!(stage, "fragment");
                assert_eq!(line, 3, "{e}");
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn an_effect_reads_input_and_depth_with_a_uniform() {
        let p = effect(
            None,
            "void MAIN() {\n\
             \x20   float d = texture(DEPTH_TEXTURE, INPUT_UV).r;\n\
             \x20   FRAGCOLOR = texture(INPUT, INPUT_UV) * factor * d;\n\
             }",
            &uniforms(&[("factor", UniformKind::Float)]),
            &[],
        )
        .unwrap();
        assert!(p.reads_depth());
        assert_eq!(p.kind(), ProgramKind::Effect);
    }

    #[test]
    fn every_kind_of_property_is_read() {
        material(
            None,
            Some(
                "void MAIN() {\n\
                 \x20   if (on && count > 1) { BASE_COLOR = m * vec4(a, b, 1.0); }\n\
                 }",
            ),
            true,
            &uniforms(&[
                ("on", UniformKind::Bool),
                ("count", UniformKind::Int),
                ("a", UniformKind::Vec2),
                ("b", UniformKind::Float),
                ("m", UniformKind::Mat4),
            ]),
            &[],
        )
        .unwrap();
    }

    #[test]
    fn the_programs_translate_to_webgl2() {
        let p = material(
            Some("VARYING vec2 t;\nvoid MAIN() { t = UV0; }"),
            Some("VARYING vec2 t;\nvoid MAIN() { BASE_COLOR = texture(picture, t); }\nvoid POINT_LIGHT() { DIFFUSE += LIGHT_COLOR * LIGHT_ATTENUATION; }"),
            true,
            &[],
            &["picture".to_owned()],
        )
        .unwrap();
        let e = effect(
            None,
            "void MAIN() { FRAGCOLOR = texture(INPUT, INPUT_UV); }",
            &[],
            &[],
        )
        .unwrap();
        for (wgsl, entries) in [
            (p.wgsl(), [MATERIAL_VERTEX, MATERIAL_FRAGMENT]),
            (e.wgsl(), [EFFECT_VERTEX, EFFECT_FRAGMENT]),
        ] {
            let module = naga::front::wgsl::parse_str(wgsl).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            for (entry, stage) in entries
                .into_iter()
                .zip([ShaderStage::Vertex, ShaderStage::Fragment])
            {
                let options = naga::back::glsl::Options {
                    version: naga::back::glsl::Version::new_gles(300),
                    ..Default::default()
                };
                let pipeline = naga::back::glsl::PipelineOptions {
                    shader_stage: stage,
                    entry_point: entry.to_owned(),
                    multiview: None,
                };
                let mut out = String::new();
                naga::back::glsl::Writer::new(
                    &mut out,
                    &module,
                    &info,
                    &options,
                    &pipeline,
                    naga::proc::BoundsCheckPolicies::default(),
                )
                .and_then(|mut w| w.write())
                .unwrap_or_else(|e| panic!("{entry}: {e}"));
            }
        }
    }

    #[test]
    fn the_block_is_laid_out_where_the_renderer_writes() {
        let kinds = [
            UniformKind::Float,
            UniformKind::Vec3,
            UniformKind::Bool,
            UniformKind::Vec2,
            UniformKind::Mat4,
            UniformKind::Int,
        ];
        let list: Vec<_> = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| (format!("u{}", letters(i)), *k))
            .collect();
        let p = effect(None, "void MAIN() { FRAGCOLOR = vec4(ua); }", &list, &[]).unwrap();
        let module = naga::front::wgsl::parse_str(p.wgsl()).unwrap();
        let (_, block) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("cmf_LjosCustom"))
            .unwrap();
        let naga::TypeInner::Struct { members, span } = &block.inner else {
            panic!("not a struct");
        };
        // After the header's six matrices and four rows.
        let offsets: Vec<usize> = members[10..]
            .iter()
            .map(|m| m.offset as usize - HEADER_ROWS * 16)
            .collect();
        assert_eq!(offsets, p.offsets());
        assert!(*span as usize <= p.block_size());
    }

    #[test]
    fn letters_never_end_in_a_digit() {
        assert_eq!(letters(0), "a");
        assert_eq!(letters(25), "z");
        assert_eq!(letters(26), "aa");
    }
}
