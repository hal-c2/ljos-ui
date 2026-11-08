//! A test that fakes a server on its own thread serves it while a
//! `TestWindow` waits for the tasks that talk to it: the task gets the
//! fake's answer, not its own timeout.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ljos::prelude::*;
use ljos::task::spawn;
use ljos::testing::TestWindow;

/// Asks the fake once and reads what it says, giving up after 5 s.
fn ask(port: u16) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.write_all(b"ping\n")?;
    let mut reply = String::new();
    s.read_to_string(&mut reply)?;
    Ok(reply)
}

/// Answers each connection waiting on `listener`, without blocking.
fn serve(listener: &TcpListener) {
    while let Ok((mut s, _)) = listener.accept() {
        s.set_nonblocking(false).unwrap();
        let mut line = [0u8; 5];
        s.read_exact(&mut line).unwrap();
        s.write_all(b"pong").unwrap();
    }
}

#[test]
fn advance_serves_the_fake_while_it_waits() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&got);
    spawn(
        &engine,
        move || ask(port).map_err(|e| e.to_string()),
        move |_, r| *seen.borrow_mut() = Some(r.unwrap()),
    )
    .unwrap();
    window.serve_while_waiting(move || serve(&listener));
    let start = Instant::now();
    window.advance(1).unwrap();
    assert_eq!(*got.borrow(), Some(Ok("pong".to_owned())));
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
}

/// A blocking HTTP/1.1 GET, so that the picture's fetch waits on the
/// fake the test serves on its own thread.
struct Get;

impl Fetcher for Get {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let failed = |e: std::io::Error| FetchError::Failed(e.to_string());
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut s = TcpStream::connect(host).map_err(failed)?;
        write!(
            s,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .map_err(failed)?;
        let mut reply = Vec::new();
        s.read_to_end(&mut reply).map_err(failed)?;
        let at = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        Ok(reply[at + 4..].to_vec())
    }
}

/// Answers one GET on `s` with `body`, once the whole head is in. A
/// request may come in several reads (`write!` alone sends it a piece at
/// a time), and a socket closed with bytes of it unread is reset: the
/// client then takes the answer as an error however much of it it had.
fn answer(s: &mut (impl Read + Write), body: &[u8]) {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let mut reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    reply.extend_from_slice(body);
    let _ = s.write_all(&reply);
}

/// A red 3×2 PNG.
fn png() -> Vec<u8> {
    ljos_render::RgbaImage::filled(3, 2, [255, 0, 0, 255])
        .to_png()
        .unwrap()
}

/// An asynchronous Image of `a.png` on the fake at `port`.
fn picture(engine: &Engine, port: u16) -> ObjectId {
    let image = engine.create_by_name("QtQuick", "Image", None).unwrap();
    engine
        .write_by_name(image, "asynchronous", Value::Bool(true))
        .unwrap();
    engine
        .write_by_name(
            image,
            "source",
            Value::from(format!("http://127.0.0.1:{port}/a.png").as_str()),
        )
        .unwrap();
    image
}

/// `image` is Image.Ready; if not, the failure says what the loader
/// logged, which is why.
fn assert_ready(engine: &Engine, image: ObjectId) {
    assert_eq!(
        engine.read_by_name(image, "status"),
        Value::Int(1),
        "status (1 is Image.Ready, 3 Image.Error); logged: {:?}",
        engine.take_warnings()
    );
}

#[test]
fn advance_serves_the_fake_while_a_picture_is_fetched() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let png = png();
    let engine = ljos::engine();
    Platform::of(&engine).set_fetcher(Arc::new(Get));
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    window.serve_while_waiting(move || {
        while let Ok((mut s, _)) = listener.accept() {
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            answer(&mut s, &png);
        }
    });
    let image = picture(&engine, port);
    let start = Instant::now();
    window.show(image).unwrap();
    window.advance(1).unwrap();
    assert_ready(&engine, image);
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
}

/// A GET that leaves in two parts, the second only once the fake has
/// read the first, as a client's writes may reach a server.
struct InParts {
    read_first: Mutex<Receiver<()>>,
    sent_rest: Sender<()>,
}

impl Fetcher for InParts {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let failed = |e: std::io::Error| FetchError::Failed(e.to_string());
        let host = url.strip_prefix("http://").unwrap().split('/').next();
        let mut s = TcpStream::connect(host.unwrap()).map_err(failed)?;
        s.write_all(b"GET /a.png HTTP/1.1\r\n").map_err(failed)?;
        let read = self
            .read_first
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5));
        read.map_err(|e| FetchError::Failed(e.to_string()))?;
        s.write_all(b"Host: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .map_err(failed)?;
        let _ = self.sent_rest.send(());
        let mut reply = Vec::new();
        s.read_to_end(&mut reply).map_err(failed)?;
        let at = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        Ok(reply[at + 4..].to_vec())
    }
}

/// The fake's side of a connection whose first read is all it gets
/// until the rest of the request has been sent and is waiting there.
struct Parted {
    s: TcpStream,
    first: Option<(Sender<()>, Receiver<()>)>,
}

impl Read for Parted {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.s.read(buf)?;
        if let Some((read_first, sent_rest)) = self.first.take() {
            let _ = read_first.send(());
            let _ = sent_rest.recv_timeout(Duration::from_secs(5));
            // Blocks until a byte the fake has not read is there, so a
            // fake that stops reading now leaves it unread.
            self.s.peek(&mut [0u8; 1])?;
        }
        Ok(n)
    }
}

impl Write for Parted {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.s.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.s.flush()
    }
}

#[test]
fn a_picture_whose_request_arrives_in_parts_is_ready() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let png = png();
    let (read_first, read_first_rx) = channel();
    let (sent_rest, sent_rest_rx) = channel();
    let engine = ljos::engine();
    Platform::of(&engine).set_fetcher(Arc::new(InParts {
        read_first: Mutex::new(read_first_rx),
        sent_rest,
    }));
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let mut first = Some((read_first, sent_rest_rx));
    window.serve_while_waiting(move || {
        while let Ok((s, _)) = listener.accept() {
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let first = first.take();
            answer(&mut Parted { s, first }, &png);
        }
    });
    let image = picture(&engine, port);
    window.show(image).unwrap();
    window.advance(1).unwrap();
    assert_ready(&engine, image);
}
