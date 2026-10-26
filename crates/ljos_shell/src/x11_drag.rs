//! Drags from other applications on X11. winit answers XDND itself, but
//! asks the source for files only and keeps the drag's position to
//! itself, so a dropped link or text never arrives and a dropped file
//! lands where `QmlView` guesses. The shell puts an `XdndProxy` on each of
//! its windows naming an unseen window of its own: sources that follow the
//! protocol then talk XDND to the shell, which places the drag by each
//! `XdndPosition` and reads every format the source offers.
//!
//! A thread waits on a connection of its own (it is that connection's
//! only reader), reads what a drag carries when it comes in, and posts
//! each step into the loop. The loop's answers go to the source on a
//! second connection. A source that ignores `XdndProxy` still talks to
//! winit, which takes its files as before.

use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;

use crate::drags::DragStep;

/// The X11 window of `window`; `None` off X11.
pub(crate) fn window_of(window: &Window) -> Option<usize> {
    imp::window_of(window)
}

/// Starts following drags when winit's display is an X11 one.
pub(crate) fn start(
    display: &OwnedDisplayHandle,
    post: Arc<dyn Fn(DragStep) + Send + Sync>,
) -> Option<Drags> {
    imp::start(display, post).map(Drags)
}

/// What follows drags; [`Drags::stop`] ends it.
pub(crate) struct Drags(imp::Follower);

impl Drags {
    /// Drags over `window` come to the shell from now on.
    pub(crate) fn window_opened(&self, window: &Window) {
        self.0.window_opened(window);
    }

    pub(crate) fn stop(self) {
        self.0.stop();
    }
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
mod imp {
    use std::collections::VecDeque;
    use std::os::fd::AsRawFd;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use egui_winit::winit::event_loop::OwnedDisplayHandle;
    use egui_winit::winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use egui_winit::winit::window::Window;
    use ljos_platform::{DragActions, DragData, DropAction, DropActions};
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask,
        PropMode, Property, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use crate::drags::{At, DragStep, Reply};

    x11rb::atom_manager! {
        pub(crate) Atoms: AtomsCookie {
            XdndAware,
            XdndProxy,
            XdndEnter,
            XdndPosition,
            XdndStatus,
            XdndLeave,
            XdndDrop,
            XdndFinished,
            XdndSelection,
            XdndTypeList,
            XdndActionCopy,
            XdndActionMove,
            XdndActionLink,
            XdndActionList,
            INCR,
            UTF8_STRING,
            LJOS_XDND_DATA,
            LJOS_XDND_STOP,
        }
    }

    /// The protocol's version the shell speaks: the one winit says too.
    const VERSION: u32 = 5;

    /// The most of one format that is read, as on Wayland.
    const MOST: usize = 8 << 20;

    /// How long a source has to give one format, or one piece of a large
    /// one. A source that never answers would otherwise hold every later
    /// drag up.
    const WRITE_WAIT: Duration = Duration::from_millis(500);

    pub(crate) fn window_of(window: &Window) -> Option<usize> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(h) => usize::try_from(h.window).ok(),
            RawWindowHandle::Xcb(h) => usize::try_from(h.window.get()).ok(),
            _ => None,
        }
    }

    pub(crate) struct Follower {
        /// The loop's connection: properties and answers.
        out: Arc<RustConnection>,
        atoms: Atoms,
        proxy: u32,
        thread: JoinHandle<()>,
    }

    impl Follower {
        pub(crate) fn window_opened(&self, window: &Window) {
            let Some(id) = window_of(window).and_then(|w| u32::try_from(w).ok()) else {
                return;
            };
            // An error is a gone server, which winit hears of too.
            let _ = self.out.change_property32(
                PropMode::REPLACE,
                id,
                self.atoms.XdndProxy,
                AtomEnum::WINDOW,
                &[self.proxy],
            );
            let _ = self.out.flush();
        }

        pub(crate) fn stop(self) {
            let stop = ClientMessageEvent::new(32, self.proxy, self.atoms.LJOS_XDND_STOP, [0; 5]);
            let sent = self
                .out
                .send_event(false, self.proxy, EventMask::NO_EVENT, stop)
                .is_ok()
                && self.out.flush().is_ok();
            // Without the server the thread's wait has ended already.
            if sent {
                let _ = self.thread.join();
            }
        }
    }

    pub(crate) fn start(
        display: &OwnedDisplayHandle,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Follower> {
        if !matches!(
            display.display_handle().ok()?.as_raw(),
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
        ) {
            return None;
        }
        // winit's display comes from DISPLAY, as these do.
        let (conn, screen) = x11rb::connect(None).ok()?;
        let (out, _) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
        let proxy = conn.generate_id().ok()?;
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            proxy,
            root,
            -10,
            -10,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            // The pieces of a large format come as property changes.
            &CreateWindowAux::new()
                .override_redirect(1)
                .event_mask(EventMask::PROPERTY_CHANGE),
        )
        .ok()?;
        // The proxy names itself, as XDND asks, so a source can tell it
        // from a stale property.
        conn.change_property32(
            PropMode::REPLACE,
            proxy,
            atoms.XdndAware,
            AtomEnum::ATOM,
            &[VERSION],
        )
        .ok()?;
        conn.change_property32(
            PropMode::REPLACE,
            proxy,
            atoms.XdndProxy,
            AtomEnum::WINDOW,
            &[proxy],
        )
        .ok()?;
        conn.flush().ok()?;
        let out = Arc::new(out);
        let thread = std::thread::Builder::new()
            .name("ljos-x11-drags".into())
            .spawn({
                let out = Arc::clone(&out);
                move || {
                    Reader {
                        conn,
                        out,
                        atoms,
                        root,
                        proxy,
                        post,
                        current: None,
                        pending: VecDeque::new(),
                    }
                    .run();
                }
            })
            .ok()?;
        Some(Follower {
            out,
            atoms,
            proxy,
            thread,
        })
    }

    /// A message of the protocol, as a source sends it.
    #[derive(Debug, PartialEq, Eq)]
    enum Xdnd {
        Enter {
            source: u32,
            /// The window it is over: the one with the proxy, not ours.
            target: u32,
            version: u32,
            /// The types are in the source's `XdndTypeList`.
            more: bool,
            types: Vec<Atom>,
        },
        Position {
            source: u32,
            root: (i16, i16),
            time: u32,
            action: Atom,
        },
        Leave {
            source: u32,
        },
        Drop {
            source: u32,
        },
        Stop,
    }

    fn parse(atoms: &Atoms, window: u32, kind: Atom, d: [u32; 5]) -> Option<Xdnd> {
        let source = d[0];
        Some(if kind == atoms.XdndEnter {
            Xdnd::Enter {
                source,
                target: window,
                version: d[1] >> 24,
                more: d[1] & 1 != 0,
                types: d[2..].iter().copied().filter(|&t| t != 0).collect(),
            }
        } else if kind == atoms.XdndPosition {
            // Each half is a coordinate on the root window.
            Xdnd::Position {
                source,
                root: ((d[2] >> 16) as u16 as i16, (d[2] & 0xffff) as u16 as i16),
                time: d[3],
                action: d[4],
            }
        } else if kind == atoms.XdndLeave {
            Xdnd::Leave { source }
        } else if kind == atoms.XdndDrop {
            Xdnd::Drop { source }
        } else if kind == atoms.LJOS_XDND_STOP {
            Xdnd::Stop
        } else {
            return None;
        })
    }

    fn action_of(atoms: &Atoms, atom: Atom) -> Option<DropAction> {
        [
            (atoms.XdndActionCopy, DropAction::Copy),
            (atoms.XdndActionMove, DropAction::Move),
            (atoms.XdndActionLink, DropAction::Link),
        ]
        .into_iter()
        .find(|(a, _)| *a == atom)
        .map(|(_, d)| d)
    }

    fn atom_of(atoms: &Atoms, action: DropAction) -> Atom {
        match action {
            DropAction::Copy => atoms.XdndActionCopy,
            DropAction::Move => atoms.XdndActionMove,
            DropAction::Link => atoms.XdndActionLink,
        }
    }

    /// What a source that asks for `requested` allows. XDND lets a target
    /// take the requested action or a copy; a source that lists more in
    /// `XdndActionList` (`listed`) allows those too. One it asks for that
    /// a DropArea cannot take (`ask`, `private`) proposes a copy.
    fn actions_of(atoms: &Atoms, requested: Atom, listed: &[Atom]) -> DragActions {
        let proposed = action_of(atoms, requested).unwrap_or(DropAction::Copy);
        let allowed: DropActions = [DropAction::Copy, proposed]
            .into_iter()
            .chain(listed.iter().filter_map(|&a| action_of(atoms, a)))
            .collect();
        DragActions { allowed, proposed }
    }

    /// `XdndStatus`: whether a drop would be taken and with which action,
    /// and that the source is to say every move, as a DropArea under it
    /// may change.
    fn status(target: u32, action: Option<Atom>) -> [u32; 5] {
        [
            target,
            u32::from(action.is_some()) | 2,
            0,
            0,
            action.unwrap_or(0),
        ]
    }

    /// `XdndFinished`: whether the drop was taken, and how.
    fn finished(target: u32, action: Option<Atom>) -> [u32; 5] {
        [
            target,
            u32::from(action.is_some()),
            action.unwrap_or(0),
            0,
            0,
        ]
    }

    /// Whether a type a source offers is one a DropArea can read: a MIME
    /// type, or X11's names of plain text.
    fn readable_format(name: &str) -> bool {
        name.contains('/') || matches!(name, "UTF8_STRING" | "STRING" | "TEXT")
    }

    /// The drag in progress.
    struct Current {
        source: u32,
        target: u32,
        version: u32,
        types: Vec<Atom>,
        /// What the source allows and asks for, as its last position said.
        actions: DragActions,
        /// The source's `XdndActionList`, read once.
        listed: Option<Vec<Atom>>,
        /// The loop was told it came in, at its first position.
        entered: bool,
    }

    struct Reader {
        conn: RustConnection,
        out: Arc<RustConnection>,
        atoms: Atoms,
        root: u32,
        proxy: u32,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
        current: Option<Current>,
        /// Events that came while a format was read.
        pending: VecDeque<Event>,
    }

    impl Reader {
        fn run(mut self) {
            loop {
                let event = match self.pending.pop_front() {
                    Some(event) => event,
                    None => match self.next(None) {
                        Some(event) => event,
                        // The server is gone.
                        None => return,
                    },
                };
                let Event::ClientMessage(m) = event else {
                    continue;
                };
                if m.format != 32 {
                    continue;
                }
                match parse(&self.atoms, m.window, m.type_, m.data.as_data32()) {
                    Some(Xdnd::Stop) => return,
                    Some(message) => self.heard(message),
                    None => {}
                }
            }
        }

        fn heard(&mut self, message: Xdnd) {
            match message {
                Xdnd::Enter {
                    source,
                    target,
                    version,
                    more,
                    types,
                } => {
                    let types = if more { self.type_list(source) } else { types };
                    if let Some(old) = self.current.take()
                        && old.entered
                    {
                        (self.post)(DragStep::Left);
                    }
                    self.current = Some(Current {
                        source,
                        target,
                        version,
                        types,
                        actions: DragActions::default(),
                        listed: None,
                        entered: false,
                    });
                }
                Xdnd::Position {
                    source,
                    root,
                    time,
                    action,
                } => {
                    let Some(target) = self.current_of(source).map(|c| c.target) else {
                        return;
                    };
                    let at = self.translate(target, root);
                    if self.current.as_ref().is_some_and(|c| c.listed.is_none()) {
                        let listed = self.action_list(source);
                        self.current.as_mut().expect("the drag is there").listed = Some(listed);
                    }
                    let current = self.current.as_mut().expect("the drag is there");
                    let actions = actions_of(
                        &self.atoms,
                        action,
                        current.listed.as_deref().unwrap_or(&[]),
                    );
                    current.actions = actions;
                    let answer = self.answer();
                    let current = self.current.as_mut().expect("the drag is there");
                    if std::mem::replace(&mut current.entered, true) {
                        (self.post)(DragStep::Moved {
                            at,
                            actions: Some(actions),
                            answer,
                        });
                    } else {
                        let types = current.types.clone();
                        let data = self.read_all(&types, time);
                        (self.post)(DragStep::Entered {
                            target: target as usize,
                            at,
                            data,
                            actions,
                            answer,
                        });
                    }
                }
                Xdnd::Leave { source } => {
                    if self.current_of(source).is_some()
                        && let Some(old) = self.current.take()
                        && old.entered
                    {
                        (self.post)(DragStep::Left);
                    }
                }
                Xdnd::Drop { source } => {
                    if self.current_of(source).is_none() {
                        return;
                    }
                    let answer = self.answer();
                    match self.current.take() {
                        Some(c) if c.entered => (self.post)(DragStep::Dropped { answer }),
                        // Dropped before it was anywhere: nothing took it.
                        _ => answer.dropped(None),
                    }
                }
                Xdnd::Stop => {}
            }
        }

        fn current_of(&self, source: u32) -> Option<&Current> {
            self.current.as_ref().filter(|c| c.source == source)
        }

        fn answer(&self) -> Box<Answer> {
            let c = self.current.as_ref().expect("the drag is there");
            Box::new(Answer {
                out: Arc::clone(&self.out),
                atoms: self.atoms,
                source: c.source,
                target: c.target,
                version: c.version,
            })
        }

        /// The actions a source lists beside the one it asks for; most
        /// list none.
        fn action_list(&self, source: u32) -> Vec<Atom> {
            self.conn
                .get_property(
                    false,
                    source,
                    self.atoms.XdndActionList,
                    AtomEnum::ATOM,
                    0,
                    16,
                )
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(Iterator::collect))
                .unwrap_or_default()
        }

        /// The root's point in the target window's pixels.
        fn translate(&self, target: u32, (x, y): (i16, i16)) -> At {
            let there = self
                .conn
                .translate_coordinates(self.root, target, x, y)
                .ok()
                .and_then(|c| c.reply().ok());
            match there {
                Some(t) => At::Physical(f64::from(t.dst_x), f64::from(t.dst_y)),
                // A window gone meanwhile; the loop finds no view for it.
                None => At::Physical(f64::from(x), f64::from(y)),
            }
        }

        fn type_list(&self, source: u32) -> Vec<Atom> {
            self.conn
                .get_property(
                    false,
                    source,
                    self.atoms.XdndTypeList,
                    AtomEnum::ATOM,
                    0,
                    1024,
                )
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(Iterator::collect))
                .unwrap_or_default()
        }

        fn read_all(&mut self, types: &[Atom], time: u32) -> DragData {
            let named: Vec<(String, Atom)> = types
                .iter()
                .filter_map(|&t| {
                    let name = self.conn.get_atom_name(t).ok()?.reply().ok()?.name;
                    let name = String::from_utf8(name).ok()?;
                    readable_format(&name).then_some((name, t))
                })
                .collect();
            let mut formats = Vec::new();
            for (name, t) in named {
                if let Some(bytes) = self.read(t, time) {
                    formats.push((name, bytes));
                }
            }
            DragData::from_formats(formats)
        }

        /// One format of the drag's selection, or `None` when the source
        /// gave none, too much, or not in time.
        fn read(&mut self, format: Atom, time: u32) -> Option<Vec<u8>> {
            let a = self.atoms;
            self.conn
                .convert_selection(self.proxy, a.XdndSelection, format, a.LJOS_XDND_DATA, time)
                .ok()?;
            self.conn.flush().ok()?;
            loop {
                match self.next(Some(Instant::now() + WRITE_WAIT))? {
                    Event::SelectionNotify(n)
                        if n.requestor == self.proxy && n.selection == a.XdndSelection =>
                    {
                        if n.property == x11rb::NONE || n.target != format {
                            return None;
                        }
                        return self.take_property();
                    }
                    other => self.pending.push_back(other),
                }
            }
        }

        /// Reads and deletes what the source wrote, in pieces when it is
        /// large (`INCR`).
        fn take_property(&mut self) -> Option<Vec<u8>> {
            let whole = self.get_data()?;
            if whole.type_ != self.atoms.INCR {
                return (whole.bytes_after == 0).then_some(whole.value);
            }
            let mut out = Vec::new();
            loop {
                match self.next(Some(Instant::now() + WRITE_WAIT))? {
                    Event::PropertyNotify(p)
                        if p.window == self.proxy
                            && p.atom == self.atoms.LJOS_XDND_DATA
                            && p.state == Property::NEW_VALUE =>
                    {
                        let piece = self.get_data()?;
                        if piece.value.is_empty() {
                            return Some(out);
                        }
                        if out.len() + piece.value.len() > MOST {
                            return None;
                        }
                        out.extend_from_slice(&piece.value);
                    }
                    Event::PropertyNotify(_) => {}
                    other => self.pending.push_back(other),
                }
            }
        }

        fn get_data(&self) -> Option<x11rb::protocol::xproto::GetPropertyReply> {
            let longs = u32::try_from(MOST / 4).unwrap_or(u32::MAX);
            self.conn
                .get_property(
                    true,
                    self.proxy,
                    self.atoms.LJOS_XDND_DATA,
                    AtomEnum::ANY,
                    0,
                    longs,
                )
                .ok()?
                .reply()
                .ok()
        }

        /// The connection's next event, waiting until `until` (or for
        /// ever); `None` when the wait ran out or the server went.
        fn next(&mut self, until: Option<Instant>) -> Option<Event> {
            loop {
                match self.conn.poll_for_event() {
                    Ok(Some(event)) => return Some(event),
                    Ok(None) => {}
                    Err(_) => return None,
                }
                let wait = until.map(|t| t.saturating_duration_since(Instant::now()));
                if wait.is_some_and(|w| w.is_zero()) {
                    return None;
                }
                if !readable(&self.conn, wait) && wait.is_some() {
                    return None;
                }
            }
        }
    }

    /// Whether the connection has something to read before `wait` runs out
    /// (`None`: however long that takes).
    fn readable(conn: &RustConnection, wait: Option<Duration>) -> bool {
        let mut poll = libc::pollfd {
            fd: conn.stream().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = wait.map_or(-1, |w| {
            libc::c_int::try_from(w.as_millis()).unwrap_or(libc::c_int::MAX)
        });
        // SAFETY: one pollfd, which lives across the call.
        let ready = unsafe { libc::poll(&raw mut poll, 1, ms) };
        ready > 0
    }

    /// Where the loop's answer to one step goes: the source, on the loop's
    /// connection.
    pub(crate) struct Answer {
        out: Arc<RustConnection>,
        atoms: Atoms,
        source: u32,
        target: u32,
        version: u32,
    }

    impl Answer {
        fn send(&self, kind: Atom, data: [u32; 5]) {
            let message = ClientMessageEvent::new(32, self.source, kind, data);
            // An error is a gone source or server; the drag is over then.
            let _ = self
                .out
                .send_event(false, self.source, EventMask::NO_EVENT, message);
            let _ = self.out.flush();
        }
    }

    impl Reply for Answer {
        fn taken(&self, action: Option<DropAction>) {
            let action = action.map(|a| atom_of(&self.atoms, a));
            self.send(self.atoms.XdndStatus, status(self.target, action));
        }

        fn dropped(self: Box<Self>, action: Option<DropAction>) {
            // Version 1 has no XdndFinished.
            if self.version >= 2 {
                let action = action.map(|a| atom_of(&self.atoms, a));
                self.send(self.atoms.XdndFinished, finished(self.target, action));
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn atoms() -> Atoms {
            Atoms {
                XdndAware: 1,
                XdndProxy: 2,
                XdndEnter: 3,
                XdndPosition: 4,
                XdndStatus: 5,
                XdndLeave: 6,
                XdndDrop: 7,
                XdndFinished: 8,
                XdndSelection: 9,
                XdndTypeList: 10,
                XdndActionCopy: 11,
                XdndActionMove: 12,
                XdndActionLink: 13,
                XdndActionList: 18,
                INCR: 14,
                UTF8_STRING: 15,
                LJOS_XDND_DATA: 16,
                LJOS_XDND_STOP: 17,
            }
        }

        #[test]
        fn a_source_says_its_types_and_version_as_it_enters() {
            let a = atoms();
            assert_eq!(
                parse(&a, 77, a.XdndEnter, [500, 5 << 24, 40, 41, 0]),
                Some(Xdnd::Enter {
                    source: 500,
                    target: 77,
                    version: 5,
                    more: false,
                    types: vec![40, 41],
                })
            );
            assert_eq!(
                parse(&a, 77, a.XdndEnter, [500, (5 << 24) | 1, 40, 41, 42]),
                Some(Xdnd::Enter {
                    source: 500,
                    target: 77,
                    version: 5,
                    more: true,
                    types: vec![40, 41, 42],
                })
            );
        }

        /// Issue #90: the place a drag is over comes with every step.
        #[test]
        fn a_position_is_the_point_on_the_root_window_the_drag_is_over() {
            let a = atoms();
            assert_eq!(
                parse(&a, 77, a.XdndPosition, [500, 0, (250 << 16) | 350, 9, 12]),
                Some(Xdnd::Position {
                    source: 500,
                    root: (250, 350),
                    time: 9,
                    action: 12,
                })
            );
            assert_eq!(
                parse(&a, 77, a.XdndDrop, [500, 0, 9, 0, 0]),
                Some(Xdnd::Drop { source: 500 })
            );
            assert_eq!(
                parse(&a, 77, a.XdndLeave, [500, 0, 0, 0, 0]),
                Some(Xdnd::Leave { source: 500 })
            );
            assert_eq!(parse(&a, 77, 99, [0; 5]), None);
        }

        #[test]
        fn the_source_hears_whether_a_drop_is_taken_and_how() {
            let a = atoms();
            assert_eq!(status(77, Some(a.XdndActionMove)), [77, 3, 0, 0, 12]);
            assert_eq!(status(77, None), [77, 2, 0, 0, 0]);
            assert_eq!(finished(77, Some(a.XdndActionCopy)), [77, 1, 11, 0, 0]);
            assert_eq!(finished(77, None), [77, 0, 0, 0, 0]);
        }

        /// Issue #115: a target may take what the source asks for, or a
        /// copy, or what it lists.
        #[test]
        fn a_source_allows_what_it_asks_for_a_copy_and_what_it_lists() {
            let a = atoms();
            let moved = actions_of(&a, a.XdndActionMove, &[]);
            assert_eq!(moved.proposed, DropAction::Move);
            assert_eq!(moved.allowed.qt(), 1 | 2);
            let asked = actions_of(&a, 99, &[a.XdndActionLink, 99]);
            assert_eq!(asked.proposed, DropAction::Copy);
            assert_eq!(asked.allowed.qt(), 1 | 4);
        }

        /// Issue #115: every MIME type is read, and X11's names of text.
        #[test]
        fn mime_types_and_the_names_of_text_are_read() {
            for name in [
                "text/uri-list",
                "text/html",
                "application/x-a",
                "UTF8_STRING",
            ] {
                assert!(readable_format(name), "{name}");
            }
            for name in ["TARGETS", "TIMESTAMP", "MULTIPLE", "_NETSCAPE_URL"] {
                assert!(!readable_format(name), "{name}");
            }
        }
    }
}

#[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
mod imp {
    use std::sync::Arc;

    use egui_winit::winit::event_loop::OwnedDisplayHandle;
    use egui_winit::winit::window::Window;

    use crate::drags::DragStep;

    pub(crate) enum Follower {}

    impl Follower {
        pub(crate) fn window_opened(&self, _window: &Window) {
            match *self {}
        }

        pub(crate) fn stop(self) {
            match self {}
        }
    }

    pub(crate) fn window_of(_window: &Window) -> Option<usize> {
        None
    }

    pub(crate) fn start(
        _display: &OwnedDisplayHandle,
        _post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Follower> {
        None
    }
}
