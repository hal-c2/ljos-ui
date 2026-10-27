---
title: Singletons and Multiple Windows
description: Share one Engine across all windows while choosing whether each singleton is shared across the application or scoped to a single window.
section: rust
order: 3
---

# Singletons and Multiple Windows

After reading this page, you will be able to register native Rust singletons and QML singletons, run multiple windows on a single `Engine`, and isolate per-window state with `#[qml_singleton(scope = "window")]`.

## Engine-wide singletons

By default, a singleton in Ljós belongs to the `Engine` and is shared across every window. It is instantiated lazily the first time QML or Rust reads it.

To define a native singleton in Rust, add `#[qml_singleton]` below `#[derive(QmlObject)]` and register it with `Engine::register_singleton`:

```rust
use ljos::Engine;
use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "io.qt.filenameprovider", name = "FileNameProvider")]
#[qml_singleton]
pub struct FileNameProvider {}

#[qml_impl]
impl FileNameProvider {
    #[qml(invokable, name = "getFileName")]
    fn get_file_name(_cx: &Cx, path: Atom) -> Atom {
        Atom::from(path.rsplit('/').next().unwrap_or_default())
    }
}

pub fn register(engine: &Engine) {
    engine.register_singleton::<FileNameProvider>("io.qt.filenameprovider", Version::new(1, 0));
}
```

QML files that import the module can call methods or bind to properties on the singleton by its type name:

```qml
import QtQuick
import io.qt.filenameprovider

Item {
    property string label: FileNameProvider.getFileName("file:///home/user/Videos/clip.mp4")
}
```

How singleton creation and context state work:

- **Default versus closure construction.** `engine.register_singleton::<T>("Module", Version::new(1, 0))` constructs the singleton with `T::default()` when first read (even if the type is marked `uncreatable`, which only stops QML from writing `FileNameProvider {}`). When your singleton does not implement `Default` or needs dependencies from the `Engine`, register it with `engine.register_singleton_with("Module", Version::new(1, 0), |engine| T { .. })`.
- **Reading a singleton from Rust.** Call `engine.singleton::<T>()` to access an engine-wide singleton from Rust. For example, `engine.singleton::<Application>()` returns the `Application` singleton behind `Qt.application`, whose `active` and `state` properties report whether the application has focus.
- **QML `pragma Singleton` files.** A QML file with `pragma Singleton` (listed on a `singleton` line in `qmldir` or found via a directory import) is also created lazily once per `Engine`, unless its scripts read a window-scoped singleton.
- **Context properties and initial properties.** When you prefer to pass an object into QML by name (the counterpart of `setContextProperty`), use `App::context_property` or `engine.set_context_property(engine.root_context(), name, value)`. When creating a root with required properties, pass them as initial `(String, Value)` pairs to `engine.create_from_url("Main", None, initial_properties)`.

```rust
use ljos::prelude::*;

#[derive(Default, QmlObject)]
#[qml(module = "Example", name = "Backend")]
pub struct Backend {
    #[qml(notify)]
    pub ready: bool,
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Backend]));
}

fn main() {
    let _app = ljos::App::new("Hello")
        .register(register)
        .context_property("backend", |engine| {
            Value::Object(engine.create_by_name("Example", "Backend", None).unwrap())
        });
}
```

## One Engine across many windows

Ljós uses one `Engine` per application across all open windows. Every window shares the compiled component definitions, the module cache, the JavaScript runtime, engine-wide singletons and shared list models without needing C++ or Rust proxy layers to synchronise state between separate engines.

At the same time, each `Window` is an independent root with its own supervision boundary (`ProcId`), its own focus tree and its own accessibility tree. If you want to embed a `Window` inside another item tree without opening a separate platform window, place it in a `WindowContainer`, which sizes the embedded window to the container. Only windows that are completely separate applications need separate `Engine` instances communicating over message channels.

## Window-scoped singletons

Qt applications that create a separate `QQmlEngine` per window usually do so to give each window its own instance of document or window-controller singletons. In Ljós, you keep one `Engine` and mark those per-window types with `#[qml_singleton(scope = "window")]`:

```rust
use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Document")]
#[qml_singleton(scope = "window")]
pub struct Document {
    #[qml(notify)]
    pub title: String,
}

pub fn run_two_windows(engine: &Engine) -> Result<(), EngineError> {
    engine.register_singleton::<Document>("Example", Version::new(1, 0));

    let first = engine.create_from_url("Main", None, Vec::new())?;
    let second = engine.create_from_url("Main", None, Vec::new())?;

    let _doc = engine.singleton_in::<Document>(second)?;

    engine.on_scoped_singleton(|_engine, s| {
        if s.is::<Document>() && s.event == ScopedEvent::Dropped {
            // save what s.object holds before it goes
        }
    });

    let _ = first;
    Ok(())
}
```

Window scopes follow clear rules:

- **How a scope is opened.** Every root created with no parent (`engine.create_from_url` or `Component.create` from Rust) opens a new window scope.
- **How QML resolves the singleton.** Everything created under that root—including child components and objects created dynamically by scripts with `createObject(null)`—belongs to that window's scope. Writing `Document.title = "Draft"` in one window modifies only that window's `Document`.
- **Lifecycle.** A window's singleton instance is created lazily when something in that window first reads it (running the closure of `register_singleton_with` once for that window) and is destroyed after the window's own objects when the window's root is destroyed. Hot reload preserves each window's singleton instance across reloads.
- **Transitive QML singletons.** If a `pragma Singleton` QML file reads a window-scoped singleton (directly or through another `pragma Singleton` file), that QML singleton automatically becomes window-scoped as well and is destroyed with the window. Any other `pragma Singleton` file remains shared across the Engine.
- **Accessing from Rust.** Calling `engine.singleton::<Document>()` on a window-scoped singleton returns an error because there is no single instance. Instead, call `engine.singleton_in::<Document>(obj)` with any `ObjectId` belonging to that window. Reading a window-scoped singleton where no window scope exists logs a warning once and returns `null`.

## Pre-populating a window scope from Rust

Sometimes the host creates a window's state object in Rust before instantiating the window's root QML component (similar to `qmlRegisterSingletonInstance` in Qt). You can open a window scope ahead of time, insert your object into it, and then create the root inside that scope:

1. Call `engine.window_scope()` to open a fresh scope.
2. Call `engine.set_singleton_in::<T>(scope, obj)` to place an already-created object of type `T` into the scope.
3. Create the window's root inside `engine.in_window_scope(scope, || ...)`; the root adopts that pre-populated scope.

Because the host created `obj` before the scope, the scope treats it as host-owned: when the window closes, the scope removes the object and fires `ScopedEvent::Dropped` to listeners registered with `on_scoped_singleton`, but does not destroy the object itself. In headless tests that use no QML root, you can read the scope directly with `engine.singleton_in_scope::<T>(scope)` and close an unclaimed scope with `engine.close_window_scope(scope)`.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Native Types in Rust](./native-types.md)
- [Models for Views](./models.md)
- [Testing a Rust and QML App](./testing-rust.md)
