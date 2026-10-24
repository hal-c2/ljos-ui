//! The desktop's platform services (ARCHITECTURE.md §15): the system
//! clipboard and dialogs, the system's opener of URLs, an HTTPS fetcher,
//! the application's badge where the desktop has one (`badge.rs`), what the
//! system prefers beyond the colour scheme (`settings.rs`), and the window
//! system's name for `Qt.platform`.
//!
//! Files dragged in from other applications are not handled here: winit's
//! file events reach each window's egui input, and the window's `QmlView`
//! hands them to its scene's DropAreas (`ljos_egui`). Only the view knows
//! where its scene is and which tree it shows, and one path delivers a drop
//! once.

use std::sync::Arc;

use ljos_engine::Engine;
use ljos_platform::{HttpFetcher, Platform};

/// Gives a desktop application the system's services. Only the shell's
/// own loop does this: an application shown in another host (eframe, a
/// test) keeps the in-process ones.
pub(crate) fn install(engine: &Engine, wayland: bool, wake: impl Fn() + Send + Sync + 'static) {
    let platform = Platform::of(engine);
    // An answer from a dialog's thread wakes the loop; nothing waits for
    // one by looking.
    platform.set_waker(wake);
    if let Some(clipboard) = ljos_platform::native::SystemClipboard::connect() {
        platform.set_clipboard(std::rc::Rc::new(clipboard));
    }
    platform.set_dialogs(std::rc::Rc::new(
        ljos_platform::native::SystemDialogs::default(),
    ));
    if ljos_platform::native::SystemOpener::AVAILABLE {
        platform.set_open_url(std::rc::Rc::new(ljos_platform::native::SystemOpener));
    } else {
        engine.log(
            ljos_engine::LogLevel::Info,
            ljos_platform::CATEGORY,
            format!(
                "Qt.openUrlExternally opens nothing on {}: there is no opener for it yet",
                std::env::consts::OS
            ),
        );
    }
    if let Some(badge) = crate::badge::service(&platform) {
        platform.set_badge(badge);
    }
    platform.set_fetcher(Arc::new(HttpFetcher::new()));
    crate::settings::install(engine);
    crate::network::install(engine);
    crate::screen_reader::watch(engine);
    platform.set_plugin_name(plugin_name(wayland));
    if wayland {
        platform.set_external_drops_unavailable(engine, NO_WAYLAND_DROPS);
    }
}

/// Why files from other applications do not come on Wayland, and how to
/// have them.
const NO_WAYLAND_DROPS: &str = "the window library (winit 0.30) has no drag-and-drop on \
    Wayland; run the application on X11 or XWayland, with LJOS_PREFER_X11=1 or \
    ShellConfig::prefer_x11";

/// Whether the shell asks for X11: the environment's `LJOS_PREFER_X11`
/// when it says yes or no, else the application's choice.
pub(crate) fn prefers_x11(config: bool, env: Option<&str>) -> bool {
    match env.map(str::trim) {
        Some("1" | "true" | "yes") => true,
        Some("0" | "false" | "no") => false,
        _ => config,
    }
}

/// What Qt calls the window system: the name of its platform plugin.
fn plugin_name(wayland: bool) -> &'static str {
    if cfg!(target_os = "macos") {
        "cocoa"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if wayland {
        "wayland"
    } else {
        "xcb"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_says_yes_or_no_to_x11_and_else_the_application_does() {
        assert!(prefers_x11(false, Some("1")));
        assert!(prefers_x11(false, Some("true")));
        assert!(!prefers_x11(true, Some("0")));
        assert!(!prefers_x11(true, Some("no")));
        assert!(prefers_x11(true, None));
        assert!(!prefers_x11(false, None));
        assert!(prefers_x11(true, Some("")));
        assert!(!prefers_x11(false, Some("maybe")));
    }

    #[test]
    fn the_window_system_is_named_by_what_the_host_speaks() {
        if cfg!(all(
            unix,
            not(target_vendor = "apple"),
            not(target_os = "android")
        )) {
            assert_eq!(plugin_name(true), "wayland");
            assert_eq!(plugin_name(false), "xcb");
        }
    }
}
