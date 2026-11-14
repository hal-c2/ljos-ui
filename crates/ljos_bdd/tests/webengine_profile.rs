//! A profile on disk keeps its pages' data where it names, and its HTTP
//! cache where it names that (`WebEngineProfile.persistentStoragePath`,
//! `cachePath`), wherever that is.
//!
//! CEF keeps every profile of a process under one directory, chosen as it
//! starts, so the child process here is a fresh one whose first view has
//! the profile: a page from the web server loads in it, and the web
//! engine shuts down. The parent looks at what is on disk after.
//!
//! `cargo test -p ljos_bdd --features webengine --test webengine_profile`,
//! with CEF's runtime and helper beside the test as `mise run features`
//! puts them; without them it says so and passes.

use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use cucumber::World;
use ljos_bdd::world::{LjosWorld, MAIN_FILE};

const CHILD: &str = "LJOS_BDD_WEBENGINE_PROFILE_CHILD";

/// Generous: CEF starts its helper processes, loads one page and closes.
const DEADLINE: Duration = Duration::from_secs(120);

fn has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

#[test]
fn a_profile_keeps_its_data_and_cache_where_it_names() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let dir = tempfile::tempdir().expect("a directory for the test");
    let root = dir.path().join("engine");
    let storage = dir.path().join("home/browser");
    let cache = dir.path().join("cache/browser");
    // SAFETY: tests of this file do not read the environment meanwhile.
    unsafe { std::env::set_var("LJOS_WEBENGINE_ROOT", &root) };
    if let Some(why) = ljos_webengine::unavailable() {
        eprintln!("not run: {why}");
        return;
    }
    let mut child = Command::new(std::env::current_exe().expect("the test's path"))
        .args([
            "--exact",
            "child_loads_a_page_in_a_named_profile",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("LJOS_WEBENGINE_ROOT", &root)
        .env("LJOS_TEST_STORAGE", &storage)
        .env("LJOS_TEST_CACHE", &cache)
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
            panic!("the child did not end within {} s", DEADLINE.as_secs());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success(), "the child failed: {status}");
    assert!(
        has_files(&storage),
        "nothing was kept in the profile's persistentStoragePath {}",
        storage.display()
    );
    assert!(
        !root.join("named").exists(),
        "the profile was kept under the engine's own directory, {}",
        root.join("named").display()
    );
    assert!(
        has_files(&cache),
        "nothing was cached in the profile's cachePath {}",
        cache.display()
    );
    assert!(
        !walk(&storage).iter().any(|p| p.ends_with("Cache_Data")),
        "the HTTP cache was kept with the profile's data, in {}",
        storage.display()
    );
}

/// Every path below `dir`.
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path.clone());
            }
            out.push(path);
        }
    }
    out
}

#[test]
#[ignore = "the child of a_profile_keeps_its_data_and_cache_where_it_names"]
fn child_loads_a_page_in_a_named_profile() {
    let storage = std::env::var("LJOS_TEST_STORAGE").expect("where the profile keeps its data");
    let cache = std::env::var("LJOS_TEST_CACHE").expect("where the profile caches");
    let mut world = futures::executor::block_on(<LjosWorld as World>::new()).expect("a world");
    world.prepare(false, false);
    world.web().expect("a web server").serve(
        "page.html",
        b"<!doctype html><html><head><title>Fixture Page</title></head>\
          <body><script>localStorage.setItem('kept', 'yes'); document.cookie = 'kept=yes; max-age=3600'</script></body></html>"
            .to_vec(),
    );
    world.stage(
        MAIN_FILE,
        &format!(
            r#"import QtQuick
import QtWebEngine
WebEngineView {{
  objectName: "view"
  width: 200; height: 100
  profile: WebEngineProfile {{
    storageName: "named"; offTheRecord: false
    persistentStoragePath: "{storage}"
    cachePath: "{cache}"
  }}
  url: "$SERVER/page.html"
}}"#
        ),
    );
    world.load(MAIN_FILE).expect("the QML loads");
    world.each(|b| b.advance(3000)).expect("time passes");
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
    drop(world);
    assert!(
        ljos_webengine::shutdown(),
        "the web engine did not shut down"
    );
}
