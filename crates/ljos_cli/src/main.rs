//! The `ljos` command line. `run`, `check` and `dump` arrive in later milestones;
//! each becomes another variant of [`Command`].

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use anyhow::Context;
use clap::{Parser, Subcommand};
use ljos_mcp_author::{AuthorServer, Project, QmlFrontEnd};
use ljos_mcp_core::CapSet;

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
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // stdout carries the MCP protocol; diagnostics go to stderr.
            eprintln!("ljos: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Same reason: logs must never touch stdout.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
                .with_env_var("LJOS_LOG")
                .from_env_lossy(),
        )
        .init();
    match cli.command {
        Command::Mcp { project, caps } => {
            let project = Project::open(&project)?;
            let front = QmlFrontEnd::new(project.root());
            let server = AuthorServer::new(project, Arc::new(front));
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("starting the async runtime")?
                .block_on(ljos_mcp_author::serve_stdio(server, caps))?;
            Ok(())
        }
    }
}
