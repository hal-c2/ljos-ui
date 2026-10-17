//! A tree model written in Rust: a `TreeView` shows its rows and the rows
//! under them, a list view of it sees its top level, and the rows it
//! gains are shown where they go.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

/// Folders with files in them: `children` of the path `[]` are the top.
#[derive(Default)]
struct Folders {
    names: RefCell<Vec<(Vec<usize>, String)>>,
    listeners: TreeListeners,
}

impl Folders {
    fn add(&self, engine: &Engine, parent: &[usize], name: &str) {
        let first = self.row_count(parent);
        let mut path = parent.to_vec();
        path.push(first);
        self.names.borrow_mut().push((path, name.to_owned()));
        let change = TreeChange::RowsInserted {
            parent: parent.to_vec(),
            first,
            count: 1,
        };
        self.listeners.emit(engine, &change);
    }
}

impl ItemTree for Folders {
    fn row_count(&self, parent: &[usize]) -> usize {
        let names = self.names.borrow();
        let under = names.iter().filter(|(p, _)| p.len() == parent.len() + 1);
        under.filter(|(p, _)| p.starts_with(parent)).count()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::intern("name")]
    }

    fn data(&self, _: &Engine, path: &[usize], column: usize, role: &str) -> Value {
        let names = self.names.borrow();
        match names.iter().find(|(p, _)| p == path) {
            Some((_, name)) if column == 0 && role == "name" => Value::from(name.as_str()),
            _ => Value::Undefined,
        }
    }

    fn subscribe(&self, listener: TreeListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Folders")]
pub struct FoldersObject {}

#[qml_impl]
impl FoldersObject {}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property alias folders: f
    property alias view: tv
    property alias list: lv
    Folders { id: f }
    TreeView {
        id: tv; width: 200; height: 300
        delegate: Item {
            implicitWidth: 80; implicitHeight: 20
            required property string name; required property int depth
        }
        function names() { var s = []; for (var r = 0; r < rows; ++r) { var c = itemAtCell(Qt.point(0, r)); s.push(c.name + c.depth) } return s.join(" ") }
    }
    ListView { id: lv; width: 200; height: 300; delegate: Item { height: 20 } }
    function show() { tv.model = f; lv.model = f }
}
"#;

#[test]
fn a_tree_view_shows_a_rust_tree_and_follows_it() {
    let engine = ljos::engine();
    engine.register_module(ljos::module!("Example" 1.0 => [FoldersObject]));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();

    let tree = Rc::new(Folders::default());
    tree.add(&engine, &[], "docs");
    tree.add(&engine, &[0], "notes");
    tree.add(&engine, &[], "bin");
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let folders = engine.read_by_name(root, "folders").as_object().unwrap();
    let view = engine.read_by_name(root, "view").as_object().unwrap();
    let list = engine.read_by_name(root, "list").as_object().unwrap();
    engine.set_object_tree(folders, tree.clone()).unwrap();
    assert!(engine.object_tree(folders).is_some());
    engine.invoke_by_name(root, "show", &[]).unwrap();

    let names = || {
        let v = engine.invoke_by_name(view, "names", &[]).unwrap();
        v.to_js_string().to_string()
    };
    assert_eq!(names(), "docs0 bin0");
    // A list sees the top level.
    assert_eq!(engine.read_by_name(list, "count"), Value::Int(2));

    engine
        .invoke_by_name(view, "expand", &[Value::Int(0)])
        .unwrap();
    assert_eq!(names(), "docs0 notes1 bin0");

    tree.add(&engine, &[0], "todo");
    assert_eq!(names(), "docs0 notes1 todo1 bin0");
    assert_eq!(engine.read_by_name(list, "count"), Value::Int(2));
}
