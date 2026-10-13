//! The permissions of QtCore (doc.qt.io/qt-6/permissions.html): what an
//! application asks before it uses the camera, the microphone, the user's
//! location, calendar or contacts, or Bluetooth.
//!
//! The system decides through a [`PermissionService`]. A permission's
//! `status` is what the service says now; `request()` asks it, and the
//! answer arrives later through the platform's inbox, as a dialog's does.
//! Without a host's service nothing guards anything ([`UngatedPermissions`]):
//! Qt 6.11 on desktop Linux reports every permission granted from the start
//! and asking changes nothing (measured).
//!
//! A permission is made granted, as that system says, and is asked once it
//! is complete. Its change handlers are connected by then, so a system
//! that says otherwise is heard as one `statusChanged` while the object is
//! made; the engine gives a native type no earlier moment that knows its
//! engine.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, QtObject, Value, qml_impl,
};

use crate::platform::{CATEGORY, Platform, Reply};

/// `Qt.PermissionStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PermissionStatus {
    #[default]
    Undetermined,
    Granted,
    Denied,
}

impl PermissionStatus {
    fn to_qt(self) -> i32 {
        match self {
            PermissionStatus::Undetermined => 0,
            PermissionStatus::Granted => 1,
            PermissionStatus::Denied => 2,
        }
    }
}

/// What is asked for, with what the QML type says about how much.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permission {
    Camera,
    Microphone,
    Location {
        precise: bool,
        always: bool,
    },
    Calendar {
        read_write: bool,
    },
    Contacts {
        read_write: bool,
    },
    /// `BluetoothPermission.CommunicationModes`.
    Bluetooth {
        modes: i32,
    },
}

impl Permission {
    /// How a scenario names it.
    pub fn describe(self) -> &'static str {
        match self {
            Permission::Camera => "camera",
            Permission::Microphone => "microphone",
            Permission::Location { .. } => "location",
            Permission::Calendar { .. } => "calendar",
            Permission::Contacts { .. } => "contacts",
            Permission::Bluetooth { .. } => "bluetooth",
        }
    }
}

/// Where the user's answer to one request goes. Sent to whatever asks the
/// user; dropping it unanswered leaves the status as the system says it is.
#[derive(Debug)]
pub struct PermissionAnswer {
    reply: Option<Reply>,
    obj: ObjectId,
}

impl PermissionAnswer {
    pub fn send(mut self, status: PermissionStatus) {
        if let Some(reply) = self.reply.take() {
            let obj = self.obj;
            reply.send(move |e| answered(e, obj, Some(status)));
        }
    }
}

impl Drop for PermissionAnswer {
    fn drop(&mut self) {
        if let Some(reply) = self.reply.take() {
            let obj = self.obj;
            reply.send(move |e| answered(e, obj, None));
        }
    }
}

/// The system's guard over what an application may use.
pub trait PermissionService {
    /// What the system says of `permission` now, without asking anyone.
    fn status(&self, permission: Permission) -> PermissionStatus;

    /// Asks the user, without waiting; the answer comes through `answer`,
    /// from any thread.
    fn request(&self, permission: Permission, answer: PermissionAnswer);
}

/// A system that guards nothing: every permission is granted, as Qt
/// reports on desktop Linux. A host on a system that asks (macOS, iOS,
/// Android) installs a service of that system instead.
#[derive(Clone, Copy, Debug, Default)]
pub struct UngatedPermissions;

impl PermissionService for UngatedPermissions {
    fn status(&self, _permission: Permission) -> PermissionStatus {
        PermissionStatus::Granted
    }

    fn request(&self, _permission: Permission, answer: PermissionAnswer) {
        answer.send(PermissionStatus::Granted);
    }
}

#[derive(Debug, Default)]
struct Asking {
    decided: Vec<(&'static str, PermissionStatus)>,
    asked: Vec<(Permission, PermissionAnswer)>,
}

/// A system that asks the user, answered by a test. Nothing is decided
/// until the user answers; clones share what is asked.
#[derive(Clone, Debug, Default)]
pub struct FakePermissions {
    state: Rc<RefCell<Asking>>,
}

impl FakePermissions {
    /// The permissions the user is being asked for, oldest first, by
    /// [`Permission::describe`].
    pub fn asked(&self) -> Vec<&'static str> {
        self.state
            .borrow()
            .asked
            .iter()
            .map(|(p, _)| p.describe())
            .collect()
    }

    /// The user answers the oldest request for `name` (`"camera"`), and the
    /// system remembers the answer. `false` when nothing asks for it.
    pub fn answer(&self, name: &str, granted: bool) -> bool {
        let status = if granted {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        };
        let answer = {
            let mut st = self.state.borrow_mut();
            let Some(at) = st.asked.iter().position(|(p, _)| p.describe() == name) else {
                return false;
            };
            let (permission, answer) = st.asked.remove(at);
            st.decided.retain(|(n, _)| *n != permission.describe());
            st.decided.push((permission.describe(), status));
            answer
        };
        answer.send(status);
        true
    }
}

impl PermissionService for FakePermissions {
    fn status(&self, permission: Permission) -> PermissionStatus {
        self.state
            .borrow()
            .decided
            .iter()
            .find(|(n, _)| *n == permission.describe())
            .map_or(PermissionStatus::Undetermined, |(_, s)| *s)
    }

    fn request(&self, permission: Permission, answer: PermissionAnswer) {
        self.state.borrow_mut().asked.push((permission, answer));
    }
}

// ---------------------------------------------------------------- QML types

fn location_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("Accuracy", vec![("Approximate", 0), ("Precise", 1)]),
        ("Availability", vec![("WhenInUse", 0), ("Always", 1)]),
    ]
}

fn access_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("AccessMode", vec![("ReadOnly", 0), ("ReadWrite", 1)])]
}

fn bluetooth_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "CommunicationMode",
        vec![("Access", 1), ("Advertise", 2), ("Default", 3)],
    )]
}

/// The status a permission is made with.
const MADE: i32 = 1;

/// `CameraPermission` (doc.qt.io/qt-6/qml-qtcore-camerapermission.html).
#[derive(Debug, QmlObject)]
#[qml(module = "QtCore", name = "CameraPermission", extends = QtObject)]
pub struct CameraPermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
}

impl Default for CameraPermission {
    fn default() -> Self {
        CameraPermission {
            object: QtObject::default(),
            status: MADE,
        }
    }
}

#[qml_impl]
impl CameraPermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(cx, &[])
    }
}

/// `MicrophonePermission`
/// (doc.qt.io/qt-6/qml-qtcore-microphonepermission.html).
#[derive(Debug, QmlObject)]
#[qml(module = "QtCore", name = "MicrophonePermission", extends = QtObject)]
pub struct MicrophonePermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
}

impl Default for MicrophonePermission {
    fn default() -> Self {
        MicrophonePermission {
            object: QtObject::default(),
            status: MADE,
        }
    }
}

#[qml_impl]
impl MicrophonePermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(cx, &[])
    }
}

/// `LocationPermission` (doc.qt.io/qt-6/qml-qtcore-locationpermission.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtCore",
    name = "LocationPermission",
    extends = QtObject,
    enums = location_enums
)]
pub struct LocationPermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
    #[qml(notify, enumeration = "Accuracy")]
    pub accuracy: i32,
    #[qml(notify, enumeration = "Availability")]
    pub availability: i32,
}

impl Default for LocationPermission {
    fn default() -> Self {
        LocationPermission {
            object: QtObject::default(),
            status: MADE,
            accuracy: 0,
            availability: 0,
        }
    }
}

#[qml_impl]
impl LocationPermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(
            cx,
            &[
                LocationPermission::ACCURACY.index,
                LocationPermission::AVAILABILITY.index,
            ],
        )
    }
}

/// `CalendarPermission` (doc.qt.io/qt-6/qml-qtcore-calendarpermission.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtCore",
    name = "CalendarPermission",
    extends = QtObject,
    enums = access_enums
)]
pub struct CalendarPermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
    #[qml(notify, enumeration = "AccessMode")]
    pub access_mode: i32,
}

impl Default for CalendarPermission {
    fn default() -> Self {
        CalendarPermission {
            object: QtObject::default(),
            status: MADE,
            access_mode: 0,
        }
    }
}

#[qml_impl]
impl CalendarPermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(cx, &[CalendarPermission::ACCESS_MODE.index])
    }
}

/// `ContactsPermission` (doc.qt.io/qt-6/qml-qtcore-contactspermission.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtCore",
    name = "ContactsPermission",
    extends = QtObject,
    enums = access_enums
)]
pub struct ContactsPermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
    #[qml(notify, enumeration = "AccessMode")]
    pub access_mode: i32,
}

impl Default for ContactsPermission {
    fn default() -> Self {
        ContactsPermission {
            object: QtObject::default(),
            status: MADE,
            access_mode: 0,
        }
    }
}

#[qml_impl]
impl ContactsPermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(cx, &[ContactsPermission::ACCESS_MODE.index])
    }
}

/// `BluetoothPermission`
/// (doc.qt.io/qt-6/qml-qtcore-bluetoothpermission.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtCore",
    name = "BluetoothPermission",
    extends = QtObject,
    enums = bluetooth_enums
)]
pub struct BluetoothPermission {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Qt.PermissionStatus")]
    pub status: i32,
    #[qml(notify, flags = "CommunicationMode")]
    pub communication_modes: i32,
}

impl Default for BluetoothPermission {
    fn default() -> Self {
        BluetoothPermission {
            object: QtObject::default(),
            status: MADE,
            // Access and Advertise: what Qt starts with.
            communication_modes: 3,
        }
    }
}

#[qml_impl]
impl BluetoothPermission {
    #[qml(invokable)]
    fn request(cx: &Cx) -> Result<(), EngineError> {
        request(cx.engine, cx.this);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        complete(cx, &[BluetoothPermission::COMMUNICATION_MODES.index])
    }
}

// ---------------------------------------------------------------- behaviour

fn is<T: ljos_engine::QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

fn int(engine: &Engine, obj: ObjectId, name: &str) -> i32 {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_number() as i32
}

/// What `obj` asks for, as its properties say now.
fn permission_of(engine: &Engine, obj: ObjectId) -> Option<Permission> {
    Some(if is::<CameraPermission>(engine, obj) {
        Permission::Camera
    } else if is::<MicrophonePermission>(engine, obj) {
        Permission::Microphone
    } else if is::<LocationPermission>(engine, obj) {
        Permission::Location {
            precise: int(engine, obj, "accuracy") == 1,
            always: int(engine, obj, "availability") == 1,
        }
    } else if is::<CalendarPermission>(engine, obj) {
        Permission::Calendar {
            read_write: int(engine, obj, "accessMode") == 1,
        }
    } else if is::<ContactsPermission>(engine, obj) {
        Permission::Contacts {
            read_write: int(engine, obj, "accessMode") == 1,
        }
    } else if is::<BluetoothPermission>(engine, obj) {
        Permission::Bluetooth {
            modes: int(engine, obj, "communicationModes"),
        }
    } else {
        return None;
    })
}

/// Writes `status` into `obj`; `statusChanged` only when it changed.
fn show_status(engine: &Engine, obj: ObjectId, status: PermissionStatus) {
    if !engine.is_alive(obj) {
        return;
    }
    if int(engine, obj, "status") == status.to_qt() {
        return;
    }
    // Read-only to QML; the system writes it.
    let written = engine.property_index(obj, "status").and_then(|index| {
        engine.write_internal(
            obj,
            index,
            Value::Int(status.to_qt()),
            ljos_reactive::WriteKind::Imperative,
        )
    });
    if let Err(err) = written {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!("a permission's status: {err}"),
        );
    }
}

/// Asks the system again what it says of `obj`.
fn check(engine: &Engine, obj: ObjectId) {
    if let Some(permission) = permission_of(engine, obj) {
        let status = Platform::of(engine).permissions().status(permission);
        show_status(engine, obj, status);
    }
}

/// The status as the system says it once the object is complete, and
/// again whenever what it asks for changes.
fn complete(cx: &Cx, asks: &[ljos_engine::PropIndex]) -> Result<(), EngineError> {
    let me = cx.this;
    for &prop in asks {
        cx.engine.watch(me, prop, move |e, _| check(e, me))?;
    }
    check(cx.engine, me);
    Ok(())
}

fn request(engine: &Engine, obj: ObjectId) {
    let Some(permission) = permission_of(engine, obj) else {
        return;
    };
    let platform = Platform::of(engine);
    platform.permissions().request(
        permission,
        PermissionAnswer {
            reply: Some(platform.expect()),
            obj,
        },
    );
}

/// The user answered; without an answer the system is asked again.
fn answered(engine: &Engine, obj: ObjectId, status: Option<PermissionStatus>) {
    match status {
        Some(status) => show_status(engine, obj, status),
        None => check(engine, obj),
    }
}
