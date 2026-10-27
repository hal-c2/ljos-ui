//! The author's desktop as their commands meet it: a session bus of the
//! scenario's own with a settings portal that says what the scenario set,
//! so what the developer's own desktop asks for (reduced motion, say)
//! never reaches a scenario.

use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};

use ljos_child::Reaped;
use zbus::blocking::{Connection, connection};
use zbus::zvariant::{OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
/// How long `dbus-daemon` may take to say its address.
const ADDRESS: Duration = Duration::from_secs(10);

/// The first line `child` writes to its piped stdout, waited for `within`.
/// A child that says nothing by then is left to its owner to end, which
/// ends the read with it.
fn first_line(child: &mut Reaped, within: Duration) -> Result<String, String> {
    let out = child.stdout.take().ok_or("its output is not piped")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = BufReader::new(out).read_line(&mut line);
        let _ = tx.send(read.map(|_| line));
    });
    match rx.recv_timeout(within) {
        Ok(Ok(line)) if !line.trim().is_empty() => Ok(line.trim().to_owned()),
        Ok(Ok(_)) => Err("it ended without a word".to_owned()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(RecvTimeoutError::Timeout) => Err(format!("nothing in {} s", within.as_secs_f32())),
        Err(RecvTimeoutError::Disconnected) => Err("its reader stopped".to_owned()),
    }
}

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
        let address = first_line(&mut daemon, ADDRESS)
            .map_err(|why| format!("dbus-daemon did not say its address: {why}"))?;
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

#[cfg(all(test, unix))]
mod tests {
    use std::time::Instant;

    use super::*;

    fn piped(program: &str, args: &[&str]) -> Reaped {
        Reaped::spawn(
            Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped()),
        )
        .unwrap()
    }

    #[test]
    fn a_daemon_that_says_nothing_is_given_up_on() {
        let mut silent = piped("sleep", &["30"]);
        let started = Instant::now();
        let why = first_line(&mut silent, Duration::from_millis(200)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5), "{why}");
        assert!(why.starts_with("nothing in"), "{why}");
    }

    #[test]
    fn the_first_line_is_the_address() {
        let mut daemon = piped("echo", &["unix:abstract=x,guid=1"]);
        assert_eq!(
            first_line(&mut daemon, ADDRESS).as_deref(),
            Ok("unix:abstract=x,guid=1")
        );
    }

    #[test]
    fn a_daemon_that_ends_without_a_word_says_so() {
        let mut daemon = piped("true", &[]);
        assert_eq!(
            first_line(&mut daemon, ADDRESS).unwrap_err(),
            "it ended without a word"
        );
    }
}
