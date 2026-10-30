//! `app_reload` asked for right after an editor's save, against a real
//! `ljos run --headless --watch --mcp`: the file is written beside and
//! renamed into place, as editors and agents save, and the reload is asked
//! for at once. Each answer is timed and has to be `applied`.
//!
//! The save's own reload races the call (the watcher sees the rename on its
//! thread, the call comes over HTTP), so one edit says little: this runs
//! many, as a reproducer, and prints how long the answers took. It is
//! ignored by default for its length; run it with
//!
//! ```text
//! cargo test -p ljos_cli --test reload_after_write -- --ignored --nocapture
//! ```
//!
//! `LJOS_RELOAD_EDITS` sets the number of edits (200 by default).

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ljos_mcp_core::remote::{Client, wait_for_token_blocking};
use serde_json::json;

/// Edits come this far apart, as an agent typing into a property panel.
const BETWEEN: Duration = Duration::from_millis(150);

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn scene(opacity: &str) -> String {
    format!(
        "import QtQuick\nRectangle {{ width: 100; height: 100; color: \"red\"; opacity: {opacity} }}\n"
    )
}

/// Writes `text` beside `path` and renames it into place: the file is never
/// seen half written.
fn save(path: &Path, text: &str) {
    let beside = path.with_extension("qml.tmp");
    std::fs::write(&beside, text).unwrap();
    std::fs::rename(&beside, path).unwrap();
}

#[test]
#[ignore = "a reproducer: 200 edits 150 ms apart"]
fn a_reload_asked_for_right_after_each_save_answers_with_it_applied() {
    let edits: usize = std::env::var("LJOS_RELOAD_EDITS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(200);
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let main = project.join("Main.qml");
    std::fs::write(&main, scene("1")).unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(["run"])
        .arg(&main)
        .arg("--project")
        .arg(&project)
        .args([
            "--headless",
            "--watch",
            "--mcp=inspect,input,mutate",
            "--size",
            "1280x800",
        ])
        .current_dir(dir.path())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let app = Running(child);
    let pid = app.0.id();
    let token = wait_for_token_blocking(dir.path(), pid, Duration::from_secs(60)).unwrap();
    let client = Client::from_token_file(token, ljos_mcp_core::remote::Trust::FOUND)
        .unwrap()
        .with_client_info("reload-after-write", "0")
        .blocking();
    client.initialize().unwrap();
    // The server is up before the files are first read: a save before the
    // application's first pass is part of what it starts with, not a
    // reload. A snapshot is answered on the UI thread, after that read.
    client.call_tool("app_snapshot", json!({})).unwrap();

    let mut answers = Vec::with_capacity(edits);
    for i in 0..edits {
        save(&main, &scene(if i % 2 == 0 { "0.5" } else { "1" }));
        let asked = Instant::now();
        let answer = client.call_tool("app_reload", json!({})).unwrap();
        let took = asked.elapsed();
        let outcome = format!(
            "{} (generation {})",
            answer.structured["outcome"].as_str().unwrap_or("none"),
            answer.structured["generation"]
        );
        answers.push((i, took, outcome));
        std::thread::sleep(BETWEEN);
    }

    let mut times: Vec<Duration> = answers.iter().map(|(_, t, _)| *t).collect();
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize].as_secs_f64() * 1e3;
    let applied = answers
        .iter()
        .filter(|(_, _, o)| o.starts_with("applied"))
        .count();
    println!(
        "{edits} edits: {applied} applied; answered in p50 {:.1} ms, p90 {:.1} ms, p99 {:.1} ms, max {:.1} ms",
        at(0.5),
        at(0.9),
        at(0.99),
        at(1.0)
    );
    let other: Vec<_> = answers
        .iter()
        .filter(|(_, _, o)| !o.starts_with("applied"))
        .map(|(i, t, o)| format!("edit {i}: {o} after {:.1} ms", t.as_secs_f64() * 1e3))
        .collect();
    assert!(other.is_empty(), "not applied: {other:#?}");
}
