//! A tree model that reads lazily, as a file system or a remote listing
//! is written: a folder not read yet has its expand arrow, the view asks
//! for its rows when it is expanded, and the rows come later, from a
//! timer here as they would from a worker thread's message.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos::prelude::*;
use ljos_runtime::{Clock as _, VirtualClock};

/// The top level is "docs" and "bin"; only "docs" has files in it, read
/// 100 ms after a view asks.
#[derive(Default)]
struct Listing {
    read: RefCell<BTreeMap<Vec<usize>, Vec<&'static str>>>,
    fetches: Cell<u32>,
    listeners: TreeListeners,
}

impl Listing {
    fn new() -> Rc<Self> {
        let me = Rc::new(Self::default());
        me.read.borrow_mut().insert(Vec::new(), vec!["docs", "bin"]);
        me
    }

    fn name(&self, path: &[usize]) -> Option<&'static str> {
        let (last, parent) = path.split_last()?;
        self.read.borrow().get(parent)?.get(*last).copied()
    }
}

/// The model as the engine holds it; the test keeps the `Rc<Listing>`.
struct Lazy(Rc<Listing>);

impl ItemTree for Lazy {
    fn row_count(&self, parent: &[usize]) -> usize {
        self.0.read.borrow().get(parent).map_or(0, Vec::len)
    }

    fn has_children(&self, path: &[usize]) -> bool {
        self.0.name(path) == Some("docs")
    }

    fn can_fetch_more(&self, path: &[usize]) -> bool {
        self.has_children(path) && !self.0.read.borrow().contains_key(path)
    }

    fn fetch_more(&self, engine: &Engine, path: &[usize]) {
        self.0.fetches.set(self.0.fetches.get() + 1);
        let (me, path) = (self.0.clone(), path.to_vec());
        engine.call_at(engine.now() + Duration::from_millis(100), move |engine| {
            me.read
                .borrow_mut()
                .insert(path.clone(), vec!["a.txt", "b.txt"]);
            let change = TreeChange::RowsInserted {
                parent: path,
                first: 0,
                count: 2,
            };
            me.listeners.emit(engine, &change);
        });
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::intern("name")]
    }

    fn data(&self, _: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        match self.0.name(path) {
            Some(name) if column == 0 && role == "name" => Value::from(name),
            _ => Value::Undefined,
        }
    }

    fn subscribe(&self, listener: TreeListener) -> u64 {
        self.0.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.0.listeners.remove(id);
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Files")]
pub struct Files {}

#[qml_impl]
impl Files {}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property alias files: f
    property alias view: tv
    Files { id: f }
    TreeView {
        id: tv; width: 200; height: 300
        delegate: Item {
            implicitWidth: 80; implicitHeight: 20
            required property string name; required property bool hasChildren
            required property int depth
        }
        function names() { var s = []; for (var r = 0; r < rows; ++r) { var c = itemAtCell(Qt.point(0, r)); s.push(c.name + c.depth + (c.hasChildren ? ">" : "")) } return s.join(" ") }
    }
    function show() { tv.model = f }
}
"#;

#[test]
fn a_tree_view_reads_a_folder_when_it_is_expanded() {
    let engine = ljos::engine();
    let clock = Arc::new(VirtualClock::new());
    engine.set_clock(Rc::new(Arc::clone(&clock)));
    engine.register_module(ljos::module!("Example" 1.0 => [Files]));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let files = engine.read_by_name(root, "files").as_object().unwrap();
    let view = engine.read_by_name(root, "view").as_object().unwrap();
    let listing = Listing::new();
    engine
        .set_object_tree(files, Rc::new(Lazy(listing.clone())))
        .unwrap();
    engine.invoke_by_name(root, "show", &[]).unwrap();
    let names = || {
        let v = engine.invoke_by_name(view, "names", &[]).unwrap();
        v.to_js_string().to_string()
    };

    // The folder is not read, and has its arrow.
    assert_eq!(names(), "docs0> bin0");
    assert_eq!(listing.fetches.get(), 0);

    engine
        .invoke_by_name(view, "expand", &[Value::Int(0)])
        .unwrap();
    assert_eq!(listing.fetches.get(), 1);
    assert_eq!(names(), "docs0> bin0");

    clock.set(clock.now() + Duration::from_millis(100));
    engine.advance_timers(clock.now());
    assert_eq!(names(), "docs0> a.txt1 b.txt1 bin0");

    // Read once: expanding again does not ask again.
    engine
        .invoke_by_name(view, "collapse", &[Value::Int(0)])
        .unwrap();
    engine
        .invoke_by_name(view, "expand", &[Value::Int(0)])
        .unwrap();
    assert_eq!(listing.fetches.get(), 1);
    assert_eq!(names(), "docs0> a.txt1 b.txt1 bin0");
}

#[test]
fn a_lazy_tree_says_which_folders_it_has_not_read() {
    let engine = ljos::engine();
    let listing = Listing::new();
    let tree: Rc<dyn ItemTree> = Rc::new(Lazy(listing.clone()));
    // The top level is read; a flat model of it has nothing more to fetch.
    let top = tree_model(tree.clone());
    assert!(!top.can_fetch_more());
    assert!(tree.can_fetch_more(&[0]));
    assert!(!tree.can_fetch_more(&[1]));
    tree.fetch_more(&engine, &[0]);
    assert_eq!(listing.fetches.get(), 1);
}
