//! Whether the system can reach the network (`NetworkInformation`), read
//! when the application starts and again each time the platform says it
//! changed.
//!
//! - Linux: the desktop portal's network monitor
//!   (`org.freedesktop.portal.NetworkMonitor` on the session bus), read
//!   again on its `changed` signal. Without it, NetworkManager on the
//!   system bus: its `Connectivity`, `Metered` and `PrimaryConnectionType`,
//!   followed through `PropertiesChanged`. Either is heard on a thread of
//!   its own that sleeps until the bus has a message. Only NetworkManager
//!   says which medium carries the network. Without either nothing is said,
//!   and the reachability stays unknown.
//! - macOS and Windows: nothing is asked yet, so the reachability is
//!   unknown, which an application can tell from "offline".

use ljos_engine::Engine;

/// Starts hearing the platform's network. The watch ends with the engine.
pub(crate) fn install(engine: &Engine) {
    #[cfg(target_os = "linux")]
    {
        let watch = dbus::watch(ljos_platform::Platform::of(engine).poster());
        engine.extension::<Kept>().0.replace(Some(watch));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = engine;
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct Kept(std::cell::RefCell<Option<dbus::Watch>>);

#[cfg(target_os = "linux")]
mod dbus;
