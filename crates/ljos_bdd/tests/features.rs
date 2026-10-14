//! `cargo test -p ljos_bdd --test features [-- --backlog | --tags <expr> | --name <re>]`
//!
//! Exits non-zero when a selected scenario fails; it still runs all of them first.
use std::process::ExitCode;

fn main() -> ExitCode {
    // A date is written in local time, and Qt's runs are made in UTC
    // (backend/qt.rs), so Ljós runs in the same zone. Nothing else runs
    // yet, so no thread reads the environment while it changes.
    // SAFETY: the process has one thread at this point.
    unsafe { std::env::set_var("TZ", "UTC") };
    if ljos_bdd::runner::run() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
