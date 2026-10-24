//! An application in development: its QML watched, and every save
//! applied while it runs. Built with `cargo check -p ljos_facade_dev`, so
//! that only this crate's features of `ljos` are on.

use std::path::Path;

fn main() {
    // Every directory `qml_dirs` compiled is watched, without naming them
    // again.
    ljos::App::new("watched")
        .qml_dirs(["user", "qml"], "Main")
        .unwrap()
        .run_with(|setup| {
            setup.on_reload(|report| {
                if let ljos::reload::Report::Reloaded(stats) = report {
                    eprintln!("reloaded in {:?}", stats.apply);
                }
            });
            if setup.qml_dirs().is_empty() {
                return setup.watch(Path::new("qml/Main.qml"));
            }
            setup.watch_qml_dirs(&["override", "Main"])
        })
        .unwrap();
}
