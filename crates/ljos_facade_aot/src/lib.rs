//! An application's start with QML compiled ahead of time: the build
//! script packed `qml/` against `ljostypes.json`, the type dump file of
//! the native module below, and the program embeds the pack and the
//! bindings and handlers the script compiled to Rust. Nothing here links
//! the parser, and the build script runs none of this code.

use ljos::prelude::*;

/// A native type with a property, a signal and an invokable method.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)?;
        Counter::emit_bumped(cx)
    }
}

/// Registers the native module; `tests/types.rs` writes its type dump
/// file from this.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
}

/// The application, ready to build or run.
pub fn app() -> ljos::App {
    ljos::App::new("Ahead of time")
        .register(register)
        .source(ljos::include_qml!("qml.pack"))
        .compiled(ljos::include_compiled!("qml.rs"))
        .root("Main")
}
