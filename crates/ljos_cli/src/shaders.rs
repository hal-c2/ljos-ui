//! `ljos shaders`: compile a project's `ShaderEffect` shaders at build
//! time (ARCHITECTURE.md section 7.1, "As built: shader effects").
//!
//! Each GLSL 440 `X.vert` or `X.frag` gets `X.vert.ljsh` or `X.frag.ljsh`
//! beside it, which an application built without `ljos_quick`'s
//! `shader-compile` feature loads where QML names `X.frag.qsb`.
//!
//! | To | What |
//! |---|---|
//! | stdout | `compiled <file>` (or `would compile <file>`) for each artifact missing or out of date, then `N shaders, M compiled` |
//! | stderr | `<file>: error: <message>` for a shader that does not compile; its artifact is left as it was |
//!
//! | Status | Means |
//! |---|---|
//! | 0 | Every artifact is up to date (or now is). |
//! | 1 | `--check` found an artifact missing or out of date. |
//! | 2 | A shader does not compile or a file could not be read or written; the others were still handled. |

use std::path::{Path, PathBuf};

use clap::Args;
use ljos_shader::{Compiled, Stage, artifact_path};

use crate::fmt::{shown, skipped};

#[derive(Args)]
pub struct Shaders {
    /// The shaders to compile, or directories for every `.vert` and
    /// `.frag` file below them.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Write nothing; name the artifacts that are missing or out of date
    /// and exit with status 1 if there is one.
    #[arg(long)]
    check: bool,
}

/// The stage a file's suffix names, if it is a shader.
fn stage(path: &Path) -> Option<Stage> {
    match path.extension()?.to_str()? {
        "vert" => Some(Stage::Vertex),
        "frag" => Some(Stage::Fragment),
        _ => None,
    }
}

/// Every shader below `dir`, in the order of their names. Links to
/// directories are not entered: they can loop.
fn shader_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !skipped(&entry.file_name().to_string_lossy()) {
                shader_files(&path, out)?;
            }
        } else if stage(&path).is_some() && path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[derive(Default)]
struct Tally {
    files: usize,
    /// Artifacts missing or out of date (and written, unless checking).
    changed: usize,
    failed: usize,
}

impl Shaders {
    /// Does the work and gives the exit status.
    pub fn run(&self) -> i32 {
        let mut tally = Tally::default();
        for path in &self.paths {
            if path.is_dir() {
                let mut files = Vec::new();
                if let Err(e) = shader_files(path, &mut files) {
                    eprintln!("{}: error: {e}", shown(path));
                    tally.failed += 1;
                }
                for file in files {
                    self.file(&file, &mut tally);
                }
            } else {
                self.file(path, &mut tally);
            }
        }
        let verb = if self.check {
            "would be compiled"
        } else {
            "compiled"
        };
        println!("{} shaders, {} {verb}", tally.files, tally.changed);
        if tally.failed > 0 {
            2
        } else if self.check && tally.changed > 0 {
            1
        } else {
            0
        }
    }

    fn file(&self, path: &Path, tally: &mut Tally) {
        let name = shown(path);
        let Some(stage) = stage(path) else {
            eprintln!("{name}: error: not a .vert or .frag file");
            tally.failed += 1;
            return;
        };
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("{name}: error: {e}");
                tally.failed += 1;
                return;
            }
        };
        tally.files += 1;
        let compiled = match ljos_shader::compile(&source, stage) {
            Ok(compiled) => compiled,
            Err(e) => {
                eprintln!("{name}: error: {e}");
                tally.failed += 1;
                return;
            }
        };
        let artifact = artifact_path(path);
        let up_to_date = std::fs::read_to_string(&artifact)
            .ok()
            .and_then(|text| Compiled::from_artifact(&text).ok())
            .is_some_and(|old| old == compiled);
        if up_to_date {
            return;
        }
        if self.check {
            println!("would compile {name}");
        } else if let Err(e) = std::fs::write(&artifact, compiled.to_artifact()) {
            eprintln!("{}: error: {e}", shown(&artifact));
            tally.failed += 1;
            return;
        } else {
            println!("compiled {name}");
        }
        tally.changed += 1;
    }
}
