//! An application's native singletons, as it registers and reads them.

use ljos::prelude::*;

/// One object the engine owns that QML reads but may not write as
/// `Store {}`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Store", uncreatable)]
#[qml_singleton]
pub struct Store {
    #[qml(constant, readonly)]
    pub n: i32,
}

/// The same without `uncreatable`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Plain")]
#[qml_singleton]
pub struct Plain {
    #[qml(constant, readonly)]
    pub n: i32,
}

fn engine() -> Engine {
    let engine = ljos::engine();
    engine.register_singleton::<Store>("Example", Version::new(1, 0));
    engine.register_singleton::<Plain>("Example", Version::new(1, 0));
    engine
}

fn compile(engine: &Engine, main: &str) -> Result<ljos::MapSource, String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), main).unwrap();
    ljos::dev::compile_dir_with(dir.path(), engine)
}

#[test]
fn an_uncreatable_singleton_is_made_by_the_engine() {
    let engine = engine();
    assert!(engine.singleton::<Plain>().is_ok());
    let store = engine.singleton::<Store>().unwrap();
    assert_eq!(engine.singleton::<Store>().unwrap(), store);
}

#[test]
fn qml_reads_an_uncreatable_singleton() {
    let engine = engine();
    let source = compile(
        &engine,
        "import QtQuick\nimport Example\nItem { property int n: Store.n + 1 }\n",
    )
    .unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    assert_eq!(engine.read_by_name(root, "n").to_number(), 1.0);
}

#[test]
fn qml_cannot_make_an_uncreatable_singleton() {
    // As for any uncreatable native type: the compiler leaves the types
    // the runtime provides to it, and making the file fails.
    let engine = engine();
    let main = "import QtQuick\nimport Example\nItem { Store {} }\n";
    let made = compile(&engine, main).map(|source| ljos::create_root(&engine, source, "Main"));
    match made {
        Ok(Err(EngineError::NotCreatable(name))) => assert_eq!(name, "Store"),
        Err(err) => assert!(err.contains("did not compile"), "{err}"),
        other => panic!("Store {{}} was made: {other:?}"),
    }
}

/// A singleton handed what it depends on, which `Default` cannot give.
#[derive(Debug, QmlObject)]
#[qml(module = "Example", name = "Library", uncreatable)]
#[qml_singleton]
pub struct Library {
    #[qml(constant, readonly)]
    pub books: i32,
}

#[test]
fn a_singleton_is_made_by_the_factory_it_was_registered_with() {
    let shelf = std::rc::Rc::new(vec!["a", "b", "c"]);
    let engine = ljos::engine();
    let made = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = made.clone();
    engine.register_singleton_with("Example", Version::new(1, 0), move |_| {
        count.set(count.get() + 1);
        Library {
            books: shelf.len() as i32,
        }
    });
    let source = compile(
        &engine,
        "import QtQuick\nimport Example\nItem { property int books: Library.books }\n",
    )
    .unwrap();
    assert_eq!(made.get(), 0, "made when first read, not when registered");
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    assert_eq!(engine.read_by_name(root, "books").to_number(), 3.0);
    let library = engine.singleton::<Library>().unwrap();
    assert_eq!(
        engine.with::<Library, _>(library, |l| l.books).ok(),
        Some(3)
    );
    assert_eq!(made.get(), 1);
}

/// One for each window: every root the application makes has its own.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Document")]
#[qml_singleton(scope = "window")]
pub struct Document {
    #[qml(notify)]
    pub title: String,
}

#[test]
fn each_window_reads_its_own_singleton_through_both_tiers() {
    let engine = ljos::engine();
    let made = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = made.clone();
    engine.register_singleton_with("Example", Version::new(1, 0), move |_| {
        count.set(count.get() + 1);
        Document::default()
    });
    engine.set_tier1(true);
    engine.set_tier1_check(true);
    let source = compile(
        &engine,
        "import QtQuick\nimport Example\nItem { property string title: Document.title + \"!\" }\n",
    )
    .unwrap();
    let a = ljos::create_root(&engine, source, "Main").unwrap();
    let b = engine.create_from_url("Main", None, Vec::new()).unwrap();
    let text = |root| match engine.read_by_name(root, "title") {
        Value::String(s) => s.to_string(),
        other => panic!("{other:?}"),
    };
    assert_eq!((text(a), text(b)), ("!".into(), "!".into()));
    assert_eq!(made.get(), 2, "the factory makes one for each window");
    let doc = engine.singleton_in::<Document>(a).unwrap();
    assert_ne!(Ok(doc), engine.singleton_in::<Document>(b));
    engine
        .write_by_name(doc, "title", Value::from("a"))
        .unwrap();
    assert_eq!(text(a), "a!");
    assert_eq!(text(b), "!");
    let stats = engine.tier1_stats();
    assert!(stats.checked >= 1, "{stats:?}");
    assert_eq!(stats.mismatches, 0, "{stats:?}");
    let warnings = engine.take_warnings();
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// One for each window, which a host makes before the window's QML.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Bridge", uncreatable)]
#[qml_singleton(scope = "window")]
pub struct WindowBridge {
    #[qml(notify)]
    pub title: Atom,
}

#[test]
fn a_window_reads_the_object_its_host_made_before_it() {
    let engine = ljos::engine();
    engine.register_singleton::<WindowBridge>("Example", Version::new(1, 0));
    // The host's per-window object, made and written before the QML.
    let made = engine
        .create_native(
            WindowBridge {
                title: Atom::from("two"),
            },
            None,
        )
        .unwrap();
    let window = engine.window_scope();
    engine
        .set_singleton_in::<WindowBridge>(window, made)
        .unwrap();
    let source = compile(
        &engine,
        "import QtQuick\nimport Example\nItem { property string title: Bridge.title }\n",
    )
    .unwrap();
    let root = engine
        .in_window_scope(window, || ljos::create_root(&engine, source, "Main"))
        .unwrap();
    assert_eq!(engine.singleton_in::<WindowBridge>(root), Ok(made));
    assert_eq!(
        engine
            .to_js_string(&engine.read_by_name(root, "title"))
            .to_string(),
        "two"
    );
    // The window goes; the host's object stays, for the host to destroy.
    engine.destroy(root);
    engine.process_deferred_deletes();
    assert!(engine.is_alive(made));
}

#[test]
fn a_test_with_no_qml_reads_a_window_s_object() {
    let engine = ljos::engine();
    engine.register_singleton::<WindowBridge>("Example", Version::new(1, 0));
    let window = engine.window_scope();
    let made = engine.singleton_in_scope::<WindowBridge>(window).unwrap();
    assert_eq!(engine.singleton_in_scope::<WindowBridge>(window), Ok(made));
    let other = engine.window_scope();
    assert_ne!(engine.singleton_in_scope::<WindowBridge>(other), Ok(made));
    // Its end destroys what it made.
    engine.close_window_scope(window).unwrap();
    engine.process_deferred_deletes();
    assert!(!engine.is_alive(made));
    assert!(engine.singleton_in_scope::<WindowBridge>(window).is_err());
}

#[test]
fn a_window_takes_one_object_of_a_type() {
    let engine = ljos::engine();
    engine.register_singleton::<WindowBridge>("Example", Version::new(1, 0));
    let window = engine.window_scope();
    let first = engine.create_native(WindowBridge::default(), None).unwrap();
    let second = engine.create_native(WindowBridge::default(), None).unwrap();
    engine
        .set_singleton_in::<WindowBridge>(window, first)
        .unwrap();
    assert!(
        engine
            .set_singleton_in::<WindowBridge>(window, second)
            .is_err()
    );
    let store = engine.create_native(Store::default(), None).unwrap();
    assert!(
        engine
            .set_singleton_in::<WindowBridge>(window, store)
            .is_err()
    );
    // A window's object outlives a scope no root took.
    engine.close_window_scope(window).unwrap();
    engine.process_deferred_deletes();
    assert!(engine.is_alive(first));
}
