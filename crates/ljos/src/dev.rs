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
//!
//! `Qt.createQmlObject` compiles the text it is given, so it makes an
//! object only where there is a compiler: [`App::qml_dirs`](crate::App::qml_dirs)
//! installs one over the directories it compiles, and
//! [`install_text_compiler`] gives one to an engine set up by hand (a
//! harness, a [`TestWindow`](crate::testing)). A text is compiled as a
//! file at the place the script names, so the types beside that place
//! are found: a directory of the user's own files is one of the
//! directories named. A shipped application has no compiler by design;
//! there the call is an error the script can catch, and QML the
//! application loads at run time has to come compiled, or the
//! application ships with this feature on and links the parser.

pub use crate::compile::{
    Compiled, Diagnostic, EngineOracle, EngineType, OnError, Severity, compile, compile_dir,
    compile_dir_with, compile_dirs, compile_quiet, install_text_compiler, text_compiler,
};
