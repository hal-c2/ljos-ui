//! Drags from other applications, which the shell follows itself where
//! winit 0.30 does not, or loses what they carry: on Wayland winit has no
//! data device at all (`wayland_drag`); on X11 it takes files only and
//! keeps the drag's position to itself (`x11_drag`); on macOS (`appkit_drag`)
//! and Windows (`win32_drag`) it takes files alone, with no position and no
//! answer for the source.
//!
//! Drags out of the application go to the window system the same way,
//! where the shell carries them ([`DragOut`]): on Wayland and X11. On
//! macOS and Windows they stay in the window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_engine::Engine;
use ljos_platform::{DragActions, DragData, DragOutDone, DragOutService, DropAction, OutgoingDrag};

use crate::{wayland_drag, x11_drag};

/// Where a drag is over a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum At {
    /// The surface's logical pixels, as Wayland says it.
    Logical(f64, f64),
    /// The window's pixels, as X11 and Windows say it.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Physical(f64, f64),
}

/// One step of a drag, for the window `target` names ([`target_of`]).
pub(crate) enum DragStep {
    /// A drag came over the window, offering `formats`, whose data is not
    /// read yet: the window system needs an answer at once, and some
    /// sources give their data only to a target that answered. The loop
    /// answers from the formats alone and then calls `answered`, after
    /// which the data is read and the drag is [`DragStep::Entered`].
    Offered {
        target: usize,
        at: At,
        formats: Vec<String>,
        actions: DragActions,
        answer: Answer,
        answered: Box<dyn FnOnce() + Send>,
    },
    Entered {
        target: usize,
        at: At,
        data: DragData,
        actions: DragActions,
        answer: Answer,
    },
    /// It moved, or the source changed what it allows or would like.
    Moved {
        at: At,
        actions: Option<DragActions>,
        answer: Answer,
    },
    Left,
    Dropped {
        answer: Answer,
    },
    /// Not a step of a drag in: the drag out of the application ended,
    /// which wakes the loop to settle it ([`DragOut::settle`]). macOS and
    /// Windows have no drags out yet.
    #[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
    OutEnded,
}

/// What the application says back to the window system of one step.
pub(crate) trait Reply: Send {
    /// The action a drop here would be taken with; `None`: it would not.
    fn taken(&self, action: Option<DropAction>);
    /// After the drop: the action it was taken with, if it was. The drag
    /// is over.
    fn dropped(self: Box<Self>, action: Option<DropAction>);
}

pub(crate) type Answer = Box<dyn Reply>;

/// What follows drags; [`Drags::stop`] ends it.
pub(crate) enum Drags {
    Wayland(wayland_drag::Drags),
    X11(x11_drag::Drags),
    #[cfg(target_os = "macos")]
    AppKit(crate::appkit_drag::Drags),
    #[cfg(target_os = "windows")]
    Win32(crate::win32_drag::Drags),
}

impl Drags {
    /// Starts following drags on winit's display, when it is one the shell
    /// follows them on.
    pub(crate) fn start(
        display: &OwnedDisplayHandle,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Self> {
        // The window system's own calls find the drags there, not winit's
        // display.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let _ = display;
        #[cfg(target_os = "macos")]
        return crate::appkit_drag::start(post).map(Self::AppKit);
        #[cfg(target_os = "windows")]
        return crate::win32_drag::start(post).map(Self::Win32);
        #[cfg_attr(
            any(target_os = "macos", target_os = "windows"),
            allow(unreachable_code)
        )]
        if wayland_drag::is_wayland(display) {
            return wayland_drag::start(display, post).map(Self::Wayland);
        }
        x11_drag::start(display, post).map(Self::X11)
    }

    /// A platform window was made: drags over it come to the shell.
    /// `Err`: they do not, and why.
    pub(crate) fn window_opened(&self, window: &Window) -> Result<(), String> {
        match self {
            Self::Wayland(_) => Ok(()),
            Self::X11(drags) => {
                drags.window_opened(window);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::AppKit(drags) => drags.window_opened(window),
            #[cfg(target_os = "windows")]
            Self::Win32(drags) => drags.window_opened(window),
        }
    }

    /// Handles what the window system said of drags since the last time;
    /// the loop calls it each time before it sleeps.
    pub(crate) fn dispatch(&mut self) {
        if let Self::Wayland(drags) = self {
            drags.dispatch();
        }
    }

    pub(crate) fn stop(self) {
        match self {
            Self::Wayland(drags) => drags.stop(),
            Self::X11(drags) => drags.stop(),
            #[cfg(target_os = "macos")]
            Self::AppKit(drags) => drags.stop(),
            #[cfg(target_os = "windows")]
            Self::Win32(drags) => drags.stop(),
        }
    }

    fn start_out(&mut self, data: &DragData, actions: DragActions) -> bool {
        match self {
            Self::Wayland(drags) => drags.start_out(data, actions),
            Self::X11(drags) => drags.start_out(data, actions),
            // Drags out stay in the window there.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            _ => false,
        }
    }

    fn cancel_out(&mut self) {
        match self {
            Self::Wayland(drags) => drags.cancel_out(),
            Self::X11(drags) => drags.cancel_out(),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            _ => {}
        }
    }

    fn out_ended(&mut self) -> Option<Option<DropAction>> {
        match self {
            Self::Wayland(drags) => drags.out_ended(),
            Self::X11(drags) => drags.out_ended(),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            _ => None,
        }
    }
}

/// The application's drags out of its windows, handed to what follows
/// drags. It is called from inside the Engine, which the loop is running,
/// so it reaches [`Drags`] through a cell the loop borrows only between
/// turns of the Engine.
pub(crate) struct DragOut {
    drags: Rc<RefCell<Option<Drags>>>,
    done: RefCell<Option<DragOutDone>>,
    /// A drag was handed out since the loop last asked: the window system
    /// holds the pointer now.
    took_pointer: Cell<bool>,
}

impl DragOut {
    pub(crate) fn new(drags: Rc<RefCell<Option<Drags>>>) -> Self {
        Self {
            drags,
            done: RefCell::new(None),
            took_pointer: Cell::new(false),
        }
    }

    /// Tells the application how its drag out ended, once it has.
    pub(crate) fn settle(&self, engine: &Engine) {
        let ended = self.drags.borrow_mut().as_mut().and_then(Drags::out_ended);
        if let Some(action) = ended
            && let Some(done) = self.done.take()
        {
            done(engine, action);
        }
    }

    /// Whether a drag was handed out since the last time it was asked.
    pub(crate) fn took_pointer(&self) -> bool {
        self.took_pointer.replace(false)
    }
}

impl DragOutService for DragOut {
    fn start(&self, _engine: &Engine, drag: OutgoingDrag, done: DragOutDone) -> bool {
        let Ok(mut drags) = self.drags.try_borrow_mut() else {
            return false;
        };
        let started = drags
            .as_mut()
            .is_some_and(|d| d.start_out(&drag.data, drag.actions));
        if started {
            self.done.replace(Some(done));
            self.took_pointer.set(true);
        }
        started
    }

    fn cancel(&self, _engine: &Engine) {
        if let Ok(mut drags) = self.drags.try_borrow_mut()
            && let Some(d) = drags.as_mut()
        {
            d.cancel_out();
        }
        self.done.take();
    }
}

impl std::fmt::Debug for Drags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Wayland(_) => "Drags(Wayland)",
            Self::X11(_) => "Drags(X11)",
            #[cfg(target_os = "macos")]
            Self::AppKit(_) => "Drags(AppKit)",
            #[cfg(target_os = "windows")]
            Self::Win32(_) => "Drags(Win32)",
        })
    }
}

/// How a drag names `window`: its Wayland surface, its X11 window, its
/// AppKit content view or its Win32 handle.
pub(crate) fn target_of(window: &Window) -> Option<usize> {
    #[cfg(target_os = "macos")]
    return crate::appkit_drag::view_id(window);
    #[cfg(target_os = "windows")]
    return crate::win32_drag::window_id(window);
    #[cfg_attr(
        any(target_os = "macos", target_os = "windows"),
        allow(unreachable_code)
    )]
    wayland_drag::surface_of(window).or_else(|| x11_drag::window_of(window))
}
