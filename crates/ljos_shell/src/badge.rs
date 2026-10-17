//! The application's badge where the desktop has one
//! (`Platform::set_badge_number`, as `QGuiApplication::setBadgeNumber`).
//!
//! - Linux: the launcher-entry signal on the session bus
//!   (`com.canonical.Unity.LauncherEntry.Update`), which KDE Plasma's task
//!   manager and the GNOME docks read. It names the application by its
//!   desktop entry (`Platform::desktop_file_name`).
//! - macOS: the Dock tile's badge label.
//! - Elsewhere the in-process service stays: the count is kept, not shown.
//!   Windows shows a count only as an overlay icon the application draws,
//!   which needs a picture per count; there is none yet.

use std::rc::Rc;

use ljos_platform::{BadgeService, Platform};

/// The desktop's badge, when this system has one.
pub(crate) fn service(platform: &Rc<Platform>) -> Option<Rc<dyn BadgeService>> {
    #[cfg(target_os = "linux")]
    {
        Some(Rc::new(launcher::LauncherEntry::session(platform)))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = platform;
        Some(Rc::new(dock::DockBadge))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = platform;
        None
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod launcher {
    use std::cell::{Cell, OnceCell};
    use std::collections::HashMap;
    use std::rc::{Rc, Weak};
    use std::sync::{Arc, Condvar, Mutex};

    use ljos_platform::{BadgeService, Platform};
    use zbus::blocking::Connection;
    use zbus::zvariant::Value;

    const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";

    type Connect = Box<dyn FnOnce() -> zbus::Result<Connection> + Send>;

    /// Sends the count to the session bus from a thread of its own, which
    /// is started with the first count: connecting may wait on the bus,
    /// and the Engine thread never does. The thread sleeps until there is
    /// a count to send; only the latest unsent one is kept.
    pub(crate) struct LauncherEntry {
        platform: Weak<Platform>,
        connect: Cell<Option<Connect>>,
        sender: OnceCell<Arc<Latest>>,
    }

    #[derive(Default)]
    struct Latest {
        state: Mutex<Pending>,
        ready: Condvar,
    }

    #[derive(Default)]
    struct Pending {
        next: Option<(String, u32)>,
        closed: bool,
    }

    impl LauncherEntry {
        /// On the session bus; without one, the counts go nowhere.
        pub(crate) fn session(platform: &Rc<Platform>) -> Self {
            Self::new(platform, Box::new(Connection::session))
        }

        pub(crate) fn new(platform: &Rc<Platform>, connect: Connect) -> Self {
            Self {
                platform: Rc::downgrade(platform),
                connect: Cell::new(Some(connect)),
                sender: OnceCell::new(),
            }
        }

        fn start(&self) -> Arc<Latest> {
            let latest = Arc::new(Latest::default());
            if let Some(connect) = self.connect.take() {
                let shared = Arc::clone(&latest);
                let started = std::thread::Builder::new()
                    .name("ljos-badge".to_owned())
                    .spawn(move || send_all(connect, &shared));
                // Without the thread the counts are kept and not shown, as
                // they are where the desktop has no badge.
                drop(started);
            }
            latest
        }
    }

    impl BadgeService for LauncherEntry {
        fn show(&self, count: u32) {
            let Some(platform) = self.platform.upgrade() else {
                return;
            };
            let name = platform.desktop_file_name();
            let latest = self.sender.get_or_init(|| self.start());
            let mut state = latest.state.lock().unwrap_or_else(|e| e.into_inner());
            state.next = Some((name, count));
            latest.ready.notify_one();
        }
    }

    impl Drop for LauncherEntry {
        fn drop(&mut self) {
            if let Some(latest) = self.sender.get() {
                let mut state = latest.state.lock().unwrap_or_else(|e| e.into_inner());
                state.closed = true;
                latest.ready.notify_one();
            }
        }
    }

    fn send_all(connect: Connect, latest: &Latest) {
        // No session bus: nothing to show the badge on.
        let Ok(bus) = connect() else {
            return;
        };
        loop {
            let next = {
                let mut state = latest.state.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if let Some(next) = state.next.take() {
                        break next;
                    }
                    if state.closed {
                        return;
                    }
                    state = latest.ready.wait(state).unwrap_or_else(|e| e.into_inner());
                }
            };
            let (name, count) = next;
            // A failed send (the bus went away) loses this count only.
            let _ = update(&bus, &name, count);
        }
    }

    /// `Update(app_uri, properties)`, from an object path of the
    /// application's own as the convention has it.
    fn update(bus: &Connection, desktop_file_name: &str, count: u32) -> zbus::Result<()> {
        let uri = format!("application://{desktop_file_name}.desktop");
        let path = format!("/com/canonical/unity/launcherentry/{}", fnv1a(&uri));
        let mut properties: HashMap<&str, Value<'_>> = HashMap::new();
        properties.insert("count", Value::I64(i64::from(count)));
        properties.insert("count-visible", Value::Bool(count > 0));
        bus.emit_signal(
            None::<&str>,
            path.as_str(),
            INTERFACE,
            "Update",
            &(uri, properties),
        )
    }

    /// A name for the object path made only of the characters one allows.
    fn fnv1a(s: &str) -> u64 {
        s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[cfg(test)]
    mod tests {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        use std::rc::Rc;
        use std::sync::mpsc;
        use std::time::Duration;

        use ljos_platform::{BadgeService, Platform};
        use zbus::blocking::{Connection, MessageIterator, connection};
        use zbus::zvariant::OwnedValue;

        use super::{INTERFACE, LauncherEntry};

        type Update = (String, std::collections::HashMap<String, OwnedValue>);

        /// A bus of the test's own, so nothing reaches the desktop's.
        struct Bus {
            daemon: std::process::Child,
            address: String,
            _dir: tempfile::TempDir,
        }

        impl Bus {
            fn start() -> Option<Self> {
                let dir = tempfile::tempdir().ok()?;
                let mut daemon = Command::new("dbus-daemon")
                    .args(["--session", "--nofork", "--print-address"])
                    .arg(format!(
                        "--address=unix:path={}",
                        dir.path().join("bus").display()
                    ))
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()?;
                let mut address = String::new();
                BufReader::new(daemon.stdout.take()?)
                    .read_line(&mut address)
                    .ok()?;
                Some(Bus {
                    daemon,
                    address: address.trim().to_owned(),
                    _dir: dir,
                })
            }

            fn connect(&self) -> Connection {
                connection::Builder::address(self.address.as_str())
                    .and_then(connection::Builder::build)
                    .expect("the test's bus")
            }
        }

        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.daemon.kill();
                let _ = self.daemon.wait();
            }
        }

        #[test]
        fn a_count_is_sent_as_the_launcher_entry_update_of_the_desktop_entry() {
            let Some(bus) = Bus::start() else {
                eprintln!("skipped: no dbus-daemon to start");
                return;
            };
            let listener = bus.connect();
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(INTERFACE)
                .expect("the interface")
                .build();
            let mut updates =
                MessageIterator::for_match_rule(rule, &listener, None).expect("a match rule");
            let (tx, rx) = mpsc::channel::<Update>();
            std::thread::spawn(move || {
                while let Some(Ok(msg)) = updates.next() {
                    if let Ok(update) = msg.body().deserialize::<Update>()
                        && tx.send(update).is_err()
                    {
                        return;
                    }
                }
            });

            let engine = ljos_engine::Engine::new();
            let platform: Rc<Platform> = Platform::of(&engine);
            platform.set_desktop_file_name("org.example.Notes.desktop");
            let address = bus.address.clone();
            let entry = LauncherEntry::new(
                &platform,
                Box::new(move || connection::Builder::address(address.as_str())?.build()),
            );
            entry.show(3);
            let (uri, properties) = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("an Update signal");
            assert_eq!(uri, "application://org.example.Notes.desktop");
            assert_eq!(i64::try_from(&properties["count"]).ok(), Some(3));
            assert_eq!(
                bool::try_from(&properties["count-visible"]).ok(),
                Some(true)
            );

            entry.show(0);
            let (_, properties) = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("a second Update signal");
            assert_eq!(i64::try_from(&properties["count"]).ok(), Some(0));
            assert_eq!(
                bool::try_from(&properties["count-visible"]).ok(),
                Some(false)
            );
        }

        #[test]
        fn without_a_bus_a_count_goes_nowhere() {
            let engine = ljos_engine::Engine::new();
            let platform = Platform::of(&engine);
            let entry = LauncherEntry::new(
                &platform,
                Box::new(|| Err(zbus::Error::Address("no bus".to_owned()))),
            );
            entry.show(2);
            entry.show(0);
        }
    }
}

#[cfg(target_os = "macos")]
mod dock {
    use ljos_platform::BadgeService;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    /// The Dock tile's badge label: the count, or none for 0.
    pub(crate) struct DockBadge;

    impl BadgeService for DockBadge {
        fn show(&self, count: u32) {
            // AppKit is the main thread's, which is where the shell runs the
            // Engine on macOS; anywhere else the count is only kept.
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
            NSApplication::sharedApplication(mtm)
                .dockTile()
                .setBadgeLabel(label.as_deref());
        }
    }
}
