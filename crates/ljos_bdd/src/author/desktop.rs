//! The author's desktop as their commands meet it: a session bus of the
//! scenario's own with a settings portal that says what the scenario set,
//! so what the developer's own desktop asks for (reduced motion, say)
//! never reaches a scenario.

use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use ljos_child::Reaped;
use zbus::blocking::{Connection, connection};
use zbus::zvariant::{OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";

/// A value of a desktop setting, of the types portals send.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    Text(String),
    Uint(u32),
    Bool(bool),
}

impl Setting {
    fn value(&self) -> Option<OwnedValue> {
        match self {
            Setting::Text(s) => OwnedValue::try_from(Value::from(s.as_str())).ok(),
            Setting::Uint(n) => Some(OwnedValue::from(*n)),
            Setting::Bool(b) => Some(OwnedValue::from(*b)),
        }
    }
}

/// The settings, by namespace and key.
pub type Settings = BTreeMap<(String, String), Setting>;

type All = HashMap<String, HashMap<String, OwnedValue>>;

struct Portal(Settings);

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl Portal {
    fn read_all(&self, namespaces: Vec<String>) -> All {
        let mut all: All = HashMap::new();
        for ((namespace, key), setting) in &self.0 {
            if !namespaces.is_empty() && !namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = setting.value() {
                all.entry(namespace.clone())
                    .or_default()
                    .insert(key.clone(), value);
            }
        }
        all
    }
}

/// A bus with the portal on it, while it lives.
pub struct Desktop {
    /// The bus, ended with the desktop.
    _daemon: Reaped,
    /// Holds the portal's name on the bus.
    _portal: Connection,
    pub address: String,
}

impl Desktop {
    /// Starts a bus named after `dir` that says `settings`.
    ///
    /// # Errors
    /// `dbus-daemon` cannot be started, or the portal cannot be served.
    pub fn start(dir: &Path, settings: &Settings) -> Result<Self, String> {
        // An abstract socket: a path inside a scenario's directory can be
        // longer than a socket's address may be. The directory's name is
        // the scenario's own, so two scenarios never share a bus.
        let name = dir.file_name().map_or_else(
            || "ljos-bdd-desktop".into(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut daemon = Reaped::spawn(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .arg(format!("--address=unix:abstract={name}"))
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        )
        .map_err(|e| format!("cannot start dbus-daemon for the desktop's settings: {e}"))?;
        let mut address = String::new();
        let read = daemon
            .stdout
            .take()
            .map(|out| BufReader::new(out).read_line(&mut address));
        if !matches!(read, Some(Ok(n)) if n > 0) {
            return Err("dbus-daemon did not say its address".to_owned());
        }
        let address = address.trim().to_owned();
        let portal = connection::Builder::address(address.as_str())
            .and_then(|b| b.name(PORTAL))
            .and_then(|b| b.serve_at(PATH, Portal(settings.clone())))
            .and_then(connection::Builder::build);
        match portal {
            Ok(portal) => Ok(Desktop {
                _daemon: daemon,
                _portal: portal,
                address,
            }),
            Err(e) => Err(format!("cannot serve the settings portal: {e}")),
        }
    }
}
