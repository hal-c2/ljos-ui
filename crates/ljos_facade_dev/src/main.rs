//! An application in development: its QML watched, and every save
//! applied while it runs. Built with `cargo check -p ljos_facade_dev`, so
//! that only this crate's features of `ljos` are on.

use std::path::Path;

fn main() {
    ljos::App::new("watched")
        .run_with(|setup| {
            setup.on_reload(|report| {
                if let ljos::reload::Report::Reloaded(stats) = report {
                    eprintln!("reloaded in {:?}", stats.apply);
                }
            });
            setup.watch(Path::new("qml/Main.qml"))
        })
        .unwrap();
}
