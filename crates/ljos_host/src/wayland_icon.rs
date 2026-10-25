//! Window icons on Wayland, which winit does not set: the staging
//! `xdg-toplevel-icon-v1` protocol, on the connection winit opened. It is
//! bound when the first icon is given, so that an application without one
//! costs the compositor nothing. A compositor without the protocol shows
//! what the window's app id (its desktop entry) names.
//!
//! The protocol's events (the sizes the compositor prefers, a buffer let
//! go) wait in a queue of their own and are read and dropped at the next
//! icon: an icon is drawn at the size it was given and scaled by the
//! compositor.

use std::collections::HashMap;
use std::io::Write;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};

use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_v1::XdgToplevelIconV1;
use winit::platform::wayland::WindowExtWayland;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::Window;

use crate::window::{Icon, WindowId};

/// The icons set on windows, and the protocol they are set with.
#[derive(Default)]
pub(crate) struct Icons {
    /// `Some(None)` once the compositor was found to have no protocol.
    bound: Option<Option<Bound>>,
    shown: HashMap<WindowId, Shown>,
}

struct Bound {
    conn: Connection,
    queue: EventQueue<State>,
    shm: WlShm,
    manager: XdgToplevelIconManagerV1,
}

/// An icon set on a window, with the buffer it is drawn from: the
/// protocol has the buffer outlive the icon.
struct Shown {
    icon: XdgToplevelIconV1,
    buffer: WlBuffer,
}

impl Shown {
    fn destroy(self) {
        self.icon.destroy();
        self.buffer.destroy();
    }
}

struct State;

impl Icons {
    /// Shows `icon` for `window`, or gives it back the desktop's; nothing
    /// for a window that is not Wayland's. With `commit` the window's
    /// surface is committed for the icon to be taken now; without it the
    /// window takes it with the first picture it shows.
    pub(crate) fn set(&mut self, id: WindowId, window: &Window, icon: Option<&Icon>, commit: bool) {
        let Some(toplevel) = window.xdg_toplevel() else {
            return;
        };
        if self.bound.is_none() {
            if icon.is_none() {
                return;
            }
            self.bound = Some(bind(window));
        }
        let Some(Some(b)) = &mut self.bound else {
            return;
        };
        // An error is a gone compositor, which winit hears of too.
        let _ = b.queue.dispatch_pending(&mut State);
        // SAFETY: winit's toplevel lives as long as its window, which the
        // caller holds; the proxy made here sends one request with it.
        let Ok(oid) =
            (unsafe { ObjectId::from_ptr(XdgToplevel::interface(), toplevel.as_ptr().cast()) })
        else {
            return;
        };
        let Ok(toplevel) = XdgToplevel::from_id(&b.conn, oid) else {
            return;
        };
        let shown = icon.and_then(|icon| b.make(icon));
        b.manager
            .set_icon(&toplevel, shown.as_ref().map(|s| &s.icon));
        if commit && let Some(surface) = surface(&b.conn, window) {
            surface.commit();
        }
        let old = match shown {
            Some(shown) => self.shown.insert(id, shown),
            None => self.shown.remove(&id),
        };
        // The toplevel keeps an icon that was set on it after the icon
        // object goes.
        if let Some(old) = old {
            old.destroy();
        }
        let _ = b.conn.flush();
    }

    /// The window is gone: what its icon held goes too.
    pub(crate) fn forget(&mut self, id: WindowId) {
        if let (Some(shown), Some(Some(b))) = (self.shown.remove(&id), &self.bound) {
            shown.destroy();
            let _ = b.conn.flush();
        }
    }
}

impl Bound {
    /// An icon of `icon`'s pixels in a buffer of shared memory, square
    /// (as the protocol has it) with the picture in its middle.
    fn make(&self, icon: &Icon) -> Option<Shown> {
        let qh = self.queue.handle();
        let side = icon.width.max(icon.height);
        let pixels = argb_square(icon);
        let len = i32::try_from(pixels.len()).ok()?;
        let stride = i32::try_from(side).ok()?.checked_mul(4)?;
        let side = i32::try_from(side).ok()?;
        let fd = memfd()?;
        std::fs::File::from(fd.try_clone().ok()?)
            .write_all(&pixels)
            .ok()?;
        let pool = self.shm.create_pool(fd.as_fd(), len, &qh, ());
        let buffer = pool.create_buffer(0, side, side, stride, Format::Argb8888, &qh, ());
        // The buffer keeps the memory; the pool and the file are not needed.
        pool.destroy();
        let made = self.manager.create_icon(&qh, ());
        made.add_buffer(&buffer, 1);
        Some(Shown { icon: made, buffer })
    }
}

/// The pixels as `wl_shm`'s ARGB8888: premultiplied, B G R A in memory,
/// in a square of the larger side with the picture in its middle.
fn argb_square(icon: &Icon) -> Vec<u8> {
    let (w, h) = (icon.width as usize, icon.height as usize);
    let side = w.max(h);
    let (left, top) = ((side - w) / 2, (side - h) / 2);
    let mut out = vec![0; side * side * 4];
    for (y, row) in icon.rgba.chunks_exact(w * 4).enumerate() {
        for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
            let a = u32::from(px[3]);
            let pre = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
            let at = ((top + y) * side + left + x) * 4;
            out[at..at + 4].copy_from_slice(&[pre(px[2]), pre(px[1]), pre(px[0]), px[3]]);
        }
    }
    out
}

fn memfd() -> Option<OwnedFd> {
    // SAFETY: a call with a valid string; the descriptor returned is ours.
    let fd = unsafe { libc::memfd_create(c"ljos-icon".as_ptr(), libc::MFD_CLOEXEC) };
    // SAFETY: a descriptor just opened, owned by nothing else.
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

fn bind(window: &Window) -> Option<Bound> {
    let RawDisplayHandle::Wayland(handle) = window.display_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: winit's display stays connected while its event loop lives,
    // and the host, which holds this, goes with the loop.
    let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let manager = globals
        .bind::<XdgToplevelIconManagerV1, _, _>(&qh, 1..=1, ())
        .ok()?;
    let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).ok()?;
    Some(Bound {
        conn,
        queue,
        shm,
        manager,
    })
}

fn surface(conn: &Connection, window: &Window) -> Option<WlSurface> {
    let RawWindowHandle::Wayland(h) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: winit's surface lives as long as its window, which the
    // caller holds.
    let id =
        unsafe { ObjectId::from_ptr(WlSurface::interface(), h.surface.as_ptr().cast()) }.ok()?;
    WlSurface::from_id(conn, id).ok()
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

delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore XdgToplevelIconManagerV1);
delegate_noop!(State: XdgToplevelIconV1);

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn an_icon_is_premultiplied_bgra_in_the_middle_of_a_square() {
        let icon = Icon {
            width: 1,
            height: 2,
            rgba: Arc::from(vec![255, 0, 0, 255, 0, 255, 255, 128]),
        };
        let px = argb_square(&icon);
        assert_eq!(px.len(), 2 * 2 * 4);
        // A square of two, the picture's one column at its left.
        assert_eq!(&px[0..4], &[0, 0, 255, 255]);
        assert_eq!(&px[4..8], &[0, 0, 0, 0]);
        assert_eq!(&px[8..12], &[128, 128, 0, 128]);
    }
}
