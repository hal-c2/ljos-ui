//! Ahead-of-time QML compilation for an application's build script
//! (the non-default `build` feature, for `[build-dependencies]`).
//!
//! The build script compiles the application's QML — imports of its
//! own native modules included — into a pack in `OUT_DIR`; the
//! program embeds the pack with [`include_qml!`](crate::include_qml)
//! and links no parser (ARCHITECTURE.md principle 5). A file that names
//! a property, signal, method or enum key its type does not have fails
//! the build.
//!
//! The compile learns the native types one of two ways:
//!
//! - From the application's type dump file ([`compile_dir_with_types`]),
//!   which the application's own test writes and keeps current
//!   ([`crate::types::check_file`]). The build script runs none of the
//!   application's code, so the native types live in the application's
//!   own crate and the build script depends on `ljos` alone.
//! - From a throwaway Engine the script's `register` function fills
//!   ([`compile_dir`]). A build script cannot use the types of the crate
//!   it builds, so those types then live in a crate of their own that
//!   both the build script and the program depend on.
//!
//! ```no_run
//! // build.rs, with `ljos = { features = ["build"], default-features = false }`
//! // under [build-dependencies]
//! let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
//! ljos::build::compile_dir_with_types(
//!     "qml".as_ref(),
//!     "ljostypes.json".as_ref(),
//!     &out.join("qml.pack"),
//! )
//! .unwrap();
//! ```
//!
//! ```ignore
//! // main.rs
//! ljos::App::new("Hello")
//!     .register(my_app::register)
//!     .source(ljos::include_qml!("qml.pack"))
//!     .root("Main")
//!     .run()?;
//! ```
//!
//! The pack also carries the files under the QML's directories that QML
//! loads: pictures, fonts, models, sounds, films and the data a model
//! reads ([`Options::with_files`] names other kinds, [`Options::without_files`]
//! carries none of them), and every file a module's CMake `RESOURCES` list
//! names, whatever its kind and whatever kinds the build takes. A file
//! whose link leads out of its directory is not carried. The program that runs the pack finds each where the
//! QML names it, relative to the file that names it or at its module's
//! `qrc:/qt/qml/<uri>/` address, with no code of its own
//! ([`register_pack_files`](crate::register_pack_files)). The pack says
//! which of the QML files it was given did not compile and why, those the
//! compile left out (a link, a file that is not UTF-8) among them.
//!
//! The `_to_rust` forms also compile the files' bindings and handlers to
//! Rust (ARCHITECTURE.md §5.4), beside the pack (`qml.pack` and
//! `qml.rs`); the program embeds both, and what the generator refused
//! runs as it would without:
//!
//! ```ignore
//! ljos::App::new("Hello")
//!     .source(ljos::include_qml!("qml.pack"))
//!     .compiled(ljos::include_compiled!("qml.rs"))
//!     .root("Main")
//!     .run()?;
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ljos_compiler::sources::PackSources;
use ljos_engine::{Engine, EngineError, MapSource, TypeDump, TypeDumpError};

pub use crate::compile::{CompileError, EngineOracle, EngineType, NativeTypeError};
use crate::compile::{OnError, compile_dirs as compile_to_source, compile_with_oracle};
use crate::types::WRITE_VAR;

/// Why a build script's compile wrote no pack.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// The QML did not compile, or a directory could not be read.
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// The type dump file could not be read: the text says what writes it.
    #[error("{}: {source}; the application's test writes it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    TypesUnread {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The type dump file is not a dump this version reads.
    #[error("{}: {source}; the application's test writes it with `{WRITE_VAR}=write cargo test --no-fail-fast`", path.display())]
    TypesUnreadable {
        path: PathBuf,
        #[source]
        source: TypeDumpError,
    },
    /// The type dump describes a type the compiler cannot be told of.
    #[error("{}: {source}", path.display())]
    NativeTypes {
        path: PathBuf,
        #[source]
        source: NativeTypeError,
    },
    /// A file the pack carries, or a directory it is in, could not be
    /// read.
    #[error("{}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The compiled files do not fit in a pack.
    #[error(transparent)]
    Pack(EngineError),
    /// The pack could not be written.
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Compiles every `.qml` file under `dir` into the pack `out`, against
/// the table of types plus what `register` registers; see
/// [`compile_dirs`].
pub fn compile_dir(dir: &Path, register: impl Fn(&Engine), out: &Path) -> Result<(), BuildError> {
    compile_dirs(&[dir], register, out)
}

/// Compiles every `.qml` file under each of `dirs` (keyed from its own
/// root, as [`App::qml_dirs`](crate::App::qml_dirs) reads them) into
/// the pack `out`. The types known are the table's and those
/// `register` puts on a throwaway Engine that also holds the standard
/// modules, as the program's Engine will. Any file with an error fails
/// the compile with every error, one a line: what ships is clean.
/// Prints `cargo:rerun-if-changed` for every directory and file read,
/// so the script runs again when the QML changes.
pub fn compile_dirs(
    dirs: &[&Path],
    register: impl Fn(&Engine),
    out: &Path,
) -> Result<(), BuildError> {
    Options::default().compile_dirs(dirs, register, out)
}

/// [`compile_dirs`], and the files' bindings and handlers compiled to
/// Rust in `out` with the extension `rs`, for
/// [`include_compiled!`](crate::include_compiled). What the generator
/// compiled and refused, by file, comes back.
pub fn compile_dirs_to_rust(
    dirs: &[&Path],
    register: impl Fn(&Engine),
    out: &Path,
) -> Result<Vec<CompiledFile>, BuildError> {
    Options::default().compile_dirs_to_rust(dirs, register, out)
}

fn registered_source(
    dirs: &[&Path],
    register: impl Fn(&Engine),
) -> Result<(MapSource, EngineOracle), BuildError> {
    for dir in dirs {
        rerun_if_changed(dir);
    }
    let engine = crate::engine();
    register(&engine);
    let source = compile_to_source(dirs, Some(&engine), OnError::Fail)?;
    Ok((source, EngineOracle::from_engine(&engine)))
}

/// Compiles every `.qml` file under `dir` into the pack `out`, against
/// the table of types plus the native types the type dump file `types`
/// describes; see [`compile_dirs_with_types`].
pub fn compile_dir_with_types(dir: &Path, types: &Path, out: &Path) -> Result<(), BuildError> {
    compile_dirs_with_types(&[dir], types, out)
}

/// [`compile_dirs`] against a type dump file in place of a `register`
/// function: the native types are those `types` describes (written by
/// [`crate::types::check_file`]), over the toolkit's own. Runs none of
/// the application's code. Any file with an error fails the compile,
/// unless `LJOS_TYPES=write` is set: then the file is being written
/// again, and a QML file that does not compile against the old one is
/// packed as a failure (it fails where it is made) and reported as a
/// warning, so that the application builds and its test can write the
/// new file.
pub fn compile_dirs_with_types(dirs: &[&Path], types: &Path, out: &Path) -> Result<(), BuildError> {
    Options::default().compile_dirs_with_types(dirs, types, out)
}

/// [`compile_dirs_with_types`], and the files' bindings and handlers
/// compiled to Rust in `out` with the extension `rs`; see
/// [`compile_dirs_to_rust`].
pub fn compile_dirs_with_types_to_rust(
    dirs: &[&Path],
    types: &Path,
    out: &Path,
) -> Result<Vec<CompiledFile>, BuildError> {
    Options::default().compile_dirs_with_types_to_rust(dirs, types, out)
}

/// The kinds of files a pack carries unless its build says otherwise
/// ([`Options::with_files`]), by extension: those a QML type loads. A
/// picture (`Image`, `BorderImage`, `AnimatedImage`, a texture), a font
/// (`FontLoader`), a model or its buffers (`RuntimeLoader`, a `Model`'s
/// mesh), a sound or a film, a document (`PdfDocument`) and the data a
/// model reads (`XmlListModel`, `JsonListModel`, a Lottie animation).
// One line a kind, so that each comment heads its own.
#[rustfmt::skip]
pub const FILE_KINDS: &[&str] = &[
    // Pictures.
    "png", "jpg", "jpeg", "gif", "bmp", "svg", "svgz", "webp", "ico", "tga", "tif", "tiff", "pbm",
    "pgm", "ppm", "ktx", "ktx2", "hdr", "exr", "astc", "pkm", "dds",
    // Fonts.
    "ttf", "otf", "ttc", "otc", "woff", "woff2",
    // Models and their buffers.
    "glb", "gltf", "bin", "obj", "mesh",
    // Sound and film.
    "wav", "mp3", "ogg", "oga", "opus", "flac", "m4a", "aac", "mp4", "m4v", "webm", "mkv", "mov",
    // Documents and data.
    "pdf", "json", "xml",
];

/// How a pack is written: whether it carries the sources its files were
/// compiled from, for a program whose users change it while it runs (the
/// feature `user-qml`, ARCHITECTURE.md §12.1). Its methods are the
/// module's functions.
///
/// ```no_run
/// // build.rs
/// # let out = std::path::Path::new("qml.pack");
/// ljos::build::Options::new()
///     .with_sources()
///     .compile_dirs_with_types(&["qml".as_ref()], "ljostypes.json".as_ref(), out)
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Options {
    sources: bool,
    /// The extensions of the files under the QML's directories the pack
    /// carries, lower case and without the dot.
    files: Vec<String>,
}

impl Default for Options {
    /// Sources when the build's own `ljos` has `user-qml` on: a build
    /// script that names the feature builds for a program that has it.
    /// `dev` turns `user-qml` on too, and so does any other crate's
    /// build-dependency on `ljos` with either feature in the same build,
    /// as Cargo unifies them: a program that ships without its sources
    /// says so with [`without_sources`](Self::without_sources), one that
    /// ships with them with [`with_sources`](Self::with_sources).
    fn default() -> Self {
        Self {
            sources: cfg!(feature = "user-qml"),
            files: FILE_KINDS.iter().map(|&kind| kind.to_owned()).collect(),
        }
    }
}

impl Options {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The pack carries the text of every QML file, script, declaration
    /// and `qmldir` it was compiled from, deflated, after its files: what
    /// a user's file is compiled against. A program without `user-qml`
    /// never reads them.
    #[must_use]
    pub fn with_sources(mut self) -> Self {
        self.sources = true;
        self
    }

    /// The pack carries its compiled files alone.
    #[must_use]
    pub fn without_sources(mut self) -> Self {
        self.sources = false;
        self
    }

    /// The pack carries the files under the QML's directories with these
    /// extensions (`png`, any case), in place of [`FILE_KINDS`].
    #[must_use]
    pub fn with_files<'a>(mut self, kinds: impl IntoIterator<Item = &'a str>) -> Self {
        self.files = kinds
            .into_iter()
            .map(|kind| kind.trim_start_matches('.').to_ascii_lowercase())
            .collect();
        self
    }

    /// The pack carries no files besides the QML and those a module's
    /// build keeps inside the program (`RESOURCES`), which the QML names by
    /// their `qrc:` addresses whatever their kind.
    #[must_use]
    pub fn without_files(mut self) -> Self {
        self.files.clear();
        self
    }

    /// `source`, compiled from `dirs`, as a pack: what the module's
    /// functions write, for a build that compiled it some other way.
    /// The pack carries the files under `dirs` of the kinds this build
    /// takes ([`with_files`](Self::with_files)), each by its path from its
    /// directory's root as the QML is keyed (the first directory's file of
    /// a path wins), and those a module's build keeps inside the program
    /// (`RESOURCES`) of whatever kind; a file of a module is also at its
    /// `qrc:` address. Prints nothing for Cargo.
    pub fn pack(&self, source: &MapSource, dirs: &[&Path]) -> Result<Vec<u8>, BuildError> {
        let trailer = if self.sources {
            sources_of(dirs).encode(true)
        } else {
            Vec::new()
        };
        let mut source = source.clone();
        self.carry(&mut source, dirs)?;
        source
            .to_pack_with_trailer(&trailer)
            .map_err(BuildError::Pack)
    }

    /// Puts the files under `dirs` this build carries into `source`.
    fn carry(&self, source: &mut MapSource, dirs: &[&Path]) -> Result<(), BuildError> {
        let unread = |path: &Path| {
            let path = path.to_path_buf();
            move |source| BuildError::Read { path, source }
        };
        // By path from its root, where each file is and which of `dirs` it
        // was found under.
        let mut carried: BTreeMap<String, (PathBuf, usize)> = BTreeMap::new();
        let mut addresses = Vec::new();
        // Each module's `qrc:` address, with its directory's path and which
        // of `dirs` it is in.
        let mut modules = Vec::new();
        for (at, &dir) in dirs.iter().enumerate() {
            let root = std::fs::canonicalize(dir).map_err(unread(dir))?;
            // A file is the project's only where it really is under its
            // root: a link that leads out of it (`pics/dot.png` to a
            // private file elsewhere) would ship a file nobody chose.
            let key = |path: &Path| {
                let relative = path.strip_prefix(dir).ok()?;
                let plain = relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)));
                let inside = std::fs::canonicalize(path).is_ok_and(|real| real.starts_with(&root));
                let parts: Option<Vec<&str>> = relative.iter().map(|p| p.to_str()).collect();
                (plain && inside).then_some(parts?.join("/"))
            };
            let mut files = Vec::new();
            ljos_compiler::files_below_naming(dir, |_| true, &mut files)
                .map_err(|(path, source)| BuildError::Read { path, source })?;
            for path in files {
                let wanted = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| self.files.iter().any(|kind| x.eq_ignore_ascii_case(kind)));
                let hidden = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_none_or(|n| n.starts_with('.'));
                if let (true, false, Some(key)) = (wanted, hidden, key(&path)) {
                    carried.entry(key).or_insert((path, at));
                }
            }
            for (address, path) in ljos_compiler::resources(dir).map_err(unread(dir))? {
                if let Some(key) = key(&path) {
                    // Kept in the program by its build, whatever its kind.
                    carried.entry(key.clone()).or_insert((path, at));
                    addresses.push((address, key));
                }
            }
            for (prefix, module) in ljos_compiler::resource_dirs(dir).map_err(unread(dir))? {
                if let Some(under) = key(&module) {
                    modules.push((prefix, under, at));
                }
            }
        }
        for (prefix, under, at) in modules {
            let under = if under.is_empty() {
                under
            } else {
                format!("{under}/")
            };
            // A module's address names the files of its own directory: a
            // file at the same path under another of `dirs` is not its.
            for (key, (_, from)) in &carried {
                if let (Some(rest), true) = (key.strip_prefix(&under), *from == at) {
                    addresses.push((format!("{prefix}/{rest}"), key.clone()));
                }
            }
        }
        for (key, (path, _)) in carried {
            let bytes = std::fs::read(&path).map_err(unread(&path))?;
            source.insert_resource(key, bytes);
        }
        for (address, key) in addresses {
            source.insert_resource_address(address, key);
        }
        Ok(())
    }

    /// See [`compile_dirs`].
    pub fn compile_dirs(
        &self,
        dirs: &[&Path],
        register: impl Fn(&Engine),
        out: &Path,
    ) -> Result<(), BuildError> {
        let (source, _) = registered_source(dirs, register)?;
        self.write_pack(&source, dirs, out).map(drop)
    }

    /// See [`compile_dirs_to_rust`].
    pub fn compile_dirs_to_rust(
        &self,
        dirs: &[&Path],
        register: impl Fn(&Engine),
        out: &Path,
    ) -> Result<Vec<CompiledFile>, BuildError> {
        let (source, oracle) = registered_source(dirs, register)?;
        let pack = self.write_pack(&source, dirs, out)?;
        write_rust(&pack, &oracle, &out.with_extension("rs"))
    }

    /// See [`compile_dirs_with_types`].
    pub fn compile_dirs_with_types(
        &self,
        dirs: &[&Path],
        types: &Path,
        out: &Path,
    ) -> Result<(), BuildError> {
        let (source, _) = typed_source(dirs, types)?;
        self.write_pack(&source, dirs, out).map(drop)
    }

    /// See [`compile_dirs_with_types_to_rust`].
    pub fn compile_dirs_with_types_to_rust(
        &self,
        dirs: &[&Path],
        types: &Path,
        out: &Path,
    ) -> Result<Vec<CompiledFile>, BuildError> {
        let (source, oracle) = typed_source(dirs, types)?;
        let pack = self.write_pack(&source, dirs, out)?;
        write_rust(&pack, &oracle, &out.with_extension("rs"))
    }

    fn write_pack(
        &self,
        source: &MapSource,
        dirs: &[&Path],
        out: &Path,
    ) -> Result<MapSource, BuildError> {
        write_pack(&self.pack(source, dirs)?, out)
    }
}

/// The texts of the files under `dirs` a user's file may be compiled
/// against, keyed from each directory's root as the compile keys them
/// (the first directory's file of a path wins), with the style the
/// compile chose.
fn sources_of(dirs: &[&Path]) -> PackSources {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if !ljos_compiler::skipped_dir(name) {
                    walk(root, &path, files);
                }
                continue;
            }
            if name.starts_with('.') || !PackSources::carries(name) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let key: Vec<&str> = relative.iter().filter_map(|p| p.to_str()).collect();
            if let Ok(text) = std::fs::read_to_string(&path) {
                files.entry(key.join("/")).or_insert(text);
            }
        }
    }
    let mut files = BTreeMap::new();
    for dir in dirs {
        walk(dir, dir, &mut files);
    }
    PackSources {
        files,
        style: ljos_compiler::controls_style(
            |name| std::env::var(name).ok(),
            |path| std::fs::read_to_string(path).ok(),
        ),
    }
}

fn typed_source(dirs: &[&Path], types: &Path) -> Result<(MapSource, EngineOracle), BuildError> {
    for dir in dirs {
        rerun_if_changed(dir);
    }
    println!("cargo:rerun-if-changed={}", types.display());
    println!("cargo:rerun-if-env-changed={}", crate::types::WRITE_VAR);
    let writing = crate::types::writing();
    let path = || types.to_path_buf();
    let read = std::fs::read_to_string(types)
        .map_err(|source| BuildError::TypesUnread {
            path: path(),
            source,
        })
        .and_then(|text| {
            TypeDump::from_json(&text).map_err(|source| BuildError::TypesUnreadable {
                path: path(),
                source,
            })
        });
    let dump = match read {
        Ok(dump) => dump,
        Err(_) if writing => TypeDump {
            format: TypeDump::FORMAT,
            modules: Vec::new(),
            unnamed: Vec::new(),
        },
        Err(e) => return Err(e),
    };
    let engine = crate::engine();
    let oracle =
        EngineOracle::from_dump(&dump, &engine).map_err(|source| BuildError::NativeTypes {
            path: path(),
            source,
        })?;
    let source = match compile_with_oracle(dirs, oracle.clone(), OnError::Fail) {
        Ok(source) => source,
        Err(e) if writing => {
            for line in e.to_string().lines() {
                println!("cargo:warning={line}");
            }
            println!(
                "cargo:warning=packed as failing while {} is written; build again once it is",
                types.display()
            );
            compile_with_oracle(dirs, oracle.clone(), OnError::Record)?
        }
        Err(e) => return Err(e.into()),
    };
    Ok((source, oracle))
}

/// Writes the pack and hands back the files as the program will read
/// them from it: compiled code is matched to a file by the content of
/// what the program loads.
fn write_pack(pack: &[u8], out: &Path) -> Result<MapSource, BuildError> {
    std::fs::write(out, pack).map_err(|source| BuildError::Write {
        path: out.to_path_buf(),
        source,
    })?;
    MapSource::from_pack(pack).map_err(BuildError::Pack)
}

/// What the generator made of one file.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CompiledFile {
    /// The key the file is known by (`Main`).
    pub key: String,
    /// Its expressions that run compiled code.
    pub compiled: usize,
    /// Those that run dynamically, each with why: `(expression, reason)`.
    pub refused: Vec<(u32, String)>,
}

const RUNTIME: &str = "::ljos::aot";

fn write_rust(
    source: &MapSource,
    oracle: &EngineOracle,
    out: &Path,
) -> Result<Vec<CompiledFile>, BuildError> {
    use ljos_engine::ComponentSource as _;

    let mut generated = Vec::new();
    for key in source.keys() {
        if let Some(file) = source.load(key) {
            generated.push((
                key.to_owned(),
                ljos_compiler::aot::generate(&file, oracle, RUNTIME),
            ));
        }
    }
    let named: Vec<(&str, &ljos_aot::Generated)> =
        generated.iter().map(|(k, g)| (k.as_str(), g)).collect();
    let rust = ljos_aot::assemble(RUNTIME, &named);
    std::fs::write(out, rust).map_err(|source| BuildError::Write {
        path: out.to_path_buf(),
        source,
    })?;
    Ok(generated
        .iter()
        .map(|(key, g)| CompiledFile {
            key: key.clone(),
            compiled: g.compiled(),
            refused: g
                .outcomes
                .iter()
                .enumerate()
                .filter_map(|(i, o)| match o {
                    ljos_aot::Outcome::Refused(why) => Some((i as u32, why.to_string())),
                    ljos_aot::Outcome::Compiled => None,
                })
                .collect(),
        })
        .collect())
}

fn rerun_if_changed(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rerun_if_changed(&p);
        } else {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}
