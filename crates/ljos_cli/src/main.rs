//! The `ljos` command line. `run`, `check` and `dump` arrive in later milestones;
//! each becomes another variant of [`Command`].

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use anyhow::Context;
use clap::{Parser, Subcommand};
use ljos_mcp_author::{AuthorServer, Project, QmlFrontEnd};
use ljos_mcp_core::CapSet;

mod bench;
mod check;
mod fmt;
#[cfg(feature = "dhat-heap")]
mod heap;
mod i18n;
mod render;
mod run;
mod shaders;
mod stderr;
mod test;

#[derive(Parser)]
#[command(name = "ljos", version, about = "Ljós UI toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the authoring MCP server on stdio.
    Mcp {
        /// Directory the server may read; every path is confined to it.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Extra capabilities to grant, comma separated:
        /// inspect, input, mutate, eval, fs, run. Authoring is read-only by default.
        #[arg(long, value_parser = CapSet::parse_list, default_value = "")]
        caps: CapSet,
        /// The program `features_run` starts to run scenarios, for a project
        /// whose runner is not found by itself. It is given
        /// `[--input <glob>] [-t <tags>] [-n <name>] [--backlog]` and
        /// `LJOS_BDD_BACKEND`, in the project's directory.
        #[arg(long, value_name = "PROGRAM")]
        features_runner: Option<PathBuf>,
    },
    /// Translation catalogues.
    I18n {
        #[command(subcommand)]
        command: i18n::I18nCommand,
    },
    /// Compile QML projects and report what the compiler says; with
    /// `--stats`, how many expressions run in the tier-1 binding VM.
    Check(check::Check),
    /// Open a window on a QML file; with `--watch`, apply every save to
    /// the running application.
    Run(run::Run),
    /// Draw a project to a PNG without a window, after letting a stated
    /// time pass; what `mise run gallery` renders with.
    Render(render::Render),
    /// Run the `TestCase`s of `tst_*.qml` files on a virtual clock and
    /// print the results as Qt's runner for QML tests does; the exit
    /// status is the number of failed tests.
    Test(test::Test),
    /// Lay QML files out the one way the formatter knows: `--check` says
    /// which files are not laid out, `--diff` shows what would change.
    Fmt(fmt::Fmt),
    /// Compile the GLSL 440 shaders of `ShaderEffect`s to `.ljsh` files
    /// beside them, for an application that compiles none at run time:
    /// `--check` says which are missing or out of date.
    Shaders(shaders::Shaders),
    /// Measure QML applications on Ljós and on Qt inside a limit on
    /// memory and CPU (a systemd scope); writes `report.json` and
    /// `report.md`. Exits with 2 when an application was not inside the
    /// limit asked for.
    Bench(Box<bench::Bench>),
    /// What `ljos bench` starts an application through.
    #[command(hide = true)]
    BenchGate(bench::Gate),
}

fn main() -> ExitCode {
    #[cfg(feature = "dhat-heap")]
    heap::start();
    let status = match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // stdout carries the MCP protocol; diagnostics go to stderr.
            stderr::line(&format!("ljos: {e:#}"));
            ExitCode::FAILURE
        }
    };
    stderr::flush();
    status
}

/// Ends the process with `status` once what it logged is written.
fn exit(status: i32) -> ! {
    stderr::flush();
    std::process::exit(status)
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse_from(test::arguments(std::env::args_os()));
    // Same reason: logs must never touch stdout.
    tracing_subscriber::fmt()
        .with_writer(stderr::events)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
                .with_env_var("LJOS_LOG")
                .from_env_lossy(),
        )
        .init();
    match cli.command {
        Command::Mcp {
            project,
            caps,
            features_runner,
        } => {
            let project = Project::open(&project)?;
            let front = QmlFrontEnd::new(project.root());
            let server = AuthorServer::new(project, Arc::new(front));
            let server = match features_runner {
                Some(program) => server.with_runner(program),
                None => server,
            };
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("starting the async runtime")?
                .block_on(ljos_mcp_author::serve_stdio(server, caps))?;
            Ok(())
        }
        Command::I18n { command } => i18n::run(command),
        Command::Check(check) => check.run(),
        Command::Run(run) => run.run(),
        // The status says whether a picture was written; why not is in the
        // notes it printed.
        Command::Render(render) => {
            if render.run() {
                Ok(())
            } else {
                exit(1)
            }
        }
        // The status is how many tests failed, which is what Qt's runner
        // gives a script that runs a suite on both.
        Command::Test(test) => match test.run()? {
            0 => Ok(()),
            failed => exit(i32::from(failed)),
        },
        // 1 is `--check` finding a file to lay out, 2 a file that does not
        // parse; both are for a script to tell apart.
        Command::Fmt(fmt) => match fmt.run() {
            0 => Ok(()),
            status => exit(status),
        },
        // As `fmt`: 1 is `--check` finding work, 2 a shader in error.
        Command::Shaders(shaders) => match shaders.run() {
            0 => Ok(()),
            status => std::process::exit(status),
        },
        Command::Bench(bench) => match bench.run()? {
            0 => Ok(()),
            status => exit(status),
        },
        // Returns only when the application could not be started.
        Command::BenchGate(gate) => Err(gate.run()),
    }
}
