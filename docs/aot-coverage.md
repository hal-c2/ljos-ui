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

### What the cross-check leaves unchecked

The cross-check runs compiled code dry first, holding its writes and emits. A read after
a held effect cannot see what the effect would have done, so that run is counted
`inconclusive` and not compared. Of the 2,174 compiled expressions, 126 write or emit,
and 11 of those (in 4 files: `MapLayersDrawer`, `photosurface`, `CalendarControl`,
`CustomizationCard`) read a property after their first effect. That is an upper bound:
it counts a read in one branch after a write in another. It holds because compiled code
has no loops. The other 2,163 can be checked whenever they run.

The 11 are left unchecked rather than given held values. A held write's handlers and
dependent bindings do not run in a dry run, so any read after it could differ from what
the script reads, including a read of the property it wrote.

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

## Against Qt's compiled QML

The qml6 numbers above are Qt's interpreter and JIT. An application built with CMake's
`qt_add_qml_module` is compiled too: qmlcachegen turns the bindings and functions it can
type into C++ at build time, and the rest run as bytecode. `examples/aot_spike/qt-aot` is
that build for the spike's cases. It compiles `qml/World.qml` (the same file A1 compiles)
and `WorldTyped.qml`, a copy with the annotations qmlcachegen asks for, through
`qt_add_qml_module`. Its `main.cpp` drives them from C++ the way `aot_bench` drives Ljós. It
writes the inputs with `QObject::setProperty` by name, then reads the binding or emits the
handler's signal by name (`QMetaObject::invokeMethod`), and reads what the handler wrote
after the clock stops. The same file can be loaded uncompiled, from its path with
`QML_DISABLE_DISK_CACHE=1`, which runs it as bytecode through Qt's interpreter and JIT.
`QV4_FORCE_INTERPRETER=1` keeps it in the interpreter. Qt is used as a black box: only its
documented API and the reports of qmlcachegen and qmlaotstats were read, never the C++ it
generates.

Both drivers end in the same state after 2,000 steps of all eight cases:
`aot_bench snapshot aot` and `qt_aot_bench snapshot module:World` print the same values,
model rows included.

### The cases

Qt evaluates a binding when its inputs change, and Ljós when it is read. So reading
`progress` in Qt (c1, c4, c5) only fetches a value already computed. What compares is the
step: the inputs and the trigger together, inside the clock. `aot_bench steps <mode>` and
`qt_aot_bench steps <source>` time it, with the same 200 warm-up steps and 2,000 samples
as `cases`. Each cell is the median over three interleaved runs of each run's median /
p99, in ns. For 34 of the 40 cells, the three runs' medians are within 10 % of each other.
The widest spreads are Qt compiled c5 (3,436 to 4,228) and c7 (531 to 732), and annotated
c6 (1,072 to 1,332). The machine is the one above, with a load average of 15 to 25. Lower is better.

| Case | Ljós tier 1 | **Ljós A1** | qml6 uncompiled (JIT) | qml6 compiled, file as is | qml6 compiled, annotated |
|---|---|---|---|---|---|
| c1 binding, 3 reads | 681 / 892 | **571 / 761** | 631 / 1,072 | 501 / 731 | 511 / 721 |
| c2 handler, branch + emit | 972 / 1,203 | **370 / 441** | 561 / 1,753 | 430 / 1,513 | 330 / 601 |
| c3 handler, one write | 381 / 431 | **331 / 381** | 310 / 1,333 | 300 / 1,403 | 210 / 1,382 |
| c4 binding, switch + `qsTr` | 1,132 / 1,543 | **501 / 551** | 1,052 / 2,805 | 1,372 / 3,196 | 1,313 / 2,665 |
| c5 binding, filter + plural | 2,284 / 3,737 | 2,305 / 3,637 | 2,886 / 6,142 | 3,547 / 7,294 | 4,388 / 8,336 |
| c6 handler, list toggle | 2,245 / 3,116 | 2,255 / 3,206 | 791 / 3,506 | 801 / 3,096 | 1,082 / 3,346 |
| c7 handler, keys | 1,293 / 1,724 | **651 / 742** | 531 / 1,714 | 551 / 1,613 | 390 / 1,353 |
| c8 handler, model update | 4,960 / 10,519 | 4,889 / 10,099 | 4,769 / 16,281 | 4,548 / 12,304 | 4,308 / 11,351 |

With Qt kept in its interpreter, the steps are within 6 % of the JIT column (c1 661, c4
1,082, c5 2,916, c6 781, c7 551).

The trigger alone, as A1's table above times it (median / p99, ns):

| Case | Ljós tier 1 | Ljós A1 | qml6 uncompiled | qml6 compiled | qml6 compiled, annotated |
|---|---|---|---|---|---|
| c1 | 381 / 491 | 241 / 331 | 80 / 110 | 70 / 110 | 70 / 100 |
| c2 | 841 / 1,132 | 250 / 301 | 481 / 1,743 | 380 / 1,623 | 290 / 441 |
| c3 | 370 / 430 | 331 / 381 | 291 / 1,302 | 291 / 1,503 | 210 / 1,342 |
| c4 | 922 / 1,252 | 320 / 421 | 100 / 160 | 100 / 150 | 100 / 130 |
| c5 | 1,683 / 2,655 | 1,673 / 2,655 | 91 / 141 | 100 / 131 | 91 / 131 |
| c6 | 2,244 / 3,065 | 2,214 / 2,775 | 761 / 2,855 | 751 / 2,836 | 1,072 / 3,135 |
| c7 | 872 / 1,232 | 260 / 320 | 381 / 611 | 381 / 581 | 240 / 371 |
| c8 | 4,919 / 10,500 | 4,939 / 9,879 | 4,508 / 14,047 | 4,559 / 11,963 | 4,278 / 11,101 |

Against Qt compiling the same file, the step medians come out as follows:

- **Ljós wins** c2 (370 against 430), c4 (501 against 1,372, 2.7×) and c5 (2,305 against
  3,547). c5 is a case A1 refuses, so Ljós runs it dynamically and is still faster there.
- **Qt wins** c6 by 2.8× (801 against 2,255). It also wins c1, c3, c7 and c8, by 7 to 18 %.
- **Annotated, Qt wins more.** It takes c2 (330 against 370), c3 (210 against 331) and c7
  (390 against 651). Annotating made Qt's c5 slower (4,388) and its c6 slower (1,082): the
  binding and the handler compile, but the functions they call (`runLabel`, `toggled`) do
  not, and the compiled code calls into bytecode.
- **The tail is mostly Ljós's.** A1's p99 is lower than every Qt column in six of the
  eight cases. The exceptions are c1 (761 against 721 to 731) and the refused c6 (3,206
  against compiled Qt's 3,096), each within 6 %. Qt's c2 and c3 have p99s of 1.3 to 1.8 µs
  on medians of 0.2 to 0.6 µs (annotated c2: 0.6 µs). On the cases A1 compiles, its p99 is
  at most 1.35× its median.

The two harnesses differ in one way that counts against Ljós. Each of `aot_bench`'s
input writes finds its object by `objectName`, `root` included (a tree walk from the root
that reads `objectName`). Qt's driver writes `root` through a pointer it already holds.
Both find `editor` and `field` by name. Steps with inputs on `root` (c1, c2, c4, c5, c7)
therefore carry one lookup per write in Ljós's column only. On c2, the step less the trigger
is 120 ns in Ljós and 50 ns in Qt, for one bool write.

### What compiled, case by case

qmlcachegen's own report (`--dump-aot-stats`, formatted by `qmlaotstats`) for
`World.qml` as it is: **7 of 18 bindings and functions compiled to C++**. For the annotated
copy: **12 of 18**. qmlaotstats lists a handler as two entries. A1 compiles the five
cases it compiles from the file as it is, with no annotation.

| Case | qmlcachegen, file as is | qmlcachegen, annotated | A1 |
|---|---|---|---|
| c1 `progress` | compiled | compiled | compiled |
| c2 `onClicked` | compiled | compiled | compiled |
| c3 `onSubmitted` | compiled | compiled | compiled |
| c4 `statusText` | no: "call to untyped JavaScript function"; `statusLabel`: "Functions without type annotations won't be compiled" | compiled, with `statusLabel` | compiled, `statusLabel` inlined |
| c5 `runText` | no: "call to untyped JavaScript function" | binding compiled; `runLabel` not: "Cannot generate efficient code for LoadClosure" | refused: `runLabel` has a closure |
| c6 `onPicked` | one entry compiled, one not: "call to untyped JavaScript function"; `toggled` not annotated | both entries compiled; `toggled` not: "Cannot generate efficient code for LoadLocal" | refused: a closure, and `var property selected` |
| c7 `onKeyPressed` | compiled; `leaveSubmenu` not: "function without return type annotation returns bool" | compiled, with `leaveSubmenu` | compiled, `leaveSubmenu` inlined |
| c8 `onItemsChanged` | no: "Cannot load property length from World::items with type QVariant" | the same | refused: a closure first |
| `changes`, `selected`, `items` (list initialisers) | no: "Cannot generate efficient code for storing an array in a non-sequence type" | the same | refused: lists |

What Qt needed for c4, c6 and c7 was type annotations on the functions' parameters and
returns. Its handlers' parameters needed them too, and an arrow function's parameters take
no annotation, so the handlers had to be rewritten from arrow functions to `function(...)`
(`qt-aot/WorldTyped.qml` is the file with them). A1 needed nothing. A handler's
parameters take their types from the signal's declaration. A function with parameters is not
compiled on its own; A1 inlines it into a compiled caller, whose arguments have types.

### The qtdoc corpus

qmlcachegen ran over the same 495 files A1 counted, one file at a time. Each run had the
project's directory (the nearest `CMakeLists.txt`) and the file's own directory as import
paths, and `--dump-aot-stats`. qmlaotstats then aggregated the results. Building every
example with CMake instead would need their C++ backends and Qt modules that are not
installed here.

| Files | Qt: bindings and functions compiled to C++ | Ljós A1: expressions compiled |
|---|---:|---:|
| all 495 | 3,974 of 11,425 (**34.8 %**), and 310 more qmlaotstats calls skipped (33.9 % counting them) | 2,174 of 8,765 (**24.8 %**) |
| the 253 whose imports all resolved for qmlcachegen | 2,120 of 4,024 (**52.7 %**), 221 skipped | 1,030 of 3,620 (**28.5 %**) |

The 242 other files fail at least one import: QtQuick3D (91 files), QtGraphs and
QtLocation are not installed, and modules that an example declares only in its CMake
(`Thermostat`, `ToDoList`, `Config`, …) have no `qmldir` in the source tree. Every member of
an unresolved type is unknown to qmlcachegen, so the 253 files are the like-for-like set.
Ljós's count uses its Qt type tables for all 495.

The two units differ:

- **What one count is.** qmlaotstats counts a binding or a function as one, however large,
  and a signal handler as two entries. A1 counts each expression of the file's IR: every
  binding, handler and function. A function with parameters counts as refused even when it
  runs compiled, inlined into its callers (`statusLabel`, `leaveSubmenu`).
- **What happens to the rest.** Qt runs the rest as bytecode, through its interpreter and
  JIT, and a compiled binding can call an uncompiled function, as c5 and c6 show. A1 runs
  the rest in tier 1 or QuickJS, and a compiled expression calls only functions it can
  inline.

On the files both can resolve, Qt compiles about half and A1 a bit over a quarter. The most
common refusals differ too. Qt refuses for members of types it could not resolve and for
functions without annotations (127 entries say so). A1 refuses for objects as values and
for non-scalar property types (the table above).

### Memory, size, build time and edits

RSS (`rss`, a fresh process each, two runs, after 2,000 steps of all eight cases), in KB:

| | at start | world loaded | after the steps | growth in the steps |
|---|---:|---:|---:|---:|
| Ljós tier 1 | 4,060 to 4,240 | 13,776 to 14,332 | 17,760 to 17,772 | 3,428 to 3,996 |
| Ljós A1 | 3,928 to 4,312 | 13,704 to 13,728 | 17,052 to 17,716 | 3,348 to 3,988 |
| Qt compiled | 23,460 to 23,592 | 36,480 to 36,644 | 38,640 to 38,804 | 2,160 |
| Qt compiled, annotated | 23,464 to 23,500 | 36,488 to 36,520 | 41,628 to 41,656 | 5,136 to 5,140 |
| Qt uncompiled | 23,404 to 23,516 | 38,868 to 38,964 | 41,028 to 41,124 | 2,160 |

The totals are not like for like. `aot_bench` uses a stand-in `Item` and no renderer, and it
carries the QML compiler. Qt's process has QtGui and QtQuick mapped before `main` runs.
The growth during the steps is comparable, and there Qt with the file as it is grows less:
2.2 MB against A1's 3.3 to 4.0 MB.

Size and build time for `World.qml` alone. Ljós's module is the generator's `f0` of
`shipped.rs`, compiled as one rlib against `ljos_engine` (16 codegen units). Qt's object is
qmlcachegen's translation unit compiled with the build's own flags. CPU time is user + sys
of the child (getrusage), median of 5:

| | Qt | Ljós A1 |
|---|---:|---:|
| object size (`size`: text + data + bss) | 27,648 B (`-O3`) | 6,227 B (`-O3`), 17,900 B (`-O0`) |
| generator CPU | qmlcachegen 0.057 s | about 0.4 ms (all 495 files take 0.18 s) |
| compiler CPU, `-O0` | c++ 1.15 s (not less anything: the headers are part of each file's cost) | rustc 0.059 s (0.074 s, less 0.015 s for an empty crate) |
| compiler CPU, `-O3` | c++ 1.36 s | rustc 0.081 s |

Qt's object also carries the file's bytecode, which it runs for what it did not compile. A1's
module does not carry the file's IR, which the application ships anyway. Qt compiles each
QML file as its own C++ translation unit, so the header cost is paid once per file. Ljós
puts every file's module in one crate, at 2.9 ms (`-O0`) to 6.5 ms (`-O3`) per file over
the corpus.

An edit without a rebuild:

- **Qt.** `World.qml` with "In progress" changed, run without a rebuild: `module:World`
  still says "In progress". The file is compiled into the binary's resources, and the
  source is not read. Loaded from its path, the edit shows, but nothing of the file is
  compiled.
- **Ljós.** An application that loads an edited file runs it dynamically, and the rest
  stays compiled. The content key does not match, so the edited file's compiled code is
  not used (`features/language/compiled-shipped-files.feature`, "A file edited while the
  application runs gives what its new text says").

### Where Ljós is behind, and what would close it

- **Lists and closures (c6, and c5 and c8 once compiled).** Qt runs c6's list toggle in
  0.8 µs even uncompiled. Ljós takes 2.2 µs in both tiers, because A1 refuses it and the
  dynamic tiers pay for the `var` list on every call. To close it, A1 would compile typed
  lists and arrow functions over them, the `filter`/`includes`/spread subset, which would
  also take in c5. Short of that, the dynamic tiers would need to stop converting a `var`
  list at every boundary crossing.
- **Entering a compiled handler (c3, c7 annotated, c1).** A1's boundary costs 169 ns to
  enter a binding, less its read, and 60 ns per write. c3, which is one write, costs 331 ns
  against annotated Qt's 210. Resolving the slot once, and passing the arguments without
  boxing them as `Value`s, would likely close it. That is untested. Taking the harness's lookup of `root` out
  of Ljós's steps would show how much of c1 and c7 is the harness.
- **Memory grown in the steps** is 3.3 to 4.0 MB against Qt's 2.2 MB, and has not been
  looked into. The refused cases' QuickJS heap is the first suspect.
- **Coverage.** On the files both resolve, Qt compiles 52.7 % of bindings and functions,
  and A1 compiles 28.5 % of expressions. A1's top refusals (objects as values, `parent`)
  are the widening already planned. Qt reaches its figure by asking for annotations, which
  A1 does not.

Where A1 compiles a case, it beats Qt's compiled file as it is on c2 and c4, by 1.2× and
2.7×. Its tail is shorter on every case. A1 needs no annotations and no rebuild after an
edit, and its compiled module is a quarter of the size. Its build is 15 to 20× cheaper per
file.

## Reproducing

```sh
cargo build --release -p ljos_example_aot_spike -p ljos_compiler -p ljos_cli
target/release/aot_bench cases aot        # and tier1, quickjs
target/release/aot_bench steps aot        # and tier1: inputs and trigger together
target/release/aot_bench snapshot aot     # the end state, to check another driver
target/release/aot_bench boundary aot     # and tier1
target/release/aot_bench rss aot          # and tier1
target/release/ljos-compile aot-coverage corpus/qtdoc/examples
LJOS_AOT_OUT=corpus.rs target/release/ljos-compile aot-coverage corpus/qtdoc/examples
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input examples/aot_spike/qt
NO_QT=1 mise run bench:js bindings handlers props strings
cargo test -p ljos_quick --test budget

# Qt's compiled QML (Qt 6.8 or later, CMake, Ninja)
cmake -S examples/aot_spike/qt-aot -B qt-build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build qt-build && cmake --build qt-build --target all_aotstats
export QT_QPA_PLATFORM=offscreen
qt-build/qt_aot_bench steps module:World  # and module:WorldTyped, examples/aot_spike/qml/World.qml
qt-build/qt_aot_bench snapshot module:World
QV4_FORCE_INTERPRETER=1 qt-build/qt_aot_bench steps examples/aot_spike/qml/World.qml
LJOS_AOT_FILES=1 target/release/ljos-compile aot-coverage corpus/qtdoc/examples 2> per-file.tsv
```
