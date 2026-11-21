//! The lifecycle a relayed platform (Android) puts the application
//! through, driven from a thread that stands for the platform's.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

#[derive(Debug, PartialEq)]
enum Msg {
    Wake,
    Stop,
    Ping,
}

impl From<Wake> for Msg {
    fn from(_: Wake) -> Self {
        Self::Wake
    }
}

type Log = Arc<Mutex<Vec<String>>>;
/// What the app does when its first surface comes.
type OnFirst = Box<dyn FnMut(&mut RelayControl<'_, u32>)>;

/// Writes down what it is handed, makes its window at the first surface
/// and stops at [`Msg::Stop`].
struct App {
    log: Log,
    surfaces: u32,
    on_first: Option<OnFirst>,
    /// Stops once it has drawn on this surface (counted from 1).
    stop_when_drawn_on: Option<u32>,
    /// How long it takes to let go of a surface.
    letting_go: Duration,
}

impl App {
    fn new(log: &Log) -> Self {
        Self {
            log: Arc::clone(log),
            surfaces: 0,
            on_first: None,
            stop_when_drawn_on: None,
            letting_go: Duration::ZERO,
        }
    }
}

impl HostApp<RelayHost<u32, Msg>> for App {
    fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
        let line = match &event {
            Event::SurfaceCreated => {
                self.surfaces += 1;
                if self.surfaces == 1 {
                    host.create_window(&WindowAttributes::default()).unwrap();
                }
                format!("surface {} (#{})", host.surface().unwrap(), self.surfaces)
            }
            // What the application made on the surface goes here.
            Event::SurfaceDestroyed => {
                std::thread::sleep(self.letting_go);
                String::from("let go of the surface")
            }
            Event::Window {
                event: WindowEvent::Resized { px, .. },
                ..
            } => format!("resized {}x{}", px[0], px[1]),
            Event::Window {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if self.stop_when_drawn_on == Some(self.surfaces) {
                    host.exit();
                }
                String::from("drawn")
            }
            Event::Window { event, .. } => format!("{event:?}"),
            Event::User(Msg::Stop) => {
                host.exit();
                String::from("stop")
            }
            Event::User(Msg::Wake) => String::from("woken"),
            Event::AboutToWait => {
                if let Some(mut f) = self.on_first.take() {
                    f(host);
                }
                return;
            }
            Event::Exiting => String::from("exiting"),
            other => format!("{other:?}"),
        };
        self.log.lock().unwrap().push(line);
    }
}

fn host(requests: &Arc<Mutex<Vec<Request>>>) -> (RelayHost<u32, Msg>, Relay<u32, Msg>) {
    let requests = Arc::clone(requests);
    RelayHost::new(Capabilities::default(), move |r| {
        requests.lock().unwrap().push(r);
    })
}

fn lines(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[test]
fn the_platform_frees_a_surface_only_after_the_application_let_go_of_it() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let platform = {
        let log = Arc::clone(&log);
        std::thread::spawn(move || {
            relay
                .send(Arrival::SurfaceCreated {
                    surface: 7,
                    px: [1080, 2400],
                    scale: 2.75,
                })
                .unwrap();
            relay.send_and_wait(Arrival::SurfaceDestroyed).unwrap();
            log.lock().unwrap().push(String::from("platform freed it"));
            relay.post(Msg::Stop).unwrap();
        })
    };
    // Long enough that a platform told too soon writes its line first.
    let app = App {
        letting_go: Duration::from_millis(100),
        ..App::new(&log)
    };
    host.run(app).unwrap();
    platform.join().unwrap();
    let lines = lines(&log);
    let let_go = lines.iter().position(|l| l == "let go of the surface");
    let freed = lines.iter().position(|l| l == "platform freed it");
    assert!(let_go.is_some() && let_go < freed, "{lines:?}");
}

#[test]
fn the_application_outlives_the_platform_loop() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let platform = std::thread::spawn(move || {
        // The first activity.
        relay
            .send(Arrival::SurfaceCreated {
                surface: 1,
                px: [1080, 2400],
                scale: 2.75,
            })
            .unwrap();
        relay.send_and_wait(Arrival::SurfaceDestroyed).unwrap();
        relay.send(Arrival::Detached).unwrap();
        // The next one, on another thread of the same process.
        let relay = relay.clone();
        std::thread::spawn(move || {
            relay
                .send(Arrival::SurfaceCreated {
                    surface: 2,
                    px: [1080, 2400],
                    scale: 2.75,
                })
                .unwrap();
        })
        .join()
        .unwrap();
    });
    // It stops itself once it has drawn on the new surface: a stop the
    // platform posted could come before that frame.
    let app = App {
        stop_when_drawn_on: Some(2),
        ..App::new(&log)
    };
    host.run(app).unwrap();
    platform.join().unwrap();
    let lines = lines(&log);
    assert!(lines.contains(&String::from("surface 1 (#1)")), "{lines:?}");
    assert!(
        lines.contains(&String::from("AppState(Suspended)")),
        "{lines:?}"
    );
    // The same application, its window kept, draws into the new surface.
    assert!(lines.contains(&String::from("surface 2 (#2)")), "{lines:?}");
    let second = lines.iter().position(|l| l == "surface 2 (#2)").unwrap();
    assert!(
        lines[second..].contains(&String::from("drawn")),
        "{lines:?}"
    );
    assert_eq!(*requests.lock().unwrap(), [Request::Exit]);
}

#[test]
fn nothing_is_drawn_without_a_surface() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let platform = std::thread::spawn(move || {
        relay
            .send(Arrival::SurfaceCreated {
                surface: 1,
                px: [100, 200],
                scale: 1.0,
            })
            .unwrap();
        relay.send_and_wait(Arrival::SurfaceDestroyed).unwrap();
        // A wake asks for a frame: there is nowhere to draw it.
        relay.post(Msg::Wake).unwrap();
        relay.post(Msg::Stop).unwrap();
    });
    host.run(App::new(&log)).unwrap();
    platform.join().unwrap();
    let lines = lines(&log);
    let drawn: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| *l == "drawn")
        .map(|(i, _)| i)
        .collect();
    let gone = lines
        .iter()
        .position(|l| l == "let go of the surface")
        .unwrap();
    assert!(drawn.iter().all(|i| *i < gone), "{lines:?}");
}

#[test]
fn sizes_that_arrive_together_reach_the_window_as_the_last_one() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    // Queued before the loop runs, so they are handled in one turn.
    relay
        .send(Arrival::SurfaceCreated {
            surface: 1,
            px: [1080, 2400],
            scale: 2.75,
        })
        .unwrap();
    for px in [[2400, 1080], [1080, 2400], [2400, 1080]] {
        relay
            .send(Arrival::Window(WindowEvent::Resized { px, scale: 2.75 }))
            .unwrap();
    }
    // Stopped after the turn, once the window has been told: a stop among
    // them would end the loop before it is.
    let app = App {
        on_first: Some(Box::new(|host| host.exit())),
        ..App::new(&log)
    };
    host.run(app).unwrap();
    let lines = lines(&log);
    let sizes: Vec<_> = lines.iter().filter(|l| l.starts_with("resized")).collect();
    assert_eq!(sizes, ["resized 2400x1080"], "{lines:?}");
}

#[test]
fn many_wakes_from_other_threads_wake_the_loop_once() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let waker = host.waker();
    for _ in 0..100 {
        waker.wake();
    }
    relay.post(Msg::Stop).unwrap();
    host.run(App::new(&log)).unwrap();
    let lines = lines(&log);
    assert_eq!(
        lines.iter().filter(|l| *l == "woken").count(),
        1,
        "{lines:?}"
    );
}

#[test]
fn the_loop_sleeps_until_the_time_asked_for() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, _relay) = host(&requests);
    let waker = host.waker();
    waker.wake();
    let started = Instant::now();
    let mut app = App::new(&log);
    app.on_first = Some(Box::new(|host| {
        host.wake_at(Some(Instant::now() + Duration::from_millis(30)));
    }));
    struct Until(App);
    impl HostApp<RelayHost<u32, Msg>> for Until {
        fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
            if matches!(event, Event::Wake) {
                host.exit();
            }
            self.0.event(host, event);
        }
    }
    host.run(Until(app)).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(30));
    assert!(lines(&log).contains(&String::from("Wake")));
}

#[test]
fn a_time_passed_wakes_the_loop_though_messages_keep_coming() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    host.waker().wake();
    // Each message brings the next, so one is always waiting when the
    // loop looks again; the time it asked for passes meanwhile.
    struct Busy {
        log: Log,
        relay: Relay<u32, Msg>,
        pings: u32,
    }
    impl HostApp<RelayHost<u32, Msg>> for Busy {
        fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
            match event {
                Event::User(Msg::Wake) => {
                    host.wake_at(Some(Instant::now()));
                    self.relay.post(Msg::Ping).unwrap();
                }
                Event::User(Msg::Ping) if self.pings < 5 => {
                    self.pings += 1;
                    std::thread::sleep(Duration::from_millis(2));
                    self.relay.post(Msg::Ping).unwrap();
                }
                Event::User(Msg::Ping) => self.relay.post(Msg::Stop).unwrap(),
                Event::User(Msg::Stop) => host.exit(),
                Event::Wake => self.log.lock().unwrap().push(String::from("Wake")),
                _ => {}
            }
        }
    }
    let app = Busy {
        log: Arc::clone(&log),
        relay,
        pings: 0,
    };
    host.run(app).unwrap();
    assert_eq!(lines(&log), ["Wake"]);
}

#[test]
fn the_one_window_is_the_only_one() {
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    struct Two(Arc<Mutex<Option<String>>>);
    impl HostApp<RelayHost<u32, Msg>> for Two {
        fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
            if let Event::User(Msg::Stop) = event {
                host.create_window(&WindowAttributes::default()).unwrap();
                let second = host.create_window(&WindowAttributes::default());
                *self.0.lock().unwrap() = second.err().map(|e| e.to_string());
                host.exit();
            }
        }
    }
    relay.post(Msg::Stop).unwrap();
    let refused = Arc::default();
    host.run(Two(Arc::clone(&refused))).unwrap();
    assert!(
        refused
            .lock()
            .unwrap()
            .as_deref()
            .is_some_and(|e| e.contains("one window")),
        "{refused:?}"
    );
}

/// Draws on surface 7 and asks, through `on_draw`, what to do next.
struct Drawer {
    log: Log,
    draws: u32,
    on_draw: fn(u32, &mut RelayControl<'_, u32>),
    on_wait: fn(u32, &mut RelayControl<'_, u32>),
}

impl HostApp<RelayHost<u32, Msg>> for Drawer {
    fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
        match event {
            Event::SurfaceCreated => {
                host.create_window(&WindowAttributes::default()).unwrap();
            }
            Event::Window {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                self.draws += 1;
                self.log.lock().unwrap().push(String::from("drawn"));
                (self.on_draw)(self.draws, host);
            }
            Event::AboutToWait => (self.on_wait)(self.draws, host),
            Event::User(Msg::Stop) => {
                self.log.lock().unwrap().push(String::from("stop"));
                host.exit();
            }
            _ => {}
        }
    }
}

fn surface_then_stop_after(
    relay: &Relay<u32, Msg>,
    after: Duration,
) -> std::thread::JoinHandle<()> {
    relay
        .send(Arrival::SurfaceCreated {
            surface: 7,
            px: [1080, 2400],
            scale: 2.75,
        })
        .unwrap();
    let relay = relay.clone();
    // Only for a loop that went to sleep with a redraw pending: it stops
    // the test, and the lines say what was drawn before.
    std::thread::spawn(move || {
        std::thread::sleep(after);
        let _ = relay.post(Msg::Stop);
    })
}

#[test]
fn a_redraw_asked_while_about_to_wait_is_drawn_without_another_message() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let stopper = surface_then_stop_after(&relay, Duration::from_secs(2));
    let app = Drawer {
        log: Arc::clone(&log),
        draws: 0,
        on_draw: |draws, host| {
            if draws == 2 {
                host.exit();
            }
        },
        // The scene's next pass is due now (an animation, a script that
        // finished preparing): asked for after the loop looked.
        on_wait: |draws, host| {
            if draws == 1 {
                host.request_redraw(WINDOW);
            }
        },
    };
    host.run(app).unwrap();
    drop(stopper);
    assert_eq!(lines(&log), ["drawn", "drawn"]);
}

#[test]
fn a_pending_redraw_is_drawn_while_messages_keep_coming() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let stopper = surface_then_stop_after(&relay, Duration::from_secs(2));
    // Each message brings the next, so the queue is never empty.
    struct Flood {
        drawer: Drawer,
        relay: Relay<u32, Msg>,
        pings: u32,
    }
    impl HostApp<RelayHost<u32, Msg>> for Flood {
        fn event(&mut self, host: &mut RelayControl<'_, u32>, event: Event<'_, (), Msg>) {
            if let Event::User(Msg::Ping) = event {
                self.pings += 1;
                if self.pings == 1 {
                    host.request_redraw(WINDOW);
                }
                if self.pings < 1_000_000 {
                    let _ = self.relay.post(Msg::Ping);
                }
                return;
            }
            self.drawer.event(host, event);
        }
    }
    relay.post(Msg::Ping).unwrap();
    let app = Flood {
        drawer: Drawer {
            log: Arc::clone(&log),
            draws: 0,
            // Drawn once, it asks for its next pass at once.
            on_draw: |draws, host| match draws {
                1 => host.request_redraw(WINDOW),
                _ => host.exit(),
            },
            on_wait: |_, _| {},
        },
        relay: relay.clone(),
        pings: 0,
    };
    host.run(app).unwrap();
    drop(stopper);
    assert_eq!(lines(&log), ["drawn", "drawn"]);
}

#[test]
fn insets_told_before_the_window_reach_it_once_it_is_made() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let insets = crate::Insets {
        system_bars: [0.0, 63.0, 0.0, 126.0],
        ..crate::Insets::default()
    };
    // The activity tells its insets as it starts, before the application
    // has a surface to make its window at.
    relay
        .send(Arrival::Window(WindowEvent::Insets(insets)))
        .unwrap();
    relay
        .send(Arrival::SurfaceCreated {
            surface: 7,
            px: [1080, 2400],
            scale: 2.75,
        })
        .unwrap();
    relay.post(Msg::Stop).unwrap();
    host.run(App::new(&log)).unwrap();
    let told = format!("{:?}", WindowEvent::Insets(insets));
    assert!(lines(&log).contains(&told), "{:?}", lines(&log));
}

#[test]
fn insets_told_before_a_window_made_while_waiting_come_before_newer_ones() {
    let log = Log::default();
    let requests = Arc::default();
    let (host, relay) = host(&requests);
    let older = crate::Insets {
        system_bars: [0.0, 63.0, 0.0, 126.0],
        ..crate::Insets::default()
    };
    let newer = crate::Insets {
        system_bars: [0.0, 63.0, 0.0, 0.0],
        ..crate::Insets::default()
    };
    relay
        .send(Arrival::Window(WindowEvent::Insets(older)))
        .unwrap();
    let mut app = App::new(&log);
    let later = relay.clone();
    // The window is made as the loop is about to wait, not at a message.
    app.on_first = Some(Box::new(move |host| {
        host.create_window(&WindowAttributes::default()).unwrap();
        later
            .send(Arrival::Window(WindowEvent::Insets(newer)))
            .unwrap();
        later.post(Msg::Stop).unwrap();
    }));
    host.run(app).unwrap();
    let told: Vec<String> = lines(&log)
        .into_iter()
        .filter(|line| line.starts_with("Insets"))
        .collect();
    assert_eq!(
        told,
        [
            format!("{:?}", WindowEvent::Insets(older)),
            format!("{:?}", WindowEvent::Insets(newer)),
        ]
    );
}

/// What the method holds: `text`, a selection from `anchor` to `cursor`,
/// and a composition over `compose`.
fn holds(text: &str, [anchor, cursor]: [usize; 2], compose: Option<[usize; 2]>) -> crate::HeldText {
    crate::HeldText {
        text: text.to_owned(),
        anchor,
        cursor,
        compose: compose.map(|[start, end]| start..end),
    }
}

#[test]
fn a_held_request_is_dropped_once_the_method_has_typed_since_it_was_made() {
    // Made while the method held "ab"; it has typed "c" since.
    let over = holds("ab", [0, 0], None);
    assert!(held_is_stale(Some(&over), &holds("abc", [0, 0], None)));
    assert!(!held_is_stale(Some(&over), &holds("ab", [0, 0], None)));
    // A method newly set up holds nothing the application knows of.
    assert!(!held_is_stale(None, &holds("abc", [0, 0], None)));
}

#[test]
fn a_held_request_is_dropped_once_the_method_has_moved_its_cursor_since() {
    let over = holds("ab", [2, 0], None);
    // The user put the cursor at the start; the application's move, made
    // before it, would put it back.
    assert!(held_is_stale(Some(&over), &holds("ab", [0, 0], None)));
    // The same selection, its ends in the order the platform keeps them.
    assert!(!held_is_stale(Some(&over), &holds("ab", [0, 2], None)));
    assert!(!held_is_stale(Some(&over), &holds("ab", [2, 0], None)));
}

#[test]
fn a_held_request_is_dropped_once_the_method_has_changed_its_composition_since() {
    let over = holds("ab", [2, 2], None);
    // The method took "ab" up as a composition again, its text and cursor
    // as they were; the request, made before, would end the composition.
    assert!(held_is_stale(
        Some(&over),
        &holds("ab", [2, 2], Some([0, 2]))
    ));
    let composing = holds("ab", [2, 2], Some([0, 2]));
    assert!(held_is_stale(
        Some(&composing),
        &holds("ab", [2, 2], Some([1, 2]))
    ));
    assert!(held_is_stale(Some(&composing), &holds("ab", [2, 2], None)));
    // The same composition, its ends in either order; an empty one is none.
    assert!(!held_is_stale(
        Some(&composing),
        &holds("ab", [2, 2], Some([2, 0]))
    ));
    assert!(!held_is_stale(
        Some(&over),
        &holds("ab", [2, 2], Some([1, 1]))
    ));
}

#[test]
fn a_held_request_made_while_the_last_write_is_on_its_way_reaches_the_method() {
    let mut writes = Writes::default();
    let a = holds("a", [1, 1], None);
    let b = holds("ab", [2, 2], None);
    let c = holds("abc", [3, 3], None);
    // The application writes "ab" over "a"; the platform holds "a" still.
    let seen = writes.holds(a.clone());
    assert!(!held_is_stale(Some(&a), &seen));
    writes.wrote(&a, b.clone());
    // It writes "abc" over "ab" before the platform got to "ab": the
    // platform reporting "a" has not typed, it is behind.
    let seen = writes.holds(a.clone());
    assert_eq!(seen, b);
    assert!(!held_is_stale(Some(&b), &seen));
    writes.wrote(&a, c.clone());
    // The platform got to "ab" but not yet "abc".
    let d = holds("abcd", [4, 4], None);
    let seen = writes.holds(b.clone());
    assert_eq!(seen, c);
    assert!(!held_is_stale(Some(&c), &seen));
    writes.wrote(&b, d.clone());
    // Both applied, then the method typed "e": a request made over "abcd"
    // is stale, as before.
    let typed = holds("abcde", [5, 5], None);
    let seen = writes.holds(typed.clone());
    assert_eq!(seen, typed);
    assert!(held_is_stale(Some(&d), &seen));
}

#[test]
fn a_method_that_typed_before_the_write_reached_it_still_drops_the_request() {
    let mut writes = Writes::default();
    let a = holds("a", [1, 1], None);
    let b = holds("ab", [2, 2], None);
    writes.wrote(&a, b.clone());
    // The method typed "x" over "a" before the platform got to "ab".
    let typed = holds("ax", [2, 2], None);
    let seen = writes.holds(typed.clone());
    assert_eq!(seen, typed);
    assert!(held_is_stale(Some(&holds("abc", [3, 3], None)), &seen));
    assert!(held_is_stale(Some(&b), &seen));
}
