//! Drags on X11. winit answers XDND itself, but asks the source for
//! files only and keeps the drag's position to itself, so a dropped link
//! or text never arrives and a dropped file lands where `QmlView` guesses.
//! The shell puts an `XdndProxy` on each of its windows naming an unseen
//! window of its own: sources that follow the protocol then talk XDND to
//! the shell, which places the drag by each `XdndPosition` and reads every
//! format the source offers.
//!
//! winit does none of XDND's source side, so a drag out of the
//! application is the shell's too, with the unseen window as its source:
//! it owns `XdndSelection`, grabs the pointer (winit lets go of the one
//! its press grabbed), finds the window under it that takes drags, talks
//! XDND to it, and gives it each format it asks for (in pieces when it is
//! large). A drag back over the application's own window comes to the
//! unseen window by its `XdndProxy`, as any other.
//!
//! A thread waits on a connection of its own (it is that connection's
//! only reader), reads what a drag carries when it comes in, carries a
//! drag out, and posts each step into the loop. The loop's answers go to
//! the source on a second connection. A source that ignores `XdndProxy`
//! still talks to winit, which takes its files as before.

use std::sync::Arc;

use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_platform::{DragActions, DragData, DropAction};

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

    /// Carries a drag out of the application from the press held now.
    /// `false` when the pointer could not be had, and the drag stays in
    /// the window.
    pub(crate) fn start_out(&mut self, data: &DragData, actions: DragActions) -> bool {
        self.0.start_out(data, actions)
    }

    /// Gives the drag out of the application up.
    pub(crate) fn cancel_out(&mut self) {
        self.0.cancel_out();
    }

    /// How the drag out of the application ended, once it has: the action
    /// the target took, or `None`. [`DragStep::OutEnded`] says it has.
    pub(crate) fn out_ended(&mut self) -> Option<Option<DropAction>> {
        self.0.out_ended()
    }

    pub(crate) fn stop(self) {
        self.0.stop();
    }
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
mod imp {
    use std::collections::VecDeque;
    use std::ffi::{c_int, c_ulong, c_void};
    use std::os::fd::AsRawFd;
    use std::ptr::NonNull;
    use std::sync::{Arc, Mutex, mpsc};
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
        Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _,
        CreateWindowAux, EventMask, GrabMode, GrabStatus, Keycode, Keysym, PropMode, Property,
        SelectionNotifyEvent, SelectionRequestEvent, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use crate::drags::{At, DragStep, Reply};
    use crate::lock;

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
            TARGETS,
            LJOS_XDND_DATA,
            LJOS_XDND_STOP,
            LJOS_XDND_OUT,
            LJOS_XDND_CANCEL,
        }
    }

    /// The protocol's version the shell speaks: the one winit says too.
    const VERSION: u32 = 5;

    /// The oldest version a drag out talks to: the first with
    /// `XdndTypeList` and actions.
    const OLDEST: u32 = 3;

    /// The most of one format that is read, as on Wayland.
    const MOST: usize = 8 << 20;

    /// How long a source has to give one format, or one piece of a large
    /// one. A source that never answers would otherwise hold every later
    /// drag up.
    const WRITE_WAIT: Duration = Duration::from_millis(500);

    /// A format larger than this goes to a target in pieces of it
    /// (`INCR`), as ICCCM asks of one larger than a request may be; the
    /// shell keeps every request small.
    const PIECE: usize = 64 << 10;

    /// How long a target has to answer the last position once the drag is
    /// let go of: until then it is not known whether it takes the drop.
    const STATUS_WAIT: Duration = Duration::from_secs(1);

    /// How long a target has to say it finished with a drop, from the drop
    /// or the last format or piece it asked for. One that never says ends
    /// the drag with no action.
    const FINISH_WAIT: Duration = Duration::from_secs(5);

    /// The keysym of Escape, which ends a drag out with no action.
    const ESCAPE: Keysym = 0xff1b;

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
        /// The thread's connection, on which a drag out grabs the pointer
        /// so that the thread hears it.
        conn: Arc<RustConnection>,
        atoms: Atoms,
        root: u32,
        proxy: u32,
        winit: Option<WinitGrab>,
        outs: Arc<Mutex<Outs>>,
        /// The drag out the loop handed over last.
        number: u32,
        thread: JoinHandle<()>,
    }

    /// What the loop and the thread both see of drags out.
    #[derive(Default)]
    struct Outs {
        /// Handed over, not yet heard of by the thread.
        handed: VecDeque<Handed>,
        /// How the drag out numbered so ended.
        ended: Option<(u32, Option<DropAction>)>,
    }

    struct Handed {
        number: u32,
        formats: Vec<(String, Vec<u8>)>,
        actions: DragActions,
    }

    impl Follower {
        pub(crate) fn start_out(&mut self, data: &DragData, actions: DragActions) -> bool {
            // The press winit heard grabbed the pointer for winit; the
            // server gives it to nobody else until winit lets go.
            if let Some(winit) = &self.winit {
                winit.let_go();
            }
            let grabbed = self
                .conn
                .grab_pointer(
                    false,
                    self.root,
                    EventMask::POINTER_MOTION | EventMask::BUTTON_RELEASE,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                )
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.status == GrabStatus::SUCCESS);
            if !grabbed {
                return false;
            }
            self.number = self.number.wrapping_add(1);
            {
                let mut outs = lock(&self.outs);
                outs.ended = None;
                outs.handed.push_back(Handed {
                    number: self.number,
                    formats: offered(data),
                    actions,
                });
            }
            // Sent on the thread's connection, last: what the thread hears
            // of the grab comes before it, and nothing after it is read
            // here, so the thread's wait sees it.
            self.tell(self.atoms.LJOS_XDND_OUT);
            true
        }

        pub(crate) fn cancel_out(&mut self) {
            self.tell(self.atoms.LJOS_XDND_CANCEL);
            // How the given-up drag ends is no one's to hear.
            self.number = self.number.wrapping_add(1);
            lock(&self.outs).ended = None;
        }

        pub(crate) fn out_ended(&mut self) -> Option<Option<DropAction>> {
            let mut outs = lock(&self.outs);
            match outs.ended {
                Some((number, action)) if number == self.number => {
                    outs.ended = None;
                    Some(action)
                }
                _ => None,
            }
        }

        /// Tells the thread of drag out `self.number`.
        fn tell(&self, kind: Atom) {
            let message = ClientMessageEvent::new(32, self.proxy, kind, [self.number, 0, 0, 0, 0]);
            // An error is a gone server, which winit hears of too.
            let _ = self
                .conn
                .send_event(false, self.proxy, EventMask::NO_EVENT, message);
            let _ = self.conn.flush();
        }

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
            let send = |conn: &RustConnection| {
                conn.send_event(false, self.proxy, EventMask::NO_EVENT, stop)
                    .is_ok()
                    && conn.flush().is_ok()
            };
            // The loop's connection may be gone while the thread's is not:
            // then the thread's own tells it. When neither can, the
            // thread's connection is gone, and its wait has ended with it;
            // so the join always returns.
            let _ = send(&self.out) || send(&self.conn);
            let _ = self.thread.join();
        }
    }

    pub(crate) fn start(
        display: &OwnedDisplayHandle,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ) -> Option<Follower> {
        let handle = display.display_handle().ok()?.as_raw();
        if !matches!(handle, RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) {
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
        let conn = Arc::new(conn);
        let outs = Arc::new(Mutex::new(Outs::default()));
        let thread = std::thread::Builder::new()
            .name("ljos-x11-drags".into())
            .spawn({
                let out = Arc::clone(&out);
                let conn = Arc::clone(&conn);
                let outs = Arc::clone(&outs);
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
                        outs,
                        going: None,
                        pieces: Vec::new(),
                    }
                    .run();
                }
            })
            .ok()?;
        Some(Follower {
            out,
            conn,
            atoms,
            root,
            proxy,
            winit: WinitGrab::of(handle),
            outs,
            number: 0,
            thread,
        })
    }

    /// winit's own connection, for the one thing the shell does on it:
    /// letting go of the pointer a press on a window grabbed for winit, so
    /// that a drag out can have it.
    struct WinitGrab {
        display: NonNull<c_void>,
        ungrab_pointer: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
        sync: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    }

    impl WinitGrab {
        fn of(handle: RawDisplayHandle) -> Option<Self> {
            let RawDisplayHandle::Xlib(h) = handle else {
                return None;
            };
            let display = h.display?;
            // SAFETY: Xlib is loaded already, as winit opened its display
            // with it; the symbols are Xlib's documented functions, of
            // these types.
            unsafe {
                let lib =
                    libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);
                if lib.is_null() {
                    return None;
                }
                let ungrab = libc::dlsym(lib, c"XUngrabPointer".as_ptr());
                let sync = libc::dlsym(lib, c"XSync".as_ptr());
                // RTLD_NOLOAD still counted a reference. Given back at
                // once: winit's keeps the library, and the symbols, loaded
                // for as long as its display is open.
                libc::dlclose(lib);
                if ungrab.is_null() || sync.is_null() {
                    return None;
                }
                Some(Self {
                    display,
                    ungrab_pointer: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
                    >(ungrab),
                    sync: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
                    >(sync),
                })
            }
        }

        /// Lets go of winit's grab, and waits until the server has: the
        /// grab that follows is on another connection.
        fn let_go(&self) {
            // SAFETY: the display is winit's, open while its loop runs,
            // and this is the loop's thread.
            unsafe {
                (self.ungrab_pointer)(self.display.as_ptr(), 0);
                (self.sync)(self.display.as_ptr(), 0);
            }
        }
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
        /// A target, of a drag out: whether it takes the drop, with which
        /// action, and where it needs no more positions.
        Status {
            target: u32,
            taken: Option<Atom>,
            quiet: Option<Quiet>,
        },
        /// A target is done with a drop out: whether it took it, and how
        /// (from version 5).
        Finished {
            target: u32,
            taken: bool,
            action: Atom,
        },
        /// The loop handed drag out `number` over, or gave it up.
        Out {
            number: u32,
        },
        Cancel {
            number: u32,
        },
        Stop,
    }

    /// A rectangle on the root window inside which a target needs no more
    /// positions: what it would say there stays as it is.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Quiet {
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    }

    impl Quiet {
        fn holds(self, (x, y): (i16, i16)) -> bool {
            let (x, y) = (i32::from(x), i32::from(y));
            let (left, top) = (i32::from(self.x), i32::from(self.y));
            x >= left
                && y >= top
                && x < left + i32::from(self.width)
                && y < top + i32::from(self.height)
        }
    }

    /// A point on the root window, as one half of a word each.
    fn halves(d: u32) -> (i16, i16) {
        ((d >> 16) as u16 as i16, (d & 0xffff) as u16 as i16)
    }

    fn word((x, y): (i16, i16)) -> u32 {
        (u32::from(x as u16) << 16) | u32::from(y as u16)
    }

    fn parse(atoms: &Atoms, window: u32, kind: Atom, d: [u32; 5]) -> Option<Xdnd> {
        let source = d[0];
        if kind == atoms.XdndStatus {
            let (width, height) = halves(d[3]);
            let (x, y) = halves(d[2]);
            return Some(Xdnd::Status {
                target: d[0],
                taken: (d[1] & 1 != 0).then_some(d[4]),
                // Bit 1 asks for every position anyway.
                quiet: (d[1] & 2 == 0).then_some(Quiet {
                    x,
                    y,
                    width: width as u16,
                    height: height as u16,
                }),
            });
        }
        if kind == atoms.XdndFinished {
            return Some(Xdnd::Finished {
                target: d[0],
                taken: d[1] & 1 != 0,
                action: d[2],
            });
        }
        if kind == atoms.LJOS_XDND_OUT {
            return Some(Xdnd::Out { number: d[0] });
        }
        if kind == atoms.LJOS_XDND_CANCEL {
            return Some(Xdnd::Cancel { number: d[0] });
        }
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
                root: halves(d[2]),
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

    /// `XdndEnter`, of a drag out: the version both sides speak, and the
    /// first three types; bit 0 says the rest are in `XdndTypeList`.
    fn enter(source: u32, version: u32, types: &[Atom]) -> [u32; 5] {
        let first = |i: usize| types.get(i).copied().unwrap_or(0);
        [
            source,
            (version << 24) | u32::from(types.len() > 3),
            first(0),
            first(1),
            first(2),
        ]
    }

    /// `XdndPosition`: the point on the root window, and the action the
    /// drag would like.
    fn position(source: u32, at: (i16, i16), time: u32, action: Atom) -> [u32; 5] {
        [source, 0, word(at), time, action]
    }

    fn leave(source: u32) -> [u32; 5] {
        [source, 0, 0, 0, 0]
    }

    /// `XdndDrop`: the time the target converts the selection at.
    fn drop_message(source: u32, time: u32) -> [u32; 5] {
        [source, 0, time, 0, 0]
    }

    /// The action a drop out was taken with: from version 5, what
    /// `XdndFinished` says; before it, what the last `XdndStatus` said.
    fn finished_action(
        atoms: &Atoms,
        version: u32,
        status: Option<Atom>,
        taken: bool,
        action: Atom,
    ) -> Option<DropAction> {
        let of = |a: Atom| action_of(atoms, a).unwrap_or(DropAction::Copy);
        if version < 5 {
            return status.map(of);
        }
        taken.then(|| {
            action_of(atoms, action).unwrap_or_else(|| status.map_or(DropAction::Copy, of))
        })
    }

    /// What a drag out offers, by name: each MIME type, and plain text by
    /// X11's name for UTF-8 too, which older targets ask for.
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
                out.push(("UTF8_STRING".to_owned(), text.clone().into_bytes()));
            }
            out.push((format, text.into_bytes()));
        }
        out
    }

    /// `XdndActionList`, and the action a drag out proposes: one it allows.
    fn out_actions(atoms: &Atoms, actions: DragActions) -> (Vec<Atom>, Atom) {
        let allowed = if actions.allowed.is_empty() {
            DropActions::from(DropAction::Copy)
        } else {
            actions.allowed
        };
        let list: Vec<Atom> = allowed.iter().map(|a| atom_of(atoms, a)).collect();
        let proposed = if allowed.contains(actions.proposed) {
            atom_of(atoms, actions.proposed)
        } else {
            list[0]
        };
        (list, proposed)
    }

    /// The keys whose keysyms include `keysym`, from a keyboard mapping
    /// that starts at `first`.
    fn keycodes_of(first: Keycode, per: u8, keysyms: &[Keysym], keysym: Keysym) -> Vec<Keycode> {
        keysyms
            .chunks(usize::from(per.max(1)))
            .enumerate()
            .filter(|(_, syms)| syms.contains(&keysym))
            .filter_map(|(i, _)| u8::try_from(usize::from(first) + i).ok())
            .collect()
    }

    /// The next piece of a large format, after `sent` bytes; empty once
    /// it is all sent, which ends the transfer.
    fn piece(bytes: &[u8], sent: usize) -> &[u8] {
        let start = sent.min(bytes.len());
        &bytes[start..(start + PIECE).min(bytes.len())]
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

    /// The drag out under way, with the unseen window as its source.
    struct Going {
        number: u32,
        formats: Vec<(Atom, Arc<[u8]>)>,
        types: Vec<Atom>,
        proposed: Atom,
        escape: Vec<Keycode>,
        /// The window under the pointer that takes drags.
        over: Option<Over>,
        phase: Phase,
        /// When the target has waited too long to answer.
        deadline: Option<Instant>,
        grabbed: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Phase {
        Dragging,
        /// Let go of before the target answered the last position.
        Released {
            time: u32,
        },
        /// Dropped; the target has not said it finished.
        Dropped,
    }

    /// A window a drag out is over that takes drags.
    struct Over {
        window: u32,
        /// Where its messages go: the window, or its `XdndProxy`.
        to: u32,
        version: u32,
        /// A position it has not answered yet: XDND sends the next only
        /// after the answer.
        waiting: bool,
        next: Option<((i16, i16), u32)>,
        /// The action it would take the drop with, as it last said.
        taken: Option<Atom>,
        quiet: Option<Quiet>,
    }

    /// A large format a target is taking in pieces.
    struct Piecewise {
        requestor: u32,
        property: Atom,
        kind: Atom,
        bytes: Arc<[u8]>,
        sent: usize,
    }

    struct Reader {
        conn: Arc<RustConnection>,
        out: Arc<RustConnection>,
        atoms: Atoms,
        root: u32,
        proxy: u32,
        post: Arc<dyn Fn(DragStep) + Send + Sync>,
        current: Option<Current>,
        /// Events that came while a format was read.
        pending: VecDeque<Event>,
        outs: Arc<Mutex<Outs>>,
        going: Option<Going>,
        pieces: Vec<Piecewise>,
    }

    impl Reader {
        fn run(mut self) {
            loop {
                let deadline = self.going.as_ref().and_then(|g| g.deadline);
                let event = match self.pending.pop_front() {
                    Some(event) => event,
                    None => match self.next(deadline) {
                        Some(event) => event,
                        None if deadline.is_some_and(|d| Instant::now() >= d) => {
                            self.ran_out();
                            continue;
                        }
                        // The server is gone.
                        None => return,
                    },
                };
                match event {
                    Event::ClientMessage(m) if m.format == 32 => {
                        match parse(&self.atoms, m.window, m.type_, m.data.as_data32()) {
                            Some(Xdnd::Stop) => return,
                            Some(message) => self.heard(message),
                            None => {}
                        }
                    }
                    Event::MotionNotify(m) => {
                        let (at, time) = self.last_motion((m.root_x, m.root_y), m.time);
                        self.moved(at, time);
                    }
                    Event::ButtonRelease(b) => self.released(b.time),
                    Event::KeyPress(k) => self.pressed(k.detail),
                    Event::SelectionRequest(r) => self.requested(&r),
                    Event::PropertyNotify(p) if p.state == Property::DELETE => {
                        self.piece_taken(p.window, p.atom);
                    }
                    _ => {}
                }
            }
        }

        /// The last of the motions that came together: a drag needs only
        /// where the pointer is now.
        fn last_motion(&mut self, mut at: (i16, i16), mut time: u32) -> ((i16, i16), u32) {
            while let Ok(Some(event)) = self.conn.poll_for_event() {
                if let Event::MotionNotify(m) = event {
                    (at, time) = ((m.root_x, m.root_y), m.time);
                } else {
                    self.pending.push_back(event);
                    break;
                }
            }
            (at, time)
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
                        let target = target as usize;
                        // The application's own drag out, back over its
                        // window: this thread is the source, and gives
                        // what it would give.
                        if let Some(going) = self.going.as_ref().filter(|_| source == self.proxy) {
                            let data = self.own(&going.formats, &types);
                            (self.post)(DragStep::Entered {
                                target,
                                at,
                                data,
                                actions,
                                answer,
                            });
                            return;
                        }
                        // The source hears an answer before its data is
                        // asked for: some (Qt's) give a target that has
                        // not answered in time no data and leave it.
                        let named = self.readable_types(&types);
                        let (answered, heard) = mpsc::sync_channel(1);
                        (self.post)(DragStep::Offered {
                            target,
                            at,
                            formats: DragData::formats_of(named.iter().map(|(name, _)| name)),
                            actions,
                            answer,
                            answered: Box::new(move || {
                                let _ = answered.send(());
                            }),
                        });
                        // A loop that never answers is gone or stuck; the
                        // data is read all the same.
                        let _ = heard.recv_timeout(WRITE_WAIT);
                        let data = self.read_all(named, time);
                        (self.post)(DragStep::Entered {
                            target,
                            at,
                            data,
                            actions,
                            answer: self.answer(),
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
                Xdnd::Status {
                    target,
                    taken,
                    quiet,
                } => self.status(target, taken, quiet),
                Xdnd::Finished {
                    target,
                    taken,
                    action,
                } => self.target_finished(target, taken, action),
                Xdnd::Out { number } => self.handed(number),
                Xdnd::Cancel { number } => {
                    if let Some(going) = self.going.take_if(|g| g.number == number) {
                        self.give_up(going);
                    }
                }
                Xdnd::Stop => {}
            }
        }

        /// What the application's own drag out offers, by name.
        fn own(&self, formats: &[(Atom, Arc<[u8]>)], types: &[Atom]) -> DragData {
            let named = types.iter().filter_map(|t| {
                let bytes = &formats.iter().find(|(a, _)| a == t)?.1;
                let name = self.conn.get_atom_name(*t).ok()?.reply().ok()?.name;
                Some((String::from_utf8(name).ok()?, Arc::clone(bytes)))
            });
            DragData::from_formats(named.collect::<Vec<_>>())
        }

        /// The loop handed drag out `number` over, with the pointer
        /// grabbed for it.
        fn handed(&mut self, number: u32) {
            let handed = {
                let mut outs = lock(&self.outs);
                let at = outs.handed.iter().position(|h| h.number == number);
                // Earlier ones were given up before they were heard of.
                at.map(|at| outs.handed.drain(..=at).next_back().expect("one is there"))
            };
            let Some(handed) = handed else {
                return;
            };
            if let Some(old) = self.going.take() {
                self.give_up(old);
            }
            let a = self.atoms;
            let formats: Vec<(Atom, Arc<[u8]>)> = handed
                .formats
                .into_iter()
                .filter_map(|(name, bytes)| {
                    let atom = self.conn.intern_atom(false, name.as_bytes()).ok()?;
                    Some((atom.reply().ok()?.atom, Arc::from(bytes)))
                })
                .collect();
            let types: Vec<Atom> = formats.iter().map(|(a, _)| *a).collect();
            let (listed, proposed) = out_actions(&a, handed.actions);
            let p = self.proxy;
            let _ = self.conn.change_property32(
                PropMode::REPLACE,
                p,
                a.XdndTypeList,
                AtomEnum::ATOM,
                &types,
            );
            let _ = self.conn.change_property32(
                PropMode::REPLACE,
                p,
                a.XdndActionList,
                AtomEnum::ATOM,
                &listed,
            );
            let _ = self
                .conn
                .set_selection_owner(p, a.XdndSelection, x11rb::CURRENT_TIME);
            let escape = self.escape_keys();
            // Without the keyboard the drag still ends where it is let go.
            let _ = self.conn.grab_keyboard(
                false,
                self.root,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
            self.pieces.clear();
            self.going = Some(Going {
                number,
                formats,
                types,
                proposed,
                escape,
                over: None,
                phase: Phase::Dragging,
                deadline: None,
                grabbed: true,
            });
            // The drag is somewhere before the pointer next moves.
            if let Some(p) = self
                .conn
                .query_pointer(self.root)
                .ok()
                .and_then(|c| c.reply().ok())
            {
                self.moved((p.root_x, p.root_y), x11rb::CURRENT_TIME);
            }
            let _ = self.conn.flush();
        }

        fn escape_keys(&self) -> Vec<Keycode> {
            let setup = self.conn.setup();
            let (first, last) = (setup.min_keycode, setup.max_keycode);
            self.conn
                .get_keyboard_mapping(first, last.saturating_sub(first).saturating_add(1))
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|m| keycodes_of(first, m.keysyms_per_keycode, &m.keysyms, ESCAPE))
                .unwrap_or_default()
        }

        /// Sends `data` to window `to`, of the drag `window` names.
        fn send(&self, to: u32, window: u32, kind: Atom, data: [u32; 5]) {
            let message = ClientMessageEvent::new(32, window, kind, data);
            // An error is a gone target or server; the drag ends without
            // its answer, by the deadline or where it is let go.
            let _ = self
                .conn
                .send_event(false, to, EventMask::NO_EVENT, message);
        }

        /// The drag out is over `at` on the root window.
        fn moved(&mut self, at: (i16, i16), time: u32) {
            let Some(mut going) = self.going.take() else {
                return;
            };
            if going.phase != Phase::Dragging
                || going
                    .over
                    .as_ref()
                    .and_then(|o| o.quiet)
                    .is_some_and(|q| q.holds(at))
            {
                self.going = Some(going);
                return;
            }
            let found = self.taker_at(at);
            if going.over.as_ref().map(|o| o.window) != found.map(|f| f.0) {
                if let Some(old) = going.over.take() {
                    self.send(old.to, old.window, self.atoms.XdndLeave, leave(self.proxy));
                }
                if let Some((window, to, version)) = found {
                    let message = enter(self.proxy, version, &going.types);
                    self.send(to, window, self.atoms.XdndEnter, message);
                    going.over = Some(Over {
                        window,
                        to,
                        version,
                        waiting: false,
                        next: None,
                        taken: None,
                        quiet: None,
                    });
                }
            }
            if let Some(over) = going.over.as_mut() {
                if over.waiting {
                    over.next = Some((at, time));
                } else {
                    over.waiting = true;
                    let message = position(self.proxy, at, time, going.proposed);
                    self.send(over.to, over.window, self.atoms.XdndPosition, message);
                }
            }
            let _ = self.conn.flush();
            self.going = Some(going);
        }

        /// The window at `at` that takes drags, with where its messages go
        /// and the version both sides speak: the first window down from
        /// the root that is `XdndAware`, or names a proxy that is.
        fn taker_at(&self, (x, y): (i16, i16)) -> Option<(u32, u32, u32)> {
            let mut window = self.root;
            loop {
                window = self
                    .conn
                    .translate_coordinates(self.root, window, x, y)
                    .ok()?
                    .reply()
                    .ok()?
                    .child;
                if window == x11rb::NONE {
                    return None;
                }
                if let Some(taker) = self.taker(window) {
                    return Some(taker);
                }
            }
        }

        fn taker(&self, window: u32) -> Option<(u32, u32, u32)> {
            let a = self.atoms;
            let first = |w: u32, property: Atom, kind: AtomEnum| -> Option<u32> {
                let reply = self
                    .conn
                    .get_property(false, w, property, kind, 0, 1)
                    .ok()?;
                reply.reply().ok()?.value32()?.next()
            };
            // A proxy is one when it names itself, as XDND asks; one that
            // does not is stale, and the window speaks for itself.
            let to = first(window, a.XdndProxy, AtomEnum::WINDOW)
                .filter(|&p| first(p, a.XdndProxy, AtomEnum::WINDOW) == Some(p))
                .unwrap_or(window);
            let version = first(to, a.XdndAware, AtomEnum::ATOM)?;
            (version >= OLDEST).then_some((window, to, version.min(VERSION)))
        }

        fn status(&mut self, target: u32, taken: Option<Atom>, quiet: Option<Quiet>) {
            let Some(mut going) = self.going.take() else {
                return;
            };
            let Some(over) = going.over.as_mut().filter(|o| o.window == target) else {
                self.going = Some(going);
                return;
            };
            over.waiting = false;
            over.taken = taken;
            over.quiet = quiet;
            match going.phase {
                Phase::Dragging => {
                    if let Some((at, time)) = over.next.take() {
                        over.waiting = true;
                        let message = position(self.proxy, at, time, going.proposed);
                        self.send(over.to, over.window, self.atoms.XdndPosition, message);
                        let _ = self.conn.flush();
                    }
                    self.going = Some(going);
                }
                Phase::Released { time } => self.decide(going, time),
                Phase::Dropped => self.going = Some(going),
            }
        }

        fn released(&mut self, time: u32) {
            let Some(mut going) = self.going.take() else {
                return;
            };
            if going.phase != Phase::Dragging {
                self.going = Some(going);
                return;
            }
            self.let_go(&mut going);
            match &going.over {
                None => self.end(going, None),
                Some(over) if over.waiting => {
                    going.phase = Phase::Released { time };
                    going.deadline = Some(Instant::now() + STATUS_WAIT);
                    self.going = Some(going);
                }
                Some(_) => self.decide(going, time),
            }
        }

        /// The drag was let go of and the target has answered: it is
        /// dropped there, or nothing takes it.
        fn decide(&mut self, mut going: Going, time: u32) {
            let Some(over) = going.over.as_ref() else {
                return self.end(going, None);
            };
            if over.taken.is_some() {
                let message = drop_message(self.proxy, time);
                self.send(over.to, over.window, self.atoms.XdndDrop, message);
                let _ = self.conn.flush();
                going.phase = Phase::Dropped;
                going.deadline = Some(Instant::now() + FINISH_WAIT);
                self.going = Some(going);
            } else {
                self.send(
                    over.to,
                    over.window,
                    self.atoms.XdndLeave,
                    leave(self.proxy),
                );
                self.end(going, None);
            }
        }

        fn target_finished(&mut self, target: u32, taken: bool, action: Atom) {
            let Some(going) = self.going.take_if(|g| {
                g.phase == Phase::Dropped && g.over.as_ref().is_some_and(|o| o.window == target)
            }) else {
                return;
            };
            let over = going.over.as_ref().expect("it was dropped there");
            let action = finished_action(&self.atoms, over.version, over.taken, taken, action);
            self.end(going, action);
        }

        fn pressed(&mut self, key: Keycode) {
            let Some(going) = self
                .going
                .take_if(|g| g.phase == Phase::Dragging && g.escape.contains(&key))
            else {
                return;
            };
            if let Some(over) = &going.over {
                self.send(
                    over.to,
                    over.window,
                    self.atoms.XdndLeave,
                    leave(self.proxy),
                );
            }
            self.end(going, None);
        }

        /// The target took too long: to answer the last position once the
        /// drag was let go of, or to finish with the drop.
        fn ran_out(&mut self) {
            let Some(going) = self.going.take() else {
                return;
            };
            if let (Phase::Released { .. }, Some(over)) = (going.phase, &going.over) {
                self.send(
                    over.to,
                    over.window,
                    self.atoms.XdndLeave,
                    leave(self.proxy),
                );
            }
            self.end(going, None);
        }

        /// The loop gave the drag up, or handed a newer one over: no one
        /// hears how this one ends.
        fn give_up(&mut self, mut going: Going) {
            if going.phase == Phase::Dragging
                && let Some(over) = &going.over
            {
                self.send(
                    over.to,
                    over.window,
                    self.atoms.XdndLeave,
                    leave(self.proxy),
                );
            }
            self.let_go(&mut going);
            self.pieces.clear();
            let _ = self.conn.flush();
        }

        fn let_go(&self, going: &mut Going) {
            if std::mem::replace(&mut going.grabbed, false) {
                let _ = self.conn.ungrab_pointer(x11rb::CURRENT_TIME);
                let _ = self.conn.ungrab_keyboard(x11rb::CURRENT_TIME);
            }
        }

        fn end(&mut self, mut going: Going, action: Option<DropAction>) {
            self.let_go(&mut going);
            self.pieces.clear();
            let _ = self.conn.set_selection_owner(
                x11rb::NONE,
                self.atoms.XdndSelection,
                x11rb::CURRENT_TIME,
            );
            let _ = self.conn.flush();
            lock(&self.outs).ended = Some((going.number, action));
            (self.post)(DragStep::OutEnded);
        }

        /// A target asks for a format of the drag out (or the list of
        /// them).
        fn requested(&mut self, r: &SelectionRequestEvent) {
            let a = self.atoms;
            // ICCCM's older requestors name no property.
            let property = if r.property == x11rb::NONE {
                r.target
            } else {
                r.property
            };
            let given = match self
                .going
                .as_mut()
                .filter(|_| r.selection == a.XdndSelection)
            {
                None => false,
                Some(going) => {
                    if going.phase == Phase::Dropped {
                        going.deadline = Some(Instant::now() + FINISH_WAIT);
                    }
                    if r.target == a.TARGETS {
                        let mut targets = going.types.clone();
                        targets.push(a.TARGETS);
                        self.conn
                            .change_property32(
                                PropMode::REPLACE,
                                r.requestor,
                                property,
                                AtomEnum::ATOM,
                                &targets,
                            )
                            .is_ok()
                    } else if let Some((_, bytes)) =
                        going.formats.iter().find(|(f, _)| *f == r.target)
                    {
                        let bytes = Arc::clone(bytes);
                        self.give(r.requestor, property, r.target, bytes)
                    } else {
                        false
                    }
                }
            };
            let notify = SelectionNotifyEvent {
                response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
                sequence: 0,
                time: r.time,
                requestor: r.requestor,
                selection: r.selection,
                target: r.target,
                property: if given { property } else { x11rb::NONE },
            };
            let _ = self
                .conn
                .send_event(false, r.requestor, EventMask::NO_EVENT, notify);
            let _ = self.conn.flush();
        }

        /// Writes a format where the requestor asked, whole or, when it
        /// is large, as `INCR`: each piece once the last is taken.
        fn give(&mut self, requestor: u32, property: Atom, kind: Atom, bytes: Arc<[u8]>) -> bool {
            if bytes.len() <= PIECE {
                return self
                    .conn
                    .change_property8(PropMode::REPLACE, requestor, property, kind, &bytes)
                    .is_ok();
            }
            // It says it took a piece by deleting it, which only a
            // requestor that is watched is heard doing. The unseen window
            // is watched already.
            if requestor != self.proxy {
                let watch = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
                let _ = self.conn.change_window_attributes(requestor, &watch);
            }
            let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            self.pieces
                .retain(|p| p.requestor != requestor || p.property != property);
            self.pieces.push(Piecewise {
                requestor,
                property,
                kind,
                bytes,
                sent: 0,
            });
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    self.atoms.INCR,
                    &[size],
                )
                .is_ok()
        }

        /// A requestor took what was there of a large format: the next
        /// piece follows, or the empty one that ends it.
        fn piece_taken(&mut self, window: u32, property: Atom) {
            let Some(at) = self
                .pieces
                .iter()
                .position(|p| p.requestor == window && p.property == property)
            else {
                return;
            };
            if let Some(going) = self.going.as_mut().filter(|g| g.phase == Phase::Dropped) {
                going.deadline = Some(Instant::now() + FINISH_WAIT);
            }
            let p = &mut self.pieces[at];
            let next = piece(&p.bytes, p.sent);
            let _ = self
                .conn
                .change_property8(PropMode::REPLACE, window, property, p.kind, next);
            p.sent += next.len();
            if next.is_empty() {
                self.pieces.remove(at);
                if window != self.proxy && !self.pieces.iter().any(|p| p.requestor == window) {
                    let unwatch = ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT);
                    let _ = self.conn.change_window_attributes(window, &unwatch);
                }
            }
            let _ = self.conn.flush();
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

        /// The types a drag offers that are read, by name.
        fn readable_types(&self, types: &[Atom]) -> Vec<(String, Atom)> {
            types
                .iter()
                .filter_map(|&t| {
                    let name = self.conn.get_atom_name(t).ok()?.reply().ok()?.name;
                    let name = String::from_utf8(name).ok()?;
                    readable_format(&name).then_some((name, t))
                })
                .collect()
        }

        fn read_all(&mut self, named: Vec<(String, Atom)>, time: u32) -> DragData {
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
                TARGETS: 19,
                LJOS_XDND_DATA: 16,
                LJOS_XDND_STOP: 17,
                LJOS_XDND_OUT: 20,
                LJOS_XDND_CANCEL: 21,
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

        /// Issue #115: a drag out names its first three types as it
        /// enters, and says when there are more in `XdndTypeList`.
        #[test]
        fn a_drag_out_enters_with_its_first_types_and_says_there_are_more() {
            assert_eq!(enter(9, 5, &[40, 41]), [9, 5 << 24, 40, 41, 0]);
            assert_eq!(enter(9, 4, &[40, 41, 42]), [9, 4 << 24, 40, 41, 42]);
            assert_eq!(
                enter(9, 5, &[40, 41, 42, 43]),
                [9, (5 << 24) | 1, 40, 41, 42]
            );
        }

        /// Issue #115: a drag out's position, drop and leave are what a
        /// target reads them as.
        #[test]
        fn a_drag_out_says_where_it_is_and_when_it_is_dropped() {
            let a = atoms();
            let message = position(9, (250, 350), 7, a.XdndActionMove);
            assert_eq!(
                parse(&a, 77, a.XdndPosition, message),
                Some(Xdnd::Position {
                    source: 9,
                    root: (250, 350),
                    time: 7,
                    action: a.XdndActionMove,
                })
            );
            // A point left of or above the root's origin, on a screen
            // beside the first.
            assert_eq!(position(9, (-3, 2), 0, 0)[2], 0xfffd_0002);
            assert_eq!(drop_message(9, 7), [9, 0, 7, 0, 0]);
            assert_eq!(
                parse(&a, 77, a.XdndDrop, drop_message(9, 7)),
                Some(Xdnd::Drop { source: 9 })
            );
            assert_eq!(
                parse(&a, 77, a.XdndLeave, leave(9)),
                Some(Xdnd::Leave { source: 9 })
            );
        }

        /// Issue #115: a target says whether it takes the drop, how, and
        /// where it needs no more positions.
        #[test]
        fn a_target_says_whether_it_takes_the_drop_and_where_it_needs_no_more_positions() {
            let a = atoms();
            assert_eq!(
                parse(
                    &a,
                    9,
                    a.XdndStatus,
                    [77, 1, (100 << 16) | 120, (50 << 16) | 20, 11]
                ),
                Some(Xdnd::Status {
                    target: 77,
                    taken: Some(11),
                    quiet: Some(Quiet {
                        x: 100,
                        y: 120,
                        width: 50,
                        height: 20,
                    }),
                })
            );
            assert_eq!(
                parse(&a, 9, a.XdndStatus, status(77, None)),
                Some(Xdnd::Status {
                    target: 77,
                    taken: None,
                    quiet: None,
                })
            );
            let quiet = Quiet {
                x: 100,
                y: 120,
                width: 50,
                height: 20,
            };
            assert!(quiet.holds((100, 120)));
            assert!(quiet.holds((149, 139)));
            assert!(!quiet.holds((150, 130)));
            assert!(!quiet.holds((120, 119)));
            // An empty rectangle holds nothing: every move is said.
            assert!(!Quiet { width: 0, ..quiet }.holds((100, 120)));
        }

        /// Issue #115: from version 5 a target says how it finished; an
        /// older one took the drop as its last status said.
        #[test]
        fn a_target_says_how_it_finished_with_the_drop() {
            let a = atoms();
            assert_eq!(
                parse(&a, 9, a.XdndFinished, finished(77, Some(a.XdndActionMove))),
                Some(Xdnd::Finished {
                    target: 77,
                    taken: true,
                    action: a.XdndActionMove,
                })
            );
            let copy = Some(a.XdndActionCopy);
            assert_eq!(
                finished_action(&a, 5, copy, true, a.XdndActionMove),
                Some(DropAction::Move)
            );
            assert_eq!(finished_action(&a, 5, copy, false, 0), None);
            assert_eq!(
                finished_action(&a, 5, copy, true, 0),
                Some(DropAction::Copy)
            );
            assert_eq!(
                finished_action(&a, 4, Some(a.XdndActionLink), false, 0),
                Some(DropAction::Link)
            );
            assert_eq!(finished_action(&a, 3, None, false, 0), None);
        }

        /// Issue #115: `XdndActionList` is what `supportedActions` allows;
        /// the action proposed is `proposedAction`, when it is allowed.
        #[test]
        fn a_drag_out_lists_what_it_allows_and_proposes_one_of_them() {
            let a = atoms();
            let allows = |allowed: i32, proposed| DragActions {
                allowed: DropActions::from_qt(allowed),
                proposed,
            };
            assert_eq!(
                out_actions(&a, allows(1 | 2, DropAction::Move)),
                (vec![a.XdndActionCopy, a.XdndActionMove], a.XdndActionMove)
            );
            assert_eq!(
                out_actions(&a, allows(4, DropAction::Move)),
                (vec![a.XdndActionLink], a.XdndActionLink)
            );
            assert_eq!(
                out_actions(&a, allows(0, DropAction::Move)),
                (vec![a.XdndActionCopy], a.XdndActionCopy)
            );
        }

        /// Issue #115: text goes out by its MIME types and by X11's name.
        #[test]
        fn a_drag_out_offers_text_by_x11s_name_too() {
            let data = DragData {
                urls: vec!["https://example.org/".into()],
                text: Some("hello".into()),
                other: vec![("application/x-example".into(), "42".into())],
                ..DragData::default()
            };
            let names: Vec<String> = offered(&data).into_iter().map(|(n, _)| n).collect();
            assert_eq!(
                names,
                [
                    "text/uri-list",
                    "text/plain;charset=utf-8",
                    "UTF8_STRING",
                    "text/plain",
                    "application/x-example",
                ]
            );
        }

        /// Issue #115: a large format goes in pieces, and an empty one
        /// ends it.
        #[test]
        fn a_large_format_goes_in_pieces_and_an_empty_one_ends_it() {
            let bytes = vec![7u8; PIECE * 2 + 10];
            assert_eq!(piece(&bytes, 0).len(), PIECE);
            assert_eq!(piece(&bytes, PIECE).len(), PIECE);
            assert_eq!(piece(&bytes, 2 * PIECE).len(), 10);
            assert!(piece(&bytes, bytes.len()).is_empty());
            assert!(piece(&bytes, bytes.len() + 5).is_empty());
        }

        #[test]
        fn escape_is_found_on_whichever_key_has_it() {
            // Two keysyms a key, from keycode 8.
            let syms = [0x61, 0x41, 0xff1b, 0, 0x62, 0x42, 0, 0xff1b];
            assert_eq!(keycodes_of(8, 2, &syms, ESCAPE), [9, 11]);
            assert!(keycodes_of(8, 2, &syms[..2], ESCAPE).is_empty());
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
    use ljos_platform::{DragActions, DragData, DropAction};

    use crate::drags::DragStep;

    pub(crate) enum Follower {}

    impl Follower {
        pub(crate) fn window_opened(&self, _window: &Window) {
            match *self {}
        }

        pub(crate) fn start_out(&mut self, _data: &DragData, _actions: DragActions) -> bool {
            match *self {}
        }

        pub(crate) fn cancel_out(&mut self) {
            match *self {}
        }

        pub(crate) fn out_ended(&mut self) -> Option<Option<DropAction>> {
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
