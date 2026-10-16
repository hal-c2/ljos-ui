//! Development-only QML compilation (the non-default `dev-qml`
//! feature).
//!
//! A shipped application never links the QML parser: it embeds
//! components compiled ahead of time ([`crate::build`] in its build
//! script, [`crate::include_qml!`] in the program) and hands them to
//! [`App::source`](crate::App::source). What is here compiles a
//! directory of `.qml` files at start-up instead, for examples,
//! harnesses and the first run of a project — the feature is named
//! for that, and turning it on links `ljos_compiler` and with it
//! the parser.

pub use crate::compile::{
    EngineOracle, EngineType, OnError, compile, compile_dir, compile_dir_with, compile_dirs,
};
