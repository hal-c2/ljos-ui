//! Drags out of the application on Windows, which winit 0.30 has no way
//! to start: OLE's `DoDragDrop`, with an `IDataObject` and an
//! `IDropSource` of the shell's.
//!
//! `DoDragDrop` is modal: it runs a message loop of its own until the drag
//! ends, on the thread that calls it. Called where the drag is asked for,
//! inside the Engine's turn, it would run inside winit's event handler,
//! which winit does not enter again: every event the windows heard during
//! the drag would wait in winit's buffer until it ended, so a drag over
//! the application's own window would reach none of its `DropArea`s
//! (its `IDropTarget` posts the steps, which the loop could not take) and
//! the scene would hear nothing until the drop. So the drag starts from a
//! thread timer's callback instead (`SetTimer` with no window), which
//! winit's loop calls from its `DispatchMessageW`, outside its handler:
//! inside `DoDragDrop`'s loop, winit hands its windows' events, and the
//! steps the shell's own `IDropTarget` posts, to the handler as it always
//! does, and the scene follows a drag over its own window as it follows
//! one from another application (`crate::native_drag`). What does not run
//! during the drag is what winit does between turns of its own loop
//! (`AboutToWait`, where the shell settles and draws): the windows are not
//! drawn again until the drag ends, except as `WM_PAINT` asks.
//!
//! The scene hears `dragStarted` in the turn that asks for the drag, before
//! the timer fires, and `dragFinished` once `DoDragDrop` returns and the
//! loop settles, which [`DragStep::OutEnded`] wakes it for.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use ljos_platform::drop_systems::win32::{self, Continue, Format};
use ljos_platform::{DragActions, DragData, DropAction};
use windows::Win32::Foundation::{
    DATA_S_SAMEFORMATETC, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
    DV_E_FORMATETC, E_NOTIMPL, E_POINTER, GlobalFree, OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::System::Com::{
    DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject, IDataObject_Impl,
    IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{
    DROPEFFECT, DoDragDrop, IDropSource, IDropSource_Impl, ReleaseStgMedium,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;
use windows::core::{BOOL, HRESULT, HSTRING, Ref, implement};

use crate::drags::DragStep;

/// A drag's formats, each a clipboard number and its bytes.
type Formats = Vec<(u16, Vec<u8>)>;
type Post = Arc<dyn Fn(DragStep) + Send + Sync>;

thread_local! {
    /// The drag out the timer is to start: its formats, allowed effects
    /// and the number of `Logical Performed DropEffect`.
    static ASKED: RefCell<Option<(Formats, u32, u16)>> = const { RefCell::new(None) };
    /// The effect the target wrote it performed, during the drag.
    static LOGICAL: Cell<Option<u32>> = const { Cell::new(None) };
    /// The application gave the drag up: the source cancels it.
    static GIVEN_UP: Cell<bool> = const { Cell::new(false) };
    /// How the last drag ended, until the loop asks.
    static ENDED: Cell<Option<Option<DropAction>>> = const { Cell::new(None) };
    /// A drag was asked for and has not ended.
    static UNDER_WAY: Cell<bool> = const { Cell::new(false) };
    static POST: RefCell<Option<Post>> = const { RefCell::new(None) };
}

/// The application's drags out of its windows, on the loop's thread.
pub(crate) struct Out(());

impl Out {
    pub(crate) fn new(post: Post) -> Self {
        POST.with(|p| p.replace(Some(post)));
        Self(())
    }

    /// Asks for `data` to be dragged out once winit's loop is back in its
    /// own hands. `false`: one is under way, or the timer could not be
    /// set.
    pub(crate) fn start(&mut self, data: &DragData, actions: DragActions) -> bool {
        if UNDER_WAY.get() {
            return false;
        }
        let formats: Formats = win32::offered(data)
            .into_iter()
            .filter_map(|(format, bytes)| Some((number(&format)?, bytes)))
            .collect();
        if formats.is_empty() {
            return false;
        }
        let logical = number(&Format::Named(win32::LOGICAL_PERFORMED.to_owned())).unwrap_or(0);
        ASKED.with(|a| a.replace(Some((formats, win32::effects_of(actions.allowed), logical))));
        // SAFETY: a thread timer (no window), whose callback is a function
        // of this module; winit's loop dispatches it on this thread.
        let timer =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(0, 0, 0, Some(begin)) };
        if timer == 0 {
            ASKED.with(|a| a.replace(None));
            return false;
        }
        GIVEN_UP.set(false);
        UNDER_WAY.set(true);
        true
    }

    /// The application gave its drag up: one not started yet is not, and
    /// one under way is cancelled at the source's next question.
    pub(crate) fn cancel(&mut self) {
        if UNDER_WAY.get() {
            GIVEN_UP.set(true);
        }
        ENDED.set(None);
    }

    pub(crate) fn ended(&mut self) -> Option<Option<DropAction>> {
        ENDED.take()
    }

    pub(crate) fn stop(self) {
        POST.with(|p| p.replace(None));
    }
}

/// A format's clipboard number; `None` for a name the system would not
/// register.
fn number(format: &Format) -> Option<u16> {
    match format {
        Format::System(n) => u16::try_from(*n).ok(),
        Format::Named(name) => {
            // SAFETY: a NUL-terminated wide string that lives for the call.
            let n = unsafe { RegisterClipboardFormatW(&HSTRING::from(name.as_str())) };
            u16::try_from(n).ok().filter(|&n| n != 0)
        }
    }
}

/// The timer's callback: runs the drag, which returns when it ends.
unsafe extern "system" fn begin(
    _: windows_sys::Win32::Foundation::HWND,
    _: u32,
    id: usize,
    _: u32,
) {
    // SAFETY: the thread timer `id` this callback was set for.
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(0, id) };
    let Some((formats, effects, logical)) = ASKED.with(|a| a.take()) else {
        return;
    };
    let action = if GIVEN_UP.get() {
        None
    } else {
        LOGICAL.set(None);
        let data: IDataObject = Data { formats, logical }.into();
        let source: IDropSource = Source.into();
        let mut effect = DROPEFFECT(0);
        // SAFETY: OLE was initialised on this thread, the loop's, for the
        // windows' drop targets; both objects are the shell's, alive for
        // the call.
        let result = unsafe { DoDragDrop(&data, &source, DROPEFFECT(effects), &mut effect) };
        let performed = LOGICAL.take().map(|l| l & effects);
        win32::action_of(effect.0 & effects, performed, result == DRAGDROP_S_DROP)
    };
    UNDER_WAY.set(false);
    ENDED.set(Some(action));
    if let Some(post) = POST.with(|p| p.borrow().clone()) {
        post(DragStep::OutEnded);
    }
}

#[implement(IDropSource)]
struct Source;

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        match win32::continues(escape.as_bool(), keys.0, GIVEN_UP.get()) {
            Continue::Go => S_OK,
            Continue::Drop => DRAGDROP_S_DROP,
            Continue::Cancel => DRAGDROP_S_CANCEL,
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// The drag's data: each format's bytes, handed out in global memory.
#[implement(IDataObject)]
struct Data {
    formats: Formats,
    /// `Logical Performed DropEffect`'s number (0: not registered).
    logical: u16,
}

impl Data {
    fn find(&self, asked: *const FORMATETC) -> Result<&[u8], HRESULT> {
        if asked.is_null() {
            return Err(E_POINTER);
        }
        // SAFETY: OLE's in parameter, valid for the call.
        let asked = unsafe { &*asked };
        if asked.tymed & TYMED_HGLOBAL.0 as u32 == 0 || asked.dwAspect != DVASPECT_CONTENT.0 {
            return Err(DV_E_FORMATETC);
        }
        self.formats
            .iter()
            .find(|(f, _)| *f == asked.cfFormat)
            .map(|(_, b)| b.as_slice())
            .ok_or(DV_E_FORMATETC)
    }
}

fn formatetc(format: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

impl IDataObject_Impl for Data_Impl {
    fn GetData(&self, asked: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let bytes = self.find(asked)?;
        // SAFETY: memory of the length asked for, written while locked;
        // the receiver frees it (no `pUnkForRelease`).
        unsafe {
            let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
            let at = GlobalLock(global);
            if at.is_null() {
                let error = windows::core::Error::from_thread();
                let _ = GlobalFree(Some(global));
                return Err(error);
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len());
            let _ = GlobalUnlock(global);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: global },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            })
        }
    }

    fn GetDataHere(
        &self,
        _asked: *const FORMATETC,
        _medium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, asked: *const FORMATETC) -> HRESULT {
        match self.find(asked) {
            Ok(_) => S_OK,
            Err(e) => e,
        }
    }

    fn GetCanonicalFormatEtc(&self, _asked: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        if !out.is_null() {
            // SAFETY: OLE's out parameter; the format is its own canonical
            // one, which is said with no device.
            unsafe { (*out).ptd = std::ptr::null_mut() };
        }
        DATA_S_SAMEFORMATETC
    }

    /// Targets write how the drag went here. `Logical Performed
    /// DropEffect` is kept: Explorer moving a file within a volume itself
    /// answers `DoDragDrop` with none and writes the move it did there
    /// (`win32::action_of`). Anything else is refused.
    fn SetData(
        &self,
        format: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> windows::core::Result<()> {
        if format.is_null() || medium.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: OLE's in parameters, valid for the call.
        let (format, given) = unsafe { (&*format, &*medium) };
        if self.logical == 0
            || format.cfFormat != self.logical
            || given.tymed != TYMED_HGLOBAL.0 as u32
        {
            return Err(E_NOTIMPL.into());
        }
        // SAFETY: an `HGLOBAL` medium, read while locked; when `release`,
        // the medium is this object's, and is given back once read.
        unsafe {
            let global = given.u.hGlobal;
            let at = GlobalLock(global);
            if !at.is_null() {
                let bytes = std::slice::from_raw_parts(at.cast::<u8>(), GlobalSize(global));
                LOGICAL.set(win32::dword(bytes));
                let _ = GlobalUnlock(global);
            }
            if release.as_bool() {
                ReleaseStgMedium(medium.cast_mut());
            }
        }
        Ok(())
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let list: Vec<FORMATETC> = self.formats.iter().map(|(f, _)| formatetc(*f)).collect();
        // SAFETY: the list lives for the call, which copies it.
        unsafe { SHCreateStdEnumFmtEtc(&list) }
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _advf: u32,
        _sink: Ref<IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _connection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}
