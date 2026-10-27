---
title: Observer
description: Inspect a running Ljós application's processes, mailboxes, objects, memory, models, wakes and supervision tree with ljos observe.
section: start
order: 8
---

# Observer

This page shows you how to attach `ljos observe` to a running Ljós application to inspect its live objects, processes, mailboxes, supervision tree, memory, models, frames and wakes.

## Starting the observer

First, start your application with `--mcp` (grant `mutate` if you want to edit properties or model cells from the observer, and `input` if you want to run interactive design audit checks):

```sh
ljos run --mcp=inspect,input,mutate Main.qml
```

In a second terminal, run `ljos observe` in the same project directory (or pass the path to `target/ljos/mcp-<pid>.json`):

```sh
ljos observe .
```

`ljos observe` finds the newest `target/ljos/mcp-*.json` file under the directory and opens a graphical window (1200×760 logical pixels by default, following your system's light or dark appearance). If the target application is not yet running or restarts, the observer reconnects automatically.

## Zero cost when idle

`ljos observe` does not poll the target application on a timer. It keeps a single `app_watch` request parked on the inspector's server thread and refreshes the open page only when the application actually produces a frame, logs a line, reloads or records an event. When your application is at rest, both the application and `ljos observe` sit at 0 % CPU and do no work.

## Pages in `ljos observe`

The observer window is organised into pages modelled on Erlang's `observer`:

### Overview

Shows the application's window size, scale, appearance, active painter (`gl` or `wgpu`), startup milestones (when the application was built, the graphics device was ready, the first frame was presented and the first input arrived), reload status and frame/paint latency summaries.

### Processes and Mailboxes

- **Processes** lists OS threads (with state, CPU time and wakeups where the platform measures them), running QML `Timer`s (with interval, repeat flag and time until next firing), running animations and scheduler owners.
- **Mailboxes** shows the bounded `UiInbox` queues that carry work to the UI thread: current depth, capacity, oldest message age, and totals for sent, refused, coalesced (`replaced`) and drained messages, plus the front message kinds (`peek`). Because mailboxes are read on the inspector's server thread, you can still inspect them if the UI thread is blocked.

### Objects

Displays the live QML object tree (`app_objects`), including non-visual objects such as `Timer`s, models and animations, with each object's `@handle`, type, `id`, `objectName` and approximate size in bytes:

- You can filter objects by name or by size (such as `>4k` or `>1m`), or sort the list **largest first** to see which objects hold the most memory.
- Selecting an object shows a rendered preview of that item, its source file and line, and all of its properties with their provenance (`bound`, `assigned`, `animated`, `host` or `edited`).
- When the target was started with `mutate`, you can edit property values in place (`keep_binding` preserves the underlying binding so you can revert your edit).
- Ticking **Bindings** shows every binding on the selected object with its tier (`0`, `1` or `JS`), the properties it reads and the properties that read it; clicking any linked object jumps to it in the tree.

### Logs and Trace

- **Logs** displays `console.log`, warnings and JavaScript errors with their QML file and line number.
- **Trace** lets you pick one or more objects from the Objects page and record a live timeline (`app_trace`) of every signal they emit, every property change with its new value, when the object is destroyed, and (optionally) every binding evaluation with its cause, duration and whether the evaluation produced the same value it already had.

### Supervision

Shows the Erlang-style fault supervision tree (`app_processes` with `kind: "supervisor"` and `kind: "fault"`): the built-in supervisors (`app`, `ui`, `services` and so on) and every `ErrorBoundary` in your QML tree, along with the restarts each boundary allows within its time window, how many restarts it has used and whether it has escalated. Below the tree, recent faults are listed newest first with their stack trace and links to the faulting object and the boundary that caught it.

### Frames and Wakes

- **Frames** records up to 600 rendered passes (`app_frames`), showing what woke each frame (input, timer, animation or thread message) and where the frame time went across timers, animations, layout, scene building, bindings, text layout and GPU upload/draw/present, with p50, p95 and maximum summaries. You can also export the recording to a Chrome/Perfetto trace file in `~/.local/state/<app>/traces/`.
- **Wakes** makes the 0 % CPU rule visible. Its **Why awake** panel (`app_processes` with `kind: "waker"`) lists everything currently scheduled to wake the UI thread: running `Timer`s, running animations, pending platform requests (file reads, network fetches, dialogs), images decoding and queued inbox messages. While recording wakes, it also shows a 10-second timeline coloured by wake cause and lets you inspect the exact sequence of handlers, property changes, binding runs and redrawn items caused by the last user input (`last_input`).

### Memory

Shows an allocator-style breakdown of the application's memory (`app_metrics` with `memory`):

- A stacked history chart of resident memory by owner (`JS heap`, `objects and items`, `reactive graph`, `scene nodes`, `text layouts`, `glyph atlas`, `meshes`, `image cache`, `model storage`, `compiled units`, `media`, shared libraries, free allocator heap and unattributed memory).
- Proportional bars for each engine subsystem and a table of object counts and bytes grouped by QML type, largest first.

### Models

Works like Erlang's Table Viewer (`app_objects` with `models` and `model`). Lists every model in the application—`ListModel`, numeric or array models on `Repeater` and views, `ObjectModel`, `DelegateModel`, `XmlListModel`, `FolderListModel` and custom Rust models—along with its row count, role names, where it is declared and which views display it. Selecting a model shows its rows 50 at a time and follows live updates; for a writable `ListModel` (when connected with `mutate`), you can edit cells in place.

### Components, Bindings and Growth

- **Components** lists every loaded file component, inline `component`, delegate and `Component` template with its live instance count and total bytes (`app_objects` with `components`), including unused components with `0` instances.
- **Bindings** profiles bindings and signal handlers while recording (`app_frames` with `bindings`), sorting them by total time, own time, run count or longest run, and lists any binding loops the reactive graph broke at start-up.
- **Growth** samples each component's live instances and bytes across frames (`app_frames` with `growth`) and highlights in red any component whose instance count rose at least twice and never fell—helping you spot objects left behind when a dialog closes or a page pops, alongside `mark` and `since_mark` on the Objects page.

### Audit

Runs the live design and accessibility audit (`app_audit`) on demand when you press **Run audit**. Findings are grouped by rule code (such as `color-text-contrast` or `layout-offscreen`) with links to the item and the fix. Interactive checks that move focus, press `Escape` or toggle controls are kept behind an explicit switch so opening the Audit page never disturbs your application's state unasked.

## See also

- [MCP servers for agents](./mcp.md)
- [Command-line reference](./cli.md)
- [Hot reload](./hot-reload.md)
- [Testing](./testing.md)
