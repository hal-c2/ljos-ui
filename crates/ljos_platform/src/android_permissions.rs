//! What Android is asked for a permission of QtCore, and what its answer
//! means: the part of `AndroidPermissions` (built on Android only) that
//! needs no phone, so it is built and tested everywhere.
//!
//! A permission is one or more of the system's runtime permissions
//! (developer.android.com/reference/android/Manifest.permission):
//!
//! | Permission | Android's |
//! |---|---|
//! | `CameraPermission` | `CAMERA` |
//! | `MicrophonePermission` | `RECORD_AUDIO` |
//! | `LocationPermission`, approximate | `ACCESS_COARSE_LOCATION` |
//! | `LocationPermission`, precise | `ACCESS_COARSE_LOCATION` and `ACCESS_FINE_LOCATION` |
//! | `LocationPermission.Always` | then `ACCESS_BACKGROUND_LOCATION` (Android 10) |
//! | `CalendarPermission` | `READ_CALENDAR`, and `WRITE_CALENDAR` to write |
//! | `ContactsPermission` | `READ_CONTACTS`, and `WRITE_CONTACTS` to write |
//! | `BluetoothPermission.Access` | `BLUETOOTH_SCAN` and `BLUETOOTH_CONNECT` (Android 12); before, `BLUETOOTH`, `BLUETOOTH_ADMIN` and `ACCESS_FINE_LOCATION` |
//! | `BluetoothPermission.Advertise` | `BLUETOOTH_ADVERTISE` (Android 12); before, `BLUETOOTH` and `BLUETOOTH_ADMIN` |
//! | `NotificationPermission` (`Ljos.Platform`) | `POST_NOTIFICATIONS` (Android 13) |
//!
//! A permission Android added after the phone's version is one that
//! version grants when the application is installed, so it is granted;
//! one it no longer has asks nothing. Before Android 12 Bluetooth was the
//! install-time `BLUETOOTH` and `BLUETOOTH_ADMIN`, granted when the
//! manifest declares them and denied when it does not, and a scan finds
//! nothing without the precise location (Android 10 and 11; the
//! approximate one did before), so the Bluetooth of those versions asks
//! for it.
//! Since Android 11 the background location is asked on its own, once the
//! location in use is granted: the permissions are asked in stages, each
//! after the one before was granted.
//!
//! The system says only whether a permission is granted. A permission it
//! does not grant is undetermined until the application has asked for it,
//! which the Java side remembers, and denied after. One the application's
//! manifest does not declare is denied without asking: the system would
//! deny it anyway, and show nothing.

use crate::permissions::{Permission, PermissionStatus};

/// One of Android's permissions, the API level that made it a runtime
/// one, and the last that asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub name: &'static str,
    pub since: u32,
    pub until: Option<u32>,
}

const fn runtime(name: &'static str) -> Runtime {
    Runtime {
        name,
        since: 0,
        until: None,
    }
}

/// Bluetooth's permissions before Android 12.
const fn before_android_12(name: &'static str) -> Runtime {
    Runtime {
        name,
        since: 0,
        until: Some(30),
    }
}

/// What `permission` asks of Android, a stage at a time.
pub fn stages(permission: Permission) -> Vec<Vec<Runtime>> {
    const COARSE: &str = "android.permission.ACCESS_COARSE_LOCATION";
    const FINE: &str = "android.permission.ACCESS_FINE_LOCATION";
    match permission {
        Permission::Camera => vec![vec![runtime("android.permission.CAMERA")]],
        Permission::Microphone => vec![vec![runtime("android.permission.RECORD_AUDIO")]],
        Permission::Location { precise, always } => {
            let mut in_use = vec![runtime(COARSE)];
            if precise {
                in_use.push(runtime(FINE));
            }
            let mut out = vec![in_use];
            if always {
                out.push(vec![Runtime {
                    name: "android.permission.ACCESS_BACKGROUND_LOCATION",
                    since: 29,
                    until: None,
                }]);
            }
            out
        }
        Permission::Calendar { read_write } => vec![read_or_write(
            "android.permission.READ_CALENDAR",
            "android.permission.WRITE_CALENDAR",
            read_write,
        )],
        Permission::Contacts { read_write } => vec![read_or_write(
            "android.permission.READ_CONTACTS",
            "android.permission.WRITE_CONTACTS",
            read_write,
        )],
        Permission::Bluetooth { modes } => {
            const ACCESS: i32 = 1;
            const ADVERTISE: i32 = 2;
            let since = |name| Runtime {
                name,
                since: 31,
                until: None,
            };
            let mut out = Vec::new();
            if modes & ACCESS != 0 {
                out.push(since("android.permission.BLUETOOTH_SCAN"));
                out.push(since("android.permission.BLUETOOTH_CONNECT"));
            }
            if modes & ADVERTISE != 0 {
                out.push(since("android.permission.BLUETOOTH_ADVERTISE"));
            }
            if modes & (ACCESS | ADVERTISE) != 0 {
                out.push(before_android_12("android.permission.BLUETOOTH"));
                out.push(before_android_12("android.permission.BLUETOOTH_ADMIN"));
            }
            if modes & ACCESS != 0 {
                out.push(before_android_12(FINE));
            }
            vec![out]
        }
        // Before Android 13 an application showed notifications without
        // asking; that version grants it, as `since` says.
        Permission::Notifications => vec![vec![Runtime {
            name: "android.permission.POST_NOTIFICATIONS",
            since: 33,
            until: None,
        }]],
    }
}

fn read_or_write(read: &'static str, write: &'static str, read_write: bool) -> Vec<Runtime> {
    let mut out = vec![runtime(read)];
    if read_write {
        out.push(runtime(write));
    }
    out
}

/// The stages as the Java side takes them: a line each, its permissions
/// `name@since`, or `name@since-until`, apart by spaces.
pub fn encode(stages: &[Vec<Runtime>]) -> String {
    stages
        .iter()
        .map(|stage| {
            stage
                .iter()
                .map(|p| match p.until {
                    None => format!("{}@{}", p.name, p.since),
                    Some(until) => format!("{}@{}-{until}", p.name, p.since),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The status of a permission from what the Java side says of each of
/// Android's, in order: `g` granted, `d` denied (asked for and not granted,
/// or not declared), `u` never asked for. `None` when it said something
/// else.
pub fn status_of(states: &str) -> Option<PermissionStatus> {
    if states.is_empty() || !states.bytes().all(|b| matches!(b, b'g' | b'd' | b'u')) {
        return None;
    }
    Some(if states.bytes().all(|b| b == b'g') {
        PermissionStatus::Granted
    } else if states.contains('d') {
        PermissionStatus::Denied
    } else {
        PermissionStatus::Undetermined
    })
}

#[cfg(target_os = "android")]
pub use self::java::AndroidPermissions;

/// The system's questions, through `dev.ljos.LjosPermissions` of Ljós's
/// Android template.
#[cfg(target_os = "android")]
mod java {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use ljos_jni::jni::objects::{JClass, JString, JValue};
    use ljos_jni::jni::{EnvUnowned, jni_sig, jni_str};
    use ljos_jni::{app_class, with_java};

    use super::{encode, stages, status_of};
    use crate::permissions::{Permission, PermissionAnswer, PermissionService, PermissionStatus};

    /// Android's runtime permissions, asked of the user in the system's
    /// own dialog.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct AndroidPermissions;

    fn pending() -> MutexGuard<'static, HashMap<i32, PermissionAnswer>> {
        static PENDING: OnceLock<Mutex<HashMap<i32, PermissionAnswer>>> = OnceLock::new();
        PENDING
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A request code, which an activity of AndroidX takes only in the low
    /// 16 bits.
    fn next_code() -> i32 {
        static NEXT: AtomicI32 = AtomicI32::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed).rem_euclid(0xFFFF) + 1
    }

    impl PermissionService for AndroidPermissions {
        fn status(&self, permission: Permission) -> PermissionStatus {
            let asked = encode(&stages(permission));
            let said = with_java(|env, context| {
                let class = app_class(env, context, jni_str!("dev.ljos.LjosPermissions"))?;
                let asked = env.new_string(asked)?;
                let states = env
                    .call_static_method(
                        &class,
                        jni_str!("states"),
                        jni_sig!("(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;"),
                        &[JValue::Object(context), JValue::from(&asked)],
                    )?
                    .l()?;
                if states.is_null() {
                    return Ok(String::new());
                }
                env.cast_local::<JString>(states)?.try_to_string(env)
            });
            match said.as_deref().map(status_of) {
                Ok(Some(status)) => status,
                Ok(None) | Err(_) => {
                    // Nothing is used that the system was not asked for.
                    log::warn!("the system did not say whether {permission:?} is granted");
                    PermissionStatus::Undetermined
                }
            }
        }

        fn request(&self, permission: Permission, answer: PermissionAnswer) {
            let code = next_code();
            pending().insert(code, answer);
            let asked = encode(&stages(permission));
            let shown = with_java(|env, context| {
                let class = app_class(env, context, jni_str!("dev.ljos.LjosPermissions"))?;
                let asked = env.new_string(asked)?;
                env.call_static_method(
                    &class,
                    jni_str!("request"),
                    jni_sig!("(Landroid/content/Context;Ljava/lang/String;I)Z"),
                    &[
                        JValue::Object(context),
                        JValue::from(&asked),
                        JValue::Int(code),
                    ],
                )?
                .z()
            })
            .unwrap_or_else(|e| {
                log::warn!("the system was not asked for {permission:?}: {e}");
                false
            });
            if !shown {
                // Dropped, the answer leaves the status as the system says.
                let dropped = pending().remove(&code);
                drop(dropped);
            }
        }
    }

    /// The system's answer, from `LjosPermissions`: the states of what was
    /// asked (`status_of`), or null when the question was cut short.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_dev_ljos_LjosPermissions_nativeAnswered<'caller>(
        mut unowned: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        code: i32,
        states: JString<'caller>,
    ) {
        unowned
            .with_env(|env| -> ljos_jni::jni::errors::Result<()> {
                let states = if states.is_null() {
                    None
                } else {
                    Some(states.try_to_string(env)?)
                };
                // Taken out of the lock before it is sent, which posts to
                // the application's inbox.
                let Some(waiting) = pending().remove(&code) else {
                    return Ok(());
                };
                match states.as_deref().and_then(status_of) {
                    Some(status) => waiting.send(status),
                    None => drop(waiting),
                }
                Ok(())
            })
            .resolve::<ljos_jni::jni::errors::ThrowRuntimeExAndDefault>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(permission: Permission) -> Vec<Vec<&'static str>> {
        stages(permission)
            .iter()
            .map(|s| s.iter().map(|p| p.name).collect())
            .collect()
    }

    #[test]
    fn the_camera_and_the_microphone_are_one_permission_each() {
        assert_eq!(names(Permission::Camera), [["android.permission.CAMERA"]]);
        assert_eq!(
            names(Permission::Microphone),
            [["android.permission.RECORD_AUDIO"]]
        );
    }

    #[test]
    fn a_precise_location_asks_for_both_and_always_asks_after() {
        let coarse = "android.permission.ACCESS_COARSE_LOCATION";
        let fine = "android.permission.ACCESS_FINE_LOCATION";
        let location = |precise, always| names(Permission::Location { precise, always });
        assert_eq!(location(false, false), vec![vec![coarse]]);
        // Android 12 grants the fine location only asked with the coarse.
        assert_eq!(location(true, false), vec![vec![coarse, fine]]);
        assert_eq!(
            location(true, true),
            vec![
                vec![coarse, fine],
                vec!["android.permission.ACCESS_BACKGROUND_LOCATION"]
            ]
        );
        let background = &stages(Permission::Location {
            precise: false,
            always: true,
        })[1][0];
        assert_eq!(background.since, 29);
    }

    #[test]
    fn writing_asks_for_reading_too() {
        assert_eq!(
            names(Permission::Calendar { read_write: false }),
            [["android.permission.READ_CALENDAR"]]
        );
        assert_eq!(
            names(Permission::Contacts { read_write: true }),
            [[
                "android.permission.READ_CONTACTS",
                "android.permission.WRITE_CONTACTS"
            ]]
        );
    }

    #[test]
    fn bluetooth_asks_for_what_its_modes_do_since_android_12() {
        let access = stages(Permission::Bluetooth { modes: 1 });
        let since_12: Vec<_> = access[0]
            .iter()
            .filter(|p| p.since == 31)
            .map(|p| p.name)
            .collect();
        assert_eq!(
            since_12,
            [
                "android.permission.BLUETOOTH_SCAN",
                "android.permission.BLUETOOTH_CONNECT"
            ]
        );
        assert!(
            access[0]
                .iter()
                .all(|p| p.since == 31 || p.until == Some(30))
        );
        let advertise = stages(Permission::Bluetooth { modes: 2 });
        assert_eq!(
            advertise[0]
                .iter()
                .filter(|p| p.since == 31)
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            ["android.permission.BLUETOOTH_ADVERTISE"]
        );
        assert_eq!(names(Permission::Bluetooth { modes: 3 })[0].len(), 6);
    }

    /// What a phone of `api` asks for: what Java's `applies` keeps.
    fn on(api: u32, permission: Permission) -> Vec<&'static str> {
        stages(permission)
            .concat()
            .into_iter()
            .filter(|p| p.since <= api && p.until.is_none_or(|until| api <= until))
            .map(|p| p.name)
            .collect()
    }

    #[test]
    fn bluetooth_before_android_12_is_the_precise_location_and_the_manifests() {
        let access = Permission::Bluetooth { modes: 1 };
        // Android 11: a scan finds nothing without the precise location.
        assert_eq!(
            on(30, access),
            [
                "android.permission.BLUETOOTH",
                "android.permission.BLUETOOTH_ADMIN",
                "android.permission.ACCESS_FINE_LOCATION"
            ]
        );
        assert_eq!(
            on(31, access),
            [
                "android.permission.BLUETOOTH_SCAN",
                "android.permission.BLUETOOTH_CONNECT"
            ]
        );
        assert_eq!(
            on(30, Permission::Bluetooth { modes: 2 }),
            [
                "android.permission.BLUETOOTH",
                "android.permission.BLUETOOTH_ADMIN"
            ]
        );
        assert!(
            encode(&stages(access)).contains("android.permission.ACCESS_FINE_LOCATION@0-30"),
            "{}",
            encode(&stages(access))
        );
    }

    #[test]
    fn notifications_are_asked_for_since_android_13() {
        assert_eq!(
            names(Permission::Notifications),
            [["android.permission.POST_NOTIFICATIONS"]]
        );
        assert!(on(32, Permission::Notifications).is_empty());
        assert_eq!(
            on(33, Permission::Notifications),
            ["android.permission.POST_NOTIFICATIONS"]
        );
        assert_eq!(
            encode(&stages(Permission::Notifications)),
            "android.permission.POST_NOTIFICATIONS@33"
        );
    }

    #[test]
    fn the_stages_are_a_line_each() {
        assert_eq!(
            encode(&stages(Permission::Location {
                precise: true,
                always: true
            })),
            "android.permission.ACCESS_COARSE_LOCATION@0 \
             android.permission.ACCESS_FINE_LOCATION@0\n\
             android.permission.ACCESS_BACKGROUND_LOCATION@29"
        );
    }

    #[test]
    fn granted_takes_every_one_and_denied_any_one() {
        assert_eq!(status_of("g"), Some(PermissionStatus::Granted));
        assert_eq!(status_of("gg"), Some(PermissionStatus::Granted));
        assert_eq!(status_of("gu"), Some(PermissionStatus::Undetermined));
        assert_eq!(status_of("u"), Some(PermissionStatus::Undetermined));
        assert_eq!(status_of("gud"), Some(PermissionStatus::Denied));
        assert_eq!(status_of("d"), Some(PermissionStatus::Denied));
    }

    #[test]
    fn anything_else_is_no_answer() {
        assert_eq!(status_of(""), None);
        assert_eq!(status_of("gx"), None);
        assert_eq!(status_of("granted"), None);
    }
}
