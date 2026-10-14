//! The example's own `Main.qml`, compiled against an Engine this
//! crate's `BackEnd` is registered on (so the front end knows the
//! type) and created, with the backend answering as a desktop.

use std::path::PathBuf;

use ljos::dev::EngineOracle;
use ljos::prelude::*;
use ljos_compiler::{Project, Severity, compile_file};

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples/demos/qtjennydemo")
}

/// The corpus is a submodule the check job does not check out; without
/// it a test says why on the terminal and passes.
fn corpus_present(test: &str) -> bool {
    let dir = example_dir();
    if dir.is_dir() {
        return true;
    }
    use std::io::Write as _;
    let _ = writeln!(
        std::io::stderr(),
        "SKIPPED {test}: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
        dir.display()
    );
    false
}

fn engine() -> Engine {
    let engine = ljos::engine();
    ljos_example_qtjennydemo::register(&engine);
    let dir = example_dir();
    let mut project = Project::load(&dir, EngineOracle::from_engine(&engine))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut source = ljos::MapSource::new();
    for key in project.files().map(str::to_owned).collect::<Vec<_>>() {
        let compiled = compile_file(&project, &key).unwrap_or_else(|d| panic!("{key}: {d:?}"));
        let errors: Vec<String> = compiled
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(errors.is_empty(), "{key}: {errors:?}");
        source.insert(&key, compiled.file);
    }
    let _ = &mut project;
    engine.set_component_source(source);
    engine
}

fn walk(e: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut all = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        all.push(o);
        stack.extend(e.children(o));
    }
    all
}

fn backend(e: &Engine, root: ObjectId) -> ObjectId {
    walk(e, root)
        .into_iter()
        .find(|&o| matches!(e.read_by_name(o, "canWriteSystemSettings"), Value::Bool(_)))
        .expect("no BackEnd in the tree")
}

#[test]
fn main_compiles_clean_and_creates() {
    if !corpus_present("main_compiles_clean_and_creates") {
        return;
    }
    let e = engine();
    let root = e
        .create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main: {err}"));
    let b = backend(&e, root);
    assert!(!e.read_by_name(b, "canWriteSystemSettings").to_boolean());
    assert_eq!(e.read_by_name(b, "minVolume").to_number(), 0.0);
    assert_eq!(e.read_by_name(b, "maxVolume").to_number(), 0.0);
}

#[test]
fn the_device_says_it_cannot_write_settings_and_the_popup_shows_it() {
    if !corpus_present("the_device_says_it_cannot_write_settings_and_the_popup_shows_it") {
        return;
    }
    let e = engine();
    let root = e.create_from_url("Main", None, Vec::new()).expect("Main");
    let said = walk(&e, root).into_iter().any(|o| {
        e.read_by_name(o, "text")
            .to_js_string()
            .as_str()
            .contains("cannot grant the right to write system settings")
    });
    assert!(said, "no text shows the backend's message");
}

#[test]
fn a_refused_brightness_reads_back_and_is_logged() {
    if !corpus_present("a_refused_brightness_reads_back_and_is_logged") {
        return;
    }
    let e = engine();
    let root = e.create_from_url("Main", None, Vec::new()).expect("Main");
    let b = backend(&e, root);
    e.write_by_name(b, "brightness", Value::from(120))
        .expect("write");
    assert_eq!(e.read_by_name(b, "brightness").to_number(), 0.0);
    assert!(e.log_snapshot().iter().any(|l| {
        l.level == LogLevel::Warning
            && l.message
                .contains("writing system settings is not permitted")
    }));
}

#[test]
fn services_the_desktop_lacks_are_refused_not_faked() {
    if !corpus_present("services_the_desktop_lacks_are_refused_not_faked") {
        return;
    }
    let e = engine();
    let root = e.create_from_url("Main", None, Vec::new()).expect("Main");
    let b = backend(&e, root);
    for method in ["vibrate", "notify", "setFullWakeLock", "setPartialWakeLock"] {
        e.invoke_by_name(b, method, &[])
            .unwrap_or_else(|err| panic!("{method}: {err}"));
    }
    // Releasing what is not held is quiet.
    let before = e.log_snapshot().len();
    for method in ["disableFullWakeLock", "disablePartialWakeLock"] {
        e.invoke_by_name(b, method, &[]).expect(method);
    }
    assert_eq!(e.log_snapshot().len(), before);
    let text: String = e
        .log_snapshot()
        .iter()
        .map(|l| l.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    for expect in [
        "cannot vibrate",
        "no notification service",
        "no power manager",
    ] {
        assert!(text.contains(expect), "{expect} not logged in:\n{text}");
    }
}
