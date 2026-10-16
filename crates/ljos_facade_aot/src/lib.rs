//! An application's start with QML compiled ahead of time: the build
//! script packed `qml/` against [`ljos_facade_app`]'s native module,
//! and the program embeds the pack. Nothing here links the parser.

/// The application, ready to build or run.
pub fn app() -> ljos::App {
    ljos::App::new("Ahead of time")
        .register(ljos_facade_app::register)
        .source(ljos::include_qml!("qml.pack"))
        .root("Main")
}
