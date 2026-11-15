//! A page's DevTools, shown in another view.
//!
//! Chromium's DevTools front end is a page like any other, so the other
//! view shows it windowless as it shows every page; CEF cannot show its own
//! DevTools window windowless. The front end reaches the page it inspects
//! over a WebSocket on loopback, at an address only that view is given: an
//! unguessable path, asked for from a DevTools page. What it sends goes to
//! the page as the view's own DevTools messages do, under ids of the
//! view's, so its answers come back to it and the view's to the view.

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

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
    link: Mutex<Link>,
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
            link: Mutex::default(),
        });
        let serving = Arc::clone(&me);
        std::thread::Builder::new()
            .name("ljos-devtools".to_owned())
            .spawn(move || serving.serve(&listener, Arc::new(wake)))?;
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
    pub(super) fn take(&self, mut next_id: impl FnMut() -> i32) -> Vec<Vec<u8>> {
        let mut link = self.lock();
        let inbox = std::mem::take(&mut link.inbox);
        let mut out = Vec::new();
        for message in inbox {
            let Ok(mut message) = serde_json::from_slice::<Json>(&message) else {
                continue;
            };
            let theirs = message["id"].clone();
            let method = message["method"].as_str().unwrap_or("");
            if KEPT_ON.contains(&method) {
                let answer = json!({ "id": theirs, "result": {} });
                link.send(TEXT, serde_json::to_vec(&answer).unwrap_or_default());
                continue;
            }
            let id = next_id();
            message["id"] = json!(id);
            link.asked.insert(id, theirs);
            out.push(serde_json::to_vec(&message).unwrap_or_default());
        }
        out
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
        link.out = None;
        link.asked.clear();
        link.inbox.clear();
        if let Some(socket) = link.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    fn serve(self: &Arc<Self>, listener: &TcpListener, wake: Arc<dyn Fn() + Send + Sync>) {
        for stream in listener.incoming() {
            if self.stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(stream) = stream else { continue };
            // Each connection on its own thread, so one that is still
            // talking, or never finishes asking, holds no later one up.
            // One front end at a time: the newest is the one shown, and
            // the one before is let go (see `converse`).
            let (me, wake) = (Arc::clone(self), Arc::clone(&wake));
            let _ = std::thread::Builder::new()
                .name("ljos-devtools-in".to_owned())
                .spawn(move || {
                    if let Some(reader) = me.handshake(stream) {
                        me.converse(reader, &*wake);
                    }
                });
        }
    }

    /// The front end's opening request, answered: `None` when it is not
    /// the front end's, or not one for this page.
    fn handshake(&self, stream: TcpStream) -> Option<BufReader<TcpStream>> {
        // A connection that never asks holds nobody up for long.
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut reader = BufReader::new(stream);
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
        let stream = reader.get_mut();
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
        Some(reader)
    }

    /// Passes what the front end sends on until it goes.
    fn converse(&self, mut reader: BufReader<TcpStream>, wake: &dyn Fn()) {
        let (Ok(writing), Ok(socket)) =
            (reader.get_ref().try_clone(), reader.get_ref().try_clone())
        else {
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
            let mut ping = |payload| self.lock().send(PONG, payload);
            while let Ok(Some(payload)) = read_message(&mut reader, &mut ping) {
                self.lock().inbox.push_back(payload);
                wake();
            }
        }
        self.drop_link(Some(serial));
    }
}

impl Link {
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
        let total = length + message.as_ref().map_or(0, |m| m.len() as u64);
        if total > LARGEST {
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
    fn the_front_end_hears_its_answers_under_its_own_ids() {
        let inspector = Inspector {
            port: 0,
            token: String::new(),
            stop: AtomicBool::new(false),
            link: Mutex::default(),
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
        assert_eq!(sent, vec![br#"{"id":41,"method":"DOM.enable"}"#.to_vec()]);
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
    }
}
