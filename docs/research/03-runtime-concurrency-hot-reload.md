# 03 — Runtime: concurrency model, fair scheduling, fault isolation and hot code update

Status: research report, October 2026. Scope: how the Ljós runtime should schedule work, isolate faults and update code (QML, JS and Rust) while it runs, on desktop and mobile.
Clean-room note: this report uses only public documentation for Qt, Slint, Felgo, Flutter, React and Erlang/OTP. Source code was read only for crates Ljós will depend on or evaluate: rquickjs 0.14.0, quickjs-ng 0.16.2 (bundled in rquickjs-sys), egui 0.36.2, egui_mobius 0.6.0, subsecond 0.8.0-alpha.1 and tokio 1.53.

---

## 1. Summary of key findings

- **Current versions (crates.io, 2026-10-10):** rquickjs **0.14.0** (released 2026-09-18, bundles **quickjs-ng 0.16.2**, bytecode format `BC_VERSION 28`), tokio **1.53.2**, egui/egui_kittest **0.36.2**, egui_mobius **0.6.0**, ractor **0.16.5**, kameo **0.22.2**, actix **0.13.5** (last release June 2024), boa_engine **0.22.0**, wasmtime **49.0.2** stable (50.0.0-rc.1), subsecond/dioxus **0.7.10** stable (0.8.0-alpha.1), hot-lib-reloader **0.8.2**, accesskit **0.25.1**, winit **0.30.13** stable (0.31.0-beta.3).
- **rquickjs is a good fit for the UI thread.** I measured it locally (rquickjs 0.14, release build, Ryzen AI Max+ 395). A runtime costs **28 KB**. The first full context adds **78 KB** and each further full context **~54 KB**; a `Context::base` costs **~25 KB**. Creating a runtime plus a context takes **~49 µs**. A Rust→JS call takes **~53 ns**. Evaluating a compiled binding with `this` takes **~92 ns**. Compiling a one-line binding function takes **~2.5 µs**.
- **Preemption works and costs almost nothing.** In a tight loop, quickjs-ng calls the interrupt handler about every **266 µs** (4,000 calls in 1.06 s), with **<0.5 % overhead**. A `while(true){}` given a 4 ms deadline was stopped at **4.02 ms** with an uncatchable `Error: interrupted`, and the context was still usable afterwards. Hitting `set_memory_limit(16 MiB)` raises `out of memory`, and the context also survives that. So per-handler and per-frame JS budgets are practical.
- **Bytecode caching pays off.** A 197 KB module took **8.5 ms** to parse and compile but **0.6 ms** to load from bytecode, about 14× faster. Bytecode is **477 KB**, or **254 KB** with `strip_debug` and `strip_source`. `Module::load` is `unsafe`, because malformed bytecode can corrupt memory. Bundles therefore have to be signed and checked against the engine version.
- **rquickjs constraints that shape the architecture:**
  - `Runtime` is `!Send` without the experimental `parallel` feature (it is `Rc<RefCell<RawRuntime>>`). JS therefore lives on the thread that created it.
  - Calling `Runtime` methods inside `Context::with` panics (`RefCell already borrowed`, observed). Budget state must sit in `Rc<Cell<_>>` captured by the interrupt closure, not behind runtime calls.
  - quickjs-ng has **no module-unload API**, so JS module hot reload has to use generation-versioned specifiers or a fresh `Context`.
  - **No pre-generated bindings exist for Android or iOS targets**, so mobile builds need the `bindgen` feature (libclang at build time) or bindings vendored in-tree.
  - **No debugger protocol** exists in quickjs-ng 0.16.2.
- **What we take from Erlang/BEAM:** supervision trees with restart intensity, "let it crash" at component boundaries, per-plugin heaps (one runtime per plugin), mailboxes with bounded and coalescing back-pressure, two-generation code (old and current) plus explicit state migration (`code_change`), and budgeted ("reduction") time-slicing of JS and incubation.
- **What does not carry over:** preemption of arbitrary Rust code, per-component heaps for the scene graph (the scene must have a single owner and be mutated in place), copy-on-send for scene data, unbounded mailboxes, and production hot loading of native code (app-store policy).
- **egui_mobius:** adopt the *idea* (typed `Signal<T>`/`Slot<T>` message passing that separates the UI from the backend). Do **not** adopt the implementation:
  - one OS thread per slot (`Slot::start`)
  - `start_async` busy-polls `try_recv()` + `yield_now()`, burning a core
  - `Slot::clone` returns a disconnected receiver
  - `Arc<Mutex<_>>` everywhere, with subscriber callbacks invoked while the lock is held (re-entrancy deadlock risk)
  - eager, glitch-prone `Derived` recomputation
- **Supervision:** kameo 0.22 has OTP-like `SupervisionStrategy::{OneForOne, OneForAll, RestForOne}`, `RestartPolicy::{Permanent, Transient, Never}` and `.restart_limit(n, window)`. ractor leaves restart policy to the implementor. Neither can supervise UI-thread logical processes or `!Send` JS runtimes on pinned threads. **Recommendation: a small in-house supervisor** with OTP semantics over three child kinds: UI boundaries, tokio tasks, and pinned threads.
- **Hot reload:** QML hot reload should be **IR-diff based with explicit identity keys**. It should keep imperatively assigned property values, models, scroll positions, focus and navigation stacks, always re-evaluate bindings, and never re-run `Component.onCompleted` on preserved objects. Follow Flutter's split between hot reload, hot restart and full restart, and React Fast Refresh's rules (type change means remount, plus a reset pragma). On error, keep the old tree on screen, as Slint and Fast Refresh do.
- **Rust hot-patching:** subsecond supports Android arm64-v8a and armeabi-v7a and the iOS simulator, but **not iOS devices**. It patches only the tip crate, cannot change struct layouts, resets thread-locals, and needs the `dx` CLI (ThinLink is not a standalone tool). It is a dev-only nicety, not a foundation.
- **Production updates of QML/JS bundles are allowed** under Apple DPLA §3.3.1(B) (interpreted code, provided it does not change the app's primary purpose or bypass OS security) and Google Play policy ("code that runs in a virtual machine or an interpreter"). Native code may never be downloaded on mobile. Ship signed bundles with A/B slots and automatic rollback.

---

## 2. Detailed findings

### 2.1 Erlang/BEAM lessons and how they map to a UI runtime

| BEAM mechanism | What it gives BEAM | Mapping to Ljós | Verdict |
|---|---|---|---|
| Reduction-counted preemption (≈4,000 reductions per slice in modern ERTS) | No process can hog a scheduler | JS: interrupt handler plus deadline (§2.2). Incubation and model diffing: explicit work units charged to a `SliceBudget`. Rust code cannot be preempted, so every Rust job must be written to yield. | **Adopt for JS and incubation**; Rust stays cooperative |
| Per-process heap and GC | GC pauses are per process and small; memory is attributable | One rquickjs `Runtime` per plugin or worker gives a separate heap, memory limit and GC. The main QML engine shares one heap (QML objects reference each other freely). | **Adopt for plugins and workers**; not per component |
| Mailboxes and selective receive | Asynchronous decoupling | Typed inboxes with *priority lanes* instead of selective receive, which is O(n) scanning. Inboxes are bounded, or coalescing by key (latest value wins for property and model updates). | **Adopt, bounded** (BEAM's unbounded mailboxes are a known overload failure mode) |
| Links, monitors, supervisors (`one_for_one`, `one_for_all`, `rest_for_one`; restart `permanent`/`transient`/`temporary`; intensity, default 1 restart in 5 s) | Self-healing and failure containment | Supervisor tree over services (tokio), plugins (threads) and *component boundaries* (UI subtrees recreated from their definition). Intensity caps stop crash loops. | **Adopt** |
| "Let it crash" | Simple error handling; clean restarts | A failing component subtree is torn down and rebuilt from its QML definition plus a state snapshot, or replaced by a fallback. Defensive code is not scattered through handlers. | **Adopt at boundaries** |
| Two-version code loading (current and old; a third load purges old code and kills lingering processes; fully qualified calls switch to new code) | Upgrade without downtime | Generation-tagged `ComponentDef`/module instances. Old instances live until their objects are remounted or released. A "fully qualified call" corresponds to the next binding or handler dispatch through the definition table. | **Adopt the generation model** |
| `gen_server:code_change(OldVsn, State, Extra)`; an `{error, R}` return aborts the upgrade and rolls back | Explicit state migration | `Migrate` trait for Rust-side state, and an optional `__ljos_migrate(prev)` export for JS singletons and modules. A failed migration means remount with defaults. | **Adopt** |
| Copying message passing | Isolation | Copying only *across heaps* (plugin↔UI, network↔UI). Scene mutations on the UI thread move ownership and are never copied. | Partial |
| Distribution, hot loading of native code in production | — | Not applicable on mobile; HAL-C2's Elixir server already does this on the backend. | **Reject** |

The key structural difference: BEAM schedulers are symmetric and processes migrate between them. A UI toolkit has one asymmetric resource, the scene and its GPU surface, and soft real-time deadlines (8.33 ms at 120 Hz, 16.7 ms at 60 Hz). The answer is BEAM-style *logical* processes multiplexed on the UI thread by a deadline-aware scheduler, with true parallelism only for work that does not touch the scene.

### 2.2 JavaScript execution with rquickjs 0.14

**API surface** (read from the `rquickjs-core-0.14.0` source):
- `Runtime::new()` and `new_with_alloc(A)`.
- Runtime controls: `set_interrupt_handler(Option<Box<dyn FnMut() -> bool>>)`, `set_memory_limit(usize)`, `set_max_stack_size(usize)`, `set_gc_threshold(usize)`, `run_gc()`, `memory_usage() -> MemoryUsage`.
- Job and promise hooks: `is_job_pending()`, `execute_pending_job()`, `set_promise_hook`, `set_host_promise_rejection_tracker`.
- `set_loader(R: Resolver, L: Loader)`, gated by the `loader` feature.
- `AsyncRuntime` and `AsyncContext` (the same API as async fns, plus `idle()` and `drive()`), gated by the `futures` feature.

`Resolver::resolve(&mut self, ctx, base, name, attributes) -> Result<String>` and `Loader::load(&mut self, ctx, name, attributes) -> Result<Module<Declared>>` support import attributes. Bytecode is written with `Module::write(WriteOptions { endianness, strip_source, strip_debug, object_reference, allow_shared_array_buffer })` and read back with `unsafe Module::load(ctx, &[u8])`. `Persistent<T>::save/restore` keeps JS values alive outside a `with` scope; Ljós needs it to store binding functions and promise resolvers in Rust structures.

**Feature flags:**

| Group | Flags |
|---|---|
| Core | `std`, `macro`, `classes`, `properties`, `array-buffer`, `allocator`, `rust-alloc` |
| Loading | `loader`, `dyn-load` |
| Async | `futures` |
| Experimental | `parallel` ("may not work as expected") |
| Build | `bindgen` |
| Extra types | `chrono`, `either`, `indexmap`, `phf` |
| Debug dumps | `dump-*` |

Bundled bindings cover Linux, macOS, Windows and wasm, but **not `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android` or `aarch64-apple-ios`**. Android needs `bindgen` with the NDK sysroot, or Ljós should generate and check in bindings per target in CI.

**Threading.** Without `parallel`, `Runtime` wraps `Rc<Mutex-like RefCell>` and is `!Send` (compile-checked). With `parallel`, the `InterruptHandler` type gains a `+ Send` bound and contexts are freed through an mpsc channel. Ljós should **not** enable `parallel`. Each runtime stays pinned to one OS thread, and cross-thread communication uses messages. This matches the BEAM model and avoids an experimental feature.

**Re-entrancy hazard (observed).** Calling `rt.memory_usage()` inside `ctx.with(..)` panics with `RefCell already borrowed`. Rules for Ljós:
- the scheduler sets the deadline *before* `with`, by writing an `Rc<Cell<Instant>>` that the interrupt closure reads;
- memory accounting and GC run between JS entries, never inside them.

**Measured numbers** (my bench, rquickjs 0.14.0, `-O3` C build):

| Metric | Value |
|---|---|
| Bare runtime | 28.6 KB malloc |
| + first `Context::full` | +78 KB |
| Each extra full context | ~54 KB |
| `Context::base` | ~25 KB |
| Create + drop runtime + full context | 49 µs |
| Create + drop context on existing runtime | 43 µs |
| Interrupt handler call interval (tight loop) | ~266 µs; overhead 1064 → 1068 ms (<0.5 %) |
| Deadline enforcement (4 ms budget, `while(true){}`) | stopped at 4.02 ms; context reusable |
| OOM at 16 MiB limit | `Error: out of memory`; context reusable |
| Parse + compile 197 KB module | 8.5 ms |
| Load same as bytecode | 0.6 ms (477 KB; 254 KB stripped) |
| Rust→JS call | 53–55 ns |
| Compile 1,000 tiny binding fns | 2.3–2.7 ms |
| Evaluate binding (call with `this`) | ~92 ns |

Implications:
- **Memory.** A runtime per plugin (~100 KB each) is cheap; even 100 plugins cost about 10 MB. A runtime per QML component is *not* viable. The reason is not memory but that objects cannot be shared across heaps, which breaks QML's free cross-referencing of objects.
- **Startup.** HAL-C2 `apps/desktop-qt` has **944 QML files (~159k lines)**, 446 `Repeater`, 342 `ListView` and 108 `Loader` uses. Tens of thousands of bindings at ~2.5 µs each means tens of milliseconds of compile time, so Ljós needs ahead-of-time bytecode (the analogue of Qt's documented qmlcachegen) plus a content-hash disk cache. It should also evaluate *simple* bindings (property paths, arithmetic, ternaries) in a Rust IR interpreter without entering JS at all.

**Comparison with other engines:**
- **Boa 0.22** is pure Rust and spec-strong, but its own nightly V8-benchmark-suite dashboard (boajs.dev/benchmarks) and a 2026 analysis of that data put it at roughly 215 versus QuickJS ~1,227 and V8-jitless ~2,522. It is a credible fallback for wasm-only or no-C builds, but too slow to be the default.
- **V8 (rusty_v8)** is multi-megabyte and JIT is not allowed on iOS. Jitless V8 is about 40 % slower on Speedometer per V8's own blog. Rejected.
- QuickJS gives the best balance of size, startup time, predictability (refcounting plus cycle GC, no JIT warm-up) and embeddability.

**Debugging.** quickjs-ng 0.16.2 exposes no breakpoint or stepping API (only `JS_SetDumpFlags` and backtrace helpers). Expect to write a minimal DAP (Debug Adapter Protocol) adapter ourselves, built on an opcode or line hook. That requires patching quickjs-ng or waiting for upstream. Plan for logging, stack traces and an in-app REPL first.

**Promises and async.** For the UI thread, use the synchronous `Runtime` and pump microtasks with `execute_pending_job()` inside the frame's JS budget. QML expects handlers to finish synchronously, and `AsyncRuntime` puts an async lock on every entry. Rust futures (network calls, file reads) run on tokio. Each returns a `UiMsg::Resolve(PromiseId, Result<OwnedValue, JsErrorInfo>)`, and the UI thread settles the stored `Persistent<Function>` resolve or reject callback. `AsyncRuntime::drive()` is appropriate for plugin and worker runtimes running on their own threads with a `LocalRuntime`.

### 2.3 Rust async and scheduling

**tokio 1.53:**
- `Builder::new_multi_thread()` for networking.
- `LocalRuntime` (`Builder::build_local(LocalOptions)`) is **stable** in 1.53 (no `tokio_unstable` gate in `runtime/mod.rs`). It is the right executor for `!Send` work on a pinned thread, such as a plugin's JS runtime, and replaces the `LocalSet` boilerplate.
- Cooperative budgeting: each task gets **128 units per poll** (`Budget::initial() = 128`). The public API is `tokio::task::coop::{consume_budget, has_budget_remaining, unconstrained, cooperative}`. Use `consume_budget().await` in long loops on the network side, such as decoding big HAL-C2 snapshots.
- `spawn_blocking` defaults to up to 512 threads, which is too many on phones. Use a bounded compute pool (2–4 threads on mobile) for image decode, QML parse and bytecode compile.
- Structured concurrency: `JoinSet` per supervisor child group, plus `tokio_util::sync::CancellationToken` and `task::TaskTracker` (tokio-util 0.7.20) for orderly shutdown.

**Bridging to the UI thread.** egui 0.36 `Context` is `Send + Sync`. It offers `request_repaint()`, `request_repaint_after(Duration)`, `request_repaint_of(ViewportId)`, `set_request_repaint_callback(..)`, `request_discard(reason)` and `will_discard()`. Pattern:
1. Background code pushes onto a bounded MPSC.
2. A shared `AtomicBool wake_pending` is swapped. Only on a false→true transition does the sender call `ctx.request_repaint()`, which coalesces wake storms.
3. At the start of each frame the UI thread drains the inbox up to its message budget.

Keyed coalescing (`HashMap<CoalesceKey, UiMsg>`) keeps only the latest update per property or model, which is the main back-pressure tool against a chatty server. When nothing is dirty, do not request a repaint; egui then sleeps, which matters for battery.

**egui_mobius 0.6** (source read). `Signal<T>` wraps `std::sync::mpsc::Sender<T>`; `Slot<T>` wraps `Arc<Mutex<Receiver<T>>>`.

| Part | Behaviour |
|---|---|
| `Slot::start` | Spawns **one OS thread per slot** |
| `Slot::start_async` | Loops on `try_recv()` + `tokio::task::yield_now()`, a busy poll that never parks; each handler is spawned with `catch_unwind` |
| `Slot::clone` | Creates a fresh, unconnected channel |
| `MobiusRuntime` | Routes events by string key to `Arc<dyn Fn(E) -> Pin<Box<dyn Future>>>` |
| `egui_mobius_reactive` | `Arc<Mutex<Vec<Box<dyn Fn()>>>>` subscribers, notified *while holding the lock*; `Derived` recomputes eagerly; tests rely on `thread::sleep(50ms)` |

Adopt:
- typed message enums between UI and backend;
- the "signal/slot over channels" naming that QML developers recognise;
- `catch_unwind` around async handlers.

Avoid:
- threads per slot;
- busy polling;
- string routing (use enums);
- locks in the reactive graph (the Ljós property graph is single-threaded on the UI thread: `Cell`/`RefCell` plus dirty bits, pull-based evaluation, topological and glitch-free).

**Actor crates:**
- **kameo 0.22**: `Spawn::supervise(&parent, child).restart_policy(..).restart_limit(5, 10s).spawn()`, `Actor::supervision_strategy()`, bounded and unbounded mailboxes, libp2p remoting.
- **ractor 0.16**: gen_server-like with priority signals (Signal > Stop > SupervisionEvent > Message) and `monitors`; restart logic is left to the user.
- **actix 0.13.5**: no release since 2024. Rejected.

All of them assume `Send` actors on a tokio runtime. Ljós's most important supervised children are UI-thread subtrees and `!Send` JS runtimes, so an in-house supervisor (~500–800 LOC) is a better fit (§3.4).

**Deadline-aware UI scheduler.** Qt's documented `QQmlIncubationController` is the reference point:
- `incubateFor(msecs)`, `incubateWhile(std::atomic<bool>*, msecs)`;
- the docs' example spends **5 ms of every 16 ms** frame and notes that a fixed budget is simplistic;
- `QQmlIncubator` modes are `Asynchronous`, `AsynchronousIfNested` (used by ListView for initial delegates) and `Synchronous`, with `forceCompletion()` and `setInitialState()` before complex bindings run.

Ljós should make the budget *adaptive*: `work_budget = frame_period − measured(paint + tessellate) − safety_margin(1 ms)`, clamped to [1 ms, 0.6 × period]. Work is split into lanes:

| Lane | Contents | Budget rule |
|---|---|---|
| `Input` | pointer, key, IME and a11y actions, and their handlers | always runs; each handler has a JS slice (default 50 ms in dev to catch bugs, 16 ms in release) |
| `Critical` | binding propagation and layout for **visible** dirty items, animations | always runs to completion for visible items |
| `Visible` | delegate creation for items entering the viewport, synchronous `Loader` | runs until the budget is spent; may overrun by one item |
| `Prefetch` | ListView `cacheBuffer` delegates, `Loader { asynchronous: true }`, image decode completion | only from leftover budget |
| `Deferred` | off-screen binding refresh, JS microtask tail, `Qt.callLater` (66 uses in HAL-C2) | leftover budget, fair round-robin across owners |
| `Idle` | QuickJS cycle GC (`run_gc`, with `set_gc_threshold` raised during interaction), cache trimming | only when no repaint is pending |

Visibility drives priority: `Visible > Near (cacheBuffer) > Offscreen > Hidden`. When a window is minimised or the app is backgrounded (Android `onPause`/`onStop`), only `Input` and `Idle` lanes run; timers are throttled to ≥1 s, and incubation is suspended.

Off-screen items keep their *values* stale and dirty-flagged. They are recomputed lazily when they become visible or when a visible binding reads them, which is pull semantics. That keeps off-screen resource use small.

### 2.4 Fault isolation

| Fault | Mechanism | Containment |
|---|---|---|
| A binding throws | Catch `CaughtError`, log with QML source location, keep the property's previous value, mark the binding errored (matches documented QML warning behaviour) | binding |
| A handler or binding loops | Interrupt deadline means an uncatchable exception; the context survives (verified) | handler |
| JS OOM in a plugin | `set_memory_limit` per runtime; `out of memory` (verified) | plugin runtime |
| Deep recursion | `set_max_stack_size` (set it on the thread that runs JS) | handler |
| Rust panic in a native type or handler | `catch_unwind` at dispatch boundaries | component boundary |
| Repeated failures | Supervisor intensity is exceeded, so escalate to the parent boundary, then the fallback UI | subtree |
| Untrusted third-party native-speed plugin | wasmtime 49: `Config::epoch_interruption` + `Store::set_epoch_deadline` (wasmtime docs say epochs are up to 2–3× cheaper than `consume_fuel`) + `ResourceLimiter`; on platforms without JIT (iOS), the Pulley interpreter | plugin |
| Hostile code | Out-of-process (desktop only) | process |

`catch_unwind` limits:
- it requires `panic = "unwind"` in release; many mobile templates set `abort`, and Ljós must document that;
- double panics abort;
- panics across `extern "C"` (quickjs callbacks) must be caught on the Rust side, as rquickjs already does in its interrupt trampoline (`catch_unwind` → `set_panic` → return true → re-raise);
- state touched during the panic is suspect. Treat the whole component subtree as poisoned and rebuild it rather than continuing ("let it crash").

HAL-C2's desktop shell loads user customisations from `~/.config/hal-c2/shell/`. That is exactly the tier-1 case: user QML runs in the main engine but under an error boundary with per-handler budgets.

### 2.5 Hot code update

**Prior art (documentation only):**
- **Flutter.** Recompiles changed libraries, the VM reloads them, then the framework rebuilds every widget. State is preserved; `main()` and `initState()` do *not* re-run. Changes to static initializers, enum↔class conversions and generic arity need a hot restart; native code needs a full restart. Compile errors reject the reload. "Mixed state" can differ from a fresh start.
- **React Fast Refresh.** A component-only module updates in place. A module with non-component exports re-executes along with its importers. If the edit propagates outside the React tree, the result is a full reload. Hook state survives if the order and arguments of hook calls stay the same. Effects re-run regardless of their dependencies. Class components remount. `// @refresh reset` forces a remount per file. Error boundaries retry on the next edit.
- **Slint live preview.** With `SLINT_LIVE_PREVIEW=1` and `--features slint/live-preview`, the app uses the interpreter. Properties, callbacks and models survive. On a syntax error the old UI stays visible. Renaming a property used from Rust can terminate the app. Dev only.
- **Felgo QML Hot Reload.** Per Felgo's blog and docs, it reloads properties, bindings, JS function bodies and signal handlers, adds, removes and replaces objects, handles imports and assets, and "preserves state". The QML Book describes the mechanism as diffing the old and new component trees and updating changed subtrees. Errors show an overlay and can be reverted without losing state.
- **Erlang.** Two code generations, `code_change`, rollback on error (§2.1).

**Identity rules** for matching old objects to new ones are original Ljós design, given in §3.5.

**JS module reload.** QuickJS caches modules per context by normalized name and has no unload API. Ljós's `Resolver` therefore appends a generation (`app/util.js` → `app/util.js#g7`) to every *dirty* module and to its transitive importers, up to the first QML component boundary. Fast Refresh shows that propagation is necessary. Old module records become unreachable and are reclaimed by the cycle GC. `.pragma library` scripts, the shared-instance JS imports used by HAL-C2's 12 `.js` files, are singletons and can carry an optional `export function __ljos_migrate(prev)`. If the reload fails, fall back to a **hot restart**: a fresh `Context`, re-instantiate the root, and restore a state snapshot.

**Rust native hot reload:**
- **subsecond 0.7.10 / 0.8.0-alpha.1:**
  - it routes calls through a jump table: `subsecond::call(|| ..)` and `HotFn::try_call`, with stale calls unwinding to the nearest `call`;
  - on Android it `dlopen`s a memmapped copy to get around permission problems;
  - it supports Android arm64-v8a and armeabi-v7a, the iOS simulator only, Linux, macOS, Windows and wasm32;
  - limitations: tip crate only, no struct-layout changes ("re-instancing" is the framework's job), statics never destructed, thread-locals reset;
  - it needs the `dx serve --hotpatch` toolchain (ThinLink is not standalone).
- **hot-lib-reloader 0.8.2:** a `cdylib` with `#[no_mangle]` functions and a libloading reload. It is hampered by the unstable Rust ABI and generics, and it does not work on iOS.

Conclusion: Ljós should put `subsecond::call` at its dispatch points behind a `dev-hotpatch` feature, so apps run under `dx` get patching of handler bodies for free. It should **not** design core types as hot-swappable dylibs. Instead, keep app logic in QML/JS (hot-reloadable everywhere) or on the server (HAL-C2 already hot-reloads in Elixir).

**Production updates:**
- **Apple DPLA §3.3.1(B):** "Interpreted code may be downloaded to an Application but only so long as such code: (a) does not change the primary purpose of the Application … (b) does not bypass signing, sandbox, or other security features of the OS; and (c) … does not create a store or storefront for other Applications."
- **App Review 2.5.2** forbids downloaded code that "introduces or changes features or functionality". In practice this is read together with §3.3.1(B) as permitting bug-fix and UI updates. Feature changes still need review, so treat it as a policy risk.
- **Google Play** forbids downloading dex, JAR or `.so` files from outside Play, but "does not apply to code that runs in a virtual machine or an interpreter". Interpreted code must still not enable policy violations.
- QML is an interpreted declarative language and JS bytecode runs in the QuickJS VM, so both qualify. Native code updates go only through the stores. Desktop has no such constraint.

---

## 3. Recommendations for Ljós

### 3.1 Threads

```
                         ┌──────────────────────── UI / main thread (single owner) ───────────────────────┐
 winit / android-activity│  EventLoop ─► FrameDriver ─► Scheduler(lanes, SliceBudget)                    │
  input, a11y actions ──►│     │             │  ├─ Input handlers ──► JsHost (rquickjs Runtime+Context)   │
                         │     │             │  ├─ Binding graph (Rust IR fast path, JS slow path)        │
                         │     │             │  ├─ Layout (visible dirty only)                            │
                         │     │             │  ├─ Incubator (time-sliced object creation)                │
                         │     │             │  └─ Idle: QuickJS GC, cache trim                           │
                         │     │             ▼                                                            │
                         │     │        Scene (retained QML object tree) ─► egui emit (visible only)      │
                         │     │                                   └─► egui tessellate ─► wgpu present    │
                         │  UiInbox ◄─ bounded MPSC + keyed coalescing + AtomicBool wake ─► request_repaint│
                         └───────▲──────────────────────────────────────────────▲─────────────────────────┘
                                 │ UiMsg                                         │ UiMsg / PluginMsg
     ┌───────────────────────────┴───────────┐        ┌──────────────────────────┴──────────────────────┐
     │ tokio multi-thread runtime (2–4 wkrs) │        │ Pinned threads, one per isolated JS runtime     │
     │  services: HAL-C2 WS client, HTTP,    │        │  WorkerScript / plugin: own rquickjs Runtime,   │
     │  SQLite, file watcher, devtools WS    │        │  memory limit, stack limit, interrupt deadline, │
     │  JoinSet + CancellationToken per sup. │        │  tokio LocalRuntime for its futures             │
     └───────────────┬───────────────────────┘        └─────────────────────────────────────────────────┘
                     │ jobs
     ┌───────────────▼──────────────────┐   ┌────────── Supervisor tree (in-house, OTP semantics) ──────────┐
     │ Bounded compute pool (2–4 thr.)  │   │ AppSup ─┬─ UiEngineSup ── Boundary(Loader/StackView page/…) │
     │ image decode, QML parse, IR,     │   │         ├─ ServiceSup (one_for_one, 5 restarts / 30 s)      │
     │ bytecode compile, text shaping   │   │         ├─ PluginSup  (one_for_one, transient, 3 / 60 s)    │
     └──────────────────────────────────┘   │         └─ DevSup: watcher → compiler → patch server        │
                                            └──────────────────────────────────────────────────────────────┘
```

- **No separate render thread in v1.** egui's emit, tessellate and paint path runs on the UI thread in eframe. Keep the egui emit stage a pure function of a `Scene` snapshot so a render thread can be added later (for example if tessellation profiles above 2 ms on mid-range Android). Rejected for now because it doubles scene memory or needs a snapshot or diff layer before we have measurements.
- **JS for app QML runs on the UI thread, in one `Runtime` + one `Context` per QML engine** (normally one per app; one per window only if windows are independent apps). Rejected alternatives: a runtime per window (breaks shared singletons and models between windows); a runtime or context per component (breaks QML object sharing and multiplies 54 KB per context); JS on a worker thread (bindings must be synchronous).
- **Separate runtimes on pinned threads** for WorkerScript, third-party plugins and anything with a memory cap. Do not enable the experimental `parallel` feature.

### 3.2 Messages and back-pressure

- One `UiMsg` enum, with no string routing. Senders use `UiHandle::send`, which is bounded and fails fast with `Full`, or `send_coalesced(key, msg)`, where the latest value wins.
- Network services own their buffering and apply tokio `consume_budget` while decoding. Frame budgets also cap how many messages the UI drains per frame. Overflow stays in the queue, which pushes back on senders, and drain time is reported in `FrameReport`.
- Rejected: an unbounded `std::sync::mpsc` (egui_mobius), which lets a burst from the server run away with memory; and a broadcast bus, which makes back-pressure impossible to attribute.

### 3.3 Scheduler

- Lanes and adaptive budget as in §2.3. JS entries go through `JsHost::enter(slice, f)`, which writes the deadline into `Rc<Cell<Instant>>` before `Context::with`. Microtasks are pumped by `JsHost::pump_jobs(until)`.
- Each logical owner (component boundary, plugin, Loader) has a `ProcId`, and the scheduler round-robins `Deferred` and `Prefetch` work across owners. Tracking cost per owner gives a `top`-like dev overlay ("which component ate the frame"), the UI analogue of BEAM's per-process reductions.
- Incubation is resumable object-tree construction that yields between objects: a Ljós original analogous to the documented Qt incubation behaviour. `Loader.asynchronous`, `ListView` cache-buffer delegates and `StackView` push transitions use it. `forceCompletion` is used when a synchronous read needs the result.

### 3.4 Supervision

- An in-house `Supervisor` with `Strategy::{OneForOne, OneForAll, RestForOne}`, `Restart::{Permanent, Transient, Temporary}` and `Intensity { max_restarts, within }`. It has three child kinds:
  1. **UI boundaries**: restarting one recreates the subtree from its `ComponentDef`, using a state snapshot if one is available and the fallback otherwise. Boundaries are implicit at `Loader`, `StackView` pages and `Window`, and explicit with an `ErrorBoundary { fallback: Component }` element (a Ljós extension).
  2. **tokio tasks**: `JoinSet` + `CancellationToken`.
  3. **Pinned threads**: plugin runtimes, supervised over a channel with a heartbeat.
- kameo 0.22 is a reasonable option *inside* the service layer if HAL-C2 later wants remote actors, but it is not the backbone. Rejected: ractor (no restart policies), actix (stale), and building on BEAM itself via an embedded runtime such as Lumen (too heavy and unmaintained).

### 3.5 Hot-reload pipeline

```
 host: notify watcher ─► qml-solid oxc QML parser ─► Ljós IR (ComponentDef w/ stable ObjectKeys, signatures)
        ─► diff(prev IR, new IR) ─► ReloadPatch{generation, ops}
        ─► devtools WebSocket (desktop: loopback; Android: `adb reverse`; iOS sim: loopback)
 device: DevSup receives ─► queue as UiMsg::Reload ─► applied at a frame boundary as one transaction
        ─► on any error: keep old generation live, show overlay, keep watching
```

**Identity (original design).** `ObjectKey = (ComponentId, path of KeySeg)`, where `KeySeg` is, in order of preference:
1. `id: foo`, which is stable;
2. an explicit `objectName`;
3. `ModelRow(row key)` for delegates, keyed by the model's stable key (role `id` or a configured key role) rather than by index;
4. `Typed { type, nth }`, the n-th child of that type under its parent.

Matching rules:
- If the type differs at the same key, remount (as React does).
- If the `ComponentSignature` changed, keep state for each property by name only when its type is unchanged. The signature is a hash of the declared properties' names and types, the signals and the inline components.
- Renaming an `id` counts as a new object.

What is preserved:
- property values whose provenance is *assigned* rather than *bound*; every property carries a provenance bit;
- `ListModel` contents;
- `Flickable` `contentX/Y` (clamped);
- `currentIndex`, the `StackView` stack (page keys and their assigned properties), `Loader` source, `Timer.running`;
- text input contents and selection;
- `activeFocus`, restored by `ObjectKey` path;
- accessibility focus.

What is re-evaluated: every binding and handler body comes from the new IR. `Component.onCompleted` does **not** re-run on preserved objects (as Flutter's `initState` does not); a dev-only `Component.onReloaded` hook exists for custom refresh. A `pragma LjosReload: reset` per file forces a remount, like `@refresh reset`.

What triggers a hot restart: changes to singleton `ComponentDef` shapes, imports of new native modules, and the generation-versioned JS fallback failing (§2.5).

**State migration.** Rust-side state that wants to survive implements `Migrate` (Erlang's `code_change`). If `migrate` fails, remount with defaults and report the failure.

### 3.6 Production updates

- **Bundle format.** A content-addressed archive containing QML IR, JS source, optional stripped bytecode and assets. A manifest lists `native_api` (the semver of the exported Rust↔QML surface), `qjs_bc_version = 28`, `min_app_version`, a BLAKE3 hash per file, and an **ed25519 signature** over the manifest.
- **Loading checks.** Check the signature, then the native API and bytecode version. If the bytecode version does not match, compile from source and ignore the bytecode, because `Module::load` is `unsafe` on unverified input.
- **A/B slots with rollback.** Keep a last-known-good slot. A new bundle boots in *trial* mode and is promoted only after a "healthy" mark: first frame, the HAL-C2 server connected, and no supervisor escalation within 30 s. If it crashes in trial mode, the next launch reverts automatically. A rollback can also be forced server-side by a kill switch.
- **Policy.** No native code is downloaded on mobile. Feature-level changes still go through store review, to respect App Review 2.5.2. Microsoft's CodePush/App Center has been retired, which is a reminder to own this path rather than rely on a hosted service.

### 3.7 Key Rust types (signatures only, original design)

```rust
// ---- identity & processes
#[derive(Copy, Clone, Eq, PartialEq, Hash)] pub struct ProcId(u32 /*index*/, u32 /*generation*/);
pub enum Lane { Input, Critical, Visible, Prefetch, Deferred, Idle }
pub enum Visibility { Visible, Near, Offscreen, Hidden }

// ---- budgets
pub struct FrameBudget { pub frame_start: Instant, pub deadline: Instant, pub period: Duration }
pub struct SliceBudget { deadline: Cell<Instant>, spent: Cell<Duration> }
impl SliceBudget {
    pub fn expired(&self) -> bool;
    pub fn remaining(&self) -> Duration;
}

// ---- scheduler
pub enum JobPoll { Done, Yield, Failed(Fault) }
pub trait Job {
    fn lane(&self) -> Lane;
    fn owner(&self) -> ProcId;
    fn poll(&mut self, cx: &mut JobCx<'_>) -> JobPoll;
}
pub struct JobCx<'a> { pub scene: &'a mut Scene, pub js: &'a JsHost, pub budget: &'a SliceBudget }
pub struct FrameReport { pub per_lane: [Duration; 6], pub top_owners: SmallVec<[(ProcId, Duration); 8]>, pub overrun: bool }
pub struct Scheduler { /* lanes, per-owner queues, round-robin cursors */ }
impl Scheduler {
    pub fn spawn(&mut self, job: Box<dyn Job>);
    pub fn cancel_owner(&mut self, owner: ProcId);
    pub fn run_frame(&mut self, scene: &mut Scene, js: &JsHost, budget: FrameBudget) -> FrameReport;
}

// ---- JS host (UI thread; !Send by construction)
pub struct JsHost { rt: rquickjs::Runtime, ctx: rquickjs::Context, deadline: Rc<Cell<Instant>> }
impl JsHost {
    pub fn new(limits: JsLimits) -> rquickjs::Result<Self>;
    pub fn enter<R>(&self, slice: Duration,
        f: impl for<'js> FnOnce(rquickjs::Ctx<'js>) -> rquickjs::Result<R>) -> Result<R, Fault>;
    pub fn pump_jobs(&self, until: Instant) -> usize;
    pub fn collect_if_idle(&self, budget: &SliceBudget);
}
pub struct JsLimits { pub memory: Option<usize>, pub stack: usize, pub gc_threshold: usize }

// ---- messaging into the UI thread
pub enum UiMsg {
    Service(ServiceEvent), Model(ModelDelta), Resolve(PromiseId, Result<OwnedJs, JsErrorInfo>),
    Plugin(PluginId, PluginMsg), Supervision(SupEvent), Reload(Box<ReloadPatch>), Lifecycle(AppLifecycle),
}
#[derive(Clone)] pub struct UiHandle { /* tx, egui::Context, Arc<AtomicBool> wake */ }
impl UiHandle {
    pub fn send(&self, msg: UiMsg) -> Result<(), SendError>;
    pub fn send_coalesced(&self, key: CoalesceKey, msg: UiMsg) -> Result<(), SendError>;
}

// ---- supervision
pub enum Strategy { OneForOne, OneForAll, RestForOne }
pub enum Restart { Permanent, Transient, Temporary }
pub struct Intensity { pub max_restarts: u32, pub within: Duration }
pub enum ExitReason { Normal, Shutdown, Reload, Fault(Fault) }
pub enum Fault { JsException(JsErrorInfo), JsInterrupted { slice: Duration }, JsOutOfMemory,
                 Panic(Cow<'static, str>), Startup(String) }
pub trait Child {
    fn id(&self) -> ChildId;
    fn restart(&self) -> Restart;
    fn terminate(&mut self, reason: &ExitReason);
}
pub trait ChildSpec { fn start(&self, cx: &mut StartCx<'_>) -> Result<Box<dyn Child>, Fault>; }
pub struct Supervisor { /* strategy, intensity, children, restart log */ }
impl Supervisor {
    pub fn on_exit(&mut self, child: ChildId, reason: ExitReason, cx: &mut StartCx<'_>) -> Escalation;
}
pub enum Escalation { Handled, EscalateToParent(Fault) }

// ---- hot reload & migration
pub struct ObjectKey { pub component: ComponentId, pub path: SmallVec<[KeySeg; 8]> }
pub enum KeySeg { Id(Atom), Named(Atom), ModelRow(RowKey), Typed { ty: TypeKey, nth: u16 } }
#[derive(Copy, Clone, Eq, PartialEq)] pub struct ComponentSignature(pub u64);
pub enum PatchOp {
    ReplaceBinding { key: ObjectKey, prop: Atom, expr: ExprId },
    ReplaceHandler { key: ObjectKey, signal: Atom, body: FnId },
    Insert { parent: ObjectKey, at: u16, def: ObjectDefId },
    Remove { key: ObjectKey },
    Remount { key: ObjectKey, reason: RemountReason },
    ReplaceComponent { id: ComponentId, sig: ComponentSignature },
    ReloadModule { url: Arc<str>, generation: u64 },
}
pub struct ReloadPatch { pub generation: u64, pub ir: Arc<ProgramIr>, pub ops: Vec<PatchOp> }
pub trait HotReloadHost { fn apply(&mut self, patch: ReloadPatch) -> Result<ReloadReport, ReloadError>; }
pub trait Migrate: Sized {
    const VERSION: u32;
    fn snapshot(&self) -> StateBlob;
    fn migrate(from_version: u32, old: StateBlob) -> Result<Self, MigrateError>;
}

// ---- production bundles
pub struct BundleManifest {
    pub id: Arc<str>, pub version: semver::Version, pub native_api: semver::VersionReq,
    pub qjs_bc_version: u32, pub min_app_version: semver::Version,
    pub files: Vec<(Arc<str>, [u8; 32])>, pub signature: [u8; 64],
}
pub trait BundleStore {
    fn stage(&mut self, bytes: &[u8]) -> Result<BundleId, BundleError>;
    fn activate_trial(&mut self, id: BundleId) -> Result<(), BundleError>;
    fn mark_healthy(&mut self);
    fn rollback(&mut self) -> Result<BundleId, BundleError>;
}
```

---

## 4. Risks and open questions

1. **Mobile builds of rquickjs.** No bundled Android or iOS bindings exist. Generating them with `bindgen` in CI per target, or upstreaming them, is a prerequisite for the Android-first goal. Validate the NDK r27+ toolchain early.
2. **JS debugging.** quickjs-ng 0.16.2 has no debugger API. A DAP adapter needs an engine patch (opcode or line hook), which means maintaining a fork. Is good logging plus stack traces enough for v1?
3. **Interrupt granularity.** The handler fires about every 10k opcodes (~266 µs here). Long-running *native* calls made from JS (a huge `JSON.parse`, a regex) are not interruptible. Mitigation: chunked native APIs, and moving big parses to the compute pool.
4. **`panic = "abort"` in mobile release profiles** turns every Rust panic into a crash. Ljós has to ship guidance (or a check) for `unwind`, and accept that a binary size and performance difference is possible.
5. **Identity heuristics** (`Typed{nth}`) misfire when siblings of the same type are reordered. Should Ljós lint for `id`s on stateful siblings? Delegates depend on models exposing stable keys; HAL-C2's models need a key role.
6. **App Review 2.5.2 ambiguity.** OTA bundles that "change features" can get an app rejected. We need a policy for what goes OTA versus through the store; HAL-C2 should start conservatively (fixes and styling only).
7. **subsecond** is tied to `dx` and alpha releases, patches only the tip crate, and does not work on iOS devices. Treat it as optional, and keep Ljós usable without it.
8. **Single UI-thread ceiling.** If tessellation or text layout of large views (HAL-C2's 342 ListViews) exceeds budget on mid-range Android, we will need the render-thread split anyway. Keep the emit stage free of side effects from day one.
9. **Memory accounting** across the main runtime is global, not per component. Attributing JS memory to boundaries would need a custom allocator (`new_with_alloc`) with tagging. Is that worth it?
10. **quickjs-ng bytecode stability.** `BC_VERSION` can change on any rquickjs upgrade, which invalidates the OTA bytecode in installed bundles. Always ship source as well, and treat bytecode as a cache.

---

## 5. Sources

- rquickjs crate and docs: https://crates.io/crates/rquickjs · https://docs.rs/rquickjs/latest/rquickjs/ · source read from the crates.io tarballs rquickjs-core 0.14.0 and rquickjs-sys 0.14.0 (quickjs-ng 0.16.2 headers)
- quickjs-ng runtime/context concepts: https://www.mintlify.com/quickjs-ng/quickjs/concepts/runtime-and-contexts · https://www.mintlify.com/quickjs-ng/quickjs/api/runtime/memory
- Boa benchmarks: https://boajs.dev/benchmarks · analysis citing boa-dev/data: https://www.issoh.co.jp/tech/details/3100/
- V8 jitless: https://v8.dev/blog/jitless · historical QuickJS benchmark: https://bellard.org/quickjs/bench.html
- tokio 1.53 (coop, LocalRuntime): https://docs.rs/tokio/latest/tokio/task/coop/index.html · https://docs.rs/tokio/latest/tokio/runtime/struct.LocalRuntime.html (source checked locally, tokio-1.53.1)
- egui 0.36 Context repaint API: https://docs.rs/egui/latest/egui/struct.Context.html (source checked, egui-0.36.2)
- egui_mobius: https://github.com/saturn77/egui_mobius · https://crates.io/crates/egui_mobius (source read, 0.6.0)
- kameo supervision: https://docs.rs/kameo/latest/kameo/supervision/index.html · https://docs.rs/kameo/latest/kameo/
- ractor: https://docs.rs/ractor/latest/ractor/
- actix: https://crates.io/crates/actix
- wasmtime Config (epoch, fuel, pooling): https://docs.wasmtime.dev/api/wasmtime/struct.Config.html
- subsecond: https://crates.io/crates/subsecond · https://docs.rs/subsecond (crate docs read from 0.8.0-alpha.1 source) · HN discussion: https://hn.svelte.dev/item/44369642
- hot-lib-reloader: https://crates.io/crates/hot-lib-reloader
- Erlang code loading: https://www.erlang.org/doc/system/code_loading.html · gen_server (`code_change`): https://www.erlang.org/doc/apps/stdlib/gen_server.html · supervisor: https://www.erlang.org/doc/apps/stdlib/supervisor.html · The BEAM Book (scheduling, reductions): https://blog.stenmans.org/theBeamBook/
- Qt QQmlIncubator: https://doc.qt.io/qt-6/qqmlincubator.html · QQmlIncubationController: https://doc.qt.io/qt-6/qqmlincubationcontroller.html
- Felgo hot reload: https://www.qt.io/blog/speed-up-qt-development-with-qml-hot-reload · https://felgo.com/doc/felgo-qml-hot-reload/ · https://blog.felgo.com/qt/qml-hot-reload-with-felgo-live-for-qt · QML Book ch. 19: https://www.qmlbook.org/ch19-felgo-plugins/felgo-plugins.html
- Slint live preview: https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview/
- Flutter hot reload: https://docs.flutter.dev/tools/hot-reload
- React (Native) Fast Refresh: https://reactnative.dev/docs/fast-refresh
- Apple App Review Guidelines (2.5.2, 4.7): https://developer.apple.com/app-store/review/guidelines/ · Apple Developer Program License Agreement §3.3.1(B): https://developer.apple.com/support/terms/apple-developer-program-license-agreement/
- Google Play Device and Network Abuse policy: https://support.google.com/googleplay/android-developer/answer/9888379
- Local measurements: throwaway benchmarks in `/tmp/ljos-research/bench{,2}` (rquickjs =0.14.0, release, AMD Ryzen AI Max+ 395, rustc 1.98.1, gcc 16.2.1)
- HAL-C2 usage counts: `/home/olafura/dev/hal-c2/apps/desktop-qt/qml` (read-only survey)
