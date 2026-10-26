//! Drags from other applications on Windows, which winit 0.30 takes as
//! files alone (`CF_HDROP`), with no position and no answer for the
//! source.
//!
//! Each window gets an `IDropTarget` of the shell's in place of winit's:
//! `RevokeDragDrop` takes winit's off, `RegisterDragDrop` puts this one
//! on, on the OLE winit initialised for its own (a window that allows
//! drags, as the shell's do, initialises it). When a window closes,
//! winit's `RevokeDragDrop` takes this one off with it. OLE calls it on
//! the loop's thread, which is answered as [`crate::native_drag`] says.
//! The formats are those the source's `IDataObject` lists: the system's
//! text and file list, and registered formats by name.

use std::cell::RefCell;
use std::sync::Arc;

use egui_winit::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use egui_winit::winit::window::Window;
use ljos_platform::DragData;
use ljos_platform::drop_systems::win32;
use windows::Win32::Foundation::{HWND, POINT, POINTL};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::GetClipboardFormatNameW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    DROPEFFECT, IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::core::{Ref, implement};

use crate::drags::{At, DragStep};
use crate::native_drag::{Follower, NO_ACTIONS, Read};

/// At most this much of one format is read, as on X11.
const MOST: usize = 8 << 20;

thread_local! {
    static FOLLOWER: RefCell<Option<Follower>> = const { RefCell::new(None) };
}

/// Follows drags until [`Drags::stop`].
pub(crate) struct Drags(());

pub(crate) fn start(post: Arc<dyn Fn(DragStep) + Send + Sync>) -> Option<Drags> {
    FOLLOWER.with(|f| f.replace(Some(Follower::new(post))));
    Some(Drags(()))
}

impl Drags {
    /// `Err`: the window's drags stay winit's, and why.
    pub(crate) fn window_opened(&self, window: &Window) -> Result<(), String> {
        let Some(hwnd) = hwnd_of(window) else {
            return Ok(());
        };
        let target: IDropTarget = Target { hwnd }.into();
        // SAFETY: the window is open, and this is the thread that made it,
        // which OLE was initialised on.
        unsafe {
            // Not one there is not a failure: winit registers none for a
            // window that takes no drags.
            let _ = RevokeDragDrop(hwnd);
            RegisterDragDrop(hwnd, &target)
        }
        .map_err(|e| format!("drags from other applications stay files alone ({e})"))
    }

    pub(crate) fn stop(self) {
        FOLLOWER.with(|f| f.replace(None));
    }
}

fn hwnd_of(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// How a drag names `window`: its handle.
pub(crate) fn window_id(window: &Window) -> Option<usize> {
    hwnd_of(window).map(|h| h.0 as usize)
}

fn follow<R>(f: impl FnOnce(&mut Follower) -> R) -> Option<R> {
    FOLLOWER.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

#[implement(IDropTarget)]
struct Target {
    hwnd: HWND,
}

impl Target {
    /// `pt`, which is on the screen, in the window's pixels.
    fn at(&self, pt: &POINTL) -> At {
        let mut p = POINT { x: pt.x, y: pt.y };
        // SAFETY: a window this target was registered on, still open.
        let _ = unsafe { ScreenToClient(self.hwnd, &mut p) };
        At::Physical(f64::from(p.x), f64::from(p.y))
    }
}

/// Writes the answer where OLE reads it.
fn answer(effect: *mut DROPEFFECT, allowed: u32, action: Option<ljos_platform::DropAction>) {
    if !effect.is_null() {
        // SAFETY: OLE's out parameter, valid for the call.
        unsafe { *effect = DROPEFFECT(win32::effect(action) & allowed) };
    }
}

fn allowed(effect: *mut DROPEFFECT) -> u32 {
    if effect.is_null() {
        0
    } else {
        // SAFETY: OLE's in-out parameter: in, the effects the source allows.
        unsafe { (*effect).0 }
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let allowed = allowed(effect);
        let actions = win32::actions(allowed, keys.0).unwrap_or(NO_ACTIONS);
        let Some(data) = data.cloned() else {
            answer(effect, allowed, None);
            return Ok(());
        };
        let offered = offered(&data);
        let formats = DragData::formats_of(offered.iter().map(|f| f.mime.as_str()));
        let read: Read = Box::new(move || read(&data, &offered));
        let target = self.hwnd.0 as usize;
        let at = self.at(pt);
        let taken = follow(|f| f.entered(target, at, formats, actions, read)).flatten();
        answer(effect, allowed, taken);
        Ok(())
    }

    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let allowed = allowed(effect);
        let actions = win32::actions(allowed, keys.0).unwrap_or(NO_ACTIONS);
        let at = self.at(pt);
        let taken = follow(|f| f.moved(at, actions)).flatten();
        answer(effect, allowed, taken);
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        follow(Follower::left);
        Ok(())
    }

    fn Drop(
        &self,
        _data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let allowed = allowed(effect);
        // Where it was let go, as the last move may have been elsewhere.
        let actions = win32::actions(allowed, keys.0).unwrap_or(NO_ACTIONS);
        let at = self.at(pt);
        follow(|f| f.moved(at, actions));
        let taken = follow(Follower::dropped).flatten();
        answer(effect, allowed, taken);
        Ok(())
    }
}

/// One format the source offers, as it is read.
struct Offered {
    format: u16,
    name: Option<String>,
    mime: String,
}

/// The formats the source lists in global memory, Unicode text first.
fn offered(data: &IDataObject) -> Vec<Offered> {
    let mut out = Vec::new();
    // SAFETY: COM calls on the source's object, on its apartment's
    // thread; each FORMATETC comes back filled.
    let Ok(list) = (unsafe { data.EnumFormatEtc(DATADIR_GET.0 as u32) }) else {
        return out;
    };
    let mut one = [FORMATETC::default()];
    let mut got = 0u32;
    while unsafe { list.Next(&mut one, Some(&mut got)) }.is_ok() && got == 1 {
        let f = one[0];
        if f.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
            continue;
        }
        let name = format_name(u32::from(f.cfFormat));
        let Some(mime) = win32::mime_of(u32::from(f.cfFormat), name.as_deref()) else {
            continue;
        };
        if out.iter().any(|o: &Offered| o.format == f.cfFormat) {
            continue;
        }
        out.push(Offered {
            format: f.cfFormat,
            name,
            mime,
        });
    }
    out.sort_by_key(|o| win32::rank(u32::from(o.format)));
    out
}

/// The name of a registered format; `None` for the system's own.
fn format_name(format: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    // SAFETY: a buffer of the length passed.
    let n = unsafe { GetClipboardFormatNameW(format, &mut buf) };
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
}

/// Every format offered, as [`DragData::from_formats`] takes them.
fn read(data: &IDataObject, offered: &[Offered]) -> DragData {
    let formats: Vec<(String, Vec<u8>)> = offered
        .iter()
        .filter_map(|o| Some((o.mime.clone(), bytes_of(data, o)?)))
        .collect();
    DragData::from_formats(formats)
}

fn bytes_of(data: &IDataObject, offered: &Offered) -> Option<Vec<u8>> {
    let asked = FORMATETC {
        cfFormat: offered.format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: the medium is the source's global memory, locked while it
    // is copied, and released once it is.
    unsafe {
        let mut medium = data.GetData(&asked).ok()?;
        let global = medium.u.hGlobal;
        let bytes = if u32::from(offered.format) == win32::CF_HDROP {
            Some(files(HDROP(global.0)).into_bytes())
        } else {
            let size = GlobalSize(global);
            let at = GlobalLock(global);
            let bytes = (!at.is_null() && size <= MOST).then(|| {
                let raw = std::slice::from_raw_parts(at.cast::<u8>(), size);
                win32::utf8_of(u32::from(offered.format), offered.name.as_deref(), raw)
            });
            if !at.is_null() {
                let _ = GlobalUnlock(global);
            }
            bytes
        };
        ReleaseStgMedium(&mut medium);
        bytes
    }
}

/// `CF_HDROP`'s paths, as a `text/uri-list`.
///
/// # Safety
/// `drop` is a locked-or-lockable `HDROP` the caller still holds.
unsafe fn files(drop: HDROP) -> String {
    // SAFETY: the caller's; `u32::MAX` asks for the count.
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    let paths = (0..count).filter_map(|i| {
        // SAFETY: as above; a first call with no buffer asks the length.
        let len = unsafe { DragQueryFileW(drop, i, None) } as usize;
        let mut buf = vec![0u16; len + 1];
        let n = unsafe { DragQueryFileW(drop, i, Some(&mut buf)) } as usize;
        (n > 0).then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    });
    win32::uri_list_of(paths)
}
