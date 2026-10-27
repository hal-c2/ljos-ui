# Compiling shipped scripts ahead of time: a spike of two back ends

Research note, 2026-10-27. It measures; the recommendation at the end is the writer's, for
the lead to take or leave.

ARCHITECTURE.md §5.4 rejects "typed-only AOT to Rust (it needs annotations and works against
hot reload)". Note 12 left open what compiled code would pay at the engine's boundary. This
spike compiles eight scripts from note 11 two ways and runs them against the real object
store next to tier 1 and QuickJS:

- **A, scriptc native through FFI.** scriptc 0.4.5 compiles the scripts into a static
  library. The engine calls its exports, and the compiled code reads, writes and emits
  through callbacks into the engine.
- **B, typed subset to Rust at build time.** A `build.rs` reads the same TypeScript with the
  compiler's own parser and writes Rust. The Rust calls `Engine::read_number`, `write`,
  `emit` and `invoke` directly, with no foreign runtime and no second heap.

The code is in `examples/aot_spike` (crate `ljos_example_aot_spike`). Nothing depends on it.
A is behind the `scriptc` feature, so the workspace builds without scriptc installed.

## The cases

Each case is a TypeScript file in `cases/`, written in note 11's typed subset. A name like
`root__progress` is a binding of `root.progress`; a name like `root__onPicked` is the
handler of `root.picked`. `cases/world.d.ts` declares the objects as typed constants; in a
real build these would be generated from the QML property types. The dynamic version of
every case is `qml/World.qml`. The world that the compiled versions fill in is
`qml/Shell.qml`, the same objects with the bindings and handlers removed.

| Case | What it is | Shape asked for |
|---|---|---|
| c1 `root__progress` | `total > 0 ? Math.min(1, done / total) * bar.width : 0` | a binding reading several properties |
| c2 `root__onClicked` | on a mode, emit one signal or another | a branch on a mode, a one-line handler |
| c3 `editor__onSubmitted` | `root.lastMode = mode` | a one-line handler that writes |
| c4 `root__statusText` | `switch` over a status string to `qsTr` labels | a branch on a mode, strings |
| c5 `root__runText` | count kept changes with `filter`, then plural text with `qsTr(...).arg(n)` | string formatting, a list filter |
| c6 `root__onPicked` | toggle a value in a string list (`includes`, `filter`, spread) | a list filter |
| c7 `root__onKeyPressed` | Ctrl+digit, Down, Backspace on an empty field, else ignore | a branch, several writes |
| c8 `root__onItemsChanged` | keep a `ListModel` in step with a list of ids: remove, move, insert | a model update |

Each step of the drive sets a case's inputs (not timed), then triggers it (timed). A
trigger reads the binding, emits the signal, or writes `items` for c8. The inputs cycle
through the values in `prepare` in `src/lib.rs`: three totals including 0, six statuses
including one with no label, four change lists including an empty one, and five id lists
that remove, reorder, insert and empty the model.

## Machine and method

- **Machine.** `ai-beast`: AMD Ryzen AI Max+ 395 (32 threads), 62 GiB, Linux 7.2.5,
  performance governor. Rust 1.98.1, release profile (thin LTO). scriptc 0.4.5 (`release`
  optimisation, LLVM emission).
- **Load.** The machine was shared with other jobs throughout, with a load average of
  43–55 on 32 threads. This is the main source of noise below. Every number is one run of
  the command shown; nothing was repeated to smooth it.
- **Time per case.** `aot_bench cases <mode>`: a fresh world per case, 200 warm-up
  triggers, then 2,000 samples, each one trigger timed alone with `Instant`. The table gives
  the median and p99.
- **Boundary.** `aot_bench boundary <mode>` gives the median of 2,000 samples. Reads are
  measured in a binding: one read, then 33 reads, re-evaluated after a write to what they
  read; the cost per read is `(t33 − t1) / 32`. Writes and emits in A and B are 1 against
  33 in one call. In tier 1 and QuickJS they are a handler that writes or emits once,
  against an empty handler, each triggered by the same write.
- **Memory.** `aot_bench rss <mode>`, a fresh process per mode, reads `VmRSS` and `VmHWM`
  after 2,000 steps of all eight cases. All modes are the same binary.
- **Collector.** `aot_bench pauses` records the slowest of 16,000 A triggers, then times
  a forced full collection.

## Results

### The table

Times are in ns, median / p99 per trigger. Lower is better.

| | tier 1 | QuickJS | A (scriptc) | B (Rust) |
|---|---|---|---|---|
| c1 binding, 3 reads | 541 / 1,132 | 621 / 972 | 210 / 531 | 250 / 561 |
| c2 handler, branch + emit | 1,102 / 1,864 | 1,323 / 2,404 | 240 / 370 | 221 / 461 |
| c3 handler, one write | 461 / 942 | 922 / 1,814 | 351 / 662 | 341 / 641 |
| c4 binding, switch + `qsTr` | 1,012 / 2,164 | 972 / 1,913 | 220 / 381 | 261 / 541 |
| c5 binding, filter + plural | 2,093 / 3,677 | 1,963 / 3,917 | 380 / 702 | 381 / 801 |
| c6 handler, list toggle | 2,504 / 4,038 | 2,244 / 3,236 | 521 / 922 | 441 / 802 |
| c7 handler, keys | 982 / 1,904 | 882 / 1,323 | 241 / 431 | 200 / 401 |
| c8 handler, model update | 7,133 / 18,013 | 5,300 / 12,453 | 1,914 / 5,841 | 1,833 / 6,072 |
| **Boundary**, ns per tracked read | 36.9 | 40.1 | 11.3 | 6.5 |
| ns per write | not separable (see below) | 150 | 64 | 61 |
| ns per emit (no receivers) | 451 | 361 | 73 | 42 |
| entering a binding, less its read | 204 | 390 | 119 | 105 |
| **Size**, bytes of code per case (c1…c8) | 0 (IR) | 0 (IR) | 1,460 · 1,336 · 1,528 · 2,105 · 2,783 · 3,800 · 2,189 · 2,747 | 221 · 1,280 · 571 · 2,378 · 1,627 · 1,774 · 2,434 · 4,338 |
| total for the eight | 4,267 B of QML text | same | 42.8 KB (9.4 KB scriptc + 33.4 KB Rust glue) | 29.9 KB (23.3 KB generated + 6.6 KB helpers) |
| fixed runtime | in the engine | QuickJS, in the engine | ≈ 170 KB code and data + 844 KB zeroed (bss) | none beyond the helpers |
| **Memory**, RSS after 2,000 steps (HWM) | 17,268 KB (17,272) | 17,364 KB (17,364) | 15,004 KB (15,008) | 15,444 KB (15,448) |
| collector pause in a 4 ms frame | — | QuickJS GC, not measured here | none seen: worst trigger 0.12 ms, full collection 2.3 µs | no collector |
| **Build time** added per case | 0 | 0 | 0.16–0.3 s per library build typical (spikes to 2.8 s) | generator 20–300 µs; rustc share below noise |

**Tier 1 is mostly QuickJS here.** Tier 1 completed every c1 and c3 trigger and half of
the c2 triggers (one branch). Every other case fell to QuickJS even in tier-1 mode (the
`tier1_runs_per_trigger` column of `aot_bench cases`). So the tier-1 column is tier 1
for c1–c3 only, and QuickJS with a failed tier-1 attempt for the rest.

**How noisy.** The boundary was run twice: once with a measurement bug in the entry row
(fixed before the run shown), and once more after the fix. The two runs differ by up to
40 % in the dynamic tiers (tier 1 read: 51 then 37 ns; QuickJS write: 250 then 150 ns)
and up to 35 % in A and B (B read: 10.3 then 6.5 ns). Load is the cause: the median of
2,000 samples does not hide another process taking the core. The order of the modes did
not change between the runs. Treat differences under about 1.5× as noise, especially
between A and B, which are within noise of each other on every case. The p99 values carry
more of the load than the medians.

**The write row.** In tier-1 mode, the handler that writes ran in tier 1 (2,199 of 2,200).
The empty handler it is compared with has no tier-1 count, so it probably went through
QuickJS. The difference came out at 0 ns, which is the cost of two different paths, not of
a write. QuickJS's 150 ns is the same comparison with both handlers in QuickJS. A and B
measure the write itself, including its change notification.

**Where size comes from.** For A, the stripped bench binary grows by 213,272 bytes with
the feature on. Of that, 9,420 bytes are the compiled cases (`program.o` `.text`, which
also holds the probes and `init`) and 33,350 bytes are the Rust glue in `a.rs`. Each case's
closure is 1.1–2.0 KB, because it reads its strings before the call and copies the result
after. The rest, about 170 KB, is scriptc's runtime. On top of that the bss grows by
864,224 bytes, which is mostly the cycle collector's tables (`scr_cycle.o`). The archive
scriptc hands over is 3.3 MB; the linker keeps about a twentieth of it. For B, the sizes
are `nm` sizes of each case's `#[inline(never)]` functions plus its export closure. c4's
`statusLabel` is counted once, in c4, though c5 calls it.

**Memory.** A and B both load `Shell.qml`, which has no script functions; tier 1 and QuickJS
load `World.qml`, which has them. That accounts for the 1.8–2.4 MB difference. A against B
(0.4 MB) is within what one process differs from the next. Very little of A's 844 KB of bss
is touched, so it barely shows in RSS.

**A's collector.** scriptc frees by reference count and collects cycles in a full pass. In
16,000 triggers of all eight cases the p99.9 was 8.4 µs. The slowest was 121.9 µs, on c8,
which on a machine this loaded cannot be told apart from a preemption. A forced full
collection after the run took 2.3 µs (a second one, 0.06 µs). No pause came near a 4 ms
frame. The cases make little garbage, however: c6's arrays and c8's `Set` are the only
allocations. This does not show what a script that builds large object graphs would pay.

**Build time.** A, with scriptc's cache bypassed (`SCRIPTC_NO_CACHE=1`), one library per
case, in ms: no case 419, c1 635, c2 317, c3 185, c4 183, c5 2,766, c6 231, c7 161,
c8 178; all eight in one library, 265. The same builds with the cache on ran from 129 ms to
7,549 ms in no order the inputs explain, so the spread is the machine's load. A library
build is a fraction of a second, but the app still has to relink against it. B: the
generator itself took 20–302 µs per case (`CODEGEN_NS` in the generated file). Rebuilding
the crate with one case at a time (`AOT_CASES`) took 5.8–23.3 s: with no cases 12.8 s,
with all eight 5.8 s. The share of a single case does not show above that noise. B wrote
296 lines (14 KB) of Rust for 3.6 KB of TypeScript.

### The boundary in one sentence

A tracked read from compiled code costs 6–11 ns, against 37–40 ns from the dynamic tiers.
A write costs about 60 ns in either back end, most of it the engine's own write and notify.
An emit costs 42 ns from B and 73 ns from A, where A's emits are queued and delivered after
the call. Entering A's library costs 30 ns on top of entering a Rust closure (an export that
does nothing: 30 ns). That is the number note 12 could not give.

## Fallback: one compiled case swapped for its dynamic version

`tests/fallback.rs` runs every mode on the same 24 steps of all eight cases. It records each
change of every property of `root` with its value, each emit of each signal with its
arguments, and each change of the model's count. It then compares the log and the end state
of every property and of the model with QuickJS's.

- **Every back end matches.** Tier 1, A and B produce the same log as QuickJS, entry by
  entry: about 300 entries of values and signal order. They also produce the same end
  state. The tier-1 cross-check (`set_tier1_check`, which is what `LJOS_TIER1_CHECK=1`
  sets) is on in every run. The test asserts that every tier-1 run was checked against
  QuickJS: 76 runs, with no mismatches. No warning is logged.
- **Swapped mid-run, still the same.** At step 12, the test replaces two compiled cases
  with their dynamic versions, as a user's edit on the device would. These are c6 (a
  handler) and c1 (a binding). `qml/Swap.qml` holds the two as QML. It is compiled at load,
  as a build that ships IR next to native code would, and instantiated at the swap. The
  handler is replaced inside the slot it was connected through, so it keeps its place in
  the connection order. The binding is replaced with `Qt.binding`. Both A and B, swapped,
  give the log and end state of the unswapped QuickJS run.
- **One difference was found, and it is the engine's, not the back ends'.** The dynamic
  world runs `onItemsChanged` once while it is being created: the `var`'s first value
  counts as a change. A handler connected after creation does not see that change. The
  harness sets `syncs` to 0 after load in every mode. A compiled build that installs its
  handlers before the creation completes would need to match this.

`cargo test --release -p ljos_example_aot_spike --test fallback` passes with and without
`--features scriptc`.

## What each back end can't compile

**B** compiles all eight cases as written. Its generator names and refuses the following
(`build/gen.rs`); a case that hits one of these stays dynamic:

- Union types, including string-literal unions: the cases type a status as `string`, not
  `"todo" | "doing"`.
- `null`; `==` (it converts; the subset takes `===`).
- Template literals; object literals, except a row passed to a model.
- Function values, except a `filter` predicate whose body is one expression.
- Destructuring; postfix `++`/`--`; labelled `break` and `continue`.
- `while`, `do`, `try` and `throw` (any statement besides `if`, `for`, `switch`, `return`,
  `break`, `continue`, declarations and expressions); a `case` that falls through.
- `new`, except `Set` of numbers.
- A parameter without a type, or a function without a return type.
- `.arg` uses the plain number for both `%N` and `%L`. That is wrong once a locale formats
  numbers differently; it can be fixed by calling the engine's localized form, but the spike
  didn't.

The annotation burden that §5.4 names is real but small. The eight cases needed only
parameter and return types. Everything the cases read was typed by `world.d.ts`, which a
build can generate from the QML property declarations.

**A** compiles all eight only after they were rewritten by hand (`scriptc/cases.ts`). An
AOT step would have to generate that rewrite, and what it changes is the list:

- **Objects become numbered slots.** scriptc has no view of the engine's objects. Each
  read, write and emit is a callback with a slot number, and each model row field is a
  callback (`modelRowNum`).
- **A callback cannot return a string or a list** (callback returns are scalar or void).
  Every string a case reads is read by the host before the call and passed in. For c6, that
  includes a list of strings joined with U+001F. For c7, it means reading `field.text` even
  on the branches that don't use it. In a binding this would over-track: the binding would
  depend on properties it doesn't read.
- **`qsTr` can't reach the catalogue**, for the same reason, so A has it as the identity.
  That is wrong as soon as a translation is installed. **`.arg` became `replace("%1", …)`**,
  which is wrong for `%L`, for `%10`, and for number formatting.
- **The library can't be entered again from a callback.** c2 emits `editor.submitted`,
  whose handler, c3, is also compiled. Emits are therefore queued and delivered after the
  export returns. That gives the same order here, because the emit is c2's last act. A case
  that emits and then writes something a compiled handler of that signal reads would see a
  different order. The same rule means a compiled handler cannot write a property that an
  observed compiled binding reads: the engine would evaluate the binding inside the
  callback. Bindings are lazy in this engine, so nothing in the drive hit this, but
  `a.rs` would stop with "entered again" if one did.
- **One instance.** The library is initialised per thread. A trap aborts the instance (the
  sink aborts the process here; scriptc's docs say to discard it). The two fallback tests
  take turns on a mutex.
- **Profile format.** The installed scriptc reads profile formats 1 and 2. The repository's
  docs describe format 3, for string callback parameters, and the build accepted string
  parameters in format 2 anyway.

## What this says about the §5.4 rejection

The rejection gives two reasons.

- **"It needs annotations."** The typed subset needed parameter and return types only.
  Object and property types come from the QML declarations the engine already has, here in
  a `world.d.ts` that a build would generate. A script that can't be typed, or uses
  something on the refused list, isn't compiled and stays dynamic. This costs nothing at
  run time, because the fallback is exact.
- **"It works against hot reload."** It doesn't, if every compiled case keeps its dynamic
  version. A user's edit swaps the compiled code for the IR of the edited file. The
  property values and signal order were identical before and after the swap, checked entry
  by entry. What hot reload still costs is that an edited file runs at dynamic speed until
  the next build. That is no worse than today.

What it buys, on these cases, against what runs today:

- 2–5× less time per case: c5 from 2.1 µs to 0.38 µs, c8 from 7.1 µs to 1.8 µs.
- 3–6× cheaper tracked reads.
- About 2 MB less resident memory, because the shipped scripts aren't instantiated as
  QuickJS functions.

The cost is 0.2–4 KB of code per case for B, and a build step per shipped file.

## Recommendation

Reopen the §5.4 rejection for shipped files only. Keep IR, tier 1 and QuickJS as the
fallback for edited files and for whatever the subset refuses. Take back end B. A was as
fast as B, but it carries 170 KB of runtime and 844 KB of collector tables, and its ABI
gets the wrong answer in four places (translations, `.arg`, emit order, re-entry). B uses
the engine's own values, catalogue and call paths.

Before B is built for real:

- Lower from the IR, not from TypeScript, so the compiled and the dynamic paths start from
  the same resolved names, as tier 1 does.
- Generate `world.d.ts` from the QML declarations.
- Install compiled handlers before creation completes, so creation-time changes behave as
  they do in the dynamic world.
- Measure the per-case rustc cost on a quiet machine. This spike couldn't see it under
  load.
