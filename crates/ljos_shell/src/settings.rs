//! What the system prefers: less motion, more contrast and larger text,
//! read when the application starts and again each time the platform says
//! one changed (`Qt.styleHints.accessibility`). The colour scheme comes
//! from winit through egui (`ljos_egui`), and on Linux, where winit says
//! none, from the portal.
//!
//! - Linux: the desktop's settings portal (`org.freedesktop.portal.Settings`
//!   on the session bus): its `ReadAll` at start and its `SettingChanged`
//!   signal after, heard on a thread of its own that sleeps until the bus
//!   has a message. The keys are those of [`ljos_a11y::DesktopSettings`],
//!   the colour scheme among them. Without a portal nothing is said, and
//!   the application keeps the defaults. The desktop's interface font is
//!   heard there too, and becomes the application font
//!   (`crate::desktop_font::followed`).
//! - macOS: `NSWorkspace`'s `accessibilityDisplayShouldReduceMotion` and
//!   `accessibilityDisplayShouldIncreaseContrast`, read again on its
//!   `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`. macOS
//!   has no text size for applications to follow, so none is said.
//! - Windows: `SPI_GETCLIENTAREAANIMATION`, high contrast
//!   (`SPI_GETHIGHCONTRAST`) and the text scale of the Accessibility
//!   settings (`TextScaleFactor` under `HKCU\Software\Microsoft\Accessibility`),
//!   read again on `WM_SETTINGCHANGE` and on a change of that key, by a
//!   thread with a hidden window that sleeps until either comes.
//!
//! What a host hears is a report (`ljos_quick::report_system_prefs`): a
//! setting an application or a test set itself stays over it.

use ljos_engine::Engine;

/// Starts hearing the platform's settings. The watch ends with the engine.
pub(crate) fn install(engine: &Engine) {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let watch = imp::watch(ljos_platform::Platform::of(engine).poster());
        engine.extension::<Kept>().0.replace(Some(watch));
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = engine;
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Default)]
struct Kept(std::cell::RefCell<Option<imp::Watch>>);

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn report(post: &ljos_platform::Post, prefs: ljos_a11y::HostPrefs) {
    post.send(move |e: &Engine| ljos_quick::report_system_prefs(e, prefs));
}

#[cfg(target_os = "linux")]
use portal as imp;

#[cfg(target_os = "linux")]
mod portal;

#[cfg(target_os = "macos")]
use workspace as imp;

#[cfg(target_os = "macos")]
mod workspace;

#[cfg(target_os = "windows")]
use win32 as imp;

#[cfg(target_os = "windows")]
mod win32;
