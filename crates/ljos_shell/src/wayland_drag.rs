//! Drags from other applications on Wayland, which winit 0.30 does not
//! follow at all: a `wl_data_device` of the shell's own, on the connection
//! winit opened.
//!
//! winit reads the connection, and the device's events wait in a queue of
//! their own until the loop is about to sleep ([`Drags::dispatch`]). That
//! is not enough alone: winit's loop goes back to sleep without being
//! about to when the connection brought none of its own events, as when a
//! drag comes in over a window no pointer is over, so a drag's events
//! would wait for the next thing a window hears. While no pointer is over
//! a window of the application, or a drag is over one or out of it, a
//! thread waits on the connection too and dispatches the queue. It wakes
//! for every event any window gets, so it does not while a pointer is
//! over a window, where every click would wake it: winit hears the
//! pointer then, and the loop is about to sleep after each event. Another
//! thread reads what a drag carries when it comes in (a `DropArea`
//! decides in `entered` from its `urls`, `text` and `formats`), as the
//! source may be this application, which writes it from the loop. It
//! posts each step into the loop in order, which tells the view of the
//! window the drag is over and gives the compositor the view's answer.
//!
//! Drags out of the application go the same way: a `wl_data_source` of
//! the shell's own, started from the press the drag began with. The
//! compositor wants that press's serial, which winit keeps to itself, so
//! the shell has a `wl_pointer` of its own on each seat too; it hears the
//! presses and nothing comes of them. The source writes what the drag
//! carries from threads of its own, as the target may be this
//! application, which reads it on the thread above.

use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;

use crate::drags::DragStep;

/// The surface of a window, as a drag names it; `None` off Wayland.
pub(crate) fn surface_of(window: &Window) -> Option<usize> {
    imp::surface_of(window)
}

/// Starts following drags when winit's display is a Wayland one.
pub(crate) fn start(
    display: &OwnedDisplayHandle,
    post: Arc<dyn Fn(DragStep) + Send + Sync>,
) -> Option<Drags> {
    imp::start(display, post).map(Drags)
}

/// Whether winit's display is a Wayland one, where nothing but [`start`]
/// brings drags from other applications.
pub(crate) fn is_wayland(display: &OwnedDisplayHandle) -> bool {
    imp::is_wayland(display)
}

/// What follows drags; [`Drags::stop`] ends it.
pub(crate) struct Drags(imp::Thread);

impl Drags {
    /// Handles what the compositor said of drags since the last time; the
    /// loop calls it each time before it sleeps.
    pub(crate) fn dispatch(&mut self) {
        self.0.dispatch();
    }

    pub(crate) fn stop(self) {
        self.0.stop();
    }

    /// Hands a drag out of the application to the compositor, from the
    /// press held now. `false` when no press is held, the compositor has
    /// no drag actions (before version 3), or a drag is under way.
    pub(crate) fn start_out(
        &mut self,
        data: &ljos_platform::DragData,
        actions: ljos_platform::DragActions,
    ) -> bool {
        self.0.start_out(data, actions)
    }

    /// Gives the drag out of the application up.
    pub(crate) fn cancel_out(&mut self) {
        self.0.cancel_out();
    }

    /// How the drag out of the application ended, once it has: the action
    /// the target took, or `None`. [`DragStep::OutEnded`] says it has.
    pub(crate) fn out_ended(&mut self) -> Option<Option<ljos_platform::DropAction>> {
        self.0.out_ended()
    }
}

impl std::fmt::Debug for Drags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Drags")
    }
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
mod imp {
    use std::io::{Read, Write};
    use std::os::fd::{AsFd, AsRawFd, OwnedFd};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use egui_winit::winit::event_loop::OwnedDisplayHandle;
    use egui_winit::winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use egui_winit::winit::window::Window;
    use ljos_platform::{DragActions, DragData, DropAction, DropActions};
    use wayland_client::backend::Backend;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_callback::{self, WlCallback};
    use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
    use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
    use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
    use wayland_client::protocol::wl_data_source::{self, WlDataSource};
    use wayland_client::protocol::wl_pointer::{self, ButtonState, WlPointer};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::{self, WlSeat};
    use wayland_client::protocol::wl_surface::WlSurface;
    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child,
    };

    use crate::drags::{At, DragStep, Reply};

    /// The most of one format that is read; a larger one is left out, as
    /// a source that offers a film as a drag would otherwise be held in
    /// memory for every window it crosses.
    const MOST: usize = 8 << 20;

    /// How long a source has to write one format. A source that never
    /// writes would otherwise hold every later drag up.
    const WRITE_WAIT: Duration = Duration::from_millis(500);

    /// Where the application's answer to one step goes.
    pub(crate) struct Answer {
        conn: Connection,
        offer: WlDataOffer,
        serial: u32,
        /// The format said to be taken: the source asks for one.
        format: Option<String>,
    }

    impl Reply for Answer {
        /// Tells the source whether a drop here would be taken, and with
        /// which action: the one the DropArea chose is the only one the
        /// compositor may pick.
        fn taken(&self, action: Option<DropAction>) {
            self.offer
                .accept(self.serial, action.and_then(|_| self.format.clone()));
            if self.offer.version() >= 3 {
                let action = action.map_or(DndAction::empty(), dnd);
                self.offer.set_actions(action, action);
            }
            // An error is a gone compositor, which winit hears of too.
            let _ = self.conn.flush();
        }

        /// After the drop: what was taken is done with, and the offer goes.
        fn dropped(self: Box<Self>, action: Option<DropAction>) {
            if action.is_some() && self.offer.version() >= 3 {
                self.offer.finish();
            }
            self.offer.destroy();
            let _ = self.conn.flush();
        }
    }

    fn dnd(action: DropAction) -> DndAction {
        match action {
            DropAction::Copy => DndAction::Copy,
            DropAction::Move => DndAction::Move,
            // Never told: a Wayland source allows no link (`actions_of`).
            DropAction::Link => DndAction::Copy,
        }
    }

    /// What a source that allows `source` offers, the compositor having
    /// picked `selected` so far. Wayland has no link: its third action is
    /// `ask`, which leaves the choice to the target, and is not one a
    /// DropArea can take, so it is left out.
    fn actions_of(source: DndAction, selected: DndAction) -> DragActions {
        let allowed: DropActions = [
            (DndAction::Copy, DropAction::Copy),
            (DndAction::Move, DropAction::Move),
        ]
        .into_iter()
        .filter(|(d, _)| source.contains(*d))
        .map(|(_, a)| a)
        .collect();
        let proposed = picked(selected)
            .filter(|a| allowed.contains(*a))
            .or_else(|| {
                allowed
                    .contains(DropAction::Copy)
                    .then_some(DropAction::Copy)
            })
            .or_else(|| allowed.iter().next())
            .unwrap_or(DropAction::Copy);
        DragActions { allowed, proposed }
    }

    pub(crate) fn surface_of(window: &Window) -> Option<usize> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Wayland(h) => Some(h.surface.as_ptr() as usize),
            _ => None,
        }
    }

    pub(crate) struct Thread {
        conn: Connection,
        qh: QueueHandle<State>,
        /// One for each seat, as the seats are numbered.
        devices: Vec<WlDataDevice>,
        manager: WlDataDeviceManager,
        /// Dispatched by the loop, or by the dispatcher while it waits on
        /// the connection ([`State::away`]).
        events: Arc<Mutex<Events>>,
        out: Arc<Shared>,
        stopping: Arc<AtomicBool>,
        dispatcher: JoinHandle<()>,
        reader: JoinHandle<()>,
    }

    struct Events {
        queue: EventQueue<State>,
        state: State,
    }

    impl Thread {
        pub(crate) fn dispatch(&mut self) {
            let mut events = match self.events.try_lock() {
                Ok(events) => events,
                Err(TryLockError::Poisoned(events)) => events.into_inner(),
                // The dispatcher has the queue, and dispatches it.
                Err(TryLockError::WouldBlock) => return,
            };
            let Events { queue, state } = &mut *events;
            // An error is a gone compositor, which winit hears of too.
            let _ = queue.dispatch_pending(state);
            self.out.set_away(state.away());
        }

        pub(crate) fn start_out(&mut self, data: &DragData, actions: DragActions) -> bool {
            if self.manager.version() < 3 {
                return false;
            }
            // winit and the queue hear the press in the same read, and the
            // loop has not been about to sleep since.
            self.dispatch();
            let mut out = self.out.lock();
            if out.drag.is_some() {
                return false;
            }
            let Some(press) = out.press.clone() else {
                return false;
            };
            let Some(device) = self.devices.get(press.seat) else {
                return false;
            };
            let formats = offered(data);
            let source = self
                .manager
                .create_data_source(&self.qh, Source(Arc::new(formats.clone())));
            for (format, _) in &formats {
                source.offer(format.clone());
            }
            let allowed = actions
                .allowed
                .iter()
                .filter(|a| *a != DropAction::Link)
                .fold(DndAction::empty(), |all, a| all | dnd(a));
            source.set_actions(if allowed.is_empty() {
                DndAction::Copy
            } else {
                allowed
            });
            device.start_drag(Some(&source), &press.surface, None, press.serial);
            if self.conn.flush().is_err() {
                source.destroy();
                return false;
            }
            out.ended = None;
            out.drag = Some(Out {
                source,
                action: None,
            });
            drop(out);
            self.out.set_away(true);
            true
        }

        pub(crate) fn cancel_out(&mut self) {
            let mut out = self.out.lock();
            if let Some(drag) = out.drag.take() {
                // The compositor ends a drag whose source goes.
                drag.source.destroy();
                let _ = self.conn.flush();
            }
            out.ended = None;
        }

        pub(crate) fn out_ended(&mut self) -> Option<Option<DropAction>> {
            self.out.lock().ended.take()
        }

        pub(crate) fn stop(self) {
            // The dispatcher looks when it is woken, or when its queue next
            // hears something, which a sync makes it. It ends the reader as
            // it ends.
            self.out.stop(&self.stopping);
            self.conn.display().sync(&self.qh, ());
            let _ = self.conn.flush();
            let _ = self.dispatcher.join();
            let _ = self.reader.join();
        }
    }

    /// What the loop and the dispatcher both see of drags out: the press a
    /// drag starts from, and the drag under way; and whether the
    /// dispatcher is to wait on the connection.
    #[derive(Default)]
    struct Shared {
        out: Mutex<OutState>,
        /// Set with the queue held, by whoever dispatched it last.
        away: Mutex<bool>,
        /// Told when `away` becomes true, or the shell stops.
        wake: Condvar,
    }

    impl Shared {
        fn lock(&self) -> MutexGuard<'_, OutState> {
            lock(&self.out)
        }

        fn set_away(&self, away: bool) {
            *lock(&self.away) = away;
            if away {
                self.wake.notify_all();
            }
        }

        /// Waits until the dispatcher is to wait on the connection; `false`
        /// when the shell stops instead.
        fn wait_away(&self, stopping: &AtomicBool) -> bool {
            let mut away = lock(&self.away);
            while !*away && !stopping.load(Ordering::Acquire) {
                away = self.wake.wait(away).unwrap_or_else(PoisonError::into_inner);
            }
            !stopping.load(Ordering::Acquire)
        }

        fn stop(&self, stopping: &AtomicBool) {
            let _away = lock(&self.away);
            stopping.store(true, Ordering::Release);
            self.wake.notify_all();
        }
    }

    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[derive(Default)]
    struct OutState {
        /// The press held now, which a drag out starts from.
        press: Option<Press>,
        /// The drag out of the application under way.
        drag: Option<Out>,
        /// How the last drag out ended, until the loop asks.
        ended: Option<Option<DropAction>>,
    }

    /// What the thread does, in the order the compositor said it.
    enum Job {
        /// Reads what the drag carries, then posts that it came in.
        Entered {
            surface: usize,
            at: (f64, f64),
            formats: Vec<String>,
            actions: DragActions,
            answer: Answer,
        },
        Post(DragStep),
    }

    /// The offer of the drag in progress and what is said back with it.
    struct Current {
        offer: WlDataOffer,
        serial: u32,
        format: Option<String>,
    }

    struct State {
        conn: Connection,
        jobs: Sender<Job>,
        current: Option<Current>,
        /// Where the drag in progress was last, on its surface.
        at: (f64, f64),
        /// The shell's own pointer of each seat, while the seat has one:
        /// asking a seat that never had a pointer for one is a protocol
        /// error, so it is made when the seat says it has one.
        pointers: Vec<Option<WlPointer>>,
        /// The surface each seat's pointer is over.
        over: Vec<Option<WlSurface>>,
        out: Arc<Shared>,
    }

    #[derive(Clone)]
    struct Press {
        seat: usize,
        serial: u32,
        surface: WlSurface,
    }

    struct Out {
        source: WlDataSource,
        /// The action the compositor picked last.
        action: Option<DropAction>,
    }

    /// What a drag out of the application offers, by MIME type.
    struct Source(Arc<Vec<(String, Vec<u8>)>>);

    /// What `data` is offered as: each of its formats, with plain text
    /// also under the name Wayland's toolkits ask for.
    fn offered(data: &DragData) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for format in data.formats() {
            let Some(text) = data.data(&format) else {
                continue;
            };
            if format == "text/plain" {
                out.push((
                    "text/plain;charset=utf-8".to_owned(),
                    text.clone().into_bytes(),
                ));
            }
            out.push((format, text.into_bytes()));
        }
        out
    }

    /// The action a compositor picked, as a DropArea names it.
    fn picked(action: DndAction) -> Option<DropAction> {
        if action.contains(DndAction::Move) {
            Some(DropAction::Move)
        } else if action.contains(DndAction::Copy) {
            Some(DropAction::Copy)
        } else {
            None
        }
    }

    impl State {
        fn post(&self, step: DragStep) {
            let _ = self.jobs.send(Job::Post(step));
        }

        /// Whether the loop may not hear of the queue's events: no
        /// pointer is over a window of the application, which winit would
        /// hear, or a drag is over one or out of it, which it does not.
        fn away(&self) -> bool {
            self.current.is_some()
                || self.over.iter().all(Option::is_none)
                || self.out.lock().drag.is_some()
        }

        fn answer(&self) -> Option<Answer> {
            self.current.as_ref().map(|c| Answer {
                conn: self.conn.clone(),
                offer: c.offer.clone(),
                serial: c.serial,
                format: c.format.clone(),
            })
        }
    }

    /// What an offer said of itself: its formats, in the order it said
    /// them, the actions its source allows, and the one the compositor
    /// picked.
    struct Offered(Mutex<OfferedState>);

    struct OfferedState {
        formats: Vec<String>,
        source: DndAction,
        selected: DndAction,
    }

    impl Default for Offered {
        /// An offer older than version 3 says nothing of actions: it is a
        /// copy.
        fn default() -> Self {
            Self(Mutex::new(OfferedState {
                formats: Vec::new(),
                source: DndAction::Copy,
                selected: DndAction::empty(),
            }))
        }
    }

    impl Offered {
        fn formats(&self) -> Vec<String> {
            self.0.lock().map(|o| o.formats.clone()).unwrap_or_default()
        }

        fn actions(&self) -> DragActions {
            self.0.lock().map_or_else(
                |_| DragActions::default(),
                |o| actions_of(o.source, o.selected),
            )
        }
    }

    pub(crate) fn is_wayland(display: &OwnedDisplayHandle) -> bool {
        matches!(
            display.display_handle().map(|h| h.as_raw()),
            Ok(RawDisplayHandle::Wayland(_))
        )
    }

    pub(crate) fn start(
        display: &OwnedDisplayHandle,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Thread> {
        let RawDisplayHandle::Wayland(handle) = display.display_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: winit's display stays connected while its event loop
        // lives, which is the whole of the shell's run; the thread ends in
        // `Desktop::shut_down`, before the loop is let go.
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();
        let manager: WlDataDeviceManager = globals.bind(&qh, 1..=3, ()).ok()?;
        // The seats there at start; one plugged in later is not followed.
        // A seat says what it has as soon as it is bound, and again when
        // that changes; its pointer is made then. Version 3 lets one go.
        let devices: Vec<WlDataDevice> = globals.contents().with_list(|list| {
            list.iter()
                .filter(|g| g.interface == WlSeat::interface().name)
                .enumerate()
                .map(|(i, g)| {
                    let seat: WlSeat = globals.registry().bind(g.name, g.version.min(3), &qh, i);
                    manager.get_data_device(&seat, &qh, ())
                })
                .collect()
        });
        if devices.is_empty() {
            return None;
        }
        conn.flush().ok()?;
        let (jobs, todo) = channel::<Job>();
        let reader = std::thread::Builder::new()
            .name("ljos-wayland-drags".into())
            .spawn({
                let conn = conn.clone();
                move || {
                    for job in todo {
                        match job {
                            Job::Entered {
                                surface,
                                at,
                                formats,
                                actions,
                                answer,
                            } => {
                                let data = DragData::from_formats(formats.iter().filter_map(|f| {
                                    read(&conn, &answer.offer, f).map(|bytes| (f, bytes))
                                }));
                                post(DragStep::Entered {
                                    target: surface,
                                    at: At::Logical(at.0, at.1),
                                    data,
                                    actions,
                                    answer: Box::new(answer),
                                });
                            }
                            Job::Post(step) => post(step),
                        }
                    }
                }
            })
            .ok()?;
        let out = Arc::new(Shared::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let state = State {
            conn: conn.clone(),
            jobs,
            current: None,
            at: (0.0, 0.0),
            pointers: vec![None; devices.len()],
            over: vec![None; devices.len()],
            out: Arc::clone(&out),
        };
        // No pointer is over a window yet.
        out.set_away(true);
        let events = Arc::new(Mutex::new(Events { queue, state }));
        let dispatcher = std::thread::Builder::new()
            .name("ljos-wayland-drag-events".into())
            .spawn({
                let events = Arc::clone(&events);
                let out = Arc::clone(&out);
                let stopping = Arc::clone(&stopping);
                move || {
                    while out.wait_away(&stopping) {
                        let mut events = lock(&events);
                        let Events { queue, state } = &mut *events;
                        while state.away() && !stopping.load(Ordering::Acquire) {
                            // An error is a gone compositor, which winit
                            // hears of too.
                            if queue.blocking_dispatch(state).is_err() {
                                return;
                            }
                        }
                        out.set_away(state.away());
                    }
                }
            })
            .ok()?;
        Some(Thread {
            conn,
            qh,
            devices,
            manager,
            events,
            out,
            stopping,
            dispatcher,
            reader,
        })
    }

    /// Reads one format of `offer`, or `None` when the source wrote none,
    /// too much, or not in time.
    fn read(conn: &Connection, offer: &WlDataOffer, format: &str) -> Option<Vec<u8>> {
        let (mut reader, writer) = std::io::pipe().ok()?;
        offer.receive(format.to_owned(), writer.as_fd());
        conn.flush().ok()?;
        // Ours closed, the end of the source's copy ends the data.
        drop(writer);
        let deadline = Instant::now() + WRITE_WAIT;
        let mut out = Vec::new();
        let mut buf = [0u8; 16 << 10];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || !readable(&reader, left) {
                return None;
            }
            match reader.read(&mut buf) {
                Ok(0) => return Some(out),
                Ok(n) if out.len() + n <= MOST => out.extend_from_slice(&buf[..n]),
                Ok(_) => return None,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
    }

    fn readable(fd: &impl AsRawFd, wait: Duration) -> bool {
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = libc::c_int::try_from(wait.as_millis()).unwrap_or(libc::c_int::MAX);
        // SAFETY: one pollfd, which lives across the call.
        let ready = unsafe { libc::poll(&raw mut poll, 1, ms) };
        ready > 0 && poll.revents & (libc::POLLIN | libc::POLLHUP) != 0
    }

    /// What a source offering `formats` is told is taken: a list of URLs,
    /// else text, else the first it offers.
    fn chosen(formats: &[String]) -> Option<String> {
        let is = |want: &str| formats.iter().find(|f| f.eq_ignore_ascii_case(want));
        is("text/uri-list")
            .or_else(|| is("text/plain;charset=utf-8"))
            .or_else(|| is("text/plain"))
            .or_else(|| formats.first())
            .cloned()
    }

    impl Dispatch<WlDataDevice, ()> for State {
        fn event(
            state: &mut Self,
            _: &WlDataDevice,
            event: wl_data_device::Event,
            _: &(),
            conn: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                wl_data_device::Event::Enter {
                    serial,
                    surface,
                    x,
                    y,
                    id,
                } => {
                    if let Some(old) = state.current.take() {
                        old.offer.destroy();
                    }
                    let Some(offer) = id else {
                        return; // A drag that carries nothing.
                    };
                    let surface = surface.id().as_ptr() as usize;
                    let (formats, actions) = offer
                        .data::<Offered>()
                        .map(|o| (o.formats(), o.actions()))
                        .unwrap_or_default();
                    state.at = (x, y);
                    state.current = Some(Current {
                        offer,
                        serial,
                        format: chosen(&formats),
                    });
                    let Some(answer) = state.answer() else { return };
                    let _ = state.jobs.send(Job::Entered {
                        surface,
                        at: (x, y),
                        formats,
                        actions,
                        answer,
                    });
                }
                wl_data_device::Event::Motion { x, y, .. } => {
                    state.at = (x, y);
                    if let Some(answer) = state.answer() {
                        state.post(DragStep::Moved {
                            at: At::Logical(x, y),
                            actions: None,
                            answer: Box::new(answer),
                        });
                    }
                }
                wl_data_device::Event::Leave => {
                    if let Some(old) = state.current.take() {
                        old.offer.destroy();
                        let _ = conn.flush();
                        state.post(DragStep::Left);
                    }
                }
                wl_data_device::Event::Drop => {
                    // The loop finishes the offer and lets it go.
                    if let Some(answer) = state.answer() {
                        state.current = None;
                        state.post(DragStep::Dropped {
                            answer: Box::new(answer),
                        });
                    }
                }
                // A selection's offer: the clipboard is the platform's.
                wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
                _ => {}
            }
        }

        event_created_child!(State, WlDataDevice, [
            wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Offered::default()),
        ]);
    }

    impl Dispatch<WlDataOffer, Offered> for State {
        fn event(
            state: &mut Self,
            offer: &WlDataOffer,
            event: wl_data_offer::Event,
            offered: &Offered,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let Ok(mut o) = offered.0.lock() else { return };
            let before = actions_of(o.source, o.selected);
            match event {
                wl_data_offer::Event::Offer { mime_type } => {
                    if !o.formats.contains(&mime_type) {
                        o.formats.push(mime_type);
                    }
                    return;
                }
                wl_data_offer::Event::SourceActions {
                    source_actions: WEnum::Value(a),
                } => o.source = a,
                wl_data_offer::Event::Action {
                    dnd_action: WEnum::Value(a),
                } => o.selected = a,
                _ => return,
            }
            let after = actions_of(o.source, o.selected);
            drop(o);
            // A source that now allows or would like another action, as a
            // modifier key changes it, asks the area under it again.
            if after != before
                && state.current.as_ref().is_some_and(|c| &c.offer == offer)
                && let Some(answer) = state.answer()
            {
                state.post(DragStep::Moved {
                    at: At::Logical(state.at.0, state.at.1),
                    actions: Some(after),
                    answer: Box::new(answer),
                });
            }
        }
    }

    /// The shell's own pointer of seat `seat`: where it is, and the press
    /// held now.
    impl Dispatch<WlPointer, usize> for State {
        fn event(
            state: &mut Self,
            _: &WlPointer,
            event: wl_pointer::Event,
            seat: &usize,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let seat = *seat;
            match event {
                wl_pointer::Event::Enter { surface, .. } => {
                    if let Some(over) = state.over.get_mut(seat) {
                        *over = Some(surface);
                    }
                }
                wl_pointer::Event::Leave { .. } => {
                    if let Some(over) = state.over.get_mut(seat) {
                        *over = None;
                    }
                }
                wl_pointer::Event::Button {
                    serial,
                    state: WEnum::Value(ButtonState::Pressed),
                    ..
                } => {
                    if let Some(Some(surface)) = state.over.get(seat) {
                        state.out.lock().press = Some(Press {
                            seat,
                            serial,
                            surface: surface.clone(),
                        });
                    }
                }
                wl_pointer::Event::Button {
                    state: WEnum::Value(ButtonState::Released),
                    ..
                } => state.out.lock().let_go(seat),
                _ => {}
            }
        }
    }

    impl Dispatch<WlDataSource, Source> for State {
        fn event(
            state: &mut Self,
            source: &WlDataSource,
            event: wl_data_source::Event,
            offered: &Source,
            conn: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let mut out = state.out.lock();
            let ours = out.drag.as_ref().is_some_and(|o| &o.source == source);
            match event {
                wl_data_source::Event::Send { mime_type, fd } => {
                    let data = offered
                        .0
                        .iter()
                        .find(|(f, _)| *f == mime_type)
                        .map(|(_, d)| d.clone())
                        .unwrap_or_default();
                    write_out(fd, data);
                }
                wl_data_source::Event::Action {
                    dnd_action: WEnum::Value(a),
                } => {
                    if let Some(drag) = out.drag.as_mut().filter(|_| ours) {
                        drag.action = picked(a);
                    }
                }
                wl_data_source::Event::DndFinished => {
                    if ours && let Some(drag) = out.drag.take() {
                        out.ended = Some(drag.action);
                        drop(out);
                        state.post(DragStep::OutEnded);
                    }
                    source.destroy();
                    let _ = conn.flush();
                }
                // Turned down, let go over nothing, or ended by the
                // compositor.
                wl_data_source::Event::Cancelled => {
                    if ours {
                        out.drag = None;
                        out.ended = Some(None);
                        drop(out);
                        state.post(DragStep::OutEnded);
                    }
                    source.destroy();
                    let _ = conn.flush();
                }
                _ => {}
            }
        }
    }

    /// Writes `data` to a target that asked for it, on a thread of its
    /// own: the target may be slow to read, or this application.
    fn write_out(fd: OwnedFd, data: Vec<u8>) {
        let spawned = std::thread::Builder::new()
            .name("ljos-wayland-drag-out".into())
            .spawn(move || {
                // A target that stops reading has what it read.
                let _ = std::fs::File::from(fd).write_all(&data);
            });
        // No thread: the fd closes, and the target reads nothing.
        drop(spawned);
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

    impl Dispatch<WlSeat, usize> for State {
        fn event(
            state: &mut Self,
            seat: &WlSeat,
            event: wl_seat::Event,
            i: &usize,
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            let wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(has),
            } = event
            else {
                return;
            };
            let Some(pointer) = state.pointers.get_mut(*i) else {
                return;
            };
            let has = has.contains(wl_seat::Capability::Pointer);
            if has && pointer.is_none() {
                *pointer = Some(seat.get_pointer(qh, *i));
            } else if !has && let Some(gone) = pointer.take() {
                if gone.version() >= 3 {
                    gone.release();
                }
                state.over[*i] = None;
                state.out.lock().let_go(*i);
            }
        }
    }

    impl OutState {
        /// Seat `seat`'s press is let go of, or its pointer is gone.
        fn let_go(&mut self, seat: usize) {
            if self.press.as_ref().is_some_and(|p| p.seat == seat) {
                self.press = None;
            }
        }
    }

    /// The sync [`Thread::stop`] wakes the dispatcher with.
    impl Dispatch<WlCallback, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlCallback,
            _: wl_callback::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlDataDeviceManager, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlDataDeviceManager,
            _: <WlDataDeviceManager as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Issue #115: Wayland's actions are copy, move and ask; ask is no
        /// action a DropArea takes.
        #[test]
        fn a_wayland_source_allows_copy_and_move_and_the_compositor_picks() {
            let both = actions_of(
                DndAction::Copy | DndAction::Move | DndAction::Ask,
                DndAction::Move,
            );
            assert_eq!(both.allowed.qt(), 1 | 2);
            assert_eq!(both.proposed, DropAction::Move);
            let none_picked = actions_of(DndAction::Copy | DndAction::Move, DndAction::empty());
            assert_eq!(none_picked.proposed, DropAction::Copy);
            let only_move = actions_of(DndAction::Move, DndAction::Ask);
            assert_eq!(only_move.proposed, DropAction::Move);
        }

        #[test]
        fn plain_text_is_offered_under_wayland_s_name_too() {
            let data = DragData::from_formats([("text/plain", "hi"), ("application/x-a", "1")]);
            let names: Vec<String> = offered(&data).into_iter().map(|(f, _)| f).collect();
            assert_eq!(
                names,
                ["text/plain;charset=utf-8", "text/plain", "application/x-a"]
            );
        }

        #[test]
        fn a_source_is_told_a_list_of_urls_then_text_then_its_first_format_is_taken() {
            let f = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
            assert_eq!(
                chosen(&f(&["text/plain", "text/uri-list"])).as_deref(),
                Some("text/uri-list")
            );
            assert_eq!(
                chosen(&f(&["application/x-a", "text/plain"])).as_deref(),
                Some("text/plain")
            );
            assert_eq!(
                chosen(&f(&["application/x-a"])).as_deref(),
                Some("application/x-a")
            );
            assert_eq!(chosen(&[]), None);
        }
    }
}

#[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
mod imp {
    use std::sync::Arc;

    use egui_winit::winit::event_loop::OwnedDisplayHandle;
    use egui_winit::winit::window::Window;

    use crate::drags::DragStep;

    pub(crate) enum Thread {}

    impl Thread {
        pub(crate) fn dispatch(&mut self) {
            match *self {}
        }

        pub(crate) fn start_out(
            &mut self,
            _data: &ljos_platform::DragData,
            _actions: ljos_platform::DragActions,
        ) -> bool {
            match *self {}
        }

        pub(crate) fn cancel_out(&mut self) {
            match *self {}
        }

        pub(crate) fn out_ended(&mut self) -> Option<Option<ljos_platform::DropAction>> {
            match *self {}
        }

        pub(crate) fn stop(self) {
            match self {}
        }
    }

    pub(crate) fn surface_of(_window: &Window) -> Option<usize> {
        None
    }

    pub(crate) fn is_wayland(_display: &OwnedDisplayHandle) -> bool {
        false
    }

    pub(crate) fn start(
        _display: &OwnedDisplayHandle,
        _post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Thread> {
        None
    }
}
