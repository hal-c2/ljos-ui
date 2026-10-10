//! The same kind of window as `demo`, written in QML: the files under a
//! directory are compiled with `ljos_compiler` at start-up and the Engine
//! creates the root from the IR.
//!
//! `cargo run -p ljos_shell --example demo_qml [DIR] [KEY]`
//! (defaults: this example's `qml/` directory and `Main`).

use std::path::PathBuf;

use ljos_shell::{ShellConfig, run};

#[path = "shared/demo_qml.rs"]
mod demo_qml;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .map_or_else(demo_qml::default_dir, PathBuf::from);
    let key = args.next().unwrap_or_else(|| "Main".to_owned());
    let source = demo_qml::compile(&dir)?;

    let config = ShellConfig::new(format!("Ljós — {key}.qml")).with_size(640.0, 420.0);
    run::<()>(config, move |setup| {
        let engine = setup.engine();
        engine.set_component_source(source);
        Ok(engine
            .create_from_url(&key, None, Vec::new())
            .map_err(|e| e.to_string())?)
    })?;
    Ok(())
}
