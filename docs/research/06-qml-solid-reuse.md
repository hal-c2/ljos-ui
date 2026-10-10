# 06 — What of qml-solid Ljós can reuse, and how

Scope: read-only study of `/home/olafura/dev/qml-solid` (same author, MIT, edition 2024, 553 commits between 2026-10-03 and 2026-10-05, HEAD `adaa02e`). No Qt or Slint source was read. Nothing in qml-solid or hal-c2 was modified. Rust is ~11k lines; `packages/runtime` (JS) is ~68k lines.

Measurement caveat: the numbers in §2.5 come from the prebuilt `target/debug/qmlc`, which was built about 2.5 h before the HEAD commit. Treat them as approximate. A fresh build failed with a disk quota error in `/tmp`, so I did not rebuild.

---

## 1. Summary

1. **The parser is the crown jewel and it is cheap to carry.** qml-solid parses QML with a vendored `oxc_parser` 0.144.0 whose diff against crates.io is ~65 changed lines in 4 files plus 633 new lines under `src/qml/` (`ast.rs` 172, `mod.rs` 461). It parses all 495 `.qml` files of the qtdoc examples (2.07 MB) with 0 failures. Embedded JS is real Oxc AST, which suits Ljós's rquickjs path. Because QML mode uses `pub(crate)` parser internals, it cannot be a standalone crate on oxc's public API. It has to stay a patched fork of `oxc_parser`.
2. **Upstream churn is the main cost.** oxc ships weekly with 0.x breaking changes. 0.144.0 (2026-08-10) is already 9 releases behind 0.153.0 (2026-10-05). Plan on a rebase every 4–8 releases, and upstream the lexer patch if possible.
3. **Two modules are already extractable as-is:** `discover.rs` (qmldir and CMake `qt_add_qml_module` discovery, 300 lines, no oxc dependency) and `qt.rs` + `qt/types.txt` (Qt type table, std only, 461 lines + 614 KB). `crates/qmltypes` generates that table from the installed Qt, using the QML parser itself.
4. **The semantic layers are about 50% reusable.** The generic QML front end is: type-name lookup (`quick/types.rs`), context/id chains (`quick/scope.rs`, parts of `scope.rs`), the project/summary graph (`project.rs` minus its usage optimisation). The Solid/web part is: `registry.rs`, `dialects/`, `lower.rs`, `build.rs`, `resolve.rs`, the emission half of `quick/names.rs` and `quick/lower.rs`. The coupling points are few and are listed with line numbers in §2.2.
5. **The comparison tool is partly reusable.** The metric (pixelmatch threshold 0.1, `pixels` and `content` scores, ratchet file) and the Qt reference capture (`corpus/reference.mjs`) are renderer-agnostic. Only the Playwright/Vite plumbing is web-specific. I recommend a small Rust `qml-compare` CLI that takes a renderer command.
6. **Flag: licensing.** `types.txt` and the reference PNGs are derived from Qt installation data. qml-solid's own README says what is built from them "carries Qt's licence". Ljós should not simply vendor them as MIT. Ljós should generate them locally and keep them out of the published artefacts until the user decides. See §2.3 and §4.
7. **Recommended shape:** a Cargo workspace in ljos-ui with `qml_syntax` (the forked parser), `qml_front` (generic semantics) and `qt_types` as shared crates. These live in one repo, are consumed by the other through git/path dependencies, and qml-solid retains only Solid/web lowering. Details in §3.

---

## 2. Detailed findings

### 2.1 The parser

**Pipeline in qml-solid.** `.qml` → `Parser::new(allocator, source, SourceType::ts()).parse_qml()` → a QML tree whose leaves are Oxc `Expression`/`Statement`/`Function` nodes → lowering → resolution (names bound to JS) → the vendored `solidjs-compiler`, which takes an Oxc `Program`. The call site is `crates/qml_solid/src/lib.rs:173–232` (`lower()`).

**What the fork is.** `vendor/oxc_parser` is crates.io 0.144.0 (`rust-version` 1.95.0), wired with `[patch.crates-io] oxc_parser = { path = "vendor/oxc_parser" }` in the root `Cargo.toml`. I diffed it against the downloaded upstream. The patch to existing files is:

| File | Change | Size |
|---|---|---|
| `src/lib.rs` | `pub mod qml;` and `Parser::parse_qml(self) -> qml::QmlParserReturn<'a>` | ~17 lines |
| `src/diagnostics.rs` | `qml(message: &'static str, span: Span)` | ~3 lines |
| `src/lexer/mod.rs` | `pub(crate) qml: bool` field, initialised to false | ~2 lines |
| `src/lexer/string.rs` | QML strings may span lines: `read_string_literal_over_lines` plus two match arms keyed on `$lexer.qml` | ~45 lines |
| `src/qml/ast.rs`, `src/qml/mod.rs` (new) | AST types and recursive-descent parser | 633 lines |

QML mode must live inside the crate because it uses `ParserImpl`, the lexer, `checkpoint_with_error_recovery`, `fatal_error`, and `lookahead`, all `pub(crate)`. A separate crate on oxc's public API is not possible without upstream API changes.

**QML AST** (`src/qml/ast.rs`, all arena-allocated with `'a`):
- `QmlDocument{span, pragmas, imports, root}`, `QmlPragma{span,name,value}`, `QmlImport{span,source,version,alias}`.
- `QmlImportSource::{Module(QmlQualifiedName), Path(&str)}`; `QmlQualifiedName{span, parts: ArenaVec<&str>}` with `as_simple`, `last`, `Display`.
- `QmlObject{span, type_name, on, members}`.
- `QmlMember::{Object, Binding, Property, Signal, Function(ArenaBox<Function>), InlineComponent, Enum}`.
- `QmlBinding{span,name,value}`; `QmlBindingValue::{Expression(Expression), Statement(Statement), Object, Objects}`.
- `QmlPropertyDeclaration{span, is_default, is_required, is_readonly, type_name: Option<QmlTypeName>, name, value}`; `QmlTypeName{span,name,is_list}`.
- `QmlSignalDeclaration{span,name,params}`, `QmlSignalParameter`, `QmlInlineComponent`, `QmlEnumDeclaration{span,name,members}`, `QmlEnumMember{name, value: i64}`.

**Entry points.** Only `Parser::parse_qml()`. `parse_qml_document` (`src/qml/mod.rs:58`) loops over pragmas/imports and then parses exactly one root object. It returns `QmlParserReturn<'a>{document, comments, diagnostics, panicked}`. Contextual keywords (`property`, `signal`, `required`, `readonly`, `component`, `enum`, ...) are recognised by `qml_at_word` / `qml_next_is_name` lookahead (`mod.rs:450–460`). Whether `{ ... }` after a binding is an object literal or a block uses checkpoint/rewind in `parse_qml_braced_value` (`mod.rs:391`).

**Error recovery: fatal only.** Syntax errors go through `set_fatal_error` and set `panicked`; the parse stops. There is no member-level resynchronisation and no partial tree. `Kind::At` annotations give a fatal "QML annotations are not supported yet". `tests/parse.rs::syntax_errors` only asserts that diagnostics are non-empty for `Item { width: }`, unterminated objects, a bare `import`, two roots, and empty input. For a batch compiler this is fine. For an editor, a hot-reload loop that wants to keep the last good tree, or good error messages, it is not, so Ljós needs to add resync at member boundaries (`;`, newline followed by an identifier and `:`, `}`), see §3.

**Diagnostics.** The public error type is `Error{message, span}` with `position(&str) -> (line, col)` (`lib.rs`, `parse_errors` at lines 151–162). There is no severity, code, or label support, although oxc's own `OxcDiagnostic` is underneath. Ljós should keep `OxcDiagnostic` (or map to `miette`-like labelled diagnostics) for tests and the hot-reload overlay.

**Spans.** Spans are byte offsets into the file, as in Oxc. `Span::default()`/`SPAN` marks synthesised nodes. There is no CST and no trivia beyond the `comments` list, so a formatter or refactoring tool cannot be built on it as-is.

**Embedded JS.** Expression, statement, and function bodies are parsed by the normal Oxc JS parser with `SourceType::ts()`, so TypeScript-style annotations in function parameters already work. That gives Ljós full ES syntax for rquickjs, and a ready-made AST for static analysis (free-variable/name resolution), which is exactly what the `resolve.rs` pass does for Solid.

**`.js` imports.** `.js` files are not parsed by QML mode. `quick/script.rs:52` `directives()` is a textual pre-scan for `.pragma library` and `.import X as Y` lines, which it recognises while they are commented out with `//`. After that the script is parsed with `Parser::new(.., SourceType::script()).parse()` (`script.rs:83–`). Ljós can reuse this but should note that QML's `.import` is not valid JS and must be stripped or blanked first (the textual approach preserves offsets).

**Arena lifetimes.** All nodes borrow an `oxc_allocator::Allocator` (`'a`), and a document cannot outlive its arena. Consequences for Ljós:
- Hot reload: keep the `Allocator` and the source `String` together in one owning struct (self-referential; use `ouroboros`/an `Arc`-held pair with an unsafe-free approach such as lowering immediately), or lower the AST to an owned IR before the arena is dropped. I recommend the latter: parse → lower to an owned `Vec`-based IR (ids, bindings as JS source ranges) → drop the arena. This also sidesteps `Send`: I could not confirm from the 0.144 `oxc_allocator` source whether `Allocator` is `Send`; it is bumpalo-derived and certainly not `Sync`, so share nothing across threads and parse per file on the worker that owns the arena.
- Per-file arenas also let a parse of one changed file run on a tokio blocking task while the UI thread keeps the old IR.

**Compile time and weight.** `cargo tree` shows ~106 crates for `oxc_parser` alone (it pulls in `icu_*` via identifier tables, `miette` via `oxc_diagnostics`, `phf`, `syn`/proc-macro crates for build-time derive). `crates/qml_solid` has 151 crates including `solidjs-compiler`; `crates/qmltypes` has 106. For Ljós, a runtime that embeds the parser (for dev-mode hot reload) pays this weight; a release build that precompiles QML to an IR does not need to ship it.

**Maintenance burden.** The patch is tiny and well isolated, so a rebase is mostly mechanical: re-add the `pub mod qml`, `parse_qml`, the lexer flag, and the string tweak, then fix breakage in `qml/mod.rs` where it touches internals (`checkpoint_with_error_recovery`, `lookahead`). The risk is not the patch size; it is the cadence: weekly breaking releases, with 9 releases (two months) already behind. Mitigations are in §3.

**Alternatives.**

| Option | Pros | Cons |
|---|---|---|
| Keep the fork (recommended) | Embedded JS is Oxc-grade; tiny patch; proven on 495/495 files | Rebase cadence; 106-crate tree |
| Upstream a generic extension hook to oxc | Removes the fork | Unlikely to be accepted for QML specifically; needs API design work and review latency |
| Separate crate on Oxc public API | No fork | Not possible: needs `pub(crate)` internals |
| `tree-sitter-qmljs` | Error-tolerant, incremental, ideal for editors | Separate JS grammar, not Oxc AST; second parser to keep in sync; C dependency |
| Hand-written QML parser plus `boa`/`swc` for JS | Full control over recovery and CST | Largest effort; loses the working corpus result |

Recommendation: keep the fork for the compiler, add member-level recovery to it, and consider `tree-sitter-qmljs` only for an editor/LSP later.

### 2.2 Semantic layers: web/Solid-specific versus generic

| File | Lines | Generic QML front end? |
|---|---|---|
| `discover.rs` | 300 | **Yes**, standalone (no oxc) |
| `qt.rs` + `qt/types.txt` + `qt/tests.rs` | 461 + 17,986 + tests | **Yes**, standalone (std only) |
| `project.rs` | 836 | Mostly; coupled in 8 places (below) |
| `quick/types.rs` | 522 | **Yes** semantics; coupled to `Project`/`Key`/`qt`; some web-driven flags |
| `quick/scope.rs` | 459 | **Yes**: `Tree`, `Object`, `Context` chain |
| `scope.rs` | 670 | Half: id/owner/alias/delegate structure generic; `Member` enum Solid-specific |
| `quick/names.rs` | 783 | Resolution order generic; emission Solid-specific |
| `quick/lower.rs`, `lower.rs`, `build.rs`, `resolve.rs` | 1108, 1069, 625, 412 | No (JSX/Solid/`createMemo` emission) |
| `quick/paths.rs`, `texts.rs`, `resources.rs`, `script.rs` | 348, 321, 65, 225 | AOT handling of `Qt.createComponent`/`createQmlObject`, qrc, JS modules; JS-emission-specific, ideas reusable |
| `registry.rs`, `dialects/opentui.rs` | 125, 148 | No: web/CSS prop tables, drop for Ljós |

**`discover.rs`.** `pub fn qmldir(text) -> Option<Module>`, `cmake(text) -> Vec<Module>`, `kept(text) -> Vec<Resource>`; types `Module{uri, types, resources, prefix}` (with `address()`), `Resource{address, path}`, `Type{name, path}`. It has a hand-rolled CMake command tokenizer for `qt_add_qml_module` (URI/QML_FILES/RESOURCES/RESOURCE_PREFIX) and `qt_add_resources` (PREFIX/BASE/FILES), with `set` and `list(APPEND)` variable expansion. The qmldir parser handles `module`, `singleton`, `internal` and `Name [ver] File.qml` but ignores `prefer`, `typeinfo`, `import`/`optional import`, `depends`, `plugin`. Ljós should copy it unchanged and later add `depends`/`import` for correct module graphs.

**`project.rs`** (a flat `file → Summary` map keyed by component name strings). Types: `Project{files, modules, memberships, standins, resources}`, `Key{file, inline}`, `Summary{components, instances, imports, is_singleton, shapes}`, `Shape{root, properties, signals, functions, required, has_default, enums, ids, mentions, sought}`, `Interface`, `Instance`, `Usage`, `Lookup`. API: `add(name, source)`, `directories()`, `add_type(uri,name,file)`, `add_resource`, `stand_in`, `resolve(file, type_name)`, `lookup(key, name)`, `usages()` (fixed point), `dynamic_names()`, `summarize(&QmlDocument) -> Summary`, plus `directory`/`join` path helpers.

Coupling to untangle (line numbers as of HEAD):
- `project.rs:21`: `use registry::{Element, Prop, Types}`: web dialect types.
- `project.rs:133`: `Root::Element(&'static Element)`: a root can be a web element. Replace with `Root::Builtin(TypeId)`.
- `project.rs:371–376`: `element.prop(name)` and `match Prop::Event/Keyword`: web prop classification inside the summary. Move behind a `TypeOracle` trait.
- `project.rs:441`: `Types::of(&document.imports)`: dialect detection from imports.
- `project.rs:665–667`, `715`: `types.element` / `types.lookup`: same oracle.
- `project.rs:616`: `texts::given/written`: the `Qt.createQmlObject` text scan lives in `quick::texts` (emission module).
- `project.rs:297`: `qt::is_member_name`: fine, `qt` will be a shared crate.
- `usages()` / `Interface` / `children_in_root`: the Solid-specific "a component accepts only the props instances set" optimisation. Ljós doesn't need it; keep it in qml-solid.

**`quick/types.rs`** is the closest thing to a QML type-resolution spec: `Types<'p>{project, file}::find(&[&str]) -> Option<Found{kind: Kind::{Qt, Component}, origin: Origin::{Module, File, Inline, Namespace}}>`. Lookup order: inline components, imports last-to-first, own directory, module memberships, Qt modules. Its `Property` flags are partly web-driven (`is_url`, `is_script`, `takes_key`); keep the first two (valid for Ljós asset and script handling), drop `takes_key`.

**`scope.rs` / `quick/scope.rs`.** `Scopes::analyze(&QmlDocument, &mut Vec<Error>)` with `Object{members, signals, delegate, component_root, outer, document}`, and `quick::scope::Tree{objects, contexts}` with `Object{span, name, kind, origin, handle, declared, context, is_template}` and `Context{root, outer, data}`. The `Member::{Const, Getter, Memo, Signal, Function, Unsupported}` enum encodes Solid binding kinds and must be generalised to `Const | Binding | Signal | Function | Unsupported` (Ljós's reactive layer decides how a binding is evaluated). Generic helpers worth taking verbatim: `classify`, `handled`, `Own::{Signal, Changed, Completed, Destruction}`, `alias_target`, `object_id`, `is_literal`.

**`resolve.rs` and `quick/names.rs`.** The resolution order (id → own property → component root → outer context → `Qt` → globals) is QML-spec-like and valuable. The code emits `createMemo`/getter calls; Ljós needs the same lookup returning a *binding target* (`Id(ObjectId)`, `Property{object, name}`, `Context(ctx, name)`, `Global`) rather than a JS expression. I would rewrite the emission half and port only the lookup logic.

### 2.3 `crates/qmltypes` and the Qt type table

**What it does.** `crates/qmltypes` (`main.rs` 483, `qmltypes.rs` 224, `qmldir.rs` 103, `qmlfile.rs` 91, `model.rs` 125, `write.rs` 141) parses Qt's installed `*.qmltypes` files and `qmldir` files with the QML parser (dogfooding), plus some QML files (e.g. Controls' `Button.qml`) to extract declared members, and writes `crates/qml_solid/src/qt/types.txt`. Inputs: a hard-coded `MODULES` list of ~50 modules (those the examples import) and `--qt DIR` (repeatable; default `QML_IMPORT_PATH`, then `/usr/lib/qt6/qml`). It runs as `mise run types` (`cargo run -p qmltypes`).

**Output.** `qt/types.txt` (614 KB, 17,986 lines, header "Qt 6.11", "Not edited by hand"), loaded by `include_str!` at `qt.rs:328` into a lazy `OnceLock<Table>`. Indented text lines: `module`, `import`, `type CLASS QMLName`, `prototype`, `extension`, `attached`, `default`, `is ...`, `enum`/`flags`, `prop name Type [readonly pointer list required]`, `signal`, `method`. Stats: 50 modules, 1351 types, 5562 properties, 5085 signals, 528 enums, 2372 methods.

**API** (`qt.rs`): `module(uri)`, `native_types(uri)`, `written_types(uri)`, `Module{uri, imports: Vec<Import{module, kind: ImportKind::{Always, Default, Optional}}>}`, `Type::{prototype, default_property, attached, attaches, property, signal, has_method, enum_value}`, `Property`, `Signal`, `Enum`, `EnumValue`, `is_member_name`. 15 tests in `qt/tests.rs`.

**Gaps relevant to Ljós.** The table has **no property default values, no method signatures or parameter types, no enum numeric values** (qmltypes only lists names since Qt 5.15), and no versions. For a renderer these matter: default values (e.g. `Text.font.pixelSize`, `Item.opacity = 1`), method arities for `Qt.*` and item methods, enum values (`Qt.AlignHCenter` = 4 etc.). Ljós will need a second, hand-written (or independently authored) data layer for these, written against Qt's *public documentation of behaviour* only, not by reading Qt source.

**Use for Ljós.** (a) Static checking and completion: "is this a known property/signal of this type", the main use in qml-solid; (b) driving the *binding surface* of Ljós's egui-backed Quick types: a conformance test can assert that every `prop` in the table for the supported types is either implemented or explicitly listed as unsupported; (c) generating the `Qt` namespace stubs for rquickjs.

**Licensing / clean-room assessment (flag).**
- The generator reads *installed* Qt metadata (`.qmltypes`, `qmldir`). Those files are machine-generated descriptions of API surface (class names, property names and types, signal names). The facts themselves (that `Item` has a `width: real`) are API facts, but the *file as a collection* and the extracted QML-file member names come from Qt-licensed files (LGPL/GPL/commercial for the modules involved).
- For the style modules (Controls) the tool also *parses Qt's QML source files* and takes only member names. I saw no verbatim source copied into `types.txt`, but derived data from Qt-licensed sources is the part the clean-room rule is nervous about.
- qml-solid's README itself states: "Qt's QML and pictures are read from the installation and are not part of this repository: what is built from them carries Qt's licence", so the author has already drawn the line at *not committing* Qt-derived material. Yet `types.txt` **is** committed in `crates/qml_solid/src/qt/`. That is an inconsistency worth the user's decision.
- The clean-room rule for Ljós says not to read Qt or Slint source. Running a generator that parses Qt's source `.qml` files at build time is borderline (a program reads it, a human does not), but I recommend avoiding it for Ljós: restrict the generator to `.qmltypes` and `qmldir`, and treat the Controls member lists as data authored from public documentation.
- Recommendation: Ljós keeps the generator (MIT, ours) but does **not** commit `types.txt`; it is produced by `mise run types` against a developer's local Qt, git-ignored, and the crate falls back to a small built-in table for CI. If the user decides the facts-only table is acceptable to commit, record that decision in the repo. I am not a lawyer; this needs the user's call.

### 2.4 Comparison and gallery

**Reference pictures.** `corpus/reference.mjs` (16.5 KB) runs real Qt with `qml`/`qml6 -I <importPaths> harness.qml`. The harness QML is either `itemHarness` (a `Loader` in a white `Rectangle`) or `windowHarness` (`createComponent` + `grabToImage`). Environment: `QT_QPA_PLATFORM` is either `wayland` against a headless sway (`output HEADLESS-1 resolution 1920x1080`, floating windows; needed for OpenGL and shader effects) or `offscreen:configfile=screen.json`; plus `QML_DISABLE_DISTANCEFIELD=1`, `QML_DISABLE_DISK_CACHE=1`, `TZ=UTC`, `LANG=en_US.UTF-8`, `QT_SCALE_FACTOR=1`. Offline examples run under `unshare --map-root-user --net`. qsb shaders are compiled, the qmldir is rewritten to drop `prefer`/`typeinfo`, timeout is 30 s. It writes `corpus/reference/<id>.png` (20 PNGs, 1.6 MB) and `reference.json` (`capturedAt`, `width`, `height` per example). `assets.mjs` (6 KB) fetches assets.

**Corpus manifest.** `corpus/examples.json` has 27 examples (20 runnable, 7 not). Fields: `id, dir, entry, root (Window|Item), rootType, load, window{width,height,from,imposed}, importPaths, modules, imports, cpp, assets, volatile{time|animation|network}, qt{runnable, reason, style, controlsConf, env, module, properties, offline, settle, shaders}, notes`; top-level `root: "qtdoc/examples"` and `screen: {1920x1080}`. `corpus/standins/` has QML + `main.js` stand-ins for C++ types (12 dirs).

**Metric** (`examples/gallery/tests/compare.js`, 59 lines):
- `pixelmatch(a, b, mask, w, h, {threshold: 0.1, diffMask: true})` (YIQ perceptual, anti-aliasing excluded).
- `pixels = 1 - different/total`.
- `content`: only pixels that are not background in either picture (background = the reference's commonest colour, tolerance Σ|dRGB| ≤ 24): `content = 1 - contentDifferent/content` (1 if there is no content).
- Size mismatch yields a `mismatch` text instead of scores. A diff PNG is written (alpha 0.3).

**Ratchet.** `corpus/expected.json` holds per-example `{pixels, content}` floors (e.g. coffee 0.99/0.96, samegame 0.9/0.75). The test fails if a listed example stops rendering or drops below its floors; `GALLERY_RATCHET=1` adds newly rendering examples.

**Gallery plumbing (web-specific).** `manifest.js` (`readManifest` adds directory, entryFile, reference, standins, main, capturedAt, fetched, served); `main.js` (81 lines, `?example=<id>`, sets `window.gallery = {status, error, busy}`); `vite.config.js` (147 lines, `virtual:examples`, style selection); `playwright.config.js` (Chromium, DSF 1, en-US, UTC); `tests/report.spec.js` (204 lines); `tests/table.js` (136 lines, a Playwright reporter producing a terminal table, `report/report.json` and `index.html`); `tests/selftest/` (10 tiny known-answer examples with their own `run.mjs` and a stand-in `qmlc.mjs`). Volatility handling: examples with `volatile` time/animation use Playwright clock control (time fixed to `capturedAt`, animation paused at 1000 ms and run for `settle` ms); network is blocked. The viewport is `max(w,800) × h+200` with CDP `Emulation.setDeviceMetricsOverride`, and screenshots are taken of `#stage`.

Per-example result row: `id, entry, window, files, compiled, entryCompiles, renders, error, stack, reference, qt, pixels, content, mismatch, lacks, compileErrors, said[]`; outputs `<id>.web.png`, `<id>.qt.png`, `<id>.diff.png`.

**What transfers to Ljós.** Everything except the Playwright/Vite plumbing: the manifest, stand-ins, references, metric, ratchet, row schema, volatility rules (fixed time, paused animation + `settle`, no network). Ljós's equivalent of "render in Chromium" is "render with egui and capture via egui_kittest / a wgpu screenshot". Fixing the clock and pausing animation is a Ljós runtime feature (injectable clock), which should be designed in from the start.

### 2.5 Tests and corpus as a conformance dashboard

**Existing tests.** `tests/parse.rs` (234 lines, 6 tests), `compile.rs` (14), `components.rs` (9), `discover.rs` (4), `fixtures.rs` (1; tui snapshots in `tests/snapshots` and `fixtures/tui`), `quick.rs` (1549 lines, 50 tests), `qt/tests.rs` (15). `examples/parse_check.rs` parses every file named on the command line, prints `path:line: message`, and finishes with `N parsed, M failed`.

**Corpus.** `corpus/qtdoc` is a shallow git submodule of `https://code.qt.io/qt/qtdoc.git`, branch 6.11, pinned at `e05e4f4` (70 MB; 495 `.qml` files, ~2.07 MB, in `examples/`, plus 3 `.js`; the whole corpus has 524 `.qml`). The examples are Qt-licensed; they are a submodule, not copied in, which is the right pattern for Ljós too.

**Measured conformance** (debug binaries, stale by ~2.5 h):

| Stage | Result |
|---|---|
| Parse only (`parse_check`, 495 files) | **495 parsed, 0 failed**, 0.10 s |
| `qmlc --alone` (no project) | 158 compiled / 337 failed; **not meaningful**: the errors are "`_` is not defined" (1678) and "`_` is not a type of anything the file imports" (833), because components are unknown without a project |
| `qmlc` project mode (one batch, 7.4 s) | **464 compiled / 31 failed (93.7%)** |

The 31 failures are all C++-registered types needing stand-ins: `StockEngine` (58 errors), `SvgPathItem` (22), `transaction` (10), `LightningController` (7), `TimeFormatter`, `Product`, `FileNameProvider`, `ShapeAffector3D`, etc. They are *expected* gaps, not syntax errors.

**Dashboard proposal for Ljós.** Three columns per example, fed by one `report.json`:
1. **Parse**: `parse_check`-style, must be 100% and is a hard CI gate.
2. **Compile/load**: Ljós front end builds the IR with no diagnostics; failures grouped by cause (unknown type, unsupported feature) so the dashboard doubles as a feature backlog (the `lacks` and `said[]` fields already do this).
3. **Render**: pixel/content scores against `reference/*.png` with the `expected.json` ratchet.

Ljós should use the same 27-example manifest and the same stand-ins (the stand-ins are plain QML + JS authored by the same author, MIT, so shareable) but must not assume the same renderer outcomes: the ratchet floors are per-renderer, so Ljós gets its own `expected.json` starting empty.

### 2.6 mise tasks and conventions

`mise.toml` pins `rust = { version = "1.98.1", targets = "wasm32-unknown-unknown" }`, `node = "24"`, `pnpm = "12.8.1"`. Tasks are bash scripts in `mise-tasks/` with `set -euo pipefail` and `#MISE description=`, `#MISE depends=[...]`, `#MISE sources=`, `#MISE outputs=` headers; a `_default` file defines a group task. Existing tasks:

- `install` (mise install), `packages` (pnpm install; with sources/outputs), `browser` (playwright install chromium),
- `build` (+ `build:compiler`, `build:web`), `dev`,
- `test` (+ `test:compiler` = `cargo test`, `test:runtime`, `test:web`, `test:gallery` = selftest),
- `gallery` (+ `gallery:serve`),
- `corpus` (`git submodule update --init --depth 1 corpus/qtdoc`; then `corpus/report.sh`), `reference` (`node corpus/reference.mjs`), `assets` (`node corpus/assets.mjs`), `types` (`cargo run -p qmltypes`).
- Any task needing the corpus first runs the shallow submodule update.

Cargo conventions: workspace `resolver = "3"`, `edition = "2024"`, MIT, `[profile.dev] debug = "line-tables-only"` (faster builds), `exclude = ["vendor"]`. All of these are good defaults for Ljós. Ljós needs a Rust that can run edition 2024, and the vendored parser needs ≥ 1.95.0.

---

## 3. Concrete recommendations

### 3.1 Crate split

Four shared crates, all MIT, living in whichever repo is the "home" (I recommend ljos-ui as home for the generic ones, and qml-solid depending on them, since Ljós is the larger long-term consumer; if the user prefers qml-solid as home, the layout is the same mirrored):

| Crate | Content | Source in qml-solid |
|---|---|---|
| `qml_syntax` | Forked `oxc_parser` with `qml/` module (as `vendor/oxc_parser`, renamed crate `oxc_parser_qml` if published), re-exports `QmlDocument` etc. | `vendor/oxc_parser` |
| `qml_front` | `discover` (as-is), `project` (minus `usages`, with a `TypeOracle` trait replacing `registry`), `types` (from `quick/types.rs`), `scope` (generic part of `scope.rs` + `quick/scope.rs`), name-lookup (from `quick/names.rs` lookup half), `script::directives` | `discover.rs`, `project.rs`, `quick/types.rs`, `quick/scope.rs`, `scope.rs`, `quick/names.rs`, `quick/script.rs` |
| `qt_types` | `qt.rs`, `qt/tests.rs`, `types.txt` (generated, see licensing) | `qt.rs`, `qt/` |
| `qmltypes` (tool) | the generator, restricted to `.qmltypes` + `qmldir` | `crates/qmltypes` |

qml-solid then keeps `lower.rs`, `build.rs`, `resolve.rs`, `registry.rs`, `dialects/`, `quick/{lower,paths,texts,resources}.rs`, `usages()`, and the Solid `Member` mapping, all on top of `qml_front`.

### 3.2 Workspace layout for ljos-ui

```
ljos-ui/
├── Cargo.toml                 # workspace, resolver 3, edition 2024, exclude = ["vendor"]
├── mise.toml                  # rust 1.98+, node only if gallery/reference scripts kept in JS
├── mise-tasks/                # install, build, test, types, corpus, reference, gallery, dashboard
├── vendor/oxc_parser/         # or git dependency on the shared fork          [shared with qml-solid]
├── crates/
│   ├── qml_syntax/            # re-export + parse helpers over the fork        [shared]
│   ├── qml_front/             # discover, project, types, scope, names          [shared, from qml-solid]
│   ├── qt_types/              # qt.rs + generated table                         [shared, from qml-solid]
│   ├── qmltypes/              # generator CLI                                   [shared, from qml-solid]
│   ├── qml_compare/           # pixel metric + report/ratchet CLI               [new, usable by qml-solid]
│   ├── ljos_ir/               # owned IR (objects, bindings, signals)            [new]
│   ├── ljos_js/               # rquickjs bridge: bindings, signal handlers      [new]
│   ├── ljos_render/           # egui scene + Quick/Controls item set            [new]
│   ├── ljos_runtime/          # tokio, hot reload, accessibility, i18n          [new]
│   └── ljos_cli/              # `ljos run|check|dump`                           [new]
├── corpus/                    # qtdoc submodule (shallow), examples.json, standins/, expected.json
└── docs/research/
```

How to share across the two repos without copying: for now, use **git dependencies pinned by `rev`** from qml-solid to ljos-ui (or the reverse) for `qml_syntax`, `qml_front`, `qt_types`. Move to a path-based single workspace later if the two projects merge. Do not duplicate the fork: two diverging copies of `vendor/oxc_parser` is the failure mode to avoid.

### 3.3 Migration steps

1. **Freeze a baseline** in qml-solid: run `parse_check` and project-mode `qmlc` on a fresh build; record 495/495 and 464/495 as the regression bar (my numbers came from a stale binary).
2. **Extract `qt_types` and `discover`** first. They have no oxc coupling, so it is a move plus a `pub` surface review, with tests going along (15 + 4).
3. **Extract `qml_syntax`**: move `vendor/oxc_parser`; add a `docs/FORK.md` listing the exact patch (the table in §2.1) and a script `mise run oxc:diff` that diffs against the matching crates.io version. Adopt the rebase policy in §3.4.
4. **Introduce `TypeOracle`** in `project.rs` to replace `registry::{Element, Prop, Types}` at the lines listed in §2.2, with qml-solid supplying the web implementation and Ljós a Qt-types-based one. Do this inside qml-solid first, with its tests green, before the crate move.
5. **Move the generic scope/types/names code** into `qml_front`; keep the Solid `Member` mapping in qml-solid.
6. **Parser improvements (in the shared fork, benefiting both):** member-level error recovery with partial tree, annotation support (`@Foo`), and richer diagnostics (code, labels, severity). Add tests mirroring `tests/parse.rs::syntax_errors` but asserting that the recovered tree contains later members.
7. **Build `qml_compare`** (§3.5) and wire the corpus into ljos-ui as a submodule at the same pin (`e05e4f4`).
8. **Port the mise tasks** (§3.6) and add the dashboard.
9. Only then start `ljos_ir`/`ljos_render`, using `qml_front` output.

### 3.4 Fork maintenance policy

- Rebase every 4–8 oxc releases, or immediately when a security or parser-correctness fix lands, not weekly. Pin an exact version.
- Keep the patch confined to: `lib.rs` (3 hooks), `diagnostics.rs`, `lexer/{mod,string}.rs`, and the `qml/` directory, so conflicts stay local.
- A CI job (`mise run oxc:check`) tries `cargo update -p oxc_parser` against the newest release in a scratch copy and reports whether the patch applies, so drift is visible before it hurts.
- Open an upstream issue/PR for the generic part (a lexer "multi-line strings" flag is the only change outside `qml/`); if accepted, the fork shrinks to the `qml/` directory plus `pub(crate)` visibility requests. Do not count on this.

### 3.5 Comparison tool: what to extract

Extract a **Rust `qml_compare` crate and CLI** with these subcommands:
- `qml-compare diff <ref.png> <actual.png> --threshold 0.1 [--out diff.png]` → JSON `{pixels, content, mismatch?}` using the exact algorithm of `compare.js` (YIQ pixelmatch with AA exclusion; background = commonest colour of the reference, tolerance Σ|dRGB| ≤ 24). A Rust pixelmatch port is small (~200 lines); keep a cross-check test against the JS results on the 20 reference PNGs so both implementations stay in step.
- `qml-compare run --manifest corpus/examples.json --renderer '<cmd> {id} {out.png}' [--ratchet corpus/expected.json] [--update]` → runs the renderer per example, honours `volatile`/`settle`/`window` by passing them as environment variables or JSON on stdin, writes `report.json` in the existing row schema, prints the terminal table, and enforces/extends the ratchet (the equivalent of `GALLERY_RATCHET=1`).
- Output `report.json` with the same row fields (`id, entry, compiled, renders, pixels, content, mismatch, lacks, compileErrors, said`), so one HTML report works for both projects.

**Keep in JS** (in qml-solid): `reference.mjs` (capture Qt references; sway/offscreen set-up, qsb, unshare), `assets.mjs`, the Vite/Playwright gallery, and `manifest.js` as the web-side reader. Ljós consumes the *outputs* (`reference/*.png`, `reference.json`, `examples.json`) through the shared corpus directory, so Ljós needs Node only if it wants to regenerate references. Longer term, port `reference.mjs` to a Rust or shell tool if Node becomes unwelcome, but it is not on the critical path.

### 3.6 mise conventions to copy

Same layout (`mise-tasks/`, bash, `set -euo pipefail`, `#MISE` headers), same task names where meaning is the same: `install`, `build`, `test` (+ `test:front`, `test:render`), `types`, `corpus`, `reference`, `compare`, `dashboard`. Add `check` (fmt + clippy + parse_check) and `oxc:check`. Pin tools in `mise.toml` (Rust 1.98.1; drop `wasm32` target unless a web build is planned; Node only if JS scripts are retained). Use `sources`/`outputs` headers on slow tasks (`types`, `packages`) so mise can skip them. Keep `debug = "line-tables-only"` for dev builds. Tasks needing the corpus start with the shallow submodule update.

---

## 4. Risks and open questions

1. **Licensing of `types.txt` and `reference/*.png`** (decision needed). Both derive from Qt installs; qml-solid commits `types.txt` despite its README stating otherwise. Options: (a) generate locally and git-ignore (my recommendation); (b) commit the facts-only table with a recorded rationale; (c) replace with a table authored from public documentation. Reference PNGs of Qt-rendered qtdoc examples have the same question; the qtdoc examples themselves are fetched as a submodule, which is fine.
2. **Clean-room boundary for the generator.** It parses Qt's source `.qml` files for the style modules. Even though only member names are taken, I recommend Ljós not do this (see §2.3).
3. **Fork drift.** 9 releases behind in two months. If nobody rebases, the fork stalls on an old Rust MSRV and old dependencies; if the rebase is mechanical it is cheap, but it must be scheduled.
4. **Fatal-only error handling** blocks editor tooling and good hot-reload UX (a typo mid-edit kills the whole parse). It is the first parser improvement to make; the design (member-level resync, partial tree) must be done *inside* the fork.
5. **Arena lifetimes and `Send`.** I could not confirm `Allocator`'s `Send`/`Sync` status from the 0.144 source; assume per-file, thread-local arenas and an owned IR. Holding AST across hot-reload generations needs that design decision early.
6. **Dependency weight** (~106 crates for the parser, including `icu_*` and `miette` via `oxc_diagnostics`): acceptable for dev builds; a release app should not link the parser unless runtime QML loading is required. If Ljós must load `.qml` at runtime on mobile, binary size and compile time need measuring (not done here).
7. **The type table is too thin for a renderer.** No defaults, enum values, or method signatures. This is a second data-authoring effort, and it is where Ljós's real Qt-compatibility work is.
8. **Measurement quality.** The 495/495 parse and 464/495 compile numbers came from a prebuilt binary built ~2.5 h before HEAD; the `--alone` figures are not meaningful. A fresh build (once the /tmp disk quota is available, or with `CARGO_TARGET_DIR` elsewhere) should re-run them before they are quoted as a baseline.
9. **Semantic gaps in `discover.rs`**: no `depends`, `import`, `optional import`, `prefer`, `plugin`, `typeinfo` handling. Fine for the examples; module graphs for real apps need them.
10. **Home repo choice.** Whether shared crates live in ljos-ui or qml-solid, and whether they are published to crates.io (requires renaming the forked `oxc_parser` and a clear fork notice) or consumed by git `rev`. I recommend git `rev` for now.
11. **Dialect scope.** qml-solid has two paths (`Target::Dialect` and `Target::Quick`), selected by `Types::of(&imports).has_dialect()`. Ljós is only the Quick path; the dialect machinery (`registry.rs`, `dialects/`, `scope.rs`'s Solid half) is dead weight to leave behind, and removing it from the shared code is part of the split.
12. **Performance claims.** Parse speed (0.10 s for 2.07 MB in a debug build) is encouraging for the "performance" goal but says nothing about runtime; no render or binding-evaluation numbers were obtained.
