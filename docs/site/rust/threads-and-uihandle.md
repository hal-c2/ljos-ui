---
title: Threads, Tasks and UiHandle
description: Run heavy compute and network work off the UI thread with ljos::task::spawn, Platform::fetch and UiHandle.
section: rust
order: 5
---

# Threads, Tasks and UiHandle

After reading this page, you will be able to offload CPU-intensive work to the compute pool with `ljos::task::spawn`, send asynchronous HTTP requests from Rust and QML, post bounded and coalesced messages from background threads through `UiHandle`, and spot slow native calls on the UI thread.

## Why the UI thread never blocks

The `Engine` is single-threaded (`!Send`) and lives on the UI thread. Every native method invoked from QML runs synchronously on that thread.

If an invokable method blocks on a slow file read, a network request or heavy parsing, the UI thread cannot advance animations or paint the next frame until the method returns. To keep frames responsive, you start background work from the UI thread and deliver the finished value back to the `Engine` between turns.

## Offloading compute work with `ljos::task::spawn`

For CPU-bound work of your own (such as parsing a large file, indexing records or computing a layout), use `ljos::task::spawn`. It runs a work closure on the global bounded compute pool (2 to 4 threads) and delivers the result to a `done` callback on the `Engine` thread through the platform inbox:

- **Fault-isolated completion.** The `done` callback receives `Ok(value)` when the closure succeeds, or `Err(TaskPanic)` if the closure panicked on the worker pool. A worker panic is also reported to the `Engine`'s fault handler, and `done` still runs in its own turn so your application can clear any loading state it set before spawning.
- **Cancellation.** `ljos::task::spawn` returns a `Task` handle. Cancelling the `Task` before a worker picks it up prevents the closure from running at all; cancelling it while it is already running discards the result when it finishes.
- **Platform Replies on the UI thread.** Any platform `Reply` kept or dropped on the `Engine` thread itself never blocks; if the platform inbox is full, the reply is queued past the bound and delivered on the next frame.

## Fetching over the network from Rust and QML

Network requests are a host service provided by `Platform`:

- **Simple fetches in Rust.** Call `Platform::of(engine).fetch(url, done)` (`Platform`, `Fetcher` and `FetchError` are in `ljos::prelude`). The fetch runs on a background thread and calls `done` on the `Engine` thread. An application started through `ljos::App` or `ljos::run` uses the desktop shell's HTTPS fetcher, which follows redirects, enforces a 30-second timeout on the whole fetch, caps responses at 16 MiB, and reports failures as `FetchError::Failed` naming the HTTP status or transport cause.
- **Full HTTP requests in Rust.** For `POST` or other methods, custom headers or request bodies, use `Platform::request` with `Request::new(Method::Post, url).json(body)` (`Method`, `Request` and `Response` are in `ljos::prelude`). It returns `Ok(Response)` for any HTTP status the server sends—a 404 is a `Response` whose status and body you can inspect—and returns `Err` only for transport failures.
- **In headless tests.** A headless `Engine` has no network by default and fails every fetch with `FetchError::NoNetwork` unless the test installs a stand-in with `set_fetcher`.

From QML, scripts can also make asynchronous HTTP requests using the built-in `XMLHttpRequest` global, which shares the same native helper:

```qml
import QtQuick

Item {
    id: root
    objectName: "root"
    property string states: ""
    property string got: ""

    Component.onCompleted: {
        let http = new XMLHttpRequest()
        http.onreadystatechange = function () {
            root.states += http.readyState
            if (http.readyState === XMLHttpRequest.DONE)
                root.got = http.status + " " + http.statusText + " "
                    + JSON.parse(http.responseText.toString()).activity + " "
                    + http.getResponseHeader("Content-Type")
        }
        http.open("GET", "https://example.com/act.json", true)
        http.setRequestHeader("Content-type", "application/json")
        http.setRequestHeader("Connection", "close")
        http.send()
        root.states += "/"
    }
}
```

When an `XMLHttpRequest` reply has an XML content type, `http.responseXML` exposes a read-only W3C DOM document (`documentElement`, `childNodes`, `xmlVersion`, `xmlEncoding`, `xmlStandalone`), parsed out of the reply only when a script reads `responseXML`.

## Talking to the Engine with `UiHandle`

When your application runs long-lived background services (for example, on the `tokio` worker pool or a dedicated thread), use `App::run_with` (or `ljos::run(config, setup)`) instead of `App::run`. `App::run_with` performs the same start-up and then hands you the shell's `Setup`, which provides the `Engine`, the `UiHandle` that background threads clone and post through, and the message handler that runs on the `Engine` thread.

```rust
// <!-- check: verify exact Setup field or method names passed to App::run_with and UiHandle::send -->
use ljos::prelude::*;

fn main() {
    ljos::App::new("WorkerDemo")
        .size(640.0, 420.0)
        .source(ljos::include_qml!("qml.pack"))
        .root("Main")
        .run_with(|setup| {
            let _ui: ljos::UiHandle = setup.ui_handle();
        })
        .unwrap();
}
```

`UiHandle` is `Clone + Send + Sync` and offers four ways to post a message to the UI thread's bounded inbox:

- **`send(msg)`**: bounded and non-blocking. If the UI inbox is full, it returns `Full` immediately so the caller can decide how to handle back-pressure.
- **`send_coalesced(key, msg)`**: takes a `ljos::CoalesceKey` and keeps only the latest message for that key in the queue. If an earlier unread message with the same `CoalesceKey` is already queued, the new message replaces it in place; only the first message for a key requires a free slot. Use this for progress bars, live metrics or high-frequency updates where intermediate values do not matter.
- **`send_blocking(msg)`**: waits on a condition variable until the UI thread drains a slot, without spinning. Only the background sender waits; the UI thread never blocks.
- **`send_async(msg)`**: awaits free space asynchronously using a task waker when sending an ordered stream from an async task that must not drop messages.

When the first message enters an empty inbox, `UiHandle` sets an `AtomicBool` wake flag and calls `request_repaint`. On the next frame, the UI thread drains messages within the frame budget, running each `on_message` delivery in its own panic-isolated turn.

## Timers and naming slow native calls

When you want to delay or debounce work on the UI thread without spawning a background thread, use the `Engine`'s own clock (`Time` and `TimerId` are in `ljos::prelude`):

- `engine.now()` returns the current `Time` on the `Engine`'s clock (which is the wall clock in a running app and the virtual clock in tests).
- `engine.call_at(time, callback)` schedules a closure to run on the `Engine` at `time` and returns a `TimerId`.
- `engine.cancel_call(timer_id)` cancels a pending timer before it fires.

Because a native method called from QML runs on the UI thread, an unexpectedly slow method can still cause a dropped frame. A development host can call `Engine::set_slow_call_budget` to time every native call. The clock is read around native calls only when a budget is set; any call that takes longer than the budget is recorded in a 64-entry history and logged at most once a second per method (`slow native call: Type.method took N ms`), making freezes easy to find in `ljos observe`.

## See also

- [How a Rust and QML App Fits Together](./overview.md)
- [Native Types in Rust](./native-types.md)
- [Models for Views](./models.md)
- [Testing a Rust and QML App](./testing-rust.md)
