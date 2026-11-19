//! A shipped application its users change while it runs.
//!
//! `ljos_example_user_shell [DIR] [--second]` runs the QML its build
//! compiled, with the files of `DIR` (default `user-shell` in the working
//! directory) over it. A `Shell.qml` there is shown in place of the
//! application's own shell; a `Main.qml` replaces the whole window. Every
//! save is applied while it runs, the typed text kept, and a file that
//! does not compile leaves the application as it was and is printed here.
//! `--second` opens a second window beside the first, which every save
//! reaches too.

use std::path::PathBuf;

fn main() -> Result<(), ljos::ShellError> {
    let (second, dirs): (Vec<_>, Vec<_>) =
        std::env::args_os().skip(1).partition(|a| a == "--second");
    let dir = dirs
        .into_iter()
        .next()
        .map_or_else(|| PathBuf::from("user-shell"), PathBuf::from);
    let app = ljos::App::new("User shell")
        .source(ljos::include_qml!("qml.pack"))
        .user_qml(ljos::UserQml::dir(dir).on_fault(|fault| eprintln!("user-shell: {fault}")));
    if second.is_empty() {
        return app.root("Main").run();
    }
    app.run_with(|setup| {
        let engine = setup.engine();
        let made = |key| {
            engine
                .create_from_url(key, None, Vec::new())
                .map_err(|e| -> ljos::SetupError { e.to_string().into() })
        };
        let window = made("Second")?;
        // A save that makes the roots again makes this one again too. The
        // new one is a `Window` the shell shows, as it shows this one.
        engine
            .remake_on_reload(window, |_, _, _| {})
            .map_err(|e| -> ljos::SetupError { e.to_string().into() })?;
        made("Main")
    })
}
