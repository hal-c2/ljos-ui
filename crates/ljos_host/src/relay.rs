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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
#[derive(Clone, Debug, PartialEq)]
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
    /// Whether the application takes the system's Back now. While it does
    /// not, the platform does Back by itself (Android plays its own
    /// predictive animation for it).
    BackTaken(bool),
    /// The on-screen keyboard is for this kind of editor; told before it
    /// is shown.
    Editor(crate::EditorKind),
    /// What the input method is to hold now: the application changed the
    /// editor's text or selection, or refused what the method typed.
    Held {
        now: crate::HeldText,
        /// What the application last knew the method to hold. A method that
        /// holds other text by the time the platform gets to this has typed
        /// since, and the request is dropped (see [`held_is_stale`]).
        over: Option<crate::HeldText>,
    },
    /// Where the editor with focus, its cursor and what it composes are
    /// now, for an input method that asked to be told; `None` once no
    /// editor has focus, or another took it, so nothing stale is told.
    Anchor(Option<crate::CursorAnchor>),
}

/// Whether a [`Request::Held`] made `over` what the method held is stale
/// now that it `holds` this: the method typed, moved its cursor or changed
/// its composition meanwhile, and what it did is on its way to the
/// application, which asks again if the editor took other than that.
/// Applied, the request would put the older state back over the newer, and
/// the application, told of the newer, would not know to correct it. The
/// ends of the selection and of the composition are compared in either
/// order, and an empty composition is none: what the platform keeps either
/// way is the same.
///
/// What the application wrote is then lost to the method's typing: the
/// method's report, of its older text with the typing, reaches the editor
/// and is taken whole. Keeping both would need to know which text a
/// report was made from, which the platform does not say; it takes typing
/// inside the one turn between the application's write and the platform
/// getting to it.
#[must_use]
pub fn held_is_stale(over: Option<&crate::HeldText>, holds: &crate::HeldText) -> bool {
    let ends = |a: usize, b: usize| (a.min(b), a.max(b));
    let compose = |held: &crate::HeldText| {
        held.compose
            .as_ref()
            .map(|c| ends(c.start, c.end))
            .filter(|(start, end)| start < end)
    };
    over.is_some_and(|over| {
        over.text != holds.text
            || ends(over.anchor, over.cursor) != ends(holds.anchor, holds.cursor)
            || compose(over) != compose(holds)
    })
}

/// What this side wrote to the platform's method and the platform may not
/// hold yet. A platform that applies a write later, on its own thread
/// (Android's GameActivity queues it for the main thread), reports the
/// older state until then; compared with that, a second write made
/// meanwhile would look stale (see [`held_is_stale`]) and be dropped, and
/// the method would keep the older text with nothing to tell it again.
///
/// A report of the state from before the writes is read as the platform
/// behind until the method reports for itself ([`Writes::clear`]). A method
/// that got the writes and was taken back to that state by the user (a
/// deletion of what was written) is read the same way until its report
/// arrives, and a request made over the writes then puts them back. The
/// platform says only what it holds, not whose that is; the side that
/// reads its reports narrows this to the moment between the method
/// changing and its report being flagged.
#[derive(Debug, Default)]
pub struct Writes {
    /// What the platform held when the oldest write still pending was made.
    before: Option<crate::HeldText>,
    /// The writes the platform has not reported holding, oldest first.
    pending: std::collections::VecDeque<crate::HeldText>,
    /// The last writes made, landed, let go or on their way: what a method
    /// can hold by this side's doing.
    written: std::collections::VecDeque<crate::HeldText>,
    /// The method's own report, until a write follows it.
    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    typed: Option<crate::HeldText>,
}

/// Writes kept pending at most: the platform applies each within a turn of
/// its main thread, so more are a platform that has stopped applying them,
/// and the oldest is let go.
const PENDING_WRITES: usize = 16;

impl Writes {
    /// What a [`Request::Held`] is to take the method to hold now that the
    /// platform reports it `holds` this: the newest write while the report
    /// is the state before the writes or one of them, which the platform
    /// has yet to get past; otherwise the report itself, the method having
    /// changed it after the writes. A report the same as more than one
    /// write is taken as the oldest: the platform applies them in order,
    /// and one it has not reached is never let go.
    pub fn holds(&mut self, holds: crate::HeldText) -> crate::HeldText {
        let same = |a: &crate::HeldText, b: &crate::HeldText| !held_is_stale(Some(a), b);
        if let Some(applied) = self.pending.iter().position(|w| same(w, &holds)) {
            self.pending.drain(..=applied);
            self.before = Some(holds.clone());
        } else if !self.before.as_ref().is_some_and(|b| same(b, &holds))
            // A write let go for a report can land after a newer was made.
            && !self.written.iter().any(|w| same(w, &holds))
        {
            self.before = None;
            self.pending.clear();
        }
        self.pending.back().cloned().unwrap_or(holds)
    }

    /// What a method that has reported nothing, and so is not read, is
    /// taken to hold: the newest write, however many were let go before it,
    /// or what an empty one holds.
    pub fn unreported(&self) -> crate::HeldText {
        self.pending.back().cloned().unwrap_or_default()
    }

    /// The method reported what it holds for itself: what was written
    /// before has reached it, and a later report of the state from before
    /// those writes is the method's own, not the platform behind.
    pub fn clear(&mut self) {
        self.before = None;
        self.pending.clear();
    }

    /// The platform was told to hold `now` while it reported `holds`.
    pub fn wrote(&mut self, holds: &crate::HeldText, now: crate::HeldText) {
        if self.pending.is_empty() {
            self.before = Some(holds.clone());
        }
        if self.pending.len() == PENDING_WRITES {
            self.before = self.pending.pop_front();
        }
        if self.written.len() == PENDING_WRITES {
            self.written.pop_front();
        }
        self.written.push_back(now.clone());
        self.typed = None;
        self.pending.push_back(now);
    }
}

#[cfg(any(target_os = "android", test))]
impl Writes {
    pub(crate) fn wrote_it(&self, state: &crate::HeldText) -> bool {
        self.written.iter().any(|w| !held_is_stale(Some(w), state))
    }

    /// The method reported `now` for itself.
    pub(crate) fn reported(&mut self, now: &crate::HeldText) {
        self.clear();
        self.typed = Some(now.clone());
    }

    /// For a report of `state`: what to tell the application, when it is a
    /// write told back and not the method's own (`says_write`, asked only
    /// of a state that was written). That is the newest write, and none on
    /// its way is forgotten for it: one older can't age one newer.
    pub(crate) fn told_back(
        &mut self,
        state: &crate::HeldText,
        says_write: impl FnOnce() -> bool,
    ) -> Option<crate::HeldText> {
        (self.wrote_it(state) && says_write()).then(|| self.holds(state.clone()))
    }

    /// Whether a request made `over` is stale now that the method is taken
    /// to hold `holds` ([`Writes::holds`]). It is not while that is a write
    /// and not the method's last report: nothing was typed that it would
    /// put back, whichever of the writes and reports it was made over.
    pub(crate) fn stale(&self, over: Option<&crate::HeldText>, holds: &crate::HeldText) -> bool {
        let typed = self
            .typed
            .as_ref()
            .is_some_and(|t| !held_is_stale(Some(t), holds));
        held_is_stale(over, holds) && (typed || !self.wrote_it(holds))
    }
}

enum Msg<S, U> {
    Arrival(Arrival<S>),
    /// A write told back, with how many the application had asked for when
    /// the platform's side said it.
    #[cfg(any(target_os = "android", test))]
    ToldBack(u64, crate::HeldText),
    User(U),
    Wake,
}

/// The platform's side: what its thread hands events to. Clones share the
/// channel.
pub struct Relay<S, U> {
    tx: SyncSender<Msg<S, U>>,
    wake_queued: Arc<AtomicBool>,
    /// The method's reports handed to the application so far.
    reports: Arc<AtomicU64>,
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
            reports: Arc::clone(&self.reports),
        }
    }
}

impl<S: Send + 'static, U: Send + 'static> Relay<S, U> {
    /// Hands one event over; waits while the application is behind.
    ///
    /// # Errors
    /// The application's loop has ended.
    pub fn send(&self, arrival: Arrival<S>) -> Result<(), HostError> {
        if is_report(&arrival) {
            self.reports.fetch_add(1, Ordering::AcqRel);
        }
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

    /// The platform took the write `now`, asked for when the application
    /// had `seen` of the method's reports. With more on their way to it, it
    /// would end on the last of those and the method on the write: so the
    /// write follows them. Not the method's own report, so what was written
    /// is not forgotten for it ([`Writes::clear`]).
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn tell_again(&self, asked: Asked, now: &crate::HeldText) -> Result<(), HostError> {
        if self.reports.load(Ordering::Acquire) <= asked.seen {
            return Ok(());
        }
        self.tell_back(asked.writes, now.clone())
    }

    /// The method holds `now` by this side's doing, said once the first
    /// `through` writes the application asked for were handled. One it
    /// asked for since is newer, so it is not handed `now` after that.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn tell_back(&self, through: u64, now: crate::HeldText) -> Result<(), HostError> {
        self.reports.fetch_add(1, Ordering::AcqRel);
        self.tx
            .send(Msg::ToldBack(through, now))
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

/// What the application had, and had asked, when it made a request.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Asked {
    /// The reports it had.
    pub(crate) seen: u64,
    /// The writes it had asked for, the request among them if it is one.
    pub(crate) writes: u64,
}

/// Told each request with how the application stood then.
type Requests = Arc<dyn Fn(Request, Asked) + Send + Sync>;

fn is_report<S>(arrival: &Arrival<S>) -> bool {
    matches!(
        arrival,
        Arrival::Window(WindowEvent::Text(crate::TextEvent::Held(_)))
    )
}

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
        Self::stamped(capabilities, move |request, _| requests(request))
    }

    /// As [`RelayHost::new`], each request with how many of the method's
    /// reports the application had when it asked, for
    /// `Relay::tell_again`.
    pub(crate) fn stamped(
        capabilities: Capabilities,
        requests: impl Fn(Request, Asked) + Send + Sync + 'static,
    ) -> (Self, Relay<S, U>) {
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let relay = Relay {
            tx,
            wake_queued: Arc::new(AtomicBool::new(false)),
            reports: Arc::new(AtomicU64::new(0)),
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
    /// Insets told before there was a window, for the window once made.
    insets: Option<crate::Insets>,
    deadline: Option<Instant>,
    exit: bool,
    app: AppState,
    name: String,
    capabilities: Capabilities,
    requests: Requests,
    /// The method's reports the application has had.
    seen: u64,
    /// The writes it has asked for; atomic as a request takes `&self`.
    writes: AtomicU64,
}

impl<S> State<S> {
    fn asked(&self) -> Asked {
        Asked {
            seen: self.seen,
            writes: self.writes.load(Ordering::Relaxed),
        }
    }
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
        if matches!(request, Request::Held { .. }) {
            self.state.writes.fetch_add(1, Ordering::Relaxed);
        }
        (self.state.requests)(request, self.state.asked());
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
            insets: None,
            deadline: None,
            exit: false,
            app: AppState::Hidden,
            name,
            capabilities,
            requests,
            seen: 0,
            writes: AtomicU64::new(0),
        };
        // Insets kept until there was a window are told right after what
        // made it, whatever that was (a message, or the wait that follows a
        // turn), and so before any newer ones.
        let mut deliver = |state: &mut State<S>, event: Event<'static, (), U>| {
            app.event(&mut RelayControl { state }, event);
            if state.window
                && let Some(insets) = state.insets.take()
            {
                let told = Event::Window {
                    id: WINDOW,
                    event: WindowEvent::Insets(insets),
                    raw: None,
                };
                app.event(&mut RelayControl { state }, told);
            }
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
                    #[cfg(any(target_os = "android", test))]
                    Msg::ToldBack(through, now) => {
                        if state.writes.load(Ordering::Relaxed) > through {
                            state.seen += 1;
                        } else {
                            let text = WindowEvent::Text(crate::TextEvent::Held(now));
                            arrive(&mut state, &mut deliver, Arrival::Window(text));
                        }
                    }
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
        (state.requests)(Request::Exit, state.asked());
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
    if is_report(&arrival) {
        // Before it is delivered: what is asked while handling it knows it.
        state.seen += 1;
    }
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
        // Told once, as they change: kept until there is a window to
        // tell them.
        Arrival::Window(WindowEvent::Insets(insets)) if !state.window => {
            state.insets = Some(insets);
        }
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
