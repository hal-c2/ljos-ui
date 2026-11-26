//! The application the phone's scenario runner (`ljos_bdd`'s `@android`
//! backend) installs once and starts for each scenario.
//!
//! The runner writes the scenario's files into `qml/` in the
//! application's own files directory, with its entry as `Main.qml`, and
//! starts the application with its inspector (`ljos android run --mcp`
//! builds it so). Those files are laid over the window built in, as a
//! user's files are over a shipped application's (`App::user_qml`), so a
//! new scenario takes no new build.

use std::path::{Path, PathBuf};

mod secrets;
pub use secrets::{ScenarioSecrets, keep_secrets_in};

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

/// The file the runner leaves in the files directory for a scenario whose
/// application may read local files through `XMLHttpRequest`: the phone
/// takes no `QML_XHR_ALLOW_FILE_READ` from the runner.
pub const XHR_LOCAL_READ: &str = "xhr-local-read";

/// Lets `engine` read local files when `files` holds the runner's word
/// for it.
pub fn allow_local_file_read(engine: &ljos::prelude::Engine, files: &Path) {
    if files.join(XHR_LOCAL_READ).is_file() {
        ljos::prelude::Platform::of(engine).set_local_file_read(true);
    }
}

/// The file the runner leaves in the files directory for a scenario whose
/// application sets its badge as it starts: the count, in decimal. QML
/// has no member for the badge, as Qt's has none; an application sets it
/// from Rust.
pub const BADGE: &str = "badge";

/// Sets the application's badge to the count `files` holds for it, if
/// any.
pub fn badge_at_start(engine: &ljos::prelude::Engine, files: &Path) {
    let count = std::fs::read_to_string(files.join(BADGE))
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    if let Some(count) = count {
        ljos::prelude::Platform::of(engine).set_badge_number(count);
    }
}

/// The application, ready to build or run.
#[must_use]
pub fn app() -> ljos::App {
    ljos::App::new("Ljós scenarios")
        .register(|engine| {
            secrets::register(engine);
            if let Some(files) = scenario_dir().parent() {
                allow_local_file_read(engine, files);
                badge_at_start(engine, files);
            }
        })
        // Made here and put in the root's scope: the runner reaches it by
        // name through the inspector's scripts, and compiled QML has no
        // compiler to make one from text.
        .context_property(secrets::NAME, |engine| {
            engine
                .create_by_name("LjosScenarios", "ScenarioSecrets", None)
                .map_or(
                    ljos::prelude::Value::Undefined,
                    ljos::prelude::Value::Object,
                )
        })
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
