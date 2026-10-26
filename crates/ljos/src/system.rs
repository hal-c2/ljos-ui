//! What the system prefers: colour scheme, contrast, less motion, text
//! size and whether a screen reader listens (`Qt.styleHints` and its
//! `accessibility`). The shell reads the platform's settings at start and
//! on each change and reports them ([`report_system_prefs`]); what an
//! application sets itself ([`set_system_prefs`], [`update_system_prefs`])
//! stays over those reports until it is set to the platform's value again.
//! [`DesktopSettings`] turns a Linux desktop's settings-portal keys into a
//! report, for a host of the application's own.
//!
//! Whether the system reaches the network (`NetworkInformation`): the
//! shell hears the platform's network monitor and reports what it says
//! ([`report_network_status`]); [`network_status`] reads it, tracked, and
//! [`on_network_change`] calls back when it changes.
//!
//! The application's palette (`QGuiApplication::palette()`), which QML
//! cannot write: a host sets it for its theme ([`set_application_palette`]).
//! A link of rich text or Markdown its document gives no colour is drawn
//! in its link colour.
//!
//! ```
//! use ljos::system::{NetworkStatus, Reachability, network_status, report_network_status};
//!
//! let engine = ljos::engine();
//! assert_eq!(network_status(&engine).reachability, Reachability::Unknown);
//! report_network_status(
//!     &engine,
//!     NetworkStatus { reachability: Reachability::Online, ..Default::default() },
//! );
//! assert!(network_status(&engine).is_online());
//! ```

use ljos_engine::{Engine, EngineError};

pub use ljos_platform::{
    NetworkStatus, Reachability, TransportMedium, network_status, report_network_status,
};
pub use ljos_quick::{ApplicationPalette, application_palette, set_application_palette};
pub use ljos_quick::{
    ColorScheme, Contrast, DesktopSettings, HostPrefs, SettingValue, SystemPrefs,
    report_system_prefs, set_system_prefs, system_prefs, update_system_prefs,
};

/// What [`on_network_change`] returns: the subscription, until it is
/// stopped.
#[derive(Debug)]
#[must_use = "a subscription runs until it is stopped"]
pub struct NetworkWatch(ljos_engine::NodeId);

impl NetworkWatch {
    /// Ends the subscription.
    pub fn stop(self, engine: &Engine) {
        engine.stop_effect(self.0);
    }
}

/// Calls `f` with what the system says of its network whenever that
/// changes (not now).
pub fn on_network_change(
    engine: &Engine,
    f: impl Fn(&Engine, NetworkStatus) + 'static,
) -> Result<NetworkWatch, EngineError> {
    let mut first = true;
    engine
        .effect(move |e| {
            let status = network_status(e);
            if std::mem::take(&mut first) {
                return;
            }
            e.untrack(|| f(e, status));
        })
        .map(NetworkWatch)
}
