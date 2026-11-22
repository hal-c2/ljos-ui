//! The application the phone's scenario runner (`ljos_bdd`'s `@android`
//! backend) installs once and starts for each scenario.
//!
//! The runner writes the scenario's files into `qml/` in the
//! application's own files directory, with its entry as `Main.qml`, and
//! starts the application with its inspector (`ljos android run --mcp`
//! builds it so). Those files are laid over the window built in, as a
//! user's files are over a shipped application's (`App::user_qml`), so a
//! new scenario takes no new build.

use std::path::PathBuf;

/// Where the scenario's files are: `qml/` beside the inspector's token
/// file, which the activity puts in the application's files directory
/// (`LJOS_MCP_DIR`), or in the working directory without one.
#[must_use]
pub fn scenario_dir() -> PathBuf {
    std::env::var_os("LJOS_MCP_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("qml")
}

/// Where what a scenario's files did wrong is written, for the runner to
/// read: stderr goes nowhere on a phone.
#[must_use]
pub fn fault_file() -> PathBuf {
    scenario_dir().with_file_name("scenario-fault.txt")
}

/// The application, ready to build or run.
#[must_use]
pub fn app() -> ljos::App {
    ljos::App::new("Ljós scenarios")
        .source(ljos::include_qml!("qml.pack"))
        .user_qml(ljos::UserQml::dir(scenario_dir()).on_fault(|fault| {
            let _ = std::fs::write(fault_file(), fault.to_string());
        }))
        .root("Main")
}

/// What GameActivity calls on its own thread for each activity.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: ljos::android::AndroidApp) {
    ljos::run_android(app, self::app);
}
