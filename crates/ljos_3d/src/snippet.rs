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
    EFFECT_FRAGMENT, EFFECT_VERTEX, HEADER_ROWS, MATERIAL_FRAGMENT, MATERIAL_VERTEX,
    MATERIAL_VERTEX_EXTRA, Program, ProgramKind, Screen, UniformKind,
};

/// At most this many locations of VARYINGs, packed four numbers to each:
/// WebGPU has 16 for what goes between its stages, of which a surface takes 5
/// and which side it faces 1.
pub const MAX_VARYINGS: usize = 10;
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
    declared: &[String],
) -> Result<Program, SnippetError> {
    // A material may declare a texture for every map it could use and name
    // only those it has: the others take no place, and the limit is on what
    // a shader samples. The program says which it kept.
    let textures: Vec<String> = declared
        .iter()
        .filter(|t| {
            [vertex, fragment]
                .into_iter()
                .flatten()
                .any(|src| mentions(src, t))
        })
        .cloned()
        .collect();
    let textures = textures.as_slice();
    check_names(uniforms, textures)?;
    let vertex = vertex.map(by_definition);
    let vertex = vertex.as_deref();
    let fragment = fragment.map(by_definition);
    let fragment = fragment.as_deref();
    let vertex_src = vertex.map(strip_comments).unwrap_or_default();
    let fragment_src = fragment.map(strip_comments).unwrap_or_default();
    let out = varyings(&vertex_src, "vertex")?;
    let ins = varyings(&fragment_src, "fragment")?;
    let names = |w: &str| {
        fragment_src
            .split(is_break)
            .chain(vertex_src.split(is_break))
            .any(|n| n == w)
    };
    let screen = if names("SCREEN_MIP_TEXTURE") {
        Screen::Mipmapped
    } else if names("SCREEN_TEXTURE") {
        Screen::Texture
    } else {
        Screen::None
    };

    let reads_depth = names("DEPTH_TEXTURE");

    let set = 2;
    let mut header = header(set, uniforms, textures, &[], reads_depth);
    header.push_str(&min_and_max(&[&vertex_src, &fragment_src]));
    if screen != Screen::None {
        // One picture serves both names; only the mip levels differ, and
        // those are made when either asks for them.
        let _ = writeln!(
            header,
            "layout(set = {set}, binding = 28) uniform texture2D ljosscreen;\n\
             layout(set = {set}, binding = 29) uniform sampler ljosscreensampler;\n\
             #define SCREEN_TEXTURE sampler2D(ljosscreen, ljosscreensampler)\n\
             #define SCREEN_MIP_TEXTURE sampler2D(ljosscreen, ljosscreensampler)"
        );
    }
    let vs = stage_module(
        "vertex",
        ShaderStage::Vertex,
        material_vertex(&header, vertex.unwrap_or(""), &vertex_src, &out),
        "cmv_",
    )?;
    let fs = stage_module(
        "fragment",
        ShaderStage::Fragment,
        material_fragment(
            &header,
            &with_final_diffuse(fragment.unwrap_or("")),
            &fragment_src,
            &ins,
            shaded,
        ),
        "cmf_",
    )?;
    let functions = Functions::of(&fragment_src);
    let mut wgsl = String::from(LIT);
    wgsl.push_str(&vs);
    wgsl.push_str(&the_scenes_probe(fs));
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
        reads_depth,
    )
    .reading_screen(screen))
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
    let vertex = vertex.map(by_definition);
    let vertex = vertex.as_deref();
    let fragment = by_definition(fragment);
    let fragment = fragment.as_str();
    let vertex_src = vertex.map(strip_comments).unwrap_or_default();
    let fragment_src = strip_comments(fragment);
    let out = varyings(&vertex_src, "vertex")?;
    let ins = varyings(&fragment_src, "fragment")?;
    let depth = fragment_src
        .split(is_break)
        .chain(vertex_src.split(is_break))
        .any(|w| w == "DEPTH_TEXTURE");

    let set = 0;
    let mut header = header(set, uniforms, textures, &["INPUT"], depth);
    header.push_str(&min_and_max(&[&vertex_src, &fragment_src]));
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

/// What is in DIFFUSE to `POST_PROCESS` has the alpha as well, where to the
/// light functions it has none. Each of these lines goes in for one function.
fn with_final_diffuse(src: &str) -> String {
    let Some(at) = src.find("POST_PROCESS") else {
        return src.to_string();
    };
    let Some(start) = src[..at].rfind("void") else {
        return src.to_string();
    };
    let Some(open) = src[at..].find('{').map(|i| at + i) else {
        return src.to_string();
    };
    let mut deep = 0;
    let mut end = src.len();
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => deep += 1,
            '}' => {
                deep -= 1;
                if deep == 0 {
                    end = open + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    format!(
        "{}\n#define DIFFUSE ljosfinaldiffuse\n{}\n#undef DIFFUSE\n{}",
        &src[..start],
        &src[start..end],
        &src[end..]
    )
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
    probe: bool,
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
            probe: defines(src, "IBL_PROBE"),
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
            // A matrix goes as its columns, a location each, and the
            // wrappers read and write `name[i]` as they do any vector.
            "mat2" | "mat3" | "mat4" => {
                let columns = if ty == "mat2" {
                    2
                } else if ty == "mat3" {
                    3
                } else {
                    4
                };
                out.extend((0..columns).map(|c| Varying {
                    name: format!("{name}[{c}]"),
                    width: columns,
                }));
                continue;
            }
            _ => {
                return Err(SnippetError::Unsupported {
                    stage,
                    message: format!("VARYING {ty} {name}: only float, vec and mat types pass"),
                });
            }
        };
        out.push(Varying {
            name: name.to_owned(),
            width,
        });
    }
    if locations(&out) > MAX_VARYINGS {
        return Err(SnippetError::Unsupported {
            stage,
            message: format!("VARYINGs of more than {} numbers", MAX_VARYINGS * 4),
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

/// How many locations `out` takes, in the order `pack_writes` fills them.
fn locations(out: &[Varying]) -> usize {
    out.iter().map(|v| v.width).sum::<usize>().div_ceil(4)
}

const COMPONENTS: [char; 4] = ['x', 'y', 'z', 'w'];

/// Where the `at`th number of the VARYINGs goes: a location's name and a
/// component of it.
fn slot(at: usize) -> (String, char) {
    (letters(at / 4), COMPONENTS[at % 4])
}

/// The vertex stage's writes: every number of every VARYING, one after the
/// other, a vector across two locations if that is where it falls. A
/// location each would hold fewer than WebGL2 lets through.
fn pack_writes(out: &[Varying]) -> String {
    let mut s = String::from("    vec4 ljospacked;\n");
    let mut at = 0;
    for v in out {
        let _ = writeln!(s, "    ljospacked = {};", pack(v));
        for (c, packed) in COMPONENTS.iter().enumerate().take(v.width) {
            let (location, component) = slot(at + c);
            let _ = writeln!(
                s,
                "    ljosout.ljosvary{location}.{component} = ljospacked.{packed};"
            );
        }
        at += v.width;
    }
    s
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
         #define VARYING\n\
         #define texture2D(ljoss, ljosuv) texture(ljoss, ljosuv)\n\
         vec3 qt_sRGBToLinear(vec3 ljoss) {{\n\
         \x20   vec3 ljosk = max(ljoss, vec3(0.0));\n\
         \x20   return mix(pow((ljosk + vec3(0.055)) / 1.055, vec3(2.4)), ljosk / 12.92,\n\
         \x20       lessThanEqual(ljosk, vec3(0.04045)));\n\
         }}\n\
         vec4 qt_sRGBToLinear(vec4 ljoss) {{ return vec4(qt_sRGBToLinear(ljoss.rgb), ljoss.a); }}\n"
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

/// The author's code, then `ljosglobals`, which gives its globals their values.
fn push_user(s: &mut String, user: &str) {
    let (code, late) = late_globals(&strip_comments(user));
    s.push_str(&code);
    let _ = write!(s, "\nvoid ljosglobals() {{ {late} }}\n");
}

/// A snippet's globals with their values taken out, and the assignments that
/// give them. A global made of a property is not a constant, and the front end
/// would start it at 0 without a word; the wrapper runs these before `MAIN`.
/// Lines stay where they are, for errors. What is declared in a preprocessor
/// condition stays too, as an assignment would not be under that condition, and
/// so does a declaration of several.
fn late_globals(src: &str) -> (String, String) {
    let mut code = String::with_capacity(src.len());
    let mut late = String::new();
    let (mut braces, mut conditions) = (0usize, 0usize);
    let mut start = 0;
    let mut at_line_start = true;
    let mut chars = src.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '#' if at_line_start && braces == 0 => {
                let end = src[i..].find('\n').map_or(src.len(), |e| i + e);
                let word = src[i + 1..end].trim_start();
                if word.starts_with("if") {
                    conditions += 1;
                } else if word.starts_with("endif") {
                    conditions = conditions.saturating_sub(1);
                }
                code.push_str(&src[start..end]);
                start = end;
                while chars.next_if(|&(at, _)| at < end).is_some() {}
                continue;
            }
            '{' => braces += 1,
            '}' => {
                braces = braces.saturating_sub(1);
                if braces == 0 {
                    code.push_str(&src[start..=i]);
                    start = i + 1;
                }
            }
            ';' if braces == 0 => {
                let statement = &src[start..i];
                match (conditions == 0).then(|| split_global(statement)).flatten() {
                    Some((left, name, value)) => {
                        let _ = write!(late, "{name} = {value}; ");
                        code.push_str(left);
                        code.push(';');
                        code.extend(value.chars().filter(|&c| c == '\n'));
                    }
                    None => {
                        code.push_str(statement);
                        code.push(';');
                    }
                }
                start = i + 1;
            }
            _ => {}
        }
        at_line_start = c == '\n' || (at_line_start && c.is_whitespace());
    }
    code.push_str(&src[start..]);
    (code, late.replace('\n', " "))
}

/// A statement outside any function as (what declares the global, its name,
/// its value), if it is a global given a value that may be moved.
fn split_global(statement: &str) -> Option<(&str, &str, &str)> {
    let mut depth = 0usize;
    let mut equals = None;
    let bytes = statement.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return None,
            b'=' if depth == 0 && equals.is_none() => equals = Some(i),
            _ => {}
        }
    }
    let (left, value) = (&statement[..equals?], &statement[equals? + 1..]);
    let mut words = left.split(is_break).filter(|w| !w.is_empty());
    if words.clone().count() < 2
        || words.any(|w| matches!(w, "const" | "in" | "out" | "uniform" | "layout" | "shared"))
    {
        return None;
    }
    let name = left
        .split('[')
        .next()?
        .split(is_break)
        .rfind(|w| !w.is_empty())?;
    Some((left, name, value))
}

/// A fragment stage's probe as the one lit.wgsl declares: two names for one
/// binding would not validate.
fn the_scenes_probe(mut wgsl: String) -> String {
    while let Some(start) = wgsl.find("@group(3)") {
        let end = wgsl[start..]
            .find(';')
            .map_or(wgsl.len(), |e| start + e + 1);
        wgsl.replace_range(start..end, "");
    }
    wgsl.replace("cmf_ljosprobetexture", "specular_cube")
        .replace("cmf_ljosprobesampler", "cube_sampler")
}

/// GLSL defines max(x, y) as "y if x < y, otherwise x", and min the other
/// way, which says what a NaN gives. A SPIR-V or GLES maximum leaves that
/// open, and drivers differ: a shader that guards a 0/0 with max(0.0, ...)
/// paints black on one and is right on another. Written out, it is the same
/// on all of them. Only for the names a snippet uses, and only the integer
/// vectors if it has any, since every function here is compiled for it.
/// A call comes here by [`by_definition`].
fn min_and_max(sources: &[&str]) -> String {
    let says = |word: &str| sources.iter().any(|s| mentions(s, word));
    let mut families = vec![("float", "vec"), ("int", "ivec"), ("uint", "uvec")];
    families
        .retain(|(_, vector)| *vector == "vec" || (2..=4).any(|n| says(&format!("{vector}{n}"))));
    let mut out = String::new();
    for (name, chosen) in [("max", "a < b"), ("min", "b < a")] {
        if !says(&format!("ljos{name}")) {
            continue;
        }
        for scalar in ["float", "int", "uint"] {
            let _ = writeln!(
                out,
                "{scalar} ljos{name}({scalar} a, {scalar} b) {{ return {chosen} ? b : a; }}"
            );
        }
        for (scalar, vector) in &families {
            for n in 2..=4 {
                let parts = ["x", "y", "z", "w"][..n]
                    .iter()
                    .map(|c| format!("ljos{name}(a.{c}, b.{c})"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let v = format!("{vector}{n}");
                let _ = writeln!(
                    out,
                    "{v} ljos{name}({v} a, {v} b) {{ return {v}({parts}); }}"
                );
                let _ = writeln!(
                    out,
                    "{v} ljos{name}({v} a, {scalar} b) {{ return ljos{name}(a, {v}(b)); }}"
                );
            }
        }
    }
    out
}

/// A snippet's calls of max and min, sent to [`min_and_max`]'s functions.
/// Not where GLSL wants a constant, which no function call gives: the
/// initializer of a `const` or of a global, and an array's size. There the
/// compiler works out the real one, and with numbers no NaN can come of it.
/// A macro's body is sent there as well, as most are used in a function; one
/// used for a constant is the limit of this.
fn by_definition(src: &str) -> String {
    let b = src.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = String::with_capacity(src.len());
    let (mut i, mut from) = (0, 0);
    let (mut braces, mut brackets) = (0usize, 0usize);
    // Reading a `const`, and past its `=`: a parameter's ends at `,` or `)`.
    let (mut constant, mut assigned, mut directive) = (false, false, false);
    while i < b.len() {
        let rest = &src[i..];
        if rest.starts_with("//") {
            i += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            i += rest.find("*/").map_or(rest.len(), |end| end + 2);
        } else if is_word(b[i]) {
            let start = i;
            while i < b.len() && is_word(b[i]) {
                i += 1;
            }
            let word = &src[start..i];
            constant |= word == "const" && !directive;
            let called = src[i..].trim_start().starts_with('(');
            let member = start > 0 && b[start - 1] == b'.';
            let fixed = !directive && (braces == 0 || constant || brackets > 0);
            if (word == "max" || word == "min") && called && !member && !fixed {
                out.push_str(&src[from..start]);
                out.push_str("ljos");
                from = start;
            }
        } else {
            match b[i] {
                b'\n' => directive = false,
                b'#' => directive = true,
                _ if directive => {}
                b'{' => {
                    braces += 1;
                    (constant, assigned) = (false, false);
                }
                b'}' => braces = braces.saturating_sub(1),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'=' => assigned = constant,
                b';' => (constant, assigned) = (false, false),
                b',' | b')' if !assigned => constant = false,
                _ => {}
            }
            i += 1;
        }
    }
    out.push_str(&src[from..]);
    out
}

fn lines(s: &str) -> u32 {
    s.matches('\n').count() as u32
}

/// The skin's joints, the model's group's second binding: the renderer
/// fills it as lit.wgsl reads it, each joint's place times its inverse
/// bind pose.
const BONES: &str = "\
layout(std140, set = 1, binding = 1) uniform LjosBones { mat4 ljosbones[256]; };\n\
#define BONE_TRANSFORMS ljosbones\n";

fn material_vertex(header: &str, user: &str, src: &str, out: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str(BONES);
    s.push_str(MATERIAL_READS);
    s.push_str(
        "vec3 VERTEX; vec3 NORMAL; vec2 UV0; vec2 UV1; vec3 TANGENT; vec3 BINORMAL;\n\
         vec4 COLOR; vec4 POSITION; ivec4 JOINTS; vec4 WEIGHTS; float POINT_SIZE;\n\
         vec4 INSTANCE_COLOR; vec4 INSTANCE_DATA; int INSTANCE_INDEX; mat4 ljosinstance;\n\
         // There are no derivatives here to choose a level of detail by.
         #define texture(ljoss, ljosuv) textureLod(ljoss, ljosuv, 0.0)\n\
         // Targets are weighed in before the shader runs, so it is handed a mesh with none.
         const int QT_MORPH_MAX_COUNT = 0;\n\
         const float MORPH_WEIGHTS[1] = float[1](0.0);\n\
         vec3 MORPH_POSITION(int ljosi) { return VERTEX; }\n\
         vec3 MORPH_NORMAL(int ljosi) { return NORMAL; }\n\
         vec3 MORPH_TANGENT(int ljosi) { return TANGENT; }\n\
         vec3 MORPH_BINORMAL(int ljosi) { return BINORMAL; }\n\
         #define INSTANCE_MODEL_MATRIX (ljosinstance * ljosmodel)\n\
         #define INSTANCE_MODELVIEWPROJECTION_MATRIX (ljosviewprojection * ljosinstance * ljosmodel)\n\
         mat3 ljoscofactor(mat4 ljosm) {\n\
         \x20   vec3 ljosa = ljosm[0].xyz; vec3 ljosb = ljosm[1].xyz; vec3 ljosc = ljosm[2].xyz;\n\
         \x20   return mat3(cross(ljosb, ljosc), cross(ljosc, ljosa), cross(ljosa, ljosb));\n\
         }\n\
         mat3 ljosbonenormal(int ljosi) {\n\
         \x20   mat4 ljosm = ljosbones[clamp(ljosi, 0, 255)];\n\
         \x20   float ljosdet = dot(ljosm[0].xyz, cross(ljosm[1].xyz, ljosm[2].xyz));\n\
         \x20   return ljoscofactor(ljosm) / (abs(ljosdet) > 1e-12 ? ljosdet : 1.0);\n\
         }\n",
    );
    let before = lines(&s);
    push_user(&mut s, &bone_normals(user));
    s.push_str("\nstruct LjosVertex {\n    vec4 ljosclip; vec3 ljosworld; vec3 ljosnormal; vec4 ljosuvs;\n    vec3 ljostangent; vec3 ljosbinormal; vec4 ljoscolor;\n");
    for i in 0..locations(out) {
        let _ = writeln!(s, "    vec4 ljosvary{};", letters(i));
    }
    // The copy's place goes in front of the model's and the skin behind
    // it, as lit.wgsl puts them; a skinned model's MODEL_MATRIX is the
    // identity, its joints taking it into the scene.
    s.push_str(
        "};\n\
         LjosVertex ljosvertex(vec3 ljosposition, vec3 ljosnormalin, vec2 ljosuv, vec2 ljosuv1,\n\
         \x20   vec4 ljostangentin, mat4 ljoscopy, vec4 ljoscopycolor, vec4 ljoscopydata, int ljoscopyindex,\n\
         \x20   ivec4 ljosjoints, vec4 ljosweights, vec4 ljoscolorin, bool ljosskinned) {\n\
         \x20   VERTEX = ljosposition; NORMAL = ljosnormalin;\n\
         \x20   UV0 = vec2(ljosuv.x, 1.0 - ljosuv.y); UV1 = vec2(ljosuv1.x, 1.0 - ljosuv1.y);\n\
         \x20   TANGENT = ljostangentin.xyz;\n\
         \x20   BINORMAL = cross(ljosnormalin, ljostangentin.xyz) * ljostangentin.w;\n\
         \x20   COLOR = ljoscolorin; POSITION = vec4(0.0);\n\
         \x20   JOINTS = ljosjoints; WEIGHTS = ljosweights;\n\
         \x20   ljosinstance = ljoscopy; INSTANCE_COLOR = ljoscopycolor; INSTANCE_DATA = ljoscopydata;\n\
         \x20   INSTANCE_INDEX = ljoscopyindex;\n\
         \x20   mat4 ljosskin = mat4(1.0);\n\
         \x20   if (ljosskinned) {\n\
         \x20       ivec4 ljosj = clamp(ljosjoints, ivec4(0), ivec4(255));\n\
         \x20       ljosskin = ljosbones[ljosj.x] * ljosweights.x + ljosbones[ljosj.y] * ljosweights.y\n\
         \x20           + ljosbones[ljosj.z] * ljosweights.z + ljosbones[ljosj.w] * ljosweights.w;\n\
         \x20   }\n",
    );
    for v in out {
        let _ = writeln!(s, "    {} = {}(0.0);", v.name, glsl_type(v.width));
    }
    s.push_str("    ljosglobals();\n");
    if defines(src, "MAIN") {
        s.push_str("    MAIN();\n");
    }
    if !mentions(src, "POSITION") {
        s.push_str(
            "    POSITION = INSTANCE_MODELVIEWPROJECTION_MATRIX * ljosskin * vec4(VERTEX, 1.0);\n",
        );
    }
    s.push_str(
        "    mat4 ljosplace = INSTANCE_MODEL_MATRIX * ljosskin;\n\
         \x20   LjosVertex ljosout;\n\
         \x20   ljosout.ljosclip = POSITION;\n\
         \x20   ljosout.ljosworld = (ljosplace * vec4(VERTEX, 1.0)).xyz;\n\
         \x20   ljosout.ljosnormal = normalize(ljoscofactor(ljosinstance) * (NORMAL_MATRIX * (ljoscofactor(ljosskin) * NORMAL)));\n\
         \x20   ljosout.ljosuvs = vec4(UV0, UV1);\n\
         \x20   ljosout.ljostangent = (ljosplace * vec4(TANGENT, 0.0)).xyz;\n\
         \x20   ljosout.ljosbinormal = (ljosplace * vec4(BINORMAL, 0.0)).xyz;\n\
         \x20   ljosout.ljoscolor = COLOR;\n",
    );
    s.push_str(&pack_writes(out));
    s.push_str("    return ljosout;\n}\nvoid main() {}\n");
    (s, before)
}

/// `BONE_NORMAL_TRANSFORMS[i]` as a call: the normal matrices are made from
/// the joints' when read, not kept beside them. The lines stay where they
/// are, so an error still names the author's.
fn bone_normals(user: &str) -> String {
    const WORD: &str = "BONE_NORMAL_TRANSFORMS";
    let mut out = String::with_capacity(user.len());
    let mut rest = user;
    while let Some(at) = rest.find(WORD) {
        let whole = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
            && rest[at + WORD.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        out.push_str(&rest[..at]);
        rest = &rest[at + WORD.len()..];
        if !whole {
            out.push_str(WORD);
            continue;
        }
        let gap = rest.len() - rest.trim_start().len();
        if !rest[gap..].starts_with('[') {
            // Not indexed: left as it is, for the compiler to name.
            out.push_str(WORD);
            continue;
        }
        let mut depth = 0usize;
        let mut close = None;
        for (i, c) in rest.char_indices().skip(gap) {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            out.push_str(WORD);
            continue;
        };
        out.push_str("ljosbonenormal(");
        out.push_str(&rest[..gap]);
        out.push_str(&rest[gap + 1..close]);
        out.push(')');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
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

// A snippet runs in a function the WGSL entry point calls, where naga's
// gl_FragCoord is never written and reads as zero: it is the same place as
// FRAGCOORD, which the entry point hands over.
const FRAGMENT_KEYWORDS: &str = "\
#define gl_FragCoord FRAGCOORD
vec4 BASE_COLOR; vec3 EMISSIVE_COLOR; float METALNESS; float ROUGHNESS; float SPECULAR_AMOUNT;\n\
float OCCLUSION_AMOUNT; float IOR; float FRESNEL_POWER; float FRESNEL_SCALE; float FRESNEL_BIAS;\n\
float TRANSMISSION_FACTOR; float THICKNESS_FACTOR; vec3 ATTENUATION_COLOR; float ATTENUATION_DISTANCE;\n\
float CLEARCOAT_AMOUNT; float CLEARCOAT_ROUGHNESS; vec3 CLEARCOAT_NORMAL; float CLEARCOAT_FRESNEL_POWER;\n\
float CLEARCOAT_FRESNEL_SCALE; float CLEARCOAT_FRESNEL_BIAS;\n\
vec3 NORMAL; vec3 TANGENT; vec3 BINORMAL; vec2 UV0; vec2 UV1;\n\
vec3 VAR_WORLD_NORMAL; vec3 VAR_WORLD_TANGENT; vec3 VAR_WORLD_BINORMAL; vec3 VAR_WORLD_POSITION;\n\
vec4 VAR_COLOR; vec3 VIEW_VECTOR; vec4 FRAGCOORD;\n\
vec3 DIFFUSE; vec4 ljosfinaldiffuse; vec3 SPECULAR; vec3 EMISSIVE; vec3 LIGHT_COLOR; float LIGHT_ATTENUATION;\n\
float SPOT_FACTOR; float SHADOW_CONTRIB; vec3 TO_LIGHT_DIR; vec3 TOTAL_AMBIENT_COLOR;\n\
vec3 FRESNEL_CONTRIB; float AO_FACTOR; vec4 COLOR_SUM; vec4 FRAGCOLOR;\n\
mat3 IBL_ORIENTATION; float IBL_EXPOSE; float IBL_HORIZON; float IBL_MAXMIPMAP;\n\
layout(set = 3, binding = 0) uniform textureCube ljosprobetexture;\n\
layout(set = 3, binding = 2) uniform sampler ljosprobesampler;\n\
#define IBL_TEXTURE samplerCube(ljosprobetexture, ljosprobesampler)\n";

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
    s.push_str(MATERIAL_READS);
    s.push_str(MATERIAL_TEXTURE);
    let before = lines(&s);
    push_user(&mut s, user);
    s.push_str(
        "\nstruct LjosSurface {\n\
         \x20   vec4 ljosbase; vec3 ljosemissive; vec3 ljosnormal; vec3 ljosview;\n\
         \x20   float ljosmetalness; float ljosroughness; float ljosspecular; float ljosocclusion;\n\
         \x20   vec3 ljosedge; vec4 ljoscoat; vec3 ljoscoatedge; vec3 ljoscoatnormal;\n\
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
         \x20   UV0 = ljosuvs.xy; UV1 = ljosuvs.zw;\n\
         \x20   FRAGCOORD = vec4(ljosfragcoord.x, ljossizes.w - ljosfragcoord.y, ljosfragcoord.zw);\n\
         \x20   VIEW_VECTOR = normalize(ljoscamera.xyz - ljosworld);\n\
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
    s.push_str("    ljosglobals();\n");
    if f.main {
        s.push_str("    MAIN();\n");
    }
    s.push_str(
        "    LjosSurface ljosout;\n\
         \x20   ljosout.ljosbase = BASE_COLOR; ljosout.ljosemissive = EMISSIVE_COLOR;\n\
         \x20   ljosout.ljosnormal = normalize(NORMAL); ljosout.ljosview = VIEW_VECTOR;\n\
         \x20   ljosout.ljosmetalness = METALNESS; ljosout.ljosroughness = ROUGHNESS;\n\
         \x20   ljosout.ljosspecular = SPECULAR_AMOUNT; ljosout.ljosocclusion = OCCLUSION_AMOUNT;\n\
         \x20   ljosout.ljosedge = vec3(FRESNEL_POWER, FRESNEL_SCALE, FRESNEL_BIAS);\n\
         \x20   ljosout.ljoscoat = vec4(CLEARCOAT_AMOUNT, CLEARCOAT_ROUGHNESS, IOR, 0.0);\n\
         \x20   ljosout.ljoscoatedge = vec3(CLEARCOAT_FRESNEL_POWER, CLEARCOAT_FRESNEL_SCALE, CLEARCOAT_FRESNEL_BIAS);\n\
         \x20   ljosout.ljoscoatnormal = normalize(CLEARCOAT_NORMAL);\n\
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
        if f.probe {
            s.push_str(
                "void ljosprobe(mat3 ljosorientation, float ljosexpose, float ljoshorizon, float ljoslevels, float ljosao) {\n\
                 \x20   vec3 ljoswasdiffuse = DIFFUSE; vec3 ljoswasspecular = SPECULAR;\n\
                 \x20   DIFFUSE = vec3(0.0); SPECULAR = vec3(0.0);\n\
                 \x20   IBL_ORIENTATION = ljosorientation; IBL_EXPOSE = ljosexpose;\n\
                 \x20   IBL_HORIZON = ljoshorizon; IBL_MAXMIPMAP = ljoslevels; AO_FACTOR = ljosao;\n\
                 \x20   IBL_PROBE();\n\
                 \x20   DIFFUSE = ljoswasdiffuse + DIFFUSE * ljosao;\n\
                 \x20   SPECULAR = ljoswasspecular + SPECULAR * ljosao;\n\
                 }\n",
            );
        }
        s.push_str(
            "void ljosdim(float ljosby) { DIFFUSE *= ljosby; }\n\
             vec3 ljossum() {\n\
             \x20   EMISSIVE = EMISSIVE_COLOR;\n\
             \x20   return DIFFUSE + SPECULAR + EMISSIVE;\n\
             }\n\
             vec4 ljosfinish(vec3 ljosall) {\n\
             \x20   COLOR_SUM = vec4(ljosall, BASE_COLOR.a);\n\
             \x20   ljosfinaldiffuse = vec4(DIFFUSE, BASE_COLOR.a);\n",
        );
        if f.post {
            s.push_str("    POST_PROCESS();\n");
        }
        s.push_str("    return COLOR_SUM;\n}\n");
    }
    s.push_str("void main() {}\n");
    (s, before)
}

/// A place in a picture as Qt's numbers have it, v up, for the renderer's
/// pictures whose first row is the top; a way into a cube stays as it is.
const MATERIAL_READS: &str = "vec2 ljosup(vec2 ljosplace) { return vec2(ljosplace.x, 1.0 - ljosplace.y); }\n\
     vec3 ljosup(vec3 ljosway) { return ljosway; }\n\
     #define textureLod(ljospicture, ljosplace, ljoslevel) textureLod(ljospicture, ljosup(ljosplace), ljoslevel)\n";

/// A fragment shader's reads, which choose a level by their derivatives.
const MATERIAL_TEXTURE: &str =
    "#define texture(ljospicture, ljosplace) texture(ljospicture, ljosup(ljosplace))\n";

/// An effect's places count up from the picture's bottom row, as Qt has them,
/// and every picture here starts from its top one: each read turns the place
/// over. What is drawn into a picture ends up the right way for it.
const EFFECT_READS: &str = "#define texture(ljospicture, ljosplace) texture(ljospicture, vec2((ljosplace).x, 1.0 - (ljosplace).y))\n\
     #define textureLod(ljospicture, ljosplace, ljoslevel) textureLod(ljospicture, vec2((ljosplace).x, 1.0 - (ljosplace).y), ljoslevel)\n";

fn effect_vertex(header: &str, user: &str, src: &str, out: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str(EFFECT_READS);
    s.push_str("vec3 VERTEX; vec4 POSITION; vec2 INPUT_UV; vec2 TEXTURE_UV;\n");
    let before = lines(&s);
    push_user(&mut s, user);
    s.push_str("\nstruct LjosVertex {\n    vec4 ljosclip; vec4 ljosuvs;\n");
    for i in 0..locations(out) {
        let _ = writeln!(s, "    vec4 ljosvary{};", letters(i));
    }
    s.push_str(
        "};\n\
         LjosVertex ljosvertex(vec2 ljosuv) {\n\
         \x20   VERTEX = vec3(ljosuv.x * 2.0 - 1.0, 1.0 - ljosuv.y * 2.0, 0.0);\n\
         \x20   INPUT_UV = vec2(ljosuv.x, 1.0 - ljosuv.y); TEXTURE_UV = INPUT_UV; POSITION = vec4(0.0);\n",
    );
    for v in out {
        let _ = writeln!(s, "    {} = {}(0.0);", v.name, glsl_type(v.width));
    }
    s.push_str("    ljosglobals();\n");
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
    s.push_str(&pack_writes(out));
    s.push_str("    return ljosout;\n}\nvoid main() {}\n");
    (s, before)
}

fn effect_fragment(header: &str, user: &str, src: &str, ins: &[Varying]) -> (String, u32) {
    let mut s = String::from(header);
    s.push_str(EFFECT_READS);
    s.push_str("#define gl_FragCoord FRAGCOORD\nvec2 INPUT_UV; vec2 TEXTURE_UV; vec4 FRAGCOLOR; vec4 FRAGCOORD;\n");
    let before = lines(&s);
    push_user(&mut s, user);
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
    s.push_str("    ljosglobals();\n");
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

/// The vertex output both material entry points share. The binormal is in
/// the last place of the three that have one to spare, which leaves a
/// location for a material's own.
fn material_io(out: &[Varying]) -> String {
    let mut s = String::from(
        "\nstruct CmVsOut {\n\
         \x20   @builtin(position) clip: vec4<f32>,\n\
         \x20   @location(0) normal: vec4<f32>,\n\
         \x20   @location(1) world: vec4<f32>,\n\
         \x20   @location(2) uvs: vec4<f32>,\n\
         \x20   @location(3) tangent: vec4<f32>,\n\
         \x20   @location(4) color: vec4<f32>,\n",
    );
    for i in 0..locations(out) {
        let _ = writeln!(s, "    @location({}) vary{}: vec4<f32>,", 5 + i, letters(i));
    }
    s.push_str("};\n");
    s
}

/// The fragment stage's varyings, each from the vertex stage's of its
/// name, or zero where the vertex stage has none.
fn varying_args(out: &[Varying], ins: &[Varying]) -> String {
    let mut s = String::new();
    for v in ins {
        let Some(i) = out.iter().position(|o| o.name == v.name) else {
            s.push_str(", vec4<f32>(0.0)");
            continue;
        };
        let at: usize = out[..i].iter().map(|o| o.width).sum();
        let numbers: Vec<String> = (0..4)
            .map(|c| {
                if c < out[i].width {
                    let (location, component) = slot(at + c);
                    format!("in.vary{location}.{component}")
                } else {
                    "0.0".to_owned()
                }
            })
            .collect();
        let _ = write!(s, ", vec4<f32>({})", numbers.join(", "));
    }
    s
}

fn material_glue(out: &[Varying], ins: &[Varying], shaded: bool, f: &Functions) -> String {
    let mut s = material_io(out);
    // A mesh with no second buffer has no second UV set, joints or
    // weights: UV1 is UV0 and the rest zero, as the documentation says.
    let _ = write!(
        s,
        "@vertex\n\
         fn {MATERIAL_VERTEX}(\n\
         \x20   @location(0) position: vec3<f32>,\n\
         \x20   @location(1) normal: vec3<f32>,\n\
         \x20   @location(2) uv: vec2<f32>,\n\
         \x20   @location(3) tangent: vec4<f32>,\n\
         \x20   copy: Instance,\n\
         \x20   @location(11) data: vec4<f32>,\n\
         \x20   @builtin(instance_index) index: u32,\n\
         ) -> CmVsOut {{\n\
         \x20   return cm_vertex(position, normal, uv, uv, tangent, copy, data, index,\n\
         \x20       vec4<u32>(0u), vec4<f32>(0.0), vec4<f32>(1.0));\n\
         }}\n\
         @vertex\n\
         fn {MATERIAL_VERTEX_EXTRA}(\n\
         \x20   @location(0) position: vec3<f32>,\n\
         \x20   @location(1) normal: vec3<f32>,\n\
         \x20   @location(2) uv: vec2<f32>,\n\
         \x20   @location(3) tangent: vec4<f32>,\n\
         \x20   copy: Instance,\n\
         \x20   @location(11) data: vec4<f32>,\n\
         \x20   @builtin(instance_index) index: u32,\n\
         \x20   @location(8) uv1: vec2<f32>,\n\
         \x20   @location(9) joints: vec4<u32>,\n\
         \x20   @location(10) weights: vec4<f32>,\n\
         \x20   @location(12) color: vec4<f32>,\n\
         ) -> CmVsOut {{\n\
         \x20   return cm_vertex(position, normal, uv, uv1, tangent, copy, data, index, joints, weights, color);\n\
         }}\n\
         fn cm_vertex(position: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>, uv1: vec2<f32>,\n\
         \x20   tangent: vec4<f32>, copy: Instance, data: vec4<f32>, index: u32, joints: vec4<u32>,\n\
         \x20   weights: vec4<f32>, color: vec4<f32>) -> CmVsOut {{\n\
         \x20   let v = cmv_ljosvertex(position, normal, uv, uv1, tangent, copy_place(copy),\n\
         \x20       copy.color, data, i32(index), vec4<i32>(joints), weights, color, model.maps.w != 0u);\n\
         \x20   var out: CmVsOut;\n\
         \x20   out.clip = v.ljosclip;\n\
         \x20   out.normal = vec4<f32>(v.ljosnormal, v.ljosbinormal.x);\n\
         \x20   out.world = vec4<f32>(v.ljosworld, v.ljosbinormal.y);\n\
         \x20   out.uvs = v.ljosuvs;\n\
         \x20   out.tangent = vec4<f32>(v.ljostangent, v.ljosbinormal.z);\n\
         \x20   out.color = v.ljoscolor * copy.color;\n"
    );
    for i in 0..locations(out) {
        let l = letters(i);
        let _ = writeln!(s, "    out.vary{l} = v.ljosvary{l};");
    }
    let _ = write!(
        s,
        "    return out;\n\
         }}\n\
         fn cm_surface(in: CmVsOut, front: bool) -> cmf_LjosSurface {{\n\
         \x20   return cmf_ljossurface(in.normal.xyz, in.world.xyz, in.uvs, in.tangent.xyz,\n\
         \x20       vec3<f32>(in.normal.w, in.world.w, in.tangent.w), in.color,\n\
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
         \x20   return vec4<f32>(cm_out(c), clamp(c.a, 0.0, 1.0));\n\
         }}\n"
    );
    s
}

/// The light loop of lit.wgsl, with the author's processors called in
/// place of the built-in terms they replace. Nothing the snippet runs is
/// inside a branch on the pixel, so that it may sample textures.
fn shaded_glue(f: &Functions) -> String {
    // The sums of a PrincipledMaterial, in the same order: what a piece says
    // for itself takes the place of one term and the rest stay, so a piece
    // that gives back half of what it is given looks like the material beside it.
    let builtin = |gives: &str| {
        format!("cmf_ljosadd(base * {gives} * burley(n, l, v, roughness), vec3<f32>(0.0));")
    };
    let pick = |there: bool, call: &str, gives: &str| -> String {
        if there {
            format!(
                "cmf_ljoslight(less, l, attenuation, spot, shadow, amount);\n            {call}();"
            )
        } else {
            builtin(gives)
        }
    };
    let ambient = if f.ambient {
        "cmf_ljosambientlight();"
    } else {
        "cmf_ljosadd(dim, vec3<f32>(0.0));"
    };
    let specular = if f.specular {
        "cmf_ljoslight(light.color.rgb, l, attenuation * spot, spot, shadow, amount);\n        cmf_ljosspecularlight();"
    } else {
        "cmf_ljosadd(vec3<f32>(0.0), radiance * microfacets(n, l, v, f0, roughness));"
    };
    // A piece that says for itself what comes from round about takes the
    // place of all of it, the clear coat's as well. It reads the cube as it
    // is, and is told IBL_EXPOSE to expose it itself.
    let around = if f.probe {
        "cmf_ljosprobe(mat3x3<f32>(frame.probe_rot[0].xyz, frame.probe_rot[1].xyz, frame.probe_rot[2].xyz), exposure, horizon, last, ao);"
    } else {
        "let round = probe_seen(\n\
         \x20           textureSampleLevel(irradiance_cube, cube_sampler, select(probe_dir(n), n, own_probe), 0.0).rgb\n\
         \x20               * under_horizon(probe_dir(n), horizon),\n\
         \x20           own_probe,\n\
         \x20           exposure,\n\
         \x20       );\n\
         \x20       let r = normalize(reflect(-v, n));\n\
         \x20       let read = probe_light(r, clamp(roughness * 5.0, 0.0, last), own_probe, horizon);\n\
         \x20       cmf_ljosadd(base * (vec3<f32>(1.0) - amount) * round * ao, probe_seen(read * env_brdf(amount, roughness, nv), own_probe, exposure) * ao);\n\
         \x20       if coat > 0.0 {\n\
         \x20           coating += probe_seen(probe_light(normalize(reflect(-v, coat_n)), clamp(coat_rough * 5.0, 0.0, last), own_probe, horizon)\n\
         \x20               * env_brdf(bent, coat_rough, coat_nv), own_probe, exposure) * ao;\n\
         \x20       }\n"
    };
    format!(
        "fn cm_shade(in: CmVsOut, front: bool) -> vec4<f32> {{\n\
         \x20   let s = cm_surface(in, front);\n\
         \x20   let copied = select(vec4<f32>(1.0), in.color, model.map_params.w != 0.0);\n\
         \x20   let base = s.ljosbase.rgb * copied.rgb;\n\
         \x20   let n = s.ljosnormal;\n\
         \x20   let v = s.ljosview;\n\
         \x20   let nv = max(dot(n, v), 1e-4);\n\
         \x20   let metal = clamp(s.ljosmetalness, 0.0, 1.0);\n\
         \x20   let roughness = clamp(s.ljosroughness, 0.0, 1.0);\n\
         \x20   let ior = s.ljoscoat.z;\n\
         \x20   let bent = vec3<f32>(((ior - 1.0) * (ior - 1.0)) / ((ior + 1.0) * (ior + 1.0)));\n\
         \x20   let f0 = mix(bent, base, metal);\n\
         \x20   let edge = s.ljosedge;\n\
         \x20   let rim = select(pow(max(1.0 - nv, 1e-4), edge.x), 1.0, edge.x == 0.0);\n\
         \x20   let turning = f0 + (max(vec3<f32>(1.0 - roughness), f0) - f0) * rim;\n\
         \x20   let amount = vec3<f32>(metal + s.ljosspecular * (1.0 - metal))\n\
         \x20       * clamp(vec3<f32>(edge.z) + edge.y * turning, vec3<f32>(0.0), vec3<f32>(1.0));\n\
         \x20   let coat = clamp(s.ljoscoat.x, 0.0, 1.0);\n\
         \x20   let coat_rough = clamp(s.ljoscoat.y, 0.0, 1.0);\n\
         \x20   let coat_edge = s.ljoscoatedge;\n\
         \x20   let coat_n = s.ljoscoatnormal;\n\
         \x20   let coat_nv = max(dot(coat_n, v), 1e-4);\n\
         \x20   var coating = vec3<f32>(0.0);\n\
         \x20   let dim = (frame.ambient.rgb + model.ambient.rgb) * (1.0 - metal) * base;\n\
         \x20   cmf_ljosbegin(dim);\n\
         \x20   {ambient}\n\
         \x20   for (var i = 0u; i < min(frame.counts.x, MAX_LIGHTS); i++) {{\n\
         \x20       if (model.flags.x & (1u << i)) == 0u {{\n\
         \x20           continue;\n\
         \x20       }}\n\
         \x20       let light = frame.lights[i];\n\
         \x20       var l = -light.direction.xyz;\n\
         \x20       var attenuation = 1.0;\n\
         \x20       var spot = 1.0;\n\
         \x20       if light.position.w > 0.5 {{\n\
         \x20           let to_light = light.position.xyz - in.world.xyz;\n\
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
         \x20       let shadow = shadowed(light, in.world.xyz, l);\n\
         \x20       let radiance = light.color.rgb * attenuation * spot * shadow;\n\
         \x20       // Qt takes what is metal out of the light a piece is given, and\n\
         \x20       // out of the sum of them once more below.\n\
         \x20       let less = light.color.rgb * (1.0 - metal);\n\
         \x20       if light.position.w < 0.5 {{\n\
         \x20           {directional}\n\
         \x20       }} else if light.position.w < 1.5 {{\n\
         \x20           {point}\n\
         \x20       }} else if spot > 0.0 {{\n\
         \x20           {spot_light}\n\
         \x20       }}\n\
         \x20       {specular}\n\
         \x20       if coat > 0.0 {{\n\
         \x20           coating += radiance * microfacets(coat_n, l, v, bent, coat_rough);\n\
         \x20       }}\n\
         \x20   }}\n\
         \x20   let ao = s.ljosocclusion;\n\
         \x20   cmf_ljosdim(ao);\n\
         \x20   let own_probe = model.probe.y > 0.5;\n\
         \x20   if model.probe.y > 1.5 {{\n\
         \x20       let last = model.probe.x - 1.0;\n\
         \x20       let r = normalize(reflect(-v, n));\n\
         \x20       let round = textureSampleLevel(irradiance_cube, cube_sampler, righted(n, in.world.xyz), 0.0).rgb;\n\
         \x20       let read = textureSampleLevel(specular_cube, cube_sampler, righted(r, in.world.xyz), clamp(roughness * 5.0, 0.0, last)).rgb;\n\
         \x20       cmf_ljosadd(base * (vec3<f32>(1.0) - amount) * round, read * env_brdf(amount, roughness, nv));\n\
         \x20       if coat > 0.0 {{\n\
         \x20           coating += env_brdf(bent, coat_rough, coat_nv)\n\
         \x20               * textureSampleLevel(specular_cube, cube_sampler, righted(normalize(reflect(-v, coat_n)), in.world.xyz), clamp(coat_rough * 5.0, 0.0, last)).rgb;\n\
         \x20       }}\n\
         \x20   }} else if frame.env.w > 0.5 || own_probe {{\n\
         \x20       let exposure = select(frame.env.x, model.probe.z, own_probe);\n\
         \x20       let last = select(frame.env.z, model.probe.x, own_probe);\n\
         \x20       let horizon = select(frame.env.y, 0.0, own_probe);\n\
         \x20       {around}\n\
         \x20   }}\n\
         \x20   cmf_ljosdim(1.0 - metal);\n\
         \x20   var sum = fogged(cmf_ljossum(), in.world.xyz);\n\
         \x20   if coat > 0.0 {{\n\
         \x20       let turn = clamp(pow(clamp(dot(coat_n, v), 0.0, 1.0), coat_edge.x), 0.0, 1.0);\n\
         \x20       let over = clamp(\n\
         \x20           vec3<f32>(coat_edge.z) + coat_edge.y * (bent + (vec3<f32>(1.0) - bent) * (1.0 - turn)),\n\
         \x20           vec3<f32>(0.0),\n\
         \x20           vec3<f32>(1.0),\n\
         \x20       );\n\
         \x20       sum = sum * (vec3<f32>(1.0) - coat * over) + coating * coat;\n\
         \x20   }}\n\
         \x20   var done = cmf_ljosfinish(sum);\n\
         \x20   done.a *= copied.a * model.map_params.z;\n\
         \x20   return done;\n\
         }}\n\
         fn cm_out(c: vec4<f32>) -> vec3<f32> {{\n\
         \x20   return tonemap(c.rgb, frame.tone);\n\
         }}\n",
        directional = pick(f.directional, "cmf_ljosdirectional", "less"),
        point = pick(f.point, "cmf_ljospoint", "less * attenuation"),
        spot_light = pick(f.spot, "cmf_ljosspot", "less * attenuation * spot"),
    )
}

fn effect_glue(out: &[Varying], ins: &[Varying]) -> String {
    let mut s = String::from(
        "\nstruct CmEffectOut {\n\
         \x20   @builtin(position) clip: vec4<f32>,\n\
         \x20   @location(0) uvs: vec4<f32>,\n",
    );
    for i in 0..locations(out) {
        let _ = writeln!(s, "    @location({}) vary{}: vec4<f32>,", 1 + i, letters(i));
    }
    // A rectangle over the output, so that a vertex snippet moving its corners
    // leaves a rectangle.
    let _ = write!(
        s,
        "}};\n\
         @vertex\n\
         fn {EFFECT_VERTEX}(@builtin(vertex_index) i: u32) -> CmEffectOut {{\n\
         \x20   let uv = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));\n\
         \x20   let v = cmv_ljosvertex(uv);\n\
         \x20   var out: CmEffectOut;\n\
         \x20   out.clip = v.ljosclip;\n\
         \x20   out.uvs = v.ljosuvs;\n"
    );
    for i in 0..locations(out) {
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
    fn a_global_with_a_value_gets_it_in_ljosglobals() {
        let (code, late) = late_globals(
            "const float A = 1.0;\nfloat b = a * 2.0;\nvec2 c[2] = vec2[2](\n  vec2(0.0), vec2(1.0));\nfloat d, e = 1.0;\nstruct S { float f; };\nfloat g(float h) { float i = h; return i; }\n#ifdef X\nfloat j = k;\n#endif\n",
        );
        assert_eq!(
            late,
            "b =  a * 2.0; c = vec2[2](   vec2(0.0), vec2(1.0)); ".replace("c = vec2", "c =  vec2")
        );
        assert_eq!(
            code,
            "const float A = 1.0;\nfloat b ;\nvec2 c[2] ;\n\nfloat d, e = 1.0;\nstruct S { float f; };\nfloat g(float h) { float i = h; return i; }\n#ifdef X\nfloat j = k;\n#endif\n"
        );
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
                 void POST_PROCESS() { COLOR_SUM = vec4(DIFFUSE.rgb, 1.0); }",
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
    fn a_matrix_varying_passes_as_its_columns() {
        material(
            Some("VARYING mat3 tbn;\nvoid MAIN() { tbn = mat3(TANGENT, BINORMAL, NORMAL); }"),
            Some("VARYING mat3 tbn;\nvoid MAIN() { FRAGCOLOR = vec4(tbn[2], 1.0); }"),
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
            Some(
                "VARYING vec2 t;\nVARYING vec3 n;\nvoid MAIN() {\n\
                 t = UV0 + INSTANCE_DATA.xy + INSTANCE_COLOR.zw + WEIGHTS.xy;\n\
                 n = BONE_NORMAL_TRANSFORMS[JOINTS.x + INSTANCE_INDEX] * NORMAL;\n\
                 POSITION = INSTANCE_MODELVIEWPROJECTION_MATRIX * BONE_TRANSFORMS[JOINTS.y]\n\
                 \x20   * vec4(VERTEX, 1.0);\n}",
            ),
            Some("VARYING vec2 t;\nvoid MAIN() { BASE_COLOR = texture(picture, t) * texture(SCREEN_TEXTURE, t); }\nvoid POINT_LIGHT() { DIFFUSE += LIGHT_COLOR * LIGHT_ATTENUATION; }"),
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
            (
                p.wgsl(),
                &[
                    (MATERIAL_VERTEX, ShaderStage::Vertex),
                    (MATERIAL_VERTEX_EXTRA, ShaderStage::Vertex),
                    (MATERIAL_FRAGMENT, ShaderStage::Fragment),
                ][..],
            ),
            (
                e.wgsl(),
                &[
                    (EFFECT_VERTEX, ShaderStage::Vertex),
                    (EFFECT_FRAGMENT, ShaderStage::Fragment),
                ][..],
            ),
        ] {
            let module = naga::front::wgsl::parse_str(wgsl).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            for &(entry, stage) in entries {
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
    fn a_material_names_the_screen_it_reads() {
        let reads = |src: &str| material(None, Some(src), false, &[], &[]).unwrap().screen();
        assert_eq!(reads("void MAIN() {}"), Screen::None);
        assert_eq!(
            reads("void MAIN() { FRAGCOLOR = texture(SCREEN_TEXTURE, UV0); }"),
            Screen::Texture
        );
        assert_eq!(
            reads("void MAIN() { FRAGCOLOR = textureLod(SCREEN_MIP_TEXTURE, UV0, 2.0); }"),
            Screen::Mipmapped
        );
        // A comment is not a read.
        assert_eq!(reads("// SCREEN_TEXTURE\nvoid MAIN() {}"), Screen::None);
    }

    #[test]
    fn bone_normal_transforms_are_read_through_a_call() {
        assert_eq!(
            bone_normals("n = BONE_NORMAL_TRANSFORMS[ a[1] ]\n * N; MY_BONE_NORMAL_TRANSFORMS[0];"),
            "n = ljosbonenormal( a[1] )\n * N; MY_BONE_NORMAL_TRANSFORMS[0];"
        );
        assert_eq!(
            bone_normals("BONE_NORMAL_TRANSFORMS [JOINTS.x]"),
            "ljosbonenormal( JOINTS.x)"
        );
        // Left for the compiler to name where it is not indexed.
        assert_eq!(
            bone_normals("BONE_NORMAL_TRANSFORMS;"),
            "BONE_NORMAL_TRANSFORMS;"
        );
    }

    #[test]
    fn an_instanced_or_skinned_vertex_is_placed_without_author_code() {
        let p = material(Some("void MAIN() {}"), None, false, &[], &[]).unwrap();
        // The default POSITION goes through the copy and the skin.
        assert!(p.wgsl().contains("cmv_ljosinstance"), "{}", p.wgsl());
        let module = naga::front::wgsl::parse_str(p.wgsl()).unwrap();
        for entry in [MATERIAL_VERTEX, MATERIAL_VERTEX_EXTRA] {
            let ep = module.entry_points.iter().find(|e| e.name == entry);
            let ep = ep.unwrap_or_else(|| panic!("{entry}"));
            assert_eq!(ep.stage, ShaderStage::Vertex);
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
