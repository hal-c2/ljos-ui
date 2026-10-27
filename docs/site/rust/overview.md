---
title: How a Rust and QML App Fits Together
description: Understand how Ljós runs your QML and Rust code on one Engine, talks to background threads through bounded messages, uses 0 % CPU when idle, and isolates faults.
section: rust
order: 1
---

# How a Rust and QML App Fits Together

After reading this page, you will understand how a Ljós application is structured around the `Engine`, how background threads communicate with the UI thread without locks, why an idle application uses 0 % CPU, and how faults and boundaries keep the interface running when code fails.

## Starting an application with `ljos::App`

You write the Rust side of a Ljós application against the `ljos` facade crate alone. Import `ljos::prelude::*` for the traits, types and macros, and use `ljos::App` to configure and run the application. What the prelude does not re-export is internal to the engine crates and may change without notice.

Add `ljos` as your only UI dependency in `Cargo.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2024"

[dependencies]
ljos = { git = "https://github.com/olafura/ljos-ui", rev = "…" }

[build-dependencies]
ljos = { git = "https://github.com/olafura/ljos-ui", rev = "…", features = ["build"] }
```

In `src/main.rs`, register your native modules, pass the compiled QML pack, name the root component, and start the event loop:

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
    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

pub mod natives {
    use super::*;

    pub fn register(engine: &Engine) {
        engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
    }
}

fn main() {
    ljos::App::new("Hello")
        .size(640.0, 420.0)
        .register(natives::register)
        .source(ljos::include_qml!("qml.pack"))
        .root("Main")
        .run()
        .unwrap();
}
```

A shipped application compiles its QML ahead of time in `build.rs` into a binary pack in `OUT_DIR` and embeds it with `ljos::include_qml!`. Because the QML is already lowered to intermediate representation (`ljos_ir`), the shipped binary links no QML parser or compiler. During development, you can enable the non-default `dev-qml` feature and use `App::qml_dir` (or `App::qml_dirs` for multiple import trees) to compile QML from disk at start-up.

## The Engine and the UI thread

An application normally has one `Engine`. It lives on the main UI thread and is `!Send`, so it can never be moved across threads or shared behind a mutex.

Inside the `Engine` live:

- **The object store**, which keeps every QML and native object in a generational `SlotMap<ObjectId, ObjectHeader>`. Because `ObjectId` keys carry a generation counter, destroying an object never causes a use-after-free; any remaining reference in JavaScript reads as `null`.
- **The `MetaObject` registry**, which stores property, signal and method tables for both Rust types and QML components. Derived types share inherited member entries behind `Arc` pointers rather than copying tables.
- **The reactive dependency graph (`ljos_reactive`)**, which tracks property dependencies and runs bindings and effects.
- **A single JavaScript runtime and context (`rquickjs`)**, shared across all components and windows in the Engine. Wrapper objects are created lazily when a script first touches a QML object, so a UI that runs no JavaScript never starts the JS host.
- **The time-sliced `Incubator`**, which creates asynchronous components (`Loader.asynchronous`, `Component.incubateObject`, `ListView` cache delegates and `StackView` pushes) in slices of at most 3 ms per frame at 60 Hz (scaled down at higher refresh rates, never less than 250 µs).
- **The frame scheduler and render tree**, which serve work in priority order (`Input`, `Critical`, `Visible`, `Prefetch`, `Deferred`, `Idle`) and hand shapes and texture updates to the host shell.

Reading a property can evaluate a binding, which can call into JavaScript, which can call back into a Rust method. To keep re-entrant calls safe, the `Engine` never holds a single `RefCell` borrow across a JavaScript call; instead, its internal state is split into separately borrowed parts (graph, objects, JS) and every Engine API takes short borrows.

## Threads and bounded messages

Ljós follows a strict ownership rule: one owner holds each piece of state, and threads talk to each other only through bounded messages (`UiMsg`). No locks are taken on the property binding path.

| Thread | What it owns | How it talks to the UI |
|---|---|---|
| UI (main) | `Engine`: objects, reactive graph, JS runtime, scheduler, render tree, host shell | Drains `UiMsg` from bounded inboxes within the frame budget |
| `tokio` worker pool (2 workers on mobile, up to 4 on desktop) | Network requests, SQLite (`LocalStorage`), file watcher, developer tools socket | Sends `UiMsg` via `UiHandle` |
| Bounded compute pool (2 to 4 threads) | Image decoding at `sourceSize`, QML parsing and compilation, off-thread text shaping, `ljos::task::spawn` jobs | Delivers results via `UiMsg` to the platform inbox |
| Pinned runtime threads | Isolated runtimes with their own memory limits, stack limits and interrupt deadlines | Sends `UiMsg` via `UiHandle` |

When a background thread sends the first message into an empty inbox through a `UiHandle`, it sets an `AtomicBool` wake flag and calls `request_repaint` on the window host. At the next frame, the UI thread drains queued messages within the frame budget. If a burst of messages exceeds the budget, the remaining messages stay queued in the bounded inbox, which applies back-pressure to background senders without spinning or blocking the UI thread.

## Why an idle app uses 0 % CPU

Nothing in Ljós polls. The event loop sleeps until input arrives, a timer or animation fires, a state write marks the scene dirty, or a background thread posts a `UiMsg`.

The reactive graph is push-pull with three node states (`Clean`, `Check`, `Dirty`) and allocates nodes lazily only when a property is observed or bound:

1. **Writes push dirty marks and flush effects synchronously.** Writing a new value to a property marks its direct observers `Dirty` and its transitive observers `Check` (writing an equal value does not propagate). Eager effect roots—signal handlers, `Connections`, `Behavior` interceptors, Rust subscribers, layout polish effects, and accessibility observers when a screen reader is attached—then flush synchronously in height order before control returns to the writer.
2. **Render bindings are pulled lazily on demand.** Every binding that is not an effect root stays dirty until something reads it. Because the paint pass only visits visible items near the viewport, bindings on hidden or off-screen items do no work at all.
3. **Off-screen content costs close to nothing.** A `ListView` with 10,000 items creates only the delegates visible in the viewport (plus `cacheBuffer`). Items that are not painted do no layout, text shaping or GPU work.
4. **Subsystems rest when unused.** JavaScript garbage collection runs in the `Idle` lane only after 100 ms of rest following work that grew the JS heap. A `MediaPlayer` or `SoundEffect` that is not playing, a `VideoOutput` that is not visible, and an inactive sensor schedule no timers or frames. Even `TextInput.canPaste` only checks the system clipboard when the window becomes active or an editor receives keyboard focus.

## Faults and supervision

A runaway script, a failed binding or a Rust panic in a handler should not freeze the frame or crash the process. Ljós takes its fault-isolation model from Erlang/BEAM:

- **One fault pipeline.** Every failure (`Fault::{JsException, JsInterrupted, JsOutOfMemory, Panic, Startup}`) goes through `Engine::report_fault`. It is logged with its QML file and line (plus the QML call stack for JavaScript exceptions), recorded in a 32-entry fault history, and queued for delivery at the start of the next frame (`Engine::deliver_faults`). Delivering faults between turns ensures the Engine never destroys an object while its own handler is still on the call stack.
- **Rust panics are caught at the turn boundary.** Panics inside native invokable methods, signal handlers, completion hooks, `on_message` callbacks and background task completions are caught and turned into `Fault::Panic`. Other handlers connected to the same signal still run.
- **Runaway JavaScript is interrupted.** The JavaScript host checks an interrupt deadline roughly every 10,000 opcodes (about 266 µs). A handler that enters an infinite loop (`while (true) {}`) is stopped with `Fault::JsInterrupted` and the application keeps working. Time spent inside native work requested by a script (such as `Qt.createComponent` or `createObject`) is credited back so slow native setup does not trip the script's interrupt limit.
- **UI boundaries isolate and restart subtrees.** `Loader`, `StackView` pages, `Window` and `ErrorBoundary` (in the `Ljos` QML module) act as supervision boundaries, each with its own logical owner ID (`ProcId`).

You can wrap any part of your QML tree in an `ErrorBoundary` to catch faults in its children and show a fallback component:

```qml
import QtQuick
import Ljos

Item {
    width: 400
    height: 300

    ErrorBoundary {
        id: boundary
        anchors.fill: parent
        catchExceptions: true
        maxRestarts: 2
        within: 1000

        fallback: Column {
            spacing: 8
            Text {
                text: boundary.fault.kind + ": " + boundary.fault.message
            }
            Text {
                text: "Line " + boundary.fault.line
            }
            MouseArea {
                width: 80
                height: 30
                onClicked: boundary.retry()
            }
        }

        Rectangle {
            anchors.fill: parent
            color: "steelblue"
        }
    }
}
```

By default, as in Qt, a JavaScript exception in a handler or binding is logged as a warning and stops only that handler or binding. Setting `catchExceptions: true` on an `ErrorBoundary` treats JavaScript exceptions inside it the same way as panics, script interrupts, out-of-memory faults and startup failures (such as a missing `required` property): the boundary destroys only its child content and instantiates `fallback`. Calling `boundary.retry()` destroys the fallback and recreates the content from its component definition. An `ErrorBoundary` without a `fallback` restarts its content immediately until it exceeds `maxRestarts` within `within` milliseconds, at which point it gives up and escalates the fault to the boundary around it (or to the application's fault handler if no outer boundary exists).

To inspect supervision boundaries, recent faults, mailbox depths, wake causes, hot bindings and memory owners in a running application, run:

```sh
ljos observe
```

## See also

- [Native Types in Rust](./native-types.md)
- [Singletons and Multiple Windows](./singletons-and-windows.md)
- [Threads, Tasks and UiHandle](./threads-and-uihandle.md)
- [Testing a Rust and QML App](./testing-rust.md)
