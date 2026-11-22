//! A profile on disk keeps its pages' data where it names, and its HTTP
//! cache where it names that (`WebEngineProfile.persistentStoragePath`,
//! `cachePath`), wherever that is: the first profile on disk does.
//!
//! CEF keeps every profile of a process under one directory, chosen as it
//! starts: the first profile on disk's. A later profile on disk that names
//! somewhere else is kept under it instead, and says so. Each child
//! process here is a fresh one: pages from the web server load in it, and
//! the web engine shuts down. The parent looks at what is on disk after.
//!
//! `cargo test -p ljos_bdd --features webengine --test webengine_profile`,
//! with CEF's runtime and helper beside the test as `mise run features`
//! puts them; without them it says so and passes.

use std::{
    ffi::OsStr,
    io::{Read as _, Write as _},
    net::TcpListener,
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
    run_child(
        "child_loads_a_page_in_a_named_profile",
        &root,
        &[
            ("LJOS_TEST_STORAGE", storage.as_os_str()),
            ("LJOS_TEST_CACHE", cache.as_os_str()),
        ],
    );
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

/// `text` as a QML string literal, quotes and all: a path on Windows has
/// backslashes, and one anywhere may have a quote.
fn qml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
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

/// Runs `test` of this file in a fresh process, as CEF starts once per
/// process, and waits for it to end well.
fn run_child(test: &str, root: &Path, env: &[(&str, &OsStr)]) {
    let mut child = Command::new(std::env::current_exe().expect("the test's path"));
    child
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env(CHILD, "1")
        .env("LJOS_WEBENGINE_ROOT", root);
    for (name, value) in env {
        child.env(name, value);
    }
    let mut child = child.spawn().expect("start the child");
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
}

#[test]
fn a_later_profile_on_disk_is_kept_under_the_first_ones_place() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let dir = tempfile::tempdir().expect("a directory for the test");
    let root = dir.path().join("engine");
    let first = dir.path().join("one/browser");
    let second = dir.path().join("two/browser");
    // SAFETY: tests of this file do not read the environment meanwhile.
    unsafe { std::env::set_var("LJOS_WEBENGINE_ROOT", &root) };
    if let Some(why) = ljos_webengine::unavailable() {
        eprintln!("not run: {why}");
        return;
    }
    run_child(
        "child_loads_pages_in_two_named_profiles",
        &root,
        &[
            ("LJOS_TEST_FIRST", first.as_os_str()),
            ("LJOS_TEST_SECOND", second.as_os_str()),
        ],
    );
    // What went where, for when it went elsewhere.
    let kept = || {
        walk(dir.path())
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "Preferences"))
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    };
    // The view off the record shown first does not choose the place.
    assert!(
        !root.exists(),
        "CEF started from the engine's own directory, {}, not the first profile on disk's: {:#?}",
        root.display(),
        kept()
    );
    assert!(
        has_files(&first),
        "nothing was kept in the first profile's persistentStoragePath {}: {:#?}",
        first.display(),
        kept()
    );
    assert!(
        has_files(&first.join("Second")),
        "the second profile was not kept under the first one's place, {}: {:#?}",
        first.join("Second").display(),
        kept()
    );
    assert!(
        !second.exists(),
        "the second profile wrote to {}, outside the one directory CEF keeps: {:#?}",
        second.display(),
        kept()
    );
}

#[test]
#[ignore = "the child of a_later_profile_on_disk_is_kept_under_the_first_ones_place"]
fn child_loads_pages_in_two_named_profiles() {
    let first = qml_string(
        &std::env::var("LJOS_TEST_FIRST").expect("where the first profile keeps its data"),
    );
    let second = qml_string(
        &std::env::var("LJOS_TEST_SECOND").expect("where the second profile asks to keep its data"),
    );
    let qml = format!(
        r#"import QtQuick
import QtWebEngine
Item {{
  WebEngineProfile {{
    id: first; storageName: "First"; offTheRecord: false
    persistentStoragePath: {first}
  }}
  WebEngineProfile {{
    id: second; storageName: "Second"; offTheRecord: false
    persistentStoragePath: {second}
  }}
  WebEngineView {{ objectName: "anon"; width: 100; height: 100; url: "$SERVER/page.html" }}
  WebEngineView {{ objectName: "one"; x: 100; width: 100; height: 100; profile: first; url: "$SERVER/page.html" }}
  WebEngineView {{ objectName: "two"; x: 200; width: 100; height: 100; profile: second; url: "$SERVER/page.html" }}
}}"#
    );
    load_and_shut_down(&qml, &["anon", "one", "two"]);
}

#[test]
#[ignore = "the child of a_profile_keeps_its_data_and_cache_where_it_names"]
fn child_loads_a_page_in_a_named_profile() {
    let storage =
        qml_string(&std::env::var("LJOS_TEST_STORAGE").expect("where the profile keeps its data"));
    let cache = qml_string(&std::env::var("LJOS_TEST_CACHE").expect("where the profile caches"));
    let qml = format!(
        r#"import QtQuick
import QtWebEngine
WebEngineView {{
  objectName: "view"
  width: 200; height: 100
  profile: WebEngineProfile {{
    storageName: "named"; offTheRecord: false
    persistentStoragePath: {storage}
    cachePath: {cache}
  }}
  url: "$SERVER/page.html"
}}"#
    );
    load_and_shut_down(&qml, &["view"]);
}

/// Loads `qml` with a page that keeps something on the web server, waits
/// for each of `views` to show it, and shuts the web engine down, so what
/// the profiles keep is written.
fn load_and_shut_down(qml: &str, views: &[&str]) {
    let mut world = futures::executor::block_on(<LjosWorld as World>::new()).expect("a world");
    world.prepare(false, ljos_bdd::backend::Surface::Headless);
    world.web().expect("a web server").serve(
        "page.html",
        b"<!doctype html><html><head><title>Fixture Page</title></head>\
          <body><script>localStorage.setItem('kept', 'yes'); document.cookie = 'kept=yes; max-age=3600'</script></body></html>"
            .to_vec(),
    );
    world.stage(MAIN_FILE, qml);
    world.load(MAIN_FILE).expect("the QML loads");
    world.each(|b| b.advance(3000)).expect("time passes");
    for view in views {
        let mut title = String::new();
        world
            .each(|b| {
                title = format!("{:?}", b.get(view, "title")?);
                Ok(())
            })
            .expect("the title is read");
        assert!(
            title.contains("Fixture Page"),
            "the page did not load in {view}: {title}"
        );
    }
    drop(world);
    assert!(
        ljos_webengine::shutdown(),
        "the web engine did not shut down"
    );
}

/// Asks for notifications and shows the answer, from one fixed origin
/// both runs share: Chromium keeps an answer as its origin's, port and
/// all.
const ASKS: &[u8] = b"<!doctype html><html><head><title>waiting</title></head><body><script>\
    Notification.requestPermission().then(answer => document.title = answer)\
    </script></body></html>";

#[test]
fn an_answer_not_to_be_kept_is_not_kept_by_a_run_that_was_killed() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let dir = tempfile::tempdir().expect("a directory for the test");
    let root = dir.path().join("engine");
    let storage = dir.path().join("home/browser");
    // SAFETY: tests of this file do not read the environment meanwhile.
    unsafe { std::env::set_var("LJOS_WEBENGINE_ROOT", &root) };
    if let Some(why) = ljos_webengine::unavailable() {
        eprintln!("not run: {why}");
        return;
    }
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a loopback port")
        .port()
        .to_string();
    let env = [
        ("LJOS_TEST_STORAGE", storage.as_os_str()),
        ("LJOS_TEST_PORT", OsStr::new(&port)),
    ];
    // The first run is answered, sees Chromium write the answer down, and
    // ends without shutting the web engine down.
    run_child("child_is_answered_and_killed", &root, &env);
    // The next is asked again.
    run_child("child_is_asked_again", &root, &env);
}

#[test]
#[ignore = "the first child of an_answer_not_to_be_kept_is_not_kept_by_a_run_that_was_killed"]
fn child_is_answered_and_killed() {
    let storage = std::env::var("LJOS_TEST_STORAGE").expect("where the profile keeps its data");
    let (mut world, origin) = asking_world(&storage);
    // Chromium writes what a profile keeps a while after it changes, in
    // a Preferences file of the profile's directory.
    let written = || {
        walk(Path::new(&storage)).iter().any(|p| {
            p.file_name().is_some_and(|n| n == "Preferences")
                && std::fs::read_to_string(p).is_ok_and(|text| text.contains(&origin))
        })
    };
    let started = Instant::now();
    while !written() {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "Chromium did not write the answer to {origin} under {storage}, which holds {:#?}",
            walk(Path::new(&storage))
        );
        world.each(|b| b.advance(500)).expect("time passes");
    }
    // Killed: no shutdown, nothing dropped.
    std::process::exit(0);
}

#[test]
#[ignore = "the second child of an_answer_not_to_be_kept_is_not_kept_by_a_run_that_was_killed"]
fn child_is_asked_again() {
    let storage = std::env::var("LJOS_TEST_STORAGE").expect("where the profile keeps its data");
    let (world, _) = asking_world(&storage);
    drop(world);
    assert!(
        ljos_webengine::shutdown(),
        "the web engine did not shut down"
    );
}

/// A view of a profile on disk that keeps answers only while it runs, on
/// a page that asks for notifications and is granted them: the view must
/// have been asked.
fn asking_world(storage: &str) -> (LjosWorld, String) {
    let port: u16 = std::env::var("LJOS_TEST_PORT")
        .expect("the port both runs serve from")
        .parse()
        .expect("a port");
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("the shared port");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                ASKS.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(ASKS);
        }
    });
    let origin = format!("127.0.0.1:{port}");
    let mut world = futures::executor::block_on(<LjosWorld as World>::new()).expect("a world");
    world.prepare(false, ljos_bdd::backend::Surface::Headless);
    world.stage(
        MAIN_FILE,
        &format!(
            r#"import QtQuick
import QtWebEngine
WebEngineView {{
  objectName: "view"
  width: 200; height: 100
  property int asked: 0
  profile: WebEngineProfile {{
    storageName: "asking"; offTheRecord: false
    persistentStoragePath: "{storage}"
    persistentPermissionsPolicy: WebEngineProfile.StoreInMemory
  }}
  onPermissionRequested: permission => {{ asked++; permission.grant() }}
  url: "http://{origin}/asks.html"
}}"#
        ),
    );
    world.load(MAIN_FILE).expect("the QML loads");
    world.each(|b| b.advance(3000)).expect("time passes");
    let (mut title, mut asked) = (serde_json::Value::Null, serde_json::Value::Null);
    world
        .each(|b| {
            title = b.get("view", "title")?;
            asked = b.get("view", "asked")?;
            Ok(())
        })
        .expect("the view is read");
    assert_eq!(title, "granted", "the page was not answered");
    assert_eq!(
        asked.as_f64(),
        Some(1.0),
        "the view was not asked once, as a profile that keeps answers only while it runs is: {asked}"
    );
    (world, origin)
}
