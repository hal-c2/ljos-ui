//! The network as the portal's `NetworkMonitor` on the session bus and
//! NetworkManager on the system bus say it is (`crate::network`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ljos_engine::Engine;
use ljos_platform::{NetworkStatus, Post, Reachability, TransportMedium};
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::OwnedValue;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const MONITOR: &str = "org.freedesktop.portal.NetworkMonitor";

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

pub(crate) type Connect = Box<dyn FnOnce() -> zbus::Result<Connection> + Send>;

/// Where the monitors are: the portal on the session bus, NetworkManager
/// on the system bus.
pub(crate) struct Buses {
    pub(crate) session: Connect,
    pub(crate) system: Connect,
}

/// The thread that hears the network. Dropped, it closes the connection
/// the thread sleeps on, which ends it.
pub(crate) struct Watch {
    held: Arc<Mutex<Held>>,
}

#[derive(Default)]
struct Held {
    bus: Option<Connection>,
    closed: bool,
}

impl Held {
    /// Keeps `bus` to close when the watch ends; `false` if it has.
    fn keep(held: &Mutex<Held>, bus: &Connection) -> bool {
        let mut held = crate::lock(held);
        if held.closed {
            return false;
        }
        if let Some(old) = held.bus.replace(bus.clone()) {
            let _ = old.close();
        }
        true
    }
}

impl Buses {
    /// The desktop's.
    pub(crate) fn platform() -> Self {
        Buses {
            session: Box::new(Connection::session),
            system: Box::new(Connection::system),
        }
    }
}

pub(crate) fn watch_with(post: Post, buses: Buses) -> Watch {
    let held = Arc::new(Mutex::new(Held::default()));
    let shared = Arc::clone(&held);
    // Connecting may wait on the bus, and the Engine thread never does.
    // Without the thread nothing is said, as without a monitor.
    let _ = std::thread::Builder::new()
        .name("ljos-network".to_owned())
        .spawn(move || hear(buses, &post, &shared));
    Watch { held }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let mut held = crate::lock(&self.held);
        held.closed = true;
        if let Some(bus) = held.bus.take() {
            let _ = bus.close();
        }
    }
}

fn report(post: &Post, status: NetworkStatus) {
    post.send(move |e: &Engine| ljos_platform::report_network_status(e, status));
}

fn hear(buses: Buses, post: &Post, held: &Mutex<Held>) {
    if let Ok(bus) = (buses.session)() {
        if !Held::keep(held, &bus) {
            return;
        }
        if hear_portal(&bus, post) {
            return;
        }
    }
    let Ok(bus) = (buses.system)() else {
        return;
    };
    if Held::keep(held, &bus) {
        hear_network_manager(&bus, post);
    }
}

/// `false` if there is no portal with a network monitor to hear.
fn hear_portal(bus: &Connection, post: &Post) -> bool {
    // Heard before the status is read, so no change falls between.
    let Ok(changes) = signals(bus, PORTAL, PORTAL_PATH, MONITOR, "changed", None) else {
        return false;
    };
    let Ok(status) = portal_status(bus) else {
        return false;
    };
    report(post, status);
    for message in changes {
        // The connection closed: the application is done with it.
        if message.is_err() {
            break;
        }
        if let Ok(status) = portal_status(bus) {
            report(post, status);
        }
    }
    true
}

fn signals(
    bus: &Connection,
    sender: &str,
    path: &str,
    interface: &str,
    member: &str,
    arg0: Option<&str>,
) -> zbus::Result<MessageIterator> {
    let mut rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender)?
        .path(path)?
        .interface(interface)?
        .member(member)?;
    if let Some(arg) = arg0 {
        rule = rule.arg(0, arg)?;
    }
    MessageIterator::for_match_rule(rule.build(), bus, Some(64))
}

fn call<R>(bus: &Connection, method: &str) -> zbus::Result<R>
where
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    bus.call_method(Some(PORTAL), PORTAL_PATH, Some(MONITOR), method, &())?
        .body()
        .deserialize()
}

type Status = HashMap<String, OwnedValue>;

/// `GetStatus` from version 3 of the portal; before it, one call for each.
fn portal_status(bus: &Connection) -> zbus::Result<NetworkStatus> {
    if let Ok(all) = call::<Status>(bus, "GetStatus") {
        let flag = |key: &str| all.get(key).and_then(|v| v.downcast_ref::<bool>().ok());
        let connectivity = all
            .get("connectivity")
            .and_then(|v| v.downcast_ref::<u32>().ok());
        return Ok(from_portal(
            flag("available").unwrap_or(false),
            connectivity,
            flag("metered").unwrap_or(false),
        ));
    }
    let available = call::<bool>(bus, "GetAvailable")?;
    let metered = call::<bool>(bus, "GetMetered").unwrap_or(false);
    // Version 1 has no connectivity.
    let connectivity = call::<u32>(bus, "GetConnectivity").ok();
    Ok(from_portal(available, connectivity, metered))
}

/// The portal's connectivity: 1 local, 2 limited, 3 behind a captive
/// portal, 4 full.
fn from_portal(available: bool, connectivity: Option<u32>, metered: bool) -> NetworkStatus {
    if !available {
        return NetworkStatus {
            reachability: Reachability::Disconnected,
            ..NetworkStatus::default()
        };
    }
    let (reachability, behind_captive_portal) = match connectivity {
        Some(1) => (Reachability::Local, false),
        Some(2) => (Reachability::Site, false),
        Some(3) => (Reachability::Site, true),
        // A network with a way out, of a portal too old to say how far.
        Some(4) | None => (Reachability::Online, false),
        Some(_) => (Reachability::Unknown, false),
    };
    NetworkStatus {
        reachability,
        transport_medium: TransportMedium::Unknown,
        behind_captive_portal,
        metered,
    }
}

/// What NetworkManager has said, kept so a change of one property is
/// reported with the others.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Manager {
    connectivity: u32,
    state: u32,
    metered: u32,
    primary_type: String,
}

impl Manager {
    const KEYS: [&str; 4] = ["Connectivity", "State", "Metered", "PrimaryConnectionType"];

    fn set(&mut self, key: &str, value: &OwnedValue) {
        match key {
            "Connectivity" => self.connectivity = value.downcast_ref().unwrap_or(0),
            "State" => self.state = value.downcast_ref().unwrap_or(0),
            "Metered" => self.metered = value.downcast_ref().unwrap_or(0),
            "PrimaryConnectionType" => {
                self.primary_type = value.downcast_ref::<&str>().unwrap_or("").to_owned();
            }
            _ => {}
        }
    }

    pub(crate) fn status(&self) -> NetworkStatus {
        // NMConnectivityState; unknown when NetworkManager does not check,
        // and then its NMState says how far the connection goes.
        let (reachability, behind_captive_portal) = match self.connectivity {
            1 => (Reachability::Disconnected, false),
            2 => (Reachability::Site, true),
            3 => (Reachability::Site, false),
            4 => (Reachability::Online, false),
            _ => match self.state {
                70 => (Reachability::Online, false),
                60 => (Reachability::Site, false),
                50 | 30 => (Reachability::Local, false),
                10 | 20 | 40 => (Reachability::Disconnected, false),
                _ => (Reachability::Unknown, false),
            },
        };
        let connected = !matches!(
            reachability,
            Reachability::Disconnected | Reachability::Unknown
        );
        let transport_medium = match self.primary_type.as_str() {
            _ if !connected => TransportMedium::Unknown,
            "802-3-ethernet" => TransportMedium::Ethernet,
            "802-11-wireless" => TransportMedium::WiFi,
            "gsm" | "cdma" => TransportMedium::Cellular,
            "bluetooth" => TransportMedium::Bluetooth,
            _ => TransportMedium::Unknown,
        };
        NetworkStatus {
            reachability,
            transport_medium,
            behind_captive_portal,
            // NMMetered: 1 yes, 3 guessed yes.
            metered: connected && matches!(self.metered, 1 | 3),
        }
    }
}

fn hear_network_manager(bus: &Connection, post: &Post) {
    let Ok(changes) = signals(bus, NM, NM_PATH, PROPERTIES, "PropertiesChanged", Some(NM)) else {
        return;
    };
    let Ok(all) = bus
        .call_method(Some(NM), NM_PATH, Some(PROPERTIES), "GetAll", &(NM,))
        .and_then(|reply| reply.body().deserialize::<Status>())
    else {
        return;
    };
    let mut manager = Manager::default();
    for key in Manager::KEYS {
        if let Some(value) = all.get(key) {
            manager.set(key, value);
        }
    }
    report(post, manager.status());
    for message in changes {
        let Ok(message) = message else {
            return;
        };
        let Ok((_, changed, _)) = message
            .body()
            .deserialize::<(String, Status, Vec<String>)>()
        else {
            continue;
        };
        let before = manager.clone();
        for (key, value) in &changed {
            manager.set(key, value);
        }
        if manager != before {
            report(post, manager.status());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use ljos_platform::{NetworkStatus, Platform, Reachability, TransportMedium};
    use zbus::blocking::connection;
    use zbus::zvariant::{OwnedValue, Value};

    use super::{Buses, Manager, NM, NM_PATH, PORTAL, PORTAL_PATH, watch_with};
    use crate::test_bus::Bus;

    /// A network monitor of portal version 3.
    struct Monitor(Arc<Mutex<(bool, u32, bool)>>);

    #[zbus::interface(name = "org.freedesktop.portal.NetworkMonitor")]
    impl Monitor {
        fn get_status(&self) -> HashMap<String, OwnedValue> {
            let (available, connectivity, metered) = *self.0.lock().unwrap();
            HashMap::from([
                ("available".to_owned(), OwnedValue::from(available)),
                ("connectivity".to_owned(), OwnedValue::from(connectivity)),
                ("metered".to_owned(), OwnedValue::from(metered)),
            ])
        }
    }

    /// NetworkManager on wireless, with its connectivity checked.
    struct NetworkManager;

    #[zbus::interface(name = "org.freedesktop.NetworkManager")]
    impl NetworkManager {
        #[zbus(property)]
        fn connectivity(&self) -> u32 {
            4
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            70
        }
        #[zbus(property)]
        fn metered(&self) -> u32 {
            4
        }
        #[zbus(property)]
        fn primary_connection_type(&self) -> String {
            "802-11-wireless".to_owned()
        }
    }

    /// Some other portal interface.
    struct Elsewhere;

    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl Elsewhere {}

    fn on(bus: &Bus) -> super::Connect {
        let address = bus.address.clone();
        Box::new(move || connection::Builder::address(address.as_str())?.build())
    }

    /// Delivers what the watch posts until `done`.
    struct Host {
        engine: ljos_engine::Engine,
        platform: std::rc::Rc<Platform>,
        wakes: mpsc::Receiver<()>,
    }

    impl Host {
        fn new() -> Self {
            let engine = ljos_engine::Engine::new();
            ljos_platform::register(&engine);
            let platform = Platform::of(&engine);
            let (woke, wakes) = mpsc::channel();
            platform.set_waker(move || {
                let _ = woke.send(());
            });
            Self {
                engine,
                platform,
                wakes,
            }
        }

        fn until(&self, done: impl Fn(NetworkStatus) -> bool) {
            while !done(ljos_platform::network_status(&self.engine)) {
                self.wakes
                    .recv_timeout(Duration::from_secs(10))
                    .expect("a report");
                self.platform.deliver(&self.engine);
            }
        }
    }

    #[test]
    fn the_portals_monitor_is_read_at_start_and_followed() {
        let Some(bus) = Bus::start() else {
            eprintln!("skipped: no dbus-daemon to start");
            return;
        };
        let said = Arc::new(Mutex::new((true, 4u32, false)));
        let portal = connection::Builder::address(bus.address.as_str())
            .and_then(|b| b.name(PORTAL))
            .and_then(|b| b.serve_at(PORTAL_PATH, Monitor(Arc::clone(&said))))
            .and_then(connection::Builder::build)
            .expect("the portal");
        let host = Host::new();
        let watch = watch_with(
            host.platform.poster(),
            Buses {
                session: on(&bus),
                system: Box::new(|| Err(zbus::Error::Unsupported)),
            },
        );
        host.until(|s| s.is_online());

        let change = |now: (bool, u32, bool)| {
            *said.lock().unwrap() = now;
            portal
                .emit_signal(
                    None::<&str>,
                    PORTAL_PATH,
                    "org.freedesktop.portal.NetworkMonitor",
                    "changed",
                    &(),
                )
                .expect("changed");
        };
        change((false, 1, false));
        host.until(|s| s.reachability == Reachability::Disconnected);
        change((true, 3, true));
        host.until(|s| s.behind_captive_portal && s.metered);
        assert_eq!(
            ljos_platform::network_status(&host.engine).reachability,
            Reachability::Site
        );
        drop(watch);
    }

    #[test]
    fn without_a_portal_network_manager_is_asked() {
        let (Some(session), Some(system)) = (Bus::start(), Bus::start()) else {
            eprintln!("skipped: no dbus-daemon to start");
            return;
        };
        let nm = connection::Builder::address(system.address.as_str())
            .and_then(|b| b.name(NM))
            .and_then(|b| b.serve_at(NM_PATH, NetworkManager))
            .and_then(connection::Builder::build)
            .expect("NetworkManager");
        // A portal without a network monitor, and on a bus of its own so
        // the desktop's is not started for the name.
        let _portal = connection::Builder::address(session.address.as_str())
            .and_then(|b| b.name(PORTAL))
            .and_then(|b| b.serve_at(PORTAL_PATH, Elsewhere))
            .and_then(connection::Builder::build)
            .expect("the portal");
        let host = Host::new();
        let watch = watch_with(
            host.platform.poster(),
            Buses {
                session: on(&session),
                system: on(&system),
            },
        );
        host.until(|s| s.is_online());
        assert_eq!(
            ljos_platform::network_status(&host.engine).transport_medium,
            TransportMedium::WiFi
        );

        let changed: HashMap<&str, Value<'_>> = HashMap::from([
            ("Connectivity", Value::from(1u32)),
            ("State", Value::from(20u32)),
        ]);
        nm.emit_signal(
            None::<&str>,
            NM_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(NM, changed, Vec::<String>::new()),
        )
        .expect("PropertiesChanged");
        host.until(|s| s.reachability == Reachability::Disconnected);
        assert_eq!(
            ljos_platform::network_status(&host.engine),
            NetworkStatus {
                reachability: Reachability::Disconnected,
                ..NetworkStatus::default()
            }
        );
        drop(watch);
    }

    #[test]
    fn network_manager_says_how_far_without_its_check() {
        let mut m = Manager::default();
        let set = |m: &mut Manager, key: &str, v: Value<'_>| {
            m.set(key, &OwnedValue::try_from(v).unwrap());
        };
        set(&mut m, "State", Value::from(70u32));
        set(
            &mut m,
            "PrimaryConnectionType",
            Value::from("802-3-ethernet"),
        );
        set(&mut m, "Metered", Value::from(3u32));
        assert_eq!(
            m.status(),
            NetworkStatus {
                reachability: Reachability::Online,
                transport_medium: TransportMedium::Ethernet,
                behind_captive_portal: false,
                metered: true,
            }
        );
        set(&mut m, "Connectivity", Value::from(2u32));
        assert!(m.status().behind_captive_portal);
        set(&mut m, "Connectivity", Value::from(0u32));
        set(&mut m, "State", Value::from(20u32));
        assert_eq!(m.status().reachability, Reachability::Disconnected);
        assert!(!m.status().metered);
    }
}
