//! A `Task` waiting on its run costs the engine nothing: no timer, no
//! frame, no poll. The reply alone wakes the host, and that one wake
//! settles the run, the Task's state and what binds to it (ARCHITECTURE.md
//! rule 6, docs/DIALECT.md §4.19). `features/components/task.feature` runs
//! on a virtual clock, which cannot see a poll; this runs on a real socket.
#![cfg(feature = "http")]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
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

/// A server on 127.0.0.1 that answers `/<name>` with the text `name`, but
/// only once told to answer that name.
struct Server {
    url: String,
    releases: HashMap<&'static str, mpsc::Sender<()>>,
}

impl Server {
    fn new(names: &[&'static str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let url = format!("http://{}", listener.local_addr().expect("its address"));
        let mut releases = HashMap::new();
        let mut held = HashMap::new();
        for name in names {
            let (release, released) = mpsc::channel::<()>();
            releases.insert(*name, release);
            held.insert(name.to_string(), released);
        }
        let held = Arc::new(Mutex::new(held));
        std::thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { return };
                let held = held.clone();
                std::thread::spawn(move || {
                    let mut seen = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => seen.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&seen);
                    let name = head
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .trim_start_matches('/')
                        .to_string();
                    let released = held.lock().expect("the held names").remove(&name);
                    let Some(released) = released else { return };
                    if released.recv().is_err() {
                        return;
                    }
                    let _ = socket.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                             Content-Length: {}\r\nConnection: close\r\n\r\n{name}",
                            name.len()
                        )
                        .as_bytes(),
                    );
                });
            }
        });
        Server { url, releases }
    }

    fn release(&self, name: &str) {
        self.releases[name].send(()).expect("the server waits");
    }
}

/// An engine whose platform fetches over HTTP, and the count of what woke
/// the host or asked the engine for a frame.
struct Host {
    e: Engine,
    wakes: Arc<AtomicUsize>,
    woken: mpsc::Receiver<()>,
    frames: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Host {
    fn new() -> Self {
        let e = Engine::new();
        ljos_quick::register(&e);
        let platform = Platform::of(&e);
        platform.set_fetcher(Arc::new(HttpFetcher::new()));
        let wakes = Arc::new(AtomicUsize::new(0));
        let (woke, woken) = mpsc::channel::<()>();
        {
            let wakes = wakes.clone();
            let woke = Mutex::new(woke);
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
        Host {
            e,
            wakes,
            woken,
            frames,
        }
    }

    /// Nothing wakes the host for long enough that a poll or a timer would
    /// show itself, and nothing is due.
    fn sleeps(&self, why: &str) {
        assert!(
            self.woken.recv_timeout(Duration::from_millis(300)).is_err(),
            "nothing wakes the host while {why}"
        );
        assert_eq!(self.e.next_timer_deadline(), None, "no timer while {why}");
        assert_eq!(self.frames.get(), 0, "no frame asked for while {why}");
    }

    /// The host's one turn for a reply's wake.
    fn turn(&self) {
        self.woken
            .recv_timeout(Duration::from_secs(10))
            .expect("the reply wakes the host");
        assert_eq!(
            Platform::of(&self.e).deliver(&self.e),
            1,
            "one job: the reply"
        );
        self.e.run_pending_calls();
    }

    fn text(&self, obj: ObjectId, name: &str) -> String {
        self.e
            .read_by_name(obj, name)
            .to_js_string()
            .as_str()
            .to_string()
    }
}

#[test]
fn a_pending_task_schedules_nothing_and_its_reply_alone_settles_it() {
    let server = Server::new(&["book"]);
    let host = Host::new();
    let root = load(
        &host.e,
        &"import Ljos.Core
import Ljos.Ui
Item {
    id: root
    property string name: \"book\"
    Task {
        id: task
        args: [root.name]
        run: async (name: string, signal: AbortSignal): Promise<string> =>
            (await fetch(`URL/${name}`, { signal })).text()
    }
    property string shown: task.status + \" \" + (task.value ?? \"none\")
}
"
        .replace("URL", &server.url),
    );
    host.e.run_pending_calls();
    host.frames.set(0);

    assert_eq!(Platform::of(&host.e).pending(), 1, "the request is out");
    assert_eq!(host.text(root, "shown"), "pending none");
    host.sleeps("the run waits on its reply");
    assert_eq!(host.wakes.load(Ordering::SeqCst), 0);

    server.release("book");
    host.turn();
    assert_eq!(host.text(root, "shown"), "done book");
    assert_eq!(
        host.wakes.load(Ordering::SeqCst),
        1,
        "one wake: the reply's"
    );
    assert_eq!(Platform::of(&host.e).pending(), 0);
    assert_eq!(
        host.e.next_timer_deadline(),
        None,
        "and the engine sleeps again"
    );
    assert!(host.e.take_warnings().is_empty());
}

#[test]
fn a_superseded_run_is_aborted_and_what_its_reply_gives_is_dropped() {
    let server = Server::new(&["first", "second"]);
    let host = Host::new();
    // The request is not given the signal, so the superseded one is
    // answered all the same: what it gives must be dropped.
    let root = load(
        &host.e,
        &"import Ljos.Core
import Ljos.Ui
Item {
    id: root
    property string name: \"first\"
    property int aborts: 0
    property string said: \"\"
    Task {
        id: task
        args: [root.name]
        run: async (name: string, signal: AbortSignal): Promise<string> => {
            signal.onabort = () => { root.aborts = root.aborts + 1 }
            return (await fetch(`URL/${name}`)).text()
        }
        onDone: (value: string) => { root.said = root.said + value + \";\" }
    }
    property string shown: task.status + \" \" + (task.value ?? \"none\")
}
"
        .replace("URL", &server.url),
    );
    host.e.run_pending_calls();
    host.e
        .write_by_name(root, "name", "second".into())
        .expect("name is writable");
    host.e.run_pending_calls();
    host.frames.set(0);

    assert_eq!(host.text(root, "aborts"), "1", "the first run is aborted");
    assert_eq!(Platform::of(&host.e).pending(), 2, "both requests are out");
    host.sleeps("two runs wait on their replies");

    server.release("first");
    host.turn();
    assert_eq!(
        host.text(root, "shown"),
        "pending none",
        "the first is dropped"
    );
    assert_eq!(host.text(root, "said"), "");

    server.release("second");
    host.turn();
    assert_eq!(host.text(root, "shown"), "done second");
    assert_eq!(host.text(root, "said"), "second;");
    assert_eq!(
        host.wakes.load(Ordering::SeqCst),
        2,
        "a wake for each reply"
    );
    assert_eq!(host.e.next_timer_deadline(), None);
    assert!(host.e.take_warnings().is_empty());
}

#[test]
fn destroying_the_host_aborts_the_run_and_its_request() {
    let server = Server::new(&["held"]);
    let host = Host::new();
    let root = load(
        &host.e,
        &"import Ljos.Core
import Ljos.Ui
Item {
    id: root
    property int aborts: 0
    property Item page: inner
    Item {
        id: inner
        Task {
            run: async (signal: AbortSignal): Promise<string> => {
                signal.onabort = () => { root.aborts = root.aborts + 1 }
                return (await fetch(`URL/held`, { signal })).text()
            }
        }
    }
}
"
        .replace("URL", &server.url),
    );
    host.e.run_pending_calls();
    assert_eq!(Platform::of(&host.e).pending(), 1, "the request is out");

    let page = host
        .e
        .read_by_name(root, "page")
        .as_object()
        .expect("the page");
    assert!(host.e.destroy(page));
    host.e.process_deferred_deletes();
    host.e.run_pending_calls();
    host.frames.set(0);

    assert_eq!(host.text(root, "aborts"), "1", "the run is aborted");
    assert_eq!(
        Platform::of(&host.e).pending(),
        0,
        "its request is given up"
    );
    host.sleeps("nothing is left to wait on");
    // The server is never told to answer: the request was given up.
    drop(server);
    assert_eq!(host.wakes.load(Ordering::SeqCst), 0);
    assert!(host.e.take_warnings().is_empty());
}
