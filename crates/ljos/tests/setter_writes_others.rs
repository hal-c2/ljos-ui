//! A setter that takes a `Cx` can write the object's other properties
//! before the write that ran it returns, as a WRITE function does in Qt
//! (#85). It runs for equal writes too, and may not write its own
//! property.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Value};

mod browser {
    use super::*;

    /// The issue's repro: any write of `rootPath`, one of the path already
    /// held included, clears the last failure.
    #[derive(Debug, Default, QmlObject)]
    #[qml(module = "App", name = "Browser")]
    pub struct Browser {
        #[qml(notify, write = Self::keep_root)]
        pub root_path: Atom,
        #[qml(notify)]
        pub error: Atom,
        #[qml(notify, write = Self::keep_depth)]
        pub depth: i32,
    }

    impl Browser {
        fn keep_root(cx: &Cx, path: Atom) -> Option<Atom> {
            cx.set(Self::ERROR, Atom::default()).ok()?;
            Some(path)
        }

        /// Writes its own property, which is refused; what it returns is
        /// what is stored.
        fn keep_depth(cx: &Cx, depth: i32) -> Option<i32> {
            let refused = cx.set(Self::DEPTH, depth + 1);
            assert!(
                matches!(refused, Err(EngineError::WriteInOwnSetter { .. })),
                "{refused:?}"
            );
            Some(depth)
        }
    }
}

use browser::Browser;

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("App" 1.0 => [Browser]));
}

const MAIN: &str = r#"
import QtQuick
import App
Item {
    property alias browser: browser
    property string seen: "unset"
    function open(path) { browser.rootPath = path; seen = browser.error }
    Browser { id: browser; rootPath: "/home" }
}
"#;

fn run() -> (Engine, ObjectId, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let qml = dir.path().join("qml");
    std::fs::create_dir(&qml).unwrap();
    std::fs::write(qml.join("Main.qml"), MAIN).unwrap();
    let types = dir.path().join("ljostypes.json");
    std::fs::write(&types, ljos::types::dump(register).to_json()).unwrap();
    let pack = dir.path().join("qml.pack");
    ljos::build::compile_dir_with_types(&qml, &types, &pack).unwrap();
    let pack = std::fs::read(pack).unwrap();
    let (engine, root) = ljos::App::new("setterWritesOthers")
        .register(register)
        .source(ljos::MapSource::from_pack(&pack).unwrap())
        .root("Main")
        .build()
        .unwrap();
    let browser = match engine.read_by_name(root, "browser") {
        Value::Object(o) => o,
        v => panic!("no browser: {v:?}"),
    };
    (engine, root, browser)
}

fn listen(engine: &Engine, obj: ObjectId, signal: &str) -> Rc<RefCell<u32>> {
    let heard = Rc::new(RefCell::new(0));
    let count = heard.clone();
    engine
        .connect_by_name(obj, signal, move |_, _| *count.borrow_mut() += 1)
        .expect(signal);
    heard
}

#[test]
fn qml_reads_the_cleared_error_right_after_the_assignment() {
    let (engine, root, browser) = run();
    engine
        .set(browser, Browser::ERROR, Atom::from("denied"))
        .unwrap();
    engine
        .invoke_by_name(root, "open", &[Value::from("/tmp")])
        .unwrap();
    assert_eq!(engine.read_by_name(root, "seen"), Value::from(""));
    assert_eq!(
        engine.get(browser, Browser::ROOT_PATH),
        Ok(Atom::from("/tmp"))
    );
}

#[test]
fn an_equal_write_clears_the_error_and_is_not_heard_as_a_path_change() {
    let (engine, root, browser) = run();
    let paths = listen(&engine, browser, "rootPathChanged");
    let errors = listen(&engine, browser, "errorChanged");
    engine
        .set(browser, Browser::ERROR, Atom::from("denied"))
        .unwrap();
    engine
        .invoke_by_name(root, "open", &[Value::from("/home")])
        .unwrap();
    assert_eq!(engine.read_by_name(root, "seen"), Value::from(""));
    assert_eq!(*paths.borrow(), 0);
    assert_eq!(*errors.borrow(), 2);
}

#[test]
fn writes_from_rust_run_the_setter_too() {
    let (engine, _, browser) = run();
    engine
        .set(browser, Browser::ERROR, Atom::from("denied"))
        .unwrap();
    engine
        .set(browser, Browser::ROOT_PATH, Atom::from("/home"))
        .unwrap();
    assert_eq!(engine.get(browser, Browser::ERROR), Ok(Atom::default()));

    engine
        .set(browser, Browser::ERROR, Atom::from("denied"))
        .unwrap();
    engine
        .write_by_name(browser, "rootPath", Value::from("/srv"))
        .unwrap();
    assert_eq!(engine.read_by_name(browser, "error"), Value::from(""));
    assert_eq!(
        engine.read_by_name(browser, "rootPath"),
        Value::from("/srv")
    );
}

#[test]
fn a_setter_that_writes_its_own_property_is_refused() {
    let (engine, _, browser) = run();
    engine.set(browser, Browser::DEPTH, 4).unwrap();
    assert_eq!(engine.get(browser, Browser::DEPTH), Ok(4));
}
