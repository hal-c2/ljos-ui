//! A page's DevTools, shown in another view.
//!
//! Chromium's DevTools front end is a page like any other, so the other
//! view shows it windowless as it shows every page; CEF cannot show its own
//! DevTools window windowless. The front end reaches the page it inspects
//! over a WebSocket on loopback, at an address only that view is given: an
//! unguessable path, asked for from a DevTools page. What it sends goes to
//! the page as the view's own DevTools messages do, under ids of the
//! view's, so its answers come back to it and the view's to the view.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};

/// What the front end can have waiting to be written before it is taken
/// to have stopped reading: it is let go then, rather than the page's
/// messages piling up.
const BACKLOG: usize = 4096;

/// The largest message the front end may send: far over any DevTools
/// command, far under what would hurt to hold.
const LARGEST: u64 = 64 << 20;

/// The most an opening request may take: anyone on the machine can
/// connect before showing it knows the address, so what it sends is
/// bounded before then too.
const OPENING: u64 = 16 << 10;

/// How long an opening request may take, all of it.
const OPENING_TIME: Duration = Duration::from_secs(5);

/// How many connections may be opening at once: more are let go at once,
/// so a loop of connections holds no more threads than this.
const OPENINGS: usize = 8;

/// RFC 6455's key suffix.
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const TEXT: u8 = 0x1;
const BINARY: u8 = 0x2;
const CLOSE: u8 = 0x8;
const PING: u8 = 0x9;
const PONG: u8 = 0xA;

/// Domains the view's own messages need: the front end turning one off
/// would turn it off for the view too, so it is told that it did instead.
const KEPT_ON: [&str; 2] = ["Runtime.disable", "Page.disable"];

pub(super) struct Inspector {
    port: u16,
    token: String,
    stop: AtomicBool,
    /// Connections whose opening request is still being read.
    opening: AtomicUsize,
    link: Mutex<Link>,
    /// Has the browser's thread take what the front end sent, and see
    /// that one went.
    wake: Box<dyn Fn() + Send + Sync>,
}

#[derive(Default)]
struct Link {
    /// Which connection is the front end's now: one that went is not.
    serial: u64,
    out: Option<SyncSender<(u8, Vec<u8>)>>,
    socket: Option<TcpStream>,
    /// What the front end sent, for the browser's thread to pass on.
    inbox: VecDeque<Vec<u8>>,
    /// The id the page answers with, and the one the front end asked with.
    asked: HashMap<i32, Json>,
    /// What the front end now connected changed on the page.
    touched: BTreeSet<String>,
    /// What front ends that went changed, for the browser's thread to undo
    /// before it passes on anything the next one sent.
    left: BTreeSet<String>,
}

/// What the front end sent since the browser's thread last looked.
pub(super) struct Taken {
    /// What front ends that went left changed: see [`undo`].
    pub(super) left: BTreeSet<String>,
    /// For the page, in order, after `left` is undone.
    pub(super) messages: Vec<Vec<u8>>,
}

/// What a front end changes on the page and how it is put back, by the
/// method it changes it with. Chromium's own front ends each have a
/// session of their own, which takes what it changed with it as it
/// detaches; this one shares the view's session, which stays.
const UNDONE: [(&str, &[(&str, &str)]); 13] = [
    (
        "Overlay.",
        &[
            (
                "Overlay.setInspectMode",
                r#"{"mode":"none","highlightConfig":{}}"#,
            ),
            ("Overlay.hideHighlight", "{}"),
        ],
    ),
    (
        "Emulation.setDeviceMetricsOverride",
        &[("Emulation.clearDeviceMetricsOverride", "{}")],
    ),
    (
        "Emulation.setEmulatedMedia",
        &[(
            "Emulation.setEmulatedMedia",
            r#"{"media":"","features":[]}"#,
        )],
    ),
    (
        "Emulation.setTouchEmulationEnabled",
        &[("Emulation.setTouchEmulationEnabled", r#"{"enabled":false}"#)],
    ),
    (
        "Emulation.setEmitTouchEventsForMouse",
        &[(
            "Emulation.setEmitTouchEventsForMouse",
            r#"{"enabled":false}"#,
        )],
    ),
    (
        "Emulation.setCPUThrottlingRate",
        &[("Emulation.setCPUThrottlingRate", r#"{"rate":1}"#)],
    ),
    (
        "Emulation.setGeolocationOverride",
        &[("Emulation.clearGeolocationOverride", "{}")],
    ),
    (
        "Emulation.setScriptExecutionDisabled",
        &[("Emulation.setScriptExecutionDisabled", r#"{"value":false}"#)],
    ),
    (
        "Network.setCacheDisabled",
        &[("Network.setCacheDisabled", r#"{"cacheDisabled":false}"#)],
    ),
    (
        "Network.emulateNetworkConditions",
        &[(
            "Network.emulateNetworkConditions",
            r#"{"offline":false,"latency":0,"downloadThroughput":-1,"uploadThroughput":-1}"#,
        )],
    ),
    ("Debugger.", &[("Debugger.resume", "{}")]),
    // The front end has workers and frames of the page attached to it,
    // each held at its start until it says go on: with nobody left to
    // say it, a worker the page starts after would never run.
    (
        "Target.setAutoAttach",
        &[(
            "Target.setAutoAttach",
            r#"{"autoAttach":false,"waitForDebuggerOnStart":false,"flatten":true}"#,
        )],
    ),
    (
        "Target.setDiscoverTargets",
        &[("Target.setDiscoverTargets", r#"{"discover":false}"#)],
    ),
];

/// Methods that override the user agent: the view's own override, if it
/// has one, is put back after them (by the caller, who knows it).
pub(super) const AGENT: [&str; 2] = [
    "Emulation.setUserAgentOverride",
    "Network.setUserAgentOverride",
];

/// The calls that put back what a front end that went changed: what it
/// set first, then every domain it turned on that the view does not keep
/// on itself, so the page stops paying for events nobody hears.
pub(super) fn undo(left: &BTreeSet<String>) -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for (by, calls) in UNDONE {
        if left.iter().any(|m| m.starts_with(by)) {
            for (method, params) in calls {
                let params = serde_json::from_str(params).unwrap_or_else(|_| json!({}));
                out.push(((*method).to_owned(), params));
            }
        }
    }
    for method in left {
        if let Some(domain) = method.strip_suffix(".enable")
            && !KEPT_ON.contains(&format!("{domain}.disable").as_str())
        {
            out.push((format!("{domain}.disable"), json!({})));
        }
    }
    out
}

impl Inspector {
    /// Listens for the front end; `wake` has the browser's thread take
    /// what it sends (see [`Inspector::take`]).
    pub(super) fn start(wake: impl Fn() + Send + Sync + 'static) -> io::Result<Arc<Self>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        let me = Arc::new(Self {
            port,
            token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
            stop: AtomicBool::new(false),
            opening: AtomicUsize::new(0),
            link: Mutex::default(),
            wake: Box::new(wake),
        });
        let serving = Arc::clone(&me);
        std::thread::Builder::new()
            .name("ljos-devtools".to_owned())
            .spawn(move || serving.serve(&listener))?;
        Ok(me)
    }

    /// The front end's address, connected to this page.
    pub(super) fn address(&self) -> String {
        format!(
            "devtools://devtools/bundled/inspector.html?ws=127.0.0.1:{}/{}",
            self.port, self.token
        )
    }

    /// Ends the front end's connection and stops listening.
    pub(super) fn close(&self) {
        if self.stop.swap(true, Ordering::AcqRel) {
            return;
        }
        self.drop_link(None);
        // The listening thread waits in `accept`: one more connection
        // wakes it to see that it is to stop.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }

    /// What the front end sent, as messages for the page: `next_id` gives
    /// each the id the page answers it with.
    /// What front ends that went left is taken under the same lock, so
    /// none of it is the next one's.
    pub(super) fn take(&self, mut next_id: impl FnMut() -> i32) -> Taken {
        let mut link = self.lock();
        let inbox = std::mem::take(&mut link.inbox);
        let left = std::mem::take(&mut link.left);
        let mut out = Vec::new();
        for message in inbox {
            // Anyone holding the address may send anything: only an object
            // is a command.
            let Ok(mut message @ Json::Object(_)) = serde_json::from_slice::<Json>(&message) else {
                continue;
            };
            let theirs = message["id"].clone();
            let method = message["method"].as_str().unwrap_or("");
            if KEPT_ON.contains(&method) {
                let answer = json!({ "id": theirs, "result": {} });
                link.send(TEXT, serde_json::to_vec(&answer).unwrap_or_default());
                continue;
            }
            if !link.touched.contains(method) {
                link.touched.insert(method.to_owned());
            }
            let id = next_id();
            message["id"] = json!(id);
            link.asked.insert(id, theirs);
            out.push(serde_json::to_vec(&message).unwrap_or_default());
        }
        Taken {
            left,
            messages: out,
        }
    }

    /// What front ends that went left changed, once the inspector is
    /// closed and nothing more is taken.
    pub(super) fn left(&self) -> BTreeSet<String> {
        std::mem::take(&mut self.lock().left)
    }

    /// A message from the page: the front end hears its events, and the
    /// answers to what it asked, which are then its alone (`true`).
    pub(super) fn heard(&self, message: &[u8]) -> bool {
        let mut link = self.lock();
        if link.out.is_none() {
            return false;
        }
        let id = serde_json::from_slice::<Json>(message)
            .ok()
            .and_then(|m| m["id"].as_i64().map(|id| (id, m)));
        match id {
            None => {
                link.send(TEXT, message.to_vec());
                false
            }
            Some((id, mut message)) => {
                let Some(theirs) = i32::try_from(id).ok().and_then(|id| link.asked.remove(&id))
                else {
                    return false;
                };
                message["id"] = theirs;
                link.send(TEXT, serde_json::to_vec(&message).unwrap_or_default());
                true
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Link> {
        self.link.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets the front end's connection go: the one numbered `serial`, or
    /// whichever it is.
    fn drop_link(&self, serial: Option<u64>) {
        let mut link = self.lock();
        if serial.is_some_and(|s| s != link.serial) {
            return;
        }
        link.went();
        link.out = None;
        link.asked.clear();
        link.inbox.clear();
        if let Some(socket) = link.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    fn serve(self: &Arc<Self>, listener: &TcpListener) {
        for stream in listener.incoming() {
            if self.stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(stream) = stream else { continue };
            if self.opening.fetch_add(1, Ordering::AcqRel) >= OPENINGS {
                self.opening.fetch_sub(1, Ordering::AcqRel);
                continue;
            }
            // Each connection on its own thread, so one that is still
            // talking, or never finishes asking, holds no later one up.
            // One front end at a time: the newest is the one shown, and
            // the one before is let go (see `converse`).
            let me = Arc::clone(self);
            let spawned = std::thread::Builder::new()
                .name("ljos-devtools-in".to_owned())
                .spawn(move || {
                    let reader = me.handshake(stream);
                    me.opening.fetch_sub(1, Ordering::AcqRel);
                    if let Some(reader) = reader {
                        me.converse(reader);
                    }
                });
            if spawned.is_err() {
                self.opening.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }

    /// The front end's opening request, answered: `None` when it is not
    /// the front end's, or not one for this page.
    fn handshake(&self, stream: TcpStream) -> Option<BufReader<Socket>> {
        // A connection that never finishes asking holds nobody up for long.
        let mut reader = BufReader::new(Socket {
            stream,
            deadline: Some(Instant::now() + OPENING_TIME),
        });
        let mut opening = (&mut reader).take(OPENING);
        let mut line = String::new();
        opening.read_line(&mut line).ok()?;
        let mut words = line.split_whitespace();
        let ours = words.next() == Some("GET")
            && words.next().and_then(|p| p.strip_prefix('/')) == Some(self.token.as_str());
        let (mut key, mut origin) = (None, None);
        loop {
            let mut header = String::new();
            // Past `OPENING` it reads as ended: a request that long is
            // nobody's to answer.
            match opening.read_line(&mut header) {
                Ok(0) | Err(_) => return None,
                Ok(_) if header.trim().is_empty() => break,
                Ok(_) => {
                    if let Some((name, value)) = header.split_once(':') {
                        let value = value.trim().to_owned();
                        match name.trim().to_ascii_lowercase().as_str() {
                            "sec-websocket-key" => key = Some(value),
                            "origin" => origin = Some(value),
                            _ => {}
                        }
                    }
                }
            }
        }
        // Only DevTools' own pages may connect: a page that learned the
        // address still cannot drive the one inspected.
        let from_dev_tools = origin.is_none_or(|o| o.starts_with("devtools://"));
        let stream = &mut reader.get_mut().stream;
        let Some(key) = key.filter(|_| ours && from_dev_tools) else {
            let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
            return None;
        };
        let accept = base64(
            &sha1_smol::Sha1::from(format!("{key}{GUID}"))
                .digest()
                .bytes(),
        );
        let answer = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        stream.write_all(answer.as_bytes()).ok()?;
        stream.set_read_timeout(None).ok()?;
        reader.get_mut().deadline = None;
        Some(reader)
    }

    /// Passes what the front end sends on until it goes.
    fn converse(&self, mut reader: BufReader<Socket>) {
        let stream = &reader.get_ref().stream;
        let (Ok(writing), Ok(socket)) = (stream.try_clone(), stream.try_clone()) else {
            return;
        };
        let (out, written) = sync_channel(BACKLOG);
        let serial = {
            let mut link = self.lock();
            // `close` may have let the link go between the handshake and
            // now: under the lock, it either has or will see this one.
            if self.stop.load(Ordering::Acquire) {
                return;
            }
            if let Some(old) = link.socket.take() {
                let _ = old.shutdown(Shutdown::Both);
            }
            // The one replaced went: the next take undoes what it changed
            // before anything this one sends.
            link.went();
            link.serial += 1;
            link.out = Some(out);
            link.socket = Some(socket);
            link.asked.clear();
            link.inbox.clear();
            link.serial
        };
        let writer = std::thread::Builder::new()
            .name("ljos-devtools-out".to_owned())
            .spawn(move || write_frames(writing, &written));
        if writer.is_ok() {
            // What a connection that was replaced still had on its way is
            // its own, not the new one's: it stops at the first of it.
            let mut ping = |payload| {
                let mut link = self.lock();
                if link.serial == serial {
                    link.send(PONG, payload);
                }
            };
            while let Ok(Some(payload)) = read_message(&mut reader, &mut ping) {
                {
                    let mut link = self.lock();
                    if link.serial != serial {
                        break;
                    }
                    link.inbox.push_back(payload);
                }
                (self.wake)();
            }
        }
        self.drop_link(Some(serial));
        // What it left the page in is undone on the browser's thread.
        (self.wake)();
    }
}

/// A connection, read until a deadline for all of it rather than for each
/// read: a peer sending a byte now and then cannot stretch it.
struct Socket {
    stream: TcpStream,
    deadline: Option<Instant>,
}

impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(deadline) = self.deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream.set_read_timeout(Some(left))?;
        }
        self.stream.read(buf)
    }
}

impl Link {
    /// The front end connected now went: what it changed is to be undone.
    fn went(&mut self) {
        let touched = std::mem::take(&mut self.touched);
        self.left.extend(touched);
    }

    fn send(&mut self, op: u8, payload: Vec<u8>) {
        let Some(out) = &self.out else { return };
        if let Err(TrySendError::Full(_)) = out.try_send((op, payload)) {
            // Behind by a whole backlog: it has lost what it would need.
            self.out = None;
            if let Some(socket) = self.socket.take() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
    }
}

fn write_frames(mut stream: TcpStream, written: &Receiver<(u8, Vec<u8>)>) {
    for (op, payload) in written {
        let mut head = vec![0x80 | op];
        match payload.len() {
            n @ 0..=125 => head.push(n as u8),
            n @ 126..=0xFFFF => {
                head.push(126);
                head.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                head.push(127);
                head.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        if stream.write_all(&head).is_err() || stream.write_all(&payload).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// The next whole message, its fragments joined: `None` once the front
/// end closes. A ping on the way, even between fragments, goes to `ping`.
fn read_message(
    reader: &mut impl Read,
    ping: &mut impl FnMut(Vec<u8>),
) -> io::Result<Option<Vec<u8>>> {
    let mut message: Option<Vec<u8>> = None;
    loop {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head)?;
        let (fin, op) = (head[0] & 0x80 != 0, head[0] & 0x0F);
        let length = match head[1] & 0x7F {
            126 => {
                let mut n = [0u8; 2];
                reader.read_exact(&mut n)?;
                u64::from(u16::from_be_bytes(n))
            }
            127 => {
                let mut n = [0u8; 8];
                reader.read_exact(&mut n)?;
                u64::from_be_bytes(n)
            }
            n => u64::from(n),
        };
        // A length near u64::MAX would wrap the sum: checked, not added.
        let total = length.checked_add(message.as_ref().map_or(0, |m| m.len() as u64));
        if total.is_none_or(|total| total > LARGEST) {
            return Err(io::Error::other("a DevTools message too large to hold"));
        }
        let mut mask = [0u8; 4];
        if head[1] & 0x80 != 0 {
            reader.read_exact(&mut mask)?;
        }
        let mut payload = vec![0u8; length as usize];
        reader.read_exact(&mut payload)?;
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
        match op {
            CLOSE => return Ok(None),
            // Control frames come whole, even between a message's
            // fragments: the message goes on after them.
            PING => {
                ping(payload);
                continue;
            }
            PONG => continue,
            0 => match &mut message {
                Some(m) => m.extend_from_slice(&payload),
                None => return Err(io::Error::other("a continuation of nothing")),
            },
            TEXT | BINARY => message = Some(payload),
            _ => return Err(io::Error::other("a frame of no kind there is")),
        }
        if fin && let Some(message) = message.take() {
            return Ok(Some(message));
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_answers_as_rfc_6455_says() {
        // RFC 6455, section 1.3.
        let accept = base64(
            &sha1_smol::Sha1::from(format!("dGhlIHNhbXBsZSBub25jZQ=={GUID}"))
                .digest()
                .bytes(),
        );
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn a_masked_message_in_fragments_reads_whole() {
        let mask = [1u8, 2, 3, 4];
        let frame = |head: u8, text: &[u8]| {
            let mut f = vec![head, 0x80 | text.len() as u8];
            f.extend_from_slice(&mask);
            f.extend(text.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
            f
        };
        let mut bytes = frame(TEXT, b"{\"id\":");
        bytes.extend(frame(0x80 | PING, b"hi"));
        bytes.extend(frame(0x80, b"1}"));
        bytes.extend(frame(0x80 | CLOSE, b""));
        let mut reader = &bytes[..];
        let mut pinged = Vec::new();
        let mut ping = |p| pinged.push(p);
        assert_eq!(
            read_message(&mut reader, &mut ping).unwrap(),
            Some(b"{\"id\":1}".to_vec())
        );
        assert_eq!(read_message(&mut reader, &mut ping).unwrap(), None);
        assert_eq!(pinged, vec![b"hi".to_vec()]);
    }

    #[test]
    fn a_continuation_too_long_to_add_up_is_refused() {
        let mut bytes = vec![TEXT, 1, b'{'];
        bytes.extend([0x80, 127]);
        bytes.extend(u64::MAX.to_be_bytes());
        let mut reader = &bytes[..];
        assert!(read_message(&mut reader, &mut |_| {}).is_err());
    }

    #[test]
    fn the_front_end_hears_its_answers_under_its_own_ids() {
        let inspector = Inspector {
            port: 0,
            token: String::new(),
            stop: AtomicBool::new(false),
            opening: AtomicUsize::new(0),
            link: Mutex::default(),
            wake: Box::new(|| {}),
        };
        let (out, written) = sync_channel(8);
        {
            let mut link = inspector.lock();
            link.out = Some(out);
            link.inbox
                .push_back(br#"{"id":7,"method":"DOM.enable"}"#.to_vec());
            link.inbox
                .push_back(br#"{"id":8,"method":"Runtime.disable"}"#.to_vec());
        }
        let sent = inspector.take(|| 41);
        assert!(sent.left.is_empty());
        assert_eq!(
            sent.messages,
            vec![br#"{"id":41,"method":"DOM.enable"}"#.to_vec()]
        );
        // Turning off what the view needs is answered, not done.
        assert_eq!(
            written.try_recv().unwrap().1,
            br#"{"id":8,"result":{}}"#.to_vec()
        );
        // The view's own answers stay the view's.
        assert!(!inspector.heard(br#"{"id":40,"result":{}}"#));
        assert!(inspector.heard(br#"{"id":41,"result":{}}"#));
        assert_eq!(
            written.try_recv().unwrap().1,
            br#"{"id":7,"result":{}}"#.to_vec()
        );
        // Events go to both.
        assert!(!inspector.heard(br#"{"method":"Page.loadEventFired"}"#));
        assert!(written.try_recv().is_ok());
        // A front end that goes leaves what it changed to be undone, once,
        // and before what the next one sends, in the same take.
        let serial = inspector.lock().serial;
        inspector.drop_link(Some(serial));
        let (out, _written) = sync_channel(8);
        {
            let mut link = inspector.lock();
            link.out = Some(out);
            link.inbox
                .push_back(br#"{"id":1,"method":"Debugger.enable"}"#.to_vec());
            // Not a command: let go, not indexed.
            link.inbox.push_back(b"[]".to_vec());
        }
        let taken = inspector.take(|| 42);
        assert_eq!(taken.left, BTreeSet::from(["DOM.enable".to_owned()]));
        assert_eq!(taken.messages.len(), 1);
        assert!(inspector.take(|| 43).left.is_empty());
        // The inspector closing is the browser thread's own doing: what
        // the one connected then changed is left for it too.
        inspector.drop_link(None);
        assert_eq!(
            inspector.left(),
            BTreeSet::from(["Debugger.enable".to_owned()])
        );
    }

    #[test]
    fn what_a_front_end_changed_is_put_back_after_it() {
        let left = BTreeSet::from(
            [
                "Overlay.enable",
                "Overlay.highlightNode",
                "Emulation.setDeviceMetricsOverride",
                "Network.setCacheDisabled",
                "Debugger.enable",
                "Debugger.setBreakpointByUrl",
                "Target.setAutoAttach",
                "Runtime.enable",
                "DOM.getDocument",
            ]
            .map(str::to_owned),
        );
        let undone: Vec<String> = undo(&left).into_iter().map(|(m, _)| m).collect();
        assert_eq!(
            undone,
            [
                "Overlay.setInspectMode",
                "Overlay.hideHighlight",
                "Emulation.clearDeviceMetricsOverride",
                "Network.setCacheDisabled",
                "Debugger.resume",
                "Target.setAutoAttach",
                "Debugger.disable",
                "Overlay.disable",
            ]
        );
        assert!(undo(&BTreeSet::new()).is_empty());
    }

    #[test]
    fn an_opening_request_has_one_deadline_for_all_of_it() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        peer.write_all(b"GET").unwrap();
        let mut socket = Socket {
            stream,
            deadline: Some(Instant::now() + Duration::from_millis(200)),
        };
        let mut buf = [0u8; 8];
        assert_eq!(socket.read(&mut buf).unwrap(), 3);
        // Past the deadline, even a peer that is still sending is not read.
        std::thread::sleep(Duration::from_millis(250));
        peer.write_all(b" /").unwrap();
        let err = socket.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }
}
