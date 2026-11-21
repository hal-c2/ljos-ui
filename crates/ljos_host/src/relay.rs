//! A host whose platform loop runs on a thread of its own while the
//! application keeps one thread for the life of the process
//! (ARCHITECTURE.md §10.2).
//!
//! On Android the platform's loop is `android_main`: it returns when the
//! activity is destroyed, and the next activity of the same process calls
//! it again on another thread. The Engine, its scripts and its models
//! cannot move between threads, and must outlive the activity, so they do
//! not run there. The platform's thread hands what happens to a [`Relay`],
//! and the application's thread runs [`RelayHost::run`], which owns the
//! [`HostApp`] and sleeps on the relay's channel: nothing polls.
//!
//! A surface is handed over with the event that brings it, and taken back
//! with a [`Done`] the platform's thread waits for: the application lets go
//! of what it made on the surface before the platform frees it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Instant;

use crate::{
    AppState, Capabilities, Event, HostApp, HostControl, HostError, PlatformHost, Screen, Wake,
    Waker, WindowAttributes, WindowChange, WindowEvent, WindowId,
};

/// How many arrivals may wait before the platform's thread waits too.
const QUEUE: usize = 256;

/// The one window of a platform that shows one at a time.
pub const WINDOW: WindowId = WindowId(1);

/// What the platform's thread tells the application's.
#[derive(Debug)]
pub enum Arrival<S> {
    /// A surface to draw into, the size of what is drawn in physical
    /// pixels and the scale.
    SurfaceCreated {
        surface: S,
        px: [u32; 2],
        scale: f64,
    },
    /// The surface goes once `Done` is dropped: the application lets go of
    /// it first.
    SurfaceDestroyed(Done),
    /// Something about the window.
    Window(WindowEvent),
    AppState(AppState),
    MemoryPressure,
    OpenUrl(String),
    /// Write what must survive the process; the platform waits for `Done`.
    SaveState(Done),
    /// The platform's loop has ended (the activity is gone). Another may
    /// hand over a surface later; the application lives on.
    Detached,
}

/// What the platform's thread waits for. Dropping it is the answer, so a
/// thread that went down holding it answers too.
#[derive(Debug)]
pub struct Done(#[allow(dead_code)] SyncSender<()>);

impl Done {
    /// The answer and what waits for it.
    #[must_use]
    pub fn pair() -> (Self, Answered) {
        let (tx, rx) = mpsc::sync_channel(1);
        (Self(tx), Answered(rx))
    }
}

/// Waits for a [`Done`] to be dropped.
#[derive(Debug)]
pub struct Answered(Receiver<()>);

impl Answered {
    pub fn wait(self) {
        // Nothing is ever sent: the channel closing is the answer.
        let _ = self.0.recv();
    }
}

/// What the application asks of the platform's thread, which alone may
/// call some of the platform (the soft keyboard, ending the activity).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Request {
    /// The application has ended.
    Exit,
    /// Show the soft keyboard, or hide it.
    Keyboard(bool),
    /// The user went back with nothing left in the application to go back
    /// from: the platform does what it does then (Android: the task goes
    /// to the background; the activity is not finished).
    Background,
}

enum Msg<S, U> {
    Arrival(Arrival<S>),
    User(U),
    Wake,
}

/// The platform's side: what its thread hands events to. Clones share the
/// channel.
pub struct Relay<S, U> {
    tx: SyncSender<Msg<S, U>>,
    wake_queued: Arc<AtomicBool>,
}

impl<S, U> std::fmt::Debug for Relay<S, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("wake_queued", &self.wake_queued.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl<S, U> Clone for Relay<S, U> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            wake_queued: Arc::clone(&self.wake_queued),
        }
    }
}

impl<S: Send + 'static, U: Send + 'static> Relay<S, U> {
    /// Hands one event over; waits while the application is behind.
    ///
    /// # Errors
    /// The application's loop has ended.
    pub fn send(&self, arrival: Arrival<S>) -> Result<(), HostError> {
        self.tx
            .send(Msg::Arrival(arrival))
            .map_err(|_| HostError::Closed)
    }

    /// Hands over an event that carries a [`Done`], and waits for it.
    ///
    /// # Errors
    /// The application's loop has ended (nothing is left to wait for).
    pub fn send_and_wait(&self, arrival: impl FnOnce(Done) -> Arrival<S>) -> Result<(), HostError> {
        let (done, answered) = Done::pair();
        self.send(arrival(done))?;
        answered.wait();
        Ok(())
    }

    /// Posts a message of another thread; the application receives
    /// [`Event::User`].
    ///
    /// # Errors
    /// The application's loop has ended.
    pub fn post(&self, message: U) -> Result<(), HostError> {
        self.tx
            .send(Msg::User(message))
            .map_err(|_| HostError::Closed)
    }

    fn waker(&self) -> Waker {
        let tx = self.tx.clone();
        let queued = Arc::clone(&self.wake_queued);
        Waker::new(move || {
            // One wake waiting is enough: the loop answers it with the
            // state of things when it gets there.
            if queued.swap(true, Ordering::AcqRel) {
                return;
            }
            match tx.try_send(Msg::Wake) {
                Ok(()) => {}
                // A full queue wakes the loop by itself.
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                    queued.store(false, Ordering::Release);
                }
            }
        })
    }
}

type Requests = Arc<dyn Fn(Request) + Send + Sync>;

/// The application's side of a relayed platform. `S` is what a surface is
/// on the platform (an `ANativeWindow` on Android).
pub struct RelayHost<S, U> {
    rx: Receiver<Msg<S, U>>,
    relay: Relay<S, U>,
    capabilities: Capabilities,
    requests: Requests,
    name: String,
}

impl<S, U> std::fmt::Debug for RelayHost<S, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayHost")
            .field("relay", &self.relay)
            .field("capabilities", &self.capabilities)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<S: Send + 'static, U: From<Wake> + Send + 'static> RelayHost<S, U> {
    /// A host and the relay the platform's thread hands events to.
    /// `requests` is called on the application's thread with what it asks
    /// of the platform's.
    pub fn new(
        capabilities: Capabilities,
        requests: impl Fn(Request) + Send + Sync + 'static,
    ) -> (Self, Relay<S, U>) {
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let relay = Relay {
            tx,
            wake_queued: Arc::new(AtomicBool::new(false)),
        };
        let host = Self {
            rx,
            relay: relay.clone(),
            capabilities,
            requests: Arc::new(requests),
            name: String::from("screen"),
        };
        (host, relay)
    }

    /// The name the display is given in [`HostControl::screens`].
    #[must_use]
    pub fn with_screen_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

/// What the loop knows of the one window and its surface.
struct State<S> {
    surface: Option<S>,
    /// The application has made the window.
    window: bool,
    px: [u32; 2],
    scale: f64,
    /// The size the window was last told of.
    told: Option<([u32; 2], f64)>,
    redraw: bool,
    deadline: Option<Instant>,
    exit: bool,
    app: AppState,
    name: String,
    capabilities: Capabilities,
    requests: Requests,
}

/// What a [`RelayHost`]'s application may ask while it handles an event.
pub struct RelayControl<'a, S> {
    state: &'a mut State<S>,
}

impl<S> std::fmt::Debug for RelayControl<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayControl").finish_non_exhaustive()
    }
}

impl<S> RelayControl<'_, S> {
    /// The surface handed over with the last [`Event::SurfaceCreated`],
    /// until [`Event::SurfaceDestroyed`].
    pub fn surface(&self) -> Option<&S> {
        self.state.surface.as_ref()
    }

    /// Asks something of the platform's thread.
    pub fn request(&self, request: Request) {
        (self.state.requests)(request);
    }

    /// Whether the application is in front of its user.
    pub fn app_state(&self) -> AppState {
        self.state.app
    }
}

impl<S> HostControl for RelayControl<'_, S> {
    fn capabilities(&self) -> Capabilities {
        self.state.capabilities
    }

    fn create_window(&mut self, _attributes: &WindowAttributes) -> Result<WindowId, HostError> {
        if self.state.window {
            return Err(HostError::Window(String::from(
                "this platform shows one window at a time",
            )));
        }
        self.state.window = true;
        // The window is the platform's: a surface it already has is drawn
        // into at once.
        self.state.redraw = true;
        Ok(WINDOW)
    }

    fn destroy_window(&mut self, id: WindowId) {
        if id == WINDOW {
            self.state.window = false;
        }
    }

    fn change_window(&mut self, _id: WindowId, _change: WindowChange) {
        // Size, place and title are the platform's.
    }

    fn request_redraw(&mut self, id: WindowId) {
        if id == WINDOW {
            self.state.redraw = true;
        }
    }

    fn wake_at(&mut self, deadline: Option<Instant>) {
        self.state.deadline = deadline;
    }

    fn screens(&self) -> Vec<Screen> {
        vec![Screen {
            name: self.state.name.clone(),
            size: self.state.px,
            position: [0, 0],
            scale: self.state.scale,
            refresh_hz: None,
        }]
    }

    fn screen_of(&self, id: WindowId) -> Option<Screen> {
        (id == WINDOW).then(|| self.screens().remove(0))
    }

    fn exit(&mut self) {
        self.state.exit = true;
    }
}

impl<S: Send + 'static, U: From<Wake> + Send + 'static> PlatformHost for RelayHost<S, U> {
    type Raw = ();
    type User = U;
    type Control<'a> = RelayControl<'a, S>;

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn waker(&self) -> Waker {
        self.relay.waker()
    }

    fn run<A: HostApp<Self>>(self, mut app: A) -> Result<(), HostError> {
        let Self {
            rx,
            relay,
            capabilities,
            requests,
            name,
        } = self;
        let mut state = State {
            surface: None,
            window: false,
            px: [0, 0],
            scale: 1.0,
            told: None,
            redraw: false,
            deadline: None,
            exit: false,
            app: AppState::Hidden,
            name,
            capabilities,
            requests,
        };
        let mut deliver = |state: &mut State<S>, event: Event<'static, (), U>| {
            app.event(&mut RelayControl { state }, event);
        };
        loop {
            // The first message: sleep until one comes, or the time asked
            // for. Not while a redraw is pending that can be drawn: it was
            // asked for after the loop last looked, and nothing else may
            // come to wake it.
            let drawable = state.redraw && state.window && state.surface.is_some();
            let mut next = if drawable {
                rx.try_recv().ok()
            } else {
                let first = match state.deadline {
                    None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
                };
                match first {
                    Ok(msg) => Some(msg),
                    Err(RecvTimeoutError::Timeout) => {
                        state.deadline = None;
                        deliver(&mut state, Event::Wake);
                        None
                    }
                    // The host holds a relay itself, so this is not reached.
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            };
            // Then what is already waiting, before the window is drawn; at
            // most a turn's worth, so that a sender that never lets the
            // queue empty does not keep the window from being drawn.
            let mut taken = 0;
            loop {
                // Asked of the clock, not of whether the wait timed out:
                // while messages keep coming, none does.
                if state.deadline.is_some_and(|at| at <= Instant::now()) {
                    state.deadline = None;
                    deliver(&mut state, Event::Wake);
                }
                let Some(msg) = next.take() else {
                    break;
                };
                match msg {
                    Msg::Wake => {
                        relay.wake_queued.store(false, Ordering::Release);
                        deliver(&mut state, Event::User(U::from(Wake)));
                    }
                    Msg::User(u) => deliver(&mut state, Event::User(u)),
                    Msg::Arrival(arrival) => arrive(&mut state, &mut deliver, arrival),
                }
                taken += 1;
                if state.exit || taken == TURN {
                    break;
                }
                next = rx.try_recv().ok();
            }
            if state.exit {
                break;
            }
            if state.window && state.told != Some((state.px, state.scale)) && state.px != [0, 0] {
                state.told = Some((state.px, state.scale));
                let (px, scale) = (state.px, state.scale);
                deliver(
                    &mut state,
                    Event::Window {
                        id: WINDOW,
                        event: WindowEvent::Resized { px, scale },
                        raw: None,
                    },
                );
            }
            if state.redraw && state.window && state.surface.is_some() {
                state.redraw = false;
                deliver(
                    &mut state,
                    Event::Window {
                        id: WINDOW,
                        event: WindowEvent::RedrawRequested,
                        raw: None,
                    },
                );
            }
            deliver(&mut state, Event::AboutToWait);
            if state.exit {
                break;
            }
        }
        deliver(&mut state, Event::Exiting);
        (state.requests)(Request::Exit);
        Ok(())
    }
}

/// The most messages one turn of the loop takes before it draws.
const TURN: usize = 64;

fn arrive<S, U>(
    state: &mut State<S>,
    deliver: &mut impl FnMut(&mut State<S>, Event<'static, (), U>),
    arrival: Arrival<S>,
) {
    let window = |event| Event::Window {
        id: WINDOW,
        event,
        raw: None,
    };
    match arrival {
        Arrival::SurfaceCreated { surface, px, scale } => {
            state.surface = Some(surface);
            resize(state, px, scale);
            deliver(state, Event::SurfaceCreated);
            state.redraw = true;
        }
        Arrival::SurfaceDestroyed(done) => {
            if state.surface.is_some() {
                deliver(state, Event::SurfaceDestroyed);
            }
            // Only after the application has let go of what it made on it.
            state.surface = None;
            state.redraw = false;
            drop(done);
        }
        Arrival::Window(WindowEvent::Resized { px, scale }) => resize(state, px, scale),
        Arrival::Window(event) => {
            if state.window {
                deliver(state, window(event));
            }
        }
        Arrival::AppState(app) => {
            if state.app != app {
                state.app = app;
                deliver(state, Event::AppState(app));
            }
        }
        Arrival::MemoryPressure => deliver(state, Event::MemoryPressure),
        Arrival::OpenUrl(url) => deliver(state, Event::OpenUrl(url)),
        Arrival::SaveState(done) => {
            deliver(state, Event::SaveState);
            drop(done);
        }
        Arrival::Detached => {
            if state.app != AppState::Suspended {
                state.app = AppState::Suspended;
                deliver(state, Event::AppState(AppState::Suspended));
            }
        }
    }
}

fn resize<S>(state: &mut State<S>, px: [u32; 2], scale: f64) {
    // The window is told once the events waiting are in: a rotation brings
    // several sizes at once, and only the last is drawn.
    if state.px != px || state.scale != scale {
        state.px = px;
        state.scale = scale;
        state.redraw = true;
    }
}

#[cfg(test)]
mod tests;
