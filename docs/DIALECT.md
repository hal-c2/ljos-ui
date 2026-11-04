# Ljós's own API, typed scripts, and Qt's QML as compat

- **Status:** phase 1 (design and a spike), 2026-11-04. The spike's slice is
  built on the `dialect/phase-1` branch; everything past it is design.
- **Decision record:** [ADR 0004](adr/0004-compat-lowering-before-the-ir.md)
  (compat is a lowering pass in front of a Qt-agnostic IR).
- **Inputs:** [ARCHITECTURE.md](ARCHITECTURE.md) (which wins over this
  document), [note 11](research/11-scripting-language.md) (the scripting
  language, recommendation H1), notes 12 and 13 (script compilers, AOT),
  [declarative.md](declarative.md), [MIGRATION.md](MIGRATION.md), and
  ADRs 0002 and 0003.

Ljós runs Qt's QML. This document is about what Ljós offers *beside* it:

- a QML of its own, with **the same syntax and a new API** (modules, types,
  globals and typed properties);
- **scripts in a strict subset of TypeScript**, checked in Rust before
  anything runs;
- **Qt's QML as a compatibility mode**, lowered to that API as a file loads
  and, by the same edits, rewritten on disk by `ljos migrate`.

So there is one runtime and one API underneath, and the Qt surface lives in
exactly one place: the compat pass and its type table.

## 1. Goals and non-goals

### Goals

1. **One API underneath.** The IR, the engine and every native type know
   the Ljós API only. Over time, Qt's names exist only in `qml_compat` and in
   the Qt table it reads (`qt_types`). A new native type is written once, in
   Ljós's terms.
2. **Qt's QML keeps running, unchanged and at the same cost.** A file
   written for Qt Quick runs as it does today. Lowering it is a load-time
   text edit that is all-or-nothing per group of files. A file that cannot
   be lowered runs as written, on the same runtime. It must cost nothing at
   run time (§11).
3. **Typed scripts.** Handlers, functions and bindings in a Ljós file are
   strict TypeScript (note 11's H1). Every program the checker accepts is
   valid TypeScript with the same meaning, so `tsc`, editors and coding
   models keep working on it.
4. **Typed declarations.** In a Ljós file a property has a type the
   checker knows: a QML basic type, a component, a native type, or a type
   declared in TypeScript (`property Logic.Filter working`, not
   `property var working`).
5. **Migration is mechanical.** Each rename or removal in the API has a
   lowering. Anything that cannot be lowered is a decision recorded here
   (§13). `ljos migrate` writes the lowering into the file, so a migrated
   file is the file a lowered run compiled.
6. **The tools carry over.** The parser, formatter, lossless edits (Studio),
   the MCP tools and ljos-web see the same syntax and need no fork.
7. **General purpose.** Nothing in the API is shaped for one application.

### Non-goals

- **No new syntax.** There is no `match`, no `option` and no extension to
  QML or TypeScript (note 11's H2 is rejected). A Ljós file parses with
  today's parser.
- **No second script language.** There is no ReScript-like front end (note
  11's B). JavaScript stays, but only as compat.
- **No formal soundness.** H1 closes TypeScript's known holes by rule and
  forbids `any`, casts, `!` and `null`. It is not a proof (§5, risks).
- **Qt's QML is not deprecated.** Compat is a first-class mode: Qt's own
  corpus runs in it, and qml6 stays the oracle for it.
- **Not everything at once.** The API grows type by type. A Qt file keeps
  using whatever the API has not taken yet, so the lowering covers what
  exists, never more.

## 2. What the evidence says

`docs/research/tools/dialect_inventory.py` counts the Qt surface over the
corpus (536 files, 65 825 lines: the qtdoc examples and snippets, comments
and strings masked). The counts that shape the API:

| Pattern | Uses | Files | What it means for the API |
|---|---:|---:|---|
| `property` (all) | 5467 | 373 | Declarations are the bulk: types must be cheap to write. |
| `property var` / `variant` | 111 | 60 | Untyped state. These become typed or decoded at a boundary (§4.4). |
| `required property` | 314 | 128 | Already typed and explicit: kept. |
| `Qt.*` members | 815 | 114 | One global namespace for vectors, colours, factories and the app. It is split into modules (§4.3). |
| `Qt.*` enum keys | 475 | 91 | `Qt.AlignLeft` and the like are typed enums of the type that uses them. |
| `anchors.*` / `Layout.*` | 1776 / 963 | 229 / 92 | Both are used, and both stay. Attached properties keep their syntax. |
| `parent.*` reads | 1000 | 150 | Lookups by structure. They stay, typed by the parent's static type. |
| `PropertyChanges {` | 1249 | 88 | States are QML's best part: kept, with renamed members followed through. |
| `color: "…"` (string) | 507 | 177 | Colours as strings. In the API a colour literal is typed `color`, and a string goes only where a `color` is declared. |
| `==` / `!=` | 167 | 46 | Loose equality. It is an error in TS files (`ts-loose-equality`). |
| `null` / `undefined` | 86 / 17 | 29 / 12 | Absence is `undefined` only (`ts-no-null`). |
| `var` statements | 202 | 78 | Function scope. It is an error (`ts-no-var`). |
| `Connections {` | 90 | 49 | Connections kept, typed by the target's signals. |
| `Component.onCompleted` | 59 | 51 | Ordering-sensitive. It stays, but bindings and states are preferred (declarative.md). |
| `MouseArea {` / `TapHandler {` | 61 / 29 | 47 / 20 | Two input APIs. One `PointerArea` takes mouse, pen and touch. |
| `Rectangle {` | 365 | 184 | `color` means fill here and glyph colour on `Text`. In the API it is `fill`. |
| `createObject(` / `createQmlObject` | 19 / 9 | 6 / 5 | Objects from code. `createQmlObject` (QML as text) keeps its file (§6.3). `createObject` on a component is typed by it. |
| `.connect(` | 11 | 7 | Dynamic connection. It stays in compat only. |
| `this` / `arguments` | 2 / 2 | 2 / 2 | Rare enough to forbid. |

`Qt.<member>` by use: vector3d 374, quaternion 126, matrix4x4 58,
vector2d 51, binding 29, rgba 29, locale 27, resolvedUrl 18,
createComponent 15, darker 14, quit 9, createQmlObject 9, and a long tail.

**What QML gets right, and the API keeps:** declarative object trees,
bindings, ids, states and transitions, attached properties, inline
components, `required` properties, and signal handlers named `on<Signal>`.
Only names and types change: what a member means, which module it is in,
and what it holds.

## 3. Where each seam sits

```
 Qt QML file ─┐
              │  qml_compat (text → text, line-preserving, grouped)
              ▼                                  ▲ reads qt_types (the Qt table)
 Ljós file ──► qml_syntax ─► qml_front ─► ljos_compiler ─► ljos_ir ─► ljos_engine
              (oxc fork)    (names,      (checker for          (Ljós API only)
                             types)       Ljós files; tiers)
```

- **Compat pass (`qml_compat`).** It is a front-end pass over the text. It
  never sees the IR or the engine. It reads the Qt type table to know what
  every Qt name is, and the Ljós table to know what the API has (ADR 0004).
  Its output is text in the Ljós API, with every line where it was, so
  every diagnostic, breakpoint and source map of the written file still
  points at the right line.
- **Project (`ljos_compiler::Project`).** Holds both texts of a file: the
  one written and the one compiled. `Project::set_compat(true)` lowers every
  file by itself, then by group (§6.2), and replaces the text of each file
  that lowers. Reload goes through `Project::put`, which lowers again the
  file and the files of its directory that name it.
- **Checker (`ljos_compiler::compile::check`).** It runs only on a file that
  *imports* `Ljos.Ui` itself. A file the pass lowered is still compat code:
  its scripts are the JavaScript it was written in, and holding them to TS
  rules would report on code nobody wrote (§5).
- **IR and engine.** They know the Ljós API. In phase 1 the API's types
  are *faces* of today's natives (`MetaBuilder::face`): the same native,
  made, painted and taking input the same, at the same member indices,
  under new names. When a native is renamed itself, its face is removed,
  and `QtQuick` becomes a table the pass reads, not a module the engine
  registers (milestone M5).

The IR never carries a Qt name that the Ljós API renamed, so the runtime
cannot tell a lowered file from one written in the API. That is the
property the spike tests (§12).

## 4. The Ljós API

### 4.1 Modules

| Module | Holds | Replaces |
|---|---|---|
| `Ljos.Ui` | items, visuals, input areas, states, text, images | `QtQuick` |
| `Ljos.Layout` | rows, columns, grids, flows, stacks; `Layout.*` | `QtQuick.Layouts`, positioners |
| `Ljos.Controls` | the controls and their styles | `QtQuick.Controls` |
| `Ljos.Models` | typed list and tree models; delegates' typed `model` | `QtQml.Models`, `ListModel` |
| `Ljos.Core` | `Timer`, `Component`, `Connections`, `Binding`, and the cleaned globals (§4.3) | `QtQml`, the `Qt` namespace |
| `Ljos.Blocks` | functional blocks and shells (ADR 0003) | (Ljós's own, already built) |

DECISION: the module set is `Ljos.*`, one module per concern, named in the
singular. `Ljos.Ui` is the first and the only one phase 1 builds. It
imports `QtQml` for `Component`, `Timer` and the `Qt` namespace until
`Ljos.Core` exists.

### 4.2 Cleaned-up types

The rule for a rename is that a name says the one thing the member is, the
same across types. Each rename has a mechanical lowering, which the pass
does.

| Qt | Ljós | Why | Lowering |
|---|---|---|---|
| `Rectangle.color` | `Rectangle.fill` | `color` is the fill here, the glyphs on `Text` and the line on a border | the member, its handler (`onColorChanged` → `onFillChanged`), `PropertyChanges`, a value source's target (`… on color`), and any `id.color` with a known target |
| `MouseArea` | `PointerArea` | it takes pen and touch as well | the type name |
| `TapHandler`, `DragHandler`, … | members of `PointerArea` (milestone M3) | two input APIs for one job | per member, phase 2 |
| `property var x` | a typed property | untyped state | phase 1: `property unknown x`, which is TypeScript's word for a value that must be narrowed before use; a type where one is known (M4) |
| `Qt.AlignLeft`, string-typed enums | the type's own enum (`Text.Align.Left`) | one namespace for every enum | by the enum's owner in the Qt table |
| `color: "#fff"` | the same | a string is a `color` only where `color` is declared | none: the checker enforces it in Ljós files |
| positioners (`Row`, `Column`) | `Ljos.Layout` | one layout model | M2 |
| `Component.onCompleted` | kept | ordering is real | none |

The phase-1 slice (`crates/ljos_quick/src/ljos_ui.rs`):

- `Rectangle` with `fill`, a face of Qt Quick's `Rectangle`;
- `PointerArea`, a face of `MouseArea`;
- `Item`, `Text`, `State`, `PropertyChanges` and the font enums, exported
  as they are.

It is described in `crates/qt_types/src/ljos.txt`, so `ljos check` and the
lowering know it with no Qt installed.

### 4.3 Cleaned-up globals

The `Qt` object is split by what its members are:

| Qt | Ljós | Kind |
|---|---|---|
| `Qt.vector2d/3d/4d`, `quaternion`, `matrix4x4`, `point`, `size`, `rect` | value types' constructors: `vector3d(x, y, z)` in `Ljos.Core` | pure, tier 1 |
| `Qt.rgba`, `hsla`, `darker`, `lighter`, `tint` | `color.rgba(…)`, `c.darker(f)` | pure, tier 1 |
| `Qt.formatDateTime`, `locale` | `Ljos.Core` `Locale` | pure |
| `Qt.resolvedUrl` | `url("…")`, which is resolved against the file statically | compile time |
| `Qt.binding` | a `Binding` object, or a state | declarative |
| `Qt.createComponent`, `createQmlObject` | `Component` by type: `Loader { sourceComponent: Card }` | typed |
| `Qt.quit`, `exit`, `application`, `platform`, `styleHints`, `inputMethod`, `openUrlExternally` | `App` singleton in `Ljos.Core` | effects, typed |
| `Qt.callLater` | `later(() => …)` | scheduled, no polling |

Every replacement is an import of a TypeScript module or a QML singleton.
None is a global. A Ljós file sees only what it imports, plus the
ECMAScript built-ins the subset allows (`Math`, `JSON` through decoders,
`String`, `Number`, `Array` read-only methods, `Map`).

### 4.4 Typed properties and the shared type namespace

A property's type is one of:

- a QML basic type (`bool`, `int`, `real`, `string`, `color`, `url`,
  `point`, …);
- a component, from its file or an inline component;
- a native type, from the engine's `MetaObject` table, which is what
  `ljostypes.json` describes;
- a list of one of these;
- a type exported from a TypeScript module the file imports.

```qml
import Ljos.Ui
import "filter.ts" as Logic

Item {
    property Logic.Filter working: Logic.none   // a TS type, qualified as an import
    property list<Logic.Row> rows: []
    signal chosen(row: Logic.Row)
    function choose(row: Logic.Row): void { working = Logic.only(row.kind); chosen(row) }
}
```

`filter.ts` is a plain TypeScript module:
`export type Filter = { readonly kind: "none" } | { readonly kind: "only"; readonly tag: Tag }`.

The other direction is generated. Each QML component and native type has a
`.d.ts` declaration, so a TS module can take `Card` or `ListView` as a
parameter type. The declarations are generated from the same table
`ljos check` reads (§10), so `tsc` and an editor see what the checker sees.

`var` is not a type in a Ljós file. A value of unknown shape enters at a
boundary through a declared decoder (note 11 §3.7). The decoder's result is
the only cast the checker allows.

### 4.5 Scripts

The subset is note 11 §7 (H1) without change. The rules the checker must
enforce by M4 are listed in §5. Scripts are parsed by the oxc fork with
`SourceType::ts()` (`crates/ljos_compiler/src/js.rs`). Types are erased
before tier 2 runs them, so TS costs nothing at run time.

## 5. The checker

### 5.1 Rules

Each rule is an error with a code that `ljos check` and the MCP tools
print. Phase 1 builds the first five, marked ✓.

| Code | Rule |
|---|---|
| ✓ `ts-assign-type` | A value goes only where its type does. A string goes to a `color`. |
| ✓ `ts-no-null` | No `null`. Absence is `undefined`, written `T \| undefined` or `x?: T`. |
| ✓ `ts-loose-equality` | No `==` / `!=`. |
| ✓ `ts-condition-boolean` | `if`, `while`, `for`, `?:` and `!` take a boolean, with no truthiness. |
| ✓ `ts-no-var` | No `var`. |
| `ts-no-any` | No `any`, written or inferred. A parameter without a type is an error. |
| `ts-no-cast` | No `as` except to a declared decoder's result. No `!`. |
| `ts-switch-exhaustive` | A `switch` on a union covers every case. |
| `ts-readonly` | Records and arrays are readonly unless declared mutable. Writing a QML property or a model is allowed (Q6, §14). |
| `ts-index-undefined` | Indexing gives `T \| undefined` (`noUncheckedIndexedAccess`). |
| `ts-optional-exact` | An optional member is not set to `undefined` explicitly (`exactOptionalPropertyTypes`). |
| `ts-variance` | Method parameters are contravariant, and mutable arrays are invariant. |
| `ts-forbidden` | No `class`, `this` outside QML's scope object, prototypes, `delete`, `for…in`, `with`, `eval`, or `arguments`. |
| `ts-boundary` | A compat `.js` module is called only through a `.d.ts` declaration (Q7, §14). |

### 5.2 How it says them

The house style is that of `ljos check`. A message names the thing in the
file's terms, says what is wrong with it, and when there is one obvious fix,
says it. The rule's code comes last, in brackets. It is one line, with no
"error:" jargon beyond the severity:

```
Gauge.qml:4:38: error: `level` is a real; this gives it a string [ts-assign-type]
Gauge.qml:4:32: error: `==` converts what it compares; write `===` [ts-loose-equality]
Gauge.qml:5:51: error: `if` needs a boolean, and `level` is a number: compare it [ts-condition-boolean]
Gauge.qml:6:34: error: `null` is not a value here: a value that may be missing is optional (`?`) [ts-no-null]
```

Columns count characters of the line as written (UTF-8 aware). Both the
features and the MCP tool give the column a human clicks, never a byte
offset.

### 5.3 What phase 1's checker knows

It knows a value's type from:

- a literal;
- a property it reads, by its declared type or its type's, as an
  unqualified name or `id.name`;
- an id;
- the result of `!`, a comparison or `typeof`.

What it cannot type, it lets by. "Unknown" is not an error until
`ts-no-any` (M4) makes every expression typed. Until then a rule holds of
what the checker can type, not of every script.

## 6. Compat: the lowering pass

### 6.1 Per file

`qml_compat::lower(text, locals)` parses the file and walks it. Each edit
is to a name of the Ljós API, and the text keeps every line. The edits are:

- an import, `QtQuick` → `Ljos.Ui`;
- a type, `MouseArea` → `PointerArea`;
- a member, its handler (`onColorChanged`), `PropertyChanges` and
  `Connections` on a known target, a value source's target (`… on color`),
  and `id.member` where the target's type is known;
- `property var` / `variant` → `property unknown`.

A `QtQml` import stays. Any other module import (`QtQuick.Controls`, an
aliased `QtQuick`) keeps the file.

A file is lowered *whole* or *kept*. The pass keeps the file as written,
and says why with a byte offset, when anything in it names what the API
has not taken yet. That includes a type, a member, a dynamic name (a string
built at run time and used as a member), or a script that reaches
something it cannot follow. A kept file is not an error: it runs as
written, on the same runtime.

`locals` tells the pass what the root type of a file beside it is, so
`Button { color: "blue" }` on a `Button.qml` whose root is a `Rectangle` is
lowered as a `Rectangle` member.

### 6.2 By group

Files that name one another are lowered together or kept together.
Otherwise a kept `Main.qml` would set `color` on a lowered `Button` that
now has `fill` only, which the sweep found in snippet-components-application
before grouping. `qml_compat::references` collects what a file may name:

- type names;
- property types;
- `.qml` string literals;
- directories imported by path;
- capitalised identifiers, such as an enum's owner.

`qml_compat::group` joins files with union-find. A group is lowered only
when every file in it lowered or had nothing to lower. Otherwise each file
that would have lowered is kept with the reason "it is used with
`Main.qml`, which is kept: …".

DECISION: grouping is conservative. A name that might be a sibling ties
the two, even when it is a coincidence. The cost of a false tie is a file
run as written, and the cost of a missed tie is a wrong program.

### 6.3 What keeps a file

| Kept because | Phase |
|---|---|
| A type or module the API has not taken (`Column`, `Behavior`, `QtQuick.Controls`, …) | Lowered as the API grows (M1, M6) |
| `Qt.createQmlObject` (QML as text) | Kept. The text is lowered only if the pass runs at that call too (M5) |
| A name built at run time (`obj[name]`, `"col" + "or"`) | Kept for good. A static analysis cannot follow it |
| A `.qml` URL built at run time (template literal, concatenation) | Kept when the file is seen. A group tie through it is missed (risk R1) |
| `property var` whose type is not known | Kept until M4 decoders |

### 6.4 Coverage today

`ljos migrate` over the corpus, with group lowering:

```
corpus/qtdoc/examples:  495 files, 23 lowered, 462 kept
corpus/qtdoc (all):     524 files, 29 lowered, 485 kept
```

Before grouping, 75 example files lowered. 52 of those set a Qt name on, or
were used by, a file that was kept. Coverage is low on purpose, because the
slice is three types. Kept files run as before, so low coverage costs
nothing.

Why the 462 example files are kept:

| Reason | Files |
|---|---:|
| a module import: `QtQuick.Controls` (102), `QtQuick3D` (89), `QtQuick.Controls.Basic` (62), `QtQuick.Layouts` (46), `QtQuick.Templates` (15), `QtQuick.Controls.Fusion` (13), `QtQuick.Particles` (12), `QtCore` (9), … | 364 |
| a file of its group is kept | 52 |
| a type not in `Ljos.Ui` yet: `Image` (10), `Screen` (5), `Behavior` (5), `ListView` (4), `Window` (3), … | 44 |
| a member read off something whose type is not known | 2 |

`Qt.*` keeps no file today: `Ljos.Ui` imports `QtQml`, so the namespace is
still there. Modules, not globals, decide coverage. Phase 2 starts with
them (§17).

## 7. Tiers and the web

- **Tier 0 (names).** Unchanged. The checker adds types to the names
  `qml_front` resolves.
- **Tier 1 (typed opcodes).** Typed declarations let the VM specialise
  opcodes: a `real` read needs no coercion and a `bool` condition needs no
  truthiness. The subset forbids what tier 1 cannot be exact about, so
  tier-1 coverage of Ljós files should approach every binding (exit
  criterion of M4).
- **Tier 2 (rquickjs).** Types are erased (oxc's TS transform) and the
  result is the JavaScript tier 2 runs today. No TS runtime exists.
- **AOT (note 13).** It compiles from the IR and its declared types. Typed
  Ljós files are the subset AOT wants, with no annotation added for it.
- **Web (ljos-web).** It uses ljos-ui's front end. It lowers compat files
  with the same pass, checks Ljós files with the same checker, and erases
  types to JS for the DOM. It needs no change to its renderer. It needs the
  pass as a library, `qml_compat` (pure Rust, no engine), so it builds to
  wasm.

## 8. `ljos migrate`

```
ljos migrate [--write] PATH…
would lower qml/SimpleButton.qml (5 edits)
kept qml/Main.qml:12:5: `Column` is not in Ljos.Ui yet
2 files, 1 lowered, 1 kept
```

- It runs the same pass and the same grouping as `--compat lower`. A
  migrated file is the text a lowered run compiled, byte for byte.
- Without `--write` it says what it would do. With `--write` it writes in
  place.
- Exit status 0 means every file was read, and 2 means a file could not be
  read or written.
- The edits are text replacements at known spans, which keeps every line
  and comment. M3 moves them onto `qml_edit`'s lossless CST edits (ADR
  0003 part 4), so Studio and migrate share one edit engine, and an edit
  that adds a line (a new import) is possible.

## 9. How Studio, MCP and ljos-web consume it

- **Studio** edits through `qml_edit`. It shows the API's names and the
  checker's diagnostics. A block contract (ADR 0003) declares its
  properties with Ljós types, so a property sheet knows that `fill` is a
  colour without guessing.
- **MCP tools.** Each capability is a tool first:
  - `check` already returns the checker's codes;
  - M3 adds `migrate` (lower, list kept, write) and `explain` (a code's
    rule, with the fix);
  - M4 adds `types` (the shared namespace, as `.d.ts`).
- **ljos-web** links `qml_compat` and the checker (§7). The same feature
  files run on its backend, so compat and the API have one meaning on every
  target.

## 10. Testing

- **Features first.**
  - `features/language/compat-lowering.feature` holds the same button run
    as written (`@qt-parity`, held to qml6) and lowered (`@ljos-only`),
    with the same states, handlers, counts and pixels.
  - `features/tooling/check-ljos-api.feature` holds the checker's rules
    and messages.
  - Every new rename adds a scenario pair, and every new rule adds a
    scenario. The step `Given Qt QML is lowered to the Ljós API` runs any
    scenario lowered.
- **qml6 is the oracle for compat.** A lowered run must match qml6 where
  the written run does. The repaint count in the feature was 3 on qml6, not
  the 2 I first guessed. That is what the oracle is for.
- **Unit tests.**
  - `qml_compat` checks each edit, keeping, grouping and the corpus files.
    The corpus is read at run time and skipped when absent, never copied.
  - `ljos_compiler/tests/compat.rs` checks a project lowered, kept by
    group, and lowered again on reload.
  - `ljos_quick::ljos_ui` checks that a face is the native under its names.
- **Corpus sweep.** Render every runnable example kept and lowered, and
  compare. Today all 15 examples with a lowered file give the same render
  JSON both ways. Their PNGs are identical except samegame, which differs
  from itself between two kept runs (random tiles).
- **API ledger.** `crates/qt_types/src/ljos.txt` is regenerated and
  committed with each API change (`own_types`).

## 11. Performance budget

| Where | Budget | Phase 1 |
|---|---|---|
| Run time, compat file | **0** vs today: same IR, same natives, same indices | A face shares the native's indices. A renamed member is linked by name once at compile time |
| Run time, Ljós file | ≤ compat, and less where types let tier 1 specialise | equal (no specialisation yet) |
| Load time, `--compat keep` (default) | **0** | the pass does not run |
| Load time, `--compat lower` | one parse and one walk per file, plus union-find over the project. Under 5 % of today's compile of the file | `ljos migrate corpus/qtdoc` (524 files, the same per-file pass, `references`, and `group`) takes 0.26 s in a **debug** build, about 0.5 ms per file. It is not a gate yet (M1) |
| Ahead-of-time build | 0 at run time: a shipped bundle holds the lowered IR | n/a |
| Memory | no second copy of the IR. The written text is kept only in a development project, for reload and migrate | as built |

Lookups changed (faces in `MetaObject`), so the allocation budget test
(`cargo test -p ljos_quick --test budget`) is part of every change here.

## 12. The spike

**Seam spiked:** a real corpus file is lowered at load onto a slice of the
API and runs identically. That is the riskiest seam, because everything
else (checker, migrate, web) builds on "a lowered file means the same".

**File:** `corpus/qtdoc/examples/demos/hangman/qml/SimpleButton.qml`,
copied into scratch by `docs/research/tools/dialect_spike.sh`, with a
`Main.qml` of two buttons, one in each state.

**What lowering did** (5 edits, the `ljos migrate` diff):

- `import QtQuick` → `import Ljos.Ui` (module);
- `MouseArea {` → `PointerArea {` (renamed type);
- `color: buttonColor` → `fill: buttonColor` on the root `Rectangle`
  (cleaned member);
- two `PropertyChanges { target: button; color: … }` → `fill: …` (the
  member followed into states).

The typed property is `property color buttonColor`. The checker types
`fill`, `buttonColor` and `state`. The TS handler is
`onPressed: button.state = "PRESSED"`, which passes `ts-assign-type`. A
copy with `button.state = 1` is reported.

**Result** (gate output in §12.1):

- Kept, lowered and migrated renders are identical PNGs.
- Each scores `content 0.946, pixels 0.9756` against qml6. The differences
  are font raster only, and identical for all three.
- `ljos check` on the migrated file: 0 errors.
- The mutated copy: `` `state` is a string; this gives it a number
  [ts-assign-type] ``.
- The feature pair in §10 shows the same states, handler counts and pixels
  for a press and a release.

### 12.1 Gate output

`docs/research/tools/dialect_spike.sh target/debug/ljos target/debug/qml-compare $WORK`,
run on 2026-11-04 (Qt 6.11.2's qml6, offscreen):

```
== ljos migrate (what lowering changes)
would lower $WORK/button/Main.qml (1 edits)
would lower $WORK/button/SimpleButton.qml (5 edits)
2 files, 2 lowered, 0 kept
--- $WORK/button/SimpleButton.qml
+++ $WORK/migrated/SimpleButton.qml
@@ -1,7 +1,7 @@
 // Copyright (C) 2021 The Qt Company Ltd.
 // SPDX-License-Identifier: LicenseRef-Qt-Commercial OR BSD-3-Clause
 
-import QtQuick
+import Ljos.Ui
 
 Item {
     id: button
@@ -19,7 +19,7 @@
         id: buttonRect
         anchors.fill: parent
         radius: 10
-        color: buttonColor
+        fill: buttonColor
         visible: button.available
         Text {
             id: buttonText
@@ -40,7 +40,7 @@
         }
     }
 
-    MouseArea {
+    PointerArea {
         anchors.fill: parent
         onClicked: {
             button.clicked();
@@ -58,7 +58,7 @@
             name: "NORMAL"
             PropertyChanges {
                 target: buttonRect
-                color: button.buttonColor
+                fill: button.buttonColor
                 border.color: "transparent"
             }
             PropertyChanges {
@@ -70,7 +70,7 @@
             name: "PRESSED"
             PropertyChanges {
                 target: buttonRect
-                color: "transparent"
+                fill: "transparent"
                 border.color: button.buttonColor
             }
             PropertyChanges {
== renders
{"compileErrors":{},"compiled":2,"entryCompiles":true,"error":null,"files":2,"said":[]}
{"compileErrors":{},"compiled":2,"entryCompiles":true,"error":null,"files":2,"said":[]}
{"compileErrors":{},"compiled":2,"entryCompiles":true,"error":null,"files":2,"said":[]}
keep and lower: identical PNG
keep and migrated: identical PNG
== the checker on the migrated file
2 files, 0 errors, 0 warnings
SimpleButton.qml:49:28: error: `state` is a string; this gives it a number [ts-assign-type]
1 files, 1 errors, 0 warnings
ljos: 1 errors
== against qml6
keep vs qml6: {"content":0.946031746031746,"pixels":0.9756018518518519}
lower vs qml6: {"content":0.946031746031746,"pixels":0.9756018518518519}
migrated vs qml6: {"content":0.946031746031746,"pixels":0.9756018518518519}
```

The feature gate (`LJOS_QT_TYPES=none mise run features:changed
features/language/compat-lowering.feature features/tooling/check-ljos-api.feature`)
ran 7 scenarios and 39 steps, all passed. The `@qt-parity` scenario passes
on the Qt backend (10 steps).

## 13. Milestones

| M | Builds | Exit criteria |
|---|---|---|
| **M1** Ui and Layout | `ljos run --compat`; `Ljos.Layout` (Row/Column/Grid/Flow and `QtQuick.Layouts`); `Ljos.Ui` items the corpus keeps for (`Image`, `Behavior` and animations, `ListView`, `Flickable`, `Repeater`, `Loader`, `Window`, `Screen`); a load-time budget gate | the features pair for each rename; compat sweep identical; ≥ 25 % of corpus example files lowered |
| **M2** Core | `Ljos.Core` (`Timer`, `Component`, `Connections`, value-type constructors, `App`, `later`), with `Qt.*` lowered to it and `Ljos.Ui`'s `QtQml` import dropped | sweep identical; no lowered file reaches the `Qt` object |
| **M3** Tools | migrate on `qml_edit`; MCP `migrate`, `explain`; Studio shows the API; enum cleanup (`Qt.Align*`) | migrate round-trip on the corpus re-lowers to itself; Studio edits a Ljós file losslessly |
| **M4** Types | TS modules imported by QML; `property T` of TS types; generated `.d.ts`; decoders; every rule of §5.1; tier-1 typed opcodes | a Ljós file passes `tsc --noEmit` with the generated `.d.ts` iff the checker accepts its scripts' types; tier-1 coverage of the features' Ljós files 100 % |
| **M5** Natives renamed | natives take the API's names; faces removed; `QtQuick` becomes a table only the pass reads; `createQmlObject` text lowered at the call | no Qt name in `ljos_engine`/`ljos_quick` outside the table; budget test equal or better |
| **M6** Controls | `Ljos.Controls` and styles | Controls corpus lowered; qml6 parity on its features |

## 14. Answers to note 11's open questions

**Q5: how QML declarations name script types.** There is one shared
namespace, made from three sources: QML components, native types (the
engine's `MetaObject` table, which `ljostypes.json` describes), and types
exported by TS modules a file imports. A TS type is named in QML qualified
by its import (`property Logic.Filter working`), as an import's
components are today. QML and native types are named in TS through
generated `.d.ts` files. It reaches `ljostypes.json`, because a native
type's declaration is generated from it. DECISION: no unqualified TS types
in QML, so a name's source is always visible.

**Q6: readonly scope.** I take the note's proposal. Values (records,
arrays) are readonly by default. Writes to QML properties and models stay
imperative, because they are the engine's state, and bindings already make
them reactive. Making properties readonly too would change every handler
and fight QML's model. The declarative lints (`decl-*`) push toward
bindings and states without a rule.

**Q7: the compat-JS ↔ TS boundary.**

- A Ljós file may call a compat `.js` module only through a `.d.ts`
  declaration, which may be a decoder (`ts-boundary`).
- A compat file may call Ljós TS freely. It is erased to JS and has no
  runtime checks to break.
- At run time both are the same tier 2 (TS is erased), so the boundary is
  checked statically only and costs nothing.
- A lowered file is compat code, with no checker. A file becomes Ljós code
  when a person migrates it and its import says `Ljos.Ui`.

## 15. Decisions

- DECISION: the module set is `Ljos.*`, one module per concern
  (`Ljos.Ui`, `Ljos.Layout`, `Ljos.Controls`, `Ljos.Models`, `Ljos.Core`,
  `Ljos.Blocks`).
- DECISION: in phase 1 a renamed type is a *face* of today's native
  (`MetaBuilder::face` + `rename`). It has the same native and indices, so
  a renamed member costs nothing at run time. M5 renames the natives
  themselves.
- DECISION: the checker runs only on files that import `Ljos.Ui`
  themselves. Lowered files are compat code and are not checked.
- DECISION: a file is lowered all-or-nothing, and so is a group of files
  that name one another. Grouping is conservative.
- DECISION: `Qt.createQmlObject` keeps its file until M5.
- DECISION: phase 1 exposes compat on `ljos render --compat keep|lower` and
  `ljos migrate`. `ljos run --compat` is M1. The default is `keep`.
- DECISION: no Qt corpus file is copied into the repository. Tests read the
  corpus at run time and skip when it is absent. Features use fixtures
  written for them, and the spike copies into scratch.
- DECISION: qml6's observed behaviour (three repaints for a click) is the
  oracle, over a guess.
- DECISION: TS types are named in QML qualified by their import, and QML
  and native types reach TS as generated `.d.ts`.
- DECISION: properties and models stay writable in handlers, and only
  values are readonly (Q6).
- DECISION: compat JS is called from TS only through declarations (Q7).
- DECISION: phase 2 builds modules before globals: `Ljos.Layout` and
  the kept-for `Ljos.Ui` types (M1) before `Ljos.Core` (M2). Module imports
  keep 364 of 462 kept example files, and `Qt.*` keeps none.
- DECISION: the edits are text replacements at spans that keep every line,
  and they move onto `qml_edit` in M3.

## 16. Risks

- **R1: dynamic addresses.** A `.qml` URL built at run time (a template
  literal or concatenation) does not tie its target into the group, so a
  kept file may load a lowered one. Fix by keeping a file that builds a
  URL, or by lowering at the load call (M5).
- **R2: where a kept group says why.** A file kept for its group points at
  byte 0, not at the line that kept it. The message names the file.
- **R3: columns.** Renames change line length (`color` → `fill`). A column
  of a lowered file maps back to the written text only by line, so a
  column after a rename on the same line is off by the difference. A span
  map is planned in M3.
- **R4: printed names.** `String(obj)` and `toString()` print the class
  name. A face prints the native's name, which equals Qt's, so it is
  harmless today. It must stay so after M5.
- **R5: checker soundness.** Unknown types are let by until `ts-no-any`.
  H1's soundness rests on the rule list being complete.
- **R6: reload cost.** `Project::put` lowers the file, its same-directory
  namers and settles the whole project, which is O(files) per put. Fine for
  development and visible on very large projects.
- **R7: names held as strings.** `obj[name]` keeps its file.
  `properties: "color"` in an animation is a member name held as a string.
  Today it occurs only in types the API has not taken, so its file is kept.
  When animations join the API, the pass must rename such strings too.
- **R8: RectangleBorder** is described twice (Qt's and the face's table
  entries) until M5.
- **R9: nondeterministic corpus examples** (samegame) cannot be compared
  by pixels. Compare their JSON and logs.
- **R10: low coverage until `Ljos.Layout` and `Ljos.Controls`.** The
  pass's real complexity shows only when Controls lowers.
- **R11: a Ljós file using a kept Qt sibling.** It sees Qt's names on it.
  The checker types it by Qt's table. That works, but the file mixes two
  APIs until the sibling migrates.

## 17. What phase 2 builds first

M1, in this order, from §6.4's evidence:

1. `ljos run --compat`, so a lowered app can be driven, not only rendered;
2. `Ljos.Layout`. `QtQuick.Layouts` keeps 46 example files, and `Row` and
   `Column` keep more by group;
3. the `Ljos.Ui` types the corpus keeps for: `Image`, `Behavior` and the
   animations, `ListView`, `Window` and `Screen`;
4. a load-time budget gate for `--compat lower`.

`Ljos.Controls` (192 files kept by a Controls import) is the largest single
gain. It is M6 because it sits on Layout, the Ui types and styles. If the
lead wants coverage sooner, it moves up before M3.

Each comes with its feature pair (as written and `@qt-parity`, then
lowered) and the corpus sweep.
