//! Drags from other applications, which the shell follows itself where
//! winit 0.30 does not, or loses what they carry: on Wayland winit has no
//! data device at all (`wayland_drag`); on X11 it takes files only and
//! keeps the drag's position to itself (`x11_drag`). On macOS and Windows
//! they stay winit's: files only, placed where `QmlView` guesses.

use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_platform::DragData;

use crate::{wayland_drag, x11_drag};

/// Where a drag is over a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum At {
    /// The surface's logical pixels, as Wayland says it.
    Logical(f64, f64),
    /// The window's pixels, as X11 says it.
    Physical(f64, f64),
}

/// One step of a drag, for the window `target` names ([`target_of`]).
pub(crate) enum DragStep {
    Entered {
        target: usize,
        at: At,
        data: DragData,
        answer: Answer,
    },
    Moved {
        at: At,
        answer: Answer,
    },
    Left,
    Dropped {
        answer: Answer,
    },
}

/// What the application says back to the window system of one step.
pub(crate) trait Reply: Send {
    /// Whether a drop here would be taken.
    fn taken(&self, taken: bool);
    /// After the drop: whether it was taken. The drag is over.
    fn dropped(self: Box<Self>, taken: bool);
}

pub(crate) type Answer = Box<dyn Reply>;

/// What follows drags; [`Drags::stop`] ends it.
pub(crate) enum Drags {
    Wayland(wayland_drag::Drags),
    X11(x11_drag::Drags),
}

impl Drags {
    /// Starts following drags on winit's display, when it is one the shell
    /// follows them on.
    pub(crate) fn start(
        display: &OwnedDisplayHandle,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Self> {
        if wayland_drag::is_wayland(display) {
            return wayland_drag::start(display, post).map(Self::Wayland);
        }
        x11_drag::start(display, post).map(Self::X11)
    }

    /// A platform window was made: drags over it come to the shell.
    pub(crate) fn window_opened(&self, window: &Window) {
        match self {
            Self::Wayland(_) => {}
            Self::X11(drags) => drags.window_opened(window),
        }
    }

    /// Handles what the window system said of drags since the last time;
    /// the loop calls it each time before it sleeps.
    pub(crate) fn dispatch(&mut self) {
        match self {
            Self::Wayland(drags) => drags.dispatch(),
            Self::X11(_) => {}
        }
    }

    pub(crate) fn stop(self) {
        match self {
            Self::Wayland(drags) => drags.stop(),
            Self::X11(drags) => drags.stop(),
        }
    }
}

impl std::fmt::Debug for Drags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Wayland(_) => "Drags(Wayland)",
            Self::X11(_) => "Drags(X11)",
        })
    }
}

/// How a drag names `window`: its Wayland surface, or its X11 window.
pub(crate) fn target_of(window: &Window) -> Option<usize> {
    wayland_drag::surface_of(window).or_else(|| x11_drag::window_of(window))
}
