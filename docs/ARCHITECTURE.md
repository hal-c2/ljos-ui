# Ljós UI — Architecture

Status: draft 2, 2026-10-10. This document is a synthesis of the research reports in `docs/research/` (01–10). It records the decisions, the reasons for them and the plan. Every section names the report that holds the evidence, the rejected alternatives and the measurements. When this document and a report disagree, this document wins. Section 22 lists the conflicts it resolved.

Ljós ("light" in Icelandic) is a Rust UI toolkit that runs QML. It aims to be compatible with Qt Quick and Qt Quick Controls. It renders through egui, runs JavaScript with rquickjs, and targets desktop and mobile.

Ljós is a **general-purpose framework**. Its first consumer is HAL-C2 (`~/dev/hal-c2`), which serves as one validation target among several. HAL-C2 supplies evidence (usage counts, test suites) and acceptance scenarios. It does not shape Ljós's names, crates or APIs (principle 9).

---

## 1. Goals and non-goals

**Goals**

1. **Run real QML.** This means modern Qt 6 QML: typed and `required` properties, inline components, `pragma ComponentBehavior: Bound`, attached and grouped properties, `Binding`/`State`, and JS modules. It also covers Qt Quick Controls with the semantics and API that applications rely on. The yardsticks are the qtdoc example corpus (08) and real applications, the first of which is HAL-C2.
2. **Hot code update.** In development a saved file patches the running app and keeps its state. In production a signed bundle can update QML, JS and assets without a store release, within store policy.
3. **Desktop and mobile from one engine.** Linux, macOS and Windows come first, then Android, then iOS.
4. **Off-screen content costs close to nothing.** Anything not painted does no layout, paint or GPU work. Anything not visible does no binding work unless something observes it. An idle app uses 0 % CPU.
5. **Accessible and localisable from the start.** Every item can produce an AccessKit node. Text is shaped with bidi. `qsTr` works against real catalogues, with a language switch at runtime.
6. **Fault isolation and fair scheduling,** with ideas taken from Erlang/BEAM (section 3.4). A runaway handler, binding or plugin cannot take over the frame or the process.
7. **Every behaviour is specified in Gherkin** and checked against real Qt wherever that is possible (section 18).
8. **MCP out of the box.** Coding agents can check, look up, lint and test QML without a running app, and can inspect and drive a running dev build (section 19).
9. **Malleable software.** People tailor the applications they use, alone or with a coding agent, instead of accepting what they are given or reverse-engineering it. An application can let its user change its QML and scripts on their own device and see the change live, checked by the same compiler and lints the dev tools use. This is why a shipped application may carry the compiler (section 2, crate rules). Where a platform forbids code generated at run time (iOS has no JIT), the changed code still runs, as IR the engine interprets.

**Non-goals (for now)**

- Qt3D, WebEngine, QtLocation, QtGraphs and Qt.labs modules. These appear in the qtdoc corpus (08) but not in HAL-C2. In the API ledger they are `dropped` with a reason. QtQuick3D was on this list; it is being built, drawn with wgpu natively and on the web (section 7.8). QtQuick.Particles was on this list; the part the corpus uses first (systems, emitters, image particles, a seeded simulation) is built (`features/items/particles.feature`), and affectors, trails and sprite particles are not yet. ShaderEffect with custom shaders was on this list too; it is built from the shaders' GLSL source, drawn by the GL painter only (section 7.1, "As built: shader effects").
- Source compatibility with C++ `QObject` code. Native types are written in Rust (section 5.5).
- A JS debugger in v1. quickjs-ng has no debugger API (03 §4.2). Logging, stack traces with QML file and line, and the dev overlays come first.
- Non-QtQuick QML dialects, such as HAL-C2's TUI dialect (`import OpenTUI`) (D6).

---

## 2. Principles

1. **Clean room.** No source code is copied from, or read in, Qt or Slint. Allowed inputs are the Qt documentation (doc.qt.io), published specifications, and running Qt binaries as a black box: `qml6`, `lupdate` and `lrelease` as tools, and screenshots as references. Data generated from a local Qt installation (`types.txt`, reference PNGs) is generated locally and git-ignored (D1). Qt Quick Controls styles are authored from scratch. Every worker prompt and the contributor guide carry this rule.
2. **Qt semantics by default; efficiency by structure or opt-in.** Where Qt defines an observable behaviour, Ljós matches it. Examples: `visible: false` keeps bindings and timers running, and `reuseItems` defaults to false. Ljós saves work only in three ways:
   - it is unobservable, such as lazy render bindings or paint culling;
   - it is structural, with static analysis proving it safe, such as auto-freezing covered `StackView` pages that have no observable handlers;
   - the application opts in, such as `Timer.suspendWhen` or `Frozen`.

   Each deliberate divergence has a `@qt-diverges` scenario with a stated reason.
3. **Ljós owns the retained tree; egui is the host.** QML items are never egui widgets. Ljós owns the object tree, layout, the render tree, input routing, focus, text and the accessibility tree. egui provides the window, raw input, painting and texture upload (02 §3.1). The egui API Ljós touches is kept small and behind `ljos_egui`, because egui breaks its API about every two months.
4. **One owner per piece of state; messages between owners.** An Engine is single-threaded and `!Send`. Other threads (tokio, the compute pool, plugin runtimes) talk to it only through bounded messages. No locks are taken on the binding path.
5. **Work happens on demand.** Frames are scheduled only by dirty state, animations, timers or input. Bindings that feed only rendering are pulled, not pushed. Delegates, layouts, glyph runs and textures exist only near the viewport.
6. **Measure before building the expensive version.** Examples are the render thread, the B1 wgpu backend and tier-1 coverage. Each has a stated trigger, such as "tessellation over 2 ms on a mid-range Android device".
7. **Testable through the accessibility tree.** Tests find interactive elements by role and accessible name. An item that a test cannot find is also invisible to a screen reader, so a failing test points at a real accessibility bug.
8. **Determinism in tests.** One injectable `Clock` drives every timer and animation. Test runs have no sleeps and no wall-clock time.
9. **General purpose; consumers stay outside.** No crate, type, API, tool or milestone in Ljós is named after or written for one application. A need that comes from a consumer becomes a generic mechanism, such as window-scoped singletons, external acceptance suites or the MCP inspector. The consumer's own glue (shell doubles, fake servers, adapters) lives in the consumer's repository. Usage counts from HAL-C2 are evidence for priorities, weighed together with the qtdoc corpus.
10. **Agents are first-class users.** Authoring tools and running apps are reachable over MCP out of the box (section 19), with the same role-and-name addressing that tests and screen readers use.

---

## 3. System overview

### 3.1 Layers

```
 ┌──────────────────────────────────────────── tools ─────────────────────────────────────────────┐
 │ ljos CLI: run · check · dump · i18n · bundle · android     qml_compare · qmltypes · features:*  │
 └────────────────────────────────────────────────────────────────────────────────────────────────┘
 ┌──── front end (no runtime deps; dev/build time) ────┐      ┌──── dev services ────────────────┐
 │ qml_syntax (oxc fork) → qml_front (discover, scope, │      │ watcher → incremental compile →  │
 │ names, types) → ljos_compiler (lower, resolve,      │─────▶│ IR diff → ReloadPatch → devtools │
 │ tier-1 codegen, JS codegen, i18n extract, diff)     │      │ WebSocket → device               │
 └──────────────────────────┬──────────────────────────┘      └────────────────┬─────────────────┘
                            │ ljos_ir (owned, serialisable: ComponentDef, bindings, keys)
 ┌──────────────────────────▼───────────────────── Engine (UI thread, !Send) ────▼─────────────────┐
 │ object store (slotmap) · MetaObject registry · ljos_reactive graph · binding tiers 0/1/2       │
 │ ljos_js (rquickjs Runtime+Context) · incubator · scheduler (lanes, ProcId) · supervisor        │
 │ ljos_quick (items, anchors, positioners, Layouts, views, handlers, animations)                 │
 │ ljos_controls (Controls API + Basic style) · ljos_i18n · ljos_input (focus, keys, gestures)    │
 │ ljos_text (Parley, atlas, RichDoc) · ljos_a11y (a11y pass) · ljos_render (RenderTree, backends) │
 └──────────────▲──────────────────────────────────────────────────────────────┬──────────────────┘
                │ UiMsg (bounded, coalescing)                                   │ shapes, callbacks,
 ┌──────────────┴───────────────┐  ┌───────────────────────────┐              │ TreeUpdate, IME
 │ tokio (2–4 workers): network,│  │ compute pool (2–4): image │   ┌──────────▼──────────────────┐
 │ SQLite, file watch, devtools │  │ decode, parse, shaping    │   │ ljos_egui / ljos_shell      │
 └──────────────────────────────┘  └───────────────────────────┘   │ egui + glow/wgpu + AccessKit│
 ┌──────────────────────────────┐                                  │ ljos_host: PlatformHost     │
 │ pinned threads: plugins,     │                                  │ (winit / android-activity)  │
 │ WorkerScript (own runtimes)  │                                  └─────────────────────────────┘
 └──────────────────────────────┘
```

### 3.2 Threads

Source: 03 §3.1.

| Thread | Owns | Notes |
|---|---|---|
| UI (main) | Engine: objects, graph, JS runtime, scheduler, render tree, egui | One per app. Not `Send`. |
| tokio multi-thread (2 workers on phone, up to 4 on desktop) | Network services, SQLite, file watcher, devtools socket | Results arrive as `UiMsg`. Lives in the *app core* and survives the Android Activity being destroyed. |
| Compute pool (2–4, bounded) | Image decode at `sourceSize`, QML parse/compile, bytecode compile, off-thread text shaping | Jobs return results via `UiMsg`. |
| Pinned threads (one per isolated runtime) | WorkerScript, third-party plugin runtimes | Each has its own rquickjs `Runtime` with a memory limit, stack limit and interrupt deadline, plus a tokio `LocalRuntime`. |
| Render thread | Not in v1 | `FrameSnapshot` is the seam (section 7.2). Add it only if profiling demands it. |

There is **one JS Runtime and Context per Engine**, and normally one Engine per app. Measured costs: a runtime is 28 KB and a context about 54 KB. A Rust→JS call takes about 53 ns. Loading bytecode is 14× faster than compiling. The interrupt deadline costs under 0.5 %. The context survives both an interrupt and out-of-memory (03 §2.2).

Rejected:
- a runtime per window, which breaks shared singletons and models;
- a context per component;
- JS on a worker thread, because bindings must be synchronous;
- rquickjs's experimental `parallel` feature.

### 3.3 Messages

- **One `UiMsg` enum** with variants `Service`, `Model`, `Resolve`, `Plugin`, `Supervision`, `Reload` and `Lifecycle`.
- **Sending.** Senders hold a `UiHandle`.
  - `send` is bounded and fails fast with `Full`.
  - `send_coalesced(key, msg)` keeps only the latest value per key. The first message of a key still needs a free slot.
  - `send_blocking` and `send_async` wait for room instead, for an ordered stream that must not lose messages. A drain signals them as it frees slots (a condvar, a task waker), so nothing spins; only senders wait, never the UI thread.
  - A platform `Reply` kept or dropped on the Engine thread itself never waits: with the inbox full it is queued past the bound, behind that thread's earlier answers, and delivered at the next frame. Only the Engine thread adds there, once per answer it promised, so the overflow grows with the engine's own work, while other threads still wait for room.
  - The first message sets an `AtomicBool` wake flag and calls `request_repaint`.
- **Draining.** The UI drains messages under the frame budget. Overflow stays queued, which pushes back on senders. Drain time appears in `FrameReport`.
- **egui_mobius.** Its idea is adopted: typed slots and signals across threads. Its implementation is not, for four reasons:
  - unbounded `mpsc` channels;
  - a thread per slot;
  - busy polling;
  - callbacks run under a lock.

  Its eager `Derived` values are replaced by `ljos_reactive` (03 §2.3).

### 3.4 What we take from Erlang/BEAM

| BEAM idea | Ljós mechanism |
|---|---|
| Process | `ProcId`, one per logical owner: a component boundary (Loader, StackView page, Window, `ErrorBoundary`), a plugin or a service |
| Reductions, preemption | `SliceBudget` per job. The JS interrupt handler checks the deadline. The incubator and jobs yield (`JobPoll::Yield`). The scheduler round-robins Deferred and Prefetch work across owners. |
| Bounded mailbox | `UiInbox` with back-pressure and coalescing |
| Links and supervisors | An in-house `Supervisor` tree with OTP strategies and restart intensity (section 9.4) |
| Let it crash | Restart a boundary from its `ComponentDef`. Keep the last good tree when a reload fails. |
| Two code versions, `code_change` | Reload generations, where the old generation stays live until the new one applies. `Migrate` plays the role of `code_change`. |
| Per-process heaps | Separate runtimes with memory limits for plugins and WorkerScript. The main runtime collects garbage in the Idle lane. |
| `observer`/`top` | A dev overlay showing time per `ProcId` and lane ("which component ate the frame"), and `ljos observe`, a window over the inspector tools (below) |

**The observer.** Erlang's observer shows a node's applications as supervision trees, its processes, their mailboxes, a live table viewer and load charts. `ljos observe [target]` is the same for a running Ljós application. It is itself a QML + Quick Controls application, and it reaches its target only through the inspector's MCP tools (section 19.4), so an agent can see everything the window shows. The mapping:

| Observer | Ljós | Where it comes from |
|---|---|---|
| Process | An OS thread: the UI (Engine) thread, the compute pool (`ljos-compute`), tokio workers, the inspector's server (`ljos-mcp`), and the platform's readers (media, images, fonts, settings, screen reader, network, dialogs) | `/proc/self/task`: the kernel already keeps a thread's name, its state (running is *busy*, sleeping is *idle*, uninterruptible is *blocked*), its CPU time and how often it woke (voluntary context switches). Nothing is counted for it. On macOS `proc_pidinfo` tells the same but for the wakes, and on Windows the thread's times and name but neither its state nor its wakes (§19.5). |
| Process on the UI thread | A running `Timer`, an animation under way, and the `ProcId` owners the frame scheduler charges time to (application, boundaries, inspector) | The engine's timer table, the animation driver and `FrameStats` |
| Reductions | CPU time per thread; time per owner and lane | as above |
| Application (supervision tree) | The standard supervisors (`app` over `ui`, `services` and `plugins`) and every live boundary (`ErrorBoundary`, `Loader`), under the supervisor or boundary around it in document order, each with its strategy, the restarts it allows within how long, the ones it used, whether it is down and whether its content runs. A boundary is listed from when it is made; its supervisor is still made only at its first fault, so until then, and after a boundary with none around it gives up, the entry is read from the boundary's configuration when asked | `app_processes` with `kind: "supervisor"`, from the engine's `SupervisorTree` |
| Crash report | The last 32 faults of every kind (a JS exception, a script interrupted or out of memory, a panic, a component that could not start, an internal error), each with its file and line, its stack, the object it happened in by the name it had then (it may have gone with the content its boundary restarted), and what took it: a boundary, the application, the log only (an exception no boundary asked for), or nobody because the content was being discarded | `app_processes` with `kind: "fault"`, in `faults`; every fault is logged, so the watch's `logs` topic says when |
| Mailbox | Every bounded `UiInbox` named with `observe_as`: `application` (the app's own messages), `platform` (answers from platform threads), `images` (decodes), `inspector` (tool calls, counted by tool) and `reload` (the file watcher) | `ljos_runtime::observe` |
| Message queue length | Depth and capacity, the front message's age, messages sent, refused as busy, replaced by a coalesced send and drained, senders alive and senders waiting for room, and the queued messages by kind | The counters move only while an inspector is served, at one relaxed load per send. A peek names kinds, never payloads. |
| Table viewer | Every object, items or not (Timers, models, animations), each named by a `@handle` | `app_objects`; each object's properties and their provenance from `app_inspect`, an editor and a revert through `app_set_property`, a picture of the selection from `app_screenshot` |
| Table Viewer (tables) | The application's models: a ListModel, the number or array a view or Repeater was given, an ObjectModel, a DelegateModel, a list model written in Rust; each with what has it, its rows, roles and views | `app_objects` `models` lists them and `model` reads a page of rows (`offset`, `limit`); a `holder` follows one, and the watch's `models` count says when it changed; a cell is written through `app_set_property` `row` |
| Load charts | Frame time, passes drawn and skipped, resident memory, wakeups of all threads and the deepest inbox, one point each time the application changed | `app_watch` says when, `app_metrics` and `app_processes` say what |
| Trace | What an object is told, from when it is traced: each signal it emits with its arguments, each property change with the new value, and its end, in order and with the application's time, as a process's messages are traced. Asked to, also each run of its bindings: what it read that changed, how long it took, and whether it gave the value it had. A trace keeps to the properties and signals named, and each client reads only what the objects it traces were told. At most 16 objects at once | `app_trace`; the watch's `traces` topic says when |
| Frames | Each frame of every window while recorded: which window, what woke it and where its time went, as bars over a 60 Hz frame at least, with a summary; the frames only the observer's own requests woke hidden unless asked for | `app_frames`, held by the observer from Record until Stop or until it closes; the watch's `paints` topic says when |
| Wakes | Why the UI thread woke, each time: input, a window event, a timer, an animation, a message by kind (inspector, other threads, posted, accessibility), a platform redraw, dirty state, a deadline; and what it led to (passes, a frame drawn or unchanged, a redraw asked, nothing), as a strip of the last seconds and a list; beside them, why it is awake: everything that will wake it next (running Timers with interval and next firing, running animations, answers under way such as a file read, a fetch or a dialog, pictures loading, messages posted), each with its owner at file:line, and apart what only listens (a file watcher) | `app_frames`' `wakes`, held as the frames are; the watch's `wakes` topic says when, and the wakes only the observer's requests caused move nothing; `app_processes` with `kind: "waker"` |
| Profiler (hot bindings) | Which bindings and handlers ran most and took longest while recorded: each with its file:line, the property or signal it is for, its runs, total, own and longest time, and the dependencies whose changes set its runs off; the binding loops the graph broke, each with its way round, whether recorded or not | `app_frames`' `bindings` (`sort`: total, own, runs, longest), held as the frames are; the watch's `frames` topic says when |
| Growth | Each component's live instances and bytes, sampled over time while recorded, the ones that keep growing first | `app_frames`' `growth`, held as the frames are; the watch's `frames` topic says when |
| Audit | The design audit of what the application shows, when Run audit is pressed and never otherwise: findings grouped by rule, worst first, each with its severity, message, measured value and source `file:line`; the selected one with its fix, a button that opens its guide, and a box drawn at its bounds on a picture of the window. The settings (current or the full matrix) and the area come from the page's controls, the areas from the rules' table. The checks that act are behind their own switch, off until turned on, which says that they press keys and toggle controls in the application and run its handlers; off, the page says how many were not run | `app_audit`, with `app_screenshot` and `app_status` for the picture |
| Trace file | The frames, wakes and runs of bindings recorded, as a Chrome trace-event file a standard trace viewer opens | `app_frames`' `export`, written by the application in `traces` of its per-user state directory |
| Modules | What is loaded: each component of each file (the file itself, its inline components, its delegates and other `Component` contents) with its URL, how many instances live and the objects and bytes they hold; one picked lists its instances, each leading to the Objects page | `app_objects`' `components` and `component` |
| Memory allocators | Where the memory is: the objects by type, the script heap, the reactive graph, the text caches, images, and per window its scene and what its renderer keeps (meshes, glyph atlas, textures), beside the process's resident bytes; the same by owner, stacked over time and reconciled with what is resident, the rest told as not attributed; on the Objects page, each object's bytes, largest first or above a size | `app_metrics`' `memory` (`owners`, `reconciled`) and `app_objects`' `bytes`; asked when the page is shown and when a pass is drawn |
| Log | The log, filtered by level and text | `app_logs` |

WorkerScript has no runtime yet (section 3.2 plans one pinned thread per isolated runtime); when it does, its thread and its inbox are listed the same way.

---

## 4. Workspace

Source: 06 §3.2, extended.

Conventions are copied from qml-solid:
- Cargo workspace with resolver 3 and edition 2024.
- Rust 1.98.1 pinned in `mise.toml`.
- `debug = "line-tables-only"` for dev builds.
- `mise-tasks/` holds bash scripts with `#MISE` headers and `set -euo pipefail`.

```
ljos-ui/
├── Cargo.toml  mise.toml  rust-toolchain via mise
├── vendor/oxc_parser/          oxc_parser 0.144.0 fork with QML mode            [shared]
│                               (package ljos_oxc_parser, by path: no [patch])
├── crates/
│   ├── qml_syntax/             re-exports + parse helpers + member-level recovery [shared]
│   ├── qml_front/              discover, project (TypeOracle), types, scope, names [shared]
│   ├── qt_types/               Qt type-table reader (table generated locally)      [shared]
│   ├── qt_signals/             the table's modules, properties, signals only (web)  [shared]
│   ├── qmltypes/               generator CLI (.qmltypes + qmldir only)            [shared, tool]
│   ├── qml_compare/            pixelmatch port, content score, ratchet, report    [shared, tool]
│   ├── qml_fmt/                QML formatter behind `ljos fmt` and `qml_format`    [tool]
│   ├── qml_edit/               structured edits, rebase, keys and tree diff        [tool]
│   ├── ljos_ir/                owned IR: ComponentDef, ObjectKey, bindings, signatures
│   ├── ljos_compiler/          lowering, tier-0 resolution, tier-1 codegen, JS codegen,
│   │                           IR diff → ReloadPatch, i18n extraction, lints (salsa)
│   ├── ljos_reactive/          dependency graph (push-pull, height-ordered effects)
│   ├── ljos_engine/            object store, Value, MetaObject, registry, tier-1 VM,
│   │                           incubator, PropertyMap, list-model trait, Engine
│   ├── ljos_macros/            #[derive(QmlObject)], qml_impl, QmlAttached, QmlValueType,
│   │                           QmlEnum, qml_singleton, module!
│   ├── ljos_js/                JsHost, prototypes/wrappers, Qt.* builtins, String.arg
│   ├── ljos_runtime/           Scheduler, lanes, ProcId, FrameScheduler, Clock, UiMsg,
│   │                           Supervisor, HotReloadHost, BundleStore
│   ├── ljos_quick/             QtQuick items, anchors, positioners, QtQuick.Layouts,
│   │                           Flickable/ListView/GridView, handlers, animations, states
│   ├── ljos_controls/          QtQuick.Controls types + clean-room Basic style
│   ├── ljos_text/              TextEngine (Parley + Fontique), glyph atlas, elide,
│   │                           RichDoc + front ends, TextModel (editing, undo)
│   ├── ljos_input/             InputRouter, FocusTree, key dispatch, Shortcut,
│   │                           gesture arena, ScrollPhysics, TextInputClient
│   ├── ljos_a11y/              a11y pass, role table, live regions, SystemPrefs model
│   ├── ljos_design/            design rules (codes, limits), colour maths, motion and palette tokens; no deps
│   ├── ljos_i18n/              Catalog, I18n, .ts import/export, .ljl, ICU4X glue
│   ├── ljos_render/            RenderTree, DisplayList, B0 epaint, R vello_cpu, B1 wgpu
│   ├── ljos_shader/            ShaderEffect programs: GLSL 440 → GLSL ES 300 / GL 330 with
│   │                           naga [feature compile], std140 reflection, `.ljsh` artifacts
│   ├── ljos_egui/              QmlView widget, egui input/output conversion, EguiSink
│   ├── ljos_host/              PlatformHost, WinitHost, AndroidHost, (IosHost),
│   │                           PlatformTextInput impls, clipboard, prefs backends
│   ├── ljos_platform/          system services behind traits (clipboard, dialogs,
│   │                           settings, fetch, permissions, opening URLs, sensors),
│   │                           QtCore, QtQuick.Dialogs, QtSensors
│   ├── ljos_media/             QtMultimedia playback: players, outputs, SoundEffect,
│   │                           media clock, null sink [features decode, audio] (7.6)
│   ├── ljos_lottie/            Qt.labs.lottieqt: LottieAnimation, own Lottie model (7.7)
│   ├── ljos_jsonmodel/         Ljos.Models: JsonListModel, rows from a JSON document (M2)
│   ├── ljos_3d/                the scene a View3D shows and its wgpu renderer [feature gpu];
│   │                           no egui, no engine (7.8)
│   ├── ljos_quick3d/           QtQuick3D types writing an ljos_3d scene (7.8)
│   ├── ljos_shell/             app runner: eframe (M0) → own egui-winit loop, GL or wgpu (M2)
│   ├── ljos_dev/               watcher, devtools WebSocket, error and `top` overlays,
│   │                           MCP inspector [feature "mcp"]
│   ├── ljos_mcp_core/          McpSurface, ToolRegistry, caps/tokens, snapshot renderer (rmcp)
│   ├── ljos_mcp_author/        `ljos mcp` authoring tools (CLI only)
│   ├── ljos_drive/             scene driver: settle, advance a test clock, find, input,
│   │                           a11y and pictures by one set of rules (no egui, no parser)
│   ├── ljos_test/              kittest harness wrapper, a11y audit, QtTest TestCase runner
│   ├── ljos_bdd/               cucumber World, Backend trait, Ljós + Qt backends, steps
│   ├── ljos_ratchet/           acceptance suites' PIN, globs and RATCHET, judged without a backend
│   ├── ljos/                   facade: re-exports, prelude, module! registration
│   └── ljos_cli/               `ljos` binary
├── .mcp.json                   `ljos mcp` for agents working in this repo (section 19)
├── features/                   Ljós's own Gherkin ledger (section 18.2)
├── acceptance/<suite>/         external acceptance suites: allowlist + PIN + ratchet (section 18.4)
├── ledger/qtquick.toml         API ledger: type × member → status (section 18.5)
├── corpus/                     qtdoc submodule at e05e4f4, examples.json, stand-ins
├── third_party/filmcraft/      FilmCraft submodule at 8309f21 (demuxers, decoders; 7.6)
├── android/                    Gradle + GameActivity template (M3)
├── mise-tasks/
└── docs/  ARCHITECTURE.md  research/  CLEAN_ROOM.md (to write)
```

**Crate rules**

- **The parser is the application's choice.** Runtime crates depend on `ljos_ir`, never on `qml_syntax` or `qml_front`: the engine runs IR, whoever compiled it. An application that does not let its user change it loads compiled IR and links no parser and none of its roughly 106 dependency crates (06 §4.6). One that does (goal 9) links `ljos_compiler` through an opt-in feature, and compiles its user's changes to IR on the device. Today that feature is for dev builds and plugins only (`ljos`'s `dev-qml`, `ljos_shell`'s `dev`); a feature for shipped applications that their users can change is still to come.
- **MCP never ships by accident.** `ljos_mcp_author` is linked only by `ljos_cli`. The inspector is a feature of `ljos_dev` that release builds refuse (section 19.7).
- **egui stays in three crates.** Only `ljos_egui`, `ljos_shell` and `ljos_render`'s B0 backend name egui types. `ljos_test` and `ljos_bdd` use egui_kittest, but scenarios never see it.
- **wgpu stays in three crates too.** `ljos_3d` (its `gpu` feature), `ljos_egui` (its `quick3d` feature) and `ljos_shell` name wgpu types. The QtQuick3D types write a scene and never see a device (section 7.8).
- **Start small and split later.** M0 creates the crates on its own critical path. The rest begin as modules and get their own crate when the boundary has proven itself. The tree above is the target.

**Sharing with qml-solid.** ljos-ui is the home repo for `qml_syntax`, `qml_front`, `qt_types`, `qmltypes` and `qml_compare`. qml-solid depends on them by git `rev` and keeps only its Solid/web lowering. There is never a second copy of the oxc fork. The fork is rebased every 4–8 oxc releases, and `mise run oxc:check` shows drift (06 §3.4). This is decision D7.

**As built: the table of types (decision D1).** The front end reads QML against one table, `qt_types`, which has two descriptions in one format. Qt's of its own types (`crates/qt_types/src/types.txt`) is generated from a local Qt by `mise run types` and git-ignored. Ljós's of the types it implements (`crates/qt_types/src/ljos.txt`) is committed: a test writes it from the registry of a running engine and from the compiled Basic style (`LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types`), and fails with that command when the file is stale. Nothing in it comes from Qt. It is the type dump of section 5.5 as built, for Ljós's own crates; an application's native types reach the compiler through its own dump file (section 5.5), not through this table. Where Qt's table exists it is the whole truth of every module it describes, and Ljós's gives only the modules Qt's lacks (`Ljos`, `QtTest`): the choice is by module and never by type, so that no inheritance chain is half one description and half the other, and a build with Qt's table reads a file as it always did. Without it (`LJOS_QT_TYPES=none`, or a checkout where `mise run types` never ran) Ljós's table is everything and the workspace builds and tests with no Qt installed; a name no type has is then reported with a note that a table of Qt's types could tell whether Qt has it. Tests that compare the two tables hold Ljós's types to Qt's where both exist and say `SKIPPED` with the reason where not. CI's `check` job builds without Qt; a nightly job builds with Qt's table.

---

## 5. Front end: from `.qml` to IR

Sources: 06; 01 §3.4, §3.7.

### 5.1 Parsing

- **Parser.** qml-solid's oxc fork is reused unchanged at first. It parses all 495 qtdoc files, and embedded JS comes out as a real oxc AST.
- **First fork improvement: member-level error recovery.** The parser should return a partial tree plus diagnostics, so that one typo does not fail the whole file. Hot reload, the LSP and `ljos check` all need this. Annotation support (`@Foo`) and structured diagnostics follow.
- **Baselines** for the 495/495 parse and 464/495 compile figures must be re-measured on a fresh build before anyone quotes them. The earlier numbers came from a stale binary. Use a `CARGO_TARGET_DIR` outside `/tmp`, which has a disk quota.

### 5.2 Semantics (`qml_front`)

- **What it contains:** module discovery (`qmldir`, CMake `qt_add_qml_module`), the project graph, type-name lookup, the id and context scope chain, and name lookup.
- **Type knowledge** comes through a `TypeOracle` trait. Ljós implements it from its own description of the types it implements (`crates/qt_types/src/ljos.txt`, written from the registry by a test) or a generated Qt table, and for an application's native types from the `MetaObject`s registered on an Engine (`EngineOracle`, section 5.5).
- **Lookup order** follows Qt: JS locals → ids in the component → scope object → root object → ancestor contexts → globals. `pragma ComponentBehavior: Bound` restricts outer-context access (01 §2.1).
- **Gaps to fill:** `depends`, `import`, `optional import`, `prefer`, `plugin`, and `typeinfo` in `qmldir`.

### 5.3 IR (`ljos_ir`)

An owned, arena-free, serialisable IR. The oxc arena is dropped after lowering.

- **`ComponentDef`.** One per file and one per inline component. It holds an object tree of `ObjectDef`s, where each `ObjectDef` carries its type, `ObjectKey` segment, property declarations, assignments, bindings, handlers, attached and grouped blocks, and children. The `ComponentDef` also records its `ComponentSignature`, a hash of property names and types, signals and inline components.
- **Bindings** are `ExprId`s that point either to tier-1 bytecode (`Arc<[Op]>` plus a constant pool) or to a JS function in the file's JS module. They are shared by every instance of the component.
- **Serialisation** uses `postcard`. The IR is what production bundles contain, alongside JS source and optional bytecode.

### 5.4 Binding tiers

Source: 01 §3.4.

| Tier | Where | Covers |
|---|---|---|
| 0 | compiler | Static resolution of every identifier to `Resolved::{Local, Id, ScopeProp, RootProp, OuterId, Attached, Enum, Singleton, ImportNs, Global, ContextProp, Unknown}`. There is no `with`-style lookup at run time. |
| 1 | `ljos_engine` VM | A register bytecode over the dynamic `Value`. It covers literals, resolved reads and writes, value-type fields, arithmetic, comparison and logic with exact JS coercion, template strings, and pure builtins (`Math.*`, `qsTr`, `Qt.rgba/point/size/rect`, `String#arg`). User functions whose bodies are in the subset are inlined. Opcodes are specialised when the types are known. No type annotations are needed, because real QML is dynamically typed in practice (31 % of HAL-C2's properties are `var`). |
| 2 | rquickjs | Everything else: closures, loops, arrays, `JSON`, regex, JS libraries, and handlers. Each QML file compiles to one JS module whose functions take resolved references as parameters. |

- **Same dependency tracking in every tier.** Each tier reads through `Graph::read`, so dependencies are captured the same way.
- **Debug cross-check.** In debug builds, tier-1 bindings are also evaluated in tier 2 and the results compared, and what the two read: a read only one tier keeps is a dependency the other misses.
- **Coverage reporting.** `ljos check --stats` reports tier-1 coverage over the qtdoc corpus and any application corpus given to it. Optimisation work is driven by that number.
- **As built.** The generator works from the expression's tier-2 function, so both tiers start from the same resolved names. A program is compiled whole or not at all, has no effect before its end, and makes at most one write; when the VM cannot be exact it gives up and tier 2 runs the expression as if tier 1 had not been tried. Opcodes are not specialised by type yet. `LJOS_TIER1=0` turns the tier off and `LJOS_TIER1_CHECK=1` turns the cross-check on in a release build. Measured coverage and timings are in `docs/tier1-coverage.md`.
- **Shipped files compiled ahead of time (amended 2026-10-27, note 13).** A file an application ships unedited may have its bindings and handlers compiled to Rust at build time, from the IR, in a typed subset whose types come from the QML declarations. The tiers above stay the exact fallback: a file that is edited, or any expression the subset refuses, runs through tier 1 and tier 2, and a compiled handler swapped for its dynamic version mid-run gives the same values and signal order. Reads go through `Graph::read` as in every tier. Compiled code must be installed before creation completes. Note 13 measured this 2–5× faster than tier 1 and QuickJS per trigger, with no foreign runtime or collector; rustc's cost per file is still to be measured on a quiet machine. scriptc behind an FFI (note 13's back end A) was as fast but adds about 1 MB and cannot translate, keep emit order or be re-entered.
- **Rejected:** running everything in QuickJS, a Cranelift JIT (iOS forbids JIT), and typed-only AOT for every file (it needs annotations and works against hot reload; the amendment above keeps annotations out of the source and leaves edited files dynamic).

### 5.5 Native types in Rust

Source: 01 §3.7.

```rust
#[derive(QmlObject)]
#[qml(module = "QtQuick", name = "Rectangle", extends = Item, default = "data")]
pub struct Rectangle {
    #[qml(notify)] color: Color,
    #[qml(notify)] radius: f64,
    #[qml(group)]  border: Border,
    #[qml(readonly, notify)] painted_width: f64,   // exposed as paintedWidth
    #[qml(skip)]   cache: PaintCache,
}
#[qml_impl] impl Rectangle {
    #[qml(signal)]      fn pressed(&self, x: f64, y: f64);
    #[qml(invokable)]   fn contains_point(&self, p: Point) -> bool { … }
    #[qml(on_complete)] fn complete(&mut self, cx: &mut Cx) { … }
}
ljos::module! { "QtQuick" 2.15 => [Item, Rectangle, Text, …] }
```

- **What the derive generates.** It produces a `MetaObject` with:
  - a property table (name, type, flags, notify index, get/set function pointers, typed fast paths);
  - signal and method tables with JS thunks;
  - the attached-type id.
- **Final by default.** Properties declared in Rust are `final`, so the compiler can resolve them statically.
- **`QtObject` is the implied base.** A type QML can write by name (it has a `module` and is not `uncreatable`) and has no `extends` builds on `QtObject`, as every QML object does, so it has `objectName` at compile time and at run time. The struct needs no field for it: the engine stores the `QtObject` part beside the struct when it stores the object. A group's or an attached type's struct is never stored as an object of its own, so it has no implied base.
- **Explicit registration** through `module!`, not `inventory` or linker sections. This keeps static linking on iOS deterministic and hot reload simple.
- **Type dump.** `Engine::type_dump()` describes every registered type, and `to_json()` serialises it; the ledger and the type-tag tests read it from a live Engine. A type's signals are kept as its properties are: its own, each argument with its name and type, and what it inherits through `base`. A property's change signal is not listed, as `notify` implies it and it takes no arguments; `TypeDump::signals_of` gives all of a type's signals. The table of types (`qt_types`) lists change signals, since Qt's does, and writes an argument as `name:type`. `ljos run` reads a dump to preview a project without its Rust (§12.1). An application writes its own as `ljostypes.json`: its test calls `ljos::types::check_file` with its `register` function, which fails with the command to run when the file is stale and writes it under `LJOS_TYPES=write`. The file holds the application's types (properties with their kinds, signals, methods, own enums, default property, attached type, singleton or not), their bases by name, and the group and attached types they reach; the toolkit's own types are left out, as the compiler knows them. The build script compiles against the file with `ljos::build::compile_dir_with_types` (the facade's `build` feature) and runs none of the application's code, so the native types live in the application's crate. A base the file does not describe is looked up among the toolkit's types. `ljos::build::compile_dir` still takes a `register` function and a throwaway Engine instead, for a native module that is a crate of its own. Either way a binding of a property or handler that a native type lacks, written on it or through its group property or attached type (`limits.mx:`, `Counter.onTalied:`), or a `Type.Key` it does not have, is a build error: the oracle marks native types, and the group and attached types they reach, as fully described (`members_checked`), and the compiler reports such names on them, where on the table's types it leaves them for the runtime. A group or attached type that a table type reaches too is the toolkit's and is left as the table's types are.
- **`MetaObject` lifetime.** For types defined in QML it is behind an `Arc`, not `'static`, because hot reload replaces it.
- **Inherited entries are shared.** Each table holds the inherited entries first, so an index means the same member in every derived type. A property or signal entry is behind an `Arc` that the derived types share. A derived type copies only a pointer per inherited member, and copies the member itself only when it changes it (`require`, `reset_inherited`). A built table keeps no room to grow. With full copies, the registry was 6.4 of clocks' 13.7 MiB of Rust heap.
- **As built: enums, flags and reset.** A property says in its attribute which enum it holds (`enumeration = "Name"`) or which flags (`flags = "Name"`); a flags property accepts and reports combinations, and a plain enum written with two keys is a compile error. Enum keys are reachable from scripts through a type or an import namespace (`Text.AlignHCenter`, `T.ScrollBar.AlwaysOff`, `Font.Bold`). A property takes `undefined` only when its type declares a reset (`#[qml(reset)]`, `ljos_engine::Reset`): the write restores the default or the value it follows, and a binding that yields `undefined` stays installed and holds the reset value. Every other property refuses `undefined` with Qt's wording. Two ratchet tests in `ljos_controls` compare the tags and the resets of every registered type with the local Qt's type description; the resets still missing are listed there (`NOT_YET`).
- **As built: singletons.** A `pragma Singleton` file named by a `qmldir` `singleton` line or found through a directory import is made lazily, once per engine; native singletons are registered per engine. Their members resolve at tier 0 and run in tier 1 where the expression allows.
- **As built: script values in properties.** A `var` property declared in QML holds the script's own array or object: two reads give one value, `push` changes it in place and no change signal is emitted, as on Qt. Such a value is a reference into the JS heap, made only when a script first asks for it, so a UI that never runs JS still never starts the JS host. Native code keeps plain lists: `Engine::for_slot` decides on the way into a property and `Engine::plain` gives Rust a list back. A `list<T>` property declared in QML is the opposite: the script is handed a face of the property, so `push` and index writes change the property and do emit. A name that resolves to nothing throws a `ReferenceError` in a binding or handler, and the type errors that tiers 0 and 1 emulate go through the same fault sink as tier 2's (section 9.3).
- **As built: native items through the facade.** A crate that depends on `ljos` alone writes a visual item as `ljos::item` lays out: it registers the hooks of its type (`register_paint_item`, `register_key_item`, `register_wheel_item`, `register_pointer_item` with `register_hover_move` for a pointer moving over it with no button held, `register_text_input_client` for an input method's committed text and the cursor rectangle it places its window by). Outside paint it measures with the scene's own text engine (`with_text_engine`, which is `None` inside paint, where the engine is lent to `paint`) and a `Font` as a `Text` lays it out (`text_style`, at the system's text size when the font follows it). `device_pixel_ratio` is that of the display its window is on, read without making a `Screen` attached object. `repaint` paints the item again for what no property holds. A test or a host finds an item and where it is with `child_items`, `overlay` (the window's popup layer, which is not a child of the root) and `map_to_scene` or `scene_rect`, mapped exactly as `mapToItem(null, …)`. DECISION: the overlay is reached by a function of its own rather than appended to the root's `child_items`, which stays what `children` holds in QML; `scene_rect` is the box around the four mapped corners, so a turned item gives one rectangle to click the centre of. DECISION: through a hidden node of the reactive graph per painted item, read by the scene's paint effect and notified by `repaint`, made at the item's first paint and freed with the item; a `#[qml(private)]` counter property would put a member in the item's metaobject and type dump that QML could see and bind to. `ljos::style_hints` and `ljos::reduced_motion` give what `Qt.styleHints` says, and the prelude names `Prop`, `MetaRef`, `ConnectionId`, `Location` and `StandardPaths`. Its tests take the facade's `testing` feature: `ljos::testing::TestWindow` shows an item without a platform on a virtual clock and hands it keys, the pointer, the wheel and an input method's text, turns its accessibility tree on and reads it (`set_accessibility_active`, `accessibility_tree`, `accessible_node`, `accessible_item`), changes size on the same clock (`resize`), and while `advance` waits for outstanding task answers calls what the test serves them from on its own thread (`serve_while_waiting`, through `Platform::finish_with`), and the module plays the host and the system (text engine, preferences, application state, displays). Each of these is tracked where a binding or a `read` would ask, so an item that reads them repaints when they change and nothing polls. DECISION: `advance` keeps waiting for answers (a test reads what a user would see once they arrived) and gets a serve hook rather than a mode that skips the wait, which would leave each harness to deliver answers itself; the hook runs between 2 ms waits only inside that test-only wait. DECISION: the tree is read as one `ljos_a11y::AccessibleNode` value, a "window" node down to each exposed item with its role word, computed name, description, states in words ("checked", "disabled") and `focused`, the same reader and type the scenario steps compare (`ljos_bdd`'s `A11yNode` is that type), with each node's `NodeKey` for `accessible_item`; a live borrow of the scene's `Tree` would hand tests AccessKit's roles and flags, and a second reader would drift from the steps'. `crates/ljos_facade_app` is such a crate, with a terminal-like item, and its build is the test that the facade is enough.

---

## 6. Engine: objects, reactivity, creation

Source: 01 §3.1–3.5.

### 6.1 Object store

```rust
pub struct ObjectHeader {
    meta: MetaRef,                     // &'static or Arc<MetaObject>
    native: Box<dyn QmlObject>,        // typed Rust fields
    dynamic: Box<[Value]>,             // QML-declared properties
    owner: Option<ObjectId>,           // QObject parent (ownership)
    visual_parent: Option<ObjectId>,   // Item parent (may differ)
    context: ContextId,
    attached: SmallVec<[(AttachedTypeId, ObjectId); 1]>,   // lazily created
    js: Option<Persistent<Object<'static>>>,               // lazily created wrapper
    flags: ObjectFlags,                // COMPLETED, JS_OWNED, PENDING_DELETE, POOLED, FROZEN …
}
```

- **Storage.** Objects live in a `slotmap::SlotMap<ObjectId, ObjectHeader>`. Generational keys mean a destroyed object reads as `null` in JS instead of being use-after-free. Sparse side tables use `SecondaryMap`: override stacks, provenance bits and JS wrappers.
- **`Value`** is a 16-byte enum with these cases:
  - `Undefined`, `Null`, `Bool`, `Int`, `Real`;
  - `String` (an interned `Atom`), `Color`;
  - `Point`/`Size`/`Rect`, `Font`, `Url`;
  - `Object(ObjectId)`, `List`, `Map` (a record made in Rust; a script gets a new plain object, as it gets a new array for a `List`), `Enum`;
  - `Js(JsHandle)`, used for `var` properties that hold JS objects.
- **Data at the Rust boundary.** A Rust property, argument or return of `Vec<T>`, `Option<T>`, `HashMap<String, T>`, `BTreeMap<String, T>` or `serde_json::Value` has the type `TypeTag::Data`, which QML sees as `var`. A script's plain objects and arrays are copied into it as `Map`s and `List`s all the way down, once, where a `var` keeps the script's own object. `Engine::deep_copy` makes that copy of a `var` a Rust method was handed. No JS source is made for either direction.
- **Destruction.** `destroy_now` runs `Component.onDestruction` first, while everything is still alive, then the handlers connected to `destroyed` (`SignalIndex::DESTROYED`), which every object has whatever its type declares, as every `QObject` has. An owner is told before what it owns, as in Qt, and each handler runs once with the object, still alive, as its argument. Then the objects go, children first, and their connections with them. Objects still alive when the Engine itself is dropped are not told.
- **Value types.** Ljós follows the Qt 6.8+ write-back reference behaviour for value types and tests it (01 §4.7).
- **Re-entrancy rule.** A read can run a binding, which can call JS, which can call back into Rust. So the Engine never holds a `RefCell` borrow across a JS call. State is split into separately borrowed parts (graph, objects, JS), and every API uses short borrows.

### 6.2 Reactive graph (`ljos_reactive`)

The graph is push-pull with three colours, allocated lazily: a node exists only once its property has been observed or bound.

```rust
pub enum NodeState { Clean, Check, Dirty, Computing }
pub enum NodeKind  { Source(PropKey), Binding { target: PropKey, binding: BindingId }, Effect(EffectId) }
pub enum WriteKind { Imperative /*breaks binding*/, FromBinding, Initial /*no Behavior*/, Override(OverrideId) }
pub trait Reactive {
    fn read(&self, key: PropKey) -> Value;
    fn write(&self, key: PropKey, v: Value, how: WriteKind) -> Result<(), QmlError>;
    fn notify(&self, key: PropKey);
    fn batch<R>(&self, f: impl FnOnce() -> R) -> R;
}
```

**Writes**
- A write marks direct observers `Dirty` and transitive observers `Check`.
- Writing an equal value does not propagate, but `notify` always does.
- Effects then flush **synchronously**, in height order, before control returns to the writer. This reproduces Qt's synchronous `onXChanged`.
- A write made while an effect runs flushes every root that waits, so what the writer reads next is current. Past a few levels of such nesting, a write flushes only what its effect's run queued, and roots that were already waiting run after it in the flush that ran the effect. Qt's handlers likewise nest only along a chain of changes, so the call stack grows with that chain and not with how many roots wait (hiding a parent of 2000 items is 2000 changes in a row, not 2000 inside each other).
- `batch` defers the flush. Component creation, model resets, animation ticks and layout all run inside a batch.

**Effect roots** are evaluated eagerly. They are:
- signal handlers and `Connections`;
- Behavior interceptors;
- Rust subscribers;
- polish effects (anchors, positioners, Layouts);
- accessibility observers, only while an assistive technology is attached.

**What belongs in the graph.** The graph is for user QML bindings and handlers and what stands for them. The scene's own bookkeeping per item (structure, inherited visibility, anchors, place, geometry, text polish) moves out of it to dirty bits on an *item record*, set by a *native field change* ([ADR 0001](adr/0001-scene-bookkeeping-outside-the-graph.md), terms in [CONTEXT.md](../CONTEXT.md)): per-item effects are most of what a new delegate costs to adopt and to tear down. Until each step of that lands, the part it replaces still runs as an effect.

**Every other binding is lazy.** It stays dirty until something reads it. Paint reads only visible items, so an off-screen delegate's `color: hovered ? a : b` costs nothing. This is the main saving for off-screen content. It is safe because Ljós has no render-thread sync point.
- Two guards cover side-effecting bindings. A binding that writes during evaluation is promoted to an effect root.
- `--eager` mode exists for differential testing against Qt.

**Where pulling shows in the order of signals.** Qt pushes a change through bindings in the order their connections were made. Until a binding's turn comes it holds the old value, and every handler that runs first reads it stale: the property's own change handler, the other handlers connected before the binding, and a signal sent before the change is announced. MouseArea's `released`, for example, is sent while `pressed` is already false but before `pressedChanged`. A pulled binding has no old value to hold. It is worked out against what it depends on at the moment it is read, so each of those handlers reads it new. `features/bindings/change-handler-order.feature` covers the case apps rely on most, the object's own change handler: there Ljós matches Qt. `features/bindings/binding-read-in-a-signal.feature` (`@qt-diverges`) covers the rest. Most of the time reading the value new is harmless or better. It matters where reading the binding has an effect. `Drag.active: area.pressed`, read in `onReleased`, ends the drag before `Drag.drop()`; Qt still sees it active and the drop lands. Call `drop()` before reading `Drag.active`, or read `pressed` itself. The same rule makes a binding that nothing reads not run at all, so a type whose binding must act although nothing reads it watches the property itself (`Drag.active` does). A test engine with eager bindings hides that case, so a `@desktop` scenario, which runs a real application, guards it.

**Binding loops**
- Re-entering a node that is `Computing` is a binding loop. Ljós logs Qt's message once, returns the stale value and does not re-enter. The message names the property and points at its name in the source (`PropertyBinding.name_span`), as Qt's does.
- Which member of a loop is reported depends on the order bindings first run. During creation the engine follows Qt's set-up order: bindings on some properties are installed but held back (`Graph::hold`) until the component's other bindings have run, then released. Without this an eager engine reports a loop on an acyclic chain.
- A loop ends at the binding the change first entered, and each other member keeps the value it had when it changed, as in Qt, lazy or eager. A handler a type puts on a bound property (`Graph::watch`) runs inside that binding's update, as Qt runs setters and change handlers: a value it places (an Image's implicit size from its `sourceSize`) is not a binding, so the loop goes through it to the next binding and ends at the bound property (`features/bindings/rebound-loops.feature`, `features/items/image-source-size-loop.feature`).
- A lazy binding is checked when it is first read, so a loop nobody reads is not reported until then. `--eager` reports it at creation, as Qt does.
- Handlers that ping-pong between each other are capped at 100 re-dirties per node per flush; the flush ends with `RerunLimit`.

**Override stack.** `Binding {}` and `State`/`PropertyChanges` push onto a per-property override stack above the base binding or value. `restoreMode` pops it. Assigning a value imperatively breaks the binding, and `Qt.binding()` reinstalls one. Report 09 confirmed both against Qt 6.11.2.

**As built: states and the override layer (M2).** A state never saves a value and writes it back. Entering it puts each change on the override layer of its property (`Engine::push_override`) and leaving takes the layers off in the opposite order, so the value or the binding underneath, which went on changing, is what shows again. `Binding` is one such layer that lasts while `when` is true. `PropertyChanges`, `AnchorChanges`, `ParentChange`, `StateChangeScript` and `StateGroup` are built, with `when`, `extend` and `restoreEntryValues`.
- **Members of another object.** `PropertyChanges { width: 10 }` and `box.width: 10` write members the type itself does not have. A crate says which of its types keep such members (`Engine::register_foreign_members`) and is handed them instead of the "non-existent property" error; the engine only keeps the expression runnable.
- **Transitions.** One of the item's transitions is chosen by `from`, `to` and `reversible`. The properties are in the new state at once as far as bindings and layers go; each property the transition names is held at the value it had by one more layer, the animations run as one timeline, and the holds come off at the end. A property the transition does not name has its new value at once.
- **Animations a transition uses:** `AnchorAnimation`, `ParentAnimation`, `PropertyAction` and `ScriptAction`, beside the property animations, `SmoothedAnimation` and `SpringAnimation`. `SpringAnimation` is compared with values measured on Qt 6.11.2 (`animation/fixtures/spring_qt.txt`).
- **Not built:** object values and signal handlers in `PropertyChanges`; `ParentAnimation`'s `target`, `newParent` and `via` are stored only; the state an object is in is not carried across a hot reload; a change to `states` after creation is not followed.

**Provenance.** Each property has a provenance bit, *assigned* or *bound*. Hot reload uses it (section 12).

**Translations.** Every `qsTr` reads `I18n::revision`. Changing the language bumps one node, and only the translated bindings re-evaluate, visible ones first (section 15).

**Crate choices.** `slotmap` is used. `salsa` is used only in the compiler and LSP. `reactive_graph` and `sycamore-reactive` are design references only (01 §3.3).

### 6.3 Creation and incubation

Creation is a resumable state machine, the `Incubator`. Its phases:
1. Allocate objects and register their ids.
2. Apply initial, required and literal values in one batch. A missing required property is an error.
3. Install bindings. They start dirty, except effect roots.
4. Call Rust `on_complete`.
5. Attach Behaviors.
6. Run `Component.onCompleted`, in an order measured against Qt.

- **Charging.** Each step charges the owner's `SliceBudget`.
- **Synchronous creation** runs to completion.
- **Time-sliced creation.** `Loader.asynchronous`, `incubateObject`, ListView cache delegates and StackView pushes run in slices of at most 3 ms per frame at 60 Hz, scaled down at higher refresh rates. `forceCompletion()` drains the remaining work.
- **As built.** `Loader.asynchronous` and `Component.incubateObject` (with its incubator object: `status`, `object`, `onStatusChanged`, `forceCompletion()`) run in slices. A frame gives creation jobs 3 ms at 60 Hz and below and the same share of a shorter frame above it, never less than 250 µs; a host that does not report its refresh rate (`QuickScene::set_refresh_rate`; the egui shell does not yet) is taken to show 60 Hz. The time is charged to the boundary or Loader that owns the job. Applying values, evaluating bindings, completion hooks and `Component.onCompleted` are each cut into steps; allocating a component's objects is a work list cut the same way (one object a step), and one long binding or handler is never interrupted. An incubator whose parent is destroyed while it loads ends in `Error`, and an incubator object belongs to the script that holds it (`script_owned.rs`): it goes when the JS collector frees its wrapper. `Qt.createComponent` and `Component.createObject` with initial properties work from scripts. `Qt.createQmlObject` needs the compiler, so it goes through a hook the dev host installs and reports an error in a shipped app. A file that was not compiled with the program, such as a page a program fetches while it runs, goes through the same hook: with `dev-qml`, `Qt.createComponent` and `Loader` compile it when it is first asked for, with the files beside it. A shipped app runs only what it was built with, so it says so in the component's `errorString()`. For a file that is not there it gives qml6's `No such file or directory`.

- **Component plans.** What every instance of a component shares (native property indexes, metas, attached types, id slots) is linked once by the runtime, on first use, and kept beside the `LoadedFile`: the *component plan* ([ADR 0002](adr/0002-component-plans.md)). The IR carries only facts that do not depend on native types, so a native type that changes never leaves it stale. Instancing is then allocation, indexed writes and indexed binding installs; an evaluation reads its free names by slot. A view's *row plan* does the same for what each delegate is handed per row.
- **Budgets.** `crates/ljos_quick/tests/budget.rs` pins exactly what a new ListView delegate and a plain Item with a Text add: engine objects, graph nodes by kind (`Graph::census`), lookups by name (`ljos_engine::lookups`, with the `lookup-counts` feature) and heap allocations. A change that lowers one updates the pin; one that raises one fails there.

### 6.4 One Engine, many windows

**Decision (D2): one Engine per app.**
- Each `Window` is a root with its own `ProcId` boundary and its own a11y and focus trees.
- Singletons are per Engine by default.
- A native singleton type can be one per window (`#[qml_singleton(scope = "window")]`), for apps that are used to a `QQmlEngine` per window. The scope is chosen when a root is made: a root the host makes with no parent (`create_from_url`, `Component.create` from Rust) opens a window's scope; a root made under an object, and one a script makes with no parent (`createObject(null)`), is in the scope of that object or of the script's file. Everything under the root reads the scope's object by the type's name, through either tier. The object is made when something under the window first reads it, and destroyed after the window's own objects when the root is destroyed. From Rust, `Engine::singleton_in::<T>(obj)` gives the object of `obj`'s window (`singleton::<T>()` is an error for such a type), and `Engine::on_scoped_singleton` hears each one made and dropped (while it still stands). A reload keeps each window's object: the new root takes the old root's scope. A host can hand a window an object it already made, before the root exists: `Engine::window_scope()` opens a scope, `set_singleton_in::<T>(scope, obj)` puts the object in it (one per type, and only a `T`), and the root made inside `in_window_scope(scope, ..)` takes that scope, with any creation call. Such an object is the host's, as with Qt's `qmlRegisterSingletonInstance`: the window's end drops it from the scope (listeners hear it dropped) but does not destroy it. `singleton_in_scope::<T>(scope)` reads a scope with no QML at all, as a headless test does, and `close_window_scope` ends one that no root took. A QML `pragma Singleton` file whose scripts read a window type, directly or through another such file, is one per window too, made in the window's scope and destroyed with it (#126); any other stays shared, as do the module cache and compiled components. A window type read where no window is, such as from Rust with `singleton_by_name`, warns once and reads `null`. `app_inspect` lists the singletons an object's window has made. Scenarios: `features/language/singletons-per-window.feature`, `features/tooling/mcp/window-singletons.feature`.
- A singleton is made when first read, by `T::default()` (`Engine::register_singleton`) or by a closure given the Engine (`Engine::register_singleton_with`), which hands it what it depends on. `uncreatable` keeps QML from making one of its own and nothing else.
- Windows that are really independent apps can run separate Engines that talk through messages.

This keeps models and singletons shared without proxies (01 §4.8, 03 §3.1, 07 P1-27).

**As built (M2).** A `Window` says what it wants (title, size, visibility, modality, flags, transient parent) and a host that owns platform windows reads that as a `WindowSpec` and tells the window what the platform made of it (`ljos_controls::window::system`). Without a host a window is an item of the scene it was put in and showing, sizing and activating happen there, so a test without a display behaves the same. `Application`, `Qt.application` and `Screen` report what the host says; without one there is a single 800 by 800 screen and the application is active. `ljos render` is such a host for its root: a root window that leaves its size unsaid gets the size of the picture, as a platform window would give it. `Application.styleHints` is `Qt.styleHints`, one object with Qt's documented constants as an offscreen desktop reports them. The attached `SafeArea` reports per item how far it reaches into what the host covers (`ljos_quick::set_safe_area_insets`, logical pixels from the scene's edges; 0 without a host) plus its own `additionalMargins`; the shell does not report platform insets yet, and an ancestor's `additionalMargins` is not passed on, as in Qt 6.11. A `WindowContainer` shows its `window` as an item of its own, with no platform window even under a host: shown, belonging to no other window, as large as the container, whose implicit size is the window's (the window's `x` and `y` are left alone). Not built: `SafeArea.margins` as a value (`SafeArea.margins.top` only), attached `Screen.*` per window, `Screen.primaryOrientation`, and a `closing` handler refusing `Qt.quit()`.

---

## 7. Rendering, layout and text

Source: 02 §3; text from 05 §3.3.

### 7.1 Render tree and backends

**Render tree**
- `ljos_render::RenderTree` is a slotmap of `RenderNode`s. Each node holds:
  - its local and world `kurbo::Affine`, size and subtree AABB;
  - opacity (multiplied down the tree, as Qt does), `ClipSpec` and `LayerSpec`;
  - a second, optional `ClipSpec` for the node's own drawing only (`set_content_clip`), which neither its children nor hit testing heed: a `TextField` draws its text inside its padding while its background, a child, fills the padding;
  - `visible`, a cached `Arc<DisplayList>` in local coordinates, `Dirty` bitflags and `last_visible_frame`.
- The `Dirty` flags are `CONTENT`, `TRANSFORM`, `OPACITY`, `CLIP`, `ORDER`, `BOUNDS`, `LAYER`, `A11Y` and `VISIBLE`.
- Only `CONTENT` rebuilds a node's display list. Transform, opacity, clip and z changes only re-compose.

**Painting.** Items implement `PaintContent::paint(&self, cx, &mut DisplayListBuilder)` and never touch the GPU. The `DrawOp`s are `Rect` (with radii, fill, border and AA), `Shadow`, `Image`, `NinePatch`, `Glyphs`, `Path`, `Shader` and `Embed`. `Paint` is `Solid`, `Gradient` or `Image`.

**`RenderBackend { sync, encode, trim, snapshot_rgba }`** has three implementations:

| Backend | When | Notes |
|---|---|---|
| **B0 epaint** | M0 onward | Cached `Arc<Mesh>` per node. Gradients use vertex colours. Shadows use a blur or a nine-patch. Axis-aligned clips use `with_clip_rect`. Rotated clips and `layer.*` are approximated, with a debug warning. Paths are tessellated with lyon, or rasterised to cached textures with vello_cpu; this is decided at M2. |
| **R vello_cpu** | M0 onward | Deterministic reference renderer: fixed SIMD level, one thread, bundled fonts unless told otherwise (`ljos render --system-fonts`). Used for the Qt comparison, and to draw a window into shared memory where EGL would draw in software (below). The qtdoc gallery renders with the machine's fonts, because its qml6 pictures are drawn with them on the same machine; tests and the ledger keep the bundled font. |
| **B1 wgpu** | M4, or earlier if B0 profiles badly | Plugs in through `egui_wgpu::Callback` segments. Instanced SDF quads for rects, borders, gradients and shadows. Textured quads for glyphs and images. Retained instance buffers per batch root, so scrolling updates one uniform. Offscreen targets for layers, effects and rotated clips. vello_gpu renders paths into cached atlas textures. |

**As built: paths (decided at M2 by measuring).** `DrawOp::Shape` carries an `Arc<BezPath>`, a `ShapeStyle` (a fill rule, or a whole `kurbo::Stroke` with joins, caps, miter limit and dashes) and a `Brush` (a colour, or a linear, radial or conical gradient with a spread); `DrawOp::Path` means what it did. R fills and strokes it with vello_cpu, curves flattened to 0.02 device px. B0 is a mix by brush and by complexity:
- **Triangles by default.** lyon (`lyon_tessellation`) fills the outline (a stroke is outlined by kurbo first), and a feather one device pixel wide along the boundary anti-aliases it. Tolerance is 0.1 device px. The mesh is built once per outline and power-of-two scale bucket and is mapped, not rebuilt, when the node moves or scales.
- **A vello_cpu picture in a texture** for radial and conical gradients (vertex colours cannot carry them), for a repeated or reflected linear gradient with more than 16 periods across the shape, and for a stroke of more than 1 500 flattened segments. It is filled again when the scale or the sub-pixel offset changes. A picture over 8192 px a side or 16 MPx, or one that would take the frame's shape textures past 64 MB (section 8.5), is drawn as triangles instead, with a warning.
- Geometry is kept for as long as the same `Arc` comes with the same style and brush, so an unchanged shape costs a lookup. `trim(Moderate)` makes a shape that was not drawn in the last frame give up its triangles and pixels; `trim(Critical)` drops all of them. Nothing outside `ljos_render` calls `trim` yet.
- Measured (release build, one thread, scale 1 / 1.5 / 2; "error" is the mean difference from an 8×8 supersampled picture over edge pixels, in 1/255; the icon and the arc at the tolerance that was built, the other rows at 0.25 device px; GPU fill time was not measured):

| Case | Triangles | Texture |
|---|---|---|
| 24 px icon (even-odd cog) | 6.8 / 6.8 / 7.9 µs to build, 7 KB held, error 10.5 / 8.0 / 6.6 | 3.6 / 5.2 / 6.2 µs, 2.3 / 4.6 / 7.7 KB, error 5.1 / 7.0 / 7.5 |
| Chart line, 5 000 points, 1280×720, smooth | 14.5 / 15.5 / 14.0 ms, 3.3 MB held, 1.7 MB uploaded per painted frame, error 14.9 / 12.3 / 10.4 | 1.2 / 1.4 / 1.8 ms, 2.3 / 5.1 / 9.0 MB held and uploaded per change, error 21.2 / 15.7 / 12.1 |
| The same, noisy | 41 / 41 / 42 ms, error 19.9 / 16.4 / 15.5 | 2.4 / 3.2 / 4.3 ms, 2.5 / 5.7 / 10.1 MB, error 41 / 34 / 28 |
| Chart line, 500 points | 0.28 / 0.35 / 0.40 ms, 0.2 MB, error 5.3 / 5.6 / 5.7 | 0.21 / 0.37 / 0.66 ms, 2.3 / 5.1 / 9.0 MB, error 1.0 / 1.0 / 0.8 |
| Area fill, 500 points | 59 µs, 76 KB, error 4.7 / 4.6 / 4.6 | 0.20 / 0.39 / 0.77 ms, 3.0 / 6.7 / 11.9 MB, error 0.2 to 0.3 |
| Animated arc, 200 px, each frame a change | 12 to 15 µs and 4 to 6 KB uploaded per frame, error 9.6 / 9.2 / 8.9 | 16 to 38 µs and 124 to 484 KB per frame, error 22.4 / 23.2 / 22.6 |

  A mesh without the feather is off by 47 to 68, and a texture drawn half a device pixel off its grid without being filled again by 19 to 44. Triangles cost the same at every scale, hold 10 to 150 times less for a large shape and need no texture; the texture is exact for polygons and ten times faster for a dense stroke. The feather is linear, so it is at its worst at sharp tips and at features thinner than a pixel.

**Images.** A draw op names pixels by `ImageHandle`; the pixels live in `ljos_quick`'s per-engine `ImageStore`, an LRU cache keyed by URL and requested size with a byte budget (section 8.5). Blit geometry (source and destination rectangles, tiling, mirroring) and nine-patch slicing are computed in `ljos_render`, once for both backends. `ljos_render` depends on `ljos_runtime` for `MemoryPressure`, which is defined once. Decoding runs on the `ComputePool`, at most 8 at a time, and wakes the UI once per result; a request waiting for its turn is dropped unstarted when every item that asked for it has gone or loads something else (a list's delegates pushed out of view before their pictures came). PNG and JPEG are decoded by `png` and `zune-jpeg`; SVG (and SVGZ) is parsed by `usvg` and drawn by `resvg`, without their text support, so there is no second font stack. A PDF document is a drawing too, rasterised by `hayro` (pure Rust, on `vello_cpu`, with the standard fonts it embeds): `frameCount` is its pages and `currentFrame` the page shown, which is part of the cache key; a page is as large as its crop box in points, and turning to a page it does not have is ignored, as in Qt (`features/items/image-pdf.feature`). hayro brings a rasteriser, font readers and brotli of its own (5.6 MB of a stripped release `ljos`, 1.3 MiB of resident memory at start), so it is behind the `pdf` feature of `ljos_quick` (and of the `ljos` facade), off by default, as Qt's PDF image plugin is loaded only where it is installed; the `ljos` runner turns it on. Without it a PDF source is Error with Qt's warning for a format it has no plugin for, "Unsupported image format". The format is told from the bytes, never the file's name, and a source whose last name has no suffix is looked for with each format's suffix in Qt's order. A drawing is made at its `sourceSize`, stretched when both sides are given, and at the largest scale any scene paints at, so it stays sharp on a dense display; the scale is part of the cache key and the item keeps its size in logical pixels. A PNG or JPEG keeps no more pixels than the largest view can show: without a `sourceSize` it is made smaller, by averaging, until its longest side is the largest scene's longest side in device pixels rounded up to a quarter octave (512, 640, 768, 896, 1024, …; the `limit`, part of the cache key, so that a window resized a little shares pictures with what it was), and an item drawn at most two thirds that large makes its own pixels smaller from what is kept, with nothing fetched or decoded again, and the larger copy is let go. That holds for an item that is made and not painted, such as a list's row kept beyond the view, as soon as its picture comes and whenever its size or fill mode changes (`features/items/image-kept-size.feature`). Once a picture is a texture its pixels are let go, when they can be made again from fewer bytes than they are: a file is read again, and registered, `data:` or fetched bytes fewer than the pixels are kept for it. A picture that comes after every item that asked for it has gone is kept the same way, as those bytes; one whose bytes are more than its pixels (a photograph larger than what is kept of it) is not kept at all, and the next item that names it fetches it again, as Qt does with any picture that comes that late (`features/items/image-network.feature`). Whatever reads the pixels after that (Canvas `drawImage`, effects, tinting, particles, the R backend, a shrink) has them made again once, from those bytes and never the network, and they stay; an upload is not a reader. B0 keeps a texture that is a picture's only copy past its time unseen, and drops it under memory pressure. The cache's budget counts each picture as its pixels. What QML sees does not change: `implicitWidth`, `implicitHeight`, `sourceSize` and `paintedWidth` report the picture's own size (`features/items/image-large.feature`), but for what Qt does with `PreserveAspectFit` and only one side given: the other implicit side is as long as the picture is drawn, never longer than its own (`features/items/image-fill-mode.feature`), and the display list draws the fewer pixels over that size. An item drawn larger (its size, `scale` or the display's scale grew) asks for sharper pixels in the background, and whatever reads the item's pixels one to one (Canvas `drawImage`, effects) gets the whole picture. A load finds the same picture kept for another limit before it decodes. A local still picture of more than 512 by 512 pixels with no `sourceSize`, whose Image does not set `asynchronous`, is Ready at once from its header, as Qt's is from its decode, and its pixels are decoded on the pool at the size it is drawn (its size, fill mode and the `scale` of it and its parents) and painted when they come, so the window keeps answering while a folder of large pictures loads; a smaller one is decoded on the spot, and a broken body turns it to Error when its decode comes back (`features/items/image-decoded-aside.feature`). So that a window does not open blank where Qt shows the picture, a frame waits for such decodes: in a host whose loads start after the frame (`ljos_shell`), the picture is asked for just before the next painted pass, once the scene is laid out so that it decodes at the size drawn, and the pass waits for the decodes it started, at most 150 ms for the window's first frame and 50 ms for later ones, woken by each result and never polling (`ImageStore::wait_for_frame`); what has not come by then is painted when it comes. A later frame does not wait while the pointer is held or wheeled, a finger is down, or the scene asks for the next frame within one (a flick, an animation), and later frames wait no more than 100 ms in all in any second (`ImageStore::wait_for_later_frame`): flicking photosurface's 472 pictures, waiting there showed 5 frames a second where not waiting showed 61 (docs/BENCHMARKS.md). Readers other than the screen (Canvas `drawImage`, effects) still decode it on the spot. Rust code (an item that paints itself) asks the store for a picture by URL with `ljos::item::load_image`, which goes through the same cache, queue and decode threads and replies on the UI thread with an `ImageHandle`, never inside the call: from the delivery that follows the decode, or from a due-now timer when the picture is cached; the picture stays cached while the handle is held. A baseline JPEG at least twice as long as it is kept is decoded smaller by `jpeg-decoder`, whose inverse transform gives a half, a quarter or an eighth of the picture directly, so the full picture is never held; any other picture is decoded whole for a moment (`zune-jpeg` has no scaled decode). The store lets decodes start only while what they will hold together fits an eighth of the cache budget, so that at most one larger picture is decoded at a time: a decode holds the encoded file, the pixels it keeps, and what it makes them from, as the header gives it (the whole picture, or the smaller pixels of a JPEG decoded smaller and, for a progressive one, the coefficients of the whole picture, which its decoder keeps until the last scan). Not built: SVG text, raster images embedded in an SVG, and an SVG of zero size (Qt shows it as Ready and empty; here it is an error).

**As built: shader effects.** `ShaderEffect` names its shaders by URL, in Qt 6 a `.qsb` file. Ljós does not read `.qsb` (its container is Qt's, and is not reverse-engineered): for `X.frag.qsb` it reads `X.frag.ljsh`, its own artifact, and otherwise compiles the GLSL 440 source `X.frag` beside it, which is what a Qt project ships to `qsb` anyway. `ljos_shader` does the work, with `naga` (pinned in `[workspace.dependencies]`, used by no other crate) behind its `compile` feature:
- the source is rewritten where naga's GLSL front end differs from Vulkan GLSL (a combined `sampler2D` becomes a texture and a sampler, `out gl_PerVertex` is dropped), parsed, validated, and written out as GLSL ES 300 and desktop GLSL 330;
- the uniform block is reflected with std140 offsets (`qt_Matrix` and `qt_Opacity` first, then the members by name and type), and each sampler keeps its QML name;
- the result is `Compiled`, which serialises as `X.ljsh`. `ljos shaders <dir>` writes them at build time; a shipped app turns `ljos_quick`'s `shader-compile` feature off, carries no naga and loads only `.ljsh`. Running Qt's `qsb -x` to recover the source of a project that ships only `.qsb` is a manual, optional step, never needed at run time.
- With `compile` on, a shader with no `.ljsh` is compiled once on the `ComputePool`, off the UI thread, and kept in memory by path while the file's modification time and length stay the same, so the delegates of a view that share a shader compile it once; `status` is `Uncompiled` until it is done (Qt compiles synchronously on GL). It costs about 0.4 ms per shader in a release build.

The item paints `DrawOp::Shader`: the programs, the uniform bytes, the sampler pictures, the grid and the blend and cull modes, in an `Arc` that stays the same while nothing it read changes. Every other property of the item is a uniform by name, converted to the member's declared type; a sampler property takes an `Image`, a `Canvas`, a `ShaderEffectSource` or an item whose `layer.enabled` is set, as pixels from the CPU picture path below, with the source's `smooth`, `mipmap` and `wrapMode` as the sampler's filter, mipmaps and wrapping; any other item samples as transparent black, as on GL. B0 turns the op into a paint callback whose rect is the item's box on screen; it fills `qt_Matrix` (item to clip space) and `qt_Opacity` from the composed node, so moving or fading the item writes 68 bytes and compiles nothing. The GL painter in `ljos_shell` links the programs once per `Arc`, keeps one uniform buffer, a grid vertex buffer per resolution and the textures egui already has, and draws with Qt's blending (premultiplied, or none with `blending: false`) and `cullMode`. The present comparison treats the same callback over the same rect as the same shape, so an idle effect draws nothing. The wgpu painter, a `QmlView` embedded elsewhere, R and the snapshot backend draw a custom shader as nothing, as Qt's software scene graph does. A `ShaderEffect` with no shaders is Qt's default shader, `source` times opacity; it is painted as an `Image` and is the same in every backend.

`ShaderEffectSource` is built on the CPU path `MultiEffect` uses: the source's subtree is composed, fingerprinted and rasterised by R in device pixels only when it changed (`live: false` keeps the first picture until `scheduleUpdate()`), cropped to `sourceRect` (logical pixels), resampled to `textureSize` (pixels), and painted as an `Image` stretched over the item. A mipmapped picture drawn smaller is shrunk first, as sampling the mipmaps averages. `textureMirroring` flips what a shader samples, not what the item draws, as GL does: the default reads upright. `hideSource` stops painting the source without telling QML, and it still takes the pointer, as Qt documents. A source that holds its `ShaderEffectSource` is drawn again each frame until the picture comes out the same, which is the tunnel GL shows whether or not `recursive` is set; `format` and `samples` are kept and change nothing. An item's `layer` is taken the same way, with the layer's settings, while an effect samples it. Not built: drawing a layered item through its own picture (a layer that is not live, or a smaller `layer.textureSize`, shows only in what samples it) and `layer.effect`.

**As built: a window drawn on the processor.** `LJOS_PAINTER=cpu`, or `ShellConfig::drawn_on_processor`, has `ljos_shell` draw each window with R into memory shared with the window system: a `wl_shm` buffer on Wayland, an `MIT-SHM` image on X11, through softbuffer. No EGL display is made, so neither Mesa's software rasteriser nor its LLVM is mapped. For the screen R uses the machine's SIMD level and `vello_cpu`'s workers (the cores less one, at most three). `ljos_render::frame_damage` compares the frame with the last one: it finds the items that moved, changed what they draw, their opacity or their clips, came, went or changed their order. Only those parts are drawn again (`render_area`), together with the changes the buffer lacks by its age, and only they go to `wl_surface.damage_buffer`. The view stops encoding the scene for B0 (`set_encodes(false)`), so egui keeps input and layout and draws nothing. Three things R cannot draw are left out and logged once: a `ShaderEffect`'s program, what egui draws (the inspector), and what is behind a window that can be seen through. With no `LJOS_PAINTER` set, the shell chooses this path itself where EGL would offer only a software rasteriser (`software.rs`, told without EGL: Mesa is told to draw in software, no render node opens, or the compositor or X server takes no GPU buffers), provided the background is opaque. It wins there on memory and draws every frame as soon (docs/BENCHMARKS.md, SHM1). A window that then shows what R leaves out takes every window to GL before that frame is drawn, once. `features/window/cpu-drawing.feature` checks the screen against R's own picture, and `features/window/software-choice.feature` checks the choice.

**Paint order** is implemented once and shared by paint, hit-testing and a11y order:
1. children with `z < 0`, sorted stably by `(z, declaration index)`;
2. the node's own content;
3. the remaining children, sorted the same way.

`visible: false` or `opacity: 0` prunes a subtree from paint and hit-testing. Keyboard focus is still possible there, as the Qt docs say.

**Colour.** Premultiplied sRGB, blended in gamma space, which is egui's model and Qt's default. One QML logical pixel equals one egui point.

**Rejected:**
- pure immediate mode, whose per-frame cost grows with the whole tree;
- vello compute as the main renderer, because older Android GLES devices are unreliable with compute;
- Skia, a large C++ dependency;
- egui `Galley` for text, which has no bidi;
- Taffy as the core layout engine.

### 7.2 Frame pipeline

This is the canonical order, merging 01 §3.5, 02 §3.2 and 03 §3.3. It runs on the UI thread.

```
 0 wake       FrameScheduler woke us: input, UiMsg, timer/animation deadline, or dirty mark
 1 inbox      drain UiMsg under budget (Critical lane): service results, model deltas, reload patch
 2 input      egui RawInput → InputRouter (hit test, focus, gestures, keys) → handlers; each is a turn
              followed by a synchronous effect flush (Input lane)
 3 timers     Timer, setTimeout, Qt.callLater (deduplicated), JS job queue (pump_jobs)
 4 destroy    deferred destroy()
 5 animate    FrameClock at the predicted present time; animations write inside one batch
 6 polish     anchors, positioners, Layouts, implicit sizes, Text layout; ≤ 10 passes, then warn
 7 incubate   creation slices: Visible lane, then Prefetch (cache buffer) under the remaining budget
 8 sync       dirty visible items → RenderNodes; pull render bindings; rebuild CONTENT-dirty lists
 9 compose    world transforms, subtree bounds, paint order, culling → FrameSnapshot
10 a11y       TreeUpdate for A11Y-dirty nodes, only if an AT is active (update_if_active)
11 encode     backend.encode(FrameView) → egui shapes / callbacks; IME rect; textures
12 present    egui end → tessellate (B0) → egui_glow on EGL (egui-wgpu without it) → present (vsync), only if the picture changed
13 idle       Idle lane while budget remains: QuickJS GC, cache trims; compute next wake
```

- **`FrameSnapshot`**, the output of steps 8–9, holds `Arc`'d display lists, transforms and order. It is immutable, and it is where a render thread would attach (02 §3.2).
- **Debug views.** `LJOS_VISUALIZE=batches|clip|changes|overdraw|layers` shows what each frame did.

### 7.3 Shells

- **M0–M1:** `ljos_shell` runs on eframe. This is the fastest route and gets Android early.
- **From M2:** `ljos_shell` is Ljós's own loop on `ljos_host` + egui-winit + egui_glow on EGL, with egui-wgpu where there is no EGL. It owns the AccessKit adapter, which grafts its subtree into egui's tree, plus the surface lifecycle, present mode and devtools hooks.
- **`QmlView`** stays an `egui::Widget`, so QML can be embedded in any egui app. Embedded, it falls back to emitting a11y nodes through `ctx.accesskit_node_builder`.
- **Native egui widgets inside QML.** An `EguiItem` gets a child `Ui` at its world bounding box, which allows translation and uniform scale only. QML items painted above it register occluders.

**As built (M2).** `ljos_shell::run` is Ljós's own loop (`desktop.rs`) on `ljos_host`, egui-winit and a painter; `eframe` is an optional feature and a test proves the default build does not link it. There is one platform window for the root and one for every other QML `Window` that is shown, each with its own egui context, scene view, painter and, while open, surface; the graphics device is shared. All bookkeeping happens when the host is about to wait: what the application's windows want is compared with what the platform has, due passes are asked for, and the host is told the earliest time any window wants to be drawn. A window nobody asked about is not drawn, and neither is a pass whose shapes, scale, background and textures are those already shown (`present.rs`). Such a pass comes from a pointer event nothing reacts to, a click that changes nothing, or an animation of something out of sight. The next pass then waits for the scene's own next wake, not the frame-early time egui asks for, and from the second such pass in a row it waits at least one frame. The passes egui asks for itself while a window's pass runs (two more after any input, one more after a pass that asked for another at once) are not run: the scene says after its pass when it wants the next (`QmlView::take_asked`), and messages, pictures and the platform's answers wake the loop through a waker of their own (`QmlView::set_waker`), so a click that changes nothing is a pass for the press and one for the release. A timer stopped before its time tells the scene, so no pass runs for a deadline that went. A drawn pass is paced by the display, and the next one also waits until a frame after the last: llvmpipe on an X server without DRI3 presents at once, and there maroon's splash was drawn at 180 frames a second. On a display that holds each frame until it is shown, the passes already come a frame apart and nothing more is waited for. The window is cleared to its background, which is not painted again, and an opaque square-cornered rectangle drawn first over the whole scene becomes the clear colour (`QmlView::set_fold_backdrop`). On a software rasteriser every full-window quad costs as much as the rest of the frame. Where the platform has EGL (Unix other than Apple's and Android) the painter is egui_glow on one EGL context shared by all windows, each with its own window surface and swap interval 1 (`gl.rs`). wgpu's GL backend draws every frame offscreen and copies it, flipped, into the window, which on a software rasteriser cost three times what the frame did (docs/BENCHMARKS.md). The GL painter also takes the scene's opaque square-cornered rectangles that lie on whole pixels as rectangles (`QmlView::set_solid_rects`; a hard-edged one is snapped to the pixels whose centres it holds) and fills each with a scissored clear, which a software rasteriser does at a fraction of what a blended mesh costs. The GL painter draws only where the picture changed: the last picture names the places a pass changes (each changed shape where it lay and where it lies, with the pixel its smoothed edge reaches), the painter adds what changed since the buffer it draws into was last shown (EGL's buffer age), clears and draws only those places, and tells the compositor the same (`swap_buffers_with_damage`). A clip that scrolled is not drawn again where its pixels are already on screen: on GL 3 and later the painter keeps the clip's pixels as the last frame drew them, moves them by the scroll with two framebuffer copies and draws only the strip that came into sight; `LJOS_SCROLL_COPIES=off` draws the clip again instead, and a desktop scenario compares the two pixel for pixel (`features/window/scroll-pixels.feature`, `crates/ljos_bdd/tests/scroll_copies.rs`). Places that overlap are joined, and past 256 they are drawn as one. A buffer of unknown age, a new scale or background draws the window whole. The drivers start on the `ljos-gpu` thread while the application is built, one start-up at a time: for GL that is EGL's display, and no wgpu instance is made. egui-wgpu draws where there is no EGL, where the first window cannot have it (the reason is logged), or when `LJOS_PAINTER=wgpu` asks for it. GL is the default because of what was measured (docs/BENCHMARKS.md): on llvmpipe a frame costs a third of what it does through wgpu, on a real GPU (a Radeon 8060S, through `LJOS_CUA_GPU`) the two cost the same, and GL shows the first frame in a tenth of the time, since wgpu starts Vulkan first. The shell tells the scene the display's refresh rate (`QmlView::set_refresh_rate`), so animations step at the display's rate, and a pass that comes less than half a frame before the scene's next wake runs it rather than losing the frame. Measured idle CPU over 10 s: 0 ticks for the QML demo and for `ljos run --watch`, 1 tick with `--watch --mcp --top`.
- winit cannot express modality or a transient parent, so both are emulated in the shell; most window flags are ignored, and Wayland does not allow a window to place or activate itself.
- A secondary window that is hidden runs no frames.
- A window asks the window system for a blurred backdrop with `backdrop: Window.BlurBackdrop`, and for the system's glass with `Window.GlassBackdrop` (Ljós's own; `Window.NoBackdrop` is the default), on opening and on each change (`WindowChange::Backdrop`). The backdrop is told the colour scheme in effect with it (`Appearance`: the application's `Qt.styleHints.colorScheme` when it set one, else the system's), again when that changes, so that a dark window gets the dark material. The host maps it per platform: on macOS a view of the system's material in the window's frame view, below winit's content view, sized with the window by AppKit: an `NSVisualEffectView` (behind-window blending, the under-window-background material) for the blur, and an `NSGlassEffectView` for glass where the class exists (macOS 26), the blur view otherwise; its `appearance` is aqua or dark aqua, or inherited when the scheme is unknown. On Wayland the KDE blur protocol (`org_kde_kwin_blur`) through winit's `set_blur`; on X11 the `_KDE_NET_WM_BLUR_BEHIND_REGION` property, set by a connection of the host's own (x11rb), since winit has none there; both for glass too, and with no light or dark of their own. On Windows `DWMWA_SYSTEMBACKDROP_TYPE`, the transient-window (acrylic) backdrop for a blur and for glass and the system's choice for none, with the window's dark mode (winit's `set_theme`) set to the scheme while there is a backdrop, which tints the acrylic. Elsewhere, and where the compositor has no blur, nothing changes. DECISION: one enum property and no read-back of what the compositor did, the smallest API that says it, and room for more kinds (a Windows Mica backdrop) as more values; the backdrop does not make the window see-through, its `color` does, as in Qt. The view goes in the frame view and not into the content view because the painter's metal layer is a sublayer of the content view's, and a subview's layer there could be put over it. No strength: none of these platforms has a public setting for it (winit's macOS blur sets a fixed radius through a private call, and that call is no longer used); a strength would be a promise no host keeps. The macOS and Windows parts have only been checked with clippy for `aarch64-apple-darwin` and `x86_64-pc-windows-gnu`, never run.
- A window without the system's frame (`Qt.FramelessWindowHint`) is resized from where a framed one is (#140). AppKit gives a borderless window a resize area only inside its frame, and a titled one an area that reaches onto its edge and a few points past it, so on macOS such a window stays titled and shows none of its title bar: transparent (`titlebarAppearsTransparent`), its title hidden, its close, minimize and zoom buttons hidden, and its content under the bar (`NSWindowStyleMaskFullSizeContentView`), so the content covers the whole window as a borderless one's does (`ljos_host`'s `TitleBar`, `appkit_frame`). winit's view is told it cannot move the window (`mouseDownCanMoveWindow` added to its class, as the drag methods are below), so a press in the top strip goes to the content and moves nothing. The window moves when the application asks with `startSystemMove`, which is AppKit's `performWindowDrag(with:)`. A change of the flags changes the same four things; the window is never made borderless, since winit's own change of decorations would do that. Such a window has AppKit's rounded corners and shadow, as a framed one has. Only a window that has a title bar and can be resized is asked whether it is maximized (`surface.rs`): for any other, winit puts that title bar alone on the window for the question, which would show the hidden bar for the moment. DECISION: this mechanism, chosen from Apple's documentation, over a margin of the shell's own around the content, because a window hears of no press outside itself and the area past the edge is the one that was missing. It is also the issue's workaround, done inside the host so applications need not do it. On Linux and Windows nothing changes: winit's decorations are off there as before. Like the other macOS parts it has been checked only with clippy for `aarch64-apple-darwin`, never run.
- The application is handed each platform window as it opens, before anything is drawn in it (`App::on_window`, on `Platform::on_native_window`): the QML `Window` it is for (`None` for the shell's window of a plain-item root) and a `NativeWindow`, whose handles are `raw-window-handle`'s (re-exported as `ljos::window::raw_window_handle`), for what only the platform's API does. A window opened again in another's place is handed over again. DECISION: the hook lives on `Platform`, not the shell, so a host of the application's own calls it too (`Platform::native_window_opened`) and a test does without a display. Seen in the off-screen sandbox (`desktop:cua:exec`) with the `native_window` example of the `ljos` crate, a root `Window` and a second one closed and shown again. Under XWayland the hook was handed three Xlib windows: the root, the second, and the second again under a new id. The X server reported each as not yet mapped, so the hook runs before the window is first shown, and the second already carried `_KDE_NET_WM_BLUR_BEHIND_REGION` for its `Window.BlurBackdrop`. On sway's Wayland it was handed three Wayland windows. Wayland cannot be asked whether a surface is shown, so that order rests on the code there. CI has no display server, so this is a recorded run and not a test.
- AccessKit's consumer (0.35 to 0.38) panicked when the window's focus changed with the focus in a tree it had not seen; the graft releases focus first (`release_graft_focus`). The consumer of egui 0.37 (0.39) takes it, and `tests/graft.rs` holds it to that; the release stays, at the cost of one update, for hosts on an older adapter.
- Nothing calls `RenderBackend::trim` yet (section 8.5).

### 7.4 Layout

Layout is Ljós's own implementation of QML semantics.
- **Anchors** are eager geometry bindings.
- **Positioners** (Row, Column, Grid, Flow) and **QtQuick.Layouts** run in the polish pass. QtQuick.Layouts covers fill, preferred/min/max sizes, alignment, spans and margins.
- The polish pass runs the deepest items first and items of one depth in creation order, so the same scene lays out the same way every run (two layouts sharing an item through `LayoutItemProxy` give it the size of the one that runs last).
- **Implicit sizes** propagate through Loader and Repeater, as Qt does (07 R3).
- **`LayoutMirroring`** is resolved at layout time. It flips anchors, positioner order and horizontal views, but never raw `x` (section 15).
- **Taffy** may be offered later as an opt-in `Ljos.Flex` item.

### 7.5 Text

Source: 05 §3.3.

**`TextEngine`**, one per app, contains:
- `parley::FontContext` (Fontique) and `LayoutContext`;
- an LRU cache of layouts keyed by `(text hash, style hash, width bucket, scale, locale)`;
- `GlyphAtlas`: skrifa outlines rasterised by vello_cpu into egui textures, at most 2048² (4096² on desktop), separate per scale, with an RGBA path for colour emoji. A full atlas first lets go of the glyphs the frame does not draw and grows only for those it does, so sizes no longer drawn (a scale animation fills one per frame) never make it grow.

As built: `ljos_quick` holds the engine in a thread-local, since an Engine is `!Send` and there is one per app. Items emit glyph runs, not pixels: R draws the outlines directly and the atlas lives in the B0 backend, the only one that needs textures. `Gradient` and `GradientStop` fill a Rectangle vertically or with `Gradient.Horizontal` on both backends (B0 by vertex colours, with extra vertices at the stops); presets by name are not there yet.

**As built: drawing by script and transforms (M2).** `Canvas` paints in its item's polish through a 2D context into pixels that the scene shows as an image; it paints when asked (`requestPaint`, `markDirty`, a change of size) and an idle canvas asks for no frame. Its `context` is undefined until the canvas is available, and a `contextType` of "2d" makes the context then, as Qt does. `Rotation`, `Scale`, `Translate` and `Matrix4x4` transform items, and `Flipable` shows its front or back by the side its transform faces. A view that is not drawn gives back what it can rebuild (`QmlView::trim`): image textures, the glyph atlas and decoded pictures nothing shows.

**Version pin.** Parley must share skrifa 0.44 and harfrust 0.12 with egui 0.37's epaint, so the font stack is not duplicated (it does; skrifa 0.46 comes only with hayro, and epaint's vello_cpu 0.2 sits beside the 0.3 that `ljos_render` and hayro use, as 0.1 did under egui 0.36). Report 05 pins Parley 0.11.1 for this. Report 02 says 0.12.0 also matches. The scaffold verifies with `cargo tree -d` and picks the newest version that does not duplicate. Because egui tracks the latest release (D9), each egui bump re-runs this check. A temporary duplicate is allowed but tracked in CI.

**Off-screen discipline**
- Hidden text keeps only its implicit size and metrics.
- Its `Layout` is freed after N frames off-screen.
- Clipped runs are never rasterised.

**Elide** is Ljós's own: a binary search over grapheme clusters, cached per width. HAL-C2 uses `elide` about 460 times.

**Language and fonts.** The content language (`Text.lang`, a Ljós extension that defaults to the UI language) drives font fallback and CJK unification.

**As built: a family that is not installed.** An engine with the machine's fonts follows every named family with the one the platform's font matching puts in its place, as Qt does: on Linux fontconfig's best match (`fc-match`), so Helvetica is Nimbus Sans where URW's fonts are installed, with that face's ascent and descent. Fontique substitutes generic families only. Other platforms are not asked yet. With the bundled font alone, a missing family is set in it.

**Rich text** uses a `RichDoc` of `Block`s: Paragraph, Heading, Quote, Code, List, Table, Rule and Image. Each block caches its hash, layout, y and height. There are three front ends:
- `MarkdownText`, using pulldown-cmark 0.13.4;
- `StyledText`, a strict tag subset;
- `RichText`, a tolerant subset tokenizer.

Raw HTML never passes through, because rich text often comes from untrusted sources such as the network.
- **Streaming.** `RichDoc::update` re-parses only from the last unfinished block.
- **Selection.** `SelectionScope` gives one document-order offset space across blocks and across `Text` items, including virtualised ones. It exposes `selectedText`, `positionAt` and `linkAt`.

**Editing.** `TextModel` is a string, or a rope for large text areas, with:
- an undo log that coalesces runs of typing;
- `Validator` and `InputMask`;
- `EchoMode`;
- the clipboard.

Caret and selection geometry come from Parley's `Cursor`/`Selection`. Parley's `PlainEditor` is a reference, not the owner of the model.

**As built: rich text (M2).** `Text.textFormat` chooses one of four front ends (`ljos_text::rich`), all filling the same flat `RichDoc`: plain, `StyledText`, `RichText` and Markdown (CommonMark with tables, strike-through and task lists). Nesting is flattened into each block's indent and margins. `AutoText` decides as Qt does.
- **Untrusted input.** Nothing in a document is run, fetched or included. Raw HTML in Markdown is shown as the text it is; `script`, `style` elements and unknown tags of `RichText` are dropped; of a `style` attribute only `color`, `background-color`, `text-decoration` and `text-decoration-color` are read. Each front end is fuzzed (`tests/rich_fuzz.rs`).
- **Streaming.** Appending to a text lays out only the blocks from the last unfinished one (`tests/rich_stream.rs`).
- **Links.** A link reports its address (`linkActivated`, `linkHovered`, `linkAt`) and the toolkit opens nothing. A `Text` takes the pointer only for a press that lands on a link. Links are reached with Tab and are nodes of the accessibility tree. A `RichText` or Markdown link is `#0000ff` whatever `linkColor` says, as Qt documents; `linkColor` applies to `StyledText`.
- **`SelectionScope`** (module `Ljos`) gives the `Text` items below it one selection in one space of UTF-16 offsets, a line break between members. Delegates a view has not made are not members.
- **Highlighting.** `TextEdit.textDocument` takes spans or a function that makes them (`highlight`, `clearHighlights`, ours); a span changes colours, background and underline, painted over the layout, and may ask for a weight, italics, strike-out or a family, which lays the text out again with them; never the text or the undo log. What an editor paints is read back as painting merged it: `textDocument.highlights()` in QML (ours) and `ljos::item::highlights` in Rust, which asks a highlighter that has not seen the text first. How a stretch of rich text is set reads through `cursorSelection` in QML, as in Qt, and for any stretch through `ljos::item::char_formats` in Rust. Both read what the paint path keeps, so they add nothing to a frame.
- **Editing (M2).** A `TextEdit` with rich text keeps a `RichText` beside its `TextModel` (`ljos_text::rich::editing`, `ljos_quick::text_input::rich`): every edit reaches the model first, and the new characters take their format the way Qt 6.11 gives it, as measured with qml6. Undo takes formats back too. `text` and `getFormattedText` write the document out as Qt's HTML or Markdown.
- **Not built:** inline images are decoded synchronously; block and table-cell backgrounds; links in a `Label` are not activated by the pointer.

### 7.6 Media (`ljos_media`)

`QtMultimedia` for playback: `MediaPlayer`, `AudioOutput`, `VideoOutput`, `Video`, `SoundEffect`, `MediaDevices` with `AudioDevice`, and `MediaMetaData`, to the Qt 6 documentation and as `qml6` behaves (`features/multimedia/`). Capture is left out: `Camera`, `CaptureSession`, `ImageCapture`, `MediaRecorder`, `AudioInput`, `ScreenCapture`, `WindowCapture`, `AudioBufferInput` and `AudioBufferOutput` (QML's `VideoFrameInput` too; a program hands in frames from Rust, below), as are subtitles and `pitchCompensation`.

**Crate.** `ljos_media` is a runtime crate: it depends on `ljos_engine`, `ljos_quick`, `ljos_render` (without egui), `ljos_platform` and `ljos_runtime`, never on `qml_syntax` or `qml_front`. Hosts register it beside `ljos_quick` and `ljos_controls` (the facade, the shell, `ljos` and the BDD engines). It has two cargo features, both on by default:
- `decode`: the demuxers and decoders (below). Without it the types still load and every source fails the way Qt's does for a format it cannot read: `MediaPlayer` reports `FormatError` and `InvalidMedia`, `SoundEffect` goes to `SoundEffect.Error`.
- `audio`: the audio device (cpal). Without it, without a device, or on a host that asks for it (a virtual clock: `ljos render`, the BDD runner), sound goes to a **null sink** and the media clock is the engine's clock. The scenarios run that way, with no sound card and no display.

**Decoding: three options weighed.**

| | 1. FilmCraft (`filmcraft-codecs`, `filmcraft-media`) | 2. symphonia + another video stack | 3. ffmpeg or GStreamer bindings |
|---|---|---|---|
| Formats | H.264, HEVC, VP8/9, AV1, ProRes; AAC, Opus, FLAC, the rest via symphonia; MP4/MOV, MKV/WebM, Ogg, MPEG-TS, WAV | audio only; video needs a second choice (openh264, dav1d: C) | everything |
| System libraries | none: pure Rust, `unsafe_code = forbid` | none for audio | libav* 8 / gstreamer 1.28, headers in CI, a build per Android ABI and iOS |
| Mobile | builds for any Rust target | audio yes, video depends | a heavy cross build, licence care (LGPL, codec patents) |
| Licences | MIT OR Apache-2.0; symphonia MPL-2.0, used unmodified | MPL-2.0 + the video stack's | LGPL/GPL by configuration |
| Weight | about 60 crates: rayon and image come with `filmcraft-media`, symphonia with `filmcraft-codecs`, and neither has a feature to drop them | small for audio | small Rust side, large native side |
| API fit | `MediaSource::video_frame(time)` and `audio(start, frames)`: a pull API by media time, which is what a player asks for; GOP-aware seeking | build it | a full pipeline that owns its threads and clock |

**Decision: option 1.** It gives a build with no system media library on every platform, which is also what CI and the headless sandbox need, and its pull API leaves the clock and the threads to us, so the rules below can hold. FilmCraft's crates are not on crates.io: they are a git submodule at `third_party/filmcraft` (pinned at `8309f21`, `shallow = true`), excluded from the workspace and named by path in `[workspace.dependencies]`; nothing inside it is modified, and what Ljós would want changed there is reported upstream. **A fresh clone needs `git submodule update --init --depth 1 third_party/filmcraft`** before it builds; CI does that step in every job that builds, and `--locked` holds because the path crates are in `Cargo.lock`. Upstream changes worth asking for: cargo features on `filmcraft-codecs` and `filmcraft-media` to leave out symphonia, `image` (stills) and rayon, which a player needs none of. ffmpeg or a platform decoder (MediaCodec, AVFoundation, Media Foundation) can come later as another `decode` backend behind the same core.

**Audio output: cpal 0.18.** ALSA on Linux (`libasound2-dev` on CI's apt lines, the only system library), CoreAudio on macOS and iOS, WASAPI on Windows, AAudio on Android. The device callback runs on cpal's thread and reads from a bounded ring the decoder fills; it touches no engine state.

**Threads and messages.** The Engine thread never reads a file or decodes:
- **Opening** reads the bytes (a registered resource, a local file on a thread of its own, or the platform's fetcher) and probes them on that thread; the answer (duration, tracks, metadata, or the error) comes back as a `Platform` reply, through the engine's bounded inbox (`UiHandle`).
- **Frames** are decoded by one worker thread per player, which takes requests from a bounded channel. The player asks for at most a few frames ahead of the playhead (preroll) and each answer is one reply. A request the playhead has already passed, or one from before a seek or a source change, is dropped by the worker, and its answer is ignored by generation if it already left.
- **An MP4 or MOV's video** is read in order by a reader of the player's own (`mp4_video`), not through FilmCraft's source: the source keeps a frame cache for an editor that scrubs back and forth (384 MiB a source, at least 64 frames, given back only past 1 GiB), which held about 420 MiB of a 1080p clip. The reader holds the pictures decoded ahead of the one asked for and the one last given, four at most at 1080p, and frees each as it is passed. It decodes on FilmCraft's two-thread background decoder and takes a decoder of every core at the next IDR picture when decoding takes more than half a frame's time on average. Other containers, and a track with a clean aperture, still go through the source. What both hold, with the frame each output shows, is the `media` owner in `app_metrics`' `memory`.
- **Sound** for `SoundEffect` is decoded once when the source loads; `MediaPlayer`'s is decoded in blocks ahead of the playhead into the ring.

**The media clock.** With a device, the device's count of played samples is the master and video follows it. Without one, the clock is the engine's (`Engine::now`), so a virtual clock makes playback deterministic. The clock is read, never ticked: the player arms a native engine deadline (`Engine::call_at`) for the next thing that must happen — the next frame's time, the next `position` report (every 50 ms, as Qt reports it), the end of the stream or of a `SoundEffect` loop — and nothing else wakes it. Play prerolls the first frames before the clock starts.

**Video on screen.** `VideoOutput` (and `Video`) paint through the render tree: `ljos_quick::register_paint_item` lets a crate's item type give the scene what it shows, and the frame is an image in the item's display list, sized by `fillMode` (the same rectangles `contentRect` and `sourceRect` report). Every frame goes through a `VideoSink`, the output's own (`videoSink`) or one the program made, which says its `videoSize` and fires `videoFrameChanged`; an output turns the picture by `orientation` in quarter turns (a half turn is the image's two mirrors, a quarter turn moves the pixels once per frame) before `mirrored`. A program's own frames (a camera, a renderer, a stream it decodes) come in through `ljos::media::VideoFrameInput` from any thread: RGBA, or 8-bit Y'CbCr 4:2:0 converted by the decoder's own conversion on the sending thread, posted through the platform inbox (`Platform::poster`) with the newest frame replacing one not yet taken, so one job a frame at most and none without a frame. The decoder converts YUV to RGBA on its own thread for now; uploading the planes and converting on the GPU waits for a YUV texture path in the B1 backend, and is the one place this differs from the pattern above. egui is never named here.

**Streams a program writes.** `ljos::media::MediaStream::open` makes a `MediaPlayer` (or a `Video`) play bytes the program writes from any thread, in place of its source, as `QMediaPlayer::setSourceDevice` does: MJPEG, H.264 with start codes or with lengths (and its `avcC` record), or a whole container. A decode thread of the stream's own cuts the bytes into pictures (JPEG markers; H.264 access units by their first slice, `ljos_media/src/units.rs`), decodes them with FilmCraft's decoders and converts them to RGBA, and posts at most one job at a time to the player. The player is `LoadingMedia` until the first picture, then `LoadedMedia` with no duration and not seekable. Paced, it shows every picture one frame period after the last while it plays, keeps four decoded ahead and makes the writer wait beyond that, so a paused player holds its writer; live, it shows the newest picture at once, drops any not yet shown and never holds the writer. The writer finishing, and the last picture's period passing, is `EndOfMedia`; a picture that cannot be decoded or converted is a `FormatError`; a new source lets the stream go and the writer's next write fails. A container is opened when its last byte comes, as a file is: FilmCraft's demuxers index the whole file when they open it. FilmCraft's H.264 decoder decodes on threads of its own and hands out a finished picture only when it is next called, so after a write the stream's thread asks it again with nothing to decode, at 1, 2, 4, … ms, for up to 250 ms, and then sleeps until bytes come; an H.264 picture is decoded once the first NAL unit of the next one is written (or the stream ends), since only that shows where it ends. A program that wants the pictures itself, rather than a player showing them, has the same decoder as `ljos::media::Decoder` (bytes in pieces of any size in, `VideoFrame`s out as they were decoded, on the calling thread, with `poll` for H.264's late pictures; `Decoder::live` for a source whose every write ends a picture, which is then decoded with that write; and a stream whose sequence parameters say it is shown in decoding order, `pic_order_cnt_type` 2 or a VUI `max_num_reorder_frames` of 0, has each picture handed out by the call that decodes it rather than held for the decoder's threads or its reorder window), and `VideoFrame::into_rgba` converts one with the conversion a player uses, on the thread that calls it, so a program that keeps only the newest of several pictures converts only that one; a picture that cannot be converted is an error there (and a `FormatError` for a stream), not a picture dropped. The stream promises the player's next answer as a platform answer (`Platform::expect`): the first thing it tells, and for a paced stream that plays, the next picture whenever the player has none left to show, kept until the decoder tells the player something. A host's loop never waits for one; a test that lets time pass (`Platform::finish`) does, as for a fetch.

**Sources on a web server.** A `MediaPlayer` source on the network is read through `Fetcher::stream` (`ljos_platform`), which hands the body over as it arrives, with no limit on its size or how long it takes (`HttpFetcher` times only the connection and the reply's head). A body the server calls `multipart/x-mixed-replace` is MJPEG from a camera and plays live; one that begins with a JPEG or an H.264 start code (or is called `video/h264`) plays paced at 25 pictures a second, as qml6 plays them; both go into a stream as above as they arrive, so the first picture shows before the last is sent, and a camera that never stops plays until the source changes, which closes the connection. Any other body is a container and is read whole before it opens: FilmCraft's demuxers read a file that is all there (`features/multimedia/media-network.feature`). `SoundEffect` fetches its source whole.

**0 % CPU when idle.** A paused or stopped player, a `SoundEffect` that is not playing and a player with no source have no deadline armed and no request in flight. An output that is not visible, or has no size, asks for no frames and uploads none, and the player paces its `position` alone; showing it again asks for the frame at the playhead. The scenarios check this through `positionChanged` counts and the engine's armed deadlines.

**Not built:** a container opened before all of it has arrived, and `bufferProgress` below 1, more than one audio track playing, `playbackRate` other than 1 with sound (the clock follows the rate, the samples are not stretched), frames as objects in QML (`videoFrameChanged` carries `undefined`, as qml6 shows it), hardware decoders.


### 7.7 Lottie (`ljos_lottie`)

`Qt.labs.lottieqt` 1.0 and its `LottieAnimation`, to the Qt 6 documentation, the Lottie format as `lottie-docs` describes it, and how `qml6` behaves (`features/lottie/`). It is an opt-in module crate written against the `ljos` facade alone, like `ljos_xml`. `ljos run`, `render` and `test` and the BDD runner register it. An application registers it with `ljos_lottie::register`.

**Behaviour, as Qt 6.11.2 shows it.**
- A local file is read when the component completes: `status` goes `Loading` then `Ready` before `Component.onCompleted` runs.
- Loading sets `frameRate`, `startFrame`, `endFrame` and the item's size from the file. `startFrame` and `endFrame` are read-only.
- One tick of `1000 / frameRate` ms moves one frame. The last frame stays up for one tick before the run ends, so 20 frames at 10 fps finish at 2.1 s.
- `play()` resumes without resetting the run count. `stop()` goes back to `startFrame` and does not send `finished`.
- `gotoAndPlay` and `gotoAndStop` clamp the frame, and with a string they look up a marker by name.
- A missing file gives `Error` and no warning. Bad JSON gives `Error` and a "JSON parse error" warning.
- The enum is `LottieAnimation.Infinite` (-1). The docs' `Infinity` does not exist on 6.11.2.

**Drawing.** DECISION: a Lottie model of our own (`model`, read with `serde_json`), whose frames become `DrawOp::Shape` paths with solid or gradient brushes and kurbo strokes in the display list QtQuick.Shapes and SVG already use. Every backend draws it, and the inspector and render tests see it.
- dotlottie-rs was weighed and set aside. It wraps ThorVG (C++), comes only from git, and would put a C++ build into every target, Android and iOS included.
- velato was weighed and set aside. It draws into a vello `Scene`, which needs the wgpu GPU renderer and goes around the display list, so B0 and the CPU renderer could not show it.
- The whole crate is about 2,600 lines of Rust with its tests, and covers what most files use.

**Covered:**
- shape layers, precomps (with `tm` time remap, `st` and `sr`), solid layers, null layers, and parenting;
- groups with their own transforms (anchor, position joined or split, scale, rotation, opacity, skew);
- rect (rounded), ellipse, path, star and polygon;
- fill (non-zero or even-odd), stroke (caps, joins, miter, dashes), and linear and radial gradients with opacity stops and a highlight;
- trim paths, simultaneous and sequential;
- keyframes with per-component bezier easing, holds, the `e` values of older files, and curved motion paths;
- layer in and out points;
- markers.

DECISION: a hold keyframe holds, as the format says. Qt 6.11.2 does not hold it, so that scenario is `@qt-diverges`. A layer's out point is exclusive, and the composition's last frame is drawn just before `op`. `quality` is kept but changes nothing. Strokes and radial gradients under a non-uniform scale use the mean scale.

**Not built yet:**
- masks and track mattes (a matte's source layer is hidden, and a warning says so);
- image and text layers;
- expressions;
- repeaters, merge paths, round corners, and star roundness;
- the exact sign of skew;
- clipping a precomp to its size, and clipping to the canvas;
- 3D layers and auto-orient;
- elliptical radial gradients and non-uniform stroke scaling;
- `quality`.

**Runtime.**
- The clock is pure (`clock::Clock`) and counts whole ticks from an anchor time.
- While the animation plays and can be seen (an area, visible, not fully transparent up the visual parents), one deadline is armed for each frame and the item repaints through `item::repaint`.
- Hidden, a finite animation keeps a single deadline, at the moment it finishes, so `finished` comes on time. An endless one keeps none and catches up when shown.
- Paused, stopped, finished or not loaded, nothing is armed.
- A file over 256 KiB, a remote file and a resource are parsed on the compute pool (`ljos::task::spawn`). The answer comes back through the engine's message path, and a generation counter drops the answer to an older `source`.


### 7.8 3D (`ljos_3d`, `ljos_quick3d`)

QtQuick3D, to the Qt 6 documentation and how `qml6` behaves (`features/quick3d/`). One wgpu renderer draws the scene natively and on the web. The design, milestones and performance budget are in `docs/quick3d.md`.

**Crates.**
- `ljos_3d` is the scene and its renderer, and knows neither egui nor the engine. The scene is a slotmap of nodes (groups, models, cameras, lights) with local transforms, plus the scene environment. Every change bumps the scene's revision. Its `gpu` feature is the renderer: it draws the scene into a texture of its own and blits that texture into a pass the host gives it. Without `gpu` the crate is the scene alone and links no wgpu. It builds for `wasm32-unknown-unknown`, with WebGPU and WebGL2 there.
- `ljos_quick3d` holds the QML types of the module `QtQuick3D`: `View3D`, `Node`, `Model`, the cameras, the lights, the materials and `SceneEnvironment`. They are written against the `ljos` facade, like `ljos_lottie`, and a host registers them with `ljos_quick3d::register`. They depend on `ljos_3d` without `gpu`: a type writes the scene and never touches a device. A `View3D` is a paint item whose tracked `read` walks the nodes under it into a scene; a walk that comes out the same keeps the scene it had, so its picture is not drawn again. `QtQuick3D` no longer stands in as empty items; its add-on modules (`Helpers`, `Effects`, `Particles3D`, `AssetUtils`, `Physics`, `SpatialAudio`, `Xr`) still do. An application that imports `QtQuick3D` without registering the crate is told the module is not installed, as with any opt-in module.
- `ljos_egui` (feature `quick3d`) is the adapter for egui-wgpu. `ljos_shell` turns it on.

**The seam.** `View3D` paints a `DrawOp` that carries its scene (an `Arc`, the same while the revision is) and a key for the view. Each renderer handles it its own way:
- **The wgpu painter.** B0 turns the op into a `PaintCallback` (`ljos_egui::scene3d::Scene3dPaint`). In `prepare` it renders the scene into the view's target, on egui-wgpu's device and encoder. In `paint` it blits the target in egui's pass, in paint order, clipped to the item. 2D items above and below the view compose as they would around an image. Each view keeps its own target and uniforms, so two views in one window do not share them.
- **The GL painter.** GL stays the default painter where EGL is: egui-wgpu cost an animating window up to 2.75 times the processor time (docs/quick3d.md, "The painter"). So a window that shows a `View3D` keeps it, and does not open a second device. `ljos_shell` hands wgpu's GL backend the painter's own context (`wgpu::hal::gles::Adapter::new_external`) the first time a view is drawn. Each frame, before egui's meshes, every view whose scene or size changed is drawn into a picture of its own, and its callback becomes a mesh that samples that picture, so it is clipped, blended and drawn in part like any mesh. wgpu gets back the state it keeps from when its device was opened (its vertex array, the pixel store) before it works, and the painter gets its own back after. The picture of a view no longer shown is let go of at the next paint. A context wgpu cannot use (older than GL 3.3 or GLES 3) draws no 3D and says so once.
- **R, the processor's painter, snapshots and tests.** These have no wgpu device in their pass. A 3D view draws the scene off-screen with `ljos_3d`'s own device, reads it back and paints it as an image; the picture of a view no longer drawn is let go of at the end of the frame.
- **The web.** `ljos_3d::SurfaceView` draws into a surface of its own, for a host that has no egui. On the web that is a `<canvas>`, with WebGPU where the browser has it and WebGL2 where it does not.

**Idle.** A target remembers the revision it shows, and an unchanged scene at an unchanged size is not drawn again: the blit costs one textured triangle, and on the GL painter a frame that changes nothing in the view leaves it out of the areas drawn. Nothing polls. A property change on a 3D type bumps the revision and marks the `View3D`'s node `CONTENT`-dirty, which schedules a repaint as any other item does. A running animation on a 3D property asks for frames through `FrameScheduler` like any animation, and when it stops, the frames stop.

**Colour.** The scene is lit in linear space and written as premultiplied sRGB into an `Rgba8Unorm` target, so the blit copies it into egui's premultiplied gamma-space pass without converting it again (section 7.1, colour).

---

## 8. Off-screen and resource policy

Source: 04 §3.3–3.7.

### 8.1 Frame scheduling

A single `FrameScheduler` computes the next wake time:

    next_wake = min(animation deadlines, timers, request_repaint_after, messages)

- **When idle:** 0 wakeups per second, on every thread of the process. As built: the session-bus connections (screen reader, settings portal, badge, accessibility bridge) are opened on `blocking`'s thread pool, held at 1.6.1, whose idle threads end; 1.7.0 keeps one parked with a half-second timeout for good, which woke every application twice a second (`crates/ljos_shell/tests/idle_wakes.rs`). What still wakes an idle application is a message: with AT-SPI on, its registry pings each application on the accessibility bus every 20 to 30 seconds, and the bridge answers. `ljos bench` counts the wakes of its idle phase by thread name (`woken`).
- **Cursor blink:** at most 2 wakeups per second, and none while the window is unfocused.
- **Timer coalescing:** timers are coalesced to 1 ms granularity, with up to 4 ms slack for timers that are not animations.
- **Animation timing:** animations follow vsync, using Choreographer timestamps on Android, never wall-clock sleeps.
- **CPU target:** at most 50 % of the frame budget on a mid-range phone, which is about 8 ms at 60 Hz and 4 ms at 120 Hz.
- **Frame-rate policy:** `Window.frameRatePolicy` is `Auto`, `Max` or `Fixed(n)`, with thermal caps (04 §3.11).

### 8.2 Visibility lattice

Each node has one visibility state, computed top-down and propagated only when something changes:

`Active` › `VisibleUnfocused` › `Hidden` › `Suspended` › `Frozen`

| Policy | Default | Effect |
|---|---|---|
| Paint and layout culling | always on | Nodes outside the viewport or clip, or with `visible: false`, are not painted. The test is per container: Flickable knows its content window. |
| App-state suspension | always on | When the app is Hidden or Suspended, the frame scheduler stops, animation clocks stop and resume in logical time, and the surface is released. |
| `visible: false` | Qt semantics | Bindings, timers and Behaviors keep running. |
| `Timer.suspendWhen` | `Never` (opt-in) | `Never`, `Hidden` or `AppInactive`. A suspended timer does not fire and does not accumulate missed ticks. |
| Scene animations on hidden items | structural | A purely visual animation jumps to its end state on time. An animation with observable effects (`onFinished`, `ScriptAction`, or a value read elsewhere) catches up analytically without painting. |
| `Frozen` | opt-in; automatic for covered `StackView` pages and `SwipeView` pages beyond ±1, when static analysis allows | See 8.3. |
| Dehydrate | after 30 s frozen *and* memory pressure | Releases textures, glyph runs and layouts but keeps the objects. Under further pressure, only `Loader`-owned content unloads, preserving `Ljos.Persist` state. |

`Ljos.Lifecycle` (`state`, `onStarted`, `onStopped`) lets sources such as WebSocket subscriptions follow the lifecycle.

### 8.3 Freezing rules

Freezing stops *work*, not *meaning* (04 §3.5).
- **Bindings.** Inside a frozen subtree, bindings are only marked dirty. They evaluate when read or on thaw.
- **Change signals** for frozen subtrees are *coalesced*: one per property on thaw. This is a documented divergence.
- **Exemptions.** These subtrees are never frozen: bindings with side effects, `Behavior` and `Binding{when}` targets, `Connections` that reach out of the subtree, and items whose implicit size feeds a visible layout.
- **Where freezing is allowed.** The analysis is static and keyed on the parent type. StackView, SwipeView, Loader and ListView delegates qualify; a plain `ColumnLayout` child does not.
- **Frozen subtrees** are hidden from accessibility (unless they are the active route) and are not interactive.
- **Transitions and predictive back** may keep at most two cached page snapshots.
- **Property test:** with and without freezing, observers see the same final values and the same set of signals (presence, not count).
- **As built (M2).** `Engine::set_frozen` freezes an object and what it owns: effects are parked in the graph and run once on thaw against the latest values, while bindings still answer reads; a timer that came due fires once on thaw; animations stand still and catch up. `StackView` freezes the pages it covers, but not a page that owns one it shows (a page declared inside another and pushed over it, `features/controls/stack-view-owned-page.feature`), and `SwipeView` freezes its non-current pages while it rests (all pages thaw while it moves, and a page taken out thaws). The static analysis of where freezing is allowed, the exemptions, Loader and delegates are not built.

### 8.4 Virtualised views

- **ListView and GridView** create delegates for the viewport plus `cacheBuffer`. The default `cacheBuffer` is one screen on each side on a phone and two on desktop. The delegate count is not exact in Qt either; it measured 11–12 for the case below (09 §2.4).
- **Incubation budget:** about 3 ms per frame, set by `Ljos.ListView.incubationBudgetMs`. During flings, placeholders sized from estimated heights are preferred over dropped frames.
- **`reuseItems` keeps Qt's default of `false`**, which is a compatibility decision. Pooled delegates have their effect roots suspended. A lint suggests `reuseItems: true` where a delegate has no creation side effects. An app-wide default can be switched on.
- **Variable heights** are remembered by model key. Scroll anchors are a *model key plus offset*, so prepending rows (loading chat history) does not move the view (07 R5). This needs `rowsAboutToBeInserted`.
- **While a screen reader runs,** `cacheBuffer` grows to ±20 rows on desktop and ±10 on mobile (section 14, "Screen reader or listener"). Another assistive technology, such as a test driver, does not grow it.
- **`Repeater` is not virtualised**, which is Qt semantics. A lint warns above 50 items.

**As built (M2).** `ListView` and `GridView` are a `Flickable` whose content is the rows of a model (`ljos_quick::views`). The engine's `ViewDelegates` keeps one delegate for each row the view asks for and keeps it in step with the model; the view decides which rows those are.
- **The pass** starts from the anchor (the first row in sight after the last pass, and where it was), lays rows out in both directions until the window around what is shown is full, and gives back every delegate outside it. `cacheBuffer` defaults to 320 px, as on Qt. Rows never measured count as long as the average of those that were, so `contentHeight` of a long list is an estimate that improves as the view moves, as it is on Qt. A row is measured once a polish has run since it was made and none is running. As on Qt, each row is measured before the next is made: a new row whose length is still to come from what is in it (a text not laid out yet) stops the pass, the scene lays it out in the next polish, and the view fills on from it then. Those polishes do not count towards the scene's loop limit. So rows of a text over a picture are not all taken to be as short as the first row or the picture was, and made only to be given back (`features/views/list-rows-measured.feature`, `features/views/list-rows-let-go.feature`). Rows of a view whose rows show themselves (an `ObjectModel`) and rows measured before are not waited for; where the rows measured so far differ in length, a row ahead still being made by the incubator stops the pass too, and its being ready fills on from it. Rows made for the `cacheBuffer` read `visible: true` but are not drawn while they lie wholly outside what is shown and the display margins, clipped view or not; a row that is partly inside or touches the edge is drawn whole, as on qml6 (`features/views/list-cache-rows-not-drawn.feature`).
- **Changes.** Rows that arrive before the anchor push its number up and leave its place alone: the content grows upwards (the origin moves) and nothing in sight jumps. The anchor is a row and an offset, not yet a model key.
- **Also built:** sections, header and footer, `currentIndex` with keys and the highlight (a `ListView`, `GridView` or `PathView` is a focus scope whose current item gets `focus` each time it becomes current, as on qml6), `positionViewAtIndex` and its siblings, `indexAt` and `itemAt`, snap modes, `reuseItems` with `pooled` and `reused`, `ObjectModel` and `DelegateModel`. `Menu`, `ComboBox` and the other controls with rows use a real `ListView`.
- **Models and transitions as built (M2).** `DelegateModel` groups (`items`, `persistedItems`, custom `DelegateModelGroup`s, `filterOnGroup`, group methods and the attached index/groups data), and `move`/`displaced` (`addDisplaced`, `removeDisplaced`, `moveDisplaced`) view transitions with `ViewTransition` data. `DelegateChooser`/`DelegateChoice` pick a delegate per row by role (QtQml.Models and Qt.labs.qmlmodels), and `SortFilterProxyModel` sorts and filters a source model with role/value filters and role/string sorters, translating source changes into row notifications so delegates survive them. `Instantiator` is in QtQml.Models as well as QtQml. `ItemSelectionModel` keeps which items of a model are selected and which is current, with Qt's selection flags (`Rows`/`Columns` included) and its signals; a model index is a frozen script object with Qt's members, which `ListModel.index(row, column)` makes, and the selection keeps each item as its path from the top (row and column under each parent), so a tree view's indexes fit it as a list's do. The selection is ranges as Qt keeps them (never coalesced; a range split by a deselection or an insertion stays split), with a command carrying `Current` changing the selection being made, which the next command without it commits; `select()` takes an index, a range or a whole `selection`. It follows its model: inserted rows split or move the ranges and the current index, removed rows leave the selection (told as deselected) and hand the current item to the row above, else below, else the parent, moved rows take both along untold, and a reset forgets both. `PropertyMap` is native-only in Qt (no QML type), so it is a Rust type (`ljos_engine::PropertyMap`) whose keys are per-key reactive properties; a QML read or write does not yet create a key the way Qt's C++ map does.
- **Not built:** `highlightRangeMode` and `preferredHighlightBegin`/`End`; `section.labelPositioning`; `DelegateModel` `parts` and `rootIndex`, and the per-group attached properties Qt names after each group; the `Ljos.ListView.incubationBudgetMs` setting; `SortFilterProxyModel`'s function filters/sorters (Qt 6.11.2 itself crashes on the documented example, so there is no oracle), its non-dynamic mode, and column/tree filtering. Only `ListModel` and `TreeModel` have `index()`, and a `ListModel.move` inside a tree level is not followed by the selection (trees tell no moves).
- **Trees as built.** A tree model is an `ItemTree` (`ljos_engine`): rows under rows, each named by its path, with columns and roles, and changes told per parent. `TreeModel`, `TableModel` (whose rows have none under them) and `TableModelColumn` (Qt.labs.qmlmodels) make one from script rows, and `Engine::set_object_tree` makes a native object one; as an `ItemModel` a tree is its top level, so a list view of it shows the first level. `TableView` (QtQuick) shows a tree's top level in its columns, and any other model as one column whose delegates read `modelData` and `index` as a list view's do; `TreeView` is a table view of the rows in sight: those at the root and under an expanded row (`ljos_quick::views::table`). A table view is a focus scope, as on qml6, so an item of a delegate keeps its focus when the view gets it back. The rows in sight are merged by path when a row is expanded or collapsed or the tree changes, so the delegates of rows that stay are kept, and `ViewDelegates` keeps one delegate per cell in sight: the cells of the rows in sight that are in the columns in sight. Columns are as wide as `columnWidthProvider` says (0 hides one), or `setColumnWidth`, or their widest delegate in sight, and one never measured counts as wide as the average of the others; rows as tall as `rowHeightProvider` says, or `setRowHeight`, or their tallest; a view synced to another keeps its explicit sizes in the synced directions there, so a header and the view it heads share them (`views/table/resize.rs`). With `resizableColumns` or `resizableRows` (a header's are set across or down) a press within 4 of the end of a column or row, or in the spacing after it, is the view's before its cells', and dragging resizes it from where the drag began, to no less than 0.001; over such an end the pointer shows `Qt.SplitHCursor` (a column's) or `Qt.SplitVCursor`, which the scene tells by `QuickScene::cursor_shape` and `ljos_egui` shows. `moveColumn` and `moveRow` show a column or row at another place of the view, the model keeping its order (`views/table/cells.rs`): a cell is the view's, `itemAtCell`, `index` and `cellAtIndex` mapping between it and the model's, a delegate's `row` and `column` are the model's, sizes stay with the places, and `columnMoved` and `rowMoved` tell each one that moved; a change in the number of columns or rows puts the model's order back, since qml6 aborts there. `reuseItems` pools what leaves. A delegate's `TableView.view` is its view, and its attached object is told `pooled` and `reused`. The view writes `depth`, `expanded`, `hasChildren`, `isTreeNode`, `current`, `selected`, `editing` and `treeView` on a tree's delegates that have them, and `current`, `selected` and `editing` on a table's that declare them `required`, and a delegate keeps `row`, `model` and the roles in its context even with required properties, as on Qt. With a `selectionModel`, the keys and the pointer move its current index as qml6 does (`views/table/nav.rs`): Up and Down by a row, Page Up and Page Down by the rows in sight less one, Right expanding and Left collapsing a tree's current row, Home and End to the first and the last column, Tab and Backtab to the next and the previous cell across rows; Shift selects (never with Backtab) from where it began, as `selectionBehavior` and `selectionMode` say, and a move without it, or a tap, takes the selection away. A tap makes a cell current, on the view or through a delegate's `clicked`; a double tap expands or collapses a tree's row. Editing is as on qml6 (`views/table/edit.rs`): `edit(index)`, or a tap or key as `editTriggers` say, makes the delegate's `TableView.editDelegate` a child of the cell's delegate, which rises to z 2 and is told `editing`, and gives it the focus; the view takes Return, Enter, Escape, Tab and Backtab from the editor first (a key filter while it is open), Return and Enter sending the editor's attached `commit` before it closes, Tab and Backtab committing and editing the next or previous cell; `closeEditor()`, a new model and the delegate going close it without a commit, and a tap outside it commits it. The edited cell's delegate is never pooled: when the cell leaves sight it is kept out of the way, editor, text and focus with it, `itemAtCell` has none for the cell, and it takes the cell again when it is back; closed out of sight, it is destroyed, and Tab or Backtab there move the current cell as on the view, the editor staying open. A row that ends where the content in sight begins is not in sight. A view with a `syncView` lays out by it in the directions `syncDirection` gives (both unless told): across, the other's column widths, column spacing and content width stand for its own, down, its row heights, row spacing and content height, and the two share `contentX` or `contentY`, whichever moved last moving the other. A cell whose delegate has no implicit width or height counts as 50 wide or tall, and a view with no room for a cell is 50 by 50 with `topRow` and `leftColumn` -1, as on qml6. `HorizontalHeaderView` and `VerticalHeaderView` (QtQuick.Templates, with a Basic look; `ljos_controls::header_view`) are table views synced across or down to the view they head; with no model of their own they show a proxy of its model, a section per column or per row, every role of which is its number from one, and `textRole` (`display` once synced) names the role a section shows. `TreeViewDelegate`, `TableViewDelegate` and `HeaderViewDelegate` (QtQuick.Templates, with a Basic look) are the stock delegates, and the view gives `tableView` and `headerView` to a delegate that declares them `required`; what the view gives it is `required`, as in Qt. `SelectionRectangle` (QtQuick.Templates, with a Basic look; `ljos_controls::selection_rectangle`) selects in its target through the view (`ljos_quick::CellSelector`, `views/table/select.rs`): by drag the press is the view's and does not flick it, and a drag selects the block from the cell pressed to the cell under the pointer, making that one current; by press and hold a long press selects the cell pressed; `Auto` is press and hold on a view that flicks. Its handles are made from its components in the view's content at z 100, centred on the block's corners, and dragging one moves its corner; the rectangle is inactive and its handles hidden once the selection model has nothing selected. While a drag is beyond the view's edge a timer scrolls the view that way, 6.6 points a second for each point past the first 2 as on qml6, and the block reaches to the cell at that edge; the timer stops when the content ends there, the pointer is back or the drag ends. Not built: Shift or Control with the pointer, a selection's block across reordered columns (it takes the model's columns between its ends), and dragging a header's sections to move them (`movableColumns` and `movableRows` are kept without effect).
- **Trees read lazily.** A tree model may read its rows only when a view wants them, as a file system or a remote listing does. `ItemTree::has_children` says a row has rows under it before they are read (by default, that it has rows now), and `TreeView` shows its expand arrow (`hasChildren`) from it. When a row is expanded (by `expand`, `expandRecursively` or `expandToIndex`) and `ItemTree::can_fetch_more` says its rows are not read yet, the view calls `ItemTree::fetch_more` with the Engine; the model reads them, now or later, and they arrive as `TreeChange::RowsInserted`. `ItemModel` has the same pair for the rows past the last, and the top level of a tree passes it on. A `ListView`, `GridView` or `TableView` whose pass gives the last row a delegate (or that has no rows) asks `can_fetch_more` and, if so, calls `fetch_more` from a due-now timer after the pass: rows a model inserts at once never change under a pass, and however many passes reach the end before the call runs, the model is asked once.
- **`JsonListModel` (M2, module `Ljos.Models`, crate `ljos_jsonmodel`)** is a read-only list over a JSON array of objects, from the text of `json` or the file or URL of `source` (which wins), read on the worker pool. Roles are the union of the keys in first-seen order; a key a row lacks reads `undefined`, a JSON `null` reads `null`, and `get(i)` gives the row as an object. `count`, `roles`, `status` (`Null`, `Ready`, `Loading`, `Error`) and `error` say where it stands, and a document that cannot be used keeps the rows and logs `QML JsonListModel: <name>:<line>:<column>: <why>`. New data is compared with the old: the equal rows at both ends are left alone; in between, a row at the same index gets `dataChanged` with only the roles that differ, adjacent rows with the same roles sharing one signal; rows left over at the end of that middle are inserted or removed. Moves are not detected: a row that moved is a change at each index it covers. A new key is added to the roles; a key no row has any more is a `modelReset`, because a model's roles never shrink under a view. Documents are limited to 64 MiB. Qt has no such type, so the module is named as Ljós names its own (`Ljos.Blocks`, `Ljos.Design`) and its scenarios are `@ljos-only`.
- **`PathView` (M2)** lays its delegates along a `Path`, with `pathItemCount`, the preferred highlight range, snapping, a drag that flicks and `PathAttribute` values.

### 8.5 Memory budgets

These are initial values, enforced in CI by benchmarks (04 §3.7).

| Resource | Phone | Desktop |
|---|---|---|
| Decoded images (LRU, at `sourceSize` or layout size × DPR) | min(64 MiB, 6 % of usable memory) | min(256 MiB, ⅛ of usable memory) |
| GPU caches (layers, paths) | 24 MB | 64 MB |
| Glyph atlas | 2048² A8 | up to 4096² |
| JS heap | `set_memory_limit(96 MiB)`, GC threshold 8 MiB | same; limit configurable |
| JS time per dispatch | 4 ms soft (log), 100 ms hard (interrupt); a debug build allows 8 times both | same |
| Process | RSS ≤ 150 MiB, GPU ≤ 64 MiB, cold start ≤ 500 ms | — |

Usable memory is the machine's (`MemTotal`), or the lowest `memory.max` of the cgroup the process is in and the groups above it when that is less: an application in a container or a systemd unit with a limit sizes its caches to the limit, not to the machine.

What is freed goes back to the system. On Linux with glibc, `ljos_shell::run` fixes the allocator's mmap threshold at 1 MiB, so a picture, or the scratch it is decoded in, is mapped on its own and unmapped when dropped. By default glibc raises the threshold each time such a buffer is freed, and the buffers then come from per-thread heaps that keep the memory. On rssnews that held a quarter of a gigabyte for pictures long gone.

**Memory pressure**
- Caches implement `trait Evictable { fn evict(&mut self, level: MemoryPressure) -> usize }`.
- `MemoryPressure` maps from Android trim levels, iOS memory warnings and suspension.
- The order of eviction:
  1. frozen pages' textures;
  2. unused cache entries;
  3. content near the visible area.
- Under heavy pressure the JS garbage collector runs.

---

## 9. Runtime: scheduler and supervision

Source: 03 §3.

### 9.1 Lanes and budgets

`Lane::{Input, Critical, Visible, Prefetch, Deferred, Idle}` are served in that priority order.

- **Adaptive budget.** The frame budget comes from the refresh period, minus measured render cost.
- **Jobs** implement `Job { lane, owner: ProcId, poll(&mut JobCx) -> Done | Yield | Failed(Fault) }`.
- **`FrameReport`** records time per lane and the top owners. The `top` overlay displays it.

### 9.2 JS host

- **Entry points.** `JsHost::enter(slice, f)` sets the interrupt deadline and then enters the context. `pump_jobs(until)` drains microtasks. `collect_if_idle` runs GC in the Idle lane. The desktop shell runs it once a rest of 100 ms has followed a burst of work, and only when `wants_idle_collect` says the heap grew by the configured growth since the last collection. A loop whose scripts made nothing to collect arms no deadline for it. A wake that is not work, such as an agent's question or a timer the system fires slightly early, leaves an armed rest where it was and does not restart it. `app_processes` lists the armed deadline as the `collector` waker.
- **Interrupt granularity** is about every 10k opcodes, roughly 266 µs. Native calls that run long (a huge `JSON.parse`, a regex) are not interruptible. They are chunked or moved to the compute pool.
- **Nested runs.** JS that a callback runs inside a dispatch (a binding a script reads, a handler a write sets off, the `Component.onCompleted` of an object a script makes) has a hard limit of its own, and the time it took is given back to the runs around it. Native work under a script (`Qt.createComponent`, `Qt.createQmlObject`, `createObject`, `incubateObject`, and any object made because of something it wrote, such as a Loader's item) is given back to all of them (`JsHost::native`), so a slow compile does not use up the slice of the script that waits for it. Time the thread did not have the processor is excused from every limit, so a loaded machine does not cost an application its UI: the limits read the wall clock, which is cheap, and at the first poll of a dispatch and whenever a limit looks passed they compare it with the thread's own clock (`CLOCK_THREAD_CPUTIME_ID`, `GetThreadTimes`; a target with neither counts all the time that passes). Two limits get nothing back for nested runs: a dispatch's JavaScript as a whole may take 4 hard limits, and its time with native work included, 50. When the interrupt lands in a nested run, the dispatch is over: every callback throws an uncatchable `interrupted` as it returns to the script. Otherwise the poll can keep landing in the nested runs while the script around them never stops. The warning names the dispatch's code by file, line and handler, the nested run the interrupt came in, and the native time given back. The limits read the system's clocks or one a test sets (`JsHost::set_clock`, `Engine::set_js_clock`).
- **Prototypes.** One prototype per QML type, with `Accessor` getters and setters that call `Graph::read`/`write`. This keeps QuickJS shapes stable.
- **`#[exotic]` objects** are used only for genuinely dynamic objects: model role objects, `ListModel` element proxies and context-property fallbacks.
- **Wrappers** are created lazily and cached as `Persistent`. A JS-owned object is destroyed by its finaliser.

### 9.3 Faults

`Fault::{JsException, JsInterrupted, JsOutOfMemory, Panic, Startup}`.

- **Panics.** Rust panics in handlers are caught at the turn boundary. So are panics in what a host runs for the application on the Engine thread: each `on_message` message and each platform job (a task's `done`, a fetch's callback, any `Reply::send`) is a turn of its own, so one that panics is a fault and the rest of its batch still runs. A task whose work panics on the pool answers its `done` with `Err(TaskPanic)` and is reported as a fault too. Mobile release builds keep `panic = "unwind"` unless decision D11 says otherwise.
- **Exceptions.** A JS exception in a handler is logged with its QML file, line and stack. The exception does not stop other handlers.

### 9.4 Supervisor

- **Policies:** `Strategy::{OneForOne, OneForAll, RestForOne}`, `Restart::{Permanent, Transient, Temporary}` and `Intensity { max_restarts, within }`.
- **Child kinds:**
  - UI boundaries: Loader, StackView page, Window, and `ErrorBoundary { fallback: Component }`, a Ljós extension. They restart from their `ComponentDef`, using a snapshot if there is one.
  - tokio tasks, managed with `JoinSet` and `CancellationToken`.
  - Pinned plugin threads, watched by heartbeat.
- **Tree:**
  ```
  AppSup ─┬─ UiEngineSup ── boundaries
          ├─ ServiceSup (one_for_one, 5 restarts / 30 s)
          ├─ PluginSup  (one_for_one, transient, 3 / 60 s)
          └─ DevSup: watcher → compiler → patch server
  ```

**As built (M1).**
- **One place.** Every fault comes through `Engine::report_fault`, is logged with its QML file and line (and stack for a JS exception), kept in a history of 32, and queued. Panics are caught around each native method, signal handler, completion hook and JS entry, so the later handlers of the same signal still run.
- **Delivery is between turns,** at the start of the next frame (`Engine::deliver_faults`), never where the fault was caught: the code that failed may be on the stack of the object a boundary is about to discard.
- **Boundaries.** `ErrorBoundary { fallback: Component }` lives in the QML module `Ljos` (`import Ljos`). A fault below it destroys its content and shows the fallback; it exposes the fault (kind, message, file, line) and `retry()`. A Loader is a boundary too and shows `Loader.Error`. After a script was interrupted it loads again by itself, once in 10 seconds, because what stopped the script may not happen again; the log has the fault and what became of the item. When restarts exceed the boundary's intensity the fault goes to the boundary around it. A JS exception is a boundary's fault only when it asks (`catchExceptions: true`); otherwise it is logged, as in Qt.
- **No boundary.** The fault reaches the application (`Engine::application_fault`, `set_fault_handler`) and nothing is destroyed. A development build shows it in the error overlay until the next reload that applies.
- **Supervisor tree.** `ljos_runtime::supervisor` has the strategies, restart kinds and intensity on the engine's `Clock`. UI boundaries are supervisors created at their first fault; tools list every boundary from when it is made without making one. tokio tasks (`JoinSet`, `CancellationToken`) are behind the crate's `tasks` feature, as a tree of their own that its owner drives; escalation from it to the host is not wired yet. Plugin threads and `DevSup` are not built.

kameo may be used inside the service layer later. ractor and actix are rejected.

---

## 10. Platform host and mobile

Source: 04 §3.1–3.2, §3.8–3.10.

### 10.1 `PlatformHost`

`PlatformHost` takes events in and reports capabilities out.

- **Implementations:**
  - `WinitHost` for desktop and iOS, on winit 0.30.13, then 0.31.
  - `AndroidHost` on android-activity 0.6.1 with GameActivity.
- **Why Ljós owns the host.** Through eframe/winit, Android loses `Start`, `Stop`, `SaveState`, insets and real IME events.
- **Events:**
  - `SurfaceCreated`/`Destroyed`, `Resized{px, scale}`, `Insets{system_bars, cutout, ime}`;
  - `Focus`, `AppState{Active, Inactive, Hidden, Suspended}`, `MemoryPressure`;
  - `Back{phase, progress}`, `Text{composing, selection}`;
  - `Touch`/`Pointer`/`Key`, `OpenUrl`, `SaveState`.

AccessKit on Android requires GameActivity. With native-activity it fails to compile (09 §2.3).

**As built: system services (M2).** `ljos_platform` puts each service of the system behind a trait with an in-process stand-in that tests use: the clipboard (text, HTML, an image, the primary selection), the system's dialogs and `QtQuick.Dialogs` (drawn with QtQuick.Controls where the system shows none), `QtCore`'s `Settings` (written by a thread of its own) and `StandardPaths`, remote resources fetched and full HTTP requests sent off the Engine thread (which scripts send with the `XMLHttpRequest` global, a JS class over one native helper; a reply of an XML type is a read-only DOM in `responseXML`, read out of the reply only when a script asks for it: `features/platform/xml-http-request-document.feature`; one that is not asynchronous is waited for on the Engine thread, as Qt waits for it, and `ljos test` takes in what was fetched before its virtual time passes: `features/platform/xml-http-request.feature`), drags from other applications, `QtCore`'s permissions, opening a URL in the user's application for it (`Qt.openUrlExternally`; the stand-in records the URL and opens nothing), and the application's badge, a count on its icon (`Platform::set_badge_number`, as `QGuiApplication::setBadgeNumber`; 0 clears it; the stand-in records the count and shows nothing). `ljos_shell` gives the desktop ones (feature `native`); the desktop opener starts `xdg-open`, `open` or the Windows URL handler and does not wait for it, and Android and iOS have none yet. The shell shows the badge on Linux as the launcher-entry signal on the session bus (`com.canonical.Unity.LauncherEntry.Update`, which KDE Plasma and the GNOME docks read), sent by a thread that starts with the first count and sleeps until the next, and answered to a dock that asks later (`Query`) by the bus connection's own thread, which wakes only for a message, and on macOS as the Dock tile's badge label; without a session bus, and on Windows (whose taskbar shows a count only as an overlay icon drawn per count), the count is kept and not shown. The desktop knows the application by its desktop entry, `Platform::desktop_file_name` (`QGuiApplication::desktopFileName`; the program's file name unless the application gives one, as `App::desktop_file_name` does), which the shell also gives every window as its Wayland app id and X11 class. The application's icon is `Platform::set_application_icon` (`App::icon` from straight RGBA pixels, `App::icon_file`/`App::icon_url` from a picture decoded off the UI thread, `ljos run --icon`), and a window may show its own in place of it (`Platform::set_window_icon`); QML has no property for either, as Qt's `Window` has none. The shell gives what changed to the host when the loop settles, by a counter the platform bumps, so nothing polls. On X11 and Windows it is winit's window icon (`_NET_WM_ICON`, which `crates/ljos_bdd/tests/x11_icon.rs` reads back on the desktop sandbox), and on Windows the taskbar's too. On macOS the application's is the Dock's (`NSApplication.applicationIconImage`), bundled or not; a window there has no icon of its own. On Wayland it is `xdg-toplevel-icon-v1`, bound on winit's connection when the first icon is given, padded to a square and scaled by the compositor; a compositor without the protocol (sway 1.12 among them) shows what the desktop entry names. `QtSensors` (`Sensor`, `Accelerometer`, `Gyroscope`, `Magnetometer`, `Compass`, `PressureSensor`, `AmbientLightSensor`, `OrientationSensor` and their readings) reads the device through a `SensorService`; the stand-in is `NoSensors`, a desktop without any, where a sensor is not connected to a backend, its `reading` is null and `start()` fails, as Qt documents, and tests install `FakeSensors`. Only a started sensor holds anything of the service: a `SensorSink` it feeds from whatever thread reads the device. A reading posts one job to the platform's inbox and later ones replace it until that job runs, so a source faster than the frames costs one job a frame and shows its latest reading; `dataRate` is kept on the UI side too, a reading that comes sooner waiting on a timer for its moment. No host gives a backend yet. On Linux the one to write is iio-sensor-proxy, on the system bus (`net.hadess.SensorProxy` at `/net/hadess/SensorProxy`: `ClaimAccelerometer`/`ReleaseAccelerometer` with `HasAccelerometer` and `AccelerometerOrientation`, `ClaimLight`/`ReleaseLight` with `HasAmbientLight`, `LightLevel` and `LightLevelUnit`; the compass at `/net/hadess/SensorProxy/Compass`, `ClaimCompass` with `CompassHeading`), read through `PropertiesChanged` by the bus connection's own thread, as the shell's badge is. It belongs in `ljos_shell`, which already has the D-Bus crate (`ljos_platform` has none). The proxy gives an orientation, a light level in lux or a vendor's unit, and a heading, but no raw acceleration: it would back `OrientationSensor`, `AmbientLightSensor` (a lux level mapped to `LightLevel`) and `Compass`, and leave `Accelerometer` without a backend. `FontLoader` and `SystemPalette` are in `ljos_quick`.

**Runs that must not write in the user's home.** `Platform` reads its directories with `PathEnv::for_process`, which hands a Rust test binary (one in `target/*/deps`) a fresh home under the temporary directory, and does the same for any run whose configuration, data, state or cache directory falls in the fallback directories of the home `LJOS_GUARD_HOME` names, saying so on standard error. The harnesses set that variable to the real home and move `HOME` with the XDG directories: `ljos_bdd` for the qml6 and `ljos test` it starts, `scripts/own-home.sh` for `gallery` and `gallery:reference`, and `scripts/cua-sandbox.sh` for what runs in the sandbox. DECISION: a run that would reach the guarded home is moved, not stopped, so a drive still shows its app; only the fallback directories are guarded, so a sandbox may itself live under the home (`target/cua`), and a relative or empty XDG variable, which falls back to them, is the leak this closes.

- **As built: naming a freeze (wave 9).** A host may set `Engine::set_slow_call_budget`: native calls are then timed (only then is a clock read), the ones over budget kept (last 64) and named in the log at most once a second per method. `ljos::task::spawn` is the facade's shape for heavy work: the global compute pool runs it, the platform inbox delivers its value on the Engine, and a `Task` cancels it. Native code still cannot be preempted; these make the offender visible and the async shape the easy one.

**As built (M2, desktop).** `ljos_host` has `PlatformHost`, the events and `WinitHost` on winit 0.30.13. The application behind a host is a `HostApp`: it is handed every event with a `HostControl`, through which it opens and changes windows, asks to be woken at a time and ends the loop. The loop wakes by itself never or at one time; a `Waker` wakes it from another thread. The crate knows nothing of what draws into a window. `AndroidHost` is M3.

### 10.2 App core versus UI host

**The app core** lives as long as the process. It holds the object tree, JS, tokio and the models.

**The UI host** is rebuilt on `SurfaceCreated`. It holds the egui `Context`, the wgpu surface, the atlas and the GPU textures.

**Surface lifecycle.** The wgpu surface is created on resume and dropped on suspend.

**Process death.** After the process is killed, Ljós restores from a UI-state snapshot of at most about 100 KiB. The snapshot holds:
- the route stack;
- scroll anchors (key plus offset);
- drafts;
- tabs.

The snapshot is written on `Stop`. QML opts in through `Ljos.Persist { id; property }`.

### 10.3 Mobile behaviours

- **Gestures.** One gesture arena: tap, double tap, long press, pan and pinch. The platform supplies touch slop and timeouts. QML handlers sit on top.
- **`ScrollPhysics` as data**, with a `VelocityTracker` that throws what qml6 throws: each move's speed is smoothed into the ones before it, and a release throws the mean of the last three. While the content is dragged, `horizontalVelocity` and `verticalVelocity` show the last move's speed instead, the first one measured from the press, as qml6 shows them. A pointer that rested 100 ms before letting go, or a speed under the platform's least flick (75 pixels a second on the desktop), throws nothing. Per platform:
  - Android: spline fling and stretch overscroll;
  - iOS: exponential deceleration and rubber band;
  - desktop: smooth wheel.

  `Flickable.physics` defaults to the platform's values.
- **`Ljos.Back` router:** the topmost consumer wins. It supports the legacy back button and `OnBackInvokedCallback`, the latter through Java glue.
- **Insets.** `SafeArea` and the keyboard rectangle come from insets. Windows are edge-to-edge.
- **Scaling.** The DPI scale is never snapped. `fontScale` applies to text only.
- **Networking.**
  - Sockets are live only in the foreground, with a 10–30 s grace period. They resume with a cursor.
  - Wake-ups come from FCM/APNs, outside the core.
  - TLS uses rustls with `rustls-platform-verifier`.
  - Retries back off exponentially with jitter from 1 s to 30 s.
- **Build.**
  - Android: a Gradle template plus cargo-ndk 4.x, arm64-v8a and x86_64, minimum API 26, AAB.
  - iOS: a generated Xcode project and a static library.
  - Both are wrapped by `ljos android run|bundle`.
- **rquickjs** has no Android or iOS bindings, so they are generated with `bindgen` in CI per target. This is spiked in M0.

---

## 11. Input, focus and IME

Sources: 02 §3.1.6; 05 §3.2.

- **`InputRouter`** takes egui `RawInput` and hit-tests in reverse paint order. It accounts for inverse world transforms, ancestor clips and `containmentMask`. egui's `Sense` and `ScrollArea` are not used for QML content. Keys that Ljós accepts are removed from egui's input.
- **`FocusTree`** holds FocusScopes, each with its own focus child. It also holds `activeFocusItem` per window.
  - `activeFocus` is recomputed only along the old and new focus chains.
  - Tab order is precomputed per scope, honouring `KeyNavigation`, and invalidated when the structure changes. As built it is one flat order per window, built lazily on the first Tab after a change.
  - Focus and key dispatch start as the `focus` and `keys` modules of `ljos_quick`, free of egui; `ljos_input` is split out when a second crate needs them (section 4, "start small").
  - A scene starts active. The host calls `set_window_active` when the window loses or gains the keyboard.
  - egui sees one focusable widget per `QmlView`. Unaccepted Tab and arrow keys go back to egui; assistive technology follows focus inside the view through AccessKit's `active_descendant`.
- **Scrolling as built.** `Flickable` lives in `ljos_quick::flickable` and asks a `ScrollPhysics` what the content does when nothing holds it. Content follows the finger and the wheel; when they let go, `flickable::fling` follows the curve the physics gives (a fling, overshoot, rebound, a wheel that glides), frame by frame, and asks for no frame once everything rests. A `rebound` transition is run on a hidden item the content follows.
- **Pointer handlers as built (M2).** `DragHandler`, `HoverHandler`, `PinchHandler` and `PointHandler` join `TapHandler` and `WheelHandler` (`ljos_quick::handlers`): a handler takes a passive grab at a press and asks for the exclusive one when its gesture begins, as `grabPermissions` allow. Every drag starts past one threshold, the system's start-drag distance. `Drag`, `DropArea` and `MouseArea.drag` move and drop items within a scene, and a drag from another application reaches a `DropArea` through `ljos_platform::ExternalDrag`. `PointerDevice` exists uncreatable, so properties declared with `QInputDevice::DeviceTypes` and `QPointingDevice::PointerTypes` are flag-typed like Qt's; its `uniqueId` is still a string, not Qt's `pointingDeviceUniqueId` value. A `WheelHandler`'s `margin` widens where it takes the wheel, at its item's place in the stacking order, and wheel deltas are whole-number points like Qt's `QPoint`. `MouseArea.cursorShape` holds its value and resets, but the pointer does not show it yet: the scene's cursor (`QuickScene::cursor_shape`) is a table view's resize cursor alone.
- **Files dropped from outside as built.** winit 0.30.13 says that a file is hovered, dropped or taken away, but not where: the events carry only the path, on every platform, and on X11 winit ignores `XdndPosition`'s coordinates. Where the shell follows a drag itself (Wayland, X11 sources that honour `XdndProxy`, macOS and Windows, below) the position comes with it and none of this applies; it is what other X11 sources get, and macOS and Windows windows the shell's own handler could not be put on (below). egui-winit passes the files on as `hovered_files` and `dropped_files`, and the view (`ljos_egui::QmlView`) takes the pointer's position when egui has one. When it has none (the pointer never came into the window, or egui saw it leave), the drag goes where the pointer was last seen in the view, else to the view's centre. A drop put there that no `DropArea` takes is logged as a warning in `qt.quick.dnd`, so a file is never lost without a word. DECISION: the centre and not a refusal, because a window with one large drop target, the common case, then takes the file.
- **When drops cannot come, and when the shell asks for X11.** The host says which drops it has (`Capabilities::file_drops`). Where none can come (on Wayland, a compositor with no `wl_data_device_manager`), the shell tells the platform (`Platform::set_external_drops_unavailable`, with the reason and what to do), and the first `DropArea` that would take files (no `keys`, or `text/uri-list` among them) is logged as a warning once, in either order (`features/input/external-drops-unavailable.feature`). An application can still ask for X11 with `ShellConfig::prefer_x11` (or `ShellConfig::prefer_x11()`), and the user can say either way with `LJOS_PREFER_X11=1` or `0`: the shell then builds winit's loop with `EventLoopBuilderExtX11::with_x11` when there is an X display (`DISPLAY`), and `Qt.platform.pluginName` names the window system the loop speaks, `xcb` or `wayland`, rather than guessing from `WAYLAND_DISPLAY`. Drags within the application work everywhere.
  - On Wayland, where winit 0.30.13 has no `wl_data_device`, the shell keeps one of its own (`ljos_shell::wayland_drag`), on the connection winit opened (`OwnedDisplayHandle`, through wayland-client's `Backend::from_foreign_display`), with an event queue of its own. winit reads the socket and the device's events wait in that queue until the loop is about to sleep, which dispatches them: a thread reading the socket too woke for every event any window got, three times a click (`features/window/passes-per-click.feature`). That alone misses a drag that comes over a window no pointer is over: winit 0.30.13's loop is about to sleep only when the connection brought events for its own objects (its Wayland source sets `dispatched_events` only then; with `ControlFlow::Wait` it goes back to `poll` without `AboutToWait` otherwise), so the drag's events would wait for the next thing a window hears. So while no pointer is over a window of the application (the shell has a `wl_pointer` of its own on each seat, made when the seat says it has a pointer, as asking a seat without one is a protocol error), or a drag is over one or out of it, a thread waits on the socket too and dispatches the queue; once a pointer comes over a window, winit hears it and the thread sleeps on a condition variable, so a click wakes no thread but the window's. DECISION: that and not a thread always on the socket, as the time a pointer is away from every window is when a drag from another application can come, and a click never wakes it. It binds `wl_data_device_manager` (version 3 at most) and a data device for every seat there at start. At a drag's enter a thread of its own, asleep until then, reads every format the source offers from a pipe (at most 8 MiB a format, waiting at most 500 ms for each), as the source may be the application itself, which writes from the loop; that thread posts the steps, in order, (entered with the surface, moved, left, dropped, all in surface coordinates) into the shell's loop through its event-loop proxy, as AccessKit's events come in. The loop finds the window whose surface it is, hands the step to that window's view (`QmlView::window_drag`, which makes the `ExternalDrag` for the scene), and tells the compositor whether a `DropArea` takes it: `accept` with `text/uri-list`, else `text/plain;charset=utf-8`, else `text/plain`, else the first format offered, or with none; `finish` after a drop. The offer's `source_actions` and the compositor's `action` are the drag's `supportedActions` and `proposedAction` (copy and move; Wayland's third action is `ask`, which leaves the choice to the target and is no action a `DropArea` takes, so it is left out), and the action the area takes is the only one the shell offers back (`set_actions`), so a source that moved deletes only when told move (`features/input/drag-from-another-application-actions.feature`). So a drag from a Wayland application carries files, links, text and any MIME type, with its position. Seen in the off-screen sandbox: a GTK 4 window on headless sway, dragged by a virtual pointer over a Ljós `DropArea`, gave it `text/uri-list`, `text/plain` and a format of its own, with `urls`, `text` and `getDataAsString` as offered, and its drag ended. CI has no compositor, so this is a recorded run; the scene's side is tested through the seam (`features/input/drag-from-another-application.feature`). DECISION: data is read at enter and not on demand, because QML reads `drop.text` synchronously in a handler and a round trip to the source there would block the UI thread; seats plugged in after start are not followed. When winit gives Wayland drags (0.31's `DragEntered`, `DragPosition`, `DragDropped`, `DragLeft`, files only), Ljós keeps its own device for the other formats, or drops it if winit by then carries them.
  - On X11 winit answers XDND itself, asks the source for `text/uri-list` alone and drops `XdndPosition`'s coordinates. The shell takes the drag from it without a second handler on winit's window: it makes an unmapped input-only window of its own, marks it `XdndAware` (version 5), and puts an `XdndProxy` naming it on every window it opens (`ljos_shell::x11_drag`), so a source that follows the protocol sends its messages there, with the toplevel's id in each. A thread on an x11rb connection of its own, asleep in `poll` until the server writes, follows them: at the first `XdndPosition` it first has the loop answer `XdndStatus` from the formats offered alone (`DragStep::Offered`: a `DropArea` under the pointer whose `keys` those formats meet takes it, with the proposed action), as a source may give up on a target slow to answer, and waits for that answer (500 ms at most); then it converts `XdndSelection` to every type the source offers (the three in `XdndEnter`, or its `XdndTypeList`) that is a MIME type or one of X11's names of text (`UTF8_STRING`, `STRING`, `TEXT`), at most 8 MiB a format, `INCR` pieces included, waiting at most 500 ms each, and posts the same steps as on Wayland, with the position translated from the root to the window's pixels, which the loop turns into points with the window's scale. The loop's answers go back on a second connection: `XdndStatus` (accepted with the proposed action when it is copy, move or link, else copy; positions asked for on every move, as a `DropArea` under the pointer may change) and `XdndFinished` after the drop. So a drag from an X11 application carries files, links, text and any MIME type, placed where it is. A source that ignores `XdndProxy` still talks to winit: files alone, placed as above. The drag allows the action `XdndPosition` asks for and a copy, and those of the source's `XdndActionList` (read once), and proposes the one asked for (a copy when it is `ask` or `private`); `XdndStatus` and `XdndFinished` carry the action the `DropArea` chose (`drop.accept(Qt.MoveAction)` versus `drop.acceptProposedAction()`), and one the source does not allow leaves the proposed one, as Qt's `QDropEvent` does. The parsing and answers are unit-tested; the protocol is a recorded run in the off-screen sandbox (XWayland), as CI has no X server with a drag source: a small x11rb XDND source dragging over a Ljós window was told no over the half without a `DropArea` and yes with copy over the other, and the `DropArea` got its links, text, a format of its own and a 100 kB format sent in `INCR` pieces, at the place it was let go. A GTK 4 source under XWayland there leaves every window after its first position, winit's as well, so it could not be the check.
  - On macOS and Windows the window system asks the target for its answer inside the call that tells it of the drag, on the loop's own thread, so the loop cannot answer in that call (`ljos_shell::native_drag`). Each call returns the answer the loop gave last and posts the step; both systems ask again as the pointer moves and while it rests (AppKit's `draggingUpdated:` under `wantsPeriodicDraggingUpdates`, OLE's `DragOver`), so the source shows the answer one step late at most, and a drop is taken with the last answer, the action qml6 reports too. As on X11 the loop first answers from the formats alone (`DragStep::Offered`), then reads every format on its own thread, the one the system's objects belong to, at most 8 MiB a format, and posts the same steps as Wayland and X11. A drop before the loop has read the data is refused. The mappings (types to MIME types, the source's operations and keys to `supportedActions` and `proposedAction`, the answer back) are in `ljos_platform::drop_systems`, where they are unit-tested on any host.
    - macOS (`ljos_shell::appkit_drag`): AppKit sends a drag to the deepest view registered for one of its types before the window, whose delegate is winit's. The shell registers each window's content view for the types it takes (`drop_systems::appkit::TAKEN`: file and other URLs, text in its encodings, HTML, RTF, PNG, JPEG, TIFF, SVG, JSON, XML, PDF, and `public.data` and `public.item`, which every type conforms to) and adds `draggingEntered:`, `draggingUpdated:`, `draggingExited:` and `performDragOperation:` to the view's class with `class_addMethod`, found from the object, so no class of winit's is named. DECISION: methods added to the class over a subclass swapped in (`object_setClass`), as winit keeps its view's instance variables and may check its class, and adding leaves winit's own methods as they are. A class that defines one of them itself (a later winit) is left alone, which is logged once, and its drags stay winit's. Types are offered as their MIME type where one is known (`public.file-url` and `public.url` as `text/uri-list`, `public.utf8-plain-text` as `text/plain`), under their own name otherwise; `NSFilenamesPboardType` and dynamic types are not offered. URLs are read item by item, text as a string (AppKit converts its encoding), anything else as its bytes. `NSDragOperationCopy`, `Move` (or `Generic`) and `Link` are the actions; the first the source allows, in that order, is proposed, as AppKit leaves the choice to the target when no key is held.
    - Windows (`ljos_shell::win32_drag`): winit registers an `IDropTarget` of its own (files alone) on each window that takes drags; the shell revokes it (`RevokeDragDrop`) and registers its own (`RegisterDragDrop`, through the `windows` crate's `#[implement]`), and winit's `RevokeDragDrop` at `WM_DESTROY` takes it off. Every format the `IDataObject` lists in global memory is offered: `CF_UNICODETEXT`, `CF_TEXT` and `CF_OEMTEXT` as `text/plain` (Unicode read first), `CF_HDROP` as `text/uri-list`, `HTML Format` as `text/html` (its fragment), `UniformResourceLocatorW` as `text/uri-list`, `Rich Text Format` and `PNG` as their MIME types, and other registered formats under their own name, less the shell's own bookkeeping (`Shell IDList Array` and the like). The keys say the action as Explorer's do: Ctrl copies, Shift moves, Ctrl+Shift or Alt links, and none copies; when the source does not allow it, its first allowed one.
    - DECISION: neither platform runs here. Both are checked with `cargo check` and `cargo clippy` for `aarch64-apple-darwin` and `x86_64-pc-windows-gnu` (C code compiled by `zig cc`), and their shared core is tested on Linux; a run against Finder, TextEdit and Explorer is still to be recorded, with `mise run desktop:drag-in`, which opens a window (`scripts/drag-in-probe.qml`) whose two `DropArea`s log what each drag carries (`keys`, `urls`, `text`, `html`, every format) and the action it is answered with. Where the shell's handler cannot be put on a window, its drags stay winit's: files alone, placed as above.
- **Drags out of the application as built.** An item whose `Drag.dragType` is `Drag.Automatic`, or that calls `Drag.startDrag()`, hands its drag to the window system when the host has a `ljos_platform::DragOutService` (`Platform::set_drag_out`): the scene gives it an `OutgoingDrag` (the `mimeData` formats, `supportedActions` and `proposedAction`) and hears `dragStarted`; the host says how it ended (`DragOutDone`, with the action the target took or none), and the item hears `dragFinished` with it and `Drag.active` becomes false. The window system ending the drag is a host write, so a binding on `Drag.active` stays and starts the next one; a host write of false while the drag is out is not taken, as the window system ends it. The press goes with the drag: the view cancels the pointer it holds (`QmlView::pointer_taken`), as the window system takes it. A drag that comes back over the application's own window is the item's drag, with its source and keys, through the same path as one from another application. Without a service the drag stays in the window, as before (`features/input/drag-to-another-application.feature`, through `ljos_platform::RecordedDragOut`).
  - On Wayland the shell makes a `wl_data_source` of its own (`ljos_shell::wayland_drag`), offers every format (plain text under `text/plain;charset=utf-8` too), sets the actions allowed (link left out; copy when that leaves none) and starts the drag from the press its own `wl_pointer` heard, whose serial the compositor wants and winit keeps to itself. A target's `send` is written on a thread of its own, as the target may be this application. `dnd_finished` ends it with the action the compositor said last, and `cancelled` with none; each wakes the loop (`DragStep::OutEnded`), which tells the scene. A Wayland source has no proposed action: the compositor picks from the target's preference (in the recorded run the target saw copy proposed although the source proposed move). Seen in the off-screen sandbox: two Ljós applications on headless sway, dragged by a virtual pointer, carried `text/plain` and a format of the source's own to the other's `DropArea`, which read `keys`, `text` and `getDataAsString`; the source heard copy over the target, none over the desktop and over its own window without a `DropArea`, dragged again, and neither used CPU when idle. CI has no compositor, so this is a recorded run.
  - On X11 the shell is XDND's source itself (`ljos_shell::x11_drag`), on the connection and thread that follow drags in, with the unseen proxy window as the drag's source and the owner of `XdndSelection`. The loop lets go of the pointer grab the press gave winit (`XUngrabPointer` on winit's own Xlib display, which the process has loaded already), grabs the pointer on the shell's connection and hands the drag to the thread; when the grab fails the drag stays in the window. The thread sets `XdndTypeList` (every format, plain text also as `text/plain;charset=utf-8` and `UTF8_STRING`) and `XdndActionList` (`supportedActions`, copy when that is empty), owns the selection and grabs the keyboard. At each move it finds the toplevel under the pointer by walking `TranslateCoordinates` down from the root, takes its `XdndProxy` when the proxy names itself, and talks to it when it is `XdndAware` at version 3 or later, at the lower of its version and 5: `XdndEnter` (the first three types, and the bit that says the rest are in `XdndTypeList`), `XdndPosition` with `proposedAction` (or the first allowed one), and `XdndLeave` when the pointer moves on. A position waits for the `XdndStatus` of the one before it, and none is sent inside the rectangle a status says needs no more. Let go over a target that accepted, the drag is an `XdndDrop`; over one that refused, or nothing, it ends with none. `SelectionRequest`s are answered with the format, `TARGETS` with the list, and a format over 64 KiB in `INCR` pieces, each sent once the requestor deletes the last. `XdndFinished` ends the drag with its action (version 5) or the last status's (older). Escape ends it with none, as does a target that has not answered a status 1 s after the release, or finished 5 s after the drop, each piece or request it reads putting that off. The end lets go of the grabs and the selection and wakes the loop (`DragStep::OutEnded`). A drag back over the application's own window comes to the same thread through the proxy, which answers it from the formats it holds, as converting its own selection on its own thread would wait for itself. With no drag the thread sleeps in `poll` with no deadline, and pointer motion is not asked for until a drag is out. The message building and parsing are unit-tested. Recorded in the off-screen sandbox (XWayland on headless sway, a virtual pointer):
    - A Ljós source carrying text, a link, a format of its own and a 100 kB format reached a Ljós target, a small x11rb target, and qml6 on xcb, each of which read every format, the large one in `INCR` pieces. The source heard copy.
    - The x11rb target's rectangle stopped positions, and a target that refused got a position for every move, then a leave. The source heard none from it, none over the desktop, and none 5 s after the drop from a target that never finished.
    - Escape (sent through XTEST) left the target and ended the drag with none.
    - Over its own window the drag reached its `DropArea` as the item's own.
    - The drag thread woke neither while idle nor on clicks and motion with no drag out.
  - DECISION: let go of winit's grab through Xlib rather than a second click or a wait. The pointer grab from the press belongs to winit's connection, and the server gives it to no other client until that connection ungrabs. winit 0.30.13 offers no call for it. Its display is in the window's handle, and libX11 is in the process already (`RTLD_NOLOAD`), so nothing more is loaded.
  - DECISION: the limits. Under XWayland the toplevel under the pointer is found among X windows alone, so a native Wayland window above one is not seen, and the drag goes to the X window beneath it.
  - What lifting it would take, not built. The X server under XWayland stacks X windows alone; the compositor alone knows where Wayland windows are, and no protocol hands a client other clients' rectangles (`wlr-foreign-toplevel-management` and `ext-foreign-toplevel-list` give titles and states, no positions). Two ways remain. (1) When the shell runs under XWayland and a Wayland display is there, start the drag as a Wayland data source instead of XDND: `wl_data_device.start_drag` lets the compositor pick the target, Wayland windows and X windows alike (XWayland bridges to XDND itself). It needs a Wayland connection of the shell's own beside winit's X11 one, a surface for the drag's origin and the serial of the press, which only a Wayland surface that got the press has, so the window would have to be a Wayland window, which is the Wayland path already built. (2) Ask the compositor per drag through a protocol of its own (sway's IPC tree gives every window's rectangle and stacking), which ties the shell to one compositor and breaks principle 9. Neither is worth it while an application under XWayland can be run as a native Wayland one; the limit stays written down.
  - DECISION: qml6 as an X11 *source* refuses the conversions and leaves without a drop when a target answers `XdndStatus` late; the x11rb target loses it from 150 ms on, and Ljós lost about one drag in six while it read every format before its first status. Answering before the reads, the sandbox (a virtual pointer, 30 drags each) gave 29/30 before, 30/30 after with the machine idle (29/30 with a build beside it, a drag qml6 left without a drop), and qml6 to qml6 26/30. The action qml6 reports at `dragFinished` is the last status's, not `XdndFinished`'s, and a status after the drop is ignored, as Ljós has it.
  - macOS and Windows have no drags out: winit 0.30.13 has no API to start one, so a drag there stays in the window.
- **Wheel and hover as built.** A wheel event is offered to the items under the pointer, topmost first; for each, its `WheelHandler`s come before the item itself (a `MouseArea` with a `wheel` handler, or a `Flickable` that can move), and the first that keeps the event ends the round. A pointer that comes into a `hoverEnabled` MouseArea gives `entered` and `positionChanged`. `TapHandler.point` is there; `WheelHandler.margin` is stored and not acted on.
- **Key dispatch** runs `ShortcutOverride`, then `Shortcut` (context-aware, with `activatedAmbiguously`), then `Keys` (`priority`, `forwardTo`), bubbling up. `StandardKey` maps to platform-specific key sequences (`ljos_quick/src/keys/standard.rs`). On Linux they follow the desktop, as Qt's do: `XDG_CURRENT_DESKTOP` (or, when it is empty, `KDE_FULL_SESSION` and `DESKTOP_SESSION`) picks the GNOME or KDE Plasma bindings, and any other desktop gets the generic Linux column, so Ctrl+K deletes to the end of the line everywhere on Linux. The Qt oracle runs qml6 on its `offscreen` and `vnc` platforms, which ask no desktop and have the Windows column on any machine, so a scenario about another platform's bindings is `@ljos-only` and is checked by hand against qml6 on xcb.
- **Text input** goes through two traits. `TextInputClient` is implemented by TextInput and TextEdit: `surrounding`, `replace`, `set_selection`, `cursor_rect`, `hints`. `PlatformTextInput` has one implementation per platform:
  - desktop: winit IME, moving to 0.31's `ImeRequest`;
  - Android: GameTextInput (`set_text_input_state`, `set_ime_editor_info` derived from `inputMethodHints`), with a Kotlin `InputConnection` as the fallback;
  - iOS: a `UITextInput` view written with `objc2`.
- **Text input as built (desktop).** `TextModel` in `ljos_text` holds the text, selection, undo and composition, by grapheme cluster. `TextInput` and `TextEdit` are the `text_input` module of `ljos_quick`, and both traits live there too, free of egui. `TextInputClient` also has `enabled`, `set_preedit` and `commit`. `ljos_egui` has the desktop `PlatformTextInput`, over egui's IME events and platform output. The whole text is shaped again on each keystroke, and caret places come from the cluster advances of that layout rather than from Parley's cursor types. The cursor blinks from one timer deadline, so an editor without the keyboard schedules nothing.
- **Android IME is the largest risk across all the reports.** It gets a time-boxed spike in M0 that covers composing, autocorrect, Icelandic dead keys, CJK, gesture typing and password managers. The production bridge lands in M3.

---

## 12. Hot code update

Sources: 03 §2.5, §3.5–3.6; 07 §2.2.

### 12.1 Development

`ljos run` shows a project whose native types are in Rust it does not have. It reads the file `--types` names, else `ljostypes.json` in the project's directory (the entry's directory, or `--project`), and looks nowhere else. With `--example` it reads none, as the example's backends are the types. A file that is absent says nothing. One that was found and cannot be read, has another format, or describes a type whose base is neither in the file nor a toolkit type, stops the run and names the file. The compiler checks against it as a build does, so `onStopped:` on a type without that signal is the same error. Each type no Rust code registered stands in (`Engine::register_stand_ins`): properties in dynamic slots holding what they are given, signals that can be emitted and handled, methods that do nothing and return `undefined`, enums, grouped properties, the default property. The run says once on stderr which types stand in. DECISION: a stand-in's attached type is not made, because `MetaObject::attached` is a plain function and a description read at run time is not; the run says which, and `Player.rank:` is the usual warning that the type has no attached type. DECISION: the file is read once, so `--watch` does not follow it, and changing it means starting again; a change to it would have to replace types that live objects already use. A production bundle (§12.2) carries no such file: stand-ins are for previewing.

```
notify watcher (120 ms debounce, 15 ms once every file written is closed; BLAKE3 fingerprint of qml/js/qmldir/theme)
  → incremental compile (salsa) → IR → diff(prev, new) → ReloadPatch{generation, ops}
  → devtools WebSocket (loopback; `adb reverse` on Android; loopback on the iOS simulator)
  → device: UiMsg::Reload → applied at a frame boundary as one transaction
  → on any error: keep the old generation live, show the overlay, keep watching
```

**Object identity**

`ObjectKey = (ComponentId, [KeySeg])`. Segments are chosen in this order of preference:
1. `Id`;
2. `Named` (`objectName`);
3. `ModelRow(row key)`;
4. `Typed{type, nth}`.

- A different type at the same key causes a remount.
- A changed `ComponentSignature` keeps each property by name only if its type is unchanged.
- Renaming an `id` creates a new object.
- A lint asks for `id`s on stateful siblings of the same type, so that `Typed{nth}` matching is not confused when they are reordered.

**What survives a reload**
- assigned property values (according to the provenance bit), an object among them being the object at its place in the new tree;
- an object a script still holds (a closure handed to a singleton): reading it reaches the object at its place in the new tree;
- `ListModel` contents;
- `contentX/Y`, which are clamped;
- `currentIndex`;
- the `StackView` stack (the pages pushed are made again from their sources);
- objects a script made with `createObject` or `incubateObject` under an object that goes on: made again in the order they were made, from their file as it is now, under the object that goes on for their parent; a `Component` made by address makes later objects from the new file too;
- an open `Popup`;
- the `Loader` source;
- `Timer.running`, and the running of an animation that `start()` set going;
- text and selection;
- `activeFocus` and accessibility focus.

Bindings and handler bodies always come from the new IR. `Component.onCompleted` does not run again on preserved objects. `Component.onReloaded`, which is dev-only, exists for custom refresh. `pragma LjosReload: reset` forces a remount.

**What forces a full restart:** a change to a singleton's shape, a new native module, or the JS module fallback failing. JS modules reload through generation-tagged specifiers, because rquickjs cannot unload modules.

**Rust state.** State that must survive a reload implements `Migrate { VERSION, snapshot, migrate }`. If migration fails, the object remounts with defaults and the failure is reported.

**Behaviour that existing QML hot-reload setups rely on** (observed in HAL-C2, 07 R6, R7):
- the new root is created before the old one is deleted;
- the default shell is the fallback;
- `Runtime.lastError` drives the overlay;
- `theme.json` keeps the previous version when the new one has an error;
- unknown URL query strings (`?generation=N`) are ignored when resolving files.

subsecond (Rust hot-patching) is an optional extra for desktop development. It does not work on iOS devices, so Ljós never depends on it.

**As built (M1, desktop).**
- **Applying a patch.** The engine does not apply a patch op by op. It makes the application's roots again from the patched files and carries state across by `ObjectKey`: script-assigned values, the properties a type registers with `register_reload_state`, and whatever the type's `Migrate` carries. The old roots are destroyed only once every new one stands, so a patch applies whole or not at all.
- **Message path.** There is no single `UiMsg` enum; `ljos_dev` owns a bounded `UiInbox<DevMsg>` of capacity 5 whose state messages coalesce. The devtools WebSocket comes with the first device target (M3).
- **Incremental compile.** `ReloadSession` recompiles the changed files and the files that mention their types by name, which over-approximates the dependency set. salsa is not used yet.
- **Watched files.** `.qml`, `.js`, `.mjs`, `qmldir` and `CMakeLists.txt`. A changed script recompiles the QML files that name it. The theme file is watched too (Theme files, below).
- **A save in parts.** A change that does not compile is not told at once: an editor can write a file in parts, with a pause longer than the quiet time between them. The watcher waits the whole quiet time again and reads the files again (`Build::Unsettled`). The error is told only if the next build still fails, whatever it read. A reload that was asked for tells the error at once.
- **Saved pictures.** A picture file under the project (by its suffix) is no build's: after the same quiet time the image cache drops it and every `Image`, `BorderImage` and `AnimatedImage` that loads it, by its file URL or by a resource address registered to it, loads it again. Nothing is reloaded, so all state stays. The image store knows which item loads which URL only while it is told to watch (`ImageStore::set_watching`, on in `ljos_dev::prepare`); a shipped application notes nothing. `ljos run --watch` registers the project's resource pictures as `ljos run` does.
- **Saved data files.** A file a model reads beside its QML is no build's either. `FollowedFiles` (`ljos_engine`) records which object reads which file, the session's watcher follows those files even outside the watched roots, and after the same quiet time each reader reads again. `JsonListModel` is the first: it applies only what differs, nothing is made again and no QML is reloaded (`features/models/json-list-model.feature`). A shipped application has no watcher, so its model reads again on `reload()`.
- **Driven.** Ten corpus examples were edited while they ran: colours, texts, items added and removed, bindings, sibling files, a syntax error and its fix. The table of what each save kept, lost and told, and how long it took to show (17–31 ms from the save for a small or a large file, 15 ms of it the watcher's quiet time), is in `docs/hot-reload-drive.md`; `mise run gallery:hot <id>…` does it again. The reload lint says what goes wrong and asks for an `id` or an `objectName`. Like every compile-time warning, it is told once as its file is loaded (at the start, or when a `Loader` first reaches the file), as qml6 tells a file's warnings when it loads it, and a reload tells only the ones that are new (`Report::Warnings`, then the reload's own).
- **Module aliases.** `ljos run FILE --alias MODULE=DIR` (repeatable, with or without `--watch`) imports `MODULE` from `DIR` instead of the directory the import path finds; a directory that describes no such module (in a `qmldir` or a `CMakeLists.txt`) is refused with a message naming both, and the modules it does describe. With `--watch` the inspector's `app_alias` (section 19.4) sets or drops an alias while the application runs, and that reloads as a save does. A directory under a root is read with the root; one outside every root (or under a directory a walk skips) is read under the key prefix `.aliases/<MODULE>`, which no file of a root can have, and watched with the roots. Where a module's own path from a root exists (`App/Media` for `App.Media`) only that directory answers its import, so a stand-in kept beside it in the project is not seen until it is aliased. The objects made from the module are of other types after the switch, so they are made anew, as with an edit that changes an object's type; the rest of the application keeps its state. DECISION: the identity rule above is unchanged for aliases; carrying state into a different type would guess. DECISION: `ljos render` and `ljos test` take no `--alias`: a picture or a suite is judged against the files as they are, and a suite can import a stand-in with `-import`. DECISION: an agent may only name a directory under the project, one of its roots, or one the application was started aliased to; the path is checked as written (`.` and `..` resolved) before the filesystem is asked, so a refusal tells nothing of what lies outside, and checked again with its links followed. Not done: `ProjectTexts` and `Qt.createQmlObject` read the files without aliases, and a relative import out of a directory read under `.aliases/` does not reach back to the directory beside it.
- **Theme files.** A theme file sets the roles of `Ljos.Design` (`Colors`, `Spacing`, `Typography`, `Motion`) without a change to the QML. `ljos run`, `ljos render` and `ljos audit` read it, and so does the dev host. It is the file `--theme FILE` names (a named file that is not there is an error), else the `theme.json` nearest the entry: in the entry's directory or one above it, up to the project's and no further (`ljos_design::theme::find`; the first entry and first root for `watch_roots`). `docs/theme.md` is the contract other runtimes (ljos-web) read it by. DECISION: the nearest file wins and themes are not merged: one file says the whole theme, and a module run alone keeps its own. It is a JSON object with one object per singleton, each mapping a role to its value:

  | Singleton | Roles | Value |
  |---|---|---|
  | `Colors` | the colour roles (`canvas`, `accent`, `text`, `textOnAccent`, …) | a colour string (`"#9c4dc4"`, QML's `"#aarrggbb"`, an SVG name) for light and dark, or an object with any of `light`, `dark`, `lightHigh`, `darkHigh` |
  | `Colors` | `seedHue` | a number, the hue the palette is made from |
  | `Spacing` | `unit` and the steps (`none`, `xxs` … `xxxl`) | a whole number of pixels, at least 0 |
  | `Typography` | `{step}Size`, `{step}LineHeight` | a whole number of pixels, at least 0 |
  | `Typography` | `{step}Weight` | a whole number, 1 to 1000 |
  | `Motion` | the durations (`enterSmall`, `moveScreenRare`, `sheetInPanelFrequent`, `press`, …) | a whole number of milliseconds, at least 0 |
  | `Motion` | the curves (`easeOut`, `easeOutSoft`, `easeInOut`, `decelerate`, `drawer`, `linear`) | a list of numbers, as `bezierCurve` takes |
  | `Motion` | `enterScale`, `pressScale`, `gesture{Small,Panel,Screen}{Spring,Damping,Mass,Epsilon}` | a number, at least 0 |

  The roles are those `ljos_design::theme::roles()` lists, which the module's files are generated from, so the two cannot differ. A colour variant the theme does not give is the module's own: a single string sets light and dark and leaves high contrast to the module. Each role becomes an override on the singleton's property (`Engine::push_override`), so what binds to it follows and taking the theme away restores the module's value; a colour with variants is a binding that reads `Colors.dark` and `Colors.highContrast`, so it follows the system's scheme as the module does.

  Every mistake is collected and told as `file:line:column: message`: JSON that cannot be read, a duplicate key, a singleton or role the module does not have (`Colors has no role "acent"`), a value of the wrong kind or out of range (`Colors.accent is a colour: …`). DECISION: `Colors.dark`, `Colors.highContrast` and `Motion.reduced` are refused: they are the user's settings, not the theme's; `Colors.table` and `Colors.roles` are refused as not roles. DECISION: a theme that breaks the contrast the palette promises is refused, not warned about. Each of the four variants is checked as the palette checks itself (`palette::check_pairs`, the WCAG 2.2 ratios the module holds its own palette to: 4.5:1 for text at standard contrast, more at high contrast, 3:1 for marks and borders) against the palette at the application's seed hue, rounded to the 30° step the module ships, with the theme's colours put in; only a pair with a colour of the theme's is told (`Colors.text on canvas is 1.61:1 in light; text needs 4.5:1`), at that colour's line. A ground (`canvas`, `surface`, …) of the theme's must be opaque. A theme that names no colour skips the check.

  Without `--watch`, `ljos run` reads the theme before the window opens and stops on a theme that cannot be read; one the contrast check refuses stops it before the root is made. Under `--watch` the file is watched with the project (its directory too when it is outside it). A theme change is its own kind of reload (`DevMsg::Theme`, `Report::Theme`): after the same quiet time the roles are set again in one batch and nothing is made again, so all state is kept. It moves the reload generation (`ReloadSession::advance`, `Engine::advance_reload`), so `app_reload` answers `applied` with the new generation and `app_status` with `after_reload` wakes on it; it does not count as a reload of the QML (`DevSession::reloads`). A file that cannot be read, or a theme refused, keeps the overrides before; the error goes to `Runtime.lastError`, the overlay, `app_reload`'s `failed` and the terminal, and stays through reloads of the QML until a theme is set. Removing the file removes the overrides. DECISION: a theme's error is told at once, not after a second quiet time as a QML error is: the file is one small whole. DECISION: a theme refused at the start under `--watch` starts on the module's own roles, with the error shown, as a QML error does. Not done: a script that changes `Colors.seedHue` later is not checked against the theme again.
- **Where it links.** `ljos_shell` takes `ljos_dev` behind its `dev` feature, and `ljos run --watch` is the CLI entry. `ljos_shell/tests/no_parser.rs` checks that the shell without `dev` links no parser.

### 12.2 Production bundles

**Contents.** A content-addressed archive holding the IR, JS source, optional bytecode and assets.

**`BundleManifest`** holds:
- `native_api`, a semver requirement on the exported Rust↔QML surface;
- `qjs_bc_version` (currently 28);
- `min_app_version`;
- a BLAKE3 hash per file;
- an **ed25519 signature**.

**Loading.** The loader checks the signature first, then the API and bytecode versions. If the bytecode version does not match, it compiles from source. Bytecode is only a cache, because `Module::load` on unverified input is unsafe.

**A/B slots with trial mode.** A new bundle is promoted after a healthy start: the first frame renders, the server connects, and there is no supervisor escalation for 30 s. If the trial crashes, the next launch rolls back. The server can also force a rollback.

**Store policy.** No native code is ever downloaded. Apple DPLA §3.3.1(B) and Google Play allow interpreted updates. Feature-level changes still go through store review, following App Review 2.5.2.

---

## 13. Controls and styles

Source: 01 §2.5, §3.6; 08.

- **QtQuick.Controls has two layers.** `ljos_controls` implements behaviour in Rust: the `Control`/`AbstractButton` state machines, `Popup` and the overlay stack, and `Dialog`. Each *style* is QML written clean-room on top, the way Qt layers templates and styles, with the code being ours. A style is chosen with `QT_QUICK_CONTROLS_STYLE`-compatible configuration, in Qt's order: an import of a style, `-style` on the command line, the variable, the configuration file, and then the platform's style as Qt documents it (Fusion on Linux, Material on Android, Basic elsewhere, since Ljós has no macOS, Windows or iOS style).
- **As built.** A style is a directory of QML files compiled to IR by the crate's `build.rs` and registered as a component library, so a shipped app still links no parser. Style parts are deferred: a style gives each part (`background`, `contentItem`, `indicator`, `handle`, `popup`) as a `Component` and the control makes it when it completes, unless the application gave its own by then, so a replaced part is never made (a test counts creations). `Window` and `ApplicationWindow` are platform windows under the shell and items of the scene without one (section 6.4).
- **Popups as built.** A scene has a second top-level item, the layer (`ljos_quick::layer`), drawn over the root item and asked first by the pointer. `ljos_controls` puts one overlay per window there and each popup's item inside it, so nothing in the content clips a popup and a root that is a positioner does not lay it out. A closed popup's item stays in the overlay, hidden. Transitions are in the polish bullet below; a `Menu` or `ComboBox` popup with more rows than fit is limited to the window and scrolls (`features/controls/menu.feature`, `features/controls/combobox.feature`), and `ToolTip` has one shared instance per window.
- **Polish as built (M1).** `focusPolicy` and `activeFocusOnTab` say one thing between them (the Tab bit), and the Basic style draws `visualFocus` on every control that takes the keyboard. Buttons and menu items have `icon.*` and `display`; an icon is tinted with the text colour unless `icon.color` is given, and `icon.name` shows nothing because there is no icon theme. `palette.*` is inherited down the item tree and into popups (every item has a palette, made when a colour is first set on it; a plain item hands down only the colours set on it, where Qt also gives the platform's for the others, and reads those and the ones set above it, handed down when they are set so that a read costs nothing more; where nothing above set one it reads the style's, where Qt reads the platform's), the style reads the palette instead of literal colours, and a dark palette follows `Qt.styleHints.colorScheme`. A palette has Qt's three colour groups, `palette.active`, `.inactive` and `.disabled` (`ColorGroup`, each made when first written): `palette.button` reads the disabled group's colour on a control that is not enabled and the active group's otherwise (no window is ever inactive), a colour set on the palette is set in every group and one set in a group is set there alone, winning over the palette's on the same object, and a control inherits each group's colours as it does the palette's. A plain item's groups read Basic's colours where it set none, and a popup's groups are not resolved. The style's colours are Basic's, or Fusion's when the application's files use the Fusion module (`Engine::uses_module`: an import, or a type the compiler found there for `import QtQuick.Controls`), every control's then, as Qt gives the imported style's palette whatever style the program started with; Fusion's dark colours are Basic's until they can be measured. The accessibility audit runs over the style in light and dark (`crates/ljos_controls/tests/audit.rs`); the Basic placeholder colour is Qt's value and stays below 4.5:1 for parity (3.04:1 light, 3.95:1 dark), so an application that needs AA sets `palette.placeholderText`. `ComboBox` is editable, `MenuBar` with `MenuBarItem` exists (not yet a full `Container`), and `ApplicationWindow.menuBar` sits above the header. A menu or combo popup shows its rows in a `ListView`, as Qt's does. A popup runs its `enter` and `exit` transitions through `Transition` (section 6.2), with `aboutToShow`, `opened`, `aboutToHide` and `closed` at the moments Qt 6.11 shows; with reduced motion one that would move the popup fades it in or out instead, and `opened` and `closed` come when the fade ends.
- **More controls as built (M2).** The value controls (`Slider`, `RangeSlider`, `Dial`, `ProgressBar`, `BusyIndicator`, `DelayButton`, `RoundButton`, `DoubleSpinBox`, `PageIndicator`, `ToolSeparator`), the check, radio and switch delegates, `ActionGroup` and `Action.shortcut`. Navigation: `Container`, `TabBar` and `TabButton`, `SwipeView`, `StackView` (whose covered pages are frozen, section 8.3), `SplitView` and `Drawer`, which the pointer drags in from the edge of its window and out again.
- **Basic style first.** It is the neutral base that other styles and custom controls build on. Real apps lean on it (HAL-C2 imports `QtQuick.Controls.Basic` 87 times and `QtQuick.Controls` twice).
- **Material, Fusion and Universal as built.** Each is a directory beside Basic that holds only the controls it draws differently: Button, CheckBox, Switch, Slider, TextField, RoundButton and Pane in all three, plus ApplicationWindow, Dialog, Label, Popup (and ToolBar in Material) where the style changes the window and text colours. Fusion draws nearly all of its controls itself, fitted to what a running Qt 6.11 reports and built from shared parts in `QtQuick.Controls.Fusion.impl` (the button panel, the check, radio and switch indicators, the slider's groove and handle); its Tumbler is Basic's, which draws the same. Every other control in `QtQuick.Controls.Material` and the others is Basic's file, so all controls import from every style. The attached `Material` and `Universal` types (`theme`, `accent`, `primary`, `foreground`, `background`) inherit down the item tree, follow the theme and reset to the inherited value; their derived colours and `color()` come from values a running Qt 6.11 reports. `Fusion` is a singleton of the few colours its controls share. Material and Universal also have a font of their own, with another size or weight for some controls (Qt 6.11's values, `ljos_controls::style_font`): a control starts from its style's font for its type and takes from above only what an author set there, and a Basic file a style shares takes the style of the control above it, so such a control outside any of the style's own has Basic's font. These are Basic shapes in each style's sizes and colours, not the full design systems: Material's `variant` and `shade()` are missing, and a change to a style's attached properties updates every attached object of that style.
- **The native macOS and Windows styles** are deferred.
- **Phase 1 types** (the most used across the qtdoc corpus and HAL-C2):
  - Control, AbstractButton, Button, Label, TextField, TextArea, CheckBox, Switch, SpinBox, ComboBox, ToolTip;
  - Popup, Dialog, Menu/MenuItem;
  - ScrollBar, ScrollView, Pane, ApplicationWindow.
- **Phase 2:** Drawer, StackView, SwipeView, TabBar, Slider, ProgressBar, BusyIndicator and SplitView.
- **Default roles and names** are derived from documentation and checked against Qt through a small helper that queries `QAccessible`. This is needed because QML reads `Accessible.name` as `""` for defaults (09 §2.4).

---

## 14. Accessibility

Source: 05 §3.1.

- **Versions.** AccessKit 0.25.1, the version egui 0.37 uses (accesskit_winit 0.34.1, consumer 0.39.1; iOS adapter 0.2.1, Android adapter 0.9.0); its types do not mix with 0.24 (spike 04).
- **egui 0.37's own accessibility.** egui now puts popups, menus, modals and panels in its tree, leaves invisible widgets out, and names widgets with `Response::accessible_name` and `Ui::label_inputs_by`. None of it reaches the scene's tree, which is grafted (`ljos_a11y` builds it, and leaves invisible items out itself); it changes only the nodes of egui's own widgets around a view. egui_kittest 0.37 also fails a test whose tree has an input without a name, and counts AccessKit's `Splitter`, which a QML separator is, as an input: a harness over a scene with separators turns the check off.
- **A11y tree** (`ljos_a11y`, egui-free):
  - An item is exposed when it sets `Accessible.role` or a name. A name without a role gives a `group`, or a `label` on Text. Invisible and `Accessible.ignored` items are left out.
  - Each exposed item has one reactive effect that reads its accessible properties and bounds and updates its node in a retained tree; the tree records which nodes changed. This replaces the `needs_a11y`/`request_a11y` bits of the first design: the reactive graph already knows what changed.
  - When children change, the parent is always emitted with its full child list.
  - The effects exist only while an assistive technology listens (`QuickScene::set_accessibility_active`), so the tree costs nothing otherwise.
  - On egui, `QmlView` emits one embedded node per item under the view's node while AccessKit is active. egui rebuilds its own tree each pass, so the whole Ljós tree is re-emitted on passes that run; the scene's tree stays incremental for the R backend's own adapter later.
  - The AccessKit Click action becomes a press at the node's centre. An `Accessible.onPressAction` signal, and focus, come with the focus tree in M1.
  - Off-screen items that exist stay in the tree. Being culled from paint is not the same as being hidden from accessibility. Frozen pages are hidden.
- **Node ids.**
  - Static items use `(arena index, generation)`.
  - Delegates use `(list node, model key)`, so recycling a delegate never changes the id the screen reader is holding.
- **Lists.** A list has a `List`/`ListBox` role, scroll ranges and `size_of_set`. Each row has `position_in_set`. Scroll actions materialise the next rows. A "Go to latest" custom action exists, plus a synthetic "Jump to latest" node for chat.
- **Live regions.**
  - `Accessible.announce(msg, politeness)` writes to a hidden Status node (polite) or Alert node (assertive).
  - The streaming transcript is a `Log` node with `busy = true` while streaming. Announcements are coalesced to sentence or paragraph chunks.
  - A user setting controls this: off, on completion, or live.
- **Role mapping.** The QML `Accessible.Role` → `accesskit::Role` table lives as data (report 05 §2.4) and is tested.
- **`SystemPrefs`** exposes `color_scheme`, `contrast`, `reduced_motion`, `font_scale` and `screen_reader_active` as signals.
  - `reduced_motion` takes what moves to its end at once, cuts fades to `limits::motion::REDUCED_FADE_MAX_MS` and keeps endless animations running (`animation/motion.rs` says which property is which).
  - The values are exposed as `Qt.styleHints.accessibility.*`.
  - `font_scale` applies at text layout, with a per-item opt-out (`font.scaleWithSystem: false`).
- **Audit lints.** `ljos_audit::audit` (also `ljos_test::a11y::audit`) runs on a mounted scene and checks:
  - names on actionable nodes;
  - images that are either named or hidden;
  - no duplicate sibling names;
  - target size (44 dp on mobile, 24 dp on desktop);
  - that a tab walk visits each focusable node once with no trap;
  - theme contrast of 4.5:1;
  - `size_of_set` on virtualised rows.
- **Static lints** in `ljos check` flag:
  - interactive items with no name;
  - images with no name or `ignored`;
  - `qsTr` called with a non-literal.
- **As built (M1).**
  - The role table covers every documented `Accessible.Role`, with a test that none is missing. Checked, mixed, pressed, expanded, disabled, read-only and selected states, value ranges, and `position_in_set`/`size_of_set` for ComboBox and Menu rows are told. Default roles and states of controls are a reading of the documentation: what Qt hands to assistive technology by default cannot be read from QML.
  - `SystemPrefs` (`ljos_a11y::prefs`) is reported by the host (`report_system_prefs`, a `HostPrefs` naming only what it heard) and set by the application or a test (`set_system_prefs`, `update_system_prefs`; `ljos::system` on the facade). `Qt.styleHints.colorScheme`, `.accessibility.contrastPreference` and `.accessibility.motionPreference` read it; `fontScale`, `screenReaderActive` and `font.scaleWithSystem` are Ljós extensions. Reduced motion takes what moves to its end at once, keeps fades and colour changes at most 150 ms (`ljos_design::limits::motion::REDUCED_FADE_MAX_MS`), and lets endless animations run at their own pace; a popup or StackView page that would move in or out fades instead, and a change of the setting while something runs goes on from where it is. Timers and scrolling are unaffected.
  - The egui host reports the colour scheme (from winit; on Linux winit tells none, X11 having no theme and Wayland only the one the application set, so the settings portal's `org.freedesktop.appearance` `color-scheme` is read there too: 1 is dark, 0 and 2 are light), and whether something listens where the platform does not say whether a screen reader runs (below). The desktop shell reads the rest at start and on each change, on a thread that sleeps until told: on Linux the settings portal's `ReadAll` and `SettingChanged` (`ljos_a11y::DesktopSettings` names the keys: the portal's `org.freedesktop.appearance`, GNOME's and KDE's); on macOS `NSWorkspace`'s reduce-motion and increase-contrast and their change notification; on Windows `SPI_GETCLIENTAREAANIMATION`, `SPI_GETHIGHCONTRAST` and the Accessibility `TextScaleFactor`, on `WM_SETTINGCHANGE` and a change of that registry key. The macOS and Windows readers, and the backdrop on both, have only been checked with clippy for `aarch64-apple-darwin` and `x86_64-pc-windows-gnu`, never run. The C parts of the dependencies (ring, QuickJS) need a C compiler for the target; `zig cc` serves for the check.
  - DECISION: a setting the application or a test made other than the host's last report is held over later reports, field by field, until it is set to the host's value again; the other fields still follow the host. Less motion or more contrast in any one desktop key counts, since desktops keep older keys beside newer ones. macOS has no text size for applications, so it reports none.
  - The audit implements every rule `ljos_audit::rules::implemented()` lists: those eight (`a11y-unnamed-interactive`, `a11y-unnamed-image`, `a11y-duplicate-name`, `a11y-target-size`, `a11y-tab-trap`, `a11y-tab-unreachable`, `color-text-contrast` (once `a11y-contrast`), `a11y-set-size`), `a11y-hidden-exposed`, and the layout and text rules of the runtime audit below. It presses nothing and measures contrast from the last frame. `ljos test` does not run it on every mounted component yet. Known finding: the Basic style's placeholder text is 3.04:1 on white, which is Qt's palette value.
  - `ljos check` runs the static lints by default (`--no-lint` leaves them out); a finding is a warning.
  - Controls take keyboard focus by `focusPolicy`, and the Basic style draws `visualFocus` on the button-like controls.
- **Screen reader or listener.** DECISION: two things were one flag: exposing the tree, and spending memory so a screen reader can read ahead.
  - Exposing the tree follows AccessKit: the effects run while any assistive technology listens, a test driver such as cua-driver included.
  - `screen_reader_active`, and the ±20 rows a view keeps, follow the platform's word that a screen reader runs. On Linux that is the `ScreenReaderEnabled` property of `org.a11y.Status` on the session bus's `org.a11y.Bus`, read at start and followed while the app runs. Elsewhere, and where the host says nothing, it still falls back to "something listens".
  - `LJOS_SCREEN_READER=on|off` overrides both. cua-driver sets `ScreenReaderEnabled` and `IsEnabled` as Orca does, so nothing on the bus tells it from a screen reader; the sandbox (`scripts/cua-sandbox.sh`, `mise run bench`) passes `off` unless told otherwise, so its numbers are what a user without one gets.
  - What Qt does, observed as a black box with `qml6` 6.11.2 on X11 in the sandbox: it registers with the AT-SPI registry when either `IsEnabled` or `ScreenReaderEnabled` turns true, at start or later, stays registered when they turn false again, and is forced on by `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` (its documentation). It keeps no extra rows for either. So Qt's switch is the two properties, not a listener.
  - Gap: accesskit_unix 0.21 activates on `ScreenReaderEnabled` only, so an app is not exposed to a client that sets only `IsEnabled`, as Qt would be.
  - Gap: AccessKit's AT-SPI layer registers no `EditableText` interface, so a client can read, select in and move the caret of an entry but not type into it or set its text; Ljós already takes the requests it would send. What upstream would need: `docs/upstream/accesskit-editable-text.md`.
- **As built: live regions (M2).** A text whose role is a status bar, a notification or an alert tells an assistive technology each new thing it says, a status politely and an alert at once, and so does `Accessible.announce`. A control that is partly checked reports a mixed state.
- **Design guardrails (wave 11).** An interface should be hard to get wrong by accident, so the guidelines are checks, not prose. `ljos_design` is the one table of rules (`RULES`: code, area, severity, whether the source or the scene tells it) and of their limits (`limits`), which every check below reads; `docs/design/` explains each rule at `<page>#<code>` with its sources. Four layers use it:
  - *Make the right thing easy*: design tokens (a palette from a seed in OKLCH with contrast-checked pairs, motion tokens by kind and frequency, spacing and type scales) in a QML module that is plain QML, so an application that also runs on Qt can take it along; and the MCP tools `design_palette`, `design_contrast`, `design_motion` and `design_rules`.
  - *Static lints* in `ljos check` and `qml_lint` (motion curves and durations, scale from zero, layout properties animated, anchors in layouts, text with no elide or wrap, pointer-only and hover-only actions, fixed popup sizes, literal colours).
  - *Runtime audit* of the real scene (`ljos audit`, the inspector's `app_audit`, BDD steps, the observer's Audit page) over a matrix: light and dark, normal and high contrast, font scales up to 200 %, and narrow and short windows. It measures what was drawn: text contrast against the composited background, clipped and hidden-elided text, controls off the window or covered by another item, focus that is not drawn, popups that do not fit, motion that runs. A finding names the rule, the item's locator and its source line, the matrix point and the fix.
  - *Acting checks* focus each stop, press Escape, toggle controls and open popups to see what the scene does, and so run the application's own handlers, whose effects are real. They are opt-in where the application is someone's running one: `app_audit` runs them only with `act`, which needs the `input` capability, and the observer only when its switch is on. `ljos audit` and the BDD and fixture runners own the scene they audit and turn them on. Not run, the report's `notChecked` names the codes they would have checked, rather than finding nothing of them.
  - *Declarative lints* (`decl-*`, `docs/declarative.md`) are hints of `qml_lint`: script that a binding, states or a transition say better (a change handler that only copies a value, handlers that switch a group of properties, animations chained or started against each other, an item shown and hidden from several handlers). They hold back wherever one file does not show the whole story, and never flag a toggle.
  - *Severity is confidence*: an `Error` is certain and fails `ljos check` and `ljos audit`; a `Warning` needs `// ljos-allow(code): reason` to keep; a `Hint` is advice. DECISION: third-party code (the qtdoc corpus) is reported, never gated.

---

## 15. Localisation

Source: 05 §3.4.

- **Catalogue.** Qt-compatible, keyed by `(context = file stem or pragma Translator, source, disambiguation)`.
  - `.ts` is the translator-facing format. Ljós reads and writes it with quick-xml 0.42, round-tripping comments and locations.
  - At runtime Ljós uses its own binary `.ljl`, which can be memory-mapped.
  - In development, `.ts` files are read directly and watched, so translations hot-reload.
  - Ljós never reads `.qm`.
- **`I18n`** holds a fallback chain (for example `is-IS → is → en`) and three signals: `revision`, `ui_language` and `locale`.
  - Setting `Qt.uiLanguage` swaps the chain, bumps `revision`, and updates the layout direction.
  - Only visible translated bindings re-evaluate immediately. The rest are marked dirty and re-evaluate when shown.
  - The signals live in the engine (`ljos_engine::i18n`), where bindings can read them through the graph; `ljos_i18n` is plain data and has no reactive dependency. The engine links it without the `ts` feature, so a shipped app has no XML parser.
- **Plurals.**
  - `%n` uses ICU4X `PluralRules` 2.3. The mapping from `.ts` numerus forms to CLDR categories is validated against `.ts` fixtures produced by running `lupdate` as a black box, never against Qt's plural tables.
  - Where Qt's numerus forms for a language differ from CLDR's categories, a small override table written from Qt's documented plural rules and checked against those fixtures sits beside ICU4X, so an existing `.ts` file keeps its forms.
  - `%1…%9` use `String.prototype.arg` with `QString::arg` semantics. This, `qsTr` and `%n` are needed from M1.
- **Formatting.** `Qt.locale()` is built from ICU4X decimal, datetime and collator data. Qt's format tokens get a small formatter of Ljós's own.
- **ICU4X data.** Use `compiled_data` in development. Release builds use blob data trimmed by datagen to the shipped locales and markers. The size on Android is still to be measured.
- **RTL.** `LayoutMirroring` is resolved per item. Text uses Parley's base direction, and `Start`/`End` alignment resolves against it. AccessKit gets `text_direction` and `language`. Snapshots in Hebrew and Arabic go into the gallery.
- **Extraction.** `ljos i18n extract` walks the qml-solid AST for `qsTr`, `qsTranslate`, `qsTrId` and `QT_*_NOOP`, honours the `//:` `//~` `//=` comments, and merges into `.ts`. `--check` mode runs in CI.
- **Rejected:** Fluent as the primary format, since existing QML code is written against `qsTr` (HAL-C2 alone has 1,562 calls); gettext; and MessageFormat 2, which has no ICU4X crate yet.

Localisation is tested on Ljós's own fixtures, including an Icelandic catalogue.

---

## 16. Plugins and isolation

- **Plugins are trusted in v1 (D5),** as QML plugins are in Qt apps today. They load into the main Engine under their own `ProcId` boundary, supervised by `PluginSup`, so a broken plugin is restarted or disabled without taking down the app.
- **As built: `Ljos.Blocks` (ADR 0003 §4).** A native module of `ljos_quick` (`crates/ljos_quick/src/blocks/`), importable as `import Ljos.Blocks`. A `Contribution { slot; mode: Append | Replace | Prepend; delegate; order }` registers itself under its `slot` name, when it completes, in a registry the Engine keeps (an `Engine::extension`). Each name has its list of contributions and a reactive version node; a `Slot` reads that node tracked, so a contribution that comes, goes or moves to another name runs again only the Slots of the names it touches. There is no polling and no walk of the tree. A contribution leaves the registry when it is destroyed for whatever reason: its Loader unloads, its boundary discards it after a fault, or its parent goes. A Slot makes one item of each contribution's delegate, sorted by `order` and then by when each was made, and puts the items in its own parent just before itself, as a Repeater does. An Append goes at the end; a Prepend goes before the Slot's own delegate, after the Prepends before it; a Replace takes the place of everything so far. With nothing contributed the Slot shows its own `delegate`. A contribution's item is owned by the contribution, not the Slot, so a fault in it belongs to the boundary that made the contribution, and the item goes with the contribution. A block's `list<Contribution>` property is a slot to `ljos_blocks`; it hands its contributions to a Slot's `contributions`, where they show whatever their `slot` (`features/blocks/slots.feature`). `ShellLoader { source; fallback; readonly error, item, status }` makes the shell at `source`. If that shell does not compile, or something in it faults while it is made or later (an exception in a binding or handler, a native panic), the loader discards it and shows `fallback`; `error` reads `file:line: message`, and `status` is `Fallback`. If the fallback fails too, `status` is `Error` and `error` holds both reasons, one per line. A new `source` or `fallback`, or a reload, tries again, and the source's shell shown clears `error` (`features/blocks/shell-loader.feature`). DECISION: it is not built on `Loader`, which only logs why it could not load and holds one URL. Under `--watch`, a shell already shown that an edit breaks keeps the generation that runs, and the reason goes to the reload error (§12.1), not to `error`. There is no application-wide `Runtime.lastError`; each ShellLoader keeps its own `error`. Not built: `PluginSup` for QML plugins. The registry removes a destroyed Contribution whoever destroyed it, so a supervisor that discards a plugin's content gets that behaviour already.
- **First-class module aliases** let an app map one import name to another module without rewriting source text.
- **Sandboxing.** A sandboxed tier would run plugin JS in a pinned runtime with capability-gated singletons, no `Qt.createQmlObject` on arbitrary text, and quotas. The architecture leaves room for it, but it is not built in v1 (D5).

---

## 17. Migrating existing Qt apps

Source: 07 (HAL-C2 as the worked example).

Many QML apps are a thin QML view over native controllers. The QML reads state from a context object or singleton, and sends actions back. Ljós supports that pattern generically, so an app can move its QML over unchanged and replace only the native side.

**What Ljós provides:**
- **`PropertyMap`**, the equivalent of `QQmlPropertyMap`. It has per-key reactivity and accepts whole-object assignment.
- **The `QmlListModel` trait.** It has roles and the full row-signal set: `rowsAboutToBeInserted`, `rowsInserted`, `rowsRemoved`, `rowsMoved`, `dataChanged` and `modelReset`. `ListView`, `Repeater` and `Instantiator` consume it, and so do JS arrays and ints. `TreeView` reads a tree model (`ItemTree`), and a list model as a tree of one level.
- **Native QML types written in Rust:** per-Engine and per-window singletons (§6.4), creatable types and enums.
- **Module registration:** native modules from Rust, and QML modules from a directory with `qmldir`.
- **Runtime creation:** `Qt.createQmlObject`, `Qt.createComponent` and `Component.createObject`.
- **The `QtTest` module (`ljos_test`),** so existing `tst_*.qml` suites keep running. It supports:
  - `TestCase` with `when: windowShown`, `init`, `cleanup` and `*_data`;
  - `compare`, `verify`, `tryCompare`, `tryVerify` and `fuzzyCompare`;
  - `createTemporaryObject` and `findChild` by `objectName`;
  - mouse, key and touch synthesis;
  - `grabImage` and `SignalSpy`.

  This is an *import path* for existing tests. New behaviour is specified in Gherkin (section 18).

  As built: `ljos test <files or dirs> [-import <dir>] [--filter name] [--json file]` finds `tst_*.qml`, runs each file in a fresh engine and prints QtTest's plain-text lines; its exit status is the number of failed tests, as Qt's is. Input goes through the scene's pointer, wheel and key paths, and every wait advances the virtual clock, so a suite is deterministic and as fast as the machine. `mise run qttest:parity` runs the crate's own suites on `ljos test` and on Qt's runner and compares the result lines. Not implemented: `failOnWarning`, `ignoreWarning`, benchmarks, and touch points told apart by id.
- **External acceptance suites** (section 18.4): an app's own `.feature` files run against Ljós through an allowlist and a ratchet.
- **A migration guide and `ljos check --port`,** which reports, from the API ledger, which types and members an app uses that Ljós does not yet support. The MCP tool `api_status` exposes the same data (section 19).
  As built: `ljos check --port [--json] [--deny-missing] [--ledger <file>] [--limit n]` reads what a project uses with the authoring server's front end and asks the same function as `api_status` what Ljós has of it, so the two never disagree. The tables of types are the source of truth; the ledger is read only when `--ledger` names one. Unit tests in `port.rs` pin its verdicts, text and JSON; the guide for app authors is `docs/MIGRATION.md`. A ledger row names one type or one member exactly — it says nothing about the other.

**What stays in the app's repository:** its native controllers (rewritten in Rust or bridged), any test doubles and fake servers, and its integration crate. Ljós's repository holds no app-specific code.

### 17.1 First consumer: HAL-C2

HAL-C2 is about 31k lines of QML over about 52k lines of C++ controllers. Its QML only reads `Shell.state[key]` and calls `Shell.dispatch(action, payload)`. On HAL-C2's side, and in its repository:
1. a Rust **shell double** for its bricks, like its existing `Shell.reset()` double;
2. a Rust **FakeMc** on tokio-tungstenite, with `hold`/`answerHeld` and a `sync` barrier;
3. the controllers ported to a shared Rust `hal-c2-core` over time (D3), with the integration crate as `apps/desktop-ljos`.

**Validation stages,** which serve as exit evidence for milestones alongside the qtdoc corpus:
- S1: bricks;
- S2: the phone feature files;
- S3: desktop `native-*.feature`;
- S4: the shared Timeline scenarios.

Each stage ends by raising that suite's ratchet (section 18.4).

---

## 18. Testing and BDD

Source: 09; also 02 §3.4, 05 §3.1, 06 §3.5.

### 18.1 Kinds of test

| Kind | Tool | Owns |
|---|---|---|
| Behaviour specs | Gherkin `.feature` + `cucumber` 0.23 / `gherkin` 0.16 | Everything Ljós promises to QML authors. Main source of truth. |
| Unit tests | `cargo test` | Parser, IR, graph, VM, bridge internals |
| Property tests | `proptest` | Binding graph against naive re-evaluation; parser print/parse round-trip; layout constraints; freeze equivalence |
| Pixel parity | `qml_compare` against Qt references, using the R backend | Corpus gallery with `pixels` and `content` scores and a ratchet |
| Ljós snapshots | kittest `snapshot` (wgpu software adapter) | Ljós-internal render regressions only, used sparingly |
| Backend parity | B0/B1 against R, with a tolerance | Drift in SDFs, gradients and tessellation |
| Performance gates | headless benches on 10k-node scenes plus device benches | sync and encode times, bytes uploaded, draw calls, budgets in section 8.5 |
| Imported QtTest | `ljos_test` TestCase runner | Existing `tst_*.qml` suites from apps that migrate |

### 18.2 Ljós's ledger: `features/`

**Directories:** `language/`, `bindings/`, `components/`, `items/`, `layouts/`, `views/`, `controls/` (one file per control), `input/`, `focus/`, `text/`, `animation/`, `a11y/`, `i18n/`, `rendering/`, `offscreen/`, `hot-reload/`, `runtime/`, `network/`, `mobile/`, `tooling/`, `parity/` (generated).

**Writing rules:**
- The user is a QML author. Words such as binding, property, delegate, anchor and signal are fine.
- Never mention egui, kittest, frames, widgets, draw calls or crates.
- Time is written "1 second passes".
- Observe through properties, the log, the accessibility tree and object counts.
- One behaviour per scenario. Use a `Scenario Outline` instead of copies.
- Reverse paths and failure paths are scenarios of their own.
- QML goes in a doc string labelled `qml`, at most 25 lines.
- Every file starts with a `# Sources:` block naming Qt *documentation* URLs or research reports, never source code.
- A `@qt-parity` file names the Qt version it was verified against.

**Tags**

| Group | Tags |
|---|---|
| Surface | `@headless` (default), `@desktop`, `@android`, `@ios`, `@wgpu` |
| Status | none = passes everywhere it applies · `@backlog` · `@backlog-<surface>` · `@dropped` (with a reason) · `@blocked` (with what it waits for) · `@priority-high`/`-low` |
| Oracle | `@qt-parity` (runs on Ljós *and* real Qt) · `@qt-diverges` (a deliberate difference, with a reason) · `@ljos-only` (extensions such as hot reload) · `@qt-pixels` (PNG reference) |
| Ledger | `@api-<Module>.<Type>.<member>` · `@a11y` |

A scenario without an oracle tag is "our assumption". `features:parity:coverage` reports the share of scenarios that are verified.

### 18.3 Runner and backends

```
crates/ljos_bdd/  World, step definitions, Backend trait
  backend/ljos.rs   kittest Harness + Engine, virtual Clock
  backend/qt.rs     generates driver.qml, runs qml6 offscreen, parses JSON from stderr
tests/features.rs   [[test]] harness = false
```

- **World and determinism.**
  - `World` holds the `!Send` harness and Engine. cucumber 0.23 requires no `Send` bound.
  - `max_concurrent_scenarios(1)` and `fail_on_skipped()` are set.
  - Output goes to JUnit at `target/features/junit.xml`.
- **Filtering.** A `BacklogFilter` wraps the parser and drops `@backlog`, `@backlog-<surface>`, `@dropped` and `@blocked`. `--backlog` inverts it to backlog only. It has to wrap the parser because `--tags` *replaces* cucumber's `filter_run` closure. The filter has tests of its own. If it fights the generics, the fallback is a runner of about 400 lines on `gherkin` plus `TagOperation`.
- **The `Backend` trait** is `load(files)`, `set`, `get`, `call`, `advance(ms)`, `click(role, name)`, `key(chord)`, `touch`, `a11y_tree`, `log`, `grab`. Steps never see egui or kittest types.
- **How steps find things.** Objects are addressed by `objectName` path. Interactive elements are addressed by **role and accessible name**. `objectName` maps to AccessKit `author_id`, so both live in one tree.
- **Time.** kittest's `run()` stops at `request_repaint_after`, so "N seconds pass" advances Ljós's `Clock` and runs steps.
- **The Qt oracle.**
  - The Qt backend interprets `set`, `bind`, `call`, `wait`, `read` and `grab` in a generated driver.
  - It runs with `QT_FORCE_STDERR_LOGGING=1`, `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`, `TZ=UTC`, `LANG=en_US.UTF-8` and `GSETTINGS_BACKEND=memory` (the desktop's theme stays out).
  - `offscreen` takes no input, so a scenario that presses keys or clicks runs on Qt's `vnc` platform and a minimal RFB client in `backend/rfb.rs` types into it. That server has no password and no bind option, so the run gets a private network namespace and a relay to a unix socket; an open port must be asked for with `LJOS_BDD_OPEN_VNC=1`.
  - A scenario takes about 0.1–0.3 s.
  - It cannot check animation midpoints, default accessible names (those need the `QAccessible` helper) or text pixels.
- **Step catalogue.** Steps are kept small, with a target of about 80. `features:lint` reports distinct steps per 100 scenarios. Worker prompts include the step catalogue, to avoid phrasing sprawl (a large Gherkin suite studied in 09 reached 13.7k distinct step phrasings).
- **`LJOS_SHOTS=dir`** saves a render after every step, which makes failures readable.

### 18.4 External acceptance suites

An app that migrates to Ljós can bring its own Gherkin suite as acceptance evidence, without Ljós importing its code.

- **Layout.** `acceptance/<suite>/` holds `PIN` (the app's repo and commit), `features.globs` (an allowlist), `RATCHET` (scenario ids that pass) and a `steps` crate that adapts the app's UI-facing steps to the `Backend` trait. App-specific step code lives in the app's repository or in that adapter crate, never in `ljos_bdd`.
- **No mass tagging.** Ljós never edits the suite's files. An app may tag scenarios that can never be ported, such as `@backlog-ljos`, with a stated reason.
- **Which steps port.** Only UI-facing steps: find, click, key, touch and read property. Steps that test the app's own native layer move with that layer.
- **Port order:**
  1. the helper layer (`find`, `click`, `key`, `touch`);
  2. one domain at a time, switching from `objectName` to role and name as the QML gains `Accessible.name`.

**First suite: `acceptance/hal-c2/`.** HAL-C2's `features/` tree, read in place at a pinned commit. About 51 of its files use `objectName`, 15 use bricks and 31 synthesise input.

**As built.** `mise run features:acceptance <suite>` reads the suite in place (from `--path`, the environment variable the `PIN` names, or the `PIN`'s own path), refuses a checkout at another commit or with uncommitted changes, and refuses a glob that matches nothing. `--raise` adds what passes to `RATCHET` and never removes a line. The format is in `acceptance/README.md`. `acceptance/example/` is a small suite inside this repository that proves the mechanism and is the model to copy; an adapter crate is a workspace of its own. No application's suite is here yet. A `features.globs` line may name scenarios (`file.feature:Name*`), and an application's own runner brings its own World and tag selection (`acceptance::main_with::<W>(dir, &Selection { skip, require })`), for a ledger run against the application itself rather than through `Backend`. DECISION: generic over the World type rather than a closure that builds one, since cucumber finds steps by World type and a `#[world(init)]` already is that closure; the scenario name is a glob after `.feature:` so one file holds both the files and the scenarios. The judging half (PIN, globs, `Ratchet`, `Selection`, `ScenarioIds`, `judge`, `conclude`) is `ljos_ratchet`, which depends on `thiserror`, `globset` and `blake3` only (its test holds the manifest to that) and which `ljos_bdd::acceptance` re-exports, so `main` and an adapter's code are unchanged. DECISION: a crate rather than a default-on feature of `ljos_bdd`: an application's runner builds three small dependencies instead of `ljos_bdd` with every backend dependency made optional, and `ljos_bdd` needs no `cfg` through its World and steps.

### 18.5 API ledger

`ledger/qtquick.toml` is generated from Ljós's type dump and qml-solid's `qmltypes` names. It has one row per type, property, signal and method.

- **Status values:** `supported`, `partial`, `dropped` (with a reason) or `unsupported`.
- **Ordering.** Rows are sorted by use across the qtdoc corpus (08) and the application corpora in `acceptance/`, so the first 100 rows are the ones that matter.
- **The rule.** Every `supported` or `partial` row is named by at least one `@api-…` tag, and `features:ledger` enforces it.
- **`parity/qtquick-types.feature`** is generated from the ledger. The default values it checks come from the Qt backend, not from hand-typed tables.

### 18.6 mise tasks and CI

| Task | Does |
|---|---|
| `check` | fmt, clippy, parse_check |
| `test`, `test:front`, `test:render` | Rust tests |
| `features [--backlog]` | Every runner; continues past failures; ok/FAILED summary |
| `features:run <globs…> [-t expr]` | One directory, file, or `file:Name*` |
| `features:all` | Sharded by top-level directory |
| `features:pick [n] [@tag] [-@tag] [globs]` | Weighted backlog picker |
| `features:lint` | Undefined or ambiguous steps, missing `# Sources:`, unverified `@qt-parity`, phrasing sprawl |
| `features:ledger` | API ledger coverage |
| `features:parity [--capture]` | `@qt-parity` on real Qt; captures `@qt-pixels` references |
| `features:android` | Test binary on an emulator over adb (to be verified) |
| `features:acceptance <suite>` | An external suite's allowlist against its ratchet |
| `corpus`, `reference`, `compare`, `dashboard` | Gallery pipeline from qml-solid |
| `oxc:check`, `oxc:diff` | Fork drift |
| `bench` | Performance gates |

**CI jobs**
1. Linux headless: `check`, tests and sharded features. A wgpu software adapter is enough; no GPU is needed.
2. Nightly: `features:parity` with Qt pinned (≥ 6.11). It also runs whenever a `@qt-parity` file changes.
3. Android emulator, from M3.
4. Performance benches.

**PR gate:** no scenario in `features/RATCHET` may regress.

**As built: the gallery.** `mise run gallery` renders the qtdoc examples listed in `corpus/examples.json` with `ljos render` (R backend, headless, virtual clock), scores each against the picture `mise run gallery:reference` made with the local qml6, writes an HTML report and holds the floors in `corpus/expected.json`. An example that does not load scores 0 with its first error. The Qt pictures are not committed, so a checkout without them has nothing to hold and the task fails until `gallery:reference` has run. `mise run qttest` and `qttest:parity` run the QtTest suites. The reference harness draws the window's colour under an example whose root is not a window, because a picture of the content item does not include it; before that was done the references were mostly black and the scores were low for that reason alone.

---

## 19. MCP: agents as first-class users

Source: 10.

Ljós ships Model Context Protocol support out of the box, for two audiences:
- coding agents that write QML for Ljós apps;
- agents that develop Ljós itself.

Both use the same addressing as tests and screen readers: role and accessible name (principle 7).

### 19.1 Protocol and SDK

- **Spec.** Target MCP **2026-07-28**. It is stateless: there is no `initialize` and no session id, and `server/discover` is mandatory. Stay compatible with clients that still speak 2025-11-25.
- **Deprecated features we never use:** Roots, Sampling, Logging, HTTP+SSE and Dynamic Client Registration. The project root comes from `--project`, not from Roots.
- **Features we use:** tools with `outputSchema` and `structuredContent`, resources and templates, prompts, completion and pagination.
- **SDK.** `rmcp` 3.5.x, pinned to the minor version. It is official, tokio-native, and its `transport-worker` runs a client and server in one process for tests.
  - All our handlers sit behind our own `McpSurface` trait. Swapping SDKs (fallback: `rust-mcp-sdk` 2.x) or absorbing a breaking rmcp release touches one file.
  - The fixed authoring server uses rmcp's macros. The inspector uses a dynamic registry, because its tool list depends on the caller's capabilities.
- **Long operations** return a `run_id`, with `run_status` and `run_cancel` tools. The tasks extension is adopted once clients support it.

### 19.2 Two servers, one crate family

| | Authoring server | Live inspector |
|---|---|---|
| Started by | `ljos mcp --project <dir>` | `ljos run --mcp[=caps]`; `LJOS_MCP=caps` or `App::inspector` in a program built with `ljos`'s `mcp` feature; or `ljos_dev::mcp::attach(&engine, cfg)` |
| Transport | stdio | Streamable HTTP on `127.0.0.1:0` or on a 0600 Unix socket; `ljos attach` relays it to stdio |
| Needs a running app | No | Yes, a dev build |
| Default rights | Read-only analysis | `inspect` only |

**Crates**

```
ljos_mcp_core    McpSurface, ToolRegistry, capabilities and tokens, size capping,
                 snapshot text renderer, ref table. No egui, no engine. Tested on its own.
ljos_mcp_author  authoring tools over ljos_compiler, qml_front, the type registry, the
                 ledger and ljos_bdd. Linked only by ljos_cli.
ljos_dev         [feature "mcp"] the inspector: a tokio listener plus a UiMsg::Devtools handler.
                 It never names egui; screenshots come from ljos_render's R backend.
ljos_test, ljos_bdd  host the inspector in-process over transport-worker.
```

**Repository config.** The repo ships `.mcp.json`, so an agent working on Ljós gets `ljos mcp` with no setup. When the target is the ljos-ui repository itself, the authoring server also registers `ledger_coverage` and `oxc_check` (feature `dev-repo`).

### 19.3 Authoring tools (`ljos mcp`)

| Tool | Does | Needs |
|---|---|---|
| `qml_check` | Parse and resolve files; structured diagnostics with optional fixes, paginated | — |
| `qml_lint` | Lints, including the accessibility audit (section 14) and i18n lints | — |
| `qml_format` | Format; returns a diff unless writing is allowed | `fs` to write |
| `qml_edit_apply` | One structured edit (binding or object) through `qml_edit`; answers the splices unless writing is allowed | `fs` to write |
| `blocks_list` | The project's functional blocks (ADR 0003) and their contracts, from qmldir `# block` comments and type dumps, paginated | — |
| `qml_apply_fix` | Apply a diagnostic's fix (dry-run by default) | `fs` |
| `type_lookup` | Type, property, signal, method and enum info, with the base chain, default property and attached types | — |
| `api_status` | Ledger status of `Module.Type[.member]` (supported, partial, dropped or unsupported), with the reason and the proving `@api-` scenario | — |
| `docs_search` | Search Ljós's docs and ledger notes; links to Qt *documentation* only | — |
| `binding_tier_explain` | Which tier an expression runs in (0, 1 or JS), why, and how to lower it | — |
| `qt_compare` | Run `qml_compare` against real Qt; scores plus a PNG `resource_link` | `run`, a local Qt |
| `features_list` / `features_run` | List scenarios by tag or path; run them through `ljos_bdd` | `run` to execute |
| `kittest_run` | Run a named test target | `run` |
| `run_status` / `run_cancel` | Poll or cancel a run: summary, failures, `LJOS_SHOTS` paths | — |
| `component_scaffold` | A component plus its `.feature` skeleton, following section 18.2 | `fs` to write |
| `i18n_extract` | Missing and stale catalogue entries | — |
| `project_info` | Ljós version, modules, ledger summary and the capabilities granted | — |

**Resources:**
- `ljos://types/{module}/{type}` and `ljos://types/{module}` (completion on both arguments);
- `ljos://ledger[/{module}/{type}]`;
- `ljos://docs/{path}`;
- `ljos://features/{path}` and `ljos://features/steps` (the step catalogue);
- `ljos://lints`;
- `ljos://project/qml-files`.

Static resources carry long `ttlMs` values.

**Prompts:**
- `write-component`: ledger first, then the feature, then QML, then `qml_check`;
- `port-from-qtquick`: `api_status` for each type used, with replacements;
- `add-accessibility`;
- `localise`;
- `write-feature`.

**As built (M1).** `qml_format` answers a unified diff and writes only with the `fs` capability; the formatter is the tool crate `qml_fmt` (it depends on `qml_syntax` alone, lays out the QML structure, copies JavaScript and comments as written, is idempotent, and is also `ljos fmt [--check] [--diff]`). `binding_tier_explain` reports the compiler's own classification and a rewrite when one lowers the tier. `docs_search` is BM25 over `docs/`, `features/README.md` and the ledger notes, with research reports weighted below the rest because this document wins over them; it passes on links to Qt's documentation and never to Qt's source. `features_list` reads the ledger, and `features_run` (capability `run`) starts a job on the Ljós backend that `run_status` and `run_cancel` follow. `ljos://features/steps` is generated from `ljos_bdd`, which is why the authoring crate depends on it. `qml_lint` and `ljos check` share one table of accessibility lints. Each prompt is tested to name only tools that exist.

### 19.4 Live inspector

| Tool | Does | Cap |
|---|---|---|
| `app_status` | Windows, size, scale, appearance, focus, visibility state, clock (real or virtual), animations under way, capabilities | inspect |
| `app_snapshot` | Accessibility-tree outline with refs; `root`, `depth`, `include`, `max_nodes`, `cursor`, `since` (delta), `pointer` (also the shown items that take the pointer and have no accessible role, as `pointer` lines), optional image | inspect |
| `app_find` | Nodes by role, name (exact, substring or regex), `objectName`, text or state. Cheaper than a snapshot | inspect |
| `app_inspect` | One node in depth: type, `ObjectKey`, source location, properties with **provenance** (assigned, bound at tier 0, 1 or JS, animated, host (what the toolkit wrote itself), default), geometry, lattice state; `dependencies` names one property and gives what its binding read, what reads it and its tier, and `bindings` gives the same for every binding written for the object (not those a type keeps for itself, such as `childrenRect`). Each end names its object as the file does (its `id`, else its objectName, else its type) and gives its handle, which is a target of its own | inspect |
| `app_binding_graph` | Sources and dependents of a property, and why it last evaluated | inspect |
| `app_get_property` / `app_set_property` | Read or write; a write is recorded as "assigned by devtools". `binding` gives a JS expression instead of a value, naming the object's properties bare and worked out again as `Qt.binding` is; it is script, so it needs `eval` as well. `keep_binding` covers a binding instead of replacing it; `revert` takes devtools' edit back (the binding returns, or the value from before). `app_inspect` gives each binding's `expression`: the agent's, or the text the file has at the binding's span when the file can be read. With `row`, `property` is a role and the cell of that row of the model the target has or shows is written as a delegate writes `model.role`, through the model's own write: a role the model lacks or a value it refuses is `not_actionable`, a row past the last `invalid_argument` | inspect / mutate (+ eval for `binding`) |
| `app_invoke` | Emit a signal or call an invokable method | mutate |
| `app_click`, `app_type`, `app_press`, `app_scroll`, `app_touch` | Input through the **same `InputRouter`, `FocusTree` and `TextInputClient` paths as tests**, never by calling handlers directly. Each returns a short delta after the next frame. `app_click` aims at an item's centre (`target`) or at a point of the window (`at: [x, y]`, logical pixels whatever the scale, the right and bottom edges not in it), a move, a press and a release there, and whatever is on top gets it; a point outside the window is `not_actionable`, both or neither of the two `invalid_argument` | input |
| `app_wait_for` | Not built. Planned: wait until conditions hold (locator present or absent, property value, text, `reload_generation`, `idle`), default timeout 5 s, maximum 30 s. Until then `app_watch` waits for a change and its `rests` say when the application is idle and where its window saw the pointer | inspect |
| `app_advance` | Advance the virtual `Clock` of an application on one (`ljos test`, an in-process host); `ljos run` is on the system clock and refuses it | input |
| `app_screenshot` | PNG of a window or node from the R renderer, so it works headless or occluded; at most 1280 px | inspect |
| `app_resize` / `app_set_appearance` | Size, DPI and orientation; light or dark, contrast, reduced motion, font scale (`SystemPrefs`) | input |
| `app_reload` | Force recompile and reload. Answers `applied` with the generation when a reload was applied since the last `app_reload` was answered, so a save the watcher applied before the call was taken up is told as the agent's own; `failed` with the compiler's message; `unchanged` only when nothing was | mutate |
| `app_alias` | `{module, dir}` imports the module from that directory (under the project, a root, or one aliased at the start) and reloads, keeping state as a reload does; `dir: null` drops the alias. Answers as `app_reload` does, with the `generation` to wait for in `app_status {after_reload}`; `app_status` lists the aliases in force under `/reload/aliases`. Needs the files watched | mutate |
| `app_logs` / `app_metrics` | Logs and JS errors with QML file and line; `FrameReport` summaries, lane times, top `ProcId`s, budgets (section 8.5) | inspect |
| `app_processes` | What does work (section 3.4, the observer): threads with state, CPU time and wakeups, running Timers and animations, scheduler owners, the supervision tree (`kind: "supervisor"`), the last faults and what took each (`kind: "fault"`, in `faults`), what will wake the UI thread (`kind: "waker"`), and the named `UiInbox`es (depth, capacity, oldest age, sent, refused, replaced, drained, senders, kinds). `kind` keeps one kind; `kind: "mailbox"` with `mailbox` keeps one inbox, `peek` names its front kinds (at most 20), never contents. The inboxes are read on the server's thread, so a stuck UI thread still shows them, with what it did not answer in `ui_thread` | inspect |
| `app_objects` | Every object below a root, items or not (Timers, models, animations, items without a role), each with a `@handle`, type, id, objectName, depth and about the bytes it holds; `depth` and `max` bound it, and an answer cut by its size gives a `next_cursor` that, passed as `cursor`, lists the rest below the same root and to the same depth. `models` lists the models instead, and `model` (a target: a model, or a view that stands for the model it shows) reads a page of its rows, at most 500; with a `holder` the model is followed, one per holder, until the holder lapses. `mark` takes the objects alive now (the script collector runs first), one mark per `holder` until it lapses and one for the rest, the next replacing it; `since_mark` lists what was made since and is still alive: the component instances, and the objects by the place they are declared or made at with their count and bytes, to find what a step that undoes itself leaves behind. `components` lists each component of every loaded file (file, inline, delegate or template) with its live instances and the objects and bytes they hold, the unused ones too; `component` (its id) lists its instances by handle, a page at a time | inspect |
| `app_trace` | Traces objects over time: from `target` on, its signals with their arguments, its property changes with the new values and its end, with the application's time, and with `bindings` each run of its bindings with its cause, cost and whether it gave the value it had (a binding written on an alias by the alias's name, though it runs as the target's); `names` keeps it to those properties and signals; a `holder` has a ring of its own (1000 events, the oldest give way and are counted) that only the objects it traces write to, read by passing the same `holder`, and gone with the holder; `since` reads on from an answer's `next_since`, `stop` ends one or all, a holder's own | inspect |
| `app_frames` | A timeline of the frames, for comparing runtimes frame by frame: from `record: true` until `record: false` (or, with a `holder`, until that client stops watching), each pass of the host that ran frames of a window's scene is kept, at most 600, the oldest giving way. Each says which window (`null` for the main one), what woke it (input, a timer, an animation, a message to the UI thread), whose requests it answered (`asked_by`, the names the clients gave in the request's `_meta` `io.modelcontextprotocol/clientInfo` or when they connected) and how many messages other threads sent, and where its time went: the passes (timers, animations, layout, creating, building the scene), the time inside bindings and laying out text wherever they ran (so they overlap the passes), and painting (render, present). With a summary of p50, p95 and the largest of each part. While nobody records no clock is read for it. With `wakes` it keeps and answers the host's wakes instead: one per turn of its loop, each with its causes and what it led to, at most 600, from `since` on. With `bindings` it profiles the bindings and handlers while recorded and lists them by `sort`, with where each is written, its runs and times and what set it off, beside the binding loops met. With `growth` it samples each component's instances and bytes while recorded and marks the ones that keep growing. With `wakes` and `wake` (a wake's number) or `last_input` it lists what one wake did, in order: the handlers and bindings that ran with where each is written and its time, the properties that changed with their old and new values, inside the runs that changed them, and the items the scene laid out, placed and redrew. While wakes are kept the engine and the graph journal each turn (at most 2 000 runs and 4 096 changes a turn, the rest counted) and the latest 16 wakes that were not an agent's own requests keep what they did; nothing is journaled while nobody keeps wakes. With `export` it writes what is recorded to a trace file of that name. One of these at a time | inspect |
| `app_metrics` with `memory` | Where the application's memory is, as an allocator view shows it: the objects by type (count and about their bytes, the largest first; `type` keeps one, `max` bounds the list), the script heap's bytes as its allocator counts them, the reactive graph, the text engine's caches, the image cache, and per window its scene's tree and display lists and what its renderer keeps (meshes, glyph atlas, textures), beside the process's resident bytes. Running counts are read and the objects walked once; nothing is kept up to date for it. Bytes no allocator counts are estimates and say so (`approximate`). `owners` gives the same by owner, each with bytes and a count (JS heap, objects and items, reactive graph, scene nodes, text layouts, glyph atlas, meshes, image cache, model storage, compiled units, then what crates built on the engine tell of through `Engine::add_memory_owner`: `media`, the pictures players hold decoded and outputs show), then the program and its libraries, what the allocator keeps free and what is not attributed; `reconciled` sums them against what is resident. With `mappings` it reads the process's mappings once (on Linux `/proc/self/smaps`, on macOS a walk of the task's regions with `mach_vm_region_recurse`; elsewhere "not known") and splits the program and its libraries by file (the program itself, the JIT compiler's library, the graphics driver named by the painter's adapter, fonts, the rest) and what is not attributed by where it lies (allocated that no owner counts, split into what Rust and what C code allocated when the `ljos` binary counts its own heap, data libraries wrote, code compiled at run time, the stack, mappings outside the allocator). Textures are told, marked `gpu`, and not summed A desktop host answers it after its next pass, when it can look at every window; a host that draws nothing has `render: null` | inspect |
| `app_watch` | Waits until frames, drawn passes, log lines, reloads, traced events, kept wakes or changes of a followed model move past the counts given as `since`, at most an hour; `settle_ms` gathers a burst into one answer. Named in `topics`, `rests` counts the times the UI thread went to sleep saying something new, and every answer's `rest` says what it last said: when it wakes by itself next (`wake_in_ms`, `null` for not until input or a message), whether the primary window still owes a frame (`frame_owed`: a pass of it due within a frame, or asked of the window system and not yet run) and the pointer as the primary window's last pass saw it. Sleeps on the server's thread and wakes nothing in the application | inspect |
| `app_eval` | JS in the engine context under a `JsHost` deadline; result wrapped as `{value}`, at most 64 KB | eval |
| `app_batch` | A list of steps in one call, stopping at the first error | per step |

**Resources:** `ljos://app/tree` (subscribable, updated on reload or a change in tree structure), `ljos://app/logs`, `ljos://app/metrics` and `ljos://app/source/{path}`. The inspector also serves the authoring resources, so one connection is enough.

**Prompts:** `debug-binding`, `fix-layout` and `a11y-walkthrough`.

**Snapshot format.** The model sees an indented outline. `structuredContent` carries the same data for machines.

```
- window "Notes" [e1] 1280x800
  - toolbar [e2]
    - button "New note" [e3] id=newBtn
    - textfield "Search" value="" [e4] focused
  - list "Notes" [e5] items=240 visible=12..24
    - listitem "Groceries" [e6] pos=13/240
    - button "Delete" [e9] disabled
```

- Roles are AccessKit role names, and names are accessible names.
- `id=` and `obj=` show the QML `id` and `objectName`.
- Virtual lists show only their window and do not materialise every row.
- Hidden and frozen subtrees are summarised, for example `(hidden: 18 nodes)`.
- With `pointer: true`, a shown `MouseArea`, `Flickable` or item with a pointer handler that the tree has no node for is a `pointer` line, named by the text in it. Its reference is the item's, so the click and touch tools take it; the accessibility tree is not changed.
- Unnamed actionable nodes are flagged `⚠unnamed`, which nudges authors towards `Accessible.name`.

**Addressing**
- **Refs** (`e9`) are short and valid for one snapshot epoch. A stale ref returns `stale_ref`, or resolves itself when its role and name are still unique.
- **Locators** are durable across hot reload (section 12.1 identity rules). They use the widely known Playwright role-selector subset:
  - `role=button[name="Delete"]`, `text=…`, `#objectName` and `ref=e9`; `#name` names the object with that QML id when no object has that object name, so a window written with only an `id` is named as `#app`;
  - chaining with `>>`, plus `nth=` and `name~=regex`;
  - `at=X,Y`, a point of the window in logical pixels: the items drawn there, topmost first (the item painted last at the point comes first). It is answered from what the last frame drew, not from the outline, so items with no role are found too, and an item that is not visible, has opacity 0, or is clipped away at the point is not. A tool that needs one item takes the topmost; `at=X,Y >> nth=1` is the one under it, and no other step goes with `at=` (`invalid_argument` says why). `app_find` lists the whole stack, each with its reference (or handle, when the outline does not list it), type and source place. The source place is the item's own declaration: an item inside a component file is in that file, the root of a component file and an instance of an inline component are where they are used, and the copies a `Repeater` makes are where its delegate is written. A point outside the window is `not_found`.
- **Handles** (`@12.3`) name any object, item or not, as `app_objects` and `app_processes` give them: the place and generation of its key, so no table is kept, the handle holds for as long as the object lives and is `stale_ref` once it is gone. Every tool that takes a target takes one.

**Errors** are tool results with `isError` and a short code:
- `stale_ref`, `not_found`, `ambiguous` (with candidates), `busy` (retry);
- `not_actionable` (disabled, hidden or off-screen);
- `timeout`, `capability_denied`;
- `app_faulted`, with the `Fault` from section 9.3.

Actions never quietly fall back to coordinates.

### 19.5 Fit with the runtime

- **Message path.** The listener runs on the existing tokio workers. A request becomes `UiMsg::Devtools(DevReq { kind, reply: oneshot, deadline })` through a bounded `UiHandle`. A full inbox returns `busy`, and at most 16 devtools messages may be queued, so agents cannot starve real input.
- **Lanes.** Input synthesis runs in the **Input** lane. Snapshots and queries are **Deferred** jobs that yield (`JobPoll::Yield`), so a 10k-node snapshot is spread over frames and never blows one.
- **Idle.** No timers and no polling. A request sets the wake flag. Subscriptions push only from events that already happen. **An idle app stays at 0 % CPU,** and with no client attached there is no MCP work at all.
- **Accessibility tree on demand.** The a11y pass normally runs only for an active assistive technology. While a request is being served the inspector counts as one, and is released afterwards.
- **Determinism.** On a virtual `Clock` (`ljos test`, an in-process host), `app_advance` drives it. A BDD scenario and an agent session on the in-process transport have the same semantics. `ljos run` has no virtual clock.
- **As built: driving from outside.** Input a client sends through the compositor (a virtual pointer) reaches the application on another socket than the inspector's, so nothing orders a read after it. The desktop shell tells the inspector each time its loop goes to sleep when it next wakes and where the primary window's last pass saw the pointer, stored only when that differs from the last, and `app_watch`'s `rests` waits for it on the server's thread. The `@desktop` BDD backend (`ljos_bdd::backend::desktop`) waits that way after each pointer step, and for no pass due before a reading whose cost it measures and before a picture; `N milliseconds pass` returns once the application sleeps with no wake of its own due within them. `scripts/gallery-drive.py` waits for each grid click the same way. DECISION: `rests` is not among the topics a watch with none waits for: a rest follows every turn that did anything, which the other topics already tell, and `ljos observe`, which follows them all, would be answered twice as often for nothing new. DECISION: `N milliseconds pass` is waited out on the real clock while the scenario counts passes, work or thread wakes, or runs `ljos observe`: those measure threads and a process whose timers no rest tells, and `features/tooling/observe-idle.feature` is forty real seconds by design. DECISION: no `--test-clock` for `ljos run`: the desktop scenarios measure the real loop, and with waits on conditions a real clock costs them only the time they count. DECISION: a picture is taken once the window's picture has not changed for 100 ms after the application rests with no frame owed, which covers the compositor's own frame; the compositor says nothing an inspector could wait for. A rest alone was not enough: under load the window system lets a window draw late, and the UI thread rests meanwhile with only that stall's wake due, so a window drawn on the processor was taken still black in 2 of 5 runs at a load of 60 (`features/window/cpu-drawing.feature`).
- **As built (M1, read-only).** `ljos run --mcp` serves `app_status`, `app_snapshot`, `app_find`, `app_inspect`, `app_screenshot`, `app_logs` and `app_metrics` and the resources `ljos://app/tree`, `ljos://app/logs` and `ljos://app/metrics`. Input, mutate and eval tools are not registered and the capability check refuses them. The server has a thread of its own that sleeps until a client writes; it reaches the Engine through an inbox of 16 that is its own, not a `UiMsg` variant, and wakes the UI thread once per call. Capture is one step; only rendering the outline yields. Screenshots are drawn by the R backend on the UI thread. The transport is HTTP on a loopback port of the system's choosing; the token is in `target/ljos/mcp-<pid>.json` below the directory `ljos run` started in, and a process killed by a signal leaves the file until the next start there. The `mcp` feature is off by default, and a test proves the default shell links no `rmcp`.
- **As built: `top`.** `ljos run --top` shows the last frame's time per lane and its owners. The host hands the overlay the report of each frame the scene ran anyway, so it asks for no frame and holds no timer; hidden, it records nothing. Lanes cover scheduled jobs (creation, inbox, inspector), not the synchronous binding, layout and paint stages, which show in the frame time only.
- **As built (M2): input, mutate and eval.** `--mcp=inspect,input,mutate,eval` grants capabilities at launch and the server refuses what was not granted. `app_click`, `app_type`, `app_press`, `app_scroll` and `app_touch` are made into the events a device sends and given to the scene's router; `app_set_property` and `app_invoke` go through the engine as a script's write and call do; `app_eval` runs in the JS host with a time limit. Each act answers once the frame has settled, with the difference between the outline before and after. `app_batch` runs steps until the first error, `app_inspect` reports what a property depends on, and `app_reload` flushes the file watcher and answers `failed` with the compiler's message when the files do not compile. A client waiting for a reload sleeps on the reload feed and wakes nothing. `app_alias` takes the same slot as `app_reload` (one at a time; a second is `busy`), so a failed build after a switch answers `failed` with the compiler's message and keeps the old roots, as any failed reload does; a directory outside the project is `path_denied`, one without the module `invalid_argument`. `--mcp-bind`, `--mcp-insecure-lan` and a 0600 Unix socket are in the CLI; a socket path past the platform's limit is refused before a window opens.
  - `app_eval` names the application's singletons by their type, in an object and in the global scope, as code in its files does. A `pragma Singleton` file and a registered native singleton both count. The script was never compiled, so the name is looked up when it is read. It is tried in this order: the type a loaded file resolved the name to (the target's file first), a native singleton of the name, then a singleton file of the name in a loaded file's directory.
  - `app_eval` and `app_invoke` are refused under `--mcp-redact`, and `app_eval` for targets with a hidden echo mode, but without `--mcp-redact` a value derived from a redacted one can still be read by script or a method. A secret that is part of a value (within a longer text, a record, a script's object, an expression) is masked where it occurs in `app_inspect` and in what a script, a method or a set answers, and a key of what the application answers is masked too; two keys that mask alike are numbered (`<redacted>#2`) rather than merged. A window's title, an item's accessible name and description, its objectName and its id are masked where a secret is part of them when the scene is captured, and again as `app_status`, `app_objects` and `app_inspect` tell them. `app_screenshot` and `app_snapshot` with `image` paint over each field that hides its text and each exposed item whose value, name or description was masked, at its bounds in the window; an item that shows a secret but is not exposed to accessibility is not painted over. A field's `selectedText` and `preeditText` are secrets as its `text` and `displayText` are (one list hides them in the field and learns them), masked longest first so a part of a secret does not split the whole.
  - `app_logs`, `ljos://app/logs` and the faults `app_processes` lists are masked with every secret a look at the scene found (an act, a snapshot, or the request itself), kept after the field changes or is cleared. A value a field held only between two looks, logged or thrown and changed before the next, was never seen and is not masked.
  - `app_resize` on the desktop asks the platform to give the primary window the size, held within the window's `minimumWidth`…`maximumHeight` first (a platform is told those limits but need not hold a size asked of it to them; sway's floating windows do not), as a `Window` whose size the application wrote does, and answers in the pass that lays the scene out at it, or after a second with the size the platform gave instead (XWayland takes longer than the shell's 250 ms size check); the scene is then laid out as if the user had dragged the window's edge (`features/tooling/mcp/resize-live.feature`). A host with no window (the headless runner, an eframe application) sizes the scene alone. `app_advance` needs the virtual clock.
  - Only the primary window is inspected; `app_status` says so and gives the number of windows.
- **As built: headless.** `ljos run --headless` (`ljos_shell::run_headless`) runs the application through a loop of its own (`headless.rs`) with no `ljos_host`, no winit and no painter, so it needs no display. The root is laid out at `--size`, else at its own size, as `ljos render` lays it out, and a `Window` is an item of the scene, as without any host. The loop sleeps on a condition variable that egui's repaint request, the view's waker and the loop's own next wake set; each pass runs the scene, drops what egui would have drawn, and then settles as the desktop loop does: it serves `app_resize`, prepares scripts, trims the heap and collects at rest, and tells the inspector when it next wakes. The first pass only lays out and starts the timers, and the inspector's queued requests wait for the second, so an agent's first click lands on a laid-out scene. The platform is the desktop one without what needs a session: no system clipboard, dialogs, URL opener, badge or screen reader; settings, fetching and the network stay. `Qt.platform.pluginName` is `offscreen`, as `qml6 -platform offscreen` reports. `app_advance` is refused, since the clock is the system's as on a window. Measured from a release build, a 400×300 scene with `--watch --mcp`: the first `app_status` is answered 33 ms after the process starts, a save is reported by `app_status` `after_reload` at a p50 of 16.5 ms and a p95 of 16.8 ms over 20 saves, the process uses 0 CPU ticks over 5 s idle, and RSS is 87 MiB after start. Not built: streaming the frames out (the pass marks where they would be taken), and memory-pressure handling.
- **As built: drawing the window.** The desktop shell tells the inspector what the scene cannot see. `app_status` gets `painter` (gl or wgpu, the backend, the adapter and driver, the present mode, and whether a pass redraws only what changed) and `startup`: when the application was built, the device was ready, the first frame was presented and the first key, button, touch or wheel arrived, timed from when the shell started. `app_metrics` gets `paint`, for the primary window: passes presented (whole or in part, and of those how many moved the pixels of a scrolled clip on screen) and found unchanged, the cost of building the picture and of uploading, drawing and presenting it, the pictures uploaded and their bytes, and the time from when the repaint a pass answered was due to its present. Uploading, drawing and presenting are timed apart where the painter allows: GL times all three (as time on the UI thread; the device works on after a call returns, and what it still had to do is waited for in the present); on wgpu the shell hands the pictures to egui-wgpu's renderer itself (`update_texture`, only in a pass that has new ones) and times that as `upload`; egui-wgpu then draws and presents in one call and times only the waits inside it (for the surface's next texture, the submit and the present), so wgpu gives `present` as those waits and `draw` as the rest. The device's own time would take timestamp queries. `recent` spreads the last 240 frames (the engine's frame, the pass that painted it, and both) as p50, p95 and the longest, and `slow` logs the last 32 frames that went over budget (a frame of the primary window's display, or past the engine's deadline), each with its lanes, its painting and its cause: the lane, the engine's frame outside its lanes or the painting step that took the most, with the owner charged the most. Without an inspector nothing is measured, and nothing is kept while no frame runs.

- **As built: processes, mailboxes and `ljos observe`.** `app_processes` is answered on the UI thread, which owns the timers and animations; the thread list comes from one snapshot of the process a platform (`crates/ljos_dev/src/mcp/process.rs`: `/proc/self/task` read with procfs on Linux, libproc on macOS, Win32 on Windows), so it costs nothing until asked. Its mailboxes are added on the server's thread from `ljos_runtime::observe`: `kind: "mailbox"` does not wake the UI thread, and when the UI thread times out or is busy the answer still holds the mailboxes, as Erlang's observer shows a stuck process's queue. They were a tool of their own (`app_mailboxes`) until the tool budget wanted headroom. DECISION: the supervision tree and the faults are two kinds of `app_processes` too, not a tool of their own: they are what Erlang's observer lists beside processes, and the tool list stays inside its budget (24 tools). The counters move while an `Inspector` exists (it holds an `ObserveGuard`); with no inspector a send pays one relaxed load more. `ljos observe [target]` finds a token file (`target/ljos/mcp-*.json` below a directory, or the file itself) and opens a window with Overview, Processes, Mailboxes, Objects, Logs, Trace, Supervision, Frames, Memory, Models, Wakes, Components, Bindings and Growth pages, light or dark as the system is. The Supervision page is the tree above the faults, newest first; a boundary, the object a fault happened in and the boundary that took it lead to the Objects page while they live, and each fault opens to its stack. It asks again only when the watch's `logs` topic moves. Its client is a native object of the `ljos` binary that posts each call to the inspector's port or Unix socket from one long-lived `ljos-inspector-client` thread (`ljos_mcp_core::remote`, behind the crate's `remote` feature: a current-thread tokio runtime with hyper's HTTP/1 client, made at the first call, at most 16 calls in flight) and answers on the Engine thread through a `Reply`. `ljos attach [target]` puts the same client behind stdio, for an MCP client that starts its servers as programs: one JSON-RPC message a line, each posted as it comes so that a waiting `app_watch` holds back nothing. Any other program that drives a running application (an editor's live preview, an IDE plugin) links the same library: a `remote::Client` reads the token file again for every call, so an application started again is reached again; makes the `initialize` handshake once; and gives back a tool's whole result (the structured value, the text, the pictures as MIME type and bytes) or its refusal with its code and details. `BlockingClient` runs those calls on that same thread for synchronous code, so a caller on another runtime's thread neither enters nor blocks it. `wait_for_token` waits for a process just started to write its file: looks that double from 2 ms to 100 ms, for as long as asked, and an end as soon as the process has ended.
  - **What an answer costs the observer.** A thread per call (`Platform::request`) was born, woke and ended for every answer: 189 in a minute against a ticking target. The `ljos-observe` thread sleeps in its reactor with no timer and takes about 2.5 wakeups a call. The observer's UI thread takes 2 wakeups an answer on Wayland; on X11 (Xwayland in the sandbox) it takes about 23, before and after, in winit's X11 loop, and that is not the request path.
  - **Live without a clock.** The observer keeps one `app_watch` call waiting (an hour, settle 1 s) and reads the open page again when it answers; a target it cannot reach is tried again after 2 s, doubling to a minute. The watch counts what the host does anyway (a frame run, a pass drawn, a line logged, a reload) in one cell the server's thread sleeps on, so the target's UI thread is not woken by it: measured on a resting target with the observer attached, 9 wakeups in 35 s, all on `ljos-mcp`, and none on the UI thread. A busy target is read at most once a second; an over-eager client is answered `rate_limited` past 20 calls a second. DECISION: a watch waits at most an hour, not 30 s: at 30 s the observer and the target each did a round trip twice a minute while nothing changed, which `features/tooling/observe-idle.feature` now forbids over 40 s, counted through the inspectors (frames, passes, jobs, messages, mailbox sends) rather than by thread wakeups, which the AT-SPI ping below would spoil. The time a holder has to ask again stays at most 30 s, so traces a gone client held do not outlive it by an hour. A call whose client went away (its connection closed, or it cancelled) is dropped there and then, so a watch it was waiting on ends unanswered and what its holder held goes at once, not after the next change and that 30 s. DECISION: the MCP glue drops the call when rmcp cancels the request, rather than each tool checking: a tool's future is all a call holds.
  - **Objects** is the tree from `app_objects`, asked in parts by `cursor`, filterable, and asked again when the object count changes. An application with more objects than the tree holds (3000) is shown two levels deep, and a row whose children were left out is opened by hand, as far down again; what was opened is opened again after a reload. An object without id or objectName shows its handle, and so does every row of a filter. An edit keeps the binding below it (`keep_binding`) and can be taken back, and the provenance shows bound, assigned, animated, host or edited. A property that cannot be written is shown read-only. A Bindings box lists the picked object's bindings under its values, each with its tier, what it read and what reads it, from `app_inspect` `bindings`; it is asked with each refresh while it is ticked, and not at all otherwise. Each end that is an object is a link that picks it, by its handle. Each row says about the bytes its object holds (`app_objects` `bytes`); "largest first" lists the rows by that instead of as a tree, and a filter `>4k` (or `>512`, `>1m`) keeps the objects that hold at least that much. Both work on the rows already asked for and ask nothing more.
  - **Memory** is Erlang's allocator view: a bar for each part `app_metrics`' `memory` counts (objects, script heap, reactive graph, text caches, images, scenes, renderers) to one scale, the objects by type, and each window's scene and renderer, under the process's resident and own bytes. It is asked when the page is shown and when a pass is drawn, at most once a second as the watch settles, and never while another page is shown. DECISION: the type list asks for 200 types; an application with more says how many smaller ones were left out.
  - **Memory by owner.** Above the bars, a chart stacks `owners` over the last 120 samples, each at the time it was asked, so the bands add up to what is resident; a legend gives each owner's bytes and count, and the header the part not attributed. Resident = the program and its libraries (shared pages) + the owners + what glibc's allocator holds free (`mallinfo2`) + not attributed: other libraries' heaps (Mesa, llvmpipe's JIT, fontconfig), thread stacks and the driver's maps. Measured off-screen on llvmpipe with a debug build, 8 s after start: clocks 43.0 MiB not attributed of 243.5 MiB resident (17.6 %), calqlatr 43.4 of 246.6 MiB (17.6 %), samegame 44.4 of 250.1 MiB (17.7 %); the program and its libraries are 194–198 MiB of each, the owners 3.4–5.5 MiB, and the allocator says about 52–55 MiB of its heap is in use, so most of the gap is heap that other libraries allocate (Mesa and llvmpipe above all). Measured at rest with the page open: 0.1 wakeups a second and no CPU time in either process over 20 s. DECISION: owners are a part of `app_metrics`' `memory`, not a tool: it is the same question by another cut, and the budget stays 25. DECISION: textures are listed apart and left out of the sum, since where there is a GPU they are not in the process's memory, and on llvmpipe they are counted twice otherwise (the picture kept and its texture). DECISION: the allocator is read, never changed: `mallinfo2` is called only when asked, and only on glibc; elsewhere the free part is not told and falls into not attributed. DECISION: compiled units count each loaded file's IR as it serialises, its bytecode and its tier-0 code, a floor rather than what the heap holds for them; model storage counts a ListModel's own rows and, for a model written in Rust, rows × roles × a value's size, an estimate that says so.
  - **Models** is Erlang's Table Viewer: the models from `app_objects` `models` on the left, and a page of 50 rows of the one picked from `model`, with Previous and Next; a cell of a writable model is edited in place through `app_set_property` `row`. The model picked is read with the window's holder, so it is followed: a change of it moves the watch's `models` count, which only this page waits for, and the page is read again then. Nothing is subscribed to a model until it is picked, and the subscription goes when the holder lapses or picks another. The engine notes which model each view was given (a weak side table, forgotten when the view goes), so a number or array, which has no object, is found by its view; an object model is found by walking the objects once, when asked. DECISION: models are a mode of `app_objects`, not a tool, to keep the budget at 25; a model is an object or belongs to one. DECISION: a cell is written through the model's own write (`ItemModel::set_data`, what a delegate's `model.role = v` does), not `setProperty`'s convert-and-warn, so a role the model lacks is refused, not added; only a ListModel says it is writable. DECISION: one followed model per holder, at most 16 holders, and a change counts once (the `RowsAboutTo…` notices are skipped). Each model, and each view that shows it, says where it is declared (`at`, `shown_by`): an XmlListModel or a FolderListModel is a model written in Rust behind its object and is listed as one, read-only. Each row of the list names its views and has a link that opens the first on the Objects page; the page of rows has a button for each. DECISION: picking a row shows its rows, as before, and the view is opened by its own link: the table is what the page is for, and the view is one click further. Measured off-screen at rest with the page open and a model picked: 0.1 wakeups a second and no CPU time in either process over 20 s.
  - **Wakes** says why the UI thread woke, for 0 % CPU when idle. The shell notes, per turn of its loop, what came in (input, window events, platform and requested redraws, inspector, thread, posted and accessibility messages, a deadline, dirty state) and what the settle did (passes, frames drawn or unchanged, redraws asked); the scene's frame adds whether a timer, an animation or input ran. Nothing is gathered while no client records: one flag check a turn. `app_frames` `wakes` keeps the last 600 while a holder records, as the frames are; the page shows them as a strip of the last 10 s coloured by cause and a list, beside the running timers and animations from `app_processes` with when each is next due. A wake only an agent's requests caused, and the loop's turn after it within 250 ms, is kept but moves no `wakes` count, so the observer does not wake itself; the page hides them unless asked. The test host runs a pass each turn, so its wakes all lead to frames. Measured on a resting target with the page open: 0.1 wakeups a second in the target and in the observer, not recording, 0.2 recording, 0 CPU ticks in 20 s; detached, the recording lapsed with the holder and the target was back to the AT-SPI ping below (0.2 a second, 1 tick in 20 s).
  - **Why awake** is what will wake the UI thread next, the 0 % CPU rule made visible: `app_processes` with `kind: "waker"` lists the running Timers (interval, repeat, next firing), the running animations (a paused one is not), the answers the platform waits for (each `Platform::expect_for`, `fetch_for` and `request` names what it waits for and the object it is for: a file read, a fetch, a dialog, a permission, a decode, a stream), the pictures loading (queued or decoding, and for which items), and the messages queued in a named inbox, each with its owner's handle, locator and file:line. Scheduled rows are counted apart from rows that only listen for news that may never come: the QML file watcher and the screen reader's standing question. An application at rest lists nothing scheduled, and a Timer that stopped or fired its single shot is gone from the list. The Wakes page shows it as its Why awake panel, asked again on each of the application's frames as well as when the page asks for wakes (the watch counts wakes only while they are recorded, and a Timer started by a click shows only on a frame); a row with an object opens it on the Objects page. Measured off-screen at rest with the page open: 0.3 wakeups a second and no CPU time in either process over 20 s. DECISION: a kind of `app_processes`, not a tool: it is the timers and animations that tool already lists, with more sources, and the budget stays at 25. Like `supervisor` and `fault` it is listed only when asked for by name. DECISION: what listens is kept apart from what is scheduled, so that "nothing will wake the UI thread" stays true of an application that watches its files. DECISION: the inspector's own inbox is left out: it holds the asking client, not the application's work. `XMLHttpRequest` goes through `Platform::request`, which names its method and URL. Streams that post on their own (sensors, desktop settings) promise nothing and are not listed: they are news, like input.
  - **Components** is what is loaded: every component of every file (the file, its inline components, its delegates and other `Component` contents), how it is written, its live instances and the objects and bytes they hold, the unused listed with none, from `app_objects` `components`; a delegate is named by where in its file it is (`'list'.delegate`), so two of one type differ. The engine finds the instances from the `Instance` each made object keeps on its root, walking the objects once when asked; nothing is kept for it. Picking one lists its instances (`component`, 50 at a time), each opening the Objects page on it. It is asked when the page is shown and when a pass is drawn or a file reloaded, at most once a second as the watch settles. Measured at rest with the page open: 0.2 wakeups a second in each, 0 CPU ticks in 20 s. DECISION: components are a mode of `app_objects`, as models are, to keep the budget at 25.
  - **Bindings** is a profiler of bindings and handlers. While a client records (`app_frames` `bindings`, `record`, held as the frames are) the engine times each run of a binding and of a handler where it runs (`binding_wrapper`, `connect_handler`): its runs, its total time, its own time (less the runs nested in it), its longest run, and what set it off, found by comparing the sources it reads now with the reads kept after its last run; the first run recorded has none to compare with. It keeps 20 000 runs for a trace file and tells at most 10 000 bindings apart. While nobody records a run pays one flag check. The binding loops the graph breaks are kept whether recorded or not, the first 32, each with the property, its place and the way round, and a count of the times it was met. The page is a table sorted by a column heading (total, own, runs, longest), a bar under each row for its share of the top one, and the loops beneath; a row or a loop opens the object on the Objects page. It is asked when the page is shown and when a frame runs, drawn or not, at most once a second as the watch settles: a binding can run in a frame that changes no pixel. Measured on a resting target with the page open (after the cursor overlay of the off-screen desktop settled): 0.8 wakeups a second in the target (all on its inspector and `async-io` threads, none on its UI thread) and 0.1 in the observer, recording, 0 and 0 not recording, 0 CPU ticks in 20 s. DECISION: hot bindings, growth and trace files are modes of `app_frames`, which already holds a recording a holder keeps, to keep the budget at 25; one mode at a time, `sort` only with `bindings`, and no `record` with `export`. DECISION: the binding loops are kept always: they are found at start-up, before any client could ask, and 32 bound what they cost.
  - **Growth** samples each component's live instances and bytes (from `components`) while a client records: at a pass of the host at most once a second, at each answer and when recording starts, never by a timer, so an application at rest is not woken for it and shows a flat line. A run of equal samples keeps its first and last; at most 300 are kept. A component keeps growing when its count rose at least twice and never fell, and ends above where it began: what an action repeated leaves behind each time. The page is a chart per component, the growing ones first and red, with first → last, rises and falls and bytes. Measured at rest with the page open: 0.1 to 0.2 wakeups a second in the target (none on its UI thread) and 0.1 to 0.2 in the observer, recording or not, 0 CPU ticks in 20 s; detached, the sampling and the profiling lapsed with the holder and the target was back to the AT-SPI ping below (0.2 a second, 0 ticks in 20 s).
  - **A trace file to keep.** `app_frames` `export: "name"` writes the frames, wakes and runs of bindings recorded as a Chrome trace-event JSON file (`traceEvents`, one event a line), which Perfetto and `chrome://tracing` open: one process with a track each for frames (each frame a slice, its passes laid one after another inside it, render and present after), wakes (instants) and bindings and handlers (slices nested as they ran, named `Type.property`). Times are in microseconds from the earliest event, both ends of a slice rounded so that it stays inside the one it lies in. The Frames page has a Save trace button. DECISION: the application writes the file, below the directory its host names for traces, under a plain name the client gives (letters, digits, `.`, `_`, `-`, at most 64, `.json` added), written beside and renamed so a reader never sees half of one; a host that names no directory answers `unavailable`. A recording is up to 600 frames, 600 wakes and 20 000 runs, more than an answer should carry, and a client that is an agent has no file of its own to put pages into; a name and never a path keeps `app_frames` an `inspect` tool that cannot write anywhere else. DECISION: the shell names `traces` in the application's per-user state directory, as `StandardPaths.StateLocation` has it (on Linux `$XDG_STATE_HOME/<organization>/<name>/traces/`, by default below `~/.local/state`), found when the file is written so that the name `Qt.application.name` gives counts; it was `target/ljos/traces/` below the working directory, which put a trace wherever the application was started from, a read-only directory included. State, not cache: a trace is kept as a log is, and a cache may be emptied under it. A scenario's application writes below its own sandboxed home. DECISION: the passes of a frame are laid one after another, in the order they run: only their durations are recorded.
  - **Trace** is Erlang's Trace Overview. The engine tells a sink what a traced object is told: its signals where they are emitted, before any handler runs, so what a handler emits in turn comes after; its changes through the change signals, as an `onXChanged` would see them (a group and a property made on first read are left out); its end through `destroyed`, after which it is traced no longer. While nothing is traced an emission pays one look at a flag. The inspector writes each event down as JSON when it happens, so it holds no value alive and a field that hides its text has its strings masked as they were, not as they are when read; it keeps the last 1000, counts what gave way and numbers each so an answer reads on from `since`. Tracing is read-only (`inspect`); at most 16 objects are traced at once, each costing a connection per property. The observer starts a trace from the object picked and follows the list through the watch's `traces` topic. Each client that names a holder reads a ring of its own: an object's events go to the ring of each client tracing it whose `names` they match, and to no other, so two clients tracing one object are each told what they asked for, and a client that stops leaves the other's trace in place. A holder's ring goes when the holder lapses; a stop it asks for keeps what was written. Traces without a holder share one ring, as before. The observer reads with its holder and can keep a new trace to names typed on the Trace page. DECISION: one ring per holder rather than a cursor per client into one shared ring: a busy object another client traces cannot push a client's events out. DECISION: `names` filter in the sink, after the engine has told it: the engine's trace stays one connection per property, and the cost of tracing stays with whoever traces. A ticking target with two objects traced and the observer on the Trace page took no CPU time once it stopped ticking.
  - **Frames of every window, and whose.** The shell times a secondary window's scene exactly while a client records (before its pass, so its first frame is timed) and meters its paint into the timeline only: the paint totals of `app_metrics` stay the main window's. The engine's and text engine's clocks are one per thread, so binding and text time land in the frame of whichever scene runs next; a second scene timed leaves what they gathered for the first. The observer names itself in each call with its holder name, so the Frames page hides the frames that only its own requests woke (no input, timer, animation or thread message, and every asking client was this window), with a toggle to show them. DECISION: the client's name comes from the MCP request's `_meta` clientInfo, which a client without a session must send, rather than an argument of every tool: no tool's arguments change, and any client is told apart, not only the observer. DECISION: per-window timing has no scenario: the headless backend has one scene; it is checked by a unit test of the timeline and by driving a two-window application off-screen.
  - **Back inside the tool budget.** Where the memory is was a tool of its own (`app_memory`) and made 26; it is now `app_metrics` with `memory` (`{"memory": {"type", "max"}}`), so 25. DECISION: memory folds into metrics rather than frames into something else: `app_metrics` already gave a memory summary, the breakdown is the same question asked in more detail, and `app_frames` holds a recording that a holder keeps, which no other tool has.
  - **Where a platform measures less.** One module owns the process (`crates/ljos_dev/src/mcp/process.rs`): a `Process` snapshot of its threads, resident bytes and CPU time, one implementation a platform, on the crates `procs` reads them with, so no `/proc` file or Mach structure is parsed by hand. They are dependencies of the `mcp` feature on their platform only. On Linux procfs reads `/proc/self/task/*/{stat,schedstat,status}`, `statm` and `stat`. On macOS libproc: each thread's name, run state and times from `proc_pidinfo(PROC_PIDTHREADINFO)` (its id from Mach's `task_threads` and `THREAD_IDENTIFIER_INFO`, which libproc does not wrap), the resident size and the footprint from `proc_pid_rusage` (`ri_resident_size`, `ri_phys_footprint`), the virtual size and the CPU time from `PROC_PIDTASKINFO` (in Mach time units, converted with `mach_timebase_info`), the process's wakes from `ri_interrupt_wkups`. A thread's wakes have no counter there, so they are null, never 0. The mappings are a walk of `mach_vm_region_recurse` (grouped by the region's file, from libproc's `regionfilename`, else its tag), and the allocator's heap is `malloc_zone_statistics`. libproc's build runs bindgen on the macOS SDK, so a Mac build needs libclang. A platform that measures no process (`ProcessSource::Unmeasured`, and the stub on other systems) answers `threads: null` with `not_measured` naming what is missing, and its text says "not measured on this platform" (`processes-unmeasured.feature`); the observer shows that line where a value would be, in the Overview's tiles and charts and the Processes page's header and table, and never a 0 or a NaN it did not measure. The macOS and Windows code is checked with clippy for `aarch64-apple-darwin` and `x86_64-pc-windows-gnu` and has unit tests that run only there; neither has run on its platform yet.
  - **Windows, the process only.** The threads: a Toolhelp snapshot (`CreateToolhelp32Snapshot` with `TH32CS_SNAPTHREAD`, the process's own by `th32OwnerProcessID`), each opened (`OpenThread` with `THREAD_QUERY_LIMITED_INFORMATION`) for `GetThreadTimes` and its name (`GetThreadDescription`). The run state and a count of context switches come only from `NtQuerySystemInformation(SystemProcessInformation)`, at the cost of a snapshot of every process on the system, so they are null and the text says the state is not measured. The process: `K32GetProcessMemoryInfo` (`WorkingSetSize` as resident), its own the working set's pages `K32QueryWorkingSet` marks unshared (resident, as on Linux, where `PrivateUsage` would count committed pages never touched), and `GetProcessTimes`. Not built: the mappings, which would be a `VirtualQuery` walk with `QueryWorkingSetEx` for what is resident, and the heap, `HeapSummary` of the process heap.
  - DECISION: no feature file for the observer's window beyond its cost at rest (`observe-idle.feature`) and its Audit page (`observe-audit.feature`, which drives the window through its own `--mcp=inspect,input`); its behaviour is in the tools' scenarios (`features/tooling/mcp/observe.feature`, `mutate.feature`), and the window is checked by driving it off-screen. The client is checked by Rust tests that serve an inspector on a port and on a socket.
  - DECISION: the Audit page marks a finding by drawing a box on a picture of the window (`app_screenshot`) from the finding's bounds, in the observer, because the inspector has no way to draw in the application and none is added. The picture shows the application as it is now, the bounds may come from another point of the matrix, and the page says so.
  - What still wakes an idle application with or without an observer is the AT-SPI registry's `org.freedesktop.DBus.Peer.Ping` about every 30 s, answered by zbus on its `async-io` thread and accesskit's executor (section 8.1); it is outside the application and left alone.

### 19.6 Budgets

- **Tools:** at most ~25 per server, listed in a fixed order so prompt caches hit. Descriptions are at most 300 characters. A `--mcp-profile=slim` of 8 tools (snapshot, find, click, type, press, wait_for, screenshot, logs) is offered.
- **Responses:** 40 KB by default, with a hard cap of 80 KB. Over the cap, the result is set `truncated` and returns a `next_cursor`. Both caps stay well under the point where clients drop results (Claude Code drops results over 25k tokens).
- **Snapshots:** 400 nodes by default; names at most 200 characters; logs at most 100 lines.
- **Screenshots:** at most 1280 px on the long edge and at most 1 MB.
- **Latency on desktop:**
  - `app_find` p95 under 20 ms;
  - a 500-node snapshot under 40 ms of UI-thread time, spread over at most 2 frames;
  - `app_click` returns within one frame plus settling time.
- **Dependencies:** `ljos_mcp_core` brings at most ~60 transitive crates. CI measures this and fails on a 20 % rise.

### 19.7 Security

- **Builds.** The `mcp` feature is off by default. `ljos new` templates put it under `[features] devtools`. `ljos build --release` refuses when it is enabled, unless `--allow-mcp-in-release` is passed. CI checks that release artifacts contain no `rmcp`.
- **Binding.** Loopback or a 0600 Unix socket only. Any other address needs `--mcp-bind` plus `--mcp-insecure-lan`, and prints a loud warning.
- **Auth.**
  - A 256-bit bearer token, generated for each process start and stored in a 0600 file (`target/ljos/mcp-<pid>.json`), with the address, the process id, the capabilities granted and the calls a second answered (`url`, `token`, `pid`, `capabilities`, `calls_per_second`). The file is created 0600 (a mode given at creation, never a `chmod` after) and renamed into place whole. The directory it goes in is the user's alone: made 0700 when it is not there, and refused when it is there and is a link, belongs to another user, or group or others have any access to it, with the directory named and what to do. Nothing is written or changed in a directory that is refused, and a directory left at another mode by an older build is refused like any other. DECISION: refused rather than tightened: a directory somebody else could get into may already hold what they put there, and changing the mode of a directory that was not made here is not the inspector's to do. DECISION: a link is refused rather than judged by where it leads: the file is then written by name, so whoever can change the link would decide where the token goes. Only the directory itself is judged; a link above it (a project's `target` kept on another drive) is as any other path. `features/tooling/mcp/token-file.feature`.
  - Tokens are compared in constant time.
  - `Origin` and `Host` are validated (a 403 on mismatch), as the spec requires.
- **Capabilities.** `inspect` is the default. `input`, `mutate`, `eval` and `fs` are granted at launch or per token. Enforcement is in the server, because tool annotations are only hints.
- **Data.** Password fields are always redacted, and `--mcp-redact` masks every editable value. UI text reaches the model as tool output, so the server instructions warn against following instructions found in it.
- **The protocol alone.** `ljos_mcp_core::protocol` holds the JSON-RPC requests, a tool's answer and refusal, the rate's bounds and the token file's format (`TokenFileBody::to_text`, `parse`), with nothing that sends or stores them. `rmcp` and `tokio` come with the crate's feature `server`, on by default and implied by `http` and `client`; with `default-features = false` (and `inspect` if wanted) the crate links neither and builds for `wasm32-unknown-unknown` without `--cfg tokio_unstable`, so a program with a transport of its own shares the same words. `mise run check` holds that: it builds both for that target and fails if either crate is in the tree. Whether a token file is believed, and writing one, stay with `http` and `remote`, which have a disk.
- **Rate.** At most 20 calls per second per token by default, and at most 2 concurrent snapshot jobs. A refused call is answered 429 `rate_limited` with `Retry-After: 1`; the client in `ljos_mcp_core::remote` asks once more after it. The bound protects an application from a runaway agent. A program that starts the application as its own child and drives it as part of its own interface (a live preview, a test harness, an IDE's preview pane) makes two or three calls for each thing a person does, so a person working quickly meets the bound and waits out the second. Whoever starts the application may grant more: `ljos run --mcp-rate N` (also `ljos observe --mcp-rate`, `LJOS_MCP_RATE` for an `App` binary, `Inspector::calls_per_second` and `InspectorAccess::calls_per_second` in code). N is a whole number from 1 to 1000; anything else is a usage error that names the range, and the server refuses to start on one. DECISION: 1000 at most: a few hundred cover a person's gestures with room to spare, the limit keeps one mark per call in its second, so 1000 is still a few kilobytes, and past it the bound no longer bounds anything. The rate grants no capability and widens nothing else: the token, the capabilities, the binding and the 2-snapshot bound are as they are without it. The token file states the rate in force as `calls_per_second`, so a client can pace itself instead of learning it from refusals; `Client::calls_per_second` reads it (an inspector whose file states none answers 20). `features/tooling/mcp/rate.feature`.
- **Android.** Dev builds only. The app listens on device loopback, and `ljos mcp-connect --android` runs `adb forward` and fetches the token with `run-as`. The iOS simulator shares loopback. iOS devices are out of scope for v1.
- **Authoring server.** Paths are canonicalised and confined to `--project`. `run` and `fs` are off unless granted.

### 19.8 Integrations and app-defined tools

- **Other tools as clients.** IDEs, agent hosts and preview panels connect to the inspector as ordinary MCP clients. Ljós documents the protocol and ships no client-specific adapters. For example, HAL-C2 can map its preview panel onto `app_*` in its own repository. That is a client concern (section 17).
- **`Ljos.Mcp` (deferred past v1).** A QML module that would let a shipped app expose its own tools and resources to end users' agents. It needs release-build exposure, real authentication, consent UI and a privacy model, so it is a separate product decision. The `ToolRegistry` is designed now so that app tools and inspector tools can share it later.

### 19.9 Testing the MCP surface

- **Golden JSON** of every tool's schema and description in `tests/golden/mcp/`, so any drift is reviewed. Golden snapshot text per fixture QML file.
- **In-process protocol tests** with an rmcp client over `transport-worker` and a virtual clock.
- **Conformance.** The `@modelcontextprotocol/conformance` runner, pinned, runs in the `mcp:conformance` task. It is advisory at first.
- **BDD** in `features/tooling/mcp/`, tagged `@ljos-only @headless`. Scenarios cover:
  - snapshot contents, truncation and pagination;
  - stale refs and ambiguous locators;
  - read-only by default, and denied capabilities;
  - input through the real paths (the QML `onClicked` fires and focus moves);
  - locators that survive a reload;
  - idle CPU unchanged after a request;
  - rejected tokens and origins;
  - password redaction.
- **Agent evals (nightly, optional).** A small set of tasks, such as "fix this unnamed button" or "port this QtQuick snippet", run against `ljos mcp`. They track tool calls and tokens.

### 19.10 Milestones

| Milestone | MCP deliverable |
|---|---|
| M0 | `ljos_mcp_core`; `ljos mcp` with `qml_check`, `qml_lint`, `type_lookup`, `api_status` and `project_info`; `ljos://types`, `ljos://ledger` and `ljos://docs`; `.mcp.json` in the repo; the in-process test harness |
| M1 | `qml_format`, `binding_tier_explain`, `docs_search`, features tools and prompts; the read-only inspector (`status`, `snapshot`, `find`, `inspect`, `screenshot`, `logs`, `metrics`) with token and loopback |
| M2 | Input, mutate and eval capabilities; `app_batch`; binding graph; reload subscriptions; `qt_compare`; conformance in CI |
| M3 | The Android `adb forward` flow, touch gestures and mobile redaction |
| M4 | Recording, the tasks extension, release-guard hardening, and the `Ljos.Mcp` decision |

---

## 20. Milestones

Each milestone ends on Gherkin exit criteria, not on lists of features. The spikes in M0 tackle the largest risks first.

### M0 — Foundations (desktop, headless)

**Workspace**
- Workspace, mise and CI.
- Extract `qt_types`, `qml_front` (with `TypeOracle`) and `qml_syntax` from qml-solid. qml-solid moves to git-rev dependencies and its tests stay green.
- Fresh parse and compile baselines.
- Member-level error recovery in the fork.

**Engine**
- `ljos_ir` and lowering.
- `ljos_reactive` with proptests.
- `ljos_engine`: object store, `Value`, `MetaObject`, derive macros, `module!`.
- Tier-0 resolution and tier-2 rquickjs (`JsHost` with deadlines). Tier 1 is not in M0.

**Items, rendering and runtime**
- Item, Rectangle, Text (egui text; Parley replaces it in M1), MouseArea, TapHandler, Timer, Connections, QtObject, Component, Loader.
- Anchors, Row and Column.
- B0 and R backends, `RenderTree`, culling, and the frame pipeline up to step 12 with zero idle CPU.
- `FrameScheduler`, `Clock`, `UiMsg`/`UiHandle`, and a scheduler skeleton with lanes and `FrameReport`.
- A basic a11y pass (static roles and names) through AccessKit.

**Testing**
- `ljos_bdd` with both the Ljós and Qt backends.
- `features/bindings`, `items`, `a11y/names-and-roles` and `offscreen/list-delegates` (the last as backlog).
- `qml_compare` diff and ratchet; corpus submodule.

**Agent tooling**
- `ljos mcp` authoring server, M0 set (section 19.10), and `.mcp.json` in the repo.

**Spikes**, each time-boxed with a written result:
1. Android IME through GameTextInput, plus AccessKit on Android with GameActivity.
2. rquickjs bindgen on aarch64-android.
3. B0 cost on a mid-range Android device: 300 visible text nodes, target under 4 ms of CPU per frame.
4. AccessKit graft resolution in kittest.

**Exit criteria**
- `bindings/*.feature` passes on both backends.
- A demo window runs with 0 % idle CPU.
- Corpus parse is 495/495.
- All four spike reports are written.

### M1 — Core controls and dev loop (desktop)

**Items, controls and text**
- Polish pass: positioners, Flow, QtQuick.Layouts, implicit sizes.
- Image with async decode, `sourceSize`, the LRU cache and `Evictable`.
- Repeater, Instantiator, ListModel, Behavior, NumberAnimation and ColorAnimation.
- Controls.Basic phase-1 core (section 13) with the clean-room style.
- Parley `TextEngine`, atlas and elide.
- TextInput and TextField with desktop IME.

**Input, accessibility and localisation**
- `FocusTree`, key dispatch, `Shortcut` and `StandardKey`.
- The full a11y role table, the audit lints and `SystemPrefs`.
- `qsTr`, `.ts` import/export, `.ljl`, switching language at runtime, `String.arg`, `%n`, and `ljos i18n extract`.

**Engine and runtime**
- Tier-1 VM, with the debug cross-check and a coverage report over the corpora.
- Incubator time slicing.
- Supervisor and `ErrorBoundary`.
- Dev hot reload end to end: watcher, diff, patch, preservation and the error overlay.
- The `top` overlay.
- The read-only MCP inspector (section 19.10).

**Testing**
- `ljos_test` TestCase runner (P0 subset).
- External acceptance suite mechanism (`acceptance/<suite>/`, section 18.4).

**Exit criteria**
- The phase-1 Controls and Text scenarios pass, `@qt-parity` included.
- Validation: the first external suite (HAL-C2 stage S1, its bricks) passes on its allowlist.
- Hot-reload scenarios pass.
- Tier-1 coverage is measured and published.
- The qtdoc gallery ratchet is established.

### M2 — Full desktop apps

**Views and text**
- ListView/GridView virtualisation with variable heights, key-plus-offset anchoring and `rowsAboutToBeInserted`.
- Flickable physics on desktop.
- `RichDoc` with Markdown/StyledText/RichText, streaming, and selection across blocks.
- TextArea and TextEdit with undo and `textDocument` highlighting hooks.

**Items and controls**
- Popup, Dialog, Menu and Drawer, with the overlay and modal stack.
- Shapes, ShapePath and PathSvg.
- States, transitions and the `Binding` element.

**Platform**
- Multi-window; window-scoped singletons are built (§6.4).
- `ljos_shell` on its own loop with the AccessKit graft.
- File dialogs (rfd), clipboard and drag-and-drop.

**Migration support (section 17)**
- `PropertyMap`, `QmlListModel` and singletons.
- Live-region announcements for streaming content such as chat transcripts.
- `ljos check --port`.

**Agent tooling**
- MCP inspector input, mutate and eval capabilities, and conformance in CI (section 19.10).

**Exit criteria**
- A set of qtdoc example apps (chosen from 08) runs with their gallery scores above the ratchet.
- Validation: HAL-C2's desktop shell runs its daily-use paths, stage S3 (`desktop/native-*.feature`) passes on its allowlist, and stage S2 phone scenarios pass headless with touch. HAL-C2's side of this (FakeMc, shell double) is built in HAL-C2's repository.

### M3 — Android

- `AndroidHost` with GameActivity.
- Split between app core and UI host, and the surface lifecycle.
- GameTextInput bridge in production, hardened by the spike's findings.
- accesskit_android and a TalkBack pass.
- Insets, `SafeArea`, the back router (including predictive back), and `ScrollPhysics` for Android.
- Memory pressure, `Frozen`/Dehydrate, `Ljos.Persist`, Choreographer, frame-rate hints and thermal limits.
- The Gradle template and `ljos android run|bundle`.
- MCP inspector over `adb forward` (`ljos mcp-connect --android`).

**What the M0 spikes changed** (docs/spikes/01–04):
- `ljos_shell` drives android-activity directly on Android. winit 0.30 drops GameTextInput's text events, ignores the destroy event (BACK deadlocks) and gives no insets.
- The `TextInputClient` bridge converts UTF-16 indices. Java glue reports selection and cursor rect to the IME, maps `imeOptions` actions, and supplies insets and predictive back. The manifest sets `configChanges`.
- AccessKit on Android goes through accesskit_android 0.9 and the spike 04 graft, once egui moves to accesskit 0.25. Before that, 0.7.5 aborts when accessibility is switched off.
- rquickjs uses the Linux bindings copied for Android (spike 02); system font fallback is needed for CJK.
- B0 is far under budget in steady state on the emulator. The first frame and real arm64 numbers are measured on a phone first thing in M3 (spike 03).

**Exit criteria**
- A phone-layout reference app (qtdoc examples, plus HAL-C2's phone layout as validation) runs on a mid-range device within the budgets in section 8.5.
- `@android` scenarios pass on the emulator.
- The process-death restore scenario passes.

### M4 — Production and reach

- Production bundles: signing, A/B slots, trial and rollback.
- The B1 wgpu backend, or earlier if the M0 spike demands it.
- A render thread, only if profiling triggers it.
- iOS: host, `UITextInput` and accesskit_ios.
- RTL and mirroring, and ICU4X locale formatting.
- Material as a full design system (the M2 style is its sizes and colours on Basic's shapes).
- Sandboxed plugin tier, if D5 requires one.
- MCP: recording, the tasks extension, and the `Ljos.Mcp` decision.

**Exit criteria**
- A signed bundle round-trips, including rollback.
- RTL gallery snapshots pass.
- An iOS smoke test passes.

---

## 21. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **Android IME.** Neither winit nor egui provides composing or surrounding text. Every chat or form app on Android depends on it. | M0 spike; GameTextInput with a Kotlin `InputConnection` fallback; the `PlatformTextInput` abstraction |
| R2 | **Validation depends on a consumer's native layer.** HAL-C2's 52k lines of C++ must be ported before it can be a full validation target. | Milestones gate on generic corpora first; HAL-C2's own shell double and FakeMc (in its repository) let validation proceed meanwhile (D3) |
| R3 | **Lazy versus eager divergence** for bindings with side effects | Writers promoted to effect roots; `--eager` mode; `@qt-parity` binding scenarios |
| R4 | **Undefined orders that apps rely on**: onCompleted order, destruction order | Measure them on Qt through the oracle backend and match |
| R5 | **Tier-1 JS fidelity**: coercion, `-0`, `NaN`, int overflow | Debug cross-check against tier 2; test262-style cases |
| R6 | **egui churn** (a breaking release about every 2 months) and kittest pre-1.0 | Small egui surface behind `ljos_egui` and `Backend`; track the latest release with one upgrade PR per release (D9) |
| R7 | **Variable-height ListView with prepend anchoring** | A purpose-built feature file; external chat-timeline suites (HAL-C2's Timeline tests) |
| R8 | **AccessKit on mobile is young**; grafting in kittest is unverified | Spikes; budget for upstream fixes (MIT/Apache) |
| R9 | **Fork drift**: oxc releases weekly | Rebase policy and the `oxc:check` job |
| R10 | **rquickjs on mobile**: no prebuilt bindings | bindgen in CI per target (M0 spike) |
| R11 | **Text performance** at real-app scale (HAL-C2: ~460 elides, ~1,900 sized texts) | Layout LRU with width buckets; releasing off-screen layouts; device benches |
| R12 | **Hot-reload identity heuristics** (`Typed{nth}` on reordered siblings) | Lint for `id`s; models must expose stable keys |
| R13 | **Single UI thread ceiling** | `FrameSnapshot` seam; render thread when profiling triggers it |
| R14 | **Step-vocabulary sprawl** in Gherkin | Step catalogue in worker prompts; the `features:lint` metric |
| R15 | **Clean-room leakage**, for example through generated Qt data or style QML | Policy file; worker prompt rule; Qt data generated locally; Controls styles authored from scratch |
| R16 | **ICU4X and Parley binary size on mobile** | Datagen trimming; measure per feature before choosing |
| R17 | **GPU drivers on Android** (surface loss, GLES fallback) | Minimum tier set (D13); the R backend as a fallback |
| R18 | **MCP spec and rmcp churn.** 2026-07-28 is a stateless rewrite; clients lag; rmcp 3.x broke APIs | `McpSurface` isolation; pin the minor version; a client-version matrix in CI; quarterly upgrades |
| R19 | **Agent access to a running app** (prompt injection through UI text, mutation, eval) | Dev builds only; loopback plus token; read-only by default; capability gates; redaction (section 19.7) |
| R20 | **Token cost of tool schemas and trees** | Slim profile, `app_find`, deltas, `app_batch`, size caps; measure on reference apps in M1 |

---

## 22. Reconciled conflicts between reports

| Topic | Reports said | Decision |
|---|---|---|
| ListView `reuseItems` default | 01: keep Qt's `false`. 04: default on. | `false` (Qt semantics); a lint suggests opting in; an app-wide switch exists. |
| Incubation budget | 01: 4 ms at 60 Hz, 2 ms at 120 Hz. 04: 3 ms. | 3 ms at 60 Hz, scaled with the refresh period; configurable. |
| Polish pass cap | 01: 10. 02: 8. | 10, then a warning. |
| Parley version | 02: 0.12 shares egui's font crates. 05: pin 0.11.1. | Pick the newest version without duplicate skrifa/harfrust under the current egui, checked with `cargo tree -d`. egui tracks the latest release (D9). |
| Engine per window | 01: open. 03: one per app. 07: HAL-C2 uses an engine per window. | One per app, with window-scoped singletons (D2; built, §6.4). |
| egui_mobius | Brief: use it for slots and message passing. 03: its implementation does not fit. | Use the idea, not the crate (section 3.3). |
| External ledger tag | Early plan: a Ljós surface tag. 09: an allowlist instead. | Allowlist plus ratchet; `@backlog-ljos` only for scenarios that cannot be ported. |
| `qsTr` count | 05: 4,132. 07: 1,562. | 1,562 in 109 files. 05's figure included `build/` copies, as did its `Accessible.name` and `elide` counts. |
| B1 timing | 02: M3+. Plan: M4. | M4, unless the M0 Android spike shows B0 is too slow. |

---

## 23. Decisions

All decided by the user on 2026-10-10. Every decision except D9 took the recommendation. Rows marked *(Consumer)* are about HAL-C2's adoption, not about Ljós's design.

| # | Decision | Outcome |
|---|---|---|
| D1 | Licensing of `types.txt` and the Qt reference PNGs, which are derived from a Qt install | **Generate locally and git-ignore.** Publish only Ljós's own type dump. |
| D2 | One Engine per app or one per window | **One per app,** with window-scoped singletons (§6.4). |
| D3 | *(Consumer)* HAL-C2's 52k lines of C++ controllers | **Port to a shared Rust core (`hal-c2-core`) over time.** The shell double and FakeMc stand in meanwhile. |
| D4 | *(Consumer)* May `HalC2.Bricks` be adapted? | **Yes, targeted adaptations** that keep the same contract and pass the same tests on Qt and Ljós. |
| D5 | Plugin trust | **Trusted in v1, isolated by supervision;** sandboxing later. |
| D6 | Is the TUI dialect in scope? | **No;** only the contract is shared. |
| D7 | Home repo for the shared crates | **ljos-ui;** qml-solid depends on them by git rev. |
| D8 | Clean-room Controls styles | **Basic first, then Material.** |
| D9 | egui version policy | **Track the latest egui release** (this changes the recommendation). Upgrade on each release; the churn stays inside the three crates that touch egui. Parley follows its own releases. A temporary duplicate font stack (skrifa/harfrust) is acceptable but tracked in CI with `cargo tree -d` and fixed by the next joint bump. |
| D10 | JS debugger | **None in v1;** revisit with a quickjs-ng hook. |
| D11 | `panic` in mobile release builds | **`unwind`,** so the supervisor can recover. Measure the size cost. |
| D12 | *(Consumer)* HAL-C2 tests used as P0 validation | **S1 bricks** (`tst_Sidebar`, `tst_Timeline`, `tst_Composer`), **then the phone feature files.** |
| D13 | Minimum Android GPU tier | **Vulkan 1.1 on API 29+,** with GLES as best effort. |
| D14 | `@qt-parity` default for `language/` and `bindings/` | **Yes;** any divergence must be tagged `@qt-diverges` and explained. |
| D15 | App-defined MCP tools (`Ljos.Mcp`) for end users' agents | **Deferred past v1.** MCP is a developer tool for now; the shared `ToolRegistry` keeps the option open (decide at M4). |
| D16 | The live inspector in release builds | **Only with an explicit `--allow-mcp-in-release` flag,** which prints a warning; meant for internal QA builds. |

---

## 24. Version pins (as of 2026-10-10)

| Area | Pin |
|---|---|
| Toolchain | Rust 1.98.1, edition 2024, resolver 3 |
| egui stack | egui / eframe / egui-winit / egui-wgpu / egui_glow / egui_kittest 0.37.0 (wgpu 30, glow 0.17, glutin 0.32 with EGL only, winit 0.30.13, kittest 0.5). egui without its default features; eframe without its own (`accesskit`, `wayland`, `x11` only): its defaults bring egui's system font provider, whose fontique loads fontconfig by `dlopen` and so takes away the linked fontconfig that `ljos_text` and Parley's fontique use. Ljós draws no text with egui's fonts: `ljos_text` shapes the scene's, and the shell gives egui the bundled family for its own widgets (`ljos_egui::font_definitions`). The shell paints with egui_glow on EGL and falls back to egui-wgpu (section 7.3) |
| Accessibility | accesskit 0.25.1, the version egui 0.37 uses (accesskit_winit 0.34.1, consumer 0.39.1, android 0.9, ios 0.2); see spike 04 |
| JS | rquickjs 0.14.0 (quickjs-ng 0.16.2, `BC_VERSION` 28), `parallel` off |
| Text | parley + fontique (section 7.5 rule), skrifa 0.44 / harfrust 0.12 via egui, pulldown-cmark 0.13.4 |
| Render | vello_cpu (egui's pin), kurbo/peniko (vello's), lyon or vello_cpu for paths at M2 |
| Shaders | naga 30.0.1 (wgpu's), in `ljos_shader`'s `compile` feature only |
| Core | slotmap 1.1.1, salsa 0.28.5 (compiler only), postcard |
| Parser | oxc_parser 0.144.0 fork |
| Async and network | tokio 1.x, tokio-tungstenite 0.30.0, reqwest 0.13.5, rustls-platform-verifier 0.7.1 |
| Mobile | android-activity 0.6.1 (game-activity), cargo-ndk 4.x, Android API 26+ |
| Localisation | ICU4X 2.3, quick-xml 0.42 |
| BDD | cucumber 0.23.0, gherkin 0.16.0 |
| MCP | rmcp 3.5.x (MSRV 1.88, schemars 1.x); protocol 2026-07-28, compatible with 2025-11-25 |
| Corpus | qtdoc at `e05e4f4`; Qt ≥ 6.11 for the parity oracle |
| Media | FilmCraft at `8309f21` (submodule, path deps; symphonia 0.5 through it), cpal 0.18.2 (section 7.6) |
