//! A shipped application its users change while it runs.
//!
//! `ljos_example_user_shell [DIR]` runs the QML its build compiled, with
//! the files of `DIR` (default `user-shell` in the working directory)
//! over it. A `Shell.qml` there is shown in place of the application's
//! own shell; a `Main.qml` replaces the whole window. Every save is
//! applied while it runs, the typed text kept, and a file that does not
//! compile leaves the application as it was and is printed here.

use std::path::PathBuf;

fn main() -> Result<(), ljos::ShellError> {
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("user-shell"), PathBuf::from);
    ljos::App::new("User shell")
        .source(ljos::include_qml!("qml.pack"))
        .user_qml(ljos::UserQml::dir(dir).on_fault(|fault| eprintln!("user-shell: {fault}")))
        .root("Main")
        .run()
}
