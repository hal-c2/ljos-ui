//! The services of the system an application runs on, each behind a trait
//! with an in-process stand-in, and the QML modules built on them:
//!
//! - the clipboard ([`ClipboardService`]): text, HTML, an image and the
//!   primary selection;
//! - the system's dialogs ([`DialogService`]) and `QtQuick.Dialogs`, which
//!   draws with QtQuick.Controls what the system does not show;
//! - `QtCore`: `Settings`, kept in a file a thread of its own writes, and
//!   `StandardPaths`;
//! - remote resources ([`Fetcher`]), fetched off the Engine thread;
//! - drags from other applications ([`ExternalDrag`]);
//! - the permissions of `QtCore` ([`PermissionService`]): `CameraPermission`
//!   and the rest;
//! - opening a URL in the user's application for it ([`OpenUrlService`],
//!   `Qt.openUrlExternally`);
//! - the application's badge, a count on its icon ([`BadgeService`],
//!   `QGuiApplication::setBadgeNumber`);
//! - `QtSensors`: the device's accelerometer and other motion and
//!   environment sensors, read through a [`SensorService`];
//! - `Qt.platform`.
//!
//! Answers from other threads come back through one bounded inbox that the
//! scene drains at the start of a frame ([`Platform::deliver`]); nothing
//! here polls. A host with a desktop turns on the `native` feature and
//! installs [`native::SystemClipboard`], [`native::SystemDialogs`] and
//! [`native::SystemOpener`]; a
//! host with a network turns on `http` and installs `HttpFetcher`.

mod badge;
mod clipboard;
mod dialogs;
mod drop;
mod fakes;
mod fetch;
#[cfg(feature = "http")]
mod http;
mod library;
mod network;

use library::DialogLibrary;
#[cfg(feature = "native")]
pub mod native;
mod open_url;
mod paths;
mod permissions;
mod platform;
mod sensors;
mod settings;
mod window_handle;

use std::cell::Cell;

use ljos_engine::{Engine, LogLevel, Value, Version};

pub use badge::{BadgeService, RecordedBadge};
pub use clipboard::{
    ClipboardError, ClipboardImage, ClipboardMode, ClipboardService, MemoryClipboard,
};
pub use dialogs::{
    AbstractDialog, CANCEL, ColorDialog, DialogAnswer, DialogKind, DialogReply, DialogRequest,
    DialogService, FileDialog, FolderDialog, FontDialog, MessageDialog, NO, NameFilter,
    NoSystemDialogs, OK, STANDARD_BUTTONS, YES, button_named, button_role, role, system_buttons,
};
pub use drop::{ExternalDrag, external_drag};
pub use fakes::FakeDialogs;
pub use fetch::{
    FetchError, Fetcher, Incoming, Method, NoNetwork, Request, Response, is_remote, reason_phrase,
};
#[cfg(feature = "http")]
pub use http::HttpFetcher;
pub use network::{
    NetworkInformation, NetworkStatus, Reachability, TransportMedium, network_status,
    report_network_status, set_network_monitor,
};
pub use open_url::{OpenUrl, OpenUrlService, RecordedUrls};
pub use paths::{
    GUARD_HOME_VAR, Location, PathEnv, StandardPaths, encoded_url, file_url, url_path,
};
pub use permissions::{
    BluetoothPermission, CalendarPermission, CameraPermission, ContactsPermission, FakePermissions,
    LocationPermission, MicrophonePermission, Permission, PermissionAnswer, PermissionService,
    PermissionStatus, UngatedPermissions,
};
pub use platform::{CATEGORY, Job, Platform, Post, Reply};
pub use raw_window_handle;
pub use sensors::{
    Accelerometer, AccelerometerReading, AmbientLightReading, AmbientLightSensor, Compass,
    CompassReading, FakeSensors, Gyroscope, GyroscopeReading, Magnetometer, MagnetometerReading,
    Measurement, NoSensors, OrientationReading, OrientationSensor, PressureReading, PressureSensor,
    Sensor, SensorKind, SensorReading, SensorRequest, SensorService, SensorSink,
};
pub use settings::{Settings, flush_writes};
pub use window_handle::NativeWindow;

/// Set once the modules are in an engine.
#[derive(Default)]
struct Registered(Cell<bool>);

/// Registers `QtQuick.Dialogs`, `QtCore`, `QtSensors`, `QtNetwork`'s
/// `NetworkInformation`, `Qt.platform` and
/// `Qt.openUrlExternally`. Calling it
/// again does nothing. The drawn dialogs are QtQuick.Controls, so a host
/// registers those too.
pub fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    engine.register_module(ljos_engine::module!("QtQuick.Dialogs" 6.0 => [
        FileDialog, FolderDialog, ColorDialog, FontDialog, MessageDialog,
    ]));
    engine.register_module(ljos_engine::module!("QtCore" 6.0 => [Settings]));
    engine.register_singleton::<StandardPaths>("QtCore", Version::new(6, 0));
    // The permissions came with Qt 6.5.
    engine.register_module(ljos_engine::module!("QtCore" 6.5 => [
        CameraPermission, MicrophonePermission, LocationPermission, CalendarPermission,
        ContactsPermission, BluetoothPermission,
    ]));

    sensors::register(engine);
    network::register(engine);

    let (library, undecoded) = DialogLibrary::embedded();
    let empty = library.is_empty();
    engine.add_component_library(library);
    for bad in undecoded {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!("the drawn dialogs: {bad}"),
        );
    }
    if empty {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            "QtQuick.Dialogs draws no dialogs: they were not compiled when ljos_platform was \
             built (its build script says why)",
        );
    }

    open_url::register(engine);
    engine.on_js_started(|e| {
        let platform = Platform::of(e);
        let installed = e.eval_js(INSTALL_PLATFORM).map(|install| {
            e.call_js(
                &install,
                &[Value::from(os()), Value::from(platform.plugin_name())],
            )
        });
        if let Err(err) = installed {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("Qt.platform and Qt.PermissionStatus are not available: {err}"),
            );
        }
    });
}

/// `Qt.platform.os`, as Qt names the systems.
pub fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        "linux" => "linux",
        "windows" => "windows",
        "android" => "android",
        "ios" => "ios",
        _ if cfg!(target_family = "wasm") => "wasm",
        _ => "unix",
    }
}

// The plugin name is read when the first script runs: a host names its
// window system before it loads any QML.
const INSTALL_PLATFORM: &str = r#"((os, pluginName) => {
  Object.defineProperty(Qt, "platform", {
    value: Object.freeze({ os: os, pluginName: pluginName }),
    enumerable: true,
  });
  // The keys alone (`Qt.Granted`) are the engine's; the scoped name is
  // the permissions' own.
  if (!("PermissionStatus" in Qt)) {
    Qt.PermissionStatus = Object.freeze({ Undetermined: 0, Granted: 1, Denied: 2 });
  }
})"#;

#[cfg(test)]
mod tests;
