//! Whether the system can reach the network: `NetworkInformation`
//! (QtNetwork), and the same for Rust.
//!
//! A host reports what its platform says when the application starts and
//! again each time the platform says it changed ([`report_network_status`]);
//! the shell hears the desktop's network monitor. Nothing polls: a status
//! changes when a host says so, and what read it runs again. A host that
//! says nothing leaves it at [`Reachability::Unknown`].

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId, PropIndex, QmlObject, Value, WriteKind, qml_singleton};

/// How far the system's network reaches (`NetworkInformation.Reachability`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Reachability {
    /// The system does not say, or the host cannot ask it.
    #[default]
    Unknown,
    /// No network at all.
    Disconnected,
    /// This machine and the ones on its own link.
    Local,
    /// A network, but not the internet: an intranet, or a captive portal's
    /// sign-in page.
    Site,
    /// The internet.
    Online,
}

impl Reachability {
    /// Its value in `NetworkInformation.Reachability`.
    #[must_use]
    pub fn qt_value(self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::Disconnected => 1,
            Self::Local => 2,
            Self::Site => 3,
            Self::Online => 4,
        }
    }

    /// From its value in `NetworkInformation.Reachability`; any other is
    /// unknown.
    #[must_use]
    pub fn from_qt(value: i32) -> Self {
        match value {
            1 => Self::Disconnected,
            2 => Self::Local,
            3 => Self::Site,
            4 => Self::Online,
            _ => Self::Unknown,
        }
    }
}

/// What carries the network (`NetworkInformation.TransportMedium`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransportMedium {
    #[default]
    Unknown,
    Ethernet,
    Cellular,
    WiFi,
    Bluetooth,
}

impl TransportMedium {
    #[must_use]
    pub fn qt_value(self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::Ethernet => 1,
            Self::Cellular => 2,
            Self::WiFi => 3,
            Self::Bluetooth => 4,
        }
    }

    #[must_use]
    pub fn from_qt(value: i32) -> Self {
        match value {
            1 => Self::Ethernet,
            2 => Self::Cellular,
            3 => Self::WiFi,
            4 => Self::Bluetooth,
            _ => Self::Unknown,
        }
    }
}

/// What the system says of its network.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NetworkStatus {
    pub reachability: Reachability,
    /// Unknown when the platform does not say.
    pub transport_medium: TransportMedium,
    /// A captive portal (a hotel's or an airport's sign-in page) stands
    /// between the system and the internet.
    pub behind_captive_portal: bool,
    /// The network costs by what it carries, or is limited.
    pub metered: bool,
}

impl NetworkStatus {
    /// Whether the system reaches the internet.
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.reachability == Reachability::Online
    }
}

/// `NetworkInformation` (QtNetwork): what the host says of the system's
/// network. Every member is the system's to say, so QML only reads them.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtNetwork",
    name = "NetworkInformation",
    uncreatable,
    enums = network_enums
)]
#[qml_singleton]
pub struct NetworkInformation {
    /// A `NetworkInformation.Reachability`.
    #[qml(notify, readonly)]
    pub reachability: i32,
    #[qml(notify, readonly)]
    pub is_behind_captive_portal: bool,
    /// A `NetworkInformation.TransportMedium`.
    #[qml(notify, readonly)]
    pub transport_medium: i32,
    #[qml(notify, readonly)]
    pub is_metered: bool,
}

// Not Qt's `Feature`: its keys are the enums' names, which a flat
// `NetworkInformation.Reachability` would make two things, and every
// feature is said here anyway.
fn network_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Reachability",
            vec![
                ("Unknown", 0),
                ("Disconnected", 1),
                ("Local", 2),
                ("Site", 3),
                ("Online", 4),
            ],
        ),
        (
            "TransportMedium",
            vec![
                ("Unknown", 0),
                ("Ethernet", 1),
                ("Cellular", 2),
                ("WiFi", 3),
                ("Bluetooth", 4),
            ],
        ),
    ]
}

impl NetworkInformation {
    fn of(status: NetworkStatus) -> Self {
        Self {
            reachability: status.reachability.qt_value(),
            is_behind_captive_portal: status.behind_captive_portal,
            transport_medium: status.transport_medium.qt_value(),
            is_metered: status.metered,
        }
    }
}

/// Per engine: what the host said last.
#[derive(Default)]
struct State {
    status: Cell<NetworkStatus>,
}

fn state(engine: &Engine) -> Rc<State> {
    engine.extension::<State>()
}

pub(crate) fn register(engine: &Engine) {
    engine.register_singleton_with("QtNetwork", ljos_engine::Version::new(6, 0), |e| {
        NetworkInformation::of(state(e).status.get())
    });
}

/// The engine's `NetworkInformation`, if this engine has the type.
fn object(engine: &Engine) -> Option<ObjectId> {
    engine
        .untrack(|| engine.singleton::<NetworkInformation>())
        .ok()
}

/// What the platform says of its network, as a host heard it: at start
/// and whenever the platform reports a change. Telling the same again
/// costs nothing.
pub fn report_network_status(engine: &Engine, status: NetworkStatus) {
    let s = state(engine);
    if s.status.replace(status) == status {
        return;
    }
    let Some(obj) = object(engine) else {
        return;
    };
    let set = |prop: PropIndex, v: Value| {
        let _ = engine.write_internal(obj, prop, v, WriteKind::Imperative);
    };
    engine.batch(|| {
        set(
            NetworkInformation::REACHABILITY.index,
            Value::Int(status.reachability.qt_value()),
        );
        set(
            NetworkInformation::IS_BEHIND_CAPTIVE_PORTAL.index,
            Value::Bool(status.behind_captive_portal),
        );
        set(
            NetworkInformation::TRANSPORT_MEDIUM.index,
            Value::Int(status.transport_medium.qt_value()),
        );
        set(
            NetworkInformation::IS_METERED.index,
            Value::Bool(status.metered),
        );
    });
    engine.request_wake();
}

/// What the system says of its network now. Tracked: a binding or an
/// effect that asks runs again when it changes.
pub fn network_status(engine: &Engine) -> NetworkStatus {
    let Some(obj) = object(engine) else {
        return state(engine).status.get();
    };
    let read = |prop: PropIndex| engine.read(obj, prop);
    NetworkStatus {
        reachability: Reachability::from_qt(
            read(NetworkInformation::REACHABILITY.index).to_number() as i32,
        ),
        behind_captive_portal: read(NetworkInformation::IS_BEHIND_CAPTIVE_PORTAL.index)
            .to_boolean(),
        transport_medium: TransportMedium::from_qt(
            read(NetworkInformation::TRANSPORT_MEDIUM.index).to_number() as i32,
        ),
        metered: read(NetworkInformation::IS_METERED.index).to_boolean(),
    }
}
