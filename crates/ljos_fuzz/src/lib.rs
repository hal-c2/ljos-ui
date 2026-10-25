//! Differential, property-based testing of Ljós against Qt.
//!
//! An [`area::Area`] generates random programs Qt accepts. Both sides run
//! each one, Qt as `qml6` (a black box, many cases per process) and Ljós in
//! this process, and observe the same things through functions the program
//! itself carries. Where they differ the case is shrunk to a minimal repro
//! and put in a bucket named by what it does; buckets on the checked-in
//! known list do not fail a run.

pub mod area;
pub mod bindings;
pub mod bucket;
pub mod builtins;
pub mod compare;
pub mod connections;
pub mod creation;
pub mod geometry;
pub mod loaders;
pub mod models;
pub mod outcome;
pub mod report;
pub mod rng;
pub mod run;
pub mod shrink;
pub mod side;
pub mod states;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}:{line}: a known bucket needs a reason after its signature", path.display())]
    Known { path: PathBuf, line: usize },
}
