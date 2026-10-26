//! Whether the applications would run with motion reduced. With it Ljós
//! moves nothing, so frames, CPU and lag under a gesture or an animation
//! would come out better than the same application earns with motion: a
//! bench run then measures something no one compares against Qt.
//!
//! The setting is read the way an application reads it, from the desktop's
//! settings portal on the session bus.

use std::collections::HashMap;
use std::time::Duration;

use ljos_a11y::{DesktopSettings, SettingValue};
use serde::{Deserialize, Serialize};
use zbus::blocking::{Connection, connection};
use zbus::zvariant::{OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";

/// A portal that does not answer in this long is taken as none, so a bus
/// that hangs does not hang the bench.
const TIMEOUT: Duration = Duration::from_secs(2);

/// The motion settings a run was measured with, in its report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    /// The desktop asks for reduced motion.
    pub reduced_motion: bool,
    /// A settings portal answered; without one an application has motion.
    pub portal: bool,
}

/// The settings on the bus at `address`, or on the session bus.
#[must_use]
pub fn read(address: Option<&str>) -> Motion {
    let Ok(bus) = connect(address) else {
        return Motion::default();
    };
    let Ok(all) = read_all(&bus) else {
        return Motion::default();
    };
    let mut desktop = DesktopSettings::default();
    for (namespace, keys) in all {
        for (key, value) in keys {
            if let Some(value) = setting(&value) {
                desktop.set(&namespace, &key, &value);
            }
        }
    }
    Motion {
        reduced_motion: desktop.prefs().reduced_motion.unwrap_or(false),
        portal: true,
    }
}

fn connect(address: Option<&str>) -> zbus::Result<Connection> {
    let builder = match address {
        Some(a) => connection::Builder::address(a)?,
        None => connection::Builder::session()?,
    };
    builder.method_timeout(TIMEOUT).build()
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
    use super::{Motion, read};

    #[test]
    fn no_bus_is_motion_and_no_portal() {
        assert_eq!(
            read(Some("unix:path=/nonexistent/ljos-bench-no-bus")),
            Motion::default()
        );
    }

    #[test]
    fn a_report_without_motion_reads_as_motion() {
        assert_eq!(
            serde_json::from_str::<Motion>("{}").ok(),
            Some(Motion::default())
        );
    }
}
