use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use ljos_a11y::{DesktopSettings, SettingValue};
use ljos_platform::Post;
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::{OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";

pub(crate) type Connect = Box<dyn FnOnce() -> zbus::Result<Connection> + Send>;

/// The thread that hears the portal. Dropped, it closes the connection
/// the thread sleeps on, which ends it.
pub(crate) struct Watch {
    held: Arc<Mutex<Held>>,
}

#[derive(Default)]
struct Held {
    bus: Option<Connection>,
    closed: bool,
}

/// On the session bus.
pub(crate) fn watch(post: Post) -> Watch {
    watch_with(post, Box::new(Connection::session))
}

pub(crate) fn watch_with(post: Post, connect: Connect) -> Watch {
    let held = Arc::new(Mutex::new(Held::default()));
    let shared = Arc::clone(&held);
    // Connecting may wait on the bus, and the Engine thread never does.
    // Without the thread nothing is said, as without a portal.
    let _ = std::thread::Builder::new()
        .name("ljos-settings".to_owned())
        .spawn(move || hear(connect, &post, &shared));
    Watch { held }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.closed = true;
        if let Some(bus) = held.bus.take() {
            let _ = bus.close();
        }
    }
}

fn hear(connect: Connect, post: &Post, held: &Mutex<Held>) {
    let Ok(bus) = connect() else {
        return;
    };
    {
        let mut held = held.lock().unwrap_or_else(PoisonError::into_inner);
        if held.closed {
            return;
        }
        held.bus = Some(bus.clone());
    }
    // Heard before the settings are read, so no change falls between.
    let Ok(changes) = changes(&bus) else {
        return;
    };
    let mut desktop = DesktopSettings::default();
    for (namespace, keys) in read_all(&bus).unwrap_or_default() {
        for (key, value) in keys {
            if let Some(value) = setting(&value) {
                desktop.set(&namespace, &key, &value);
            }
        }
    }
    if desktop != DesktopSettings::default() {
        super::report(post, desktop.prefs());
    }
    for message in changes {
        // The connection closed: the application is done with it.
        let Ok(message) = message else {
            return;
        };
        let Ok((namespace, key, value)) =
            message.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if let Some(value) = setting(&value)
            && desktop.set(&namespace, &key, &value)
        {
            super::report(post, desktop.prefs());
        }
    }
}

fn changes(bus: &Connection) -> zbus::Result<MessageIterator> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(PORTAL)?
        .path(PATH)?
        .interface(SETTINGS)?
        .member("SettingChanged")?
        .build();
    MessageIterator::for_match_rule(rule, bus, Some(64))
}

type All = HashMap<String, HashMap<String, OwnedValue>>;

fn read_all(bus: &Connection) -> zbus::Result<All> {
    let namespaces: Vec<&str> = DesktopSettings::NAMESPACES.to_vec();
    bus.call_method(
        Some(PORTAL),
        PATH,
        Some(SETTINGS),
        "ReadAll",
        &(namespaces,),
    )?
    .body()
    .deserialize()
}

/// A value of the types the keys have. Some portals wrap a value in a
/// variant once more.
fn setting(value: &Value<'_>) -> Option<SettingValue> {
    let mut v = value;
    while let Value::Value(inner) = v {
        v = inner;
    }
    Some(match v {
        Value::Bool(b) => SettingValue::Bool(*b),
        Value::U8(n) => SettingValue::Uint(u32::from(*n)),
        Value::U16(n) => SettingValue::Uint(u32::from(*n)),
        Value::U32(n) => SettingValue::Uint(*n),
        Value::I16(n) => SettingValue::Int(i64::from(*n)),
        Value::I32(n) => SettingValue::Int(i64::from(*n)),
        Value::I64(n) => SettingValue::Int(*n),
        Value::F64(n) => SettingValue::Double(*n),
        Value::Str(s) => SettingValue::Text(s.as_str().to_owned()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Duration;

    use ljos_platform::Platform;
    use zbus::blocking::connection;
    use zbus::zvariant::{OwnedValue, Value};

    use super::{PATH, PORTAL, SETTINGS, watch_with};
    use crate::test_bus::Bus;

    /// A settings portal with GNOME's animations turned off.
    struct Portal;

    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl Portal {
        fn read_all(&self, namespaces: Vec<String>) -> super::All {
            let mut all = HashMap::new();
            if namespaces
                .iter()
                .any(|n| n == "org.gnome.desktop.interface")
            {
                let mut keys = HashMap::new();
                keys.insert("enable-animations".to_owned(), OwnedValue::from(false));
                keys.insert("font-name".to_owned(), OwnedValue::from(7u32));
                all.insert("org.gnome.desktop.interface".to_owned(), keys);
            }
            all
        }
    }

    #[test]
    fn the_portals_settings_are_read_at_start_and_followed() {
        let Some(bus) = Bus::start() else {
            eprintln!("skipped: no dbus-daemon to start");
            return;
        };
        let portal = connection::Builder::address(bus.address.as_str())
            .and_then(|b| b.name(PORTAL))
            .and_then(|b| b.serve_at(PATH, Portal))
            .and_then(connection::Builder::build)
            .expect("the portal");

        let engine = ljos_engine::Engine::new();
        let platform = Platform::of(&engine);
        let (woke, wakes) = mpsc::channel();
        platform.set_waker(move || {
            let _ = woke.send(());
        });
        let address = bus.address.clone();
        let watch = watch_with(
            platform.poster(),
            Box::new(move || connection::Builder::address(address.as_str())?.build()),
        );
        let until = |done: &dyn Fn() -> bool| {
            while !done() {
                wakes
                    .recv_timeout(Duration::from_secs(10))
                    .expect("a report");
                platform.deliver(&engine);
            }
        };
        until(&|| ljos_quick::system_prefs(&engine).reduced_motion);

        let change = |key: &str, value: Value<'_>| {
            portal
                .emit_signal(
                    None::<&str>,
                    PATH,
                    SETTINGS,
                    "SettingChanged",
                    &("org.gnome.desktop.interface", key, value),
                )
                .expect("SettingChanged");
        };
        change("enable-animations", Value::from(true));
        until(&|| !ljos_quick::system_prefs(&engine).reduced_motion);
        change("text-scaling-factor", Value::from(1.5));
        until(&|| ljos_quick::system_prefs(&engine).font_scale == 1.5);
        drop(watch);
    }
}
