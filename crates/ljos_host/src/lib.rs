//! The platform host (ARCHITECTURE.md §10.1): events come in, capabilities
//! go out.
//!
//! A [`PlatformHost`] owns the platform's event loop and its windows. The
//! application behind it is a [`HostApp`]: it is handed every [`Event`]
//! together with a [`HostControl`], through which it opens and changes
//! windows, asks to be woken at a time and ends the loop. Nothing here
//! knows what draws into a window: the UI layer (egui on the desktop) is
//! the application's.
//!
//! Nothing polls. The loop sleeps until the platform has an event, a
//! [`Waker`] is fired from another thread, or the time given to
//! [`HostControl::wake_at`] arrives.
//!
//! [`WinitHost`] (feature `winit`, on by default) is the host for the
//! desktop.

mod event;
mod schedule;
mod window;
#[cfg(feature = "winit")]
mod winit_host;

use std::sync::Arc;
use std::time::Instant;

pub use event::{
    AppState, BackPhase, ButtonState, Event, Insets, Key, KeyEvent, Modifiers, PointerButton,
    PointerEvent, PointerKind, TextEvent, TouchEvent, TouchPhase, WheelDelta, WindowEvent,
};
pub use schedule::{Schedule, Sleep};
pub use window::{
    Capabilities, ResizeEdge, Screen, WindowAttributes, WindowChange, WindowId, WindowKind,
    WindowLevel, WindowState,
};
#[cfg(feature = "winit")]
pub use winit_host::{WinitControl, WinitHost};

/// What a host cannot do.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The platform has no display to open, or refused the loop.
    #[error("the platform's event loop cannot start: {0}")]
    Loop(String),
    #[error("a window cannot be opened: {0}")]
    Window(String),
    /// The loop has ended, or the window is gone.
    #[error("the host is no longer running")]
    Closed,
}

/// Wakes the loop from any thread: the application is handed the message
/// the waker was made with. Firing it while the loop is awake costs one
/// queued message and no extra turn.
#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn wake(&self) {
        (self.0)();
    }

    /// The same waker as a plain hook, for interfaces that take one.
    pub fn into_hook(self) -> Arc<dyn Fn() + Send + Sync> {
        self.0
    }
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Waker")
    }
}

/// The message a [`Waker`] puts into the loop. A host's message type is
/// made from it, so that the application can tell a wake from its other
/// messages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wake;

/// A platform's event loop and windows.
pub trait PlatformHost: Sized {
    /// The platform's own window event. It is handed on beside the
    /// portable one for a UI layer that has a translation of its own.
    type Raw;
    /// What other threads post into the loop.
    type User: Send + 'static;
    /// What the application may ask of the host while it handles an event.
    type Control<'a>: HostControl
    where
        Self: 'a;

    /// What this platform can do, before a window exists.
    fn capabilities(&self) -> Capabilities;

    /// Wakes the loop from any thread; the application receives
    /// [`Event::User`] with the message made from [`Wake`].
    fn waker(&self) -> Waker;

    /// Runs the loop until the application calls [`HostControl::exit`].
    ///
    /// # Errors
    /// The platform's loop failed.
    fn run<A: HostApp<Self>>(self, app: A) -> Result<(), HostError>;
}

/// The application behind a host.
pub trait HostApp<H: PlatformHost> {
    /// One event. Whatever the application wants next (a redraw, a wake at
    /// a time, the end of the loop) it asks of `host` before it returns.
    fn event(&mut self, host: &mut H::Control<'_>, event: Event<'_, H::Raw, H::User>);
}

/// What an application asks of its host.
pub trait HostControl {
    fn capabilities(&self) -> Capabilities;

    /// Opens a window. Its events carry the id returned.
    ///
    /// # Errors
    /// The platform refused the window.
    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, HostError>;

    /// Closes a window. Unknown ids are ignored.
    fn destroy_window(&mut self, id: WindowId);

    /// Changes one thing about a window. A change the platform cannot make
    /// is dropped: [`HostControl::capabilities`] says which those are.
    fn change_window(&mut self, id: WindowId, change: WindowChange);

    /// Asks for [`WindowEvent::RedrawRequested`] for the window, once.
    fn request_redraw(&mut self, id: WindowId);

    /// When the loop wakes by itself next ([`Event::Wake`]); `None` sleeps
    /// until something happens. Each call replaces the time before it.
    fn wake_at(&mut self, deadline: Option<Instant>);

    /// The displays, the primary one first.
    fn screens(&self) -> Vec<Screen>;

    /// The display most of the window is on.
    fn screen_of(&self, id: WindowId) -> Option<Screen>;

    /// Ends the loop after this event.
    fn exit(&mut self);
}
