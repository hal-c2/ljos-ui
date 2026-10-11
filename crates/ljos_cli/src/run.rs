//! `ljos run`: opens a window on a QML file. With `--watch` the project's
//! files are watched and every save is applied to the running application
//! with its state kept (ARCHITECTURE.md §12.1); what each reload did goes
//! to stderr.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use clap::Args;
use ljos_engine::{Engine, LogLevel, MapSource};
use ljos_shell::{Report, ShellConfig};

#[derive(Args)]
pub struct Run {
    /// The entry file, such as `ui/Main.qml`. Its directory is the project.
    file: PathBuf,
    /// Watch the project's files and apply every save to the running
    /// application, keeping its state.
    #[arg(long)]
    watch: bool,
    /// The window's size in logical pixels.
    #[arg(long, value_parser = parse_size, default_value = "800x600", value_name = "WxH")]
    size: (f32, f32),
}

fn parse_size(text: &str) -> Result<(f32, f32), String> {
    let wrong = || format!("{text:?} is not a size such as 800x600");
    let (w, h) = text.split_once(['x', 'X']).ok_or_else(wrong)?;
    let side = |s: &str| {
        s.trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 1.0)
            .ok_or_else(wrong)
    };
    Ok((side(w)?, side(h)?))
}

/// The project's directory and the entry file's key in it.
fn entry(file: &Path) -> anyhow::Result<(PathBuf, String)> {
    let path = file
        .canonicalize()
        .with_context(|| format!("{}", file.display()))?;
    let key = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".qml"))
        .filter(|_| path.is_file());
    match (path.parent(), key) {
        (Some(dir), Some(key)) => Ok((dir.to_owned(), key.to_owned())),
        _ => bail!("{} is not a QML file", file.display()),
    }
}

/// Compiles the project once, for a run that does not watch. Errors are
/// the compiler's, one a line.
fn compile(dir: &Path) -> anyhow::Result<MapSource> {
    let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
    let mut source = MapSource::new().with_base_url(base);
    let mut errors = Vec::new();
    for r in ljos_compiler::compile_project(dir).with_context(|| format!("{}", dir.display()))? {
        match r.result {
            Ok(c) => {
                for d in &c.diagnostics {
                    if d.severity == ljos_compiler::Severity::Error {
                        errors.push(format!("{}.qml: {d}", r.key));
                    } else {
                        eprintln!("{}.qml: {d}", r.key);
                    }
                }
                source.insert(r.key, c.file);
            }
            Err(ds) => errors.extend(ds.iter().map(|d| format!("{}.qml: {d}", r.key))),
        }
    }
    if !errors.is_empty() {
        bail!("{} does not compile:\n{}", dir.display(), errors.join("\n"));
    }
    Ok(source)
}

/// One line for the developer's terminal about what the watcher brought.
pub fn describe(report: &Report) -> String {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    match report {
        Report::Reloaded(s) => {
            let mut line = format!(
                "reloaded in {:.1} ms (compile {:.1} ms, apply {:.1} ms): {} kept, {} new, {} made again",
                ms(s.build + s.apply),
                ms(s.build),
                ms(s.apply),
                s.kept,
                s.fresh,
                s.remounted
            );
            for w in &s.warnings {
                line.push_str(&format!("\n  {w}"));
            }
            line
        }
        Report::Failed { fault, more: 0 } => format!("reload failed: {fault}"),
        Report::Failed { fault, more } => {
            format!("reload failed: {fault} (and {more} more)")
        }
        Report::Cleared => "the files are what runs again".to_owned(),
    }
}

/// What the application logs (`console.log`, the engine's warnings) goes
/// to the terminal, as a developer expects of a run from one.
fn log_to_stderr(engine: &Engine) {
    engine.set_log_handler(|entry| {
        let level = match entry.level {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Critical => "error",
        };
        eprintln!("{level}: {}", entry.message);
    });
}

impl Run {
    pub fn run(self) -> anyhow::Result<()> {
        let (dir, key) = entry(&self.file)?;
        let config = ShellConfig::new(format!("{key}.qml")).with_size(self.size.0, self.size.1);
        if self.watch {
            let main = dir.join(format!("{key}.qml"));
            ljos_shell::run::<()>(config, move |setup| {
                log_to_stderr(setup.engine());
                setup.on_reload(|report| eprintln!("ljos: {}", describe(report)));
                let root = setup.watch(&main)?;
                // A project that does not compile opens all the same: the
                // window says why, and so does the terminal.
                if let Some(fault) = setup.engine().last_reload_error() {
                    eprintln!("ljos: {fault}");
                }
                eprintln!("ljos: watching {}", dir.display());
                Ok(root)
            })?;
        } else {
            let source = compile(&dir)?;
            ljos_shell::run::<()>(config, move |setup| {
                let engine = setup.engine();
                log_to_stderr(engine);
                engine.set_component_source(source);
                Ok(engine
                    .create_from_url(&key, None, Vec::new())
                    .map_err(|e| e.to_string())?)
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ljos_shell::ReloadStats;

    use super::*;

    #[test]
    fn sizes_are_width_by_height() {
        assert_eq!(parse_size("800x600"), Ok((800.0, 600.0)));
        assert_eq!(parse_size("320X200"), Ok((320.0, 200.0)));
        assert!(parse_size("800").is_err());
        assert!(parse_size("0x600").is_err());
        assert!(parse_size("wide x tall").is_err());
    }

    #[test]
    fn the_entry_is_a_qml_file_in_its_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem {}\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        let (root, key) = entry(&dir.path().join("Main.qml")).unwrap();
        assert_eq!(root, dir.path().canonicalize().unwrap());
        assert_eq!(key, "Main");
        assert!(entry(&dir.path().join("notes.txt")).is_err());
        assert!(entry(&dir.path().join("Gone.qml")).is_err());
        assert!(entry(dir.path()).is_err());
    }

    #[test]
    fn a_project_with_a_mistake_says_where() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Main.qml"),
            "import QtQuick\nItem {\n  width: 20 +\n}\n",
        )
        .unwrap();
        let e = compile(dir.path()).unwrap_err().to_string();
        assert!(e.contains("Main.qml: "), "{e}");
        std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem {}\n").unwrap();
        assert!(compile(dir.path()).is_ok());
    }

    #[test]
    fn a_reload_is_told_in_one_line() {
        let line = describe(&Report::Reloaded(ReloadStats {
            generation: 2,
            build: Duration::from_micros(9100),
            apply: Duration::from_micros(3200),
            kept: 41,
            fresh: 2,
            remounted: 0,
            warnings: Vec::new(),
        }));
        assert_eq!(
            line,
            "reloaded in 12.3 ms (compile 9.1 ms, apply 3.2 ms): 41 kept, 2 new, 0 made again"
        );
        assert_eq!(describe(&Report::Cleared), "the files are what runs again");
    }
}
