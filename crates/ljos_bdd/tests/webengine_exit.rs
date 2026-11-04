//! A run whose last scenario left a web page open still ends.
//!
//! A view's browser is told to close when its engine drops, but that
//! engine ran CEF's work, so nothing finished closing the browser, and CEF
//! was never shut down: a process that ends with CEF running may hang in
//! its exit on CEF's threads. The child process here does what the
//! features runner does: a page loads, its engine drops with it open, and
//! the web engine is shut down, which must close that browser and stop CEF.
//! The parent requires the child to end, and to succeed.
//!
//! `cargo test -p ljos_bdd --features webengine --test webengine_exit`,
//! with CEF's runtime and helper beside the test as `mise run features`
//! puts them; without them it says so and passes.

use std::{
    process::Command,
    time::{Duration, Instant},
};

use cucumber::World;
use ljos_bdd::world::{LjosWorld, MAIN_FILE};

const CHILD: &str = "LJOS_BDD_WEBENGINE_EXIT_CHILD";

/// Generous: CEF starts its helper processes, loads one page and closes.
const DEADLINE: Duration = Duration::from_secs(120);

#[test]
fn a_process_whose_engine_dropped_an_open_page_ends() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let root = tempfile::tempdir().expect("a directory for the pages' profile");
    // SAFETY: tests of this file do not read the environment meanwhile.
    unsafe { std::env::set_var("LJOS_WEBENGINE_ROOT", root.path()) };
    if let Some(why) = ljos_webengine::unavailable() {
        eprintln!("not run: {why}");
        return;
    }
    let mut child = Command::new(std::env::current_exe().expect("the test's path"))
        .args([
            "--exact",
            "child_drops_an_open_page",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("LJOS_WEBENGINE_ROOT", root.path())
        .spawn()
        .expect("start the child");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("ask after the child") {
            break status;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "a process whose engine dropped an open page did not end within {} s",
                DEADLINE.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success(), "the child failed: {status}");
}

#[test]
#[ignore = "the child of a_process_whose_engine_dropped_an_open_page_ends"]
fn child_drops_an_open_page() {
    let mut world = futures::executor::block_on(<LjosWorld as World>::new()).expect("a world");
    world.prepare(false, false);
    world
        .each(|b| {
            b.files()?.text_file(
                "page.html",
                "<!doctype html><html><head><title>Fixture Page</title></head><body></body></html>",
            )
        })
        .expect("the page is written");
    world.stage(
        MAIN_FILE,
        r#"import QtQuick
import QtWebEngine
WebEngineView {
  objectName: "view"
  width: 200; height: 100
  url: Qt.resolvedUrl("page.html")
}"#,
    );
    world.load(MAIN_FILE).expect("the QML loads");
    world.each(|b| b.advance(2000)).expect("time passes");
    let mut title = String::new();
    world
        .each(|b| {
            title = format!("{:?}", b.get("view", "title")?);
            Ok(())
        })
        .expect("the title is read");
    assert!(
        title.contains("Fixture Page"),
        "the page did not load: {title}"
    );
    // The engine drops with its browser open, as at the end of a scenario.
    drop(world);
    assert!(
        ljos_webengine::shutdown(),
        "the web engine did not shut down"
    );
}
