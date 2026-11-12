//! A `fetch` on its way costs the engine nothing: no timer, no frame, no
//! poll. Its reply alone wakes the host, and that one wake settles the
//! promise and runs what waited on it (ARCHITECTURE.md rule 6, docs/DIALECT.md
//! §4.19). The scenarios in `features/platform/fetch.feature` run on a
//! virtual clock, which cannot see a poll; this runs on a real socket.
#![cfg(feature = "http")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ljos_engine::{Engine, ObjectId};
use ljos_platform::{HttpFetcher, Platform};

fn load(e: &Engine, source: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

/// A server that answers its one request only when told to.
fn held_server() -> (String, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let url = format!(
        "http://{}/book",
        listener.local_addr().expect("its address")
    );
    let (release, released) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("the request");
        let mut seen = Vec::new();
        let mut buf = [0u8; 1024];
        while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut buf).expect("the request's head");
            if n == 0 {
                return;
            }
            seen.extend_from_slice(&buf[..n]);
        }
        if released.recv().is_err() {
            return;
        }
        let _ = socket.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
              Content-Length: 11\r\nConnection: close\r\n\r\nRead a book",
        );
    });
    (url, release)
}

#[test]
fn a_pending_fetch_schedules_nothing_and_its_reply_alone_wakes_the_engine() {
    let (url, release) = held_server();
    let e = Engine::new();
    ljos_quick::register(&e);
    let platform = Platform::of(&e);
    platform.set_fetcher(Arc::new(HttpFetcher::new()));

    let wakes = Arc::new(AtomicUsize::new(0));
    let (woke, woken) = mpsc::channel::<()>();
    {
        let wakes = wakes.clone();
        let woke = std::sync::Mutex::new(woke);
        platform.set_waker(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
            let _ = woke.lock().map(|w| w.send(()));
        });
    }
    let frames = std::rc::Rc::new(std::cell::Cell::new(0usize));
    {
        let frames = frames.clone();
        e.set_timer_wake(move || frames.set(frames.get() + 1));
    }

    let root = load(
        &e,
        &format!(
            "import QtQuick\n\
             Item {{\n    \
                 property string got: \"\"\n    \
                 Component.onCompleted: fetch(\"{url}\")\n        \
                     .then(reply => reply.text())\n        \
                     .then(text => {{ got = text }})\n\
             }}\n"
        ),
    );
    // What creating the item queued runs, as the host's first frame would.
    e.run_pending_calls();
    frames.set(0);

    assert_eq!(platform.pending(), 1, "the request is out");
    // Long enough for a poll or a timer to show itself.
    assert!(
        woken.recv_timeout(Duration::from_millis(300)).is_err(),
        "nothing wakes the host while the request is out"
    );
    assert_eq!(
        e.next_timer_deadline(),
        None,
        "no timer and no frame is due"
    );
    assert_eq!(frames.get(), 0, "nothing asked the engine for a frame");
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
    assert_eq!(e.read_by_name(root, "got").to_js_string().as_str(), "");

    release.send(()).expect("the server waits");
    woken
        .recv_timeout(Duration::from_secs(10))
        .expect("the reply wakes the host");
    // The host's one turn for that wake: the reply's job, then the promise
    // jobs it queued.
    assert_eq!(platform.deliver(&e), 1, "one job: the reply");
    e.run_pending_calls();
    assert_eq!(
        e.read_by_name(root, "got").to_js_string().as_str(),
        "Read a book"
    );
    assert_eq!(wakes.load(Ordering::SeqCst), 1, "one wake: the reply's");
    assert_eq!(platform.pending(), 0);
    assert_eq!(e.next_timer_deadline(), None, "and the engine sleeps again");
    assert!(e.take_warnings().is_empty());
}
