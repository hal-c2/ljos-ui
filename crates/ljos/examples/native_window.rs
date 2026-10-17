//! Says what the application is handed for each window the shell opens:
//! the QML `Window`, the kind of handle, and, on X11, whether the window
//! is mapped yet (the hook runs before the first picture, so it is not)
//! and whether its blurred backdrop was asked for. The second window is
//! closed and shown again, and handed over again.
//!
//! `cargo run -p ljos --example native_window` — opens windows: run it in
//! the off-screen sandbox (`mise run desktop:cua:exec …`).

use ljos::prelude::*;
use ljos::window::NativeWindow;
use ljos::window::raw_window_handle::RawWindowHandle;

fn main() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/native_window");
    let app = ljos::App::new("Native windows")
        .on_window(|engine, window, native| {
            let name = match window.map(|w| engine.read_by_name(w, "objectName")) {
                Some(Value::String(s)) => s.to_string(),
                _ => "(the shell's)".to_owned(),
            };
            eprintln!("on_window {name}: {}", describe(native));
        })
        .qml_dir(dir, "Main")
        .expect("the example's QML compiles");
    app.run().expect("the example runs");
}

fn describe(native: &dyn NativeWindow) -> String {
    let display = native
        .display_handle()
        .map(|h| {
            format!("{:?}", h.as_raw())
                .split('(')
                .next()
                .unwrap_or("")
                .to_owned()
        })
        .unwrap_or_else(|e| format!("no display handle ({e})"));
    let Ok(handle) = native.window_handle() else {
        return format!("no window handle, display {display}");
    };
    match handle.as_raw() {
        RawWindowHandle::Xlib(h) => x11("Xlib", h.window as u32),
        RawWindowHandle::Xcb(h) => x11("Xcb", h.window.get()),
        RawWindowHandle::Wayland(_) => format!("Wayland, display {display}"),
        other => format!("{other:?}"),
    }
}

#[cfg(all(
    unix,
    not(any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "emscripten",
        target_os = "redox"
    ))
))]
fn x11(kind: &str, window: u32) -> String {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MapState};
    let Ok((conn, _)) = x11rb::connect(None) else {
        return format!("{kind} {window:#x}, the X server cannot be reached");
    };
    let mapped = conn
        .get_window_attributes(window)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.map_state != MapState::UNMAPPED);
    let blur = conn
        .intern_atom(true, b"_KDE_NET_WM_BLUR_BEHIND_REGION")
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|a| {
            conn.get_property(false, window, a.atom, AtomEnum::ANY, 0, 0)
                .ok()?
                .reply()
                .ok()
        })
        .map(|p| p.type_ != 0);
    let _ = conn.flush();
    format!("{kind} {window:#x}, mapped {mapped:?}, blur asked {blur:?}")
}

#[cfg(not(all(
    unix,
    not(any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "emscripten",
        target_os = "redox"
    ))
)))]
fn x11(kind: &str, window: u32) -> String {
    format!("{kind} {window:#x}")
}
