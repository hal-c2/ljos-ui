//! Keyboard and pointer input for the Qt oracle.
//!
//! `qml6` takes no input on the `offscreen` platform, and `QtTest` is not
//! something a plain `qml6` run may lean on. Qt's `vnc` platform plugin does
//! take input: it is a display server that speaks RFB (the VNC wire protocol,
//! RFC 6143). So a scenario that presses keys runs `qml6` on that platform
//! and this module is the smallest RFB client that can type and click: the
//! handshake, `KeyEvent` and `PointerEvent`. Nothing is read back after the
//! handshake; what Qt did with the input is observed through properties, as
//! everywhere else.
//!
//! Qt's server has no password and listens on every interface, and it has no
//! option for either. So the run gets a network namespace of its own, where
//! the port is out of reach of the network, and a relay inside it carries the
//! one connection out over a unix socket (see [`Endpoint`]).
//!
//! The events of one step end with a pointer move to a corner pixel. RFB
//! delivers in order, so when the driver's `HoverHandler` sees that move, Qt
//! has handled everything before it. That is the acknowledgement; no sleep
//! is needed.

use std::{
    fmt::Debug,
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use std::{
    os::unix::net::UnixStream,
    process::Stdio,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

/// The window is this many pixels each way (the driver item's size).
pub(super) const SCREEN: u16 = 400;

/// X11 keysyms of the modifier keys, pressed around the key of a chord.
const SHIFT: u32 = 0xffe1;
const CONTROL: u32 = 0xffe3;
const ALT: u32 = 0xffe9;
const META: u32 = 0xffeb;

/// Keys by the names Qt's portable key sequences use (`"Ctrl+PgUp"`), with
/// the spelled-out forms a scenario author may reach for.
const NAMED: &[(&str, u32)] = &[
    ("Space", 0x20),
    ("Backspace", 0xff08),
    ("Tab", 0xff09),
    ("Return", 0xff0d),
    ("Esc", 0xff1b),
    ("Escape", 0xff1b),
    ("Home", 0xff50),
    ("Left", 0xff51),
    ("Up", 0xff52),
    ("Right", 0xff53),
    ("Down", 0xff54),
    ("PgUp", 0xff55),
    ("PageUp", 0xff55),
    ("PgDown", 0xff56),
    ("PageDown", 0xff56),
    ("End", 0xff57),
    ("Ins", 0xff63),
    ("Insert", 0xff63),
    ("Menu", 0xff67),
    ("Enter", 0xff8d),
    ("Del", 0xffff),
    ("Delete", 0xffff),
];

/// One key with the modifiers held around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Chord {
    modifiers: Vec<u32>,
    key: u32,
}

impl Chord {
    /// `"Ctrl+Shift+S"`. The key is the last part; `"Ctrl++"` is Ctrl and
    /// the plus key.
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        let (mods, key) = match text.strip_suffix("++") {
            Some(mods) => (mods, "+"),
            None => match text.rsplit_once('+') {
                Some((mods, key)) if !key.is_empty() => (mods, key),
                _ => ("", text),
            },
        };
        let mut modifiers = Vec::new();
        for m in mods.split('+').filter(|m| !m.is_empty()) {
            modifiers.push(match m.to_ascii_lowercase().as_str() {
                "shift" => SHIFT,
                "ctrl" => CONTROL,
                "alt" => ALT,
                "meta" => META,
                _ => return Err(format!("unknown modifier {m:?} in the chord {text:?}")),
            });
        }
        let shifted = modifiers.contains(&SHIFT);
        let key = keysym(key, shifted)
            .ok_or_else(|| format!("the Qt oracle cannot press the key {key:?}"))?;
        Ok(Self { modifiers, key })
    }
}

fn keysym(name: &str, shifted: bool) -> Option<u32> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        // Latin-1 keysyms are the character codes. A keyboard gives the
        // lower-case letter unless Shift is held.
        let c = if shifted {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        };
        return (c as u32 <= 0xff && !c.is_control()).then_some(c as u32);
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=35).contains(&n)
    {
        return Some(0xffbd + n);
    }
    NAMED
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, sym)| *sym)
}

/// Set to `1` to run Qt's VNC server on a port of the host, where anyone who
/// can reach the machine can watch and type into the scenario while it runs.
const OPEN_VAR: &str = "LJOS_BDD_OPEN_VNC";

/// Only the handshake reads; a server that stops answering in it is an error.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The port inside the private namespace. Nothing else is there to hold it.
#[cfg(target_os = "linux")]
const PRIVATE_PORT: u16 = 5900;

/// A namespace of its own for the network (the port is unreachable) and for
/// process ids (the relay dies with `qml6`, which is process 1 there).
#[cfg(target_os = "linux")]
const UNSHARE: &[&str] = &[
    "--user",
    "--map-current-user",
    "--keep-caps",
    "--net",
    "--pid",
    "--fork",
    "--kill-child",
    "--",
    "sh",
    "-c",
];

/// `$1` is the socket, the rest is the `qml6` command line. A new namespace
/// has its loopback interface down.
#[cfg(target_os = "linux")]
const RELAY: &str = r#"ip link set lo up && { socat UNIX-LISTEN:"$1",fork TCP:127.0.0.1:5900 2>/dev/null & } && shift && exec "$@""#;

/// Where a run that takes input has its VNC server.
#[derive(Debug)]
pub(super) enum Endpoint {
    /// In a private network namespace, behind a relay on this unix socket.
    #[cfg(target_os = "linux")]
    Private(PathBuf),
    /// On this port of the host, on every interface (`LJOS_BDD_OPEN_VNC=1`).
    Open(u16),
}

impl Endpoint {
    /// Private where the host can do it. An open port is never a silent
    /// fallback: it has to be asked for.
    pub(super) fn choose() -> io::Result<Self> {
        if std::env::var(OPEN_VAR).is_ok_and(|v| v == "1") {
            return Ok(Self::Open(free_port()?));
        }
        #[cfg(target_os = "linux")]
        if private_network_works() {
            return socket_path().map(Self::Private);
        }
        Err(io::Error::other(format!(
            "sending input to Qt needs `unshare`, `ip` and `socat` with unprivileged user \
             namespaces, to keep Qt's password-less VNC server off the network; \
             {OPEN_VAR}=1 runs it on an open port instead"
        )))
    }

    /// The value of `QT_QPA_PLATFORM`.
    pub(super) fn platform(&self) -> String {
        let port = match self {
            #[cfg(target_os = "linux")]
            Self::Private(_) => PRIVATE_PORT,
            Self::Open(port) => *port,
        };
        format!("vnc:port={port}:size={SCREEN}x{SCREEN}")
    }

    /// The command that runs `qml`; its arguments are appended by the caller.
    pub(super) fn command(&self, qml: &str) -> Command {
        match self {
            #[cfg(target_os = "linux")]
            Self::Private(socket) => {
                let mut command = Command::new("unshare");
                command
                    .args(UNSHARE)
                    .args([RELAY, "sh"])
                    .arg(socket)
                    .arg(qml);
                command
            }
            Self::Open(_) => Command::new(qml),
        }
    }

    fn connect(&self) -> io::Result<Box<dyn Wire>> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Private(socket) => {
                let stream = UnixStream::connect(socket)?;
                stream.set_read_timeout(Some(READ_TIMEOUT))?;
                Ok(Box::new(stream))
            }
            Self::Open(port) => {
                let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, *port))?;
                stream.set_nodelay(true)?;
                stream.set_read_timeout(Some(READ_TIMEOUT))?;
                Ok(Box::new(stream))
            }
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Self::Private(socket) = self {
            let _ = std::fs::remove_file(socket);
        }
    }
}

/// Whether this host can start a process in a private network namespace with
/// the relay's tools in it. Asked once per test run.
#[cfg(target_os = "linux")]
fn private_network_works() -> bool {
    static WORKS: OnceLock<bool> = OnceLock::new();
    *WORKS.get_or_init(|| {
        Command::new("unshare")
            .args(UNSHARE)
            .arg("ip link set lo up && command -v socat")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// A socket path of this process's own. The user's runtime directory is short
/// and private; a unix socket path may not be longer than 107 bytes.
#[cfg(target_os = "linux")]
fn socket_path() -> io::Result<PathBuf> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!(
        "ljos-bdd-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if path.as_os_str().len() > 107 {
        return Err(io::Error::other(format!(
            "{} is too long for a unix socket; set XDG_RUNTIME_DIR to a short path",
            path.display()
        )));
    }
    Ok(path)
}

/// A port nothing listens on right now, for Qt's VNC server. The server
/// itself offers no "any port" option, so there is a moment between this
/// probe and Qt's bind; the run fails loudly (no connection) if it is lost.
fn free_port() -> io::Result<u16> {
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port())
}

/// The byte stream to the server: TCP, or the relay's unix socket.
trait Wire: Read + Write + Debug + Send {}
impl<T: Read + Write + Debug + Send> Wire for T {}

/// A connected client, past the handshake.
#[derive(Debug)]
pub(super) struct Rfb {
    stream: Box<dyn Wire>,
    /// Acknowledgement moves sent so far; picks the next corner pixel.
    acks: u32,
}

impl Rfb {
    /// Connects as soon as the server is up (it starts with the process,
    /// before any QML is loaded) and performs the handshake. The relay
    /// accepts before the server does and then hangs up, so a connection
    /// that ends in the handshake is tried again as well.
    pub(super) fn connect(endpoint: &Endpoint, deadline: Instant) -> io::Result<Self> {
        loop {
            let attempt = endpoint.connect().and_then(|stream| {
                let mut rfb = Self { stream, acks: 0 };
                rfb.handshake()?;
                Ok(rfb)
            });
            match attempt {
                Ok(rfb) => return Ok(rfb),
                Err(e) if !not_up_yet(&e) || Instant::now() > deadline => return Err(e),
                Err(_) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn handshake(&mut self) -> io::Result<()> {
        // "RFB 003.003\n": the server names its version, the client answers
        // with the one it will speak. 3.3 is what Qt's server offers and the
        // simplest: the server then picks the security type itself.
        let mut version = [0u8; 12];
        self.stream.read_exact(&mut version)?;
        if &version[..4] != b"RFB " {
            return Err(io::Error::other("not an RFB server"));
        }
        self.stream.write_all(b"RFB 003.003\n")?;
        let mut security = [0u8; 4];
        self.stream.read_exact(&mut security)?;
        if u32::from_be_bytes(security) != 1 {
            return Err(io::Error::other("the RFB server asks for a password"));
        }
        // ClientInit: share the desktop.
        self.stream.write_all(&[1])?;
        // ServerInit: size, pixel format, then the desktop's name.
        let mut init = [0u8; 24];
        self.stream.read_exact(&mut init)?;
        let name_len = u32::from_be_bytes([init[20], init[21], init[22], init[23]]);
        io::copy(
            &mut (&mut self.stream).take(u64::from(name_len)),
            &mut io::sink(),
        )?;
        Ok(())
    }

    fn key(buf: &mut Vec<u8>, sym: u32, down: bool) {
        buf.extend_from_slice(&[4, u8::from(down), 0, 0]);
        buf.extend_from_slice(&sym.to_be_bytes());
    }

    fn pointer(buf: &mut Vec<u8>, buttons: u8, x: u16, y: u16) {
        buf.extend_from_slice(&[5, buttons]);
        buf.extend_from_slice(&x.to_be_bytes());
        buf.extend_from_slice(&y.to_be_bytes());
    }

    /// The pointer move the driver waits for. It alternates between two
    /// pixels because a move to where the pointer already is, is no event.
    fn ack(&mut self, buf: &mut Vec<u8>) {
        let (x, y) = ack_position(self.acks);
        Self::pointer(buf, 0, x, y);
        self.acks += 1;
    }

    /// Modifiers down, the key down and up, modifiers up in reverse.
    pub(super) fn press(&mut self, chord: &Chord) -> io::Result<()> {
        let mut buf = Vec::new();
        for m in &chord.modifiers {
            Self::key(&mut buf, *m, true);
        }
        Self::key(&mut buf, chord.key, true);
        Self::key(&mut buf, chord.key, false);
        for m in chord.modifiers.iter().rev() {
            Self::key(&mut buf, *m, false);
        }
        self.ack(&mut buf);
        self.stream.write_all(&buf)
    }

    /// A click of the buttons in `mask` (1 the left, 2 the middle, 4 the
    /// right).
    pub(super) fn click(&mut self, mask: u8, x: u16, y: u16) -> io::Result<()> {
        let mut buf = Vec::new();
        Self::pointer(&mut buf, 0, x, y);
        Self::pointer(&mut buf, mask, x, y);
        Self::pointer(&mut buf, 0, x, y);
        self.ack(&mut buf);
        self.stream.write_all(&buf)
    }

    /// Two clicks of the left button at one place: a double click.
    ///
    /// The one place where time matters: Qt takes two presses with the same
    /// timestamp for one, so the second click waits until the clock has
    /// moved, far inside the double-click interval.
    pub(super) fn double_click(&mut self, x: u16, y: u16) -> io::Result<()> {
        let mut buf = Vec::new();
        Self::pointer(&mut buf, 0, x, y);
        Self::pointer(&mut buf, 1, x, y);
        Self::pointer(&mut buf, 0, x, y);
        self.stream.write_all(&buf)?;
        thread::sleep(DOUBLE_CLICK_GAP);
        buf.clear();
        Self::pointer(&mut buf, 1, x, y);
        Self::pointer(&mut buf, 0, x, y);
        self.ack(&mut buf);
        self.stream.write_all(&buf)
    }

    /// The left button goes down at `from`, the pointer moves to `to` in
    /// steps of a few pixels, and the button goes up there.
    pub(super) fn drag(&mut self, from: (u16, u16), to: (u16, u16)) -> io::Result<()> {
        let mut buf = Vec::new();
        Self::pointer(&mut buf, 0, from.0, from.1);
        Self::pointer(&mut buf, 1, from.0, from.1);
        for (x, y) in drag_path(from, to) {
            Self::pointer(&mut buf, 1, x, y);
        }
        Self::pointer(&mut buf, 0, to.0, to.1);
        self.ack(&mut buf);
        self.stream.write_all(&buf)
    }

    /// One step of a press that is held across steps. No acknowledgement
    /// move follows: with the button down it would be part of the drag, and
    /// with it up it would let go. The driver acknowledges the step's own
    /// event instead (`HELD_HANDLER` in `qt.rs`).
    pub(super) fn held(&mut self, step: HeldStep, x: u16, y: u16) -> io::Result<()> {
        let mut buf = Vec::new();
        match step {
            HeldStep::Press => {
                Self::pointer(&mut buf, 0, x, y);
                Self::pointer(&mut buf, 1, x, y);
            }
            HeldStep::Move => Self::pointer(&mut buf, 1, x, y),
            HeldStep::Hover => Self::pointer(&mut buf, 0, x, y),
            HeldStep::Release => {
                // Where the button goes up is first a place the pointer is
                // dragged to.
                Self::pointer(&mut buf, 1, x, y);
                Self::pointer(&mut buf, 0, x, y);
            }
        }
        self.stream.write_all(&buf)
    }

    /// One key press per character, with Shift held around a capital
    /// letter as a keyboard would have it.
    pub(super) fn type_text(&mut self, text: &str) -> io::Result<()> {
        let mut buf = Vec::new();
        for c in text.chars() {
            let sym = char_keysym(c)
                .ok_or_else(|| io::Error::other(format!("the Qt oracle cannot type {c:?}")))?;
            let shift = c.is_uppercase();
            if shift {
                Self::key(&mut buf, SHIFT, true);
            }
            Self::key(&mut buf, sym, true);
            Self::key(&mut buf, sym, false);
            if shift {
                Self::key(&mut buf, SHIFT, false);
            }
        }
        self.ack(&mut buf);
        self.stream.write_all(&buf)
    }
}

/// Between the clicks of a double click: longer than a tick of Qt's event
/// clock, much shorter than its double-click interval (400 ms).
const DOUBLE_CLICK_GAP: Duration = Duration::from_millis(50);

/// What the pointer does in one step of a press that lasts several steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeldStep {
    Press,
    /// With the button down.
    Move,
    /// With no button down.
    Hover,
    Release,
}

impl HeldStep {
    /// The name the step has in the replayed program.
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name {
            "press" => Some(Self::Press),
            "move" => Some(Self::Move),
            "hover" => Some(Self::Hover),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Move => "move",
            Self::Hover => "hover",
            Self::Release => "release",
        }
    }
}

/// The keysym that types `c`: Latin-1 keysyms are the character codes.
pub(super) fn char_keysym(c: char) -> Option<u32> {
    (c as u32 <= 0xff && !c.is_control()).then_some(c as u32)
}

/// The points a drag passes after the press, the last of them `to`.
fn drag_path(from: (u16, u16), to: (u16, u16)) -> Vec<(u16, u16)> {
    const STEP: f64 = 8.0;
    let (dx, dy) = (
        f64::from(to.0) - f64::from(from.0),
        f64::from(to.1) - f64::from(from.1),
    );
    let steps = (dx.hypot(dy) / STEP).ceil().max(1.0) as u32;
    (1..=steps)
        .map(|i| {
            let t = f64::from(i) / f64::from(steps);
            (
                (f64::from(from.0) + dx * t).round() as u16,
                (f64::from(from.1) + dy * t).round() as u16,
            )
        })
        .collect()
}

/// The errors of a server (or relay) that has not started listening yet.
fn not_up_yet(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

/// Where acknowledgement number `n` (from 0) moves the pointer.
pub(super) fn ack_position(n: u32) -> (u16, u16) {
    (SCREEN - 2 + (n % 2) as u16, SCREEN - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_name_modifiers_then_a_key() {
        let c = Chord::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(c.modifiers, [CONTROL, SHIFT]);
        assert_eq!(c.key, u32::from(b'S'));
        assert_eq!(Chord::parse("s").unwrap().key, u32::from(b's'));
        assert_eq!(Chord::parse("S").unwrap().key, u32::from(b's'));
        assert_eq!(Chord::parse("Tab").unwrap().key, 0xff09);
        assert_eq!(Chord::parse("F5").unwrap().key, 0xffc2);
        assert_eq!(Chord::parse("Ctrl++").unwrap().key, u32::from(b'+'));
        assert_eq!(Chord::parse("+").unwrap().key, u32::from(b'+'));
    }

    #[test]
    fn unknown_keys_and_modifiers_are_errors() {
        assert!(Chord::parse("Hyper+A").is_err());
        assert!(Chord::parse("Ctrl+NoSuchKey").is_err());
    }

    #[test]
    fn a_drag_ends_where_it_was_told_to() {
        let path = drag_path((10, 10), (50, 10));
        assert_eq!(path.len(), 5);
        assert_eq!(path.first(), Some(&(18, 10)));
        assert_eq!(path.last(), Some(&(50, 10)));
        assert_eq!(drag_path((10, 10), (10, 10)), [(10, 10)]);
    }

    #[test]
    fn held_steps_go_by_their_names() {
        for step in [
            HeldStep::Press,
            HeldStep::Move,
            HeldStep::Hover,
            HeldStep::Release,
        ] {
            assert_eq!(HeldStep::parse(step.name()), Some(step));
        }
        assert_eq!(HeldStep::parse("click"), None);
    }

    #[test]
    fn only_latin_1_can_be_typed() {
        assert_eq!(char_keysym('a'), Some(0x61));
        assert_eq!(char_keysym('é'), Some(0xe9));
        assert_eq!(char_keysym('\n'), None);
        assert_eq!(char_keysym('þ'), Some(0xfe));
        assert_eq!(char_keysym('あ'), None);
    }

    #[test]
    fn acknowledgements_alternate_between_two_corner_pixels() {
        assert_eq!(ack_position(0), (398, 399));
        assert_eq!(ack_position(1), (399, 399));
        assert_eq!(ack_position(2), (398, 399));
    }
}
