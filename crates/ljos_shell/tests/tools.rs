//! The shell's development tools: the `top` overlay (feature `dev`) and
//! the read-only inspector (feature `mcp`). Both are held to the rule the
//! rest of the shell keeps: a window in which nothing happens runs
//! nothing, with the overlay shown and with a client connected.
//!
//! The tests wait as the window does: asleep until egui is asked for a
//! repaint.
#![cfg(feature = "dev")]

use std::{
    fs,
    sync::{Arc, mpsc},
    time::Duration,
};

use egui::{Vec2, ViewportId};
use egui_kittest::{Harness, kittest::Queryable as _};
use ljos_runtime::SystemClock;
use ljos_shell::{Setup, SetupError, ShellApp, ShellConfig};

const MAIN: &str = r#"import QtQuick
Item {
  objectName: "root"
  Rectangle {
    objectName: "box"; width: 120; height: 40; color: "green"
    Accessible.role: Accessible.Button
    Accessible.name: "Save"
  }
}
"#;

struct Window {
    // Dropped first: the watcher's thread ends before the directory goes.
    h: Harness<'static, ShellApp<()>>,
    /// Each request for a pass, by how long it may wait.
    woke: mpsc::Receiver<Duration>,
    _tmp: tempfile::TempDir,
}

/// A window on a project on disk; `tools` is the part of the setup a
/// test is about.
fn window(tools: impl FnOnce(&mut Setup<'_, ()>) -> Result<(), SetupError> + 'static) -> Window {
    window_of(MAIN, tools)
}

fn window_of(
    main: &str,
    tools: impl FnOnce(&mut Setup<'_, ()>) -> Result<(), SetupError> + 'static,
) -> Window {
    let mut w = opened(main, tools);
    w.h.run();
    w.forget_wakes();
    w
}

/// The window after its first pass, before it has come to rest.
fn opened(
    main: &str,
    tools: impl FnOnce(&mut Setup<'_, ()>) -> Result<(), SetupError> + 'static,
) -> Window {
    let tmp = tempfile::tempdir().unwrap();
    let entry = tmp.path().canonicalize().unwrap().join("Main.qml");
    fs::write(&entry, main).unwrap();
    let (tx, woke) = mpsc::channel();
    let h = Harness::builder()
        .with_size(Vec2::new(480.0, 320.0))
        .with_max_steps(8)
        .build_eframe(move |cc| {
            // What a real window's event loop sleeps on.
            cc.egui_ctx.set_request_repaint_callback(move |info| {
                let _ = tx.send(info.delay);
            });
            ShellApp::new(
                &cc.egui_ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                |setup| {
                    tools(setup)?;
                    setup.watch(&entry)
                },
            )
            .unwrap()
        });
    Window { h, woke, _tmp: tmp }
}

impl Window {
    fn forget_wakes(&self) {
        while self.woke.try_recv().is_ok() {}
    }

    fn frames(&self) -> u64 {
        self.h.state().view().stats().frames
    }

    fn repaint_delay(&self) -> Duration {
        self.h.output().viewport_output[&ViewportId::ROOT].repaint_delay
    }

    /// Nothing asks for a pass, and a pass that egui runs anyway runs no
    /// frame of the application.
    fn assert_idle(&mut self) {
        assert_eq!(self.repaint_delay(), Duration::MAX);
        let frames = self.frames();
        for _ in 0..3 {
            self.h.step();
            assert_eq!(self.repaint_delay(), Duration::MAX);
        }
        assert_eq!(self.frames(), frames);
        assert!(
            self.woke.recv_timeout(Duration::from_millis(300)).is_err(),
            "something woke an idle window"
        );
    }
}

#[test]
fn the_top_overlay_is_hidden_unless_asked_for() {
    let mut w = window(|_| Ok(()));
    assert!(!w.h.state().top_visible());
    assert_eq!(w.h.state().top(), None);
    assert!(w.h.query_by_label_contains("frame ").is_none());
    w.assert_idle();
}

#[test]
fn the_top_overlay_shows_the_last_frame_by_lane() {
    let w = window(|setup| {
        setup.show_top();
        Ok(())
    });
    let top = w.h.state().top().expect("the overlay is shown");
    assert!(top.title.starts_with("frame "), "{}", top.title);
    let names: Vec<&str> = top.lanes.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ljos_dev::top::LANE_NAMES);
    // Painted: the title is on the screen, in the window's corner.
    let title = w.h.get_by_label(&top.title);
    assert!(title.rect().right() <= 480.0 && title.rect().top() < 40.0);
}

#[test]
fn the_top_overlay_keeps_no_frames_coming() {
    let mut w = window(|setup| {
        setup.show_top();
        Ok(())
    });
    // It was given the frames the application ran and asks for none: what
    // it shows stays until the application has a reason to run again.
    let shown = w.h.state().top();
    w.assert_idle();
    assert_eq!(w.h.state().top(), shown);
}

#[test]
fn the_top_overlay_is_shown_and_hidden_while_the_window_runs() {
    let mut w = window(|_| Ok(()));
    w.h.state_mut().set_top_visible(true);
    // One pass to paint it. Nothing was recorded while it was hidden.
    assert!(w.woke.try_recv().is_ok(), "showing it asks for a pass");
    w.h.run();
    let top = w.h.state().top().expect("the overlay is shown");
    assert!(w.h.query_by_label(&top.title).is_some());
    w.forget_wakes();
    w.assert_idle();

    w.h.state_mut().set_top_visible(false);
    assert!(w.woke.try_recv().is_ok(), "hiding it asks for a pass");
    w.h.run();
    assert_eq!(w.h.state().top(), None);
    assert!(w.h.query_by_label(&top.title).is_none());
    w.forget_wakes();
    w.assert_idle();
}

/// An application whose first frames make an item in a creation job.
const LATE: &str = r#"import QtQuick
Item {
  Component { id: part; Rectangle { width: 30; height: 30; color: "green" } }
  Loader { objectName: "loader"; asynchronous: true; sourceComponent: part }
}
"#;

#[test]
fn the_top_overlay_names_the_component_whose_work_a_frame_did() {
    let mut w = opened(LATE, |setup| {
        setup.show_top();
        Ok(())
    });
    // The overlay shows one frame at a time, so each is looked at: the
    // item is made in one of the first, and the frames after it did no
    // such work.
    let mut charged = Vec::new();
    for _ in 0..20 {
        let top = w.h.state().top().expect("the overlay is shown");
        charged.extend(top.owners.into_iter().map(|row| row.name));
        if w.repaint_delay() == Duration::MAX {
            break;
        }
        w.h.step();
    }
    assert!(
        charged.iter().any(|name| name == "Loader \"loader\""),
        "{charged:?}"
    );
}

#[test]
fn work_done_before_the_top_overlay_was_shown_is_not_its_first_frame() {
    let mut w = window_of(LATE, |_| Ok(()));
    w.h.state_mut().set_top_visible(true);
    // A frame of the application's own, with no creation job in it.
    w.h.state_mut().view_mut().scene_mut().request_frame();
    w.h.run();
    let top = w.h.state().top().expect("the overlay is shown");
    assert!(top.title.starts_with("frame "), "{}", top.title);
    assert_eq!(top.owners, [], "{:?}", top.lines());
}

#[cfg(feature = "mcp")]
mod inspector {
    use std::{
        io::{Read as _, Write as _},
        net::TcpStream,
        os::unix::fs::PermissionsExt as _,
        thread,
    };

    use ljos_dev::mcp::{Cap, CapSet};
    use ljos_shell::{InspectorAccess, InspectorAddress, InspectorOptions};

    use super::*;

    /// Long enough for a debug build on a loaded machine; a wake ends the
    /// wait at once.
    const PATIENCE: Duration = Duration::from_secs(30);

    fn served() -> (Window, InspectorAddress) {
        served_of(MAIN)
    }

    /// As [`served`], on the window of `main`.
    fn served_of(main: &str) -> (Window, InspectorAddress) {
        let (tx, rx) = mpsc::channel();
        let w = window_of(main, move |setup| {
            let dir = tempfile::tempdir()?.keep();
            let address = setup.serve_inspector(
                InspectorOptions {
                    port: 0,
                    token_file: dir.join("mcp.json"),
                    redact_all: false,
                },
                |_| {},
            )?;
            let _ = tx.send(address);
            Ok(())
        });
        (w, rx.recv().unwrap())
    }

    /// The token a client reads from the file the window wrote.
    fn token(address: &InspectorAddress) -> String {
        let text = fs::read_to_string(&address.token_file).unwrap();
        let (_, rest) = text.split_once("\"token\"").expect("a token");
        rest.split('"').nth(1).unwrap().to_owned()
    }

    /// One `tools/call` over HTTP, as a client that is no MCP library
    /// writes it. Returns the status and the body.
    fn call(address: &InspectorAddress, bearer: &str, tool: &str) -> (u16, String) {
        call_with(address, bearer, tool, "{}")
    }

    /// As [`call`], with the arguments (a JSON object) of the tool.
    fn call_with(
        address: &InspectorAddress,
        bearer: &str,
        tool: &str,
        arguments: &str,
    ) -> (u16, String) {
        let host = address
            .url
            .strip_prefix("http://")
            .and_then(|u| u.strip_suffix("/mcp"))
            .expect("a loopback URL");
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{tool}","arguments":{arguments}}}}}"#
        );
        let mut stream = TcpStream::connect(host).unwrap();
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
             Authorization: Bearer {bearer}\r\nContent-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let status = raw.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, raw)
    }

    /// Runs the window on wakes, as its event loop does, until the client
    /// on the other thread has its answer.
    fn answer(w: &mut Window, address: &InspectorAddress, tool: &'static str) -> (u16, String) {
        let (address, bearer) = (address.clone(), token(address));
        let client = thread::spawn(move || call(&address, &bearer, tool));
        while !client.is_finished() {
            if w.woke.recv_timeout(Duration::from_millis(20)).is_ok() {
                w.h.run();
            }
        }
        // The frame a request left the scene asking for.
        w.h.run();
        w.forget_wakes();
        client.join().unwrap()
    }

    /// A window whose clients hold `caps`.
    fn served_with(caps: &[Cap]) -> (Window, InspectorAddress) {
        let (tx, rx) = mpsc::channel();
        let caps = CapSet::of(caps.iter().copied());
        let w = window(move |setup| {
            let dir = tempfile::tempdir()?.keep();
            let address = setup.serve_inspector_as(
                InspectorOptions {
                    port: 0,
                    token_file: dir.join("mcp.json"),
                    redact_all: false,
                },
                InspectorAccess {
                    caps,
                    ..InspectorAccess::default()
                },
                |_| {},
            )?;
            let _ = tx.send(address);
            Ok(())
        });
        (w, rx.recv().unwrap())
    }

    /// The answer's structured content: the JSON object in the body.
    fn structured(body: &str) -> serde_json::Value {
        let (from, to) = (body.find('{').unwrap(), body.rfind('}').unwrap());
        let reply: serde_json::Value =
            serde_json::from_str(&body[from..=to]).unwrap_or_else(|e| panic!("{e}: {body}"));
        reply["result"]["structuredContent"].clone()
    }

    /// The project's entry file, changed to say `name` on its button.
    fn save_as(w: &Window, name: &str) {
        let entry = w._tmp.path().canonicalize().unwrap().join("Main.qml");
        fs::write(entry, MAIN.replace("\"Save\"", &format!("\"{name}\""))).unwrap();
    }

    // A Timer a minute away: a loop sleeps until it unless a request for
    // a pass at once wakes it.
    #[test]
    fn app_metrics_answers_while_a_long_timer_is_pending() {
        let (mut w, address) = served_of(
            "import QtQuick\nItem { Timer { interval: 60000; repeat: true; running: true } }\n",
        );
        let (a, bearer) = (address.clone(), token(&address));
        let client = thread::spawn(move || call(&a, &bearer, "app_metrics"));
        let until = std::time::Instant::now() + PATIENCE;
        while !client.is_finished() {
            assert!(
                std::time::Instant::now() < until,
                "the request did not ask for a pass at once"
            );
            if w.woke
                .recv_timeout(Duration::from_millis(20))
                .is_ok_and(|delay| delay.is_zero())
            {
                w.h.step();
            }
        }
        let (status, body) = client.join().unwrap();
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("frames"), "{body}");
    }

    #[test]
    fn a_watched_window_says_that_its_files_are_watched() {
        let (mut w, address) = served_with(&[Cap::Inspect]);
        let (status, body) = answer(&mut w, &address, "app_status");
        assert_eq!(status, 200, "{body}");
        assert_eq!(structured(&body)["reload"]["watched"], true, "{body}");
        assert!(!body.contains("files are not watched"), "{body}");
    }

    #[test]
    fn app_status_says_how_many_windows_there_are_and_which_is_inspected() {
        let (mut w, address) = served();
        let (status, body) = answer(&mut w, &address, "app_status");
        assert_eq!(status, 200, "{body}");
        let s = structured(&body);
        assert_eq!(s["window_count"], 1, "{body}");
        assert_eq!(s["inspected"], "primary_window_only", "{body}");
    }

    #[test]
    fn app_reload_applies_a_save_at_once() {
        let (mut w, address) = served_with(&[Cap::Inspect, Cap::Mutate]);
        save_as(&w, "Saved");
        let (status, body) = answer(&mut w, &address, "app_reload");
        assert_eq!(status, 200, "{body}");
        let s = structured(&body);
        assert_eq!(s["generation"], 1, "{body}");
        assert!(
            matches!(s["outcome"].as_str(), Some("applied" | "unchanged")),
            "{body}"
        );
        let (_, body) = answer(&mut w, &address, "app_snapshot");
        assert!(body.contains("button \\\"Saved\\\""), "{body}");
        assert_eq!(w.h.state().reloads(), Some(1));
    }

    #[test]
    fn app_reload_with_nothing_changed_says_so() {
        let (mut w, address) = served_with(&[Cap::Inspect, Cap::Mutate]);
        let (status, body) = answer(&mut w, &address, "app_reload");
        assert_eq!(status, 200, "{body}");
        assert_eq!(structured(&body)["outcome"], "unchanged", "{body}");
        assert_eq!(w.h.state().reloads(), Some(0));
        w.assert_idle();
    }

    #[test]
    fn app_reload_tells_a_save_that_does_not_compile() {
        let (mut w, address) = served_with(&[Cap::Inspect, Cap::Mutate]);
        let entry = w._tmp.path().canonicalize().unwrap().join("Main.qml");
        fs::write(entry, "import QtQuick\nItem {\n").unwrap();
        let (status, body) = answer(&mut w, &address, "app_reload");
        assert_eq!(status, 200, "{body}");
        let s = structured(&body);
        assert_eq!(s["outcome"], "failed", "{body}");
        assert_eq!(s["generation"], 0, "{body}");
        assert_eq!(w.h.state().reloads(), Some(0));
    }

    #[test]
    fn a_client_that_waits_for_a_reload_does_not_wake_the_window() {
        let (mut w, address) = served_with(&[Cap::Inspect]);
        let (a, bearer) = (address.clone(), token(&address));
        let client = thread::spawn(move || {
            call_with(
                &a,
                &bearer,
                "app_status",
                r#"{"after_reload":0,"wait_ms":25000}"#,
            )
        });
        // A few seconds, in which a timer on the window's thread or on
        // the server's would show as a wake.
        assert!(
            w.woke.recv_timeout(Duration::from_secs(3)).is_err(),
            "a waiting client woke the window"
        );
        assert!(!client.is_finished());
        save_as(&w, "Waited");
        while !client.is_finished() {
            if w.woke.recv_timeout(Duration::from_millis(20)).is_ok() {
                w.h.run();
            }
        }
        w.h.run();
        let (status, body) = client.join().unwrap();
        assert_eq!(status, 200, "{body}");
        let s = structured(&body);
        assert_eq!(s["reload"]["generation"], 1, "{body}");
        assert_eq!(s["reload"]["waited"], true, "{body}");
    }

    #[test]
    fn the_inspector_is_served_on_loopback_with_a_private_token_file() {
        let (mut w, address) = served();
        assert_eq!(w.h.state().inspector_address(), Some(&address));
        assert!(
            address.url.starts_with("http://127.0.0.1:"),
            "{}",
            address.url
        );
        let mode = fs::metadata(&address.token_file)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(token(&address).len(), 64);

        w.h.state_mut().stop_inspector().unwrap();
        assert!(!address.token_file.exists(), "the token file is removed");
        assert_eq!(w.h.state().inspector_address(), None);
        let host = address
            .url
            .trim_start_matches("http://")
            .trim_end_matches("/mcp");
        assert!(
            TcpStream::connect(host).is_err(),
            "nothing listens any more"
        );
    }

    #[test]
    fn a_client_is_answered_by_the_running_window() {
        let (mut w, address) = served();
        let (status, body) = answer(&mut w, &address, "app_snapshot");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("button \\\"Save\\\""), "{body}");

        let (status, body) = answer(&mut w, &address, "app_metrics");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("frames"), "{body}");
    }

    #[test]
    fn a_window_serves_the_capabilities_it_was_given_and_no_other() {
        let (tx, rx) = mpsc::channel();
        let mut w = window(move |setup| {
            let dir = tempfile::tempdir()?.keep();
            let address = setup.serve_inspector_as(
                InspectorOptions {
                    port: 0,
                    token_file: dir.join("mcp.json"),
                    redact_all: false,
                },
                InspectorAccess {
                    caps: CapSet::of([Cap::Inspect, Cap::Input]),
                    ..InspectorAccess::default()
                },
                |_| {},
            )?;
            let _ = tx.send(address);
            Ok(())
        });
        let address = rx.recv().unwrap();
        let (status, body) = answer(&mut w, &address, "app_status");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("capabilities: inspect, input"), "{body}");
        // `eval` is not among them, and the refusal says how to get it.
        let (status, body) = answer(&mut w, &address, "app_eval");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("capability_denied"), "{body}");
        assert!(body.contains("--mcp=inspect,input,eval"), "{body}");
    }

    #[test]
    fn a_client_without_the_token_is_turned_away_and_wakes_nothing() {
        let (mut w, address) = served();
        let frames = w.frames();
        let (status, _) = call(&address, &"0".repeat(64), "app_snapshot");
        assert_eq!(status, 401);
        assert!(
            w.woke.try_recv().is_err(),
            "a refusal reached the UI thread"
        );
        w.assert_idle();
        assert_eq!(w.frames(), frames);
    }

    #[test]
    fn a_served_window_with_a_client_that_asks_nothing_is_idle() {
        let (mut w, address) = served();
        w.assert_idle();
        // A client that is connected and has asked: once it is answered,
        // the window is asleep again.
        let (status, _) = answer(&mut w, &address, "app_snapshot");
        assert_eq!(status, 200);
        w.assert_idle();
        // And a connection that stays open and says nothing costs nothing.
        let host = address
            .url
            .trim_start_matches("http://")
            .trim_end_matches("/mcp");
        let _open = TcpStream::connect(host).unwrap();
        w.assert_idle();
    }

    #[test]
    fn the_top_overlay_charges_the_inspector_for_its_work() {
        let (tx, rx) = mpsc::channel();
        let mut w = window(move |setup| {
            setup.show_top();
            let dir = tempfile::tempdir()?.keep();
            let address = setup.serve_inspector(
                InspectorOptions {
                    port: 0,
                    token_file: dir.join("mcp.json"),
                    redact_all: false,
                },
                |_| {},
            )?;
            let _ = tx.send(address);
            Ok(())
        });
        let address: InspectorAddress = rx.recv().unwrap();
        let owners = |w: &Window| -> Vec<String> {
            let top = w.h.state().top().expect("the overlay is shown");
            top.owners.into_iter().map(|r| r.name).collect()
        };
        assert!(!owners(&w).contains(&"inspector".to_owned()));

        let (address, bearer) = (address.clone(), token(&address));
        let client = thread::spawn(move || call(&address, &bearer, "app_snapshot"));
        w.woke
            .recv_timeout(PATIENCE)
            .expect("the request woke the window");
        // The harness reads the accessibility tree as a screen reader
        // does, so the scene has it already and a look needs no frame.
        // With nobody else reading, the look itself is followed by one;
        // this stands for it.
        w.h.state_mut().view_mut().scene_mut().request_frame();
        w.h.run();
        assert_eq!(client.join().unwrap().0, 200);
        // The frame in which an agent looked says so, under its own name.
        assert_eq!(owners(&w), ["inspector"]);
        let top = w.h.state().top().unwrap();
        assert!(top.lanes[4].time > Duration::ZERO, "{top:?}");
        w.forget_wakes();
        w.assert_idle();
    }
}
