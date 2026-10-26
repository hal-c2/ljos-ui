//! Whether EGL would draw only on the processor, told without making an
//! EGL display. Making one maps Mesa's software rasteriser and its LLVM,
//! the memory a window drawn on the processor saves (ARCHITECTURE §7.1,
//! docs/BENCHMARKS.md SHM1), so the shell asks what Mesa would ask.
//!
//! Mesa draws in software when the user tells it to, when there is no GPU
//! the process may open, and when the window system takes no buffers from
//! a GPU: a Wayland compositor that offers neither `zwp_linux_dmabuf_v1`
//! nor `wl_drm` (one that composes in software), an X server with neither
//! DRI3 nor DRI2 (Xvfb).

use std::path::Path;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

/// Where the GPUs a process may draw with are.
const RENDER_NODES: &str = "/dev/dri";

/// Whether EGL on `display` would offer only a software rasteriser.
pub(crate) fn only_software(display: &OwnedDisplayHandle) -> bool {
    told(|name| std::env::var(name).ok())
        || !has_gpu(Path::new(RENDER_NODES))
        || window_system_takes_gpu_buffers(display) == Some(false)
}

/// Whether the user told Mesa to draw in software.
fn told(var: impl Fn(&str) -> Option<String>) -> bool {
    let software = var("LIBGL_ALWAYS_SOFTWARE").is_some_and(|v| {
        ["1", "true", "yes", "y"]
            .iter()
            .any(|t| v.trim().eq_ignore_ascii_case(t))
    });
    let driver = var("MESA_LOADER_DRIVER_OVERRIDE").is_some_and(|v| {
        ["swrast", "kms_swrast", "llvmpipe", "softpipe"]
            .iter()
            .any(|d| v.trim().eq_ignore_ascii_case(d))
    });
    software || driver
}

/// Whether `dir` has a render node this process may open.
fn has_gpu(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with("renderD")
            && std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(entry.path())
                .is_ok()
    })
}

/// Whether the window system takes buffers a GPU drew; `None` when that
/// cannot be told.
fn window_system_takes_gpu_buffers(display: &OwnedDisplayHandle) -> Option<bool> {
    match display.display_handle().ok()?.as_raw() {
        RawDisplayHandle::Wayland(handle) => {
            // SAFETY: winit's display stays connected while its event
            // loop lives, and the connection made here goes before this
            // returns.
            let interfaces = unsafe { wayland::globals(handle.display.as_ptr().cast()) }?;
            Some(wayland::takes_gpu_buffers(&interfaces))
        }
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => x11::takes_gpu_buffers(),
        _ => None,
    }
}

mod wayland {
    use wayland_client::backend::Backend;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct State;

    /// The interfaces of the compositor's globals.
    ///
    /// # Safety
    /// `display` is a connected `wl_display` that outlives the call.
    pub(super) unsafe fn globals(display: *mut std::ffi::c_void) -> Option<Vec<String>> {
        // SAFETY: as the caller says.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, _queue) = registry_queue_init::<State>(&conn).ok()?;
        Some(
            globals
                .contents()
                .with_list(|list| list.iter().map(|g| g.interface.clone()).collect()),
        )
    }

    /// Whether a compositor with these globals takes buffers a GPU drew.
    pub(super) fn takes_gpu_buffers(interfaces: &[String]) -> bool {
        interfaces
            .iter()
            .any(|i| i == "zwp_linux_dmabuf_v1" || i == "wl_drm")
    }

    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
}

mod x11 {
    use x11rb::protocol::xproto::ConnectionExt;

    /// Whether the X server takes buffers a GPU drew, asked on a
    /// connection of its own: winit's is Xlib's.
    pub(super) fn takes_gpu_buffers() -> Option<bool> {
        let (conn, _) = x11rb::connect(None).ok()?;
        let has = |name: &[u8]| {
            conn.query_extension(name)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| reply.present)
        };
        Some(has(b"DRI3")? || has(b"DRI2")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn the_user_can_tell_mesa_to_draw_in_software() {
        assert!(!told(env(&[])));
        assert!(told(env(&[("LIBGL_ALWAYS_SOFTWARE", "1")])));
        assert!(told(env(&[("LIBGL_ALWAYS_SOFTWARE", " True ")])));
        assert!(!told(env(&[("LIBGL_ALWAYS_SOFTWARE", "0")])));
        assert!(told(env(&[("MESA_LOADER_DRIVER_OVERRIDE", "kms_swrast")])));
        assert!(!told(env(&[("MESA_LOADER_DRIVER_OVERRIDE", "radeonsi")])));
    }

    #[test]
    fn a_gpu_is_a_render_node_that_opens() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_gpu(dir.path()));
        assert!(!has_gpu(&dir.path().join("missing")));
        std::fs::write(dir.path().join("card0"), b"").unwrap();
        assert!(!has_gpu(dir.path()));
        std::fs::write(dir.path().join("renderD128"), b"").unwrap();
        assert!(has_gpu(dir.path()));
    }

    #[test]
    fn a_compositor_without_dmabuf_or_wl_drm_takes_no_gpu_buffers() {
        let globals = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert!(!wayland::takes_gpu_buffers(&globals(&[
            "wl_compositor",
            "wl_shm",
            "xdg_wm_base"
        ])));
        assert!(wayland::takes_gpu_buffers(&globals(&[
            "wl_shm",
            "zwp_linux_dmabuf_v1"
        ])));
        assert!(wayland::takes_gpu_buffers(&globals(&["wl_drm"])));
    }
}
