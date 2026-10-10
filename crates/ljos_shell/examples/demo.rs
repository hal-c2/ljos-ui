//! A window built natively through the Engine API: a Column holding a Text
//! and a Row of Rectangles, a framed box centred with anchors, and a button
//! anchored to the bottom-right corner whose MouseArea changes its colour.
//! A worker thread posts one message through the `UiHandle` after a second.
//!
//! `cargo run -p ljos_shell --example demo`
//!
//! Once it settles the process should use no CPU until you touch it.

use std::time::Duration;

use ljos_engine::Value;
use ljos_shell::{ShellConfig, run};

#[path = "shared/demo_scene.rs"]
mod demo_scene;

enum Msg {
    Status(String),
}

fn main() -> anyhow::Result<()> {
    let config = ShellConfig::new("Ljós demo").with_size(640.0, 420.0);
    run::<Msg>(config, |setup| {
        let built = demo_scene::build(setup.engine()).map_err(|e| e.to_string())?;
        let status = built.status;
        setup.on_message(move |e, msg| match msg {
            Msg::Status(s) => {
                let _ = e.write_by_name(status, "text", Value::string(s));
            }
        });
        let handle = setup.handle();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            let _ = handle.send(Msg::Status("Hello from a worker thread".to_owned()));
        });
        Ok(built.root)
    })?;
    Ok(())
}
