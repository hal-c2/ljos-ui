//! The platform's own windows, for what only the platform's API does
//! (a window manager's hint, a native view added to the window):
//! [`App::on_window`](crate::App::on_window) is handed each window the host opens, as a
//! [`NativeWindow`] whose handles are `raw-window-handle`'s, re-exported
//! here at the version the host uses.
//!
//! [`replace_root`] puts another root in the window of the one shown, for
//! an application that makes its whole interface again.
//!
//! An [`Icon`] is what the desktop shows for the application
//! ([`App::icon`](crate::App::icon)) or for one window
//! ([`Platform::set_window_icon`](crate::prelude::Platform::set_window_icon)).

pub use ljos_platform::{Icon, IconError, NativeWindow, raw_window_handle};
pub use ljos_quick::roots::{RootError, replace_root};

use ljos_engine::{Engine, LogLevel};

/// Gives the application the icon at `url` (as
/// [`App::icon_url`](crate::App::icon_url) does) once it is decoded,
/// off the UI thread; one that cannot be read is a warning in the log.
pub fn load_application_icon(engine: &Engine, url: &str) {
    let named = url.to_owned();
    crate::item::load_image(engine, url, move |engine, loaded| {
        let icon = loaded.map_err(|e| e.to_string()).and_then(|image| {
            let (w, h) = (image.width(), image.height());
            Icon::from_premultiplied_rgba(w, h, image.into_data()).map_err(|e| e.to_string())
        });
        match icon {
            Ok(icon) => ljos_platform::Platform::of(engine).set_application_icon(Some(icon)),
            Err(e) => engine.log(
                LogLevel::Warning,
                ljos_platform::CATEGORY,
                format!("the application's icon {named} could not be read: {e}"),
            ),
        }
    });
}
