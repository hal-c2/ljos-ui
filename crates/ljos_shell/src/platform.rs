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
pub(crate) fn install(engine: &Engine, wake: impl Fn() + Send + Sync + 'static) {
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
    crate::screen_reader::watch(engine);
    platform.set_plugin_name(plugin_name());
}

/// What Qt calls the window system: the name of its platform plugin.
fn plugin_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "cocoa"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty()) {
        // winit prefers Wayland when the session has one, as Qt does.
        "wayland"
    } else {
        "xcb"
    }
}
