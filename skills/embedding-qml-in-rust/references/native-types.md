# Native Types in Rust

## Contents

- Defining a `QmlObject`
- Property attributes and custom setters
- Passing structured data and calling JavaScript
- Extending `Item` and custom painting
- Ahead-of-time QML compilation and `ljostypes.json`
- Previewing without your Rust
- See also

After reading this page, you will be able to expose Rust structs to QML with reactive properties, custom setters, signals and invokable methods, pass structured data across the boundary, compile QML against your types ahead of time, and write custom visual items that extend `Item`.

## Defining a `QmlObject`

A native type is a Rust struct annotated with `#[derive(QmlObject)]` and `#[qml(module = "...", name = "...")]`. Struct fields become QML properties, and an accompanying `#[qml_impl]` block declares signals, invokable methods and completion hooks.

`use ljos::prelude::*;` brings the derive macro, `#[qml_impl]`, `Cx`, `Engine`, `EngineError` and property types into scope:

```rust
use ljos::prelude::*;

#[derive(Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
}

fn main() {
    let engine = ljos::engine();
    register(&engine);
    let counter = engine.create_by_name("Example", "Counter", None).unwrap();
    engine.invoke_by_name(counter, "increment", &[]).unwrap();
    assert_eq!(engine.get(counter, Counter::COUNT), Ok(1));
}
```

Once registered on the `Engine` with `ljos::module!`, QML files can import the module and instantiate `Counter`:

```qml
import QtQuick
import Example 1.0

Item {
    width: 200
    height: 100

    Counter {
        id: counter
        objectName: "mainCounter"
    }

    Text {
        anchors.centerIn: parent
        text: "Count: " + counter.count
    }

    MouseArea {
        anchors.fill: parent
        onClicked: counter.increment()
    }
}
```

Key details of how `#[derive(QmlObject)]` works:

- **Typed property handles.** For each exposed field (such as `count`), the derive generates a typed handle constant (`Counter::COUNT`). Inside `#[qml_impl]` methods you read and write properties through `cx.get(Counter::COUNT)` and `cx.set(Counter::COUNT, value)`, and outside you use `engine.get(obj, Counter::COUNT)`.
- **`QtObject` is the implied base.** Any creatable type that has a `module` and does not specify `extends` builds on `QtObject`. It automatically has `objectName` at compile time and runtime without needing a field on your Rust struct.
- **`final` by default.** Properties declared in Rust are `final`, allowing the QML compiler to resolve member accesses statically.
- **Explicit registration.** Types are registered explicitly via `ljos::module!` rather than linker sections, keeping static linking on iOS deterministic and hot reload straightforward.

## Property attributes and custom setters

You can control how fields and methods are exposed to QML with `#[qml(...)]` attributes:

- `#[qml(notify)]` adds a change notification signal (`countChanged` for `count`) so QML bindings track the property.
- `#[qml(readonly, notify)]` exposes a property that QML can read and bind to, but cannot assign from QML. Snake-case field names in Rust (such as `painted_width`) are exposed in camelCase (`paintedWidth`) in QML.
- `#[qml(name = "getFileName")]` overrides the QML name of a property or invokable method.
- `#[qml(group)]` embeds a grouped property struct (such as `border: Border`), which is stored inline rather than as a separate `QtObject`.
- `#[qml(skip)]` omits a private Rust field from the `MetaObject`.
- `#[qml(reset)]` allows QML code or a binding to assign `undefined` to restore the property's default value; without `reset`, assigning `undefined` is rejected with a type error.
- `enumeration = "Name"` and `flags = "Name"` associate a property with a registered enum or bitflags type.
- `#[qml(on_complete)]` on a method in `#[qml_impl]` runs during component creation right before `Component.onCompleted`.

When a property needs a custom setter that clamps or rejects values (like a `WRITE` function on a `Q_PROPERTY` in Qt), use `#[qml(write = Self::name)]`:

- **Validating or clamping the value alone:** write a method `fn(&self, T) -> Option<T>`. Every write—from QML, a binding, an animation or Rust—passes through this function before being stored. Return `Some(new_value)` to store the (possibly clamped) value, or `None` to refuse the write. If the returned value equals the value already stored, no change notification is emitted.
- **Updating sibling properties during a write:** write a method `fn(cx: &Cx, T) -> Option<T>`. You can update other properties on the same object via `cx.set(Self::OTHER, ...)` before returning `Some(new_value)`. Those sibling writes go through their own setters and notify their observers before the assignment returns. A setter may not call `cx.set` on its own property (doing so returns `EngineError::WriteInOwnSetter`); instead, it returns the value to store.

```rust
use ljos::prelude::*;

#[derive(Default, QmlObject)]
#[qml(module = "Example", name = "BoundedValue")]
pub struct BoundedValue {
    #[qml(notify, write = Self::clamp_level)]
    pub level: i32,
}

// <!-- check: verify whether custom setter methods live in a plain impl block or #[qml_impl] block -->
impl BoundedValue {
    fn clamp_level(&self, value: i32) -> Option<i32> {
        if value < 0 {
            None
        } else {
            Some(value.min(100))
        }
    }
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [BoundedValue]));
}
```

## Passing structured data and calling JavaScript

Structured data crosses the Rust–QML boundary directly as typed `Map` and `List` values without serialising to or parsing from JSON text:

- **Rust collections as QML `var` data.** Any property, method parameter or return value of type `Vec<T>`, `Option<T>` (where `None` maps to `null`), `HashMap<String, T>`, `BTreeMap<String, T>` or `serde_json::Value` has `TypeTag::Data`, which QML sees as `var`. Returning `Vec<HashMap<String, Value>>` from an invokable gives QML a list of plain objects (`rows[1].path`), and `Value::map([("path", Value::from("a.txt")), ("size", Value::Int(42))])` builds a single record.
- **Coercion rules on parameters.** A `Vec<T>` parameter that is omitted or passed `null` becomes an empty `Vec`, and a single non-list argument is wrapped into a one-element `Vec` (matching QML `list` property coercion). Use `Option<Vec<T>>` if you need to distinguish an omitted argument from an empty list.
- **`serde_json::Value` parameters.** A `serde_json::Value` parameter copies nested JavaScript objects and arrays using `JSON.stringify` semantics: functions and `undefined` fields are omitted, while QML `ObjectId` references or value types (`point`, `font`) are rejected with an error naming the method, parameter and path (`Sink.json: payload.item is a QML object, not JSON`). Take a `Value` parameter if you want to accept QML objects, or use `engine.deep_copy(&v)` to copy a `var` into Rust `Map` and `List` values.
- **Runtime-keyed objects.** When the set of property names is not known at compile time at all, use `PropertyMap` (in `ljos::prelude`) to create an object with per-key reactive properties at runtime.
- **Calling a JavaScript function from Rust.** If Rust holds a reference to a JavaScript function, call it with `Engine::try_call_js`. If the function throws, it returns `JsError::Thrown`, which carries both `value` (the thrown `Error` object or primitive, whose properties you can read with `Engine::js_member`) and `JsError::message(&engine)` (which returns the error's `message` field without a leading `Error: ` prefix).

## Extending `Item` and custom painting

To create a visual QML type in Rust that participates in layout and anchoring, add `extends = Item` to `#[qml(...)]` and include a `#[qml(base)] pub item: Item` field.

When your item also paints custom graphics (the counterpart of `QQuickPaintedItem`, such as a gauge, chart or terminal view), implement `ljos::item::ItemPaint` and register a paint hook with `ljos::item::register_paint_item`. Reading properties inside the hook via `item::prop` is tracked in the reactive graph, so the item automatically repaints whenever those properties change:

```rust
use std::rc::Rc;

use ljos::item::{self, DisplayListBuilder, ItemPaint, Rgba, TextEngine, kurbo};
use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Gauge", extends = Item)]
pub struct Gauge {
    #[qml(base)]
    pub item: Item,
    /// How full, 0 to 1.
    #[qml(notify)]
    pub value: f64,
}

/// What a gauge shows: a bar as long as its value.
#[derive(Debug)]
struct Bar(f64);

impl ItemPaint for Bar {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        let filled = kurbo::Rect::new(0.0, 0.0, size.width * self.0, size.height);
        out.fill_rect(filled, Rgba::BLACK);
    }
}

fn read(engine: &Engine, gauge: ObjectId) -> Rc<dyn ItemPaint> {
    Rc::new(Bar(item::prop(engine, gauge, Gauge::VALUE)))
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Gauge]));
    item::register_paint_item::<Gauge>(engine, read);
}
```

The `ljos::item` module provides the remaining hooks a custom visual item needs without depending on internal engine crates:

- **Input hooks:** `register_key_item` (receives keys while the item has active focus via `item::force_active_focus`), `register_wheel_item`, `register_pointer_item`, `register_hover_move` (pointer motion with no button held), and `register_text_input_client` (IME committed text and cursor rectangle placement).
- **Text measurement and display scale:** outside `paint`, measure text with `item::with_text_engine` and `item::text_style` (inside `paint`, the `&mut TextEngine` is passed directly). Read the window's scale factor with `item::device_pixel_ratio`.
- **Imperative repaints and syntax highlighting:** call `item::repaint` when external state not stored in a QML property changes, and use `item::set_highlighter` to attach a syntax highlighter to a `TextEdit`.

## Ahead-of-time QML compilation and `ljostypes.json`

Because Cargo runs `build.rs` before compiling your crate's library or binary, a build script cannot call your crate's `register` function directly when the native types live in the same crate. Instead, your test suite exports a description of your registered native types to `ljostypes.json`, and `build.rs` compiles QML against that file.

Add a test in `tests/types.rs`:

```rust
#[test]
fn the_type_dump_file_is_current() {
    ljos::types::check_file("ljostypes.json".as_ref(), my_app::register).unwrap();
}
```

And compile your `qml/` directory in `build.rs`:

```rust
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    ljos::build::compile_dir_with_types(
        "qml".as_ref(),
        "ljostypes.json".as_ref(),
        &out.join("qml.pack"),
    )
    .unwrap();
}
```

When you add or change a native type, update `ljostypes.json` and commit it:

```sh
LJOS_TYPES=write cargo test --no-fail-fast   # writes ljostypes.json; commit it
cargo test                                   # fails, naming that command, when it is stale
```

With `ljostypes.json` in place, the compiler checks every member on your native types—including grouped properties, attached types and enum keys—at build time, failing the build on any unknown property or signal handler name (`onBumpd:`, `Counter.Eleven`). If your native types live in a separate library crate, your application's `build.rs` can call `ljos::build::compile_dir("qml".as_ref(), natives::register, &out)` directly without a `ljostypes.json` file.

## Previewing without your Rust

`ljos run` has none of your Rust, but it can show your interface from the same file. It reads the file `--types` names, otherwise `ljostypes.json` in the project directory (the entry file's directory, or `--project`). It looks nowhere else.

```sh
ljos run ui/Main.qml --project .
```

Your QML is checked against the file as your build checks it. Each type no Rust code provides stands in with what the file declares, and the run says so once:

```text
ljos: ljostypes.json: App.Media.Player, App.Media.Wave stand in: no Rust code provides them
```

A stand-in's properties hold what they are given and report changes. Its signals can be emitted and handled. Its methods do nothing and return `undefined`. Its enums and grouped properties work, and a type based on `Item` is laid out as one.

- A type's attached type is not made. The run says which ones, and `Player.rank:` gives a warning that the type has no attached type.
- A file that was found and cannot be read, has another format or names an unknown base stops the run.
- With `--watch`, saved QML is compiled against the file, but the file itself is read once: restart after you change it.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Singletons and Multiple Windows](./singletons-and-windows.md)
- [Models for Views](./models.md)
- [Testing a Rust and QML App](./testing-rust.md)
