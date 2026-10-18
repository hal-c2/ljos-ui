//! A Wayland client of the compositor the applications run on: a virtual
//! pointer that clicks them, and screencopy that sees each frame the
//! compositor shows of them.
//!
//! Both are wlroots protocols, so sway offers them and an application does
//! not know they are there. A headless sway's seat has no pointer of its
//! own; the virtual pointer gives it one.

use std::{
    fs::OpenOptions,
    os::fd::{AsFd, AsRawFd},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool},
};
use wayland_protocols_wlr::{
    screencopy::v1::client::{
        zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
        zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    },
    virtual_pointer::v1::client::{
        zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
        zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
    },
};

/// The left button, as evdev numbers it.
const BTN_LEFT: u32 = 0x110;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot reach the compositor at {0}: {1}")]
    Connect(PathBuf, String),
    #[error("the compositor does not offer {0}")]
    Missing(&'static str),
    #[error("talking to the compositor: {0}")]
    Protocol(String),
}

fn protocol(e: impl std::fmt::Display) -> Error {
    Error::Protocol(e.to_string())
}

fn connect(display: &Path) -> Result<Connection, Error> {
    let stream = std::os::unix::net::UnixStream::connect(display)
        .map_err(|e| Error::Connect(display.to_owned(), e.to_string()))?;
    Connection::from_socket(stream).map_err(|e| Error::Connect(display.to_owned(), e.to_string()))
}

#[derive(Debug, Default)]
struct State {
    /// The buffer screencopy asked for: format, width, height, stride.
    shape: Option<(wl_shm::Format, u32, u32, u32)>,
    shape_done: bool,
    /// The last frame's outcome: its time in nanoseconds, or failed.
    ready: Option<Result<u64, ()>>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore wl_output::WlOutput);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(State: ZwlrVirtualPointerV1);
wayland_client::delegate_noop!(State: ZwlrScreencopyManagerV1);

impl Dispatch<ZwlrScreencopyFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => state.shape = Some((format, width, height, stride)),
            zwlr_screencopy_frame_v1::Event::BufferDone => state.shape_done = true,
            zwlr_screencopy_frame_v1::Event::Ready {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => {
                let secs = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                state.ready = Some(Ok(secs * 1_000_000_000 + u64::from(tv_nsec)));
            }
            zwlr_screencopy_frame_v1::Event::Failed => state.ready = Some(Err(())),
            _ => {}
        }
    }
}

/// Reads and handles what the compositor sent within `timeout`.
fn pump(queue: &mut EventQueue<State>, state: &mut State, timeout: Duration) -> Result<(), Error> {
    queue.flush().map_err(protocol)?;
    queue.dispatch_pending(state).map_err(protocol)?;
    if let Some(guard) = queue.prepare_read() {
        let mut fd = libc::pollfd {
            fd: guard.connection_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: one valid pollfd, owned by this frame, for the length given.
        let n = unsafe { libc::poll(&raw mut fd, 1, ms) };
        if n > 0 {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(protocol(e)),
            }
        }
    }
    queue.dispatch_pending(state).map_err(protocol)?;
    Ok(())
}

/// A pointer the compositor treats as a mouse plugged into its seat.
#[derive(Debug)]
pub struct Pointer {
    queue: EventQueue<State>,
    state: State,
    pointer: ZwlrVirtualPointerV1,
    started: Instant,
    sent: Vec<u64>,
}

impl Pointer {
    /// # Errors
    /// The compositor cannot be reached or has no virtual pointers.
    pub fn new(display: &Path) -> Result<Self, Error> {
        let conn = connect(display)?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(protocol)?;
        let qh = queue.handle();
        let seat: wl_seat::WlSeat = globals
            .bind(&qh, 1..=7, ())
            .map_err(|_| Error::Missing("a seat"))?;
        let manager: ZwlrVirtualPointerManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .map_err(|_| Error::Missing("zwlr_virtual_pointer_manager_v1"))?;
        let pointer = manager.create_virtual_pointer(Some(&seat), &qh, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).map_err(protocol)?;
        Ok(Self {
            queue,
            state,
            pointer,
            started: Instant::now(),
            sent: Vec::new(),
        })
    }

    /// When each input that moves the picture was sent, in nanoseconds of
    /// the clock screencopy times frames by, so that the frame that
    /// answered it can be found.
    #[must_use]
    pub fn sent(&self) -> &[u64] {
        &self.sent
    }

    fn mark(&mut self) {
        self.sent.push(monotonic_ns());
    }

    fn time(&self) -> u32 {
        // Event times are milliseconds that wrap; only differences count.
        #[allow(clippy::cast_possible_truncation)]
        let ms = self.started.elapsed().as_millis() as u32;
        ms
    }

    /// Moves to `(x, y)` of a layout `extent` pixels large and clicks the
    /// left button there.
    ///
    /// # Errors
    /// The compositor went away.
    pub fn click(&mut self, x: u32, y: u32, extent: (u32, u32)) -> Result<(), Error> {
        let p = &self.pointer;
        p.motion_absolute(self.time(), x, y, extent.0, extent.1);
        p.frame();
        p.button(self.time(), BTN_LEFT, wl_pointer::ButtonState::Pressed);
        p.frame();
        self.queue.flush().map_err(protocol)?;
        self.mark();
        thread::sleep(Duration::from_millis(40));
        let p = &self.pointer;
        p.button(self.time(), BTN_LEFT, wl_pointer::ButtonState::Released);
        p.frame();
        self.queue.flush().map_err(protocol)?;
        pump(&mut self.queue, &mut self.state, Duration::ZERO)
    }

    /// Moves to `(x, y)` and holds the left button down there.
    ///
    /// # Errors
    /// The compositor went away.
    pub fn press(&mut self, x: u32, y: u32, extent: (u32, u32)) -> Result<(), Error> {
        let p = &self.pointer;
        p.motion_absolute(self.time(), x, y, extent.0, extent.1);
        p.frame();
        p.button(self.time(), BTN_LEFT, wl_pointer::ButtonState::Pressed);
        p.frame();
        self.queue.flush().map_err(protocol)?;
        self.mark();
        pump(&mut self.queue, &mut self.state, Duration::ZERO)
    }

    /// Moves to `(x, y)`, dragging if a button is down.
    ///
    /// # Errors
    /// The compositor went away.
    pub fn move_to(&mut self, x: u32, y: u32, extent: (u32, u32)) -> Result<(), Error> {
        let p = &self.pointer;
        p.motion_absolute(self.time(), x, y, extent.0, extent.1);
        p.frame();
        self.queue.flush().map_err(protocol)?;
        self.mark();
        pump(&mut self.queue, &mut self.state, Duration::ZERO)
    }

    /// Lets the left button go.
    ///
    /// # Errors
    /// The compositor went away.
    pub fn release(&mut self) -> Result<(), Error> {
        let p = &self.pointer;
        p.button(self.time(), BTN_LEFT, wl_pointer::ButtonState::Released);
        p.frame();
        self.queue.flush().map_err(protocol)?;
        pump(&mut self.queue, &mut self.state, Duration::ZERO)
    }

    /// Turns the wheel `steps` notches at `(x, y)`: down for positive.
    ///
    /// # Errors
    /// The compositor went away.
    pub fn wheel(&mut self, x: u32, y: u32, extent: (u32, u32), steps: i32) -> Result<(), Error> {
        let p = &self.pointer;
        p.motion_absolute(self.time(), x, y, extent.0, extent.1);
        p.frame();
        p.axis_source(wl_pointer::AxisSource::Wheel);
        // 15 is what a notch scrolls by in libinput's units.
        p.axis_discrete(
            self.time(),
            wl_pointer::Axis::VerticalScroll,
            15.0 * f64::from(steps),
            steps,
        );
        p.frame();
        self.queue.flush().map_err(protocol)?;
        self.mark();
        pump(&mut self.queue, &mut self.state, Duration::ZERO)
    }
}

/// `CLOCK_MONOTONIC` in nanoseconds: the clock wlroots stamps a
/// screencopy frame with.
pub(crate) fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid timespec, owned by this frame.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut ts) };
    u64::try_from(ts.tv_sec).unwrap_or(0) * 1_000_000_000 + u64::try_from(ts.tv_nsec).unwrap_or(0)
}

/// Frames the compositor showed of a region while the counter ran.
#[derive(Debug)]
pub struct FrameCounter {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<Counted, Error>>,
}

/// The frames a [`FrameCounter`] saw.
#[derive(Debug, Default)]
pub struct Counted {
    /// The times of those that changed a pixel of the region, in
    /// nanoseconds of the compositor's clock.
    pub changed: Vec<u64>,
    /// Every frame the client sent that damaged the region, changed or not.
    pub committed: usize,
}

impl FrameCounter {
    /// Starts counting the frames that change the region `(x, y, w, h)` of
    /// the first output: those that damage it, and of those, the ones that
    /// change a pixel of it.
    ///
    /// # Errors
    /// The compositor cannot be reached or has no screencopy.
    pub fn start(display: &Path, region: (i32, i32, i32, i32)) -> Result<Self, Error> {
        let conn = connect(display)?;
        let (globals, queue) = registry_queue_init::<State>(&conn).map_err(protocol)?;
        let qh = queue.handle();
        let output: wl_output::WlOutput = globals
            .bind(&qh, 1..=4, ())
            .map_err(|_| Error::Missing("an output"))?;
        let shm: wl_shm::WlShm = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| Error::Missing("wl_shm"))?;
        let manager: ZwlrScreencopyManagerV1 = globals
            .bind(&qh, 2..=3, ())
            .map_err(|_| Error::Missing("zwlr_screencopy_manager_v1 version 2"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = thread::Builder::new()
            .name("bench-frames".to_owned())
            .spawn(move || count(queue, &output, &shm, &manager, region, &flag))
            .map_err(protocol)?;
        Ok(Self { stop, handle })
    }

    /// Stops and gives the frames it saw.
    ///
    /// # Errors
    /// The compositor failed a copy or went away.
    pub fn finish(self) -> Result<Counted, Error> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .join()
            .map_err(|_| Error::Protocol("the frame counter panicked".to_owned()))?
    }
}

/// Frames of one region of the first output, as pixels.
struct Region {
    queue: EventQueue<State>,
    state: State,
    output: wl_output::WlOutput,
    shm: wl_shm::WlShm,
    manager: ZwlrScreencopyManagerV1,
    rect: (i32, i32, i32, i32),
    buffer: Option<(wl_buffer::WlBuffer, std::fs::File, usize)>,
}

impl Region {
    fn new(display: &Path, rect: (i32, i32, i32, i32)) -> Result<Self, Error> {
        let conn = connect(display)?;
        let (globals, queue) = registry_queue_init::<State>(&conn).map_err(protocol)?;
        let qh = queue.handle();
        Ok(Self {
            output: globals
                .bind(&qh, 1..=4, ())
                .map_err(|_| Error::Missing("an output"))?,
            shm: globals
                .bind(&qh, 1..=1, ())
                .map_err(|_| Error::Missing("wl_shm"))?,
            manager: globals
                .bind(&qh, 2..=3, ())
                .map_err(|_| Error::Missing("zwlr_screencopy_manager_v1 version 2"))?,
            queue,
            state: State::default(),
            rect,
            buffer: None,
        })
    }

    /// The region's pixels: as they are now, or once the compositor has
    /// shown a frame that changed something, when `damage`. `None` when
    /// `deadline` passed first.
    fn copy(&mut self, damage: bool, deadline: Instant) -> Result<Option<Vec<u8>>, Error> {
        let qh = self.queue.handle();
        let (x, y, w, h) = self.rect;
        let frame = self
            .manager
            .capture_output_region(0, &self.output, x, y, w, h, &qh, ());
        self.state.ready = None;
        if self.buffer.is_none() {
            self.state.shape = None;
            self.state.shape_done = false;
            while !(self.state.shape_done
                || (self.manager.version() < 3 && self.state.shape.is_some()))
            {
                if Instant::now() > deadline {
                    return Ok(None);
                }
                pump(&mut self.queue, &mut self.state, Duration::from_millis(20))?;
            }
            let (format, bw, bh, stride) = self.state.shape.ok_or(Error::Protocol(
                "screencopy offered no shm buffer".to_owned(),
            ))?;
            let (buffer, file) = shm_buffer(&self.shm, &qh, format, bw, bh, stride)?;
            let size = usize::try_from(u64::from(stride) * u64::from(bh)).map_err(protocol)?;
            self.buffer = Some((buffer, file, size));
        }
        let Some((target, file, size)) = &self.buffer else {
            return Ok(None);
        };
        if damage {
            frame.copy_with_damage(target);
        } else {
            frame.copy(target);
        }
        while self.state.ready.is_none() && Instant::now() < deadline {
            pump(&mut self.queue, &mut self.state, Duration::from_millis(5))?;
        }
        frame.destroy();
        match self.state.ready.take() {
            Some(Ok(_)) => {
                use std::os::unix::fs::FileExt;
                let mut pixels = vec![0; *size];
                file.read_exact_at(&mut pixels, 0).map_err(protocol)?;
                Ok(Some(pixels))
            }
            Some(Err(())) => Err(Error::Protocol("screencopy failed a frame".to_owned())),
            None => Ok(None),
        }
    }
}

/// When the region `(x, y, w, h)` of the first output first shows anything
/// other than what it showed before `cause` ran: `None` when nothing
/// changed by `deadline`.
///
/// # Errors
/// The compositor cannot be reached, has no screencopy or failed a copy.
pub fn first_change(
    display: &Path,
    region: (i32, i32, i32, i32),
    deadline: Instant,
    cause: impl FnOnce(),
) -> Result<Option<Instant>, Error> {
    let mut r = Region::new(display, region)?;
    let Some(before) = r.copy(false, deadline)? else {
        return Ok(None);
    };
    cause();
    while let Some(now) = r.copy(true, deadline)? {
        if now != before {
            return Ok(Some(Instant::now()));
        }
    }
    Ok(None)
}

fn count(
    mut queue: EventQueue<State>,
    output: &wl_output::WlOutput,
    shm: &wl_shm::WlShm,
    manager: &ZwlrScreencopyManagerV1,
    (x, y, w, h): (i32, i32, i32, i32),
    stop: &AtomicBool,
) -> Result<Counted, Error> {
    use std::os::unix::fs::FileExt;
    let qh = queue.handle();
    let mut state = State::default();
    let mut buffer: Option<(wl_buffer::WlBuffer, std::fs::File, Vec<u8>)> = None;
    let mut last: Option<Vec<u8>> = None;
    let mut counted = Counted::default();
    while !stop.load(Ordering::Relaxed) {
        let frame = manager.capture_output_region(0, output, x, y, w, h, &qh, ());
        state.ready = None;
        if buffer.is_none() {
            state.shape = None;
            state.shape_done = false;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !(state.shape_done || (manager.version() < 3 && state.shape.is_some())) {
                if Instant::now() > deadline {
                    return Err(Error::Protocol("screencopy named no buffer".to_owned()));
                }
                pump(&mut queue, &mut state, Duration::from_millis(100))?;
            }
            let (format, bw, bh, stride) = state.shape.ok_or(Error::Protocol(
                "screencopy offered no shm buffer".to_owned(),
            ))?;
            let (b, file) = shm_buffer(shm, &qh, format, bw, bh, stride)?;
            let size = usize::try_from(u64::from(stride) * u64::from(bh)).map_err(protocol)?;
            buffer = Some((b, file, vec![0; size]));
        }
        let Some((target, file, pixels)) = &mut buffer else {
            break;
        };
        frame.copy_with_damage(target);
        while state.ready.is_none() && !stop.load(Ordering::Relaxed) {
            pump(&mut queue, &mut state, Duration::from_millis(50))?;
        }
        frame.destroy();
        match state.ready.take() {
            Some(Ok(t)) => {
                file.read_exact_at(pixels, 0).map_err(protocol)?;
                // The first copy is of whatever was there when counting
                // began, not a frame the application made.
                match &mut last {
                    None => last = Some(pixels.clone()),
                    Some(was) => {
                        counted.committed += 1;
                        // A frame the client sent again unchanged is work,
                        // not a frame anyone saw: one runtime that redraws
                        // every frame of a running particle system and one
                        // that sends only changes are compared by what
                        // they showed.
                        if was != pixels {
                            was.copy_from_slice(pixels);
                            counted.changed.push(t);
                        }
                    }
                }
            }
            Some(Err(())) => return Err(Error::Protocol("screencopy failed a frame".to_owned())),
            None => break,
        }
    }
    Ok(counted)
}

fn shm_buffer(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<State>,
    format: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
) -> Result<(wl_buffer::WlBuffer, std::fs::File), Error> {
    let size = u64::from(stride) * u64::from(height);
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("ljos-bench-{}.shm", std::process::id()));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(protocol)?;
    // The compositor keeps its own reference through the pool.
    let _ = std::fs::remove_file(&path);
    file.set_len(size).map_err(protocol)?;
    let to_i32 = |v: u64| i32::try_from(v).map_err(protocol);
    let pool = shm.create_pool(file.as_fd(), to_i32(size)?, qh, ());
    let buffer = pool.create_buffer(
        0,
        to_i32(width.into())?,
        to_i32(height.into())?,
        to_i32(stride.into())?,
        format,
        qh,
        (),
    );
    pool.destroy();
    Ok((buffer, file))
}
