//! `ljos run`: opens a window on a QML file. With `--watch` the project's
//! files are watched and every save is applied to the running application
//! with its state kept (ARCHITECTURE.md §12.1); what each reload did goes
//! to stderr. `--mcp` serves the inspector of the running application to
//! MCP clients on this machine (§19.4, §19.7), with the capabilities it is
//! given by name, and `--top` shows where the time of each frame went.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use clap::Args;
use ljos_engine::{Engine, LogEntry, LogLevel, MapSource};
use ljos_mcp_core::{Cap, CapSet};
use ljos_shell::{InspectorAccess, InspectorOptions, Report, Setup, SetupError, ShellConfig};

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
    /// Serve the running application to MCP clients on this machine, so
    /// that an agent can look at it and, if you say so, act on it. Each
    /// capability is granted by name: `--mcp=inspect,input,mutate,eval`.
    /// `inspect` reads (outline, objects, properties, a picture, the log,
    /// the frame times); `input` clicks, types, presses, scrolls, touches
    /// and moves a virtual clock; `mutate` assigns properties, calls
    /// methods and asks for a reload; `eval` runs script in the context of
    /// an object and is never implied by another capability. `--mcp` alone
    /// means `--mcp=inspect`. The server listens on a loopback port of the
    /// system's choosing. The address is printed to stderr; it and the
    /// token a client must show are written to `target/ljos/mcp-<pid>.json`
    /// (readable by you alone, removed when the window closes or on
    /// SIGINT or SIGTERM). The token itself is never printed.
    #[arg(
        long,
        value_name = "CAPS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "inspect",
        value_parser = parse_caps
    )]
    mcp: Option<CapSet>,
    /// With `--mcp`: mask the value of every editable field in what a
    /// client is told and shown, not only of password fields. `app_eval`
    /// is refused then, because a script reads values around the mask.
    #[arg(long, requires = "mcp")]
    mcp_redact: bool,
    /// With `--mcp`: listen on this address instead of a loopback port,
    /// such as `127.0.0.1:7600`. An address that is not this machine's
    /// also needs `--mcp-insecure-lan`.
    #[arg(
        long,
        requires = "mcp",
        value_name = "ADDR",
        conflicts_with = "mcp_socket"
    )]
    mcp_bind: Option<SocketAddr>,
    /// With `--mcp-bind` on an address beyond this machine: accept that
    /// anyone on the network who gets the token can drive the application.
    #[arg(long, requires = "mcp_bind")]
    mcp_insecure_lan: bool,
    /// With `--mcp`: listen on a Unix socket only you can open, instead
    /// of a port. The path may have at most 107 bytes on Linux (103 on
    /// macOS); a longer one is refused, never moved.
    #[arg(long, requires = "mcp", value_name = "PATH", value_parser = socket_path)]
    mcp_socket: Option<PathBuf>,
    /// Show the `top` overlay: the time of the last frame by lane and by
    /// owner. It is updated by the frames the application runs and keeps
    /// none coming.
    #[arg(long)]
    top: bool,
}

/// What `--mcp` may grant. `fs` and `run` belong to the authoring server
/// (`ljos mcp`), not to a running application, so they are refused here
/// and not silently left out. `inspect` has to be named with anything
/// else: the references every other tool takes come from it.
fn parse_caps(text: &str) -> Result<CapSet, String> {
    let caps = CapSet::parse_list(text).map_err(|e| e.to_string())?;
    let apart: Vec<String> = caps
        .iter()
        .filter(|c| matches!(c, Cap::Fs | Cap::Run))
        .map(|c| c.to_string())
        .collect();
    if !apart.is_empty() {
        return Err(format!(
            "{} belong to the authoring server (`ljos mcp`), not to a running application. \
             Use --mcp=inspect,input,mutate,eval, naming each capability to grant",
            apart.join(", ")
        ));
    }
    if !caps.contains(Cap::Inspect) {
        return Err(
            "`inspect` has to be named: the other tools take the references it \
             gives. Use --mcp or --mcp=inspect,input,mutate,eval"
                .to_owned(),
        );
    }
    Ok(caps)
}

/// What a run was asked to show a developer or an agent besides the
/// application itself.
#[derive(Clone, Debug)]
struct Tools {
    top: bool,
    inspector: Option<(InspectorOptions, InspectorAccess)>,
}

impl Tools {
    /// The log goes to the terminal, and to the inspector when there is
    /// one. Before the root is made, so that nothing it logs is missed.
    fn start(self, setup: &mut Setup<'_, ()>) -> Result<(), SetupError> {
        log_to_stderr(setup.engine());
        if self.top {
            setup.show_top();
        }
        if let Some((options, access)) = self.inspector {
            let granted = access
                .caps
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let at = setup.serve_inspector_as(options, access, print_log)?;
            // Said loudly and apart from the line below: it is the one
            // thing in the start that can hand the application to others.
            if let Some(warning) = &at.warning {
                eprintln!("ljos: WARNING: {warning}");
            }
            eprintln!(
                "ljos: inspector (--mcp={granted}) at {}, token in {}",
                at.url,
                at.token_file.display()
            );
        }
        Ok(())
    }
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
    engine.set_log_handler(print_log);
}

fn print_log(entry: &LogEntry) {
    let level = match entry.level {
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warning => "warning",
        LogLevel::Critical => "error",
    };
    eprintln!("{level}: {}", entry.message);
}

/// Refuses a socket path the system cannot bind before a window opens,
/// with the words of the inspector's own refusal.
fn socket_path(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    ljos_mcp_core::check_socket_path(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

impl Run {
    pub fn run(self) -> anyhow::Result<()> {
        let (dir, key) = entry(&self.file)?;
        let config = ShellConfig::new(format!("{key}.qml")).with_size(self.size.0, self.size.1);
        let tools = Tools {
            top: self.top,
            inspector: match self.mcp {
                // Below the directory `ljos` was started in, where a
                // client started in the same place looks.
                Some(caps) => Some((
                    InspectorOptions {
                        redact_all: self.mcp_redact,
                        ..InspectorOptions::below(
                            std::env::current_dir().context("the working directory")?,
                        )
                    },
                    InspectorAccess {
                        caps,
                        bind: self.mcp_bind,
                        socket: self.mcp_socket.clone(),
                        insecure_lan: self.mcp_insecure_lan,
                        // This process is the application's: a signal
                        // must not leave a token file naming a dead port.
                        clean_up_on_signals: true,
                    },
                )),
                None => None,
            },
        };
        if self.watch {
            let main = dir.join(format!("{key}.qml"));
            ljos_shell::run::<()>(config, move |setup| {
                tools.start(setup)?;
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
                tools.start(setup)?;
                let engine = setup.engine();
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

    #[derive(clap::Parser)]
    struct Line {
        #[command(flatten)]
        run: Run,
    }

    fn line(args: &[&str]) -> Result<Run, String> {
        use clap::Parser as _;
        Line::try_parse_from([&["ljos-run"], args].concat())
            .map(|l| l.run)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn the_inspector_and_the_overlay_are_off_unless_asked_for() {
        let run = line(&["Main.qml"]).unwrap();
        assert_eq!(run.mcp, None);
        assert!(!run.mcp_redact);
        assert!(!run.top);
        let run = line(&["Main.qml", "--top"]).unwrap();
        assert!(run.top);
        assert_eq!(run.mcp, None);
    }

    #[test]
    fn mcp_alone_grants_inspect() {
        let inspect = Some(CapSet::of([Cap::Inspect]));
        assert_eq!(line(&["Main.qml", "--mcp"]).unwrap().mcp, inspect);
        assert_eq!(line(&["--mcp", "Main.qml"]).unwrap().mcp, inspect);
        assert_eq!(line(&["Main.qml", "--mcp=inspect"]).unwrap().mcp, inspect);
    }

    #[test]
    fn every_capability_is_granted_by_name() {
        let all = CapSet::of([Cap::Inspect, Cap::Input, Cap::Mutate, Cap::Eval]);
        let run = line(&["Main.qml", "--mcp=inspect,input,mutate,eval"]).unwrap();
        assert_eq!(run.mcp, Some(all));
        let run = line(&["Main.qml", "--mcp=inspect,input"]).unwrap();
        assert_eq!(run.mcp, Some(CapSet::of([Cap::Inspect, Cap::Input])));
        // Eval is not implied by the others.
        let run = line(&["Main.qml", "--mcp=inspect,input,mutate"]).unwrap();
        assert!(!run.mcp.unwrap().contains(Cap::Eval));
    }

    #[test]
    fn a_capability_that_is_not_for_an_application_is_refused() {
        for apart in ["inspect,fs", "inspect,run", "fs"] {
            let e = line(&["Main.qml", &format!("--mcp={apart}")])
                .err()
                .unwrap_or_else(|| panic!("--mcp={apart} was taken"));
            assert!(e.contains("authoring server"), "{e}");
        }
        let e = line(&["Main.qml", "--mcp=input"]).err().unwrap();
        assert!(e.contains("`inspect` has to be named"), "{e}");
        let e = line(&["Main.qml", "--mcp=everything"]).err().unwrap();
        assert!(e.contains("everything"), "{e}");
        assert!(line(&["Main.qml", "--mcp="]).is_err());
    }

    #[test]
    fn redaction_is_asked_for_with_the_inspector() {
        let run = line(&["Main.qml", "--mcp", "--mcp-redact"]).unwrap();
        assert!(run.mcp_redact);
        let e = line(&["Main.qml", "--mcp-redact"]).err().unwrap();
        assert!(e.contains("--mcp"), "{e}");
    }

    #[test]
    fn the_address_is_chosen_with_the_inspector_and_the_lan_word_needs_it() {
        let run = line(&["Main.qml", "--mcp", "--mcp-bind=127.0.0.1:7600"]).unwrap();
        assert_eq!(run.mcp_bind, Some("127.0.0.1:7600".parse().unwrap()));
        assert!(!run.mcp_insecure_lan);
        let run = line(&[
            "Main.qml",
            "--mcp",
            "--mcp-bind=0.0.0.0:7600",
            "--mcp-insecure-lan",
        ]);
        assert!(run.unwrap().mcp_insecure_lan);
        assert!(line(&["Main.qml", "--mcp-bind=127.0.0.1:7600"]).is_err());
        let e = line(&["Main.qml", "--mcp", "--mcp-insecure-lan"])
            .err()
            .unwrap();
        assert!(e.contains("--mcp-bind"), "{e}");
        assert!(line(&["Main.qml", "--mcp", "--mcp-bind=lan"]).is_err());
    }

    #[test]
    fn a_socket_and_an_address_are_two_ways_to_listen() {
        let run = line(&["Main.qml", "--mcp", "--mcp-socket=/run/user/1/ljos.sock"]).unwrap();
        assert_eq!(run.mcp_socket, Some(PathBuf::from("/run/user/1/ljos.sock")));
        let both = line(&[
            "Main.qml",
            "--mcp",
            "--mcp-socket=/run/user/1/ljos.sock",
            "--mcp-bind=127.0.0.1:7600",
        ]);
        assert!(both.is_err());
        assert!(line(&["Main.qml", "--mcp-socket=/run/user/1/ljos.sock"]).is_err());
    }

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
