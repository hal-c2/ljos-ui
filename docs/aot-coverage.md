# Shipped files compiled ahead of time: what milestone A1 covers and costs

ARCHITECTURE.md §5.4's amendment lets an application compile the bindings and handlers of
the QML files it ships to Rust when it is built. The dynamic tiers stay the exact fallback.
This page measures milestone A1, which is the generator (`ljos_aot`, from the IR only), the
engine's seam and the opt-in through a build script, against what research note 13's spike
measured. The scenarios are `features/language/compiled-shipped-files.feature`.

## What A1 compiles

An expression is compiled when every name it uses has a type the file's declarations or the
type tables give: numbers, bools and strings, the file's own properties and signals, those of
native types the tables know, `qsTr` of one written text, `Math`, and calls of the file's
own functions that compile. A file's code is used only for the file it was made from (its
content key, blake3 of the IR). A file edited while the application runs runs dynamically,
and so does each refused expression. Everything else, including lists, `var`, closures,
`new` and objects as values, is refused by name and runs dynamically.

## The spike's eight cases

`examples/aot_spike` has a fourth mode, `aot`: `qml/World.qml` with the code the generator
made from it in the example's build script, registered as an application registers it. The
dynamic modes run the same file. `aot_bench cases <mode>` takes a fresh world per case,
runs 200 warm-up triggers, then takes 2,000 samples, each one trigger timed alone. It
reports the median and the p99.

These are one run each, with a release build (thin LTO) on an AMD Ryzen AI Max+ 395 (32
threads) shared with other jobs: the load average was 9 to 26. Times are in ns, median /
p99. Lower is better.

| Case | tier 1 | QuickJS | **A1 (`aot`)** | compiled runs per trigger |
|---|---|---|---|---|
| c1 binding, 3 reads | 390 / 531 | 642 / 1,022 | **241 / 351** | 1.00 |
| c2 handler, branch + emit | 872 / 1,142 | 1,272 / 1,784 | **250 / 301** | 1.50 |
| c3 handler, one write | 391 / 441 | 922 / 1,213 | **331 / 431** | 1.00 |
| c4 binding, switch + `qsTr` | 1,022 / 1,503 | 952 / 1,343 | **310 / 421** | 1.00 |
| c5 binding, filter + plural | 1,713 / 2,785 | 1,653 / 2,404 | 1,613 / 2,625 | 0 (refused) |
| c6 handler, list toggle | 2,375 / 3,165 | 2,284 / 2,555 | 2,284 / 3,096 | 0 (refused) |
| c7 handler, keys | 942 / 1,583 | 882 / 1,112 | **251 / 340** | 1.00 |
| c8 handler, model update | 4,929 / 12,012 | 4,939 / 10,189 | 5,110 / 12,774 | 0 (refused) |

c2 runs 1.5 compiled functions per trigger: on half of the triggers its emit starts c3's
handler, which is compiled too. Five of the eight cases compile, and they are 1.6× to 3.8×
faster than tier 1 at the median. That matches note 13's back end B (221 to 341 ns on the
same five). The three refused cases run as they did, within noise. Their reasons, from
`ljos-compile aot examples/aot_spike/qml examples/aot_spike/qml/World.qml`:

- `runText` (c5): it calls `runLabel`, which is not compiled because it has a closure
  (`changes.filter(...)`).
- `onPicked` (c6): a closure, and `var property selected`.
- `onItemsChanged` (c8): a closure is refused first; `new Set` and the `ListModel` calls
  would be refused next.
- The rest of the file compiles: `progress`, `statusText`, `onClicked`, `onKeyPressed`
  (with `leaveSubmenu` inlined) and `editor.onSubmitted`.

The boundary (`aot_bench boundary <mode>`) is the median of 2,000 samples, in ns:

| | tier 1 | A1 |
|---|---|---|
| per tracked read | 37.9 | 21.0 |
| per write | 91.0 | 60.0 |
| per emit, no receivers | 582.0 | 70.0 |
| entering a binding, less its read | 192.1 | 169.0 |

Memory (`aot_bench rss <mode>`, a fresh process each, after 2,000 steps of all eight
cases): tier 1 17,128 KB (HWM 17,128), A1 17,104 KB (HWM 17,108). An earlier run gave
17,140 and 17,792 KB, and that difference did not repeat. The compiled code adds no memory
that this measures.

The spike's test `every_back_end_matches_the_dynamic_tiers` runs the `aot` mode too. It
checks that the trace of every write and emit, and the end state, are the same as
QuickJS's, with the cross-check on in a debug build and no warning.

## The qtdoc corpus

`ljos-compile aot-coverage corpus/qtdoc/examples`, every project of the corpus against
Qt's type tables:

**495 files, 8,765 expressions: 2,174 compiled (24.8 %), 6,591 run dynamically.**

The ten most common reasons an expression is refused, with the most common detail of each:

| Expressions | Reason | Most common |
|---:|---|---|
| 1,974 | `type`: a property whose type is not a number, bool or string | `parent` (QtObject), 1,093 |
| 1,639 | `object as a value`: an id or object passed or compared | `root`, 53 |
| 728 | `member the type table does not have` | `parent`, 232 |
| 616 | `global function call` | the compiler's own `__ljos_single`, 587 |
| 566 | `method call` on an object | `.responsiveSize()`, 164 |
| 264 | `untyped parameter` | `mouse` of `clicked`, 31 |
| 113 | `call of a function that is not compiled` | `updateState`, 8 |
| 94 | `context property` | `topLevel`, 43 |
| 94 | `member of a type` (an enum value) | `ShaderEffectSource.RGBA`, 27 |
| 79 | `alias` | |

Widening the subset to cover more of these comes after A1. `parent` as an object and
objects as values together would cover about 3,600 more expressions.

## What it costs the build

`LJOS_AOT_OUT=corpus.rs ljos-compile aot-coverage …` writes the Rust for all 495 files as an
application's build would: every file's module and the table, 1.24 MB of source. rustc
1.98.1 compiles it as one library crate against `ljos_engine`'s rlib, with 16 codegen units.
CPU time is user + sys of the rustc process (getrusage), less that of an empty crate (0.02 s):

| | whole corpus (495 files) | per file | the spike's two files |
|---|---:|---:|---:|
| opt-level 0 | 1.45 s | 2.9 ms | 0.04 s |
| opt-level 3 | 3.22 s | 6.5 ms | 0.08 s |

The generator itself is fast. `aot-coverage` parses, compiles and generates all 495 files
in 0.18 s of user time.

## The hot paths and the allocation budget

The seam is one flag read for an application without compiled code. It is set only when
code is registered and compiled code is on. There is also one more flag read on a script's
writes and emits, for the cross-check's spies. Callgrind's instructions per round
(`NO_QT=1 mise run bench:js`), on the base commit fb3c07bd and on this branch:

| bench | base | A1 | change |
|---|---:|---:|---:|
| bindings | 89,174 | 89,193 | +0.02 % |
| handlers | 7,194 | 7,190 | −0.06 % |
| props | 6,254 | 6,281 | +0.4 % |
| strings | 6,701,712 | 6,700,302 | −0.02 % |

The wall times are the same within noise (props 337 → 331 ms, handlers 473 → 478 ms).
`cargo test -p ljos_quick --test budget` pins every allocation count exactly, and all six
pins hold: the budget is not worse.

## Against qml6

`mise run bench:js` with Qt 6's `qmltestrunner`, best of 5, in ms (load about 20):

| bench | Ljós | qml6 |
|---|---:|---:|
| bindings | 137 | 203 |
| handlers | 484 | 296 |
| props | 333 | 252 |
| strings | 52 | 96 |

These run in the dynamic tiers: `ljos test` does not compile what it tests. They are the
numbers docs/BENCHMARKS.md recorded before A1, and A1 leaves them where they were.

To compare the spike's cases with qml6, `examples/aot_spike/qt/tst_cases.qml` drives them
from a script, as qml6 can be run (a black box). It times each case's step (its inputs, then
its trigger) over 200,000 rounds, and the inputs alone. qml6 evaluates a binding when its
inputs change, so for c1 and c4 the step is what compares. For the handlers, the step less
the inputs is the trigger. In ns:

| Case | qml6 step | qml6 step − inputs | Ljós dynamic, same file: step − inputs | **A1 trigger** (median, above) |
|---|---:|---:|---:|---:|
| c1 | 760 | | 530 | **241** |
| c2 | 690 | 530 | 1,010 | **250** |
| c3 | 570 | 510 | 775 | **331** |
| c4 | 1,160 | | 1,050 | **310** |
| c7 | 900 | 545 | 1,500 | **251** |

**Compiled, the handlers run in half of qml6's time or less, and the bindings in under half
of qml6's step.** The two harnesses differ. A1's trigger is started from Rust through the
engine's API, which looks the name up, while qml6's is started from JIT-compiled script. So
the comparison favours neither by much, and the margin is wider than the difference. Driven
from script and run dynamically, Ljós is behind qml6 on handlers, as bench:js says too. The
gap is in the dynamic tiers, and A1 does not change it for files that are not compiled.

`ljos test` does not yet resolve `import "../qml"` from a test directory, which qml6 does.
The Ljós column was run on a copy with `World.qml` beside the test.

## Reproducing

```sh
cargo build --release -p ljos_example_aot_spike -p ljos_compiler -p ljos_cli
target/release/aot_bench cases aot        # and tier1, quickjs
target/release/aot_bench boundary aot     # and tier1
target/release/aot_bench rss aot          # and tier1
target/release/ljos-compile aot-coverage corpus/qtdoc/examples
LJOS_AOT_OUT=corpus.rs target/release/ljos-compile aot-coverage corpus/qtdoc/examples
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input examples/aot_spike/qt
NO_QT=1 mise run bench:js bindings handlers props strings
cargo test -p ljos_quick --test budget
```
