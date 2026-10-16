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
