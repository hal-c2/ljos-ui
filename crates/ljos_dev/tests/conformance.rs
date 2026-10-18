//! Every tool of the inspector, over both ways a client reaches it: the
//! in-process client and loopback HTTP. What is checked is what an agent
//! relies on whatever the tool does: it has a schema and is told the same
//! on both wires, it is refused without its capability before the
//! application is bothered, and what it takes and gives back has a size
//! limit that holds.
//!
//! What each tool does is in `features/tooling/mcp/`; this file is the
//! part that has to be true of all of them at once, so that a tool added to
//! the list cannot be left out of it.

use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use ljos_dev::{
    DevSession,
    mcp::{self, Cap, CapSet, HttpServer, Inspector, InspectorConfig, Listen, TOOLS},
};
use ljos_engine::Engine;
use ljos_mcp_core::glue::local::{LocalClient, LocalError, LocalWake};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::{Clock as _, SystemClock};
use serde_json::{Value, json};

const MAIN: &str = r##"import QtQuick
Item {
    id: page
    objectName: "page"
    width: 300; height: 200
    property int count: 0
    function bump() { count += 1 }
    Rectangle {
        objectName: "save"
        width: 80; height: 30
        Accessible.role: Accessible.Button
        Accessible.name: "Save"
        MouseArea { anchors.fill: parent; onClicked: page.count += 1 }
    }
    TextInput {
        objectName: "note"
        x: 100; width: 120; height: 20
        focus: true
        Accessible.name: "Note"
    }
}
"##;

const ALL: [Cap; 4] = [Cap::Inspect, Cap::Input, Cap::Mutate, Cap::Eval];

/// The capability a tool needs, written down here a second time on purpose:
/// the registry says it too, and the two must agree.
fn needs(tool: &str) -> Cap {
    match tool {
        "app_set_property" | "app_invoke" | "app_reload" => Cap::Mutate,
        "app_eval" => Cap::Eval,
        "app_status" | "app_snapshot" | "app_find" | "app_inspect" | "app_screenshot"
        | "app_logs" | "app_metrics" | "app_processes" | "app_watch" | "app_objects"
        | "app_trace" => Cap::Inspect,
        _ => Cap::Input,
    }
}

fn all_but(left_out: Cap) -> CapSet {
    CapSet::of(ALL.into_iter().filter(|c| *c != left_out))
}

#[derive(Default)]
struct Bell {
    rung: Mutex<bool>,
    ring: Condvar,
    count: AtomicUsize,
}

/// What a call came to, the same whichever way it was made.
struct Out {
    /// The code of a refusal.
    code: Option<String>,
    message: String,
    structured: Value,
    text: String,
}

/// A way to reach the inspector with some capabilities.
enum Wire {
    Local(LocalClient),
    Http { server: HttpServer },
}

struct Rig {
    inspector: Inspector,
    scene: QuickScene,
    engine: Engine,
    clock: SystemClock,
    surface: Arc<mcp::InspectorSurface>,
    local: LocalWake,
    bell: Arc<Bell>,
    dir: std::path::PathBuf,
    _session: DevSession,
    _tmp: tempfile::TempDir,
}

fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    fs::write(dir.join("Main.qml"), MAIN).unwrap();
    let engine = Engine::new();
    ljos_quick::register(&engine);
    let session = DevSession::open(&engine, &dir.join("Main.qml"), Arc::new(|| {})).unwrap();
    let root = session.root().expect("Main.qml loads");
    let mut scene = QuickScene::new(kurbo::Size::new(300.0, 200.0));
    scene.set_root(&engine, root).unwrap();
    let clock = SystemClock::new();
    for _ in 0..16 {
        scene.frame(&engine, clock.now());
        if !scene.needs_frame() {
            break;
        }
    }
    let local = LocalWake::new();
    let bell = Arc::new(Bell::default());
    let wake = {
        let (local, bell) = (local.hook(), bell.clone());
        Arc::new(move || {
            bell.count.fetch_add(1, Ordering::SeqCst);
            local();
            *bell.rung.lock().unwrap() = true;
            bell.ring.notify_one();
        })
    };
    let (inspector, surface) = Inspector::new(InspectorConfig::default(), wake);
    inspector.watch_log(&engine, |_| {});
    Rig {
        inspector,
        scene,
        engine,
        clock,
        surface,
        local,
        bell,
        dir,
        _session: session,
        _tmp: tmp,
    }
}

impl Rig {
    fn wakes(&self) -> usize {
        self.bell.count.load(Ordering::SeqCst)
    }

    fn local(&self, caps: CapSet) -> Wire {
        Wire::Local(LocalClient::connect(self.surface.clone(), caps, &self.local).unwrap())
    }

    fn http(&self, caps: CapSet, name: &str) -> Wire {
        let file = self.dir.join(format!("{name}.json"));
        let server =
            mcp::serve_as(self.surface.clone(), Listen::Loopback(0), file, caps).expect("serves");
        Wire::Http { server }
    }

    /// What a UI thread does for as long as `until` is not finished: sleep
    /// until woken, let the inspector work, run the frame it asks for.
    fn serving<T: Send>(&mut self, request: impl FnOnce() -> T + Send) -> T {
        std::thread::scope(|threads| {
            let client = threads.spawn(request);
            while !client.is_finished() {
                let rung = self.bell.rung.lock().unwrap();
                let (mut rung, _) = self
                    .bell
                    .ring
                    .wait_timeout_while(rung, Duration::from_millis(20), |rung| !*rung)
                    .unwrap();
                let woken = std::mem::take(&mut *rung);
                drop(rung);
                if woken {
                    self.inspector
                        .pump(&self.engine, &mut self.scene, &self.clock);
                    self.scene.frame(&self.engine, self.clock.now());
                }
            }
            client.join().unwrap()
        })
    }

    fn call(&mut self, wire: &mut Wire, tool: &str, args: Value) -> Out {
        match wire {
            Wire::Local(client) => {
                let Rig {
                    inspector,
                    scene,
                    engine,
                    clock,
                    ..
                } = self;
                let reply = client.call(tool, args, &mut || {
                    inspector.pump(engine, scene, clock);
                });
                self.scene.frame(&self.engine, self.clock.now());
                match reply {
                    Ok(r) => Out {
                        code: r.error_code().map(str::to_string),
                        message: r.error_message().unwrap_or_default().to_string(),
                        structured: r.structured,
                        text: r.text,
                    },
                    Err(LocalError::Refused(e)) => Out {
                        code: Some(e.code.as_str().to_string()),
                        message: e.message,
                        structured: e.details.unwrap_or(Value::Null),
                        text: String::new(),
                    },
                    Err(e) => panic!("`{tool}`: {e}"),
                }
            }
            Wire::Http { server } => {
                let addr = server.addr().unwrap();
                let auth = format!("Bearer {}", server.token().as_str());
                let host = addr.to_string();
                let body = json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "name": tool, "arguments": args },
                })
                .to_string();
                let answer = self.serving(|| post(addr, &host, &auth, &body));
                let reply: Value = serde_json::from_str(&answer)
                    .unwrap_or_else(|e| panic!("`{tool}`: {e}: {answer}"));
                if let Some(error) = reply.get("error") {
                    // A refusal the protocol makes itself.
                    return Out {
                        code: error
                            .pointer("/data/code")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        message: error["message"].as_str().unwrap_or_default().to_string(),
                        structured: error.clone(),
                        text: String::new(),
                    };
                }
                let result = &reply["result"];
                let failed = result["isError"] == true;
                Out {
                    code: failed
                        .then(|| result.pointer("/structuredContent/error/code"))
                        .flatten()
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    message: result
                        .pointer("/structuredContent/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    structured: result["structuredContent"].clone(),
                    text: result
                        .pointer("/content/0/text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                }
            }
        }
    }

    fn tools(&mut self, wire: &mut Wire) -> Vec<Value> {
        match wire {
            Wire::Local(client) => client
                .tools()
                .unwrap()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "read_only": t.read_only,
                        "input_schema": t.input_schema,
                    })
                })
                .collect(),
            Wire::Http { server } => {
                let addr = server.addr().unwrap();
                let auth = format!("Bearer {}", server.token().as_str());
                let host = addr.to_string();
                let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
                let answer = self.serving(|| post(addr, &host, &auth, body));
                let reply: Value = serde_json::from_str(&answer).unwrap();
                reply["result"]["tools"]
                    .as_array()
                    .unwrap_or_else(|| panic!("no tools in {answer}"))
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t["name"],
                            "description": t["description"],
                            "read_only": t.pointer("/annotations/readOnlyHint").cloned().unwrap_or(Value::Null),
                            "input_schema": t["inputSchema"],
                        })
                    })
                    .collect()
            }
        }
    }
}

fn unchunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap();
        if n == 0 {
            break;
        }
        out.push_str(&after[..n]);
        rest = &after[n + 2..];
    }
    out
}

/// One request, the body of the answer.
fn post(addr: std::net::SocketAddr, host: &str, auth: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nConnection: close\r\nHost: {host}\r\n\
         Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n\
         Authorization: {auth}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    assert!(head.contains(" 200 "), "{raw}");
    if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        unchunk(body)
    } else {
        body.to_string()
    }
}

/// Both wires, with every capability.
fn both(rig: &Rig, name: &str) -> [Wire; 2] {
    let all = CapSet::of(ALL);
    [rig.local(all), rig.http(all, name)]
}

#[test]
fn every_tool_has_a_schema_and_both_wires_tell_the_same() {
    let mut rig = rig();
    let [mut local, mut http] = both(&rig, "schema");
    let (a, b) = (rig.tools(&mut local), rig.tools(&mut http));
    let names: Vec<&str> = a.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, TOOLS, "the offer is the list, in its order");
    for tool in &a {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(tool["input_schema"]["type"], "object", "{name}");
        let said = tool["description"].as_str().unwrap().chars().count();
        assert!((60..=300).contains(&said), "{name}: {said} characters");
        let reads = needs(name) == Cap::Inspect;
        assert_eq!(tool["read_only"], json!(reads), "{name}");
    }
    // The same schemas and words whichever way they were asked for.
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x["name"], y["name"]);
        assert_eq!(x["description"], y["description"], "{}", x["name"]);
        assert_eq!(x["input_schema"], y["input_schema"], "{}", x["name"]);
    }
    assert_eq!(a.len(), b.len());
}

#[test]
fn every_tool_is_refused_without_its_capability_before_the_application_hears_of_it() {
    let mut rig = rig();
    // One server for each capability left out; a connection's capabilities
    // are those of its server.
    let mut http: Vec<(Cap, Wire)> = ALL
        .into_iter()
        .map(|cap| (cap, rig.http(all_but(cap), &format!("without-{cap}"))))
        .collect();
    let before = rig.wakes();
    for tool in TOOLS {
        let cap = needs(tool);
        let mut local = rig.local(all_but(cap));
        let wire = &mut http.iter_mut().find(|(c, _)| *c == cap).unwrap().1;
        for (way, wire) in [("in process", &mut local), ("over HTTP", wire)] {
            let out = rig.call(wire, tool, json!({}));
            assert_eq!(
                out.code.as_deref(),
                Some("capability_denied"),
                "{tool} {way}: {}",
                out.message
            );
            assert!(
                out.message.contains(cap.as_str()),
                "{tool} {way}: {}",
                out.message
            );
            // How to grant it, with the one word that is missing.
            assert!(
                out.message.contains("--mcp="),
                "{tool} {way}: {}",
                out.message
            );
            assert!(out.message.contains(cap.as_str()), "{tool} {way}");
        }
    }
    assert_eq!(rig.wakes(), before, "a refusal does not wake the UI thread");
}

#[test]
fn eval_is_not_implied_by_all_the_others() {
    let mut rig = rig();
    let others = CapSet::of([Cap::Inspect, Cap::Input, Cap::Mutate]);
    let mut wires = [rig.local(others), rig.http(others, "no-eval")];
    for wire in &mut wires {
        let out = rig.call(wire, "app_eval", json!({ "source": "1 + 1" }));
        assert_eq!(
            out.code.as_deref(),
            Some("capability_denied"),
            "{}",
            out.message
        );
        assert!(out.message.contains("never implied"), "{}", out.message);
        let names: Vec<String> = rig
            .tools(wire)
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(!names.iter().any(|n| n == "app_eval"), "{names:?}");
    }
}

#[test]
fn what_a_tool_takes_is_limited_in_size() {
    let mut rig = rig();
    for mut wire in both(&rig, "limits") {
        let long = "x".repeat(4001);
        let cases = [
            ("app_type", json!({ "text": long }), "4000"),
            (
                "app_batch",
                json!({ "steps": vec![json!({ "tool": "app_press", "args": { "keys": "Tab" } }); 33] }),
                "32",
            ),
            (
                "app_eval",
                json!({ "source": "1".repeat(16 * 1024 + 1) }),
                "16",
            ),
            ("app_press", json!({ "keys": "Tab", "repeat": 101 }), "100"),
        ];
        for (tool, args, word) in cases {
            let out = rig.call(&mut wire, tool, args);
            assert_eq!(
                out.code.as_deref(),
                Some("invalid_argument"),
                "{tool}: {}",
                out.message
            );
            assert!(out.message.contains(word), "{tool}: {}", out.message);
        }
    }
}

#[test]
fn what_app_eval_gives_back_is_at_most_64_kb() {
    let mut rig = rig();
    for mut wire in both(&rig, "answer-size") {
        let out = rig.call(
            &mut wire,
            "app_eval",
            json!({ "source": "'x'.repeat(500000)", "timeout_ms": 5000 }),
        );
        assert_eq!(out.code, None, "{}", out.message);
        // Both copies of the answer, the data and the text, are what a
        // client has to hold.
        let data = out.structured.to_string().len();
        assert!(data <= 64 * 1024, "{data} bytes as data");
        assert!(
            out.text.len() <= 64 * 1024,
            "{} bytes as text",
            out.text.len()
        );
        assert_eq!(out.structured["truncated"], true, "{}", out.structured);
        let value = out.structured["value"]
            .as_str()
            .expect("the value is a string");
        assert!(value.starts_with("xxxx") && value.len() < 500_000);
    }
}

#[test]
fn a_script_that_never_ends_is_stopped_and_the_application_goes_on() {
    let mut rig = rig();
    for mut wire in both(&rig, "time") {
        let out = rig.call(
            &mut wire,
            "app_eval",
            json!({ "source": "while (true) {}", "timeout_ms": 100 }),
        );
        assert_eq!(out.code.as_deref(), Some("timeout"), "{}", out.message);
        let out = rig.call(&mut wire, "app_eval", json!({ "source": "6 * 7" }));
        assert_eq!(out.structured["value"], 42, "{}", out.structured);
    }
}

#[test]
fn a_reference_that_was_never_given_is_told_as_stale_by_every_tool_that_takes_one() {
    let mut rig = rig();
    for mut wire in both(&rig, "stale") {
        for (tool, args) in [
            ("app_click", json!({ "target": "e99" })),
            (
                "app_set_property",
                json!({ "target": "e99", "property": "x", "value": 1 }),
            ),
            ("app_invoke", json!({ "target": "e99", "name": "bump" })),
            ("app_eval", json!({ "target": "e99", "source": "1" })),
            ("app_inspect", json!({ "target": "e99" })),
        ] {
            let out = rig.call(&mut wire, tool, args);
            assert_eq!(
                out.code.as_deref(),
                Some("stale_ref"),
                "{tool}: {}",
                out.message
            );
            assert!(
                out.message.contains("app_snapshot"),
                "{tool}: {}",
                out.message
            );
        }
    }
}
