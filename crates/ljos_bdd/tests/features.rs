//! `cargo test -p ljos_bdd --test features [-- --backlog | --tags <expr> | --name <re>]`
//!
//! Exits non-zero when a selected scenario fails; it still runs all of them first.
use std::process::ExitCode;

fn main() -> ExitCode {
    if ljos_bdd::runner::run() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
