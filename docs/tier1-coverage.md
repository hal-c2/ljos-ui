# Tier-1 coverage

How much of real QML runs in the binding VM (tier 1, ARCHITECTURE.md §5.4) rather than in
JavaScript (tier 2), why the rest does not, and what the VM is worth. Measured on 2026-10-11 at
`IR_VERSION` 5. Tier-1 coverage is an M1 exit criterion; this page is where the number is published.

An expression is tier 1 only when all of it is in the subset. There is no partial compilation: one
construct outside the subset leaves the whole expression to tier 2, and the reason is counted.
A bare property path (`parent.width`) is tier 0 at run time and is counted as tier 1 here, since
it never reaches JavaScript either.

## The numbers

| Body of QML | Files | Bindings | Handlers | Functions | All |
|---|---|---|---|---|---|
| `corpus/qtdoc` at `e05e4f4` | 524 | 8548 / 9594 (**89.1 %**) | 157 / 762 (20.6 %) | 93 / 338 (27.5 %) | 8798 / 10694 (82.3 %) |
| `features/` (the QML of every scenario) | 1123 | 841 / 1036 (**81.2 %**) | 300 / 365 (82.2 %) | 262 / 420 (62.4 %) | 1403 / 1821 (77.0 %) |
| `features/` without `bindings/expressions.feature` | 1030 | 747 / 942 (79.3 %) | 300 / 365 (82.2 %) | 78 / 236 (33.1 %) | 1125 / 1543 (72.9 %) |

Bindings are the number that matters: they re-run whenever a property they read changes. A handler
runs when its signal is emitted, and most handlers call a method or write more than once, which
is tier 2 by design.

`bindings/expressions.feature` was written with the VM and is mostly tier 1 by construction, so
the last row is the fairer picture of the ledger. The ledger also over-represents what tier 2 is
for: its scenarios exercise `Qt.locale`, colour functions, text metrics and array methods one by
one.

Of the 1046 corpus bindings that are tier 2, 875 read or call a QML singleton
(`Constants.width`, `Theme.responsiveSize(8)`). The engine does not instantiate singletons yet, so
neither tier evaluates them today. Leaving them out, 8548 of 8719 bindings (98.0 %) are tier 1.

## Why expressions are tier 2

### qtdoc

| All | Bindings | Handlers | Functions | Construct |
|---:|---:|---:|---:|---|
| 638 | 628 | 9 | 1 | member of a singleton |
| 428 | 56 | 279 | 93 | method call (`.open()`, `.push()`, `.start()`, a signal emitted) |
| 226 | 213 | 6 | 7 | method call of a singleton |
| 170 | 16 | 135 | 19 | call of a function that is itself tier 2 |
| 106 | 0 | 56 | 50 | more than one effect (two writes, or a write and a call) |
| 77 | 38 | 26 | 13 | global function call (`console.log` 15, `Qt.resolvedUrl` 11, `Qt.darker` 10, `Qt.font` 8) |
| 54 | 2 | 25 | 27 | statement (`for` 26, `var` in a block 14, `switch` 11) |
| 45 | 34 | 11 | 0 | singleton as a value |
| 29 | 2 | 26 | 1 | assignment (to an element 17, inside an expression 12) |
| 23 | 3 | 14 | 6 | import namespace |
| 20 | 10 | 3 | 7 | `new` (`Date` 15) |
| 14 | 12 | 2 | 0 | attached object of a value (`item.Layout.fillWidth`) |
| 10 | 10 | 0 | 0 | member (`Qt.platform` 6) |
| 10 | 0 | 10 | 0 | write to a member of a value (`font.bold = true` through a held value) |
| 46 | 22 | 3 | 21 | the rest, under ten each: a script that does not parse, assignment to a name only the run-time context can place, object literal, function value, regular expression, a pattern or default value as a parameter, call of a value, type as a value, `instanceof`, spread |

### features

| All | Bindings | Handlers | Functions | Construct |
|---:|---:|---:|---:|---|
| 177 | 91 | 7 | 79 | method call (`.join()` 19, `.indexOf()` 10, `.elidedText()` 9, `.advanceWidth()` 8) |
| 84 | 74 | 9 | 1 | global function call (`Qt.locale` 26, `Qt.font` 10, `Qt.lighter` 10, `console.warn` 9, `Qt.darker` 7, `Qt.tint` 7) |
| 50 | 0 | 40 | 10 | write to a member of a value |
| 35 | 15 | 0 | 20 | attached object of a value |
| 34 | 5 | 4 | 25 | more than one effect |
| 11 | 0 | 0 | 11 | statement (`for` 8) |
| 27 | 10 | 5 | 12 | the rest, six or fewer each |

### What would move the number

- **Singletons** are 909 of the 1896 tier-2 expressions of qtdoc. Once the engine instantiates
  them, a singleton's property is a tracked read like any other and its pure functions can be
  inlined as a file's own are; the generator already tells the three cases apart.
- **Colour functions** (`Qt.lighter`, `darker`, `tint`, `alpha`, `colorEqual`) and **`Qt.font`**
  live in the engine's JavaScript prelude. Tier 1 can take them only by sharing one
  implementation with tier 2 or by proving a Rust port bit-exact.
- **String and array methods** (`indexOf`, `startsWith`, `join`, `slice`) are pure, but each is
  only worth adding with a sweep against tier 2 like the ones `toFixed` and `arg` have.
- **Handlers** are mostly effects. A handler with one write is tier 1; one with two, or with a
  method call, is not, because the VM cannot take back an effect when it has to give up.

## Reproducing

```sh
cargo build -p ljos_cli
target/debug/ljos check --stats --summary --tree --quiet corpus/qtdoc
```

`--tree` reads a directory laid out as a set of examples and compiles every directory two below
it as a project of its own. Without `--summary` the report has a row per file. `--top N` bounds
the list of reasons. The command exits non-zero because 1401 qtdoc diagnostics are errors (types
Ljós does not have yet); the report is printed first, and a file with an error still counts with
what did compile.

For `features/`, the QML doc strings of each scenario (each `Examples` row filled in, each file
of a hot-reload step in its new form) were written out as one project per scenario, identical
projects once, and `ljos check --stats --tree` run over the result.

## What tier 1 is worth

Release build, AMD Ryzen AI Max+ 395, one thread. From
`cargo test --release -p ljos_engine --test tier1 -- --ignored --nocapture`.

| Per evaluation (change a property, read the bindings) | Tier 1 | Tier 2 | |
|---|---:|---:|---:|
| Five mixed bindings (arithmetic, a conditional, `Math.max`) | 279 ns, 3.6 M/s | 575 ns, 1.7 M/s | 2.1× |
| One operator | 209 ns | 417 ns | 2.0× |
| Forty operators in one expression | 606 ns | 659 ns | 1.1× |
| Three string bindings (`+`, a template with `toFixed`, `arg`) | 547 ns | 1038 ns | 1.9× |
| A bare path (tier 0 in both) | 167 ns | 167 ns | 1.0× |

An evaluation costs about 170 ns before any expression runs (the dependency graph, the write, the
notification). On top of that the VM adds about 40 ns for a small expression and about 10 ns per
operator; calling into QuickJS adds about 250 ns and about 6 ns per operator. So tier 1 wins by
avoiding the call, not by computing faster, and the two would meet at around fifty operators.

| Creating a file and reading every binding once | Tier 1 | Tier 2 | |
|---|---:|---:|---:|
| 200 rectangles, 1000 distinct bindings | 0.71 ms | 5.84 ms | 8.2× |
| 2000 rectangles, 10 000 distinct bindings | 9.73 ms | 172 ms | 17.7× |
| The shell's QML demo, offscreen, whole process (118 tier-1 runs) | 193.1 ± 3.8 ms | 197.2 ± 2.9 ms | 1.02× |

A file whose expressions are all tier 1 never has its JavaScript module parsed, which is where the
first two rows come from. The demo is too small for it to show: its start-up is the renderer's.

**Opcode specialisation was measured and left out.** A fast path for two numbers in the binary
operator took 10 % off the forty-operator expression and nothing measurable off the others;
giving a chain of operators two registers instead of one each took another 25 % off it. Opcodes
typed at compile time would save a tag check of a few nanoseconds in evaluations that cost 200 ns
or more, so they are not worth a second set of opcodes until bindings get much cheaper elsewhere.

## Switches

| | |
|---|---|
| `LJOS_TIER1=0`, `Engine::set_tier1(false)` | Every expression runs in tier 2. For bisection. |
| `LJOS_TIER1_CHECK=1` / `0`, `Engine::set_tier1_check` | The cross-check: every tier-1 evaluation is also run in tier 2 and compared; a difference is logged as a warning with the expression and both values, and tier 2's value is used. On by default in debug builds, off in release. |
| `LJOS_TIER1_LOG=<file>` | Appends every difference and, per engine, the counts of runs, bails, checked runs and differences. |
| `Engine::tier1_stats()` | The same counts. |
