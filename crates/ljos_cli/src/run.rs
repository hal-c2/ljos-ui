//! `ljos run`: opens a window on a QML file. With `--watch` the project's
//! files are watched and every save is applied to the running application
//! with its state kept (ARCHITECTURE.md §12.1); what each reload did goes
//! to stderr. `--mcp` serves the inspector of the running application to
//! MCP clients on this machine (§19.4, §19.7), with the capabilities it is
//! given by name, and `--top` shows where the time of each frame went.

use std::cell::RefCell;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context as _, bail};
use clap::Args;
use ljos::dev::EngineOracle;
use ljos_engine::{Engine, LogEntry, LogLevel};
use ljos_mcp_core::{Cap, CapSet};
use ljos_shell::{InspectorAccess, InspectorOptions, Report, Setup, SetupError, ShellConfig};

use crate::demand::{OnDemand, Say};

#[derive(Args)]
pub struct Run {
    /// The entry file, such as `ui/Main.qml`. Its directory is the project,
    /// unless `--project` names another.
    file: PathBuf,
    /// The project's directory, when the entry is below it rather than in
    /// it: a project whose entry is `MediaPlayer/Main.qml` and whose other
    /// modules are directories beside it (`MediaControls/`), as Qt's
    /// launcher adds the application's directory to the import path.
    #[arg(long, value_name = "DIR")]
    project: Option<PathBuf>,
    /// Watch the project's files and apply every save to the running
    /// application, keeping its state.
    #[arg(long)]
    watch: bool,
    /// A picture the desktop shows for the application: its windows'
    /// icon, and the Dock's on macOS. Without it the desktop's own is
    /// shown.
    #[arg(long, value_name = "FILE")]
    icon: Option<PathBuf>,
    /// The window's size in logical pixels. Without it, a root that is a
    /// plain item opens at its own size, as `qml6` opens it, and 800x600
    /// for a side it leaves at 0.
    #[arg(long, value_parser = parse_size, value_name = "WxH")]
    size: Option<(f32, f32)>,
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
    /// A gallery example id (of `corpus/examples.json`) whose native
    /// backends to register on the Engine before the project is compiled
    /// and loaded, as `ljos render --example` does, so that an example
    /// whose types are not QML can be run whole. Without it, nothing is
    /// registered. With `--watch` the types are registered too, but a
    /// save is recompiled against the table of types alone, which does
    /// not know them.
    #[arg(long, value_name = "ID")]
    example: Option<String>,
    /// What the application is given after `--`, in `Application.arguments`
    /// after the program and the entry file. `-style NAME` among them
    /// chooses the controls style before `QT_QUICK_CONTROLS_STYLE` and the
    /// configuration file do, and is taken out, as Qt takes it out.
    #[arg(last = true, value_name = "ARGS")]
    args: Vec<String>,
}

/// The program as it was started, which a command line begins with.
pub(crate) fn program() -> String {
    std::env::args().next().unwrap_or_else(|| "ljos".to_owned())
}

/// What `--mcp` may grant. `fs` and `run` belong to the authoring server
/// (`ljos mcp`), not to a running application, so they are refused here
/// and not silently left out. `inspect` has to be named with anything
/// else: the references every other tool takes come from it.
pub(crate) fn parse_caps(text: &str) -> Result<CapSet, String> {
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
                crate::stderr::line(&format!("ljos: WARNING: {warning}"));
            }
            crate::stderr::line(&format!(
                "ljos: inspector (--mcp={granted}) at {}, token in {}",
                at.url,
                at.token_file.display()
            ));
        }
        Ok(())
    }
}

pub(crate) fn parse_size(text: &str) -> Result<(f32, f32), String> {
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

/// The project's directory `project` and the key of the entry file in it.
fn entry_in(file: &Path, project: &Path) -> anyhow::Result<(PathBuf, String)> {
    let (_, name) = entry(file)?;
    let path = file
        .canonicalize()
        .with_context(|| format!("{}", file.display()))?;
    let dir = project
        .canonicalize()
        .with_context(|| format!("{}", project.display()))?;
    let Ok(below) = path.parent().unwrap_or(&path).strip_prefix(&dir) else {
        bail!("{} is not in {}", file.display(), project.display());
    };
    let mut key = String::new();
    for part in below.components() {
        let Some(part) = part.as_os_str().to_str() else {
            bail!("{} is not a path a key can name", below.display());
        };
        key.push_str(part);
        key.push('/');
    }
    key.push_str(&name);
    Ok((dir, key))
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

/// The project of a run that does not watch, as a program loads it: the
/// entry is compiled now, and each other file when the program first
/// makes an object of it, which tells `say` its warnings then. A file the
/// entry never reaches is not compiled and says nothing. A file with a
/// mistake is said to be one to whatever makes an object of it, so only
/// the entry's own mistake stops the run. With an example, its registered
/// backends are types of the project too (see `ljos render --example`).
fn compile(
    dir: &Path,
    key: &str,
    example: Option<&str>,
    say: Say,
) -> anyhow::Result<OnDemand<EngineOracle>> {
    let oracle = match example {
        Some(id) => crate::render::oracle_for_example(id),
        None => EngineOracle::table_only(),
    };
    OnDemand::open(dir, key, oracle, say)
}

/// What the compile thread made; its panic goes on here.
fn compiled(
    thread: std::thread::JoinHandle<anyhow::Result<OnDemand<EngineOracle>>>,
) -> anyhow::Result<OnDemand<EngineOracle>> {
    thread
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
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
            if let Some(shown) = s.shown {
                line.push_str(&format!("; shown {:.1} ms after the save", ms(shown)));
            }
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
        Report::Pictures { shown: 1 } => "1 image shows the saved picture".to_owned(),
        Report::Pictures { shown } => format!("{shown} images show the saved pictures"),
        // One line each, as a run that does not watch tells them.
        Report::Warnings(warnings) => warnings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\nljos: "),
    }
}

/// What the application logs (`console.log`, the engine's warnings) goes
/// to the terminal, as a developer expects of a run from one.
fn log_to_stderr(engine: &Engine) {
    engine.set_log_handler(print_log);
}

pub(crate) fn print_log(entry: &LogEntry) {
    let level = match entry.level {
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warning => "warning",
        LogLevel::Critical => "error",
    };
    // Not from this thread: a write to stderr can block, and this runs
    // inside the script that logged.
    crate::stderr::line(&format!("{level}: {}", entry.message));
}

/// The style `-style NAME`, `-style=NAME` or `--style NAME` among a
/// program's arguments names, the last if it is named twice, and the
/// arguments without it.
fn style_argument(args: &[String]) -> (Option<String>, Vec<String>) {
    let (mut style, mut rest) = (None, Vec::new());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(name) = ["-style=", "--style="]
            .iter()
            .find_map(|p| arg.strip_prefix(p))
        {
            style = Some(name.to_owned());
        } else if arg == "-style" || arg == "--style" {
            match args.next() {
                Some(name) => style = Some(name.clone()),
                None => rest.push(arg.clone()),
            }
        } else {
            rest.push(arg.clone());
        }
    }
    (style, rest)
}

/// Refuses a socket path the system cannot bind before a window opens,
/// with the words of the inspector's own refusal.
fn socket_path(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    ljos_mcp_core::check_socket_path(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

impl Run {
    /// `Application.arguments`: as `qml6 FILE ARGS` gives them, the
    /// program, the file as it was given and the rest; `run`, its options
    /// and the `--` are left out, as qml6 leaves out its own options.
    fn arguments(&self) -> Vec<String> {
        [program(), self.file.display().to_string()]
            .into_iter()
            .chain(style_argument(&self.args).1)
            .collect()
    }

    pub fn run(self) -> anyhow::Result<()> {
        if let Some(style) = style_argument(&self.args).0 {
            // The compiler reads the variable, here and on every save
            // `--watch` applies; `-style` comes before it, as in Qt.
            // SAFETY: no other thread runs yet: the font reader starts
            // below and the stderr writer at the first line written.
            unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", style) };
        }
        // While the project is compiled. It chooses the graphics drivers,
        // which sets their variables, before it starts the font reader.
        ljos_shell::read_system_fonts_early();
        let (dir, key) = match &self.project {
            Some(project) => entry_in(&self.file, project)?,
            None => entry(&self.file)?,
        };
        let config = ShellConfig::new(format!("{key}.qml"));
        let config = match self.size {
            Some((w, h)) => config.with_size(w, h),
            None => config.root_sized(),
        };
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
        let example = self.example.clone();
        let arguments = self.arguments();
        let icon = self.icon.as_deref().map(|path| {
            let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
            ljos_engine::file_url(&path)
        });
        if self.watch {
            let main = dir.join(format!("{key}.qml"));
            let project = self.project.is_some().then(|| dir.clone());
            let resources = dir.clone();
            ljos_shell::run::<()>(config, move |setup| {
                // Every module qml6 would have; databases where Qt keeps them.
                crate::render::register_modules(setup.engine());
                crate::render::register_example(setup.engine(), example.as_deref());
                tools.start(setup)?;
                if let Some(icon) = &icon {
                    ljos::window::load_application_icon(setup.engine(), icon);
                }
                ljos_controls::set_application_arguments(setup.engine(), arguments);
                // As the program is built with them; a save of one is
                // shown again like a save of any picture.
                crate::render::stage_resources(setup.engine(), &resources);
                setup.on_reload(|report| {
                    crate::stderr::line(&format!("ljos: {}", describe(report)))
                });
                // The compiler knows what the engine was given above.
                let oracle = ljos::dev::EngineOracle::from_engine(setup.engine());
                let root = setup.watch_with(
                    &main,
                    project.as_deref(),
                    crate::render::example_properties(example.as_deref()),
                    oracle,
                )?;
                // A project that does not compile opens all the same: the
                // window says why, and so does the terminal.
                if let Some(fault) = setup.engine().last_reload_error() {
                    crate::stderr::line(&format!("ljos: {fault}"));
                }
                crate::stderr::line(&format!("ljos: watching {}", dir.display()));
                Ok(root)
            })?;
        } else {
            // The project compiles while the window system and the drivers
            // start. It still stops the run before a window opens: windows
            // open after `setup`, which waits for it.
            let stop = Arc::new(AtomicBool::new(false));
            let compiling = {
                let (dir, key, example) = (dir.clone(), key.clone(), example.clone());
                let stop = Arc::clone(&stop);
                std::thread::Builder::new()
                    .name("ljos-compile".into())
                    .spawn(move || {
                        let source = compile(
                            &dir,
                            &key,
                            example.as_deref(),
                            Box::new(crate::stderr::line),
                        )?;
                        source.compile_ahead(&key, &stop);
                        Ok(source)
                    })
                    .context("a thread to compile on")?
            };
            let compiling = Rc::new(RefCell::new(Some(compiling)));
            let failed = Rc::new(RefCell::new(None));
            let (waiting, failing) = (Rc::clone(&compiling), Rc::clone(&failed));
            let ran = ljos_shell::run::<()>(config, move |setup| {
                stop.store(true, Ordering::Relaxed);
                let source = match waiting.borrow_mut().take().map(compiled) {
                    Some(Ok(source)) => source,
                    Some(Err(error)) => {
                        *failing.borrow_mut() = Some(error);
                        return Err("the project did not compile".into());
                    }
                    None => unreachable!("setup runs once"),
                };
                // Every module qml6 would have; databases where Qt keeps them.
                crate::render::register_modules(setup.engine());
                crate::render::register_example(setup.engine(), example.as_deref());
                tools.start(setup)?;
                let engine = setup.engine();
                if let Some(icon) = &icon {
                    ljos::window::load_application_icon(engine, icon);
                }
                ljos_controls::set_application_arguments(engine, arguments);
                crate::render::stage(engine, &source.resources());
                engine.set_component_source(source);
                Ok(engine
                    .create_from_url(
                        &key,
                        None,
                        crate::render::example_properties(example.as_deref()),
                    )
                    .map_err(|e| e.to_string())?)
            });
            // Its error first, as when it stopped the run before the window
            // system was asked for anything.
            if let Some(error) = failed.borrow_mut().take() {
                return Err(error);
            }
            if let Some(Err(error)) = compiling.borrow_mut().take().map(compiled) {
                return Err(error);
            }
            ran?;
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
    fn an_example_is_named_or_absent() {
        assert_eq!(line(&["Main.qml"]).unwrap().example, None);
        assert_eq!(
            line(&["Main.qml", "--example", "rssnews"]).unwrap().example,
            Some("rssnews".to_owned())
        );
    }

    #[test]
    fn the_application_is_given_the_file_and_what_follows_the_dashes() {
        let run = line(&["--watch", "photos/Main.qml", "--", "/pictures", "-v"]).unwrap();
        assert_eq!(
            run.arguments(),
            [program().as_str(), "photos/Main.qml", "/pictures", "-v"]
        );
        let run = line(&["Main.qml"]).unwrap();
        assert_eq!(run.arguments(), [program().as_str(), "Main.qml"]);
        // What is not after `--` is still `run`'s.
        assert!(line(&["Main.qml", "/pictures"]).is_err());
    }

    /// As qml6 takes `-style` out of what the application is given.
    #[test]
    fn a_style_on_the_command_line_is_chosen_and_taken_out() {
        let run = line(&["Main.qml", "--", "x", "-style", "Basic", "y"]).unwrap();
        assert_eq!(run.arguments(), [program().as_str(), "Main.qml", "x", "y"]);
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        for form in [
            &["-style", "Basic"][..],
            &["-style=Basic"],
            &["--style", "Basic"],
            &["--style=Basic"],
        ] {
            assert_eq!(
                style_argument(&args(form)),
                (Some("Basic".to_owned()), vec![]),
                "{form:?}"
            );
        }
        assert_eq!(
            style_argument(&args(&["-style", "Material", "-style=Basic"])).0,
            Some("Basic".to_owned())
        );
        // Another word that begins like it is the application's.
        assert_eq!(
            style_argument(&args(&["-stylex", "-style"])),
            (None, args(&["-stylex", "-style"]))
        );
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
    fn an_entry_below_the_project_is_keyed_by_its_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("App")).unwrap();
        std::fs::write(dir.path().join("App/Main.qml"), "import QtQuick\nItem {}\n").unwrap();
        let (root, key) = entry_in(&dir.path().join("App/Main.qml"), dir.path()).unwrap();
        assert_eq!(root, dir.path().canonicalize().unwrap());
        assert_eq!(key, "App/Main");
        let other = tempfile::tempdir().unwrap();
        assert!(entry_in(&dir.path().join("App/Main.qml"), other.path()).is_err());
    }

    fn quiet() -> Say {
        Box::new(|_| {})
    }

    fn write(dir: &Path, files: &[(&str, &str)]) {
        for (name, text) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }

    #[test]
    fn a_run_compiles_and_tells_only_what_its_entry_reaches() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &[
                (
                    "Main.qml",
                    "import QtQuick\nimport \"parts\"\nimport Widgets\nItem {\n  \
                     property alias page: loader.item\n  \
                     Sibling { }\n  Panel { objectName: \"panel\" }\n  Gauge { }\n  \
                     Loader { id: loader; source: \"pages/Page.qml\" }\n}\n",
                ),
                (
                    "Sibling.qml",
                    "import QtQuick\nItem {\n  property var v: siblingUnknown\n}\n",
                ),
                // Beside the entry, with a mistake, and nothing uses it.
                (
                    "Broken.qml",
                    "import QtQuick\nItem {\n  NoSuchThing { }\n}\n",
                ),
                ("parts/Panel.qml", "import QtQuick\nItem { }\n"),
                ("widgets/qmldir", "module Widgets\nGauge 1.0 Gauge.qml\n"),
                ("widgets/Gauge.qml", "import QtQuick\nItem { }\n"),
                (
                    "pages/Page.qml",
                    "import QtQuick\nItem {\n  objectName: \"page\"\n  property var v: pageUnknown\n}\n",
                ),
                // Below the entry, and nothing imports it.
                (
                    "stray/Stray.qml",
                    "import QtQuick\nItem {\n  property var v: strayUnknown\n}\n",
                ),
            ],
        );
        let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let to = said.clone();
        let source = compile(
            dir.path(),
            "Main",
            None,
            Box::new(move |line| to.lock().unwrap().push(line.to_owned())),
        )
        .unwrap();
        // Nothing but the entry is compiled before an object is made, and
        // only what it imports is read.
        assert!(said.lock().unwrap().is_empty(), "{said:?}");
        let read = ["Broken", "Main", "Sibling", "parts/Panel", "widgets/Gauge"];
        assert_eq!(source.read(), read);
        let engine = Engine::new();
        ljos_quick::register(&engine);
        let source = std::rc::Rc::new(source);
        engine.set_component_source(Shared(source.clone()));
        let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
        // The page's directory is read when the Loader asks for it.
        let read = [
            "Broken",
            "Main",
            "Sibling",
            "pages/Page",
            "parts/Panel",
            "widgets/Gauge",
        ];
        assert_eq!(source.read(), read);
        let page = engine.read_by_name(root, "page").as_object().unwrap();
        let name = engine.read_by_name(page, "objectName");
        assert_eq!(name.to_js_string().to_string(), "page");
        let said = said.lock().unwrap();
        let told = |file: &str| said.iter().any(|line| line.starts_with(file));
        assert!(told("Sibling.qml: 3:"), "{said:?}");
        assert!(told("pages/Page.qml: 4:"), "{said:?}");
        assert!(!told("stray/") && !told("Broken"), "{said:?}");
        assert_eq!(said.len(), 2, "{said:?}");
    }

    #[test]
    fn what_is_compiled_ahead_is_told_when_the_engine_asks_for_it() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &[
                ("Main.qml", "import QtQuick\nItem {\n  Card { }\n}\n"),
                (
                    "Card.qml",
                    "import QtQuick\nItem {\n  property var v: cardUnknown\n  Badge { }\n}\n",
                ),
                (
                    "Badge.qml",
                    "import QtQuick\nItem {\n  property var v: badgeUnknown\n}\n",
                ),
                // Beside them, and nothing makes one.
                (
                    "Spare.qml",
                    "import QtQuick\nItem {\n  property var v: spareUnknown\n}\n",
                ),
            ],
        );
        let stopped = compile(dir.path(), "Main", None, quiet()).unwrap();
        stopped.compile_ahead("Main", &AtomicBool::new(true));
        assert!(stopped.ahead().is_empty());

        let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let to = said.clone();
        let source = compile(
            dir.path(),
            "Main",
            None,
            Box::new(move |line| to.lock().unwrap().push(line.to_owned())),
        )
        .unwrap();
        source.compile_ahead("Main", &AtomicBool::new(false));
        assert_eq!(source.ahead(), ["Badge", "Card"]);
        assert!(said.lock().unwrap().is_empty(), "{said:?}");

        let engine = Engine::new();
        ljos_quick::register(&engine);
        let source = std::rc::Rc::new(source);
        engine.set_component_source(Shared(source.clone()));
        engine.create_from_url("Main", None, Vec::new()).unwrap();
        assert!(source.ahead().is_empty());
        let said = said.lock().unwrap();
        let told = |file: &str| said.iter().any(|line| line.starts_with(file));
        assert!(told("Card.qml: 3:") && told("Badge.qml: 3:"), "{said:?}");
        assert!(!told("Spare"), "{said:?}");
    }

    /// The source as the engine has it, still in the test's hands.
    struct Shared(std::rc::Rc<OnDemand<EngineOracle>>);

    impl ljos_engine::ComponentSource for Shared {
        fn load(&self, key: &str) -> Option<std::sync::Arc<ljos_ir::CompiledFile>> {
            self.0.load(key)
        }
        fn failure(&self, key: &str) -> Option<String> {
            self.0.failure(key)
        }
        fn url(&self, key: &str) -> String {
            self.0.url(key)
        }
        fn key(&self, url: &str) -> String {
            self.0.key(url)
        }
    }

    #[test]
    fn a_project_with_a_mistake_in_its_entry_says_where() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Main.qml"),
            "import QtQuick\nItem {\n  width: 20 +\n}\n",
        )
        .unwrap();
        let e = compile(dir.path(), "Main", None, quiet())
            .unwrap_err()
            .to_string();
        assert!(e.contains("Main.qml:"), "{e}");
        std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem {}\n").unwrap();
        assert!(compile(dir.path(), "Main", None, quiet()).is_ok());
        // An example without a backend compiles as before.
        assert!(compile(dir.path(), "Main", Some("no-such-example"), quiet()).is_ok());
        assert!(compile(dir.path(), "Gone", None, quiet()).is_err());
    }

    #[test]
    fn a_file_with_a_mistake_that_nothing_makes_does_not_stop_the_run() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem {}\n").unwrap();
        std::fs::write(
            dir.path().join("Broken.qml"),
            "import QtQuick\nItem {\n  NoSuchThing { }\n}\n",
        )
        .unwrap();
        let source = compile(dir.path(), "Main", None, quiet()).unwrap();
        let engine = Engine::new();
        ljos_quick::register(&engine);
        engine.set_component_source(source);
        assert!(engine.create_from_url("Main", None, Vec::new()).is_ok());
        let e = engine
            .create_from_url("Broken", None, Vec::new())
            .unwrap_err()
            .to_string();
        assert!(e.contains("Broken.qml:3:"), "{e}");
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
            saved: std::time::Instant::now(),
            quiet: Duration::from_micros(15_000),
            waited: Duration::ZERO,
            shown: Some(Duration::from_micros(48_300)),
        }));
        assert_eq!(
            line,
            "reloaded in 12.3 ms (compile 9.1 ms, apply 3.2 ms): 41 kept, 2 new, 0 made again; \
             shown 48.3 ms after the save"
        );
        assert_eq!(describe(&Report::Cleared), "the files are what runs again");
        assert_eq!(
            describe(&Report::Pictures { shown: 2 }),
            "2 images show the saved pictures"
        );
        let warning = |line| ljos_compiler::CompileFault {
            file: "Used.qml".to_owned(),
            line,
            column: 19,
            message: "`usedUnknown` is not a name in sight".to_owned(),
        };
        assert_eq!(
            describe(&Report::Warnings(vec![warning(3), warning(4)])),
            "Used.qml:3:19: `usedUnknown` is not a name in sight\n\
             ljos: Used.qml:4:19: `usedUnknown` is not a name in sight"
        );
    }
}
