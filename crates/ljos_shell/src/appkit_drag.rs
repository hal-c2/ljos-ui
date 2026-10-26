//! Drags from other applications on macOS, which winit 0.30 takes as file
//! names alone, with no position and no answer for the source.
//!
//! AppKit sends a drag to the deepest view under the pointer that is
//! registered for one of the types it carries, and to the window (whose
//! delegate is winit's) only when no view is. So the shell registers
//! each window's content view for the types it takes
//! ([`appkit::TAKEN`]) and gives the view's class the destination's
//! methods, which winit's view does not have: `draggingEntered:`,
//! `draggingUpdated:`, `draggingExited:` and `performDragOperation:`.
//! Adding methods leaves winit's own as they are, and works on whatever
//! class the content view has, so nothing here names a class of winit's.
//! Should a later winit give its view these methods itself, the shell's
//! are not added and the drag stays winit's, which is said once in the
//! log. The calls are on the main thread, the loop's, and are answered
//! as [`crate::native_drag`] says.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::Arc;

use egui_winit::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use egui_winit::winit::window::Window;
use ljos_platform::DragData;
use ljos_platform::drop_systems::appkit;
use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSDraggingInfo, NSPasteboard, NSView};
use objc2_foundation::{NSArray, NSString};

use crate::drags::{At, DragStep};
use crate::native_drag::{Follower, NO_ACTIONS, Read};

/// At most this much of one type is read, as on X11.
const MOST: usize = 8 << 20;

thread_local! {
    static FOLLOWER: RefCell<Option<Follower>> = const { RefCell::new(None) };
    /// The classes given the methods, and those that had their own.
    static HOOKED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// Follows drags until [`Drags::stop`].
pub(crate) struct Drags(());

pub(crate) fn start(post: Arc<dyn Fn(DragStep) + Send + Sync>) -> Option<Drags> {
    MainThreadMarker::new()?;
    FOLLOWER.with(|f| f.replace(Some(Follower::new(post))));
    Some(Drags(()))
}

impl Drags {
    /// `Err`: the window's drags stay winit's, and why.
    pub(crate) fn window_opened(&self, window: &Window) -> Result<(), String> {
        let Some(view) = view_of(window) else {
            return Ok(());
        };
        // SAFETY: winit's content view, alive while its window is, and
        // this is the main thread, which winit opens windows on.
        let view: &NSView = unsafe { &*view.cast::<NSView>() };
        hook(view.class())?;
        let types: Vec<Retained<NSString>> = appkit::TAKEN
            .iter()
            .map(|t| NSString::from_str(t))
            .collect();
        view.registerForDraggedTypes(&NSArray::from_retained_slice(&types));
        Ok(())
    }

    pub(crate) fn stop(self) {
        FOLLOWER.with(|f| f.replace(None));
    }
}

fn view_of(window: &Window) -> Option<*mut std::ffi::c_void> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::AppKit(h) => Some(h.ns_view.as_ptr()),
        _ => None,
    }
}

/// How a drag names `window`: its content view.
pub(crate) fn view_id(window: &Window) -> Option<usize> {
    view_of(window).map(|v| v as usize)
}

/// Gives `class` the destination's methods, once. `Err`: it has its
/// own, and drags over it stay winit's.
fn hook(class: &AnyClass) -> Result<(), String> {
    let key = class as *const AnyClass as usize;
    if HOOKED.with(|h| h.borrow().contains(&key)) {
        return Ok(());
    }
    let operation = || {
        encoding(&[
            usize::ENCODING,
            <&AnyObject>::ENCODING,
            Sel::ENCODING,
            <&AnyObject>::ENCODING,
        ])
    };
    let methods: [(Sel, Imp, CString); 4] = [
        // SAFETY (each): the function has the arguments and result the
        // encoding beside it says, which are the selector's.
        (
            sel!(draggingEntered:),
            unsafe { imp(entered as Operation) },
            operation(),
        ),
        (
            sel!(draggingUpdated:),
            unsafe { imp(updated as Operation) },
            operation(),
        ),
        (
            sel!(draggingExited:),
            unsafe { std::mem::transmute::<Exited, Imp>(exited as Exited) },
            encoding(&[
                Encoding::Void,
                <&AnyObject>::ENCODING,
                Sel::ENCODING,
                <&AnyObject>::ENCODING,
            ]),
        ),
        (
            sel!(performDragOperation:),
            unsafe { std::mem::transmute::<Perform, Imp>(perform as Perform) },
            encoding(&[
                Bool::ENCODING,
                <&AnyObject>::ENCODING,
                Sel::ENCODING,
                <&AnyObject>::ENCODING,
            ]),
        ),
    ];
    if methods.iter().any(|(sel, ..)| defines(class, *sel)) {
        return Err(format!(
            "{} takes drags itself: drags from other applications stay files alone",
            class.name().to_string_lossy()
        ));
    }
    for (sel, imp, types) in &methods {
        // SAFETY: a class registered with the runtime, on the main
        // thread, with an implementation that matches `types`.
        unsafe {
            objc2::ffi::class_addMethod(
                (class as *const AnyClass).cast_mut(),
                *sel,
                *imp,
                types.as_ptr(),
            );
        }
    }
    HOOKED.with(|h| h.borrow_mut().insert(key));
    Ok(())
}

/// Whether `class` itself, not one it inherits from, defines `sel`.
fn defines(class: &AnyClass, sel: Sel) -> bool {
    let inherited = class
        .superclass()
        .and_then(|s| s.instance_method(sel))
        .map(|m| m.implementation() as usize);
    class
        .instance_method(sel)
        .map(|m| m.implementation() as usize)
        != inherited
}

fn encoding(parts: &[Encoding]) -> CString {
    let text: String = parts.iter().map(ToString::to_string).collect();
    CString::new(text).expect("no NUL in an encoding")
}

type Operation =
    extern "C-unwind" fn(&AnyObject, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> usize;
type Exited = extern "C-unwind" fn(&AnyObject, Sel, Option<&ProtocolObject<dyn NSDraggingInfo>>);
type Perform = extern "C-unwind" fn(&AnyObject, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> Bool;

unsafe fn imp(f: Operation) -> Imp {
    // SAFETY: the caller's; an `Imp` is any method's function, called
    // with the arguments its encoding says.
    unsafe { std::mem::transmute::<Operation, Imp>(f) }
}

fn follow<R>(f: impl FnOnce(&mut Follower) -> R) -> Option<R> {
    FOLLOWER.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Where the drag is over `view`, in its points from the top left.
fn at(view: &NSView, info: &ProtocolObject<dyn NSDraggingInfo>) -> At {
    let p = view.convertPoint_fromView(info.draggingLocation(), None);
    let y = if view.isFlipped() {
        p.y
    } else {
        view.bounds().size.height - p.y
    };
    At::Logical(p.x, y)
}

fn as_view(this: &AnyObject) -> &NSView {
    // SAFETY: the methods are added to content views' classes alone,
    // which are views.
    unsafe { &*(this as *const AnyObject).cast::<NSView>() }
}

extern "C-unwind" fn entered(
    this: &AnyObject,
    _: Sel,
    info: &ProtocolObject<dyn NSDraggingInfo>,
) -> usize {
    let view = as_view(this);
    let mask = info.draggingSourceOperationMask().0 as u64;
    let actions = appkit::actions(mask).unwrap_or(NO_ACTIONS);
    let pasteboard = info.draggingPasteboard();
    let kinds = kinds(&pasteboard);
    let formats = DragData::formats_of(kinds.iter().filter_map(|k| appkit::mime_of(k)));
    let read: Read = Box::new(move || read(&pasteboard, &kinds));
    let target = this as *const AnyObject as usize;
    let answer = follow(|f| f.entered(target, at(view, info), formats, actions, read)).flatten();
    appkit::operation(answer, mask) as usize
}

extern "C-unwind" fn updated(
    this: &AnyObject,
    _: Sel,
    info: &ProtocolObject<dyn NSDraggingInfo>,
) -> usize {
    let mask = info.draggingSourceOperationMask().0 as u64;
    let actions = appkit::actions(mask).unwrap_or(NO_ACTIONS);
    let at = at(as_view(this), info);
    let answer = follow(|f| f.moved(at, actions)).flatten();
    appkit::operation(answer, mask) as usize
}

extern "C-unwind" fn exited(
    _this: &AnyObject,
    _: Sel,
    _info: Option<&ProtocolObject<dyn NSDraggingInfo>>,
) {
    follow(Follower::left);
}

extern "C-unwind" fn perform(
    _this: &AnyObject,
    _: Sel,
    _info: &ProtocolObject<dyn NSDraggingInfo>,
) -> Bool {
    Bool::new(follow(Follower::dropped).flatten().is_some())
}

/// The pasteboard's types, in the order its source gave them.
fn kinds(pasteboard: &NSPasteboard) -> Vec<String> {
    pasteboard
        .types()
        .map(|types| types.iter().map(|t| t.to_string()).collect())
        .unwrap_or_default()
}

/// Every type of the pasteboard a target could ask for, as
/// [`DragData::from_formats`] takes them. An address list is the address
/// of each of the pasteboard's items; text is read as a string, which
/// AppKit converts from any encoding; anything else is its bytes.
fn read(pasteboard: &NSPasteboard, kinds: &[String]) -> DragData {
    let mut formats: Vec<(String, Vec<u8>)> = Vec::new();
    for kind in kinds {
        // Two types of one MIME type (`public.url` beside
        // `public.file-url`) say the same; the first is read.
        let Some(mime) = appkit::mime_of(kind).filter(|m| formats.iter().all(|(f, _)| f != m))
        else {
            continue;
        };
        let name = NSString::from_str(kind);
        let bytes = if mime == "text/uri-list" {
            let items = pasteboard.pasteboardItems().unwrap_or_default();
            let lines: Vec<String> = items
                .iter()
                .filter_map(|item| item.stringForType(&name))
                .map(|s| s.to_string())
                .collect();
            (!lines.is_empty()).then(|| lines.join("\r\n").into_bytes())
        } else if appkit::is_text(kind) {
            pasteboard
                .stringForType(&name)
                .map(|s| s.to_string().into_bytes())
        } else {
            pasteboard
                .dataForType(&name)
                .filter(|d| d.len() <= MOST)
                .map(|d| d.to_vec())
        };
        if let Some(bytes) = bytes.filter(|b| b.len() <= MOST) {
            formats.push((mime.to_owned(), bytes));
        }
    }
    DragData::from_formats(formats)
}
