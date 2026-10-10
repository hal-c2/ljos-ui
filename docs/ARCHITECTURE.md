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

**Non-goals (for now)**

- QtQuick3D, Particles, Qt3D, WebEngine, QtLocation, QtGraphs, ShaderEffect with custom shaders, and Qt.labs modules. These appear in the qtdoc corpus (08) but not in HAL-C2. In the API ledger they are `dropped` with a reason.
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
 └──────────────────────────────┘  └───────────────────────────┘   │ egui + egui-wgpu + AccessKit│
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
  - `send_coalesced(key, msg)` keeps only the latest value per key.
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
| `observer`/`top` | A dev overlay showing time per `ProcId` and lane ("which component ate the frame") |

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
├── crates/
│   ├── qml_syntax/             re-exports + parse helpers + member-level recovery [shared]
│   ├── qml_front/              discover, project (TypeOracle), types, scope, names [shared]
│   ├── qt_types/               Qt type-table reader (table generated locally)      [shared]
│   ├── qmltypes/               generator CLI (.qmltypes + qmldir only)            [shared, tool]
│   ├── qml_compare/            pixelmatch port, content score, ratchet, report    [shared, tool]
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
│   ├── ljos_i18n/              Catalog, I18n, .ts import/export, .ljl, ICU4X glue
│   ├── ljos_render/            RenderTree, DisplayList, B0 epaint, R vello_cpu, B1 wgpu
│   ├── ljos_egui/              QmlView widget, egui input/output conversion, EguiSink
│   ├── ljos_host/              PlatformHost, WinitHost, AndroidHost, (IosHost),
│   │                           PlatformTextInput impls, clipboard, prefs backends
│   ├── ljos_shell/             app runner: eframe (M0) → own egui-winit/wgpu loop (M2)
│   ├── ljos_dev/               watcher, devtools WebSocket, error and `top` overlays,
│   │                           MCP inspector [feature "mcp"]
│   ├── ljos_mcp_core/          McpSurface, ToolRegistry, caps/tokens, snapshot renderer (rmcp)
│   ├── ljos_mcp_author/        `ljos mcp` authoring tools (CLI only)
│   ├── ljos_test/              kittest harness wrapper, a11y audit, QtTest TestCase runner
│   ├── ljos_bdd/               cucumber World, Backend trait, Ljós + Qt backends, steps
│   ├── ljos/                   facade: re-exports, prelude, module! registration
│   └── ljos_cli/               `ljos` binary
├── .mcp.json                   `ljos mcp` for agents working in this repo (section 19)
├── features/                   Ljós's own Gherkin ledger (section 18.2)
├── acceptance/<suite>/         external acceptance suites: allowlist + PIN + ratchet (section 18.4)
├── ledger/qtquick.toml         API ledger: type × member → status (section 18.5)
├── corpus/                     qtdoc submodule at e05e4f4, examples.json, stand-ins
├── android/                    Gradle + GameActivity template (M3)
├── mise-tasks/
└── docs/  ARCHITECTURE.md  research/  CLEAN_ROOM.md (to write)
```

**Crate rules**

- **No parser in shipped apps.** Runtime crates depend on `ljos_ir`, never on `qml_syntax` or `qml_front`. A shipped app loads compiled IR, so it does not link the parser and its roughly 106 dependency crates (06 §4.6). Loading `.qml` at runtime is an optional `ljos_compiler` feature for dev builds and for plugins.
- **MCP never ships by accident.** `ljos_mcp_author` is linked only by `ljos_cli`. The inspector is a feature of `ljos_dev` that release builds refuse (section 19.7).
- **egui stays in three crates.** Only `ljos_egui`, `ljos_shell` and `ljos_render`'s B0 backend name egui types. `ljos_test` and `ljos_bdd` use egui_kittest, but scenarios never see it.
- **Start small and split later.** M0 creates the crates on its own critical path. The rest begin as modules and get their own crate when the boundary has proven itself. The tree above is the target.

**Sharing with qml-solid.** ljos-ui is the home repo for `qml_syntax`, `qml_front`, `qt_types`, `qmltypes` and `qml_compare`. qml-solid depends on them by git `rev` and keeps only its Solid/web lowering. There is never a second copy of the oxc fork. The fork is rebased every 4–8 oxc releases, and `mise run oxc:check` shows drift (06 §3.4). This is decision D7.

---

## 5. Front end: from `.qml` to IR

Sources: 06; 01 §3.4, §3.7.

### 5.1 Parsing

- **Parser.** qml-solid's oxc fork is reused unchanged at first. It parses all 495 qtdoc files, and embedded JS comes out as a real oxc AST.
- **First fork improvement: member-level error recovery.** The parser should return a partial tree plus diagnostics, so that one typo does not fail the whole file. Hot reload, the LSP and `ljos check` all need this. Annotation support (`@Foo`) and structured diagnostics follow.
- **Baselines** for the 495/495 parse and 464/495 compile figures must be re-measured on a fresh build before anyone quotes them. The earlier numbers came from a stale binary. Use a `CARGO_TARGET_DIR` outside `/tmp`, which has a disk quota.

### 5.2 Semantics (`qml_front`)

- **What it contains:** module discovery (`qmldir`, CMake `qt_add_qml_module`), the project graph, type-name lookup, the id and context scope chain, and name lookup.
- **Type knowledge** comes through a `TypeOracle` trait. Ljós implements it from its own registry (`ljostypes.json`, produced by the derive macros), not from Qt's tables.
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
- **Debug cross-check.** In debug builds, tier-1 bindings are also evaluated in tier 2 and the results compared.
- **Coverage reporting.** `ljos check --stats` reports tier-1 coverage over the qtdoc corpus and any application corpus given to it. Optimisation work is driven by that number.
- **Rejected:** running everything in QuickJS, a Cranelift JIT (iOS forbids JIT), and typed-only AOT to Rust (it needs annotations and works against hot reload).

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
- **Explicit registration** through `module!`, not `inventory` or linker sections. This keeps static linking on iOS deterministic and hot reload simple.
- **Type dump.** The macros write `ljostypes.json`, which feeds the compiler, the LSP and the API ledger.
- **`MetaObject` lifetime.** For types defined in QML it is behind an `Arc`, not `'static`, because hot reload replaces it.

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
  - `Object(ObjectId)`, `List`, `Enum`;
  - `Js(JsHandle)`, used for `var` properties that hold JS objects.
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
- `batch` defers the flush. Component creation, model resets, animation ticks and layout all run inside a batch.

**Effect roots** are evaluated eagerly. They are:
- signal handlers and `Connections`;
- Behavior interceptors;
- Rust subscribers;
- polish effects (anchors, positioners, Layouts);
- accessibility observers, only while an assistive technology is attached.

**Every other binding is lazy.** It stays dirty until something reads it. Paint reads only visible items, so an off-screen delegate's `color: hovered ? a : b` costs nothing. This is the main saving for off-screen content. It is safe because Ljós has no render-thread sync point.
- Two guards cover side-effecting bindings. A binding that writes during evaluation is promoted to an effect root.
- `--eager` mode exists for differential testing against Qt.

**Binding loops**
- Re-entering a node that is `Computing` is a binding loop. Ljós logs Qt's message once, returns the stale value and does not re-enter. The message names the property and points at its name in the source (`PropertyBinding.name_span`), as Qt's does.
- Which member of a loop is reported depends on the order bindings first run. During creation the engine follows Qt's set-up order: bindings on some properties are installed but held back (`Graph::hold`) until the component's other bindings have run, then released. Without this an eager engine reports a loop on an acyclic chain.
- A lazy binding is checked when it is first read, so a loop nobody reads is not reported until then. `--eager` reports it at creation, as Qt does.
- Handlers that ping-pong between each other are capped at 100 re-dirties per node per flush; the flush ends with `RerunLimit`.

**Override stack.** `Binding {}` and `State`/`PropertyChanges` push onto a per-property override stack above the base binding or value. `restoreMode` pops it. Assigning a value imperatively breaks the binding, and `Qt.binding()` reinstalls one. Report 09 confirmed both against Qt 6.11.2.

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

### 6.4 One Engine, many windows

**Decision (D2): one Engine per app.**
- Each `Window` is a root with its own `ProcId` boundary and its own a11y and focus trees.
- Singletons are per Engine by default.
- `#[qml_singleton(scope = "window")]` provides per-window singletons. This serves apps that are used to a `QQmlEngine` per window, HAL-C2 among them.
- Windows that are really independent apps can run separate Engines that talk through messages.

This keeps models and singletons shared without proxies (01 §4.8, 03 §3.1, 07 P1-27).

---

## 7. Rendering, layout and text

Source: 02 §3; text from 05 §3.3.

### 7.1 Render tree and backends

**Render tree**
- `ljos_render::RenderTree` is a slotmap of `RenderNode`s. Each node holds:
  - its local and world `kurbo::Affine`, size and subtree AABB;
  - opacity (multiplied down the tree, as Qt does), `ClipSpec` and `LayerSpec`;
  - `visible`, a cached `Arc<DisplayList>` in local coordinates, `Dirty` bitflags and `last_visible_frame`.
- The `Dirty` flags are `CONTENT`, `TRANSFORM`, `OPACITY`, `CLIP`, `ORDER`, `BOUNDS`, `LAYER`, `A11Y` and `VISIBLE`.
- Only `CONTENT` rebuilds a node's display list. Transform, opacity, clip and z changes only re-compose.

**Painting.** Items implement `PaintContent::paint(&self, cx, &mut DisplayListBuilder)` and never touch the GPU. The `DrawOp`s are `Rect` (with radii, fill, border and AA), `Shadow`, `Image`, `NinePatch`, `Glyphs`, `Path` and `Embed`. `Paint` is `Solid`, `Gradient` or `Image`.

**`RenderBackend { sync, encode, trim, snapshot_rgba }`** has three implementations:

| Backend | When | Notes |
|---|---|---|
| **B0 epaint** | M0 onward | Cached `Arc<Mesh>` per node. Gradients use vertex colours. Shadows use a blur or a nine-patch. Axis-aligned clips use `with_clip_rect`. Rotated clips and `layer.*` are approximated, with a debug warning. Paths are tessellated with lyon, or rasterised to cached textures with vello_cpu; this is decided at M2. |
| **R vello_cpu** | M0 onward | Deterministic reference renderer: fixed SIMD level, one thread, bundled fonts. Used for the Qt comparison and as a fallback without a GPU. |
| **B1 wgpu** | M4, or earlier if B0 profiles badly | Plugs in through `egui_wgpu::Callback` segments. Instanced SDF quads for rects, borders, gradients and shadows. Textured quads for glyphs and images. Retained instance buffers per batch root, so scrolling updates one uniform. Offscreen targets for layers, effects and rotated clips. vello_gpu renders paths into cached atlas textures. |

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
12 present    egui end → tessellate (B0) → egui-wgpu render → present (Fifo)
13 idle       Idle lane while budget remains: QuickJS GC, cache trims; compute next wake
```

- **`FrameSnapshot`**, the output of steps 8–9, holds `Arc`'d display lists, transforms and order. It is immutable, and it is where a render thread would attach (02 §3.2).
- **Debug views.** `LJOS_VISUALIZE=batches|clip|changes|overdraw|layers` shows what each frame did.

### 7.3 Shells

- **M0–M1:** `ljos_shell` runs on eframe. This is the fastest route and gets Android early.
- **From M2:** `ljos_shell` is Ljós's own loop on `ljos_host` + egui-winit + egui-wgpu. It owns the AccessKit adapter, which grafts its subtree into egui's tree, plus the surface lifecycle, present mode and devtools hooks.
- **`QmlView`** stays an `egui::Widget`, so QML can be embedded in any egui app. Embedded, it falls back to emitting a11y nodes through `ctx.accesskit_node_builder`.
- **Native egui widgets inside QML.** An `EguiItem` gets a child `Ui` at its world bounding box, which allows translation and uniform scale only. QML items painted above it register occluders.

### 7.4 Layout

Layout is Ljós's own implementation of QML semantics.
- **Anchors** are eager geometry bindings.
- **Positioners** (Row, Column, Grid, Flow) and **QtQuick.Layouts** run in the polish pass. QtQuick.Layouts covers fill, preferred/min/max sizes, alignment, spans and margins.
- **Implicit sizes** propagate through Loader and Repeater, as Qt does (07 R3).
- **`LayoutMirroring`** is resolved at layout time. It flips anchors, positioner order and horizontal views, but never raw `x` (section 15).
- **Taffy** may be offered later as an opt-in `Ljos.Flex` item.

### 7.5 Text

Source: 05 §3.3.

**`TextEngine`**, one per app, contains:
- `parley::FontContext` (Fontique) and `LayoutContext`;
- an LRU cache of layouts keyed by `(text hash, style hash, width bucket, scale, locale)`;
- `GlyphAtlas`: skrifa outlines rasterised by vello_cpu into egui textures, at most 2048² (4096² on desktop), separate per scale, with an RGBA path for colour emoji.

**Version pin.** Parley must share skrifa 0.44 and harfrust 0.12 with egui 0.36's epaint, so the font stack is not duplicated. Report 05 pins Parley 0.11.1 for this. Report 02 says 0.12.0 also matches. The scaffold verifies with `cargo tree -d` and picks the newest version that does not duplicate. Because egui tracks the latest release (D9), each egui bump re-runs this check. A temporary duplicate is allowed but tracked in CI.

**Off-screen discipline**
- Hidden text keeps only its implicit size and metrics.
- Its `Layout` is freed after N frames off-screen.
- Clipped runs are never rasterised.

**Elide** is Ljós's own: a binary search over grapheme clusters, cached per width. HAL-C2 uses `elide` about 460 times.

**Language and fonts.** The content language (`Text.lang`, a Ljós extension that defaults to the UI language) drives font fallback and CJK unification.

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

---

## 8. Off-screen and resource policy

Source: 04 §3.3–3.7.

### 8.1 Frame scheduling

A single `FrameScheduler` computes the next wake time:

    next_wake = min(animation deadlines, timers, request_repaint_after, messages)

- **When idle:** 0 wakeups per second.
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

### 8.4 Virtualised views

- **ListView and GridView** create delegates for the viewport plus `cacheBuffer`. The default `cacheBuffer` is one screen on each side on a phone and two on desktop. The delegate count is not exact in Qt either; it measured 11–12 for the case below (09 §2.4).
- **Incubation budget:** about 3 ms per frame, set by `Ljos.ListView.incubationBudgetMs`. During flings, placeholders sized from estimated heights are preferred over dropped frames.
- **`reuseItems` keeps Qt's default of `false`**, which is a compatibility decision. Pooled delegates have their effect roots suspended. A lint suggests `reuseItems: true` where a delegate has no creation side effects. An app-wide default can be switched on.
- **Variable heights** are remembered by model key. Scroll anchors are a *model key plus offset*, so prepending rows (loading chat history) does not move the view (07 R5). This needs `rowsAboutToBeInserted`.
- **While an assistive technology is attached,** `cacheBuffer` grows to ±20 rows on desktop and ±10 on mobile (section 14).
- **`Repeater` is not virtualised**, which is Qt semantics. A lint warns above 50 items.

### 8.5 Memory budgets

These are initial values, enforced in CI by benchmarks (04 §3.7).

| Resource | Phone | Desktop |
|---|---|---|
| Decoded images (LRU, at `sourceSize` or layout size × DPR) | min(64 MiB, 6 % of RAM) | 256 MiB |
| GPU caches (layers, paths) | 24 MB | 64 MB |
| Glyph atlas | 2048² A8 | up to 4096² |
| JS heap | `set_memory_limit(96 MiB)`, GC threshold 8 MiB | same; limit configurable |
| JS time per dispatch | 4 ms soft (log), 100 ms hard (interrupt) | same |
| Process | RSS ≤ 150 MiB, GPU ≤ 64 MiB, cold start ≤ 500 ms | — |

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

- **Entry points.** `JsHost::enter(slice, f)` sets the interrupt deadline and then enters the context. `pump_jobs(until)` drains microtasks. `collect_if_idle` runs GC in the Idle lane.
- **Interrupt granularity** is about every 10k opcodes, roughly 266 µs. Native calls that run long (a huge `JSON.parse`, a regex) are not interruptible. They are chunked or moved to the compute pool.
- **Prototypes.** One prototype per QML type, with `Accessor` getters and setters that call `Graph::read`/`write`. This keeps QuickJS shapes stable.
- **`#[exotic]` objects** are used only for genuinely dynamic objects: model role objects, `ListModel` element proxies and context-property fallbacks.
- **Wrappers** are created lazily and cached as `Persistent`. A JS-owned object is destroyed by its finaliser.

### 9.3 Faults

`Fault::{JsException, JsInterrupted, JsOutOfMemory, Panic, Startup}`.

- **Panics.** Rust panics in handlers are caught at the turn boundary. Mobile release builds keep `panic = "unwind"` unless decision D11 says otherwise.
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
- **`ScrollPhysics` as data**, with a `VelocityTracker` that fits a least-squares line over about 100 ms:
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
  - Tab order is precomputed per scope, honouring `KeyNavigation`, and invalidated when the structure changes.
- **Key dispatch** runs `ShortcutOverride`, then `Shortcut` (context-aware, with `activatedAmbiguously`), then `Keys` (`priority`, `forwardTo`), bubbling up. `StandardKey` maps to platform-specific key sequences.
- **Text input** goes through two traits. `TextInputClient` is implemented by TextInput and TextEdit: `surrounding`, `replace`, `set_selection`, `cursor_rect`, `hints`. `PlatformTextInput` has one implementation per platform:
  - desktop: winit IME, moving to 0.31's `ImeRequest`;
  - Android: GameTextInput (`set_text_input_state`, `set_ime_editor_info` derived from `inputMethodHints`), with a Kotlin `InputConnection` as the fallback;
  - iOS: a `UITextInput` view written with `objc2`.
- **Android IME is the largest risk across all the reports.** It gets a time-boxed spike in M0 that covers composing, autocorrect, Icelandic dead keys, CJK, gesture typing and password managers. The production bridge lands in M3.

---

## 12. Hot code update

Sources: 03 §2.5, §3.5–3.6; 07 §2.2.

### 12.1 Development

```
notify watcher (120 ms debounce, BLAKE3 fingerprint of qml/js/qmldir/theme)
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
- assigned property values (according to the provenance bit);
- `ListModel` contents;
- `contentX/Y`, which are clamped;
- `currentIndex`;
- the `StackView` stack;
- the `Loader` source;
- `Timer.running`;
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

- **QtQuick.Controls has two layers.** `ljos_controls` implements behaviour in Rust: the `Control`/`AbstractButton` state machines, `Popup` and the overlay stack, and `Dialog`. Each *style* is QML written clean-room on top, the way Qt layers templates and styles, with the code being ours. A style is chosen with `QT_QUICK_CONTROLS_STYLE`-compatible configuration.
- **Basic style first.** It is the neutral base that other styles and custom controls build on. Real apps lean on it (HAL-C2 imports `QtQuick.Controls.Basic` 87 times and `QtQuick.Controls` twice).
- **Material** follows for Android, after M3. Material attached properties are accepted as no-ops until then.
- **Fusion and the native macOS style** are deferred.
- **Phase 1 types** (the most used across the qtdoc corpus and HAL-C2):
  - Control, AbstractButton, Button, Label, TextField, TextArea, CheckBox, Switch, SpinBox, ComboBox, ToolTip;
  - Popup, Dialog, Menu/MenuItem;
  - ScrollBar, ScrollView, Pane, ApplicationWindow.
- **Phase 2:** Drawer, StackView, SwipeView, TabBar, Slider, ProgressBar, BusyIndicator and SplitView.
- **Default roles and names** are derived from documentation and checked against Qt through a small helper that queries `QAccessible`. This is needed because QML reads `Accessible.name` as `""` for defaults (09 §2.4).

---

## 14. Accessibility

Source: 05 §3.1.

- **Versions.** AccessKit 0.24.1, the version egui 0.36 uses; its types do not mix with 0.25 (spike 04). The move to 0.25.1 (Android adapter 0.9.0, iOS adapter 0.2.1) waits for egui.
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
  - `reduced_motion` makes animations complete in one frame.
  - The values are exposed as `Qt.styleHints.accessibility.*`.
  - `font_scale` applies at text layout, with a per-item opt-out (`font.scaleWithSystem: false`).
- **Audit lints.** `ljos_test::a11y::audit` runs on every mounted test component and checks:
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
- **Plurals.**
  - `%n` uses ICU4X `PluralRules` 2.3. The mapping from `.ts` numerus forms to CLDR categories is validated against `.ts` fixtures produced by running `lupdate` as a black box, never against Qt's plural tables.
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
- **First-class module aliases** let an app map one import name to another module without rewriting source text.
- **Sandboxing.** A sandboxed tier would run plugin JS in a pinned runtime with capability-gated singletons, no `Qt.createQmlObject` on arbitrary text, and quotas. The architecture leaves room for it, but it is not built in v1 (D5).

---

## 17. Migrating existing Qt apps

Source: 07 (HAL-C2 as the worked example).

Many QML apps are a thin QML view over native controllers. The QML reads state from a context object or singleton, and sends actions back. Ljós supports that pattern generically, so an app can move its QML over unchanged and replace only the native side.

**What Ljós provides:**
- **`PropertyMap`**, the equivalent of `QQmlPropertyMap`. It has per-key reactivity and accepts whole-object assignment.
- **The `QmlListModel` trait.** It has roles and the full row-signal set: `rowsAboutToBeInserted`, `rowsInserted`, `rowsRemoved`, `rowsMoved`, `dataChanged` and `modelReset`. `ListView`, `Repeater`, `Instantiator` and `TreeView` consume it, and so do JS arrays and ints.
- **Native QML types written in Rust:** per-Engine and per-window singletons, creatable types and enums.
- **Module registration:** native modules from Rust, and QML modules from a directory with `qmldir`.
- **Runtime creation:** `Qt.createQmlObject`, `Qt.createComponent` and `Component.createObject`.
- **The `QtTest` module (`ljos_test`),** so existing `tst_*.qml` suites keep running. It supports:
  - `TestCase` with `when: windowShown`, `init`, `cleanup` and `*_data`;
  - `compare`, `verify`, `tryCompare`, `tryVerify` and `fuzzyCompare`;
  - `createTemporaryObject` and `findChild` by `objectName`;
  - mouse, key and touch synthesis;
  - `grabImage` and `SignalSpy`.

  This is an *import path* for existing tests. New behaviour is specified in Gherkin (section 18).
- **External acceptance suites** (section 18.4): an app's own `.feature` files run against Ljós through an allowlist and a ratchet.
- **A migration guide and `ljos check --port`,** which reports, from the API ledger, which types and members an app uses that Ljós does not yet support. The MCP tool `api_status` exposes the same data (section 19).

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
  - It runs with `QT_FORCE_STDERR_LOGGING=1`, `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`, `TZ=UTC` and `LANG=en_US.UTF-8`.
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
| Started by | `ljos mcp --project <dir>` | `ljos run --mcp[=caps]`, or `ljos_dev::mcp::attach(&engine, cfg)` |
| Transport | stdio | Streamable HTTP on `127.0.0.1:0`, or a Unix socket using stdio framing |
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

### 19.4 Live inspector

| Tool | Does | Cap |
|---|---|---|
| `app_status` | Windows, size, scale, appearance, focus, visibility state, clock (real or virtual), capabilities | inspect |
| `app_snapshot` | Accessibility-tree outline with refs; `root`, `depth`, `include`, `max_nodes`, `cursor`, `since` (delta), optional image | inspect |
| `app_find` | Nodes by role, name (exact, substring or regex), `objectName`, text or state. Cheaper than a snapshot | inspect |
| `app_inspect` | One node in depth: type, `ObjectKey`, source location, properties with **provenance** (assigned, bound at tier 0, 1 or JS, animated, default), geometry, lattice state | inspect |
| `app_binding_graph` | Sources and dependents of a property, and why it last evaluated | inspect |
| `app_get_property` / `app_set_property` | Read or write; a write is recorded as "assigned by devtools" | inspect / mutate |
| `app_invoke` | Emit a signal or call an invokable method | mutate |
| `app_click`, `app_type`, `app_press`, `app_scroll`, `app_touch` | Input through the **same `InputRouter`, `FocusTree` and `TextInputClient` paths as tests**, never by calling handlers directly. Each returns a short delta after the next frame | input |
| `app_wait_for` | Wait until conditions hold: locator present or absent, property value, text, `reload_generation`, `idle`. Default timeout 5 s, maximum 30 s | inspect |
| `app_advance` | Advance the virtual `Clock` (only with `--test-clock`) | input |
| `app_screenshot` | PNG of a window or node from the R renderer, so it works headless or occluded; at most 1280 px | inspect |
| `app_resize` / `app_set_appearance` | Size, DPI and orientation; light or dark, contrast, reduced motion, font scale (`SystemPrefs`) | input |
| `app_reload` | Force recompile and reload; returns a patch summary or errors | mutate |
| `app_logs` / `app_metrics` | Logs and JS errors with QML file and line; `FrameReport` summaries, lane times, top `ProcId`s, budgets (section 8.5) | inspect |
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
- Unnamed actionable nodes are flagged `⚠unnamed`, which nudges authors towards `Accessible.name`.

**Addressing**
- **Refs** (`e9`) are short and valid for one snapshot epoch. A stale ref returns `stale_ref`, or resolves itself when its role and name are still unique.
- **Locators** are durable across hot reload (section 12.1 identity rules). They use the widely known Playwright role-selector subset:
  - `role=button[name="Delete"]`, `text=…`, `#objectName` and `ref=e9`;
  - chaining with `>>`, plus `nth=` and `name~=regex`.

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
- **Determinism.** With `--test-clock`, `app_advance` drives the virtual `Clock`. A BDD scenario and an agent session on the in-process transport have the same semantics.

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
  - A 256-bit bearer token, generated for each process start and stored in a 0600 file (`target/ljos/mcp-<pid>.json`).
  - Tokens are compared in constant time.
  - `Origin` and `Host` are validated (a 403 on mismatch), as the spec requires.
- **Capabilities.** `inspect` is the default. `input`, `mutate`, `eval` and `fs` are granted at launch or per token. Enforcement is in the server, because tool annotations are only hints.
- **Data.** Password fields are always redacted, and `--mcp-redact` masks every editable value. UI text reaches the model as tool output, so the server instructions warn against following instructions found in it.
- **Rate.** At most 20 calls per second per token, and at most 2 concurrent snapshot jobs.
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
- Multi-window with window-scoped singletons.
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
- Material style.
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
| Engine per window | 01: open. 03: one per app. 07: HAL-C2 uses an engine per window. | One per app, with window-scoped singletons (D2). |
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
| D2 | One Engine per app or one per window | **One per app,** with window-scoped singletons. |
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
| egui stack | egui / eframe / egui-winit / egui-wgpu / egui_kittest 0.36.2 (wgpu 30, winit 0.30.13, kittest 0.4) |
| Accessibility | accesskit 0.24.1, the version egui 0.36 uses (accesskit_winit 0.32, android 0.7.5). Moving to 0.25 (android 0.9, ios 0.2) waits for egui; see spike 04 |
| JS | rquickjs 0.14.0 (quickjs-ng 0.16.2, `BC_VERSION` 28), `parallel` off |
| Text | parley + fontique (section 7.5 rule), skrifa 0.44 / harfrust 0.12 via egui, pulldown-cmark 0.13.4 |
| Render | vello_cpu (egui's pin), kurbo/peniko (vello's), lyon or vello_cpu for paths at M2 |
| Core | slotmap 1.1.1, salsa 0.28.5 (compiler only), postcard |
| Parser | oxc_parser 0.144.0 fork |
| Async and network | tokio 1.x, tokio-tungstenite 0.30.0, reqwest 0.13.5, rustls-platform-verifier 0.7.1 |
| Mobile | android-activity 0.6.1 (game-activity), cargo-ndk 4.x, Android API 26+ |
| Localisation | ICU4X 2.3, quick-xml 0.42 |
| BDD | cucumber 0.23.0, gherkin 0.16.0 |
| MCP | rmcp 3.5.x (MSRV 1.88, schemars 1.x); protocol 2026-07-28, compatible with 2025-11-25 |
| Corpus | qtdoc at `e05e4f4`; Qt ≥ 6.11 for the parity oracle |
