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
//! are found. Under `qml_dirs` the files are those on disk as the text
//! is made, a file added since the start among them, and a text placed
//! in a directory none of them is finds the files beside it there; the
//! same holds under [`install_text_compiler`].
//!
//! The same compiler serves a `.qml` file the program was not built
//! with, such as a page fetched while it runs: `Qt.createComponent` and
//! `Loader` compile it from the disk when it is first asked for, and keep
//! it, as Qt keeps a component it loaded. A shipped application has no
//! compiler by design. There `Qt.createQmlObject` is an error the
//! script can catch, and a file it was not built with is an error
//! component that says so. QML the application loads at run time has to
//! come compiled, or the application ships with this feature on and
//! links the parser.

pub use crate::compile::{
    CompileError, Compiled, Diagnostic, EngineOracle, EngineType, NativeTypeError, OnError,
    Severity, compile, compile_dir, compile_dir_with, compile_dirs, compile_quiet,
    install_text_compiler, text_compiler,
};
