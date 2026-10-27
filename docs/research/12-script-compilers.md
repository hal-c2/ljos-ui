# Script compilers for Ljós QML: scriptc, tsrs, OCaml, QuickJS

Reference note, 2026-10-27. It gathers references and considerations; it does not decide.

The earlier note, `docs/research/11-scripting-language.md` in the Ljós repository, compared
two languages you might design: a strict TypeScript subset (A) and a ReScript-like language
(B), both with a checker of Ljós's own in Rust. This note asks instead what already-built
compilers you could take off the shelf, and at what cost. It does not repeat the earlier
note's language-design trade-offs; where the two meet, it says so and moves on.

1. **scriptc** — Vercel Labs' TypeScript-to-native compiler (LLVM back end).
2. **tsrs** — the Oxc project's Rust port of the TypeScript 7 compiler and language server.
3. **ReasonML / OCaml to native** — `ocamlopt`, with Melange and js_of_ocaml for the web.
   reativa is one signals library on it.
4. **QuickJS** as Ljós uses it today (`crates/ljos_js`) — the baseline and compatibility mode.

## How to read the numbers

Every number is either **measured** (by the writer, 2026-10-27) or **cited** (named source,
with link). Anything else is marked *(unverified)*. Measured numbers come from one machine,
`ai-beast` (AMD Ryzen AI Max+ 395, 32 threads, 62 GiB, Linux; Node 24.21.0, clang 22.1.8,
Zig 0.16.0, Rust 1.98.1, OCaml 5.4.0), on three files:

- **Small**: 36 lines of TypeScript shaped like QML script — a mode `switch` over a string
  union, `filter`/`map`/`reduce` over records, template formatting (OCaml twin: 14 lines).
- **Large**: a generated 4,176-line TypeScript file, 320 such functions (OCaml twin:
  1,924 lines).
- **Kernel**: pure number work (`fib(35)`, sieve to two million), there only because
  published benchmarks use that shape. Ljós scripts are mostly short handlers, string
  formatting and list/model updates; treat kernel numbers as an upper bound, not the gain.

Wall times are whole-process launches (appendix: commands). Benchmark files and cloned
repositories sit beside this note, so you can rerun every measured number. Warning: two
candidates are weeks to months old; their numbers are real but say less about next year
than OCaml's or QuickJS's. Maturity evidence comes first so you can weigh the rest.

## What each candidate is

### 1. scriptc — TypeScript and JavaScript to native code and WebAssembly

scriptc compiles a TypeScript or JavaScript program, with the npm packages it imports, into
a native executable, a WASI Preview 1 module, or — in "library mode" — a static archive or
Wasm reactor a host links and calls ([README](https://github.com/vercel-labs/scriptc);
[docs](https://scriptc.dev/docs)). No JavaScript engine is in the output. Pipeline: the
TypeScript compiler checks your program (`es2025` library, your `tsconfig.json`); scriptc
lowers it to a typed IR (generics specialised, unions tagged, closures explicit); an LLVM 22
helper emits objects; a precompiled C "runtime pack" (reference counting plus cycle
collector, event loop, native Node APIs) links in. Values known only at run time (`any`,
`unknown`, `JSON.parse`) travel as "checked values", validated on conversion to a static
type — a lying cast throws where Node would carry on.

Which TypeScript? Standard TypeScript as checked by the bundled compiler (repo pins
`typescript` 5.9.3; the shipped tool also carries a self-compiled checker, tsc-ts), plus
plain JavaScript — then narrowed by representability: what it cannot lower is refused at
build time with a coded diagnostic (`SC…`) and usually a rewrite hint; `scriptc coverage`
reports what compiles before you commit. Gaps that matter for QML: no run-time code
generation (`eval`/`new Function` throw `EvalError`); dynamic `import()` only of known
modules; generics only where uses specialise; some reflection refused; legacy decorators
still open ([#2209](https://github.com/vercel-labs/scriptc/issues/2209)); library mode
excludes promises, `async` and timers. Its
[limitations pages](https://scriptc.dev/docs/limitations) are unusually frank.

Maturity (GitHub API, 2026-10-27): created 2026-07-22, three months ago; ~5,600 commits,
6,400 stars, 77 open issues; near-daily releases (v0.4.1–v0.4.5, 21–25 Oct); two authors
write nearly all of it (Chris Tate ~4,600 contributions, Malte Ubl ~1,000). Badged a Vercel
Labs *experiment*, Apache-2.0, "experimental" by its own README. Test discipline is strong:
corpus programs must match Node byte-for-byte, native and interpreted, and a CI "zero-cost
gate" requires byte-identical output for unaffected programs. Written in TypeScript and
self-compiled to the native binary it ships as.

### 2. tsrs — the TypeScript 7 compiler, ported to Rust

tsrs is a drop-in replacement for `tsc` and the TypeScript language server: parse, bind,
check, and emit JavaScript, `.d.ts`, source maps and `.tsbuildinfo`, aiming at 100 %
conformance with the Go port (tsgo) at a pinned commit, currently TypeScript 7.1.0-beta
([README](https://github.com/oxc-project/tsrs);
[compatibility](https://github.com/oxc-project/tsrs/blob/main/docs/COMPATIBILITY.md): all
13,700 error and 12,998 type/symbol baselines match). It ships as per-platform npm
binaries, an LSP server, a Node API server (`--api`) speaking TypeScript's API protocol,
and `@tsrs/wasm` (single-threaded `wasm32-wasip1`). An opt-in `--fast` preset trades exact
diagnostic parity in known history-dependent cases for more checkers and lower memory.

Which TypeScript? All of it — that is the point; default mode preserves even known quirks.
For the earlier note's direction A ("valid TypeScript, checked by a checker of our own in
Rust"), tsrs is the proof the "of our own" part may now be bought, subject to section 1's
packaging caveats. Maturity (GitHub API): created 2026-10-01, under four weeks ago;
~2,000 commits, 2,400 stars; releases every few days (0.14.0 on 11 Oct to 0.20.0 on
26 Oct); mainly Max Schwenk (~1,300 contributions), now in the Oxc project with Cloudflare
and VoidZero, by their account with the TypeScript team's cooperation
([announcement](https://blog.cloudflare.com/typescript-rust-port/)). Apache-2.0. Know
this: pull requests are closed, the Rust crates are `publish = false` (no crates.io
release; verified), and its pin is Rust 1.99.1 against Ljós's 1.98.1.

### 3. ReasonML / OCaml — a mature native compiler, two web back ends

`ocamlopt` 5.4.0 (measured; opam switch installed into scratch in 71 s, ~300 MB with dune,
js_of_ocaml and Melange) compiles each `.ml` against compiled interfaces (`.cmi`) through
the system assembler and C linker. ReasonML is alternate syntax for the same language;
ReScript forked from it and now emits only JavaScript (direction B's reference in the
earlier note). Reason syntax with native output goes through the `reason` front end
*(unverified: not installed here)*. For the web: **js_of_ocaml** compiles bytecode to one
JS file emulating the bytecode representation (32-bit ints, tagged blocks); **Melange**
compiles per-module to readable, tree-shakeable JS (the back end ReScript forked from);
**wasm_of_ocaml** exists in opam but was not installed — claims about it are *(unverified)*.

**reativa** (https://github.com/brnrdog/reativa) is a fine-grained signals UI library for
OCaml/ReasonML web programs (`Signal`, `Computed`, `Effect`, a DOM `View`, no virtual DOM);
its graph is plain OCaml that also runs natively. One person, created 2026-06-26, 54
commits, MIT, v1.2.0. For Ljós it is a worked example of an ML language around a graph like
`ljos_reactive`'s, not a component to embed. OCaml itself has shipped since 1996 (Inria and
community): extremely stable, and nobody will bend it towards QML.

### 4. QuickJS — what Ljós runs today

Tier 2 is QuickJS via a vendored rquickjs 0.14.0 fork (quickjs-ng from C, one
number-formatting fix; `vendor/QUICKJS.md`). `ljos_js` runs one runtime per engine: 96 MiB
memory limit, 8 MiB GC threshold, 512 KiB stack, 4 ms/100 ms dispatch slices
(`config.rs`). Modules may be precompiled to bytecode, private to the linked version and
unvalidated beyond its tag — relevant to untrusted code. In-situ numbers
(`docs/tier1-coverage.md`): entering QuickJS adds ~250 ns per evaluation, ~6 ns per
operator; a 200 KB module compiles in ~8 ms; 200 rectangles/1,000 bindings cost 5.84 ms in
tier 2 vs 0.71 ms in tier 1 (whose files are never JS-parsed). This note's QuickJS runs
used upstream Bellard QuickJS 2025-04-26 (`qjs`, 996 KB stripped) as a proxy for the fork.

## 1. Compile time, and shipping the compiler

Ljós exists for malleable software: people tailor the apps they use themselves, often with
a coding agent, on their own device (`docs/ARCHITECTURE.md` principle 9). A shipped app
*may include the compiler*; "no parser in shipped apps" is now the application's choice
(crate rules, as revised) and is not a constraint on the candidates. Ask instead:

- **What does shipping the whole tool chain inside an app cost** — size, memory, dependency
  crates and external tools, and does it build for every target?
- **How fast does an edit become visible**, save to new behaviour, native and web? Ljós's
  hot reload shows edits 17–31 ms after the save (`docs/hot-reload-drive.md`); a few
  hundred milliseconds feels like a rebuild, not an edit.
- **Does it work offline, on the device itself**, with no server and no outside tool chain?

The brief's two cases — a user editing their site's QML shell on the web, and a native app
validating a new shell file at run time — are that same question on two platforms.

### Measured compile times

Whole-process, on `ai-beast`. Cold = fresh cache; edit = one string literal changed.

| Command | Small cold | Small unchanged | Small edit | Large cold | Large unchanged | Large edit |
|---|---|---|---|---|---|---|
| scriptc `build` (release exe) | 348 ms | 50 ms | 190 ms | 766 ms | 52 ms | 700 ms |
| scriptc `build --optimization=dev` | — | — | — | 527 ms | — | — |
| scriptc to WASI; `--lib`; `coverage` | 3,110 ms (first Wasm); 127 ms; 184 ms | — | — | — | — | — |
| tsrs `--noEmit` | 50–66 ms | same | same | 62–66 ms | same | same |
| tsrs without `skipLibCheck`; @tsrs/wasm in Node | 89–93 ms; 122 ms in-process | — | — | — | — | — |
| `ocamlopt -O2`; `-c` only | 74–91 ms | same (no cache) | same | 422–442 ms; 353 ms | same | same |
| js_of_ocaml; Melange (dune, kernel+hello) | 534 ms; 161 ms | 26 ms no-op (jsoo) | — | — | — | — |
| TypeScript 7.0.2 `tsc --noEmit`, for scale | 58–62 ms | — | — | 79–84 ms | — | — |

Three shapes stand out. **Checking is cheap**: tsrs checks small in ~55 ms and large in
~63 ms, nearly all process start and library load by its own `--diagnostics`; at QML scale
you would not need its incremental modes (`.tsbuildinfo`, `--build`, watch), though they
exist. Cited at the other end of the scale: 17 real projects (Depot CI, 16 vCPUs), VS Code
checked in 0.82 s vs tsgo's 11.10 s at defaults, 4–12× at equal checker counts, ~⅓ the
peak memory ([RESULTS.md](https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md),
pre-0.20 snapshot; Cloudflare: "up to 8× faster, ~70 % less memory"). **scriptc is fast
but whole-program**: unchanged rebuilds are 50 ms cache hits, but any edit re-runs the
front end over everything ("a change to any input runs the whole stage again"); only LLVM
partitions (above 1 MiB of LLVM text; large made 1.1 MiB) are reused. Its changelog shows
real effort here (0.4.4: 23-module unchanged rebuild 93 ms, was 1.26 s). **OCaml's
incremental unit is the file**: no cache needed at 75 ms/file, and separate compilation
against `.cmi`s is the model your hot reload wants — if the tool chain is present at all.

### Shipping the tool chain inside the app

Sizes are measured Linux x86-64 installs.

- **scriptc: a compiler distribution, not a library.** No Rust crate, no linkable API (a
  `compileLibrary()` API exists for Node hosts). You would bundle a 78 MB stripped CLI,
  58 MB LLVM helper, ~52 MB of checker payloads and a 115 MB per-target runtime pack —
  328 MB for one host/target pair — plus the linker it shells out to (clang on glibc
  Linux/macOS, Zig elsewhere and for WASI). It *builds for* every Ljós target (CLI/runtime
  packs: macOS, Linux, Windows, iOS, Android, WASI) but *runs on* desktop hosts only; no
  browser build was found in its tree *(unverified beyond that search)*. Offline on a
  desktop: yes, as a ~0.4 GB subprocess. On a phone or in a browser: no. Edit-to-visible
  would be 190 ms + relink + reload of a whole-program artefact — a restart-sized event.
  Compile-time memory is modest (60–105 MiB; 288 MiB on the first WASI build with Zig).
- **tsrs: the only linkable candidate — which you cannot quite link today.** The size is
  right: 28 MiB binary + ~4 MB of `lib.*.d.ts` (embeddable in its Cargo/Wasm builds),
  50–110 MiB peak checking, no external tools, fully offline. Edit-to-visible: a warm
  LSP/`--api` process or a 55 ms cold check, then your existing reload; in a browser,
  `@tsrs/wasm` (9.5 MB, 2.9 MB gz, 122 ms, 115 MB peak linear memory) keeps the loop
  on-device. Blockers are packaging: unpublished crates, Go-mirroring internal APIs, Rust
  1.99.1 pin. Ship-today forms: npm binary as subprocess (seven native platforms + Wasm),
  or a git dependency. If linked, expect 37 first-party crates plus a modest third-party
  set (rayon, mimalloc, serde, icu…) — likely comparable to the QML parser's ~106
  dependency crates *(unverified: no `cargo tree` count taken)*.
- **OCaml: a guest tool chain.** `compiler-libs` serves OCaml hosts; a Rust app would spawn
  `ocamlc`/`ocamlopt` + stdlib (142 MB for `lib/ocaml` alone; ~300 MB switch as measured),
  and `ocamlopt` still needs a system assembler/linker no user's device has. The bytecode
  route (`ocamlc` + bundled `ocamlrun`) avoids the linker and is the only offline-on-device
  form; its size was not measured *(unverified)*, and it runs bytecode (section 5 turns
  that into a virtue). No supported browser-hosted compiler *(unverified beyond the
  measured tooling)*. Cross-compilation (`ocaml-cross` for Android/iOS) is a build-time
  arrangement, not cargo for the shipped app *(unverified: not exercised)*.
- **QuickJS: already shipped, the yardstick.** ~1 MB of C, no dependencies, builds on every
  Ljós target; edit-to-visible is parse-and-run inside the reload you have (~8 ms/200 KB,
  less from bytecode). Offline everywhere, today. Any other candidate is *in addition to*
  this cost while compatibility mode exists.

## 2. Speed after compilation

Measured whole-process wall time including start-up, on `ai-beast`. Node rows calibrate
against a JIT you know. tsrs has no row: it emits JavaScript, so its run speed is whatever
runs that JavaScript; what it changes is which errors you find first.

| Program | Run | Wall | Peak RSS | Output |
|---|---|---|---|---|
| kernel | scriptc native; OCaml `-O2` | 30 ms; 34 ms | 12; 21 MiB | 146 KB; 2.6 MB (1.0 stripped) |
| kernel | Node 24; Melange JS; jsoo JS (Node) | 126–179; 125–132; 115–159 ms | ~90; ~74; ~78 MiB | —; 1.2 KB module; 2.1 MB JS (496 KB gz) |
| kernel | scriptc WASI in Node; QuickJS `qjs` | 120–151 ms; 1.36–1.39 s | ~66; ~34 MiB | 111 KB wasm; 996 KB interp. |
| small | scriptc native; OCaml; Node; QuickJS; scriptc WASI | 7–8; 30–32; 91–128; 79–84; 61–72 ms | 12; 15; 94; 12; 66 MiB | 167 KB; 2.6 MB; —; —; 129 KB (58 gz) |
| hello | scriptc native; QuickJS; OCaml/jsoo/Melange (size) | ≤1 ms; 1 ms; — | 12 MiB | 64 KB; —; 414 KB stripped / 306 KB gz / 87 B module |

Three cautions. **The kernel flatters AOT compilers as Ljós work will not**: the 45×
scriptc-vs-QuickJS gap is 15M recursive calls; a handler runs microseconds then calls into
the engine, where the boundary dominates (~250 ns into QuickJS, cited in-situ; scriptc's
flat FFI call and OCaml's C FFI against Ljós's `Value` are *unmeasured* — that, not `fib`,
decides handler speed). On the handler-shaped small program QuickJS (80 ms) beat Node
(95 ms) outright: start-up and allocation beat peak JIT throughput at this scale.
**Start-up and size are scriptc's real results**: <1 ms hello at 12 MiB from a 64 KB
binary, an order of magnitude under OCaml's 414 KB stripped floor — the same source as
tier 1's win (nothing parsed, nothing warmed up), and the numbers that matter against your
"beat qml6 on speed, lag and memory" requirement. **The web split matters**: Melange emits
tree-shakeable modules (kernel: 1.2 KB + 2.6 KB gz of helpers used, V8 speed); js_of_ocaml
emits the bytecode machine (306 KB gz before your code) with 32-bit integers — my first
sieve died on `i*i` overflow where native 63-bit ints ran. TypeScript has no such seam
across targets. Published suites, cited for shape: scriptc's `benchmarks/runtime` (~45
application workloads vs Node, byte-identical output required; no results table in-repo,
so no headline ratio to cite); tsrs's are all checking speed (section 1); OCaml's are the
Benchmarks Game's audited kernels, with the kernel caveat (tables not fetched here).

## 3. Types

The QML mistakes that matter: wrong property type (`root.width = "wide"`), null where a
value must exist, a missing mode-switch case, a member that does not exist (`root.heigt`).
One measured file, `errors.ts`, contains all four (appendix):

- **tsrs** caught all four with TypeScript's codes (TS2322 ×2, TS2339, TS2366 — the last
  is exhaustiveness reported as a missing return, not "case unhandled", unless you add the
  `never` trick). `--strict` gives null checking; `noUncheckedIndexedAccess` and
  `exactOptionalPropertyTypes` give the earlier note's H1 rules. Soundness you never get:
  it is a stated TypeScript non-goal, `any`/`as` remain, tsrs preserves semantics exactly
  (in `--fast`, known cases where diagnostics move with scheduling). Closing holes is
  Ljós-side lint discipline, whoever runs the check.
- **scriptc** caught the same four (SC0001: "scriptc only analyzes programs that
  typecheck") — its front end *is* the TS checker at your `tsconfig` strictness — then adds
  what the others lack: values entering from `any`/`unknown`/`JSON.parse` are checked at
  run time (`expected number at $.port, got string`, its docs' example). That is exactly
  the boundary behaviour you want for JSON and `var` properties, paid in checked-value
  conversions and representability refusals. It adds no exhaustiveness/null discipline of
  its own.
- **OCaml** catches all four by construction, plainly ("There is no field `wdth`… Did you
  mean `width`?", measured). Non-exhaustive `match` is warning 8, promoted to error in any
  serious build; null does not exist (`option` does); inference needs no annotations (the
  small file has none). Sound, at direction B's known costs: HM errors far from their
  cause, deliberate encoding of QML objects, and the 63-vs-32-bit integer seam between
  native and web — a portability unsoundness TypeScript does not have.
- **QuickJS** catches none (`root.heigt` is `undefined`); tier 0 already reports provable
  member errors, but in-handler type errors are run-time events today. Hence: compat mode.

### Generating declarations from Ljós's type data

The source data is ready (verified): `qt_types/src/ljos.txt` (7,727 lines, generated from a
running engine by a staleness-checked test) lists modules, types, prototypes, properties
with type/readonly/required flags, signals, methods, enums; `qml_front::TypeOracle` is the
programmatic form (`property_type`, `signal`, `has_method`, `attached`, `members_checked`,
…), also exposed by `ljos_mcp_author` as `type_lookup`. `qmltypes` already writes one text
format from such data; no `.d.ts` writer exists in the tree (searched).

For **tsrs/scriptc** generation is mechanical: one ambient `.d.ts` per module, interfaces
extending prototypes, `readonly` where flagged, signals as callable properties, enums as
const objects, per-component ambient scope (ids, root, context properties). Known awkward
parts: `var` → `unknown` unless QML can name script types; attached properties need a
generated merged view; `list<T>` is not quite an array; for scriptc, engine-implemented
members must additionally be *representable* or cross as host callbacks/checked values.
For **OCaml** you would generate `.mli`s with abstract types and accessor functions
(`Item.width : item -> float`), variants for enums, `option` for nullables — same
generation effort, costlier use: `root.width + label.width` becomes two qualified calls,
and change notification has no expression in the type at all (next section).

## 4. Everything else that matters

**Untrusted code.** QuickJS runs today with memory/stack limits and uncatchable time
slices; hostile script wastes its slice and 96 MiB, no more (but never accept foreign
bytecode: unvalidated). scriptc native output and `ocamlopt` output are the user's machine
code in your process — unsandboxed unless you interpose interpreted Wasm (section 5) or a
process. tsrs *checking* untrusted code is parser-risk (a Rust binary, with open
memory-safety investigations
[#379](https://github.com/oxc-project/tsrs/issues/379)/
[#646](https://github.com/oxc-project/tsrs/issues/646); pathological types can stall any TS
checker) — run it limited, ideally the Wasm build in-page where the browser is the sandbox.

**Hot reload.** tsrs fits yours (warm LSP project, 55 ms cold check). scriptc rebuilds a
whole closed program whose globals initialise once (re-init resets them) — no
swap-one-component story. OCaml recompiles per file but relinks a program or `Dynlink`
plugin whose types must match the host exactly. QuickJS evaluates the new module; compat
mode is the reload-friendly mode.

**Debugging, maps, errors.** tsrs: byte-identical source/declaration maps; TypeScript's
messages verbatim. scriptc: coded diagnostics with rewrite hints (measured SC1013 named its
fix), `coverage` explanations, DWARF source breakpoints in `--optimization=dev` only, with
values in native dress and no expression evaluation. OCaml: short errors with hints,
`ocamldebug` (bytecode, time-travel), web source maps unmeasured *(unverified)*. QuickJS:
JS stack traces, positions unmapped in `ljos_js`.

**Async, GC, and crossing into the engine.** scriptc executables get coroutines/fibres and
a native event loop — but **library mode refuses promises, `async` and timers**, so every
async handler would need another tier. OCaml 5 offers effects/fibres natively; a
promise-shaped API is your veneer, foreign-typed on the web. GC: scriptc reference-counts
with a cycle collector and proven-lifetime arenas (some opaque cycles stick, by its
account); OCaml's generational GC is mature but unmeasured against 4 ms slices; QuickJS's
collector already runs on your idle lane. The boundary is the deep difference. QuickJS uses
per-type prototypes with accessors into the graph, reads tracked because tier 0 resolved
names. scriptc's library boundary passes flat scalars and byte strings only, with
synchronous callbacks that must return before re-entry and results in a reset-on-next-call
arena: `parent.width` costs one FFI call per read, and dependency tracking is yours to
rebuild per call. OCaml's C FFI speaks tagged ints and boxed floats under GC rooting; every
property read crosses a translation layer, and reativa shows the signals *pattern*, not a
bridge to a foreign graph. Remember this: **only tsrs improves the existing boundary;
scriptc and OCaml each make you design and pay for a new one, per property, at run time.**

## 5. Platforms that limit run-time code

Malleability assumes your user's new code *runs*. Some platforms limit what may run, and
unequally across candidates. The architecture already states the intended answer: "Where a
platform forbids code generated at run time (iOS has no JIT), the changed code still runs,
as IR the engine interprets" (principle 9). Test each candidate against that sentence.

### iOS: no JIT, no writable executable memory, guideline 2.5.2

Two limits, not to be confused. *Technical*: no JIT and no writable-executable memory
(outside WebKit's entitlement) — bytes made at run time may not become machine code.
Interpreters are unaffected. *Policy*: App Store Review Guideline 2.5.2 (from
[Apple's guidelines](https://developer.apple.com/app-store/review/guidelines/), fetched
2026-10-27; guidelines change — check the current text):

> Apps should be self-contained in their bundles, and may not read or write data outside
> the designated container area, nor may they download, install, or execute code which
> introduces or changes features or functionality of the app, including other apps.
> Educational apps designed to teach, develop, or allow students to test executable code
> may, in limited circumstances, download code provided that such code is not used for
> other purposes. Such apps must make the source code provided by the app completely
> viewable and editable by the user.

What it forbids is code that "introduces or changes features or functionality" — an app
becoming a different app after review. Code a user writes on their own device to tailor
their own copy is the grey zone every scripting app lives in; reviewer treatment is
submission-by-submission, not a fact this note can give you *(unverified: no Ljós-based app
has been through review)*. The educational exception's condition — source "completely
viewable and editable by the user" — is one a malleable app meets by design, but it is
written for teaching apps; do not lean on it for a general shell without advice. Safe
conclusion: the more run-time code looks like *data for a fixed engine* (QML file, compiled
IR, script in a shipped interpreter), the closer you stay to both limits. Workarounds:

- **QuickJS — the interpreter.** Ship the interpreter; user script is its data; no JIT to
  remove. The long-accepted shape for scriptable iOS apps, 2.5.2's features clause aside.
  Works today.
- **tsrs — compile on-device to something interpretable.** Checking and emitting JS text
  creates no machine code; the edit then runs in QuickJS/tier 1 as now. The 2.5.2 question
  attaches to what executes it (QuickJS's row), not to the checker.
- **scriptc — AOT on another machine, or interpreted Wasm.** Its iOS form is library-mode
  archives linked by Xcode at build time. An on-device edit cannot be scriptc-compiled
  there: no iOS-hosted scriptc, no linker, and loading new native code breaches both
  limits. Compiling elsewhere to native and loading it breaches them still, and breaks
  "offline, on the device". Compiling elsewhere to a Wasm reactor run in a Wasm
  *interpreter* you ship is technically clean (a JIT-ing Wasm engine is not) at
  interpreter speed plus that engine's weight; wasm3-class interpreters were not evaluated
  here *(unverified)*. In practice: scriptc script on iOS is build-time script.
- **OCaml — bytecode interpreter, or AOT elsewhere.** A bundled `ocamlc` producing portable
  bytecode for a shipped `ocamlrun` is the QuickJS shape; `ocamlopt`/`Dynlink` native code
  is build-time only. The bar is weight (compiler + stdlib + run time + FFI bridge inside
  an iOS app), not legality. `wasm_of_ocaml` + interpreted-Wasm host is the other route
  *(unverified)*.
- **Compiled Ljós IR — everywhere, by construction.** `CompiledFile` is serialised data;
  no byte of it executes as machine code, whoever compiled it. Same position as a QML file
  edited today. Section 6 prices reaching it from each candidate.

### Android, the web, consoles and kiosks

**Android** permits shipped interpreters/JITs technically, but Play policy restricts
downloading executable code outside Play's updates; interpreted script/IR is the normal
accepted pattern, fetching native libraries to load is not *(unverified: current Play text
not fetched — check before relying on it)*. The iOS shape is the safe Android shape;
on-device AOT-then-`dlopen` is possible and policy-risky. **The web** never lets you mint
machine code; compiled Wasm is first-class, gated by the page's CSP — strict `script-src`
without `'unsafe-eval'` blocks `eval`/`new Function`, and Wasm compilation needs
`'wasm-unsafe-eval'` under CSP3 *(unverified in detail against current browsers; check your
shell's CSP)*. So scriptc-Wasm and `@tsrs/wasm` need a Wasm-permitting CSP; tsrs/Melange JS
must arrive as script resources, not `eval`; and Ljós IR needs Wasm only once, for the
engine — user edits are then data under any CSP that loaded the app. **Consoles and
locked-down kiosks** resemble iOS at maximum policy (no JIT, no unsigned native code,
no post-certification behaviour change; *(unverified: requirements are under NDA)*).

Plainly: restricted platforms distinguish *interpreted data* from *new machine code*, not
languages. Every candidate is usable everywhere exactly as far as its output can be the
former.

## 6. Can it compile to Ljós's own IR instead of machine code?

`ljos_ir` is two things (verified): `CompiledFile`, the serialised QML file whose
`JsModule::source` is JavaScript text (one function per expression); and tier-1 `Tier1`
register programs inside it — flat `Op` lists, JS-valued semantics, at most one write,
whole-or-nothing with bail-out to the JS form. "Compile to `ljos_ir`" has a cheap reading
(emit that JS text; let `ljos_compiler` resolve names and generate tier 1 as today) and a
dear one (emit tier-1 programs directly: reimplement QML scope resolution, `Resolved`/`Arg`
conventions and bail-out discipline, against a subset covering 97.8 % of bindings but
21.8 % of handlers — the JS twin must exist anyway).

- **tsrs: cheap reading, today.** Emitting checked JS with maps is its job; feed it where
  `js.rs`'s erasure feeds, or use tsrs only to check and keep the oxc AST. Direct tier-1
  emission means a generator over unpublished Go-mirroring internals, buying little while
  the JS twin is required. The real prize is checker *facts* (operand types, totality,
  closed calls) for the existing tier-1 generator; whether tsrs exposes them usably
  without a fork is an open question.
- **scriptc: no.** Its typed IR exits only via LLVM to machine code/Wasm (`--emit=ir` JSON
  is a debugging artefact); no bytecode, no interpreter, no back-end hook. Retargeting
  means forking the compiler and writing the back end. Its Wasm reactor, interpreted, is
  the nearest IR-like output — but that IR is WebAssembly, with its own engine.
- **OCaml: not to `ljos_ir` — but it owns an IR.** Portable bytecode, interpreted by
  `ocamlrun` anywhere, also feeds js_of_ocaml/wasm_of_ocaml. A Lambda-to-tier-1 back end
  is a real compiler project that hits tier 1's semantic gaps at once (effectful closures,
  exceptions across writes, multi-write recursion). OCaml's sane "IR the engine
  interprets" keeps a second run time in your app, blind to dependency tracking.
- **QuickJS: defines the cheap reading.** `JsModule` text is its input; its bytecode is a
  second serialised form beside it. Any candidate emitting correct JS from checked source
  (tsrs, Melange) already "compiles to `ljos_ir`" in the sense restricted platforms care
  about. Direct tier-1 emission is unbuilt under every candidate, status quo included.

## What each would mean for Ljós's tiers

- **tsrs** is direction A's checker bought instead of built (~10–15k lines in the earlier
  note's estimate): generated `.d.ts`, a driver (subprocess/`--api`/git dep/`@tsrs/wasm`),
  Ljós-side subset rules as diagnostics. Tiers untouched. Risks are governance: four weeks
  old, PRs closed, crates unpublished, Rust pin ahead of yours, a port chasing TypeScript
  releases forever ([#618](https://github.com/oxc-project/tsrs/issues/618)).
- **scriptc** returns to the architecture's rejected "typed-only AOT", one objection
  removed (annotations now exist) and one worsened (reload). Realistically: closed,
  shipped, logic-heavy script compiled at app-build time behind a flat boundary you design;
  bindings stay tier 0/1, run-time and untrusted QML stays QuickJS; on restricted
  platforms it cannot serve on-device edits at all. First experiment: `scriptc coverage`
  over real transpiled QML script. Its tiny fast binaries also suit standalone tools.
- **OCaml/Reason** is direction B with parser, checker and optimiser pre-written — in
  someone else's build system (dune/opam beside Cargo). Buys sound types, the fastest
  per-file compilation here, Melange's small web output; costs a new language for authors
  and coding models, a per-property FFI bridge, the int-width seam, a tool chain you
  cannot ship in-process. Also the only candidate whose next decade is not a bet.
- **QuickJS** remains the floor: the only sandboxed, reloadable, run-time-loadable option.
  Every path keeps it.

## Summary table

One row per candidate, one column per consideration; measured unless the cell says cited.

| Candidate | 1. Compile time; shipped tool chain; offline | 2. Speed after compilation | 3. Types; declarations | 4. Sandbox, reload, debug, async, GC, boundary | 5. Restricted platforms | 6. To `ljos_ir`? |
|---|---|---|---|---|---|---|
| **scriptc** 0.4.5, 3 mo, Apache-2.0 | 348/190 ms small, 766/700 large. ~328 MB desktop chain + linker; builds *for* all targets, runs on desktops. Offline desktop-only | Kernel 30 ms, small 7 ms, hello <1 ms/12 MiB; 64–167 KB bins; WASI 111–129 KB. Best start-up | TS checking + run-time checks at `any`/JSON boundary. `.d.ts` mechanical; must be representable | No `eval`; native = unsandboxed. Whole-program rebuilds. Dev DWARF. Library mode: no async/timers. RC+collector. Flat FFI; tracking yours | Build-time only; on-device edits need AOT elsewhere + interpreted Wasm | No: typed IR → LLVM only |
| **tsrs** 0.20β, 4 wks, Apache-2.0 | Check ~55/63 ms cold. 28 MiB + 4 MB libs; Wasm 9.5 MB/122 ms. Offline incl. browser. Crates unpublished, Rust 1.99.1 | Emits JS: speed = QuickJS/tier 1/browser. Checking cited 4–12× tsgo at ~⅓ memory | All of TS 7.1; four mistakes caught (exhaustiveness as missing return). Unsound as TS. `.d.ts` from `TypeOracle` mechanical | Parser-risk only. LSP/watch fit reload. Identical maps. No run time; boundary now typed | No machine code made; check on-device, run interpreted. Needs Wasm CSP | Yes (cheap): emits `JsModule` JS. Tier-1 direct = new codegen |
| **OCaml** 5.4 + Melange/jsoo | 75 ms/file, 430 large, per-file. Guest chain (142 MB stdlib) + assembler/linker; bytecode avoids linker. No browser compiler | Kernel 34 ms; ≥414 KB floor. Melange small JS at engine speed; jsoo 306 KB gz floor. Ints 63 vs 32-bit | Sound, inferred, no null; all four by construction. `.mli` feasible; QML objects awkward; notify inexpressible | Native unsandboxed; bytecode interpreter-shaped. Relink reload. Mature GC (slices unmeasured). C-FFI per property | Bytecode+`ocamlrun`, else build-time. Heavy guest | No; own portable bytecode instead |
| **QuickJS** (today) | Shipped: ~1 MB C, all targets, ~8 ms/200 KB parse. Edit inside 17–31 ms reload. Offline everywhere | Kernel 1.4 s; small 80 ms (beats Node); entry ~250 ns cited | None of the four (tier 0 catches provable members) | The sandbox (limits, slices; no foreign bytecode). Reload-native. Promises today. The tracked boundary | Accepted shape: shipped interpreter, code as data (2.5.2 features clause aside) | Defines it: JS text + bytecode in `ljos_ir` |

## Open questions

1. **Boundary cost, measured?** A scriptc callback or OCaml accessor doing tracked
   `parent.width` reads at binding frequency — unmeasured; until then "compiled handlers
   are faster" is *(unverified)* either way.
2. **App Review on user-tailored code?** 2.5.2's text does not settle it; only a test
   submission or platform advice does. IR/interpreter shape reduces risk, not answers it.
3. **tsrs as a durable library?** Published crates, a checker-facts API, softer pins? And
   what does `--api` cost per reload? Not prototyped.
4. **scriptc refusal rate on real QML script?** `coverage` ran only on this note's files
   (100 %). A transpiled-corpus run with engine host types is the decisive experiment.
5. **wasm_of_ocaml, measured?** Size floor, speed, int semantics vs Melange/jsoo — located
   in opam, not installed.
6. **Interpreted Wasm as universal fallback?** Size/speed/memory of a wasm3-class
   interpreter hosting scriptc-Wasm or `@tsrs/wasm` — none evaluated.
7. **GC inside frame slices.** OCaml's and scriptc's collectors are described, not measured
   against 4 ms slices; long-tail pauses decide.
8. **`ljos_js`'s true shipped size.** The ~1 MB is upstream `qjs` as proxy; no bloat
   accounting of the vendored fork was taken.
9. **Play and console rules in current text.** Characterised generally, marked
   *(unverified)*; fetch before designing against them.
10. **Coding agents in the shipped loop.** Can agents drive `coverage` refusals and tsrs
    diagnostics to self-fix on-device edits, at what cost? Testable, untested.

## Appendix: how the measured numbers were produced

`ai-beast` (Ryzen AI Max+ 395, 32 threads, 62 GiB, Linux x86-64), 2026-10-27; `TMPDIR` on
the scratch disc throughout; nothing installed system-wide. Installs: `npm install scriptc
tsrs typescript @scriptc/runtime-wasm32-wasi@0.4.5 @tsrs/wasm` (in `bench/`); opam switch
`ocaml.5.4.0` + `js_of_ocaml js_of_ocaml-compiler dune melange` (in scratch); QuickJS
2025-04-26 tarball from bellard.org, `make -j16 qjs`. Timing: `bench/timeit.py`
(whole-process wall + child peak RSS, 2–5 runs); scriptc cold runs used fresh
`SCRIPTC_CACHE_DIR`s; Zig 0.16.0 via `mise x` for WASI. Files: `bench/{small,large,kernel,
errors}.ts`, twins in `bench/ocaml|jsoo|mel/`. Node was 24.21.0 except jsoo/Melange output
runs (Node 26 via mise shim under opam). Examples: `scriptc build small.ts -o small-bin`;
`tsrs --noEmit --strict --skipLibCheck large.ts`; `ocamlopt -O2 -o kernel-bin kernel.ml`;
`dune build ./kernel.bc.js`; `qjs kernel.mjs`. Repo facts: GitHub API, 2026-10-27
(`vercel-labs/scriptc`, `oxc-project/tsrs`, `brnrdog/reativa`). Ljós facts: read-only from
the ljos-ui repository (`docs/ARCHITECTURE.md`, `docs/research/11-scripting-language.md`,
`docs/tier1-coverage.md`, `docs/hot-reload-drive.md`, `vendor/QUICKJS.md`, `crates/ljos_js`,
`crates/ljos_ir`, `crates/qml_front`, `crates/qt_types`, `crates/ljos_mcp_author`).
