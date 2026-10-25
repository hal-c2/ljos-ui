//! Drags from other applications on Wayland, which winit 0.30 does not
//! follow at all: a `wl_data_device` of the shell's own, on the connection
//! winit opened.
//!
//! winit reads the connection, and the device's events wait in a queue of
//! their own until the loop is about to sleep ([`Drags::dispatch`]). A
//! thread that read the connection too would wake for every event any
//! window gets. A thread still reads what a drag carries when it comes in
//! (a `DropArea` decides in `entered` from its `urls`, `text` and
//! `formats`), as the source may be this application, which writes it from
//! the loop. It posts each step into the loop in order, which tells the
//! view of the window the drag is over and gives the compositor the view's
//! answer.

use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_platform::DragData;

/// One step of a drag, for the window whose surface is `surface`.
pub(crate) enum DragStep {
    Entered {
        surface: usize,
        /// In the surface's logical pixels.
        at: (f64, f64),
        data: DragData,
        answer: imp::Answer,
    },
    Moved {
        at: (f64, f64),
        answer: imp::Answer,
    },
    Left,
    Dropped {
        answer: imp::Answer,
    },
}

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
}

impl std::fmt::Debug for Drags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Drags")
    }
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
mod imp {
    use std::io::Read;
    use std::os::fd::{AsFd, AsRawFd};
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use egui_winit::winit::event_loop::OwnedDisplayHandle;
    use egui_winit::winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use egui_winit::winit::window::Window;
    use ljos_platform::DragData;
    use wayland_client::backend::Backend;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
    use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
    use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::{self, WlSeat};
    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle, event_created_child,
    };

    use super::DragStep;

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

    impl Answer {
        /// Tells the source whether a drop here would be taken.
        pub(crate) fn taken(&self, taken: bool) {
            self.offer
                .accept(self.serial, taken.then(|| self.format.clone()).flatten());
            if self.offer.version() >= 3 {
                let actions = if taken {
                    DndAction::Copy | DndAction::Move
                } else {
                    DndAction::empty()
                };
                self.offer.set_actions(actions, DndAction::Copy);
            }
            // An error is a gone compositor, which winit hears of too.
            let _ = self.conn.flush();
        }

        /// After the drop: what was taken is done with, and the offer goes.
        pub(crate) fn dropped(self, taken: bool) {
            if taken && self.offer.version() >= 3 {
                self.offer.finish();
            }
            self.offer.destroy();
            let _ = self.conn.flush();
        }
    }

    pub(crate) fn surface_of(window: &Window) -> Option<usize> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Wayland(h) => Some(h.surface.as_ptr() as usize),
            _ => None,
        }
    }

    pub(crate) struct Thread {
        queue: EventQueue<State>,
        state: State,
        /// Kept alive with the queue: dropping them sends nothing, but
        /// their events need them.
        _devices: Vec<WlDataDevice>,
        _manager: WlDataDeviceManager,
        thread: JoinHandle<()>,
    }

    impl Thread {
        pub(crate) fn dispatch(&mut self) {
            // An error is a gone compositor, which winit hears of too.
            let _ = self.queue.dispatch_pending(&mut self.state);
        }

        pub(crate) fn stop(self) {
            // The thread ends when its jobs end.
            drop(self.state);
            let _ = self.thread.join();
        }
    }

    /// What the thread does, in the order the compositor said it.
    enum Job {
        /// Reads what the drag carries, then posts that it came in.
        Entered {
            surface: usize,
            at: (f64, f64),
            formats: Vec<String>,
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
    }

    impl State {
        fn post(&self, step: DragStep) {
            let _ = self.jobs.send(Job::Post(step));
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

    /// The formats an offer said it has, in the order it said them.
    #[derive(Default)]
    struct Formats(Mutex<Vec<String>>);

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
        let devices: Vec<WlDataDevice> = globals.contents().with_list(|list| {
            list.iter()
                .filter(|g| g.interface == WlSeat::interface().name)
                .map(|g| {
                    let seat: WlSeat = globals.registry().bind(g.name, g.version.min(1), &qh, ());
                    manager.get_data_device(&seat, &qh, ())
                })
                .collect()
        });
        if devices.is_empty() {
            return None;
        }
        conn.flush().ok()?;
        let (jobs, todo) = channel::<Job>();
        let thread = std::thread::Builder::new()
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
                                answer,
                            } => {
                                let data = DragData::from_formats(formats.iter().filter_map(|f| {
                                    read(&conn, &answer.offer, f).map(|bytes| (f, bytes))
                                }));
                                post(DragStep::Entered {
                                    surface,
                                    at,
                                    data,
                                    answer,
                                });
                            }
                            Job::Post(step) => post(step),
                        }
                    }
                }
            })
            .ok()?;
        Some(Thread {
            queue,
            state: State {
                conn,
                jobs,
                current: None,
            },
            _devices: devices,
            _manager: manager,
            thread,
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
                    let formats = offer
                        .data::<Formats>()
                        .map(|f| f.0.lock().map(|f| f.clone()).unwrap_or_default())
                        .unwrap_or_default();
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
                        answer,
                    });
                }
                wl_data_device::Event::Motion { x, y, .. } => {
                    if let Some(answer) = state.answer() {
                        state.post(DragStep::Moved { at: (x, y), answer });
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
                        state.post(DragStep::Dropped { answer });
                    }
                }
                // A selection's offer: the clipboard is the platform's.
                wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
                _ => {}
            }
        }

        event_created_child!(State, WlDataDevice, [
            wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Formats::default()),
        ]);
    }

    impl Dispatch<WlDataOffer, Formats> for State {
        fn event(
            _: &mut Self,
            _: &WlDataOffer,
            event: wl_data_offer::Event,
            formats: &Formats,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_data_offer::Event::Offer { mime_type } = event
                && let Ok(mut list) = formats.0.lock()
                && !list.contains(&mime_type)
            {
                list.push(mime_type);
            }
        }
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

    impl Dispatch<WlSeat, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlSeat,
            _: wl_seat::Event,
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
        use super::chosen;

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

    use super::DragStep;

    /// Never made: there is no Wayland here.
    pub(crate) enum Answer {}

    impl Answer {
        pub(crate) fn taken(&self, _taken: bool) {
            match *self {}
        }

        pub(crate) fn dropped(self, _taken: bool) {
            match self {}
        }
    }

    pub(crate) enum Thread {}

    impl Thread {
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
