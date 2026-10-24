//! A test that fakes a server on its own thread serves it while a
//! `TestWindow` waits for the tasks that talk to it: the task gets the
//! fake's answer, not its own timeout.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::time::{Duration, Instant};

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
