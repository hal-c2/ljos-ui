//! The application is handed each window the host opens, with the
//! platform's handles, for what only the platform's API does.

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::rc::Rc;

use ljos::prelude::*;
use ljos::window::NativeWindow;
use ljos::window::raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle, XcbDisplayHandle, XcbWindowHandle,
};

/// A window as a host would hand it over, of an X server that is not
/// there: the handles are only read, never used.
struct Opened(u32);

impl HasWindowHandle for Opened {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(self.0).unwrap()));
        // SAFETY: the handle names no live window; the test reads it back
        // and passes it to nothing that would use it.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for Opened {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Xcb(XcbDisplayHandle::new(None, 0));
        // SAFETY: as for the window handle.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

fn xcb_window(native: &dyn NativeWindow) -> Option<u32> {
    match native.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

/// Each window handed over: the QML `Window` and the X11 window id.
type Seen = Rc<RefCell<Vec<(Option<ObjectId>, Option<u32>)>>>;

#[test]
fn the_application_is_handed_each_window_the_host_opens() {
    let seen: Seen = Rc::default();
    let first = seen.clone();
    let second = seen.clone();
    let engine = ljos::App::new("Windows")
        .on_window(move |_, window, native| first.borrow_mut().push((window, xcb_window(native))))
        .on_window(move |_, _, _| second.borrow_mut().push((None, None)))
        .build_engine();
    let window = engine.create_by_name("QtQuick", "Window", None).unwrap();
    let platform = Platform::of(&engine);
    platform.native_window_opened(&engine, Some(window), &Opened(7));
    platform.native_window_opened(&engine, None, &Opened(9));
    assert_eq!(
        *seen.borrow(),
        [
            (Some(window), Some(7)),
            (None, None),
            (None, Some(9)),
            (None, None)
        ]
    );
}

#[test]
fn a_hook_may_ask_for_the_windows_after_it() {
    let engine = ljos::App::new("Windows").build_engine();
    let count = Rc::new(RefCell::new(0));
    let added = count.clone();
    Platform::of(&engine).on_native_window(move |engine, _, _| {
        let added = added.clone();
        Platform::of(engine).on_native_window(move |_, _, _| *added.borrow_mut() += 1);
    });
    let platform = Platform::of(&engine);
    platform.native_window_opened(&engine, None, &Opened(1));
    assert_eq!(*count.borrow(), 0);
    platform.native_window_opened(&engine, None, &Opened(2));
    assert_eq!(*count.borrow(), 1);
}
