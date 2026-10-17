//! The programs of a `ShaderEffect` (ARCHITECTURE.md section 7.1, "As
//! built: shader effects").
//!
//! Qt 6 names a shader by a `.qsb` file, a container of Qt's own that Ljós
//! does not read. A Qt project has the GLSL 440 source beside it (what it
//! gave `qsb`), so Ljós compiles that source, with naga, into the GLSL its
//! GL painter links: GLSL ES 3.00 and desktop GLSL 3.30. What comes out is
//! [`Compiled`]: the two texts, the uniform block's members at their std140
//! offsets, and the samplers by the names QML gives their properties. It
//! serialises as a `.ljsh` file, which `ljos shaders` writes at build time,
//! so that a shipped application loads programs without naga (the
//! `compile` feature off).
//!
//! Nothing here names a GPU API: the GL painter in `ljos_shell` links the
//! texts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[cfg(feature = "compile")]
mod compile;
#[cfg(feature = "compile")]
mod rewrite;

#[cfg(feature = "compile")]
pub use compile::compile;

/// The suffix of a compiled program beside its source: `X.frag.ljsh`.
pub const ARTIFACT_SUFFIX: &str = "ljsh";

/// What a `.ljsh` file says it holds; any other is refused, and made again.
const FORMAT: u32 = 1;

/// Which part of the pipeline a shader is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    Vertex,
    Fragment,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Vertex => "vertex",
            Stage::Fragment => "fragment",
        }
    }
}

/// A shader ready to be linked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Compiled {
    format: u32,
    pub stage: Stage,
    /// For GLES 3 contexts.
    pub es300: String,
    /// For desktop GL 3.3 and later.
    pub gl330: String,
    /// The uniform block, when the shader has one.
    pub block: Option<Block>,
    /// The textures the shader samples, in the order of their bindings.
    pub samplers: Vec<Sampler>,
}

/// A uniform block as std140 lays it out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// The block's name in the GL text, which differs by stage, so the GL
    /// painter binds each stage's block to the one buffer.
    pub gl_name: String,
    /// Bytes, with std140's padding.
    pub size: u32,
    pub members: Vec<Member>,
}

/// One member of a uniform block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub offset: u32,
    pub ty: Type,
}

/// The type of a uniform, as far as a QML value converts to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Float,
    Int,
    Uint,
    /// `vec2` to `vec4`.
    Vec(u8),
    /// `ivec2` to `ivec4`.
    IVec(u8),
    /// A column-major matrix of `columns` columns of `rows` floats; std140
    /// pads each column to four.
    Mat {
        columns: u8,
        rows: u8,
    },
    /// Arrays and structures: left as zeros.
    Other,
}

/// A texture the shader samples.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sampler {
    /// The name in the source, which is the QML property's.
    pub name: String,
    /// The `sampler2D` uniform in the GL text.
    pub gl_name: String,
}

impl Compiled {
    /// Reads a `.ljsh` file's text.
    pub fn from_artifact(text: &str) -> Result<Self, String> {
        let c: Compiled = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if c.format != FORMAT {
            return Err(format!(
                "made by another version of Ljós (format {}, not {FORMAT}); run `ljos shaders` again",
                c.format
            ));
        }
        Ok(c)
    }

    /// What a `.ljsh` file holds.
    pub fn to_artifact(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

/// Why a shader could not be had. The texts are what `ShaderEffect.log`
/// says.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum Error {
    #[error("Failed to find shader \"{0}\"")]
    Missing(String),
    #[error("{qsb} is a .qsb file, which Ljós cannot read; put its GLSL source {glsl} beside it")]
    Qsb { qsb: String, glsl: String },
    #[error("{0}: this build of Ljós compiles no shaders; make {0}.ljsh with `ljos shaders`")]
    NotCompiled(String),
    #[error("{path}: {message}")]
    Read { path: String, message: String },
    #[error("{0}")]
    Compile(String),
    #[error("the vertex and fragment shaders lay out {0} differently in their uniform blocks")]
    Mismatch(String),
}

/// Where the program for a shader's URL is.
#[derive(Clone, Debug, PartialEq)]
pub enum Located {
    /// A `.ljsh` beside the source: read at once.
    Artifact(PathBuf),
    /// GLSL source, compiled in the background.
    Source(PathBuf),
}

/// Finds the program `path` names. `X.qsb` names `X` (the source) and
/// `X.ljsh`; with the `compile` feature the source wins, so an edit to it
/// is seen without making the artifact again.
pub fn locate(path: &Path) -> Result<Located, Error> {
    let shown = path.display().to_string();
    let base = match path.extension() {
        Some(e) if e == "qsb" => path.with_extension(""),
        _ => path.to_owned(),
    };
    if base.extension().is_some_and(|e| e == ARTIFACT_SUFFIX) {
        return if base.is_file() {
            Ok(Located::Artifact(base))
        } else {
            Err(Error::Missing(shown))
        };
    }
    let artifact = artifact_path(&base);
    if cfg!(feature = "compile") && base.is_file() {
        return Ok(Located::Source(base));
    }
    if artifact.is_file() {
        return Ok(Located::Artifact(artifact));
    }
    if base.is_file() {
        return Err(Error::NotCompiled(base.display().to_string()));
    }
    if path.is_file() {
        return Err(Error::Qsb {
            qsb: shown,
            glsl: base.display().to_string(),
        });
    }
    Err(Error::Missing(shown))
}

/// `X.frag` → `X.frag.ljsh`.
pub fn artifact_path(source: &Path) -> PathBuf {
    let mut s = source.as_os_str().to_owned();
    s.push(".");
    s.push(ARTIFACT_SUFFIX);
    PathBuf::from(s)
}

/// What was loaded from a path, kept while the file stays the same, so the
/// delegates of a view that share a shader have it at once.
type ByPath = HashMap<(PathBuf, Stage), (Option<SystemTime>, u64, Arc<Compiled>)>;

fn by_path() -> &'static Mutex<ByPath> {
    static CACHE: OnceLock<Mutex<ByPath>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok(), m.len()))
}

/// The program loaded from `located` before, if its file has not changed.
pub fn cached(located: &Located, stage: Stage) -> Option<Arc<Compiled>> {
    let path = match located {
        Located::Artifact(p) | Located::Source(p) => p,
    };
    let (time, len) = stamp(path)?;
    let cache = by_path().lock().ok()?;
    let (t, l, c) = cache.get(&(path.clone(), stage))?;
    (*t == time && *l == len).then(|| c.clone())
}

/// Loads the program: reads an artifact, or reads and compiles a source.
/// Compiling takes about 0.4 ms for a small shader in a release build, so
/// a caller on the UI thread sends a [`Located::Source`] elsewhere.
pub fn load(located: &Located, stage: Stage) -> Result<Arc<Compiled>, Error> {
    if let Some(c) = cached(located, stage) {
        return Ok(c);
    }
    let path = match located {
        Located::Artifact(p) | Located::Source(p) => p,
    };
    let before = stamp(path);
    let text = std::fs::read_to_string(path).map_err(|e| Error::Read {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let compiled = match located {
        Located::Artifact(_) => Compiled::from_artifact(&text).map_err(|message| Error::Read {
            path: path.display().to_string(),
            message,
        })?,
        Located::Source(_) => compile_source(&text, stage)?,
    };
    if compiled.stage != stage {
        return Err(Error::Read {
            path: path.display().to_string(),
            message: format!(
                "is a {} shader, not a {} one",
                compiled.stage.name(),
                stage.name()
            ),
        });
    }
    let compiled = Arc::new(compiled);
    if let (Some((t, l)), Ok(mut cache)) = (before, by_path().lock()) {
        cache.insert((path.clone(), stage), (t, l, compiled.clone()));
    }
    Ok(compiled)
}

#[cfg(feature = "compile")]
fn compile_source(text: &str, stage: Stage) -> Result<Compiled, Error> {
    compile(text, stage)
}

#[cfg(not(feature = "compile"))]
fn compile_source(_text: &str, _stage: Stage) -> Result<Compiled, Error> {
    Err(Error::NotCompiled(String::new()))
}

/// Qt's default vertex shader: the item's rectangle, `qt_TexCoord0` from
/// its top left (0, 0) to its bottom right (1, 1).
pub fn default_vertex() -> Arc<Compiled> {
    static V: OnceLock<Arc<Compiled>> = OnceLock::new();
    V.get_or_init(|| Arc::new(built_in(DEFAULT_VERTEX_ARTIFACT)))
        .clone()
}

/// Qt's default fragment shader: `source` times `qt_Opacity`.
pub fn default_fragment() -> Arc<Compiled> {
    static F: OnceLock<Arc<Compiled>> = OnceLock::new();
    F.get_or_init(|| Arc::new(built_in(DEFAULT_FRAGMENT_ARTIFACT)))
        .clone()
}

fn built_in(text: &str) -> Compiled {
    // Checked by the `defaults` test, which makes these files.
    Compiled::from_artifact(text).expect("a built-in shader artifact reads")
}

/// The sources of the default shaders, and what they compile to.
pub const DEFAULT_VERTEX_SOURCE: &str = include_str!("../defaults/default.vert");
pub const DEFAULT_FRAGMENT_SOURCE: &str = include_str!("../defaults/default.frag");
const DEFAULT_VERTEX_ARTIFACT: &str = include_str!("../defaults/default.vert.ljsh");
const DEFAULT_FRAGMENT_ARTIFACT: &str = include_str!("../defaults/default.frag.ljsh");

/// A vertex and a fragment shader that are drawn together, and the one
/// uniform buffer they share.
#[derive(Debug, PartialEq)]
pub struct Program {
    pub vertex: Arc<Compiled>,
    pub fragment: Arc<Compiled>,
    /// Both stages' members, each once.
    pub members: Vec<Member>,
    /// The buffer's size: the larger block's.
    pub size: u32,
}

impl Program {
    /// Pairs two shaders. Qt gives both stages one buffer, so a member both
    /// declare must be at the same place in each.
    pub fn new(vertex: Arc<Compiled>, fragment: Arc<Compiled>) -> Result<Self, Error> {
        let mut members: Vec<Member> = Vec::new();
        let mut size = 0;
        for block in [&vertex.block, &fragment.block].into_iter().flatten() {
            size = size.max(block.size);
            for m in &block.members {
                match members.iter().find(|n| n.name == m.name) {
                    Some(n) if n.offset != m.offset || n.ty != m.ty => {
                        return Err(Error::Mismatch(m.name.clone()));
                    }
                    Some(_) => {}
                    None => members.push(m.clone()),
                }
            }
        }
        Ok(Self {
            vertex,
            fragment,
            members,
            size,
        })
    }

    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.name == name)
    }

    /// Every sampler of both stages: the QML name, and where the GL text
    /// has it.
    pub fn samplers(&self) -> impl Iterator<Item = &Sampler> {
        self.vertex.samplers.iter().chain(&self.fragment.samplers)
    }

    /// The textures by QML name, each once, in the order the painter gives
    /// them units.
    pub fn texture_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for s in self.samplers() {
            if !names.contains(&s.name.as_str()) {
                names.push(&s.name);
            }
        }
        names
    }
}

/// Writes `values` (floats or ints, as `ty` wants) into a std140 buffer at
/// `member`'s place. Missing values are zeros.
pub fn write_member(buf: &mut [u8], member: &Member, values: &[f64]) {
    let at = member.offset as usize;
    let put_f = |buf: &mut [u8], i: usize, v: f64| {
        if let Some(d) = buf.get_mut(at + 4 * i..at + 4 * i + 4) {
            d.copy_from_slice(&(v as f32).to_ne_bytes());
        }
    };
    let put_i = |buf: &mut [u8], i: usize, v: f64| {
        if let Some(d) = buf.get_mut(at + 4 * i..at + 4 * i + 4) {
            d.copy_from_slice(&(v as i32).to_ne_bytes());
        }
    };
    let get = |i: usize| values.get(i).copied().unwrap_or(0.0);
    match member.ty {
        Type::Float => put_f(buf, 0, get(0)),
        Type::Int => put_i(buf, 0, get(0)),
        Type::Uint => {
            if let Some(d) = buf.get_mut(at..at + 4) {
                d.copy_from_slice(&(get(0).max(0.0) as u32).to_ne_bytes());
            }
        }
        Type::Vec(n) => (0..n as usize).for_each(|i| put_f(buf, i, get(i))),
        Type::IVec(n) => (0..n as usize).for_each(|i| put_i(buf, i, get(i))),
        Type::Mat { columns, rows } => {
            for c in 0..columns as usize {
                for r in 0..rows as usize {
                    put_f(buf, 4 * c + r, get(c * rows as usize + r));
                }
            }
        }
        Type::Other => {}
    }
}

#[cfg(test)]
mod tests;
