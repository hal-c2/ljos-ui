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
//!   and the reachability stays unknown. The watch starts the first time
//!   QML reads `NetworkInformation` or Rust asks for the status; an
//!   application that never does starts no thread and opens no bus.
//! - macOS and Windows: nothing is asked yet, so the reachability is
//!   unknown, which an application can tell from "offline".

use ljos_engine::Engine;

/// Starts hearing the platform's network the first time something asks
/// for it (`ljos_platform::set_network_monitor`): an application that
/// never does starts no thread and opens no bus. The watch ends with the
/// engine.
pub(crate) fn install(engine: &Engine) {
    #[cfg(target_os = "linux")]
    install_with(engine, dbus::Buses::platform);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = engine;
    }
}

#[cfg(target_os = "linux")]
fn install_with(engine: &Engine, buses: impl FnOnce() -> dbus::Buses + 'static) {
    ljos_platform::set_network_monitor(engine, move |e| {
        let watch = dbus::watch_with(ljos_platform::Platform::of(e).poster(), buses());
        e.extension::<Kept>().0.replace(Some(watch));
    });
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct Kept(std::cell::RefCell<Option<dbus::Watch>>);

#[cfg(target_os = "linux")]
mod dbus;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Kept, dbus::Buses, install_with};

    /// Buses that count each time one is asked to connect, and connect
    /// to none.
    fn counted(
        asked: &Arc<AtomicUsize>,
        opened: &Arc<AtomicUsize>,
    ) -> impl FnOnce() -> Buses + use<> {
        let asked = Arc::clone(asked);
        let (session, system) = (Arc::clone(opened), Arc::clone(opened));
        move || {
            asked.fetch_add(1, Ordering::SeqCst);
            Buses {
                session: Box::new(move || {
                    session.fetch_add(1, Ordering::SeqCst);
                    Err(zbus::Error::Unsupported)
                }),
                system: Box::new(move || {
                    system.fetch_add(1, Ordering::SeqCst);
                    Err(zbus::Error::Unsupported)
                }),
            }
        }
    }

    fn engine() -> ljos_engine::Engine {
        let engine = ljos_engine::Engine::new();
        ljos_platform::register(&engine);
        engine
    }

    #[test]
    fn an_application_that_never_asks_starts_no_thread_and_opens_no_bus() {
        let (asked, opened): (Arc<AtomicUsize>, Arc<AtomicUsize>) = Default::default();
        let engine = engine();
        install_with(&engine, counted(&asked, &opened));
        // What the host hears before anything asked is kept, not shown.
        ljos_platform::report_network_status(&engine, ljos_platform::NetworkStatus::default());
        engine.request_wake();
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        assert!(engine.extension::<Kept>().0.borrow().is_none());
        drop(engine);
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        assert_eq!(opened.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_watch_starts_when_qml_first_reads_network_information() {
        let (asked, opened): (Arc<AtomicUsize>, Arc<AtomicUsize>) = Default::default();
        let engine = engine();
        install_with(&engine, counted(&asked, &opened));
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        // What a QML read of `NetworkInformation` does first.
        engine
            .singleton::<ljos_platform::NetworkInformation>()
            .expect("the singleton");
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert!(engine.extension::<Kept>().0.borrow().is_some());
        // Asked again, it is the same watch.
        let _ = ljos_platform::network_status(&engine);
        assert_eq!(asked.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_watch_starts_when_rust_first_asks() {
        let (asked, opened): (Arc<AtomicUsize>, Arc<AtomicUsize>) = Default::default();
        let engine = engine();
        install_with(&engine, counted(&asked, &opened));
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        let _ = ljos_platform::network_status(&engine);
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert!(engine.extension::<Kept>().0.borrow().is_some());
    }
}
