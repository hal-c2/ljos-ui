# Models for Views

## Contents

- Implementing `QmlListModel`
- Reporting changes with `ModelSignals`
- Editable delegates and stable row keys
- How views stay fast on long lists
- See also

After reading this page, you will be able to expose Rust list data to QML views with `QmlListModel`, notify views when rows are inserted, removed, moved, modified or reset, and understand how Ljós keeps large lists cheap.

## Implementing `QmlListModel`

To back a QML view with Rust list data (the role of `QAbstractListModel` in Qt), implement the `QmlListModel` trait and attach your model to an object on the `Engine` with `Engine::set_list_model`.

`QmlListModel` requires three methods: `row_count`, `role_names` and `data`:

```rust
use std::cell::RefCell;
use std::rc::Rc;
use ljos::prelude::*;

#[derive(Debug, Default)]
struct Titles {
    rows: RefCell<Vec<String>>,
}

impl QmlListModel for Titles {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("title")]
    }

    fn data(&self, row: usize, role: &str) -> Value {
        match (self.rows.borrow().get(row), role) {
            (Some(s), "title") => Value::from(s.as_str()),
            _ => Value::Undefined,
        }
    }
}

fn main() {
    let engine = ljos::engine();
    let titles = Rc::new(Titles::default());
    let holder = engine.create_native(QtObject::default(), None).unwrap();
    let signals = engine
        .set_list_model(holder, Rc::clone(&titles) as Rc<dyn QmlListModel>)
        .unwrap();

    let at = titles.rows.borrow().len();
    signals.rows_about_to_be_inserted(&engine, at, 1);
    titles.rows.borrow_mut().push("First".to_owned());
    signals.rows_inserted(&engine, at, 1);

    assert_eq!(titles.row_count(), 1);
}
```

Once `holder` is exposed to QML (for example, through `App::context_property` or as an `ObjectId` property on a native controller), you can bind it to the `model` property of a `ListView`, `GridView`, `Repeater` or `Instantiator`:

```qml
import QtQuick

Item {
    width: 240
    height: 320

    ListView {
        anchors.fill: parent
        model: titlesModel

        delegate: Text {
            required property string title
            width: ListView.view.width
            height: 32
            text: title
        }
    }
}
```

Under the hood, views read through the lower-level `ItemModel` trait (and `Engine::set_object_model`); `Engine::set_list_model` adapts your `QmlListModel` implementation to `ItemModel`.

## Reporting changes with `ModelSignals`

`Engine::set_list_model` returns a `ModelSignals` value that your Rust code uses to tell attached views when rows or roles change:

- `rows_about_to_be_inserted(&engine, at, count)` and `rows_inserted(&engine, at, count)` when inserting `count` rows starting at index `at`.
- `rows_removed` when removing a range of rows.
- `rows_moved` when moving rows from one position to another.
- `data_changed` when role values change within existing rows.
- `roles_changed` when the set of roles changes.
- `model_reset` when the entire model contents are replaced at once. Model resets run inside a reactive graph batch (`Reactive::batch`), so dependent bindings and views flush once after the reset completes.

## Editable delegates and stable row keys

`QmlListModel` provides two optional methods with defaults:

- **`set_data`**: defaults to refusing writes from QML delegates. Override `set_data` if delegates in your view assign back to model roles.
- **`row_key`**: defaults to returning the row's current index. If your model reorders or moves rows, override `row_key` to return a stable identifier for each row so views preserve delegate state across moves.

## How views stay fast on long lists

In Ljós, a long list costs what is visible in the viewport, not what the model holds:

```qml
import QtQuick

Item {
    id: root
    objectName: "root"
    width: 200
    height: 300
    property int created: 0
    property int destroyed: 0
    readonly property int live: created - destroyed

    ListView {
        objectName: "list"
        anchors.fill: parent
        cacheBuffer: 0
        model: 10000
        delegate: Item {
            width: 200
            height: 30
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
        }
    }
}
```

Even with 10,000 rows in the model, at most 14 delegates exist at once for a 300-pixel-tall viewport with `cacheBuffer: 0`. Several mechanisms work together to keep scrolling and model updates cheap:

- **Viewport-only instantiation and lazy bindings.** Delegates are created only near the viewport. Any binding on a delegate that feeds only rendering is evaluated lazily when painted, and cache delegates outside the visible area are incubated in time slices of at most 3 ms per frame at 60 Hz.
- **Linked component and row plans.** What every instance of a delegate component shares (native property indexes, meta-objects, attached types and ID slots) is linked once on first use into a *component plan*, and a view links a *row plan* for the role values handed to each delegate per row. Instantiating a new delegate as the user scrolls is then a set of indexed writes and indexed binding installs without repeated string lookups.
- **Inspecting models at runtime.** In `ljos observe` (or through the MCP inspector's `app_objects` and `app_set_property` tools), you can list every live model—including `QmlListModel` instances, `ListModel`, `ObjectModel`, `DelegateModel`, arrays and numbers—page through its rows with `offset` and `limit`, watch its change count, and edit individual cells.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Native Types in Rust](./native-types.md)
- [Singletons and Multiple Windows](./singletons-and-windows.md)
- [Threads, Tasks and UiHandle](./threads-and-uihandle.md)
