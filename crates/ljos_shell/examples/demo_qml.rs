//! The same kind of window as `demo`, written in QML: the files under a
//! directory are compiled with `ljos_compiler` at start-up and shown
//! through the facade's `App` builder, as an application starts. (A
//! shipped application embeds the compiled IR instead of compiling
//! here, and hands it to the same builder as its source.)
//!
//! `cargo run -p ljos_shell --example demo_qml [DIR] [KEY]`
//! (defaults: this example's `qml/` directory and `Main`).

use std::path::PathBuf;

#[path = "shared/demo_qml.rs"]
mod demo_qml;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .map_or_else(demo_qml::default_dir, PathBuf::from);
    let key = args.next().unwrap_or_else(|| "Main".to_owned());
    let source = demo_qml::compile(&dir)?;

    ljos::App::new(format!("Ljós — {key}.qml"))
        .size(640.0, 420.0)
        .source(source)
        .root(key)
        .run()?;
    Ok(())
}
