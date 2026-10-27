---
title: Desktop, Mobile and the Platform Host
description: Understand how Ljós separates the long-lived app core from the UI host across Linux, macOS, Windows, Android and iOS.
section: rust
order: 7
---

# Desktop, Mobile and the Platform Host

After reading this page, you will understand how Ljós separates the long-lived application core from the replaceable UI host, how `PlatformHost` and `ljos_shell` integrate with Linux, macOS, Windows, Android and iOS, and how system services work from QML and Rust.

## App core versus UI host

Ljós splits a running application into two distinct lifetimes so that mobile surface destruction never destroys your application state:

- **The app core** lives as long as the OS process. It holds the `Engine` (the QML object tree, reactive graph and JavaScript runtime), the `tokio` runtime (network connections, SQLite) and your Rust models.
- **The UI host** is created on `SurfaceCreated` and dropped on `SurfaceDestroyed` (such as when an Android Activity is suspended or recreated). It holds the egui `Context`, the GPU surface, the glyph atlas and GPU textures.

When a mobile user switches away from your application and the operating system reclaims the display surface, the GPU surface and textures are dropped, reducing memory footprint while the app core, models and background tasks stay alive. When the user returns and `SurfaceCreated` fires, the UI host is rebuilt and paints the existing scene.

If the operating system kills the backgrounded process outright, Ljós restores from a UI-state snapshot of at most about 100 KiB written on `Stop`. The snapshot records the route stack, scroll anchors (key plus offset), drafts and tabs. In QML, components opt into state persistence with `Ljos.Persist { id; property }`.

## `PlatformHost` and desktop integration

Ljós owns its windowing and lifecycle host (`PlatformHost`) rather than delegating lifecycle events to a generic wrapper, because mobile platforms require direct access to `Start`, `Stop`, `SaveState`, system bar and cutout insets, predictive back gestures, and IME text composition:

- **Host implementations.** Desktop (Linux, macOS and Windows) and iOS use `WinitHost` (built on `winit`), where a `HostApp` receives events alongside a `HostControl` and a cross-thread `Waker`. Android uses `AndroidHost` on `android-activity` with `GameActivity` (which is also required by AccessKit on Android).
- **Host events.** `PlatformHost` delivers `SurfaceCreated`, `SurfaceDestroyed`, `Resized { px, scale }`, `Insets { system_bars, cutout, ime }`, `Focus`, `AppState { Active, Inactive, Hidden, Suspended }`, `MemoryPressure`, `Back { phase, progress }`, `Text { composing, selection }`, `Touch`, `Pointer`, `Key`, `OpenUrl` and `SaveState`.

On the desktop, `ljos::App` and `Platform` expose window-system metadata without polling:

- **Desktop entry ID.** `App::desktop_file_name` (and `Platform::desktop_file_name`, corresponding to `QGuiApplication::desktopFileName`) sets the desktop entry name, defaulting to the binary's file name. The shell also sets this name as every window's Wayland app ID and X11 window class.
- **Application and window icons.** Set the application icon in code with `App::icon` (from straight RGBA pixels), `App::icon_file` or `App::icon_url` (decoded off the UI thread), or `Platform::set_application_icon`, or on the command line with `ljos run --icon <FILE>`. Individual windows can override it via `Platform::set_window_icon`. On X11 and Windows this sets winit's `_NET_WM_ICON` (and the Windows taskbar icon); on macOS it sets `NSApplication.applicationIconImage` in the Dock; on Wayland it uses `xdg-toplevel-icon-v1` (padded to a square) and falls back to the desktop entry icon on compositors without that protocol.
- **Application badge count.** `Platform::set_badge_number` sets a count on the application icon (0 clears it). On Linux, a helper thread sends `com.canonical.Unity.LauncherEntry.Update` on the session D-Bus (read by KDE Plasma and GNOME docks) and sleeps until the count changes; on macOS, it updates the Dock tile's badge label.

## System services in QML and Rust

Every operating-system service in `ljos_platform` sits behind a trait with a desktop implementation in `ljos_shell` and an in-process stand-in for headless tests:

- **Clipboard.** In QML, `TextInput` and `TextEdit` provide `copy()`, `cut()`, `paste()` and `canPaste` (refreshed when the window activates or an editor gains focus, never by polling). In Rust, `Platform::clipboard()` returns a `ClipboardService` (`ClipboardMode`, `ClipboardImage`), whose `set_text`, `set_html`, `set_image` and `clear` methods return `Result<(), ClipboardError>` (`Unavailable`, `Denied`, `Other`) so your code knows whether the write succeeded.
- **Native and fallback dialogs.** `QtQuick.Dialogs` (`FileDialog`, `FolderDialog`, `ColorDialog`, `FontDialog`, `MessageDialog`) uses native system dialogs where available and draws `QtQuick.Controls` fallback dialogs where the system provides none. A custom host can supply its own `DialogService` (`DialogRequest`, `DialogAnswer`, `DialogReply`).
- **Settings, standard paths and SQLite.** `QtCore`'s `Settings` reads initial values from disk and writes changes back on its own background thread without blocking the UI. `StandardPaths` reports system directories (following the XDG base directory specification on Linux) based on `Platform::env()` (`PathEnv`). `QtQuick.LocalStorage` provides `LocalStorage.openDatabaseSync` backed by SQLite on the background worker pool.
- **Network reachability and external URLs.** `NetworkInformation` (`import QtNetwork`) reports `reachability` (`Unknown`, `Disconnected`, `Local`, `Site`, `Online`), `transportMedium` (`Unknown`, `Ethernet`, `Cellular`, `WiFi`, `Bluetooth`), captive portal presence and whether the connection is metered, updating reactively when the system reports a change. `Qt.openUrlExternally` opens a URL in the user's chosen application (`xdg-open`, `open` or the Windows URL handler on desktop).
- **Permissions.** `QtCore` provides `CameraPermission`, `MicrophonePermission`, `LocationPermission`, `CalendarPermission`, `ContactsPermission` and `BluetoothPermission` with `status` and `request()`. On desktop Linux, every permission is granted from the start; custom hosts can implement `PermissionService` (`Permission`, `PermissionStatus`, `PermissionAnswer`).
- **System theme and external drags.** QML's `SystemPalette` (`Active`, `Disabled`, `Inactive`) and Rust's `ljos::color_scheme(engine)`, `ljos::style_hints` and `ljos::reduced_motion` expose the platform's theme and motion preferences. Hosts forward drags from other programs via `external_drag(engine, &drag)` (`ExternalDrag`, `DragData`) to `DropArea` items.

```qml
import QtQuick
import QtNetwork
import QtQuick.Dialogs

Item {
    id: root
    width: 400
    height: 300

    property bool online: NetworkInformation.reachability === NetworkInformation.Reachability.Online

    FileDialog {
        id: openDialog
        nameFilters: ["Text files (*.txt)", "All files (*)"]
    }

    Text {
        anchors.centerIn: parent
        text: root.online ? "Online" : "Offline or local"
    }
}
```

## Mobile behaviour and building

Mobile targets use platform-native physics, insets and resource budgets:

- **Gestures and `ScrollPhysics`.** A single gesture arena resolves tap, double tap, long press, pan and pinch using platform touch slop and timeouts. `Flickable.physics` defaults to the platform's `ScrollPhysics` driven by a `VelocityTracker` (spline fling and stretch overscroll on Android, exponential deceleration and rubber-band overscroll on iOS, smooth wheel on desktop).
- **Back navigation and safe-area insets.** `Ljos.Back` routes back presses and predictive back gestures (`OnBackInvokedCallback` on Android) to the topmost active consumer. Windows are edge-to-edge, and the attached `SafeArea` (`SafeArea.margins.top` and `additionalMargins`, fed via `ljos_quick::set_safe_area_insets`) reports how far an item reaches into areas covered by system bars, display cutouts or the IME keyboard.
- **Scaling and networking.** Display DPI scale is never snapped to integers, and `fontScale` scales text only. On mobile, sockets stay open in the foreground with a 10 to 30 second grace period on backgrounding, resume with a cursor, verify TLS certificates through `rustls` with `rustls-platform-verifier`, and retry with exponential backoff and jitter between 1 s and 30 s.
- **Decoded image memory budget.** Decoded images that are not currently visible are cached (least recently used first) up to a memory budget: on desktop, 1/8 of the memory available to the process (at most 256 MiB); on mobile, 6 % of available memory (at most 64 MiB). On Linux, available memory respects cgroup v2 (`memory.max`) container and systemd limits.

To run or bundle an Android application (using a Gradle template and `cargo-ndk` for `arm64-v8a` and `x86_64`, minimum API 26, producing an AAB; iOS uses a generated Xcode project and static library):

```sh
ljos android run
ljos android bundle
```

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Threads, Tasks and UiHandle](./threads-and-uihandle.md)
- [Testing a Rust and QML App](./testing-rust.md)
- [Multimedia and Sensors](./multimedia.md)
