---
title: Testing a Rust and QML App
description: Test QML and native Rust types deterministically on a virtual clock using ljos test, App::build, TestWindow, and render_png.
section: rust
order: 6
---

# Testing a Rust and QML App

After reading this page, you will be able to verify your QML files and native type dump, run QML `TestCase` suites on a virtual clock, write headless Rust unit and integration tests against `Engine` and `App::build`, drive custom visual items and accessibility trees with `TestWindow`, and render deterministic PNG screenshots.

## Checking QML and the native type dump

Before running tests, `ljos check` compiles a QML directory and reports compiler errors, warnings and static lints, exiting with a non-zero status if any file fails to compile:

```sh
ljos check path/to/qml
```

In a Rust application that compiles QML ahead of time, keep a test in `tests/types.rs` that verifies `ljostypes.json` matches your `register` function:

```rust
#[test]
fn the_type_dump_file_is_current() {
    ljos::types::check_file("ljostypes.json".as_ref(), my_app::register).unwrap();
}
```

Run Cargo with `LJOS_TYPES=write` whenever you change a native type's properties, signals, methods or enums, and commit the updated `ljostypes.json`:

```sh
LJOS_TYPES=write cargo test --no-fail-fast   # writes ljostypes.json; commit it
cargo test                                   # fails, naming that command, when it is stale
```

If you want a Rust test to compile a directory of QML files and assert that there are zero warnings or errors without printing to standard error, enable the `dev-qml` feature in `[dev-dependencies]` and call `ljos::dev::compile_quiet(&dirs, Some(&engine))`. It returns the compiled `MapSource` along with every `Diagnostic`; `MapSource::keys()` and `MapSource::failures()` list the components that succeeded and those that failed.

## Running QML `TestCase` files with `ljos test`

`ljos test` runs QML `TestCase` components in `tst_*.qml` files on a virtual clock without opening a platform window. Timers, animations and `wait()` calls advance virtual time immediately without wall-clock sleeps, and the exit status is the number of failed tests:

```sh
ljos test
ljos test --filter test_increment --import tests/imports tests/
```

Pass `--filter <NAME>` (which may be repeated) to run only a specific test case, function or data tag, and `--import <DIR>` to add a module import path used only by tests.

A `tst_*.qml` file uses `import QtTest`:

```qml
import QtQuick
import QtTest

TestCase {
    name: "MathTests"

    function test_addition() {
        compare(2 + 2, 4)
    }
}
```

## Testing native types and QML headlessly in Rust

You test native Rust types through the same `Engine` interface that QML uses. For a full application configured with `ljos::App`, calling `App::build` instead of `App::run` constructs the `Engine` and instantiates the root component headlessly so a test can inspect properties and invoke methods.

For focused integration tests of a native module against inline QML, register your types on `ljos::engine()`, compile the QML snippet with `ljos_compiler` and `EngineOracle`, and assert on the resulting object state:

```rust
use ljos::dev::EngineOracle;
use ljos::prelude::*;
use ljos_compiler::{Project, Severity, compile_file};

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

fn setup_engine(main: &str) -> (Engine, ObjectId) {
    let types = ljos::engine();
    register(&types);
    let mut project = Project::new(EngineOracle::from_engine(&types));
    project.add("Main", main);
    let compiled = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    let errors: Vec<_> = compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    let engine = ljos::engine();
    register(&engine);
    engine.set_component_source(source);
    let root = engine.create_from_url("Main", None, vec![]).unwrap();
    (engine, root)
}

#[test]
fn a_script_names_a_file_by_the_end_of_its_url() {
    let (e, root) = setup_engine(
        r#"import QtQuick
import io.qt.filenameprovider
Item {
    property string bound: FileNameProvider.getFileName("a/b.mp4")
    property string called: ""
    function call(path) { called = FileNameProvider.getFileName(path.toString()) }
}
"#,
    );
    e.invoke_by_name(root, "call", &[Value::from("file:///v/clip.mp4")])
        .unwrap();
    assert_eq!(
        e.read_by_name(root, "bound").to_js_string().as_str(),
        "b.mp4"
    );
    assert_eq!(
        e.read_by_name(root, "called").to_js_string().as_str(),
        "clip.mp4"
    );
}
```

Headless tests run against isolated, in-process stand-ins for every platform service:

- **Home directory isolation.** `Platform` resolves standard directories through `PathEnv::for_process`, which automatically gives every Rust test binary (any binary under `target/*/deps`, or any run guarded by `LJOS_GUARD_HOME`) a fresh temporary directory for `HOME` and XDG paths. `Settings` and `LocalStorage` in tests never read or write the developer's real home directory.
- **Service stand-ins.** A headless `Engine` uses an in-memory clipboard, records URLs passed to `Qt.openUrlExternally` and badge counts passed to `Platform::set_badge_number` without touching the desktop, fails network fetches with `FetchError::NoNetwork` unless `set_fetcher` is called, and uses `NoSensors` unless the test installs `FakeSensors`. Without a display host, `Screen` reports a single 800×800 screen and `Qt.application` is active.

## Driving input and accessibility with `TestWindow`

When you write a custom visual item with `ljos::item` or want to test keyboard, pointer, wheel and IME input along with accessibility, enable the `testing` feature on `ljos` and use `ljos::testing::TestWindow`:

- **Virtual clock and input.** `TestWindow` hosts a scene without a platform window, delivers key presses, pointer moves and clicks, wheel scrolling and IME composition, and resizes the scene with `resize` on the virtual clock.
- **Waiting on compute pool tasks.** If an item under test spawns work on the compute pool with `ljos::task::spawn`, `TestWindow::advance` waits for outstanding task replies before advancing time. If the background work waits on a mock server on the test thread, register a callback with `serve_while_waiting` (backed by `Platform::finish_with`) so the test thread can serve requests while waiting.
- **Accessibility tree inspection.** Interactive elements are addressed by role and accessible name rather than coordinates. Call `set_accessibility_active` on `TestWindow` to attach an accessibility observer, and inspect the resulting `AccessibleNode` tree with `accessibility_tree`, `accessible_node` and `accessible_item`. Each node reports its role word, computed name, description, state words (such as `"checked"` or `"disabled"`) and `focused`, while `ljos::item`'s `child_items`, `overlay`, `map_to_scene` and `scene_rect` let you verify geometry.

```rust
// <!-- check: verify exact TestWindow constructor and method signatures under features = ["testing"] -->
use ljos::prelude::*;

#[test]
fn headless_engine_starts_active() {
    let engine = ljos::engine();
    let app = engine.singleton::<Application>().unwrap();
    let _ = app;
}
```

## Screenshot tests with `ljos::render`

With the `render` feature enabled on `ljos`, a Rust test can render the scene returned by `App::build` to a PNG image:

1. Install `ljos::render::Picture` before creating the root so the root's timers and animations run on the virtual clock.
2. Call `ljos::render::set_fonts` to select the bundled font (so text renders identically on every machine) or the system's fonts.
3. Call `ljos::render::render_png(&engine, root, (width, height), settle_ms)` to advance the virtual clock by `settle_ms` milliseconds and render the scene to a PNG.

This is the same renderer that powers the `ljos render` CLI command.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Native Types in Rust](./native-types.md)
- [Threads, Tasks and UiHandle](./threads-and-uihandle.md)
- [Desktop, Mobile and the Platform Host](./platforms.md)
