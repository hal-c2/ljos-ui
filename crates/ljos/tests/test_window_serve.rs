//! A test that fakes a server on its own thread serves it while a
//! `TestWindow` waits for the tasks that talk to it: the task gets the
//! fake's answer, not its own timeout.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::Arc;
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

#[test]
fn advance_serves_the_fake_while_a_picture_is_fetched() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let png = ljos_render::RgbaImage::filled(3, 2, [255, 0, 0, 255])
        .to_png()
        .unwrap();
    let engine = ljos::engine();
    Platform::of(&engine).set_fetcher(Arc::new(Get));
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    window.serve_while_waiting(move || {
        while let Ok((mut s, _)) = listener.accept() {
            s.set_nonblocking(false).unwrap();
            let mut head = [0u8; 1024];
            let _ = s.read(&mut head);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                png.len()
            );
            let _ = s.write_all(&png);
        }
    });
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
    let start = Instant::now();
    window.show(image).unwrap();
    window.advance(1).unwrap();
    // Image.Ready
    assert_eq!(engine.read_by_name(image, "status"), Value::Int(1));
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
}
