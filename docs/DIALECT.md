# Ljós's own API, typed scripts, and Qt's QML as compat

- **Status:** phase 2: being built, 2026-11-07. Phase 1's spike and design
  are on main. What of each milestone (§13) is on main:
  - **M1:** `Ljos.Ui`'s one arrangement (`Row`, `Column`, `Box`), with Qt's
    `Row`, `Column`, `RowLayout` and `ColumnLayout` lowered to it; the
    modules and directories of the project lowered with the files that
    import them (§6.2); `ljos render --compat` and `ljos migrate`.
  - **M4:** TypeScript modules imported by QML, `property T` of their types,
    `tsc` given the modules, and the checker's rules but `ts-boundary`
    (§4.4, §5).
  - **M2, M3, M5, M6:** not started.
- **Ergonomics amendment (2026-11-12):** `match`, the pipe `|>`, `fetch`
  and `Task`, Controllers, and no `Qt` in a file of Ljós's API (§4.3.1,
  §4.17 to §4.21; M7's slices S1 to S7 in §13). Designed, not built.
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
   text edit that is all-or-nothing per file, and a kept file keeps the
   files it uses. A file that cannot be lowered runs as written, on the
   same runtime. It must cost nothing at run time (§11).
3. **Typed scripts.** Handlers, functions and bindings in a Ljós file are
   strict TypeScript (note 11's H1), with two additions that compile away,
   `match` and the pipe (§4.17.1, §4.18). Every program the checker
   accepts is, once they are compiled away, valid TypeScript with the same
   meaning, so `tsc`, editors and coding models keep working on it
   (§4.21).
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
8. **A pleasure to write.** What an app says often has a short form:
   "which of these is it" is `match`, "then do this to it" is a pipe,
   "load this and show where it got to" is a `Task`, "this item also does
   that" is a Controller (§4.17 to §4.20).

### Non-goals

- **No syntax beyond TC39's.** The non-goal "no new syntax" is reversed
  (2026-11-12): a Ljós script has `match` and the pipe `|>`, as TC39's
  proposals define them, and nothing of its own. Note 11 rejected its H2
  because extensions break `tsc`, editors and coding models. Both are
  compiled away before any of those read a script, in one desugar that
  every consumer calls. The syntax is in QML files' scripts and in `.lts`
  modules, and a `.ts` module stays plain TypeScript (§4.21).
  There is still no `option` and no extension to QML's own syntax.
- **No second script language.** There is no ReScript-like front end (note
  11's B). JavaScript stays, but only as compat.
- **No formal soundness.** H1 closes TypeScript's known holes by rule and
  forbids `any`, casts, `!` and `null`. It is not a proof (§5, risks).
- **Qt's QML is not deprecated.** Compat is a first-class mode: Qt's own
  corpus runs in it, and qml6 stays the oracle for it.
- **Not everything at once.** The API grows type by type. A Qt file keeps
  using whatever the API has not taken yet, so the lowering covers what
  exists, never more.

### TC39 proposals considered

A Ljós script takes TC39's syntax and nothing of its own, so a proposal
that would give an app a short form is weighed here before Ljós invents
one. Statuses were read from the proposals' repositories on 2026-11-13.

| Proposal | Status | Ljós's position |
|---|---|---|
| [Records & Tuples](https://github.com/tc39/proposal-record-tuple) | Withdrawn (TC39, 2025-04-14). [Composites](https://github.com/tc39/proposal-composites), which took its place, reached stage 2 on 2026-09-30. | Not taken. Ljós waits for Composites to reach stage 3 before it adopts value-equal objects, and adds no form of its own meanwhile. |
| [Explicit Resource Management](https://github.com/tc39/proposal-explicit-resource-management) (`using`) | Stage 4 (TC39, 2026-05-19). | Adopted for a resource whose lifetime is a block: `using`, `Symbol.dispose` and `DisposableStack`. A Controller's `attached` and `detached` (§4.20) stay for a lifetime tied to an object. The checker types the disposables, `DisposableStack` and `AsyncDisposableStack`, and holds what a `using` declaration is given to be disposable, `null` or `undefined` (§4.22). Only tier 2 (QuickJS) runs them: tier 1 and AOT refuse a `using` declaration, as they refuse `try`, so a binding that uses one stays in tier 2. |
| [Decorators](https://github.com/tc39/proposal-decorators) | Stage 3. | Not taken. Decorators apply to classes and their members, and a Ljós script has no classes (§5.1). |

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

Qt's surface is where this section starts, not what it keeps. Every part
below asks what an app is trying to say, counts how often the corpus says
it (536 QML files under `corpus/`, counted by a
scratch script that is not committed), and gives the one way Ljós says it.

Five rules hold throughout:

1. **Every Qt construct lowers mechanically, or has a written DECISION**
   (§4.14). Compat never goes away, so a construct can wait for a later
   milestone, but it is never lost.
2. **Declarative first.** What an app states (a binding, a state, a keyed
   list) is something Studio can edit losslessly, ljos-web can render
   without a JS engine, and the AOT tiers can compile. An imperative escape
   stays, typed.
3. **The API costs less at run time, never more.** Each part says what its
   run-time cost is next to Qt's.
4. **General purpose.** Nothing below is for one app.
5. **The type is the documentation.** Enums are their type's, unions are
   strings the checker knows, and a misspelt name is a check error, not an
   `undefined` at run time.

Five corpus apps of different kinds are rewritten by hand in
a scratch directory (corpus files are never committed). Excerpts
are quoted side by side below:

| App | Kind | What it exercises |
|---|---|---|
| `clocks` | a grid of live views | typed static rows, the `Clock` global, inline components |
| `calqlatr` | input-heavy, responsive | one turning `Box`, keys as data, typed states, rules in TS |
| `photosurface` | gestures and media | `Pinch`/`Drag`/`Wheel`/`Hover` as children, a folder model |
| `samegame` | a game, objects made at run time | declarative creation, keyed exit transitions, `Component<T>` |
| `rssnews` | network master/detail | decoders, `currentIndex` as the selection, `Instant` |

### 4.1 Modules

| Module | Holds | Replaces |
|---|---|---|
| `Ljos.Ui` | items, visuals, gestures, states, text, images; the views (`Repeater`, `ListView`) | `QtQuick` |
| `Ljos.Layout` | `Box`, `Row`, `Column`, `Grid`, `Flow`, `Pages`; `Layout.*` | `QtQuick.Layouts`, positioners |
| `Ljos.Controls` | the controls in Ljós's own style, drawn with the roles of its `Theme`; `FileDialog`, `FolderDialog` (§4.16) | — |
| `Ljos.Controls.Basic` | the same controls as Qt's Basic style draws them; `FileDialog`, `FolderDialog` (§4.16) | `QtQuick.Controls`, `QtQuick.Controls.Basic`, `QtQuick.Dialogs` |
| `Ljos.Models` | `ListModel`, `ListElement`, `Instantiator`, `XmlModel`, `FolderModel`, `TreeModel`; keyed views | `QtQml.Models`, `XmlListModel`, `FolderListModel` |
| `Ljos.Core` | `Timer`, `Component`, `Connections`, `Binding`, `Clock`, `Instant`, `Locale`, `App`, `Random`, value types (§4.3) | `QtQml`, the `Qt` namespace |
| `Ljos.Design` | `Theme` roles: colour, space, type, radius, motion (§4.12) | `palette`, `Material.*`, hand-rolled `Style` singletons |
| `Ljos.Platform` | `StandardPaths`, notifications | `Qt.labs.platform`, `QtCore.StandardPaths` |
| `Ljos.Blocks` | functional blocks and shells (ADR 0003) | (Ljós's own, already built) |

A program's own modules, the types it registers from Rust, are neither
Qt's nor Ljós's. A file imports them as it did, in Qt's API and in Ljós's
alike (§4.4, §6.1).

DECISION: the module set is `Ljos.*`, one module per concern, named in the
singular. `Ljos.Ui` was the first, and phase 1 built only it.

`Ljos.Core` (M2) holds what a file says that is not an item: `QtObject`,
`Timer`, `Component`, `Connections`, `Binding` and `Locale`'s enums.
These are `QtQml`'s types, exported from it too, so a lowered file makes
the same objects. A file of Ljós's API imports it beside `Ljos.Ui`, and
`Ljos.Ui` no longer imports `QtQml`: a file that names `Timer` without
importing `Ljos.Core` is told `` `Timer` is not a type of anything the
file imports ``. The models are `Ljos.Models`' (§4.8), which a lowered
file imports for them; `Ljos.Core` still brings `QtQml.Models`. `Qt` is a global of Qt's QML
only: a file that imports `Ljos.Ui` has none (§4.3.1, S1).

DECISION: the types keep their names. `Object` for `QtObject` would hide
JavaScript's `Object` in a script; the rename stays open (§18). The
value-type constructors (`point(x, y)`) come with the lowering of
`Qt.point` and the others, since a constructor and the global it replaces
must say the same thing.

DECISION: the views are `Ljos.Ui`'s, as they are `QtQuick`'s, and the
models are `Ljos.Models`'. A view's keys and its `enter`/`exit` are the
view's (§4.15), so `Ljos.Models` holds no view. `Ljos.Models` is
registered by the engine, so a file of models needs no items.

### 4.2 Names: the rename rule and phase 1's slice

The rule for a rename is that a name says the one thing the member is, the
same across types. Each rename has a mechanical lowering, which the pass
does.

| Qt | Ljós | Why | Lowering |
|---|---|---|---|
| `Rectangle.color` | `Rectangle.fill` | `color` is the fill here, the glyphs on `Text` and the line on a border | the member, its handler (`onColorChanged` → `onFillChanged`), `PropertyChanges`, a value source's target (`… on color`), and any `id.color` with a known target |
| `MouseArea` | `PointerArea` (phase 1), then gesture children (§4.7) | it takes pen and touch as well | the type name; §4.7 |
| `property var x` | a typed property | untyped state | phase 1: `property unknown x`; a type where one is known (§4.4) |
| `Qt.AlignLeft`, string-typed enums | the type's own enum (`Text.Align.Left`), and one shared `Align` | one namespace for every enum | by the enum's owner in the Qt table |
| `color: "#fff"` | the same, or a theme role (§4.12) | a string is a `color` only where `color` is declared | none: the checker enforces it in Ljós files |
| `Component.onCompleted` | kept | ordering is real | none |

The phase-1 slice (`crates/ljos_quick/src/ljos_ui.rs`):

- `Rectangle` with `fill`, a face of Qt Quick's `Rectangle`;
- `PointerArea`, a face of `MouseArea`;
- `Tap` and `Hover`, faces of `TapHandler` and `HoverHandler` (§4.7), and
  `PointerDevice` as it is;
- `Item`, `Text`, `State`, `PropertyChanges` and the font enums, exported
  as they are;
- `Repeater` and `ListView` with its attached `ListView.*`, exported as
  they are (§4.8);
- Qt's shapes, exported as they are (§4.14): `Shape`, `ShapePath`, the
  elements of a `Path` (`PathSvg`, `PathLine` and the rest), and the
  gradients that fill a path (`LinearGradient`, `RadialGradient`,
  `ConicalGradient`) with `Gradient` and `GradientStop`;
- in `Ljos.Models`, `ListModel`, `ListElement` and `Instantiator`, exported
  as they are;
- what says in Qt Quick the one thing it says in Ljós (§4.14, "none"),
  exported as it is: `Image`, `Flickable`, `Window` and `Screen` (with
  `Screen.points`, §4.3), `TextInput` and its validators, `TextEdit`,
  `TextMetrics`, `FontMetrics`, `Shortcut`, `DropArea`, and the attached
  `Keys` (with its `KeyEvent`) and `Accessible`. DECISION: a `Window`
  keeps `color`: it is what is behind the window's items, not a fill, and
  Qt's `Window` is no `Rectangle`. `Window` and `Screen` come from
  `ljos_controls`, which builds them, into the same module.

DECISION: `Gradient` and `GradientStop` come with the shapes. A shape's
gradient is a `Gradient`, and its stops are `GradientStop`s, so a file
can't fill a path with one without them. A `Rectangle`'s `gradient` then
works in Ljós's API too.

DECISION: `ListView.view` (and `GridView.view`) is a `Flickable` to the
checker. One attached type serves every view and their common base has
no name a file can say, so a typed file reads `ListView.view.width`
directly and a member of `ListView` alone through a typed property.

It is described in `crates/qt_types/src/ljos.txt`, so `ljos check` and the
lowering know it with no Qt installed.

### 4.3 Globals

The `Qt` object (815 uses in 114 files) is one namespace for vectors,
colours, factories, time and the app. It is split by what its members are,
and nothing is global: a Ljós file sees only what it imports, plus the
ECMAScript built-ins the subset allows (`Math`, `JSON` through decoders,
`String`, `Number`, `Array` read-only methods, `Map`). `JSON.parse(text)`
gives `unknown`, for a decoder to narrow (§4.4), and `JSON.stringify(value)`
a string; neither takes a reviver, a replacer or an indent, which are
`null` and functions of `any` in TypeScript's own declarations.

| Qt | Ljós | Kind | Cost |
|---|---|---|---|
| `Qt.vector2d/3d/4d`, `quaternion`, `matrix4x4`, `point`, `size`, `rect` | constructors: `vector3d(x, y, z)` in `Ljos.Core` | pure, tier 1 | a value, no host call |
| `Qt.rgba`, `hsla`, `hsva`, `darker`, `lighter`, `alpha`, `tint` | `rgba(…)`, `hsla(…)`, `hsva(…)` in `Ljos.Core`; `c.darker(f)`, `c.lighter(f)`, `c.alpha(a)`, `c.tint(t)` on a colour | pure, tier 1 | folded at compile time when constant |
| `new Date()`, a `Timer` that reads it | `Clock.now(Clock.Resolution.Second)`: a reactive `Instant` | a source | one wake-up per resolution tick for the whole app while anything reads it; a reader whose only effect is drawing is skipped while hidden, and any other reader (a handler, a `when:`, a non-visual object) keeps it ticking |
| `Date` values, `Date.parse` | `Instant` (UTC ms), `Instant.parseRfc2822`, `Instant.parseIso`, `i.since(j)` (a `Duration`) | pure | a number |
| `Qt.formatDateTime`, `Qt.locale()`, `toLocaleString` | `Locale.current.formatDate(i, Locale.Format.Short)` | pure, reactive on the locale | |
| `Qt.resolvedUrl` | `url("…")`, resolved against the file statically | compile time | none |
| `font.pointSize` | `Screen.points(pt)`: pixels for `pt` points on the item's screen (the screen's logical DPI, exact, over 72: a host may report a fractional DPI, as Qt allows); with no screen yet, the platform's own points (72 or 96 to the inch). Built with `Screen` (M1) | a source, reactive on the item's screen | a multiply; re-evaluated only when the item moves to another screen or that screen's DPI changes |
| `Qt.binding` | a `Binding` object, or a state | declarative | |
| `Qt.createComponent`, `createQmlObject` | a component type is a value (§4.11) | typed | |
| `Qt.quit`, `exit`, `application.*`, `openUrlExternally` | `App.quit()`, `App.arguments`, `App.displayName`, `App.openUrlExternally(u)` | effects, typed | |
| `Qt.platform`, `styleHints`, `inputMethod` | `App.platform`, `Theme.motion`, `App.inputMethod` | reactive | |
| `Math.random` for a range | `Random.between(lo, hi)`, `Random.pick(list)`; `Math.random` stays | effect | |
| `Qt.callLater` | `later(() => …)` | scheduled, no polling | |
| `StandardPaths` | `Ljos.Platform` | effects | |
| `FileDialog`, `FolderDialog` | `Ljos.Controls` (§4.16) | effects | |

`clocks` shows what a reactive clock is worth. Qt's version runs a 100 ms
`Timer` in each of seven delegates, 70 wake-ups a second, and each one
builds a `Date`:

```qml
// Qt (clocks/Clock.qml)                          // Ljós (Clock.qml)
function timeChanged() {                           readonly property Zones.HandAngles angles:
    var date = new Date;                               Zones.hands(Clock.now(Clock.Resolution.Second),
    hours = internationalTime                                      clock.modelData)
        ? date.getUTCHours() + Math.floor(clock.shift)
        : date.getHours()
    ...
}
Timer {
    interval: 100; running: true; repeat: true;
    onTriggered: clock.timeChanged()
}
```

The Ljós version wakes once a second for the whole app, with all seven
clocks reading one value. `hands` is a pure TypeScript function, so tier 1
can compile it.

Built in M2: `Ljos.Core` gives `later`, `point`, `size`, `rect`,
`vector2d`, `vector3d`, `vector4d`, `quaternion`, `rgba`, `hsla`, `hsva`,
`url` and `App` (`App.platform`, `App.openUrlExternally`, `App.quit`), and
the checker types them and a colour's methods
(`features/language/ljos-core-globals.feature`).

DECISION: `Ljos.Core`'s names are seen only by a file that imports it, and
last: an id, a property, a role or a local of the same name comes first, so
`property int size` is the property and `size(…)` is not in reach there.
They are not globals of the engine. The compiled file takes them from one
frozen object at its end, so no line moves; `url` is the file's own, since
it resolves against the file.

DECISION: the colour constructors are bare `rgba(…)`, not `color.rgba(…)`:
`color` is a property of most items, and would hide them.

DECISION: the lowering pass turns `Qt.point(…)` into `point(…)`,
`Qt.callLater` into `later`, `Qt.resolvedUrl` into `url`, and
`Qt.platform`, `Qt.openUrlExternally` and `Qt.quit` into `App`'s, and the
file then imports `Ljos.Core`. It does so only where the new name can be
nothing else. The pass cannot see every name in sight, so the new name must
be no other word of the file (outside a property's type), no member of a
type the file's objects are (Qt's or Ljós's), and nothing a file it is built
on declares, and the file has no object of a type the pass does not know
and no delegate, whose roles would come first. `App` is not free in a
file that imports a directory by path without a name, since that directory
may have an `App.qml`. A project lowers a file again when an `App.qml`
comes beside it or goes. `Qt.resolvedUrl(u, ctx)` stays Qt's: Qt 6
resolves it against `ctx`'s component, and `url` takes one argument.
Until S1, the member otherwise stays Qt's and `Qt` stays a global of every
file; after it, the file is kept instead (§4.3.1).

DECISION (until S1; §4.3.1 replaces it): `Qt.darker`, `lighter`, `alpha`
and `tint` stay Qt's in a lowered file. Their first argument may be a colour's name, a string, which has no
methods, and a lowered file's scripts are not typed, so `c.darker(f)`
cannot be written for it. `Qt.binding` stays too: what it becomes, a
`Binding` or a state, is not one edit.

DECISION: there is no `date` type in a Ljós file. An `Instant` is a number
of UTC milliseconds with methods, and a calendar date is formatted through
`Locale`. Qt's `date` lowers to `Instant` where it is only stored or
compared, and keeps its file where it uses `Date`'s local-time getters.

#### 4.3.1 No `Qt` in a file of Ljós's API

Built in S1 (§4.17). A file that imports `Ljos.Ui` has no `Qt`: not in
its scripts, not in its bindings, and not when it says `pragma Scripts:
JavaScript`. `Qt` is a global of Qt's QML only, that is of a file that
imports `QtQuick` or `QtQml`, and so of compat. Every member of `Qt` has
a home in Ljós's API, and `ljos migrate` writes it:

| `Qt` | Ljós | Where |
|---|---|---|
| `Qt.point`, `size`, `rect`, `vector2d/3d/4d`, `quaternion` | `point(…)` and the others (built) | `Ljos.Core` |
| `Qt.matrix4x4(…)` | `matrix4x4(…)` | `Ljos.Core` |
| `Qt.rgba`, `hsla`, `hsva` | `rgba(…)`, `hsla(…)`, `hsva(…)` (built) | `Ljos.Core` |
| `Qt.color(s)` | `Color.of(s)`: a colour from a colour or its name | `Ljos.Core` |
| `Qt.darker(c, f)`, `lighter`, `tint(c, t)`, `alpha(c, a)` | `Color.of(c).darker(f)` and the others; on a value already typed `color`, `c.darker(f)` | `Ljos.Core` |
| `Qt.colorEqual(a, b)` | `Color.of(a).equals(b)` | `Ljos.Core` |
| `Qt.red`, `Qt.transparent`, … (`GlobalColor`) | the colour's name, `"red"`, where it goes to a `color`; elsewhere the file is kept | — |
| `Qt.font({…})`, `Qt.fontFamilies()` | `Font.of({…})`, `Font.families()` | `Ljos.Ui` |
| `Qt.binding(f)` | `bind(f)` (§18) | `Ljos.Core` |
| `Qt.callLater(f, …)` | `later(f, …)` (built) | `Ljos.Core` |
| `Qt.resolvedUrl(u)` | `url(u)` (built; the two-argument form keeps its file) | `Ljos.Core` |
| `Qt.quit()`, `Qt.exit(code)` | `App.quit()` (built), `App.exit(code)` | `Ljos.Core` |
| `Qt.openUrlExternally(u)`, `Qt.platform` | `App.openUrlExternally(u)`, `App.platform` (built) | `Ljos.Core` |
| `Qt.application.name`, `version`, `organization`, `domain`, `displayName`, `arguments`, `state`, `layoutDirection`, `screens`, `onAboutToQuit` | `App.name` and the others; `App.onQuitting` | `Ljos.Core` |
| `Qt.uiLanguage` | `App.language`, reactive and writable | `Ljos.Core` |
| `Qt.locale(name?)` | `Locale.current`, `Locale.named(name)` | `Ljos.Core` |
| `Qt.formatDate/Time/DateTime(d, f)` | `Locale.current.formatDate(d, f)` and the others | `Ljos.Core` |
| `Qt.styleHints.*` | `App.hints.*`, the same members | `Ljos.Core` |
| `Qt.inputMethod.*` | `App.inputMethod.*` | `Ljos.Core` |
| `Qt.createComponent(u)` | a component type is a value (§4.11); until that is built, the file is kept | — |
| `Qt.createQmlObject`, `Qt.isQtObject`, `Qt.include` | compat only: the file is kept | — |
| `Qt.<Key>` of an enum of `Qt` | `<Enum>.<Key>` (below) | `Ljos.Ui` |

**Enums.** Qt's `Qt` namespace holds 70 enums whose keys are reached as
`Qt.<key>`, so a key never says what it belongs to. In Ljós's API each is
a type of its own, named for what it is, with its keys named without the
part they share with it:

| Qt | Ljós |
|---|---|
| `Qt.AlignLeft`, `Qt.AlignHCenter`, `Qt.AlignTop \| Qt.AlignRight` | `Align.Left`, `Align.HCenter`, `Align.Top \| Align.Right` |
| `Qt.Key_Return`, `Qt.Key_A`, `Qt.Key_0` | `Key.Return`, `Key.A`, `Key.Digit0` (§4.7) |
| `Qt.LeftButton`, `Qt.AllButtons` | `MouseButton.Left`, `MouseButton.All` |
| `Qt.ControlModifier`, `Qt.NoModifier` | `Modifier.Control`, `Modifier.None` (as §4.7 writes them) |
| `Qt.Horizontal` | `Orientation.Horizontal` |
| `Qt.ElideRight` | `Elide.Right` |
| `Qt.PointingHandCursor` | `Cursor.PointingHand` |
| `Qt.Checked` | `CheckState.Checked` |
| `Qt.RightToLeft` | `Direction.RightToLeft` |
| `Qt.Granted`, `Qt.PermissionStatus.Granted` | `Permission.Status.Granted` |

The full table is generated beside `ljos.txt` from `QtQml.Qt`'s enums,
with the short names above written by hand and the rest following the
rule. A key the rule would leave starting with a digit (`Key_0` would be
`Key.0`) or empty is written by hand too (`Key.Digit0`, as §4.7 has it),
and the generator refuses a table that has one. A key keeps Qt's number,
so a value passes between a Ljós file and a kept Qt file unchanged. The
table records which enums are sets of flags, combined with `|` (`Align`,
`Modifier`, `MouseButton`), and which are plain (`Permission.Status`,
`Orientation`, `CheckState`): a plain enum is a union of its keys to the
checker, and a `switch` or a `match` on it is held to exhaustiveness
(§5.1). `Qt.Align*` was M3's (§13); S1 takes it.

**What a file is told.** `Qt` in a file of Ljós's API is a check error
(`qt-global`) whose fix names the home: `` `Qt.callLater` is not in a
Ljós file: use `later` (Ljos.Core) ``. The compiled file is not handed
`Qt`, so a script that reaches it at run time finds nothing there.

**Lowering.** The pass lowers a file only when it can give every `Qt`
member of it a home, and the free-name rules of §4.3 still decide whether
a home's name can be written there. Where it cannot, the whole file stays
Qt's and says which member kept it (`` kept Main.qml:12:9: `Qt.binding`
needs `bind`, which a property of the file hides ``). So no lowered file
reaches `Qt`, which was M2's exit criterion (§13).

`QtObject` is renamed with the rest (§18 asks for the name). `Ljos.Ui` and
`Ljos.Core` say nothing named for Qt after S1, beside the types the API
has not taken yet, which keep their names until M5.

DECISION: no file that imports `Ljos.Ui` sees `Qt`, a lowered one
included, and the pass lowers a file only where every `Qt` member has a
home. This replaces §4.1's "`Qt` stays a global of every file".

DECISION: `Qt.darker`, `lighter`, `tint`, `alpha` and `color` lower to
`Color.of(…)`, which takes a colour or a colour's name, so they need no
type to lower; this replaces the decision above that kept them Qt's.

DECISION: the enums of `Qt` are types of `Ljos.Ui` named for what they
are (`Align`, `Key`, `MouseButton`, `Modifier`), with Qt's numbers.
`MouseButton`, not `Button`: the controls have a `Button`, and a file that
imports both would find the control when it reads `Button.Left`.

DECISION: the colour constructors stay bare `rgba(…)` (§4.3), not
`Color.rgba(…)`: they are built, shorter, and read as CSS does. `Color`
holds what takes a colour in (`Color.of`).

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

QML has no generic syntax for a property's type, so `ListModel<T>` and
`Component<T>` are never written in a QML file. The checker infers them:
`property Component block: Block` is a `Component<Block>`, and a
`ListModel`'s row type comes from its `ListElement`s. Interfaces and maps
are declared in a `.ts` module and named from QML.

What is built (`features/language/ts-modules.feature`):

- A file, in Ljós's API or in Qt's QML, imports a module ending in `.ts`
  as it imports a script, and a module imports another with
  `import { x } from "./other.ts"` or `import * as`. Its types are erased
  (`crates/ljos_compiler/src/script/module.rs`) and what is left runs at
  tier 2 as a library script does. Its exports are read-only: a named
  import is a copy taken when the module ran.
- The module is the erasable subset of TypeScript (`erasableSyntaxOnly`):
  no `enum`, `namespace` or parameter property, which would leave code
  behind, and none of `export default` or `export *`. A module is checked
  with the rules of §5, at its own lines, and counts as a file in
  `ljos check`. A file reaches a module through its imports, the
  modules they import, and the `.import` lines of the scripts it
  imports. A file that reaches a module with an error does not run:
  whatever makes an object of it, a hot reload among them, is told the
  module's first error after the module's path (`filter.ts:2:14: …`).
- `Logic.x` is typed by what the module exports. A name it does not
  export is `unknown-member`, and a type it does not export is
  `unknown-type`. Both name the module and say what it does export.
- A property, a `list<T>` property, a signal's parameter and a
  function's parameter take a type the module exports, written qualified
  by the import (`Logic.Row`). The run time holds such a property as a
  `var`: the type is the checker's, so it costs nothing once the file is
  checked.
- `ljos check --tsc` gives TypeScript the project's modules beside the
  generated ones, and each generated module imports the modules its file
  does. What TypeScript says of a module is said at the module's lines.
  The generated `.d.ts` of `ljos types` stays the API's: a module's types
  reach TypeScript through the module itself.
- A type predicate (`v is T`) and an assertion function (`asserts v is T`)
  are typed, not read as a boolean; what they narrow is below.

A module ending in `.lts` ("Ljós TypeScript") is the same in every way
but one: it may use `match`, `is` and `|>` (§4.17 to §4.18), which the
desugar compiles away before anything else reads it (§4.21). It is
imported as a `.ts` module is, from a QML file, a script, a `qmldir` or
another module, and it imports `.ts`, `.lts` and `.d.ts` files as they
do. Types cross both ways: a `.ts` module takes a type an `.lts` module
exports, and the other way round. A `.ts` module that uses the new syntax
is `ts-lts-syntax`, whose fix renames it to `.lts` (§4.21).

The compat pass reads the same description (§6.1). A native type's own
members keep their names in a lowered file, since the program registers
them under those names. Its singletons are typed by what they hold
(`Look.palette.accent.color`), and the members it has from the toolkit
type it is built on are that type's. A name Ljós renames there
(`Rectangle`'s `color`) keeps the file. The program's type is built on the
native type, not on the face Ljós's API has of it (§3), so it answers
only to Qt's name.

The other direction is generated. Each QML component and native type has a
`.d.ts` declaration, so a TS module can take `Card` or `ListView` as a
parameter type. The declarations come from the same table `ljos check`
reads (§10, `ljos types --dts`), so `tsc` and an editor see what the
checker sees. `ljos check --tsc` gives each file the declarations of the
modules of Ljós's API it imports, and nothing else: a file of `Ljos.Ui`
alone has no `Timer` or `Component.Ready`, in TypeScript as in the
checker. Files that import the same modules share one declarations file
and one run of `tsc`. A type such a file cannot name but is handed,
the `QtObject` a `PointerArea` gives a handler, is still declared,
because what the file imports reaches it, but under its class
(`QtQml_QtObject`) when another module of the API exports its name. A
type the file writes, `property QtObject o` in a file of `Ljos.Ui`
alone, must be one of its imports: the checker says
`` `QtObject` is not a type of anything the file imports `` and
TypeScript cannot find the name. A value type (`font`, `point`) may be
written by any file, and a dotted name is a namespace's or an enum's,
which this rule does not judge. A type of the project's own (a
component declared inline, a file beside it, a type of one of the
project's modules) is found as it is in Qt's QML; the declarations do
not describe it, so TypeScript is given `unknown` for it and the checker
types it. `ljos types --dts` declares
the whole API, `Ljos.Ui` and `Ljos.Core` together.

`var` is not a type in a Ljós file. A value of unknown shape enters at a
boundary as `unknown` and is decoded (note 11 §3.7). A decoder narrows;
nothing casts: `as T` stays an error everywhere (`ts-no-cast`), a decoder's
result included. A decoder is user code a module exports, written
without casts, in one of three forms: a type guard (`v is T`), an
assertion function (`asserts v is T`), or a plain typed function that
narrows what it is given (`unknown`, or a loosely shaped value such as
the `XmlNode` of the rssnews example below) and returns a typed value or
a fallback (`T | undefined`, or a default). The checker treats the first two specially,
narrowing what they are given; the third needs nothing of it, being an
ordinary typed function:

```ts
export interface Row { tag: string; count: number }
export function isRow(v: unknown): v is Row {
    return typeof v === "object" && v !== null && "tag" in v && typeof v.tag === "string"
        && "count" in v && typeof v.count === "number"
}
export function row(v: unknown): asserts v is Row { if (!isRow(v)) { throw new Error("not a row") } }
export function rowOr(v: unknown, fallback: Row): Row { return isRow(v) ? v : fallback }
```

What is built (`features/language/decoders.feature`, and the tsc oracle's
`D*` and `B*` cases, which hold the checker to what TypeScript 5 narrows):

- The doors. `JSON.parse(text)` gives `unknown` (TypeScript's lib says
  `any`), as do `property unknown` and a native property the type table
  declares `var`. A member read on `unknown` is `ts-no-any`, and says
  which narrowing would give it a type.
- Narrowing `unknown`, in a file's scripts and in modules alike:
  `typeof v === "object" && v !== null` gives `object`, in either order
  and after an early `return`; `v !== null` alone gives `{} | undefined`;
  `"tag" in v` on an object gives `v.tag: unknown`, which `typeof v.tag`
  narrows; `Array.isArray(v)` gives `unknown[]` (TypeScript's
  `any[]` would let `v[0].x` by). `instanceof` is not in the subset, as
  `class` is not (`ts-forbidden`).
- A type guard narrows in `if`, `&&`, `?:`, `!` and after an early
  `return`, and `.filter(isRow)` gives `readonly Row[]`. Its body returns
  a boolean, and it narrows only to what its parameter can be. An
  assertion function narrows what follows its call statement. As in
  TypeScript (TS2775), it narrows only when every name in the call is
  declared with a written type: a module's export or a `const` whose
  type is written, not a function the component declares, which is
  reached through QML's scope.
- A JavaScript file is reached through the declarations beside it
  (`ts-boundary`, §5.1).

It costs nothing at run time: the checker reads the narrowing, and the
decoder runs as the code it is. `XmlModel`'s generated decoder (M1, M5)
and the tier-1 opcodes the types allow (M4) are not built. `rssnews`
shows a decoder at the network boundary:

```qml
// Qt (rssnews/Main.qml)                                       // Ljós
XmlListModel {                                                 XmlModel {
    source: "https://" + window.currentFeed                        source: `https://${Feeds.feeds[categories.currentIndex]?.path ?? ""}`
    query: "/rss/channel/item"                                     query: "/rss/channel/item"
    XmlListModelRole { name: "title"; elementName: "title" }       decode: Feeds.article      // (item: XmlNode) => Article | undefined
    XmlListModelRole { name: "content"; elementName: "content";}
                       attributeName: "url" }
    ...
}
// NewsDelegate.qml                                            // NewsDelegate.qml
required property string title                                 required property Feeds.Article modelData
required property string content
required property string link
required property string pubDate
```

In Qt's version a role misspelt in either place is an `undefined` at run
time. In Ljós's it is a check error, and the delegate gets one typed value.

### 4.5 Scripts

The subset is note 11 §7 (H1) without change. The rules the checker must
enforce by M4 are listed in §5. Scripts are parsed by the oxc fork with
`SourceType::ts()` (`crates/ljos_compiler/src/js.rs`). Types are erased
before tier 2 runs them, so TS costs nothing at run time. Logic that is not
about the view goes in a `.ts` module and is a pure function where it can
be. `calqlatr`'s 158-line `calculator.js`, which keeps module globals,
becomes `calc.ts`:
`press(calc: Calc, key: Key): Calc`. `samegame`'s 542-line `samegame.js`
becomes `game.ts`: `click(game: Game, column, row): Move`. Both can be
tested with no UI.

### 4.6 Layout: one model

**What apps say.** "Put these next to each other", "this one takes the
rest", and "this sits on top of that". The corpus says the first two
through nine types, in two families that size children by different rules:

| | Uses | Files |
|---|---:|---:|
| `Row` / `Column` / `Grid` / `Flow` (positioners: never size a child) | 43 / 46 / 1 / 3 | 72 |
| `RowLayout` / `ColumnLayout` / `GridLayout` / `StackLayout` (size children) | 102 / 63 / 32 / 4 | ~120 |
| `Layout.fillWidth/Height` | 269 | 63 |
| `Layout.preferred*` | 286 | 47 |
| `anchors.fill` | 307 | 164 |
| anchor edges / margins | 641 / 488 | 106 / 104 |
| `width: parent.width…` | 167 | 63 |

The two families disagree on what a child asks for. Measured with qml6 as a
black box (a 20-line probe file, run with `QT_QPA_PLATFORM=offscreen qml6`):

```
RowLayout { width: 400; spacing: 0
    Rectangle { width: 50; implicitWidth: 80 }              -> 80   (the explicit width is ignored)
    Rectangle { implicitWidth: 80; Layout.fillWidth: true } -> 290
    Rectangle { width: 30 }                                 -> 30   (implicit 0: the explicit width is used) }
Row { Rectangle { width: 50; implicitWidth: 80 }            -> 50   (explicit wins; Layout.fillWidth is ignored) }
```

**The design.** One arrangement, the *Box*, with one size rule.

- `Box { direction: Box.Row | Box.Column }`. `Row` and `Column` are a Box
  with the direction fixed. `Grid` places in cells, `Flow` wraps, and
  `Pages` shows one child at a time (it replaces `StackLayout`, and
  `StackView` for the simple case).
- **The size rule, per axis:** `Layout.preferredX` if it is set, else an
  explicit `width`/`height` if the file sets one, else the implicit size.
  Then `fill`, `minimum` and `maximum` work as Qt's Layouts' do. A Box whose
  children are all fixed or implicit and none fills is a positioner, so the
  two families merge with no case lost.
- **Whole pixels.** A Box any of whose children has a `Layout` is a layout,
  and a size a child takes from its content (its implicit size, where its
  file gives it no width or height) is rounded up to a whole logical pixel,
  so no seam falls between two pixels: a `Text` of implicit width 67.73 is
  68 wide, and the next child is at 68, as in Qt's `RowLayout`. A size the
  author gives (`width`, `Layout.preferredWidth`) is never rounded. A Box
  none of whose children has a `Layout` is a positioner and places each
  child at the size it has, fraction and all, as Qt's `Row` does.
- **A Box in a Box** reports its children's least size as its own
  `Layout.minimumWidth`/`Height`: along its axis the sum, with spacing and
  padding, and across it the largest. A `Row` of children of least width
  60 and 30 with spacing 4 and padding 2 in a `Column` 50 wide is 98 wide,
  not 50.
- `spacing` and `padding` (with sides) are on the container. A child's
  `Layout.margins` are its own.
- **Anchors are for a free parent only:** an `Item`, a `Rectangle`, or any
  non-arrangement. Anchors on a child of a Box are a check error
  (`layout-anchored`: "`anchors.left` on a child of a `Row` is overridden
  by it; use `Layout.alignment` or move it out of the Row"). Qt only warns,
  at run time. Overlap (z-stacking, `anchors.fill: parent`, a badge on a
  corner) is what anchors are for, and they stay.
  Qt's `Row` and `Column` place a child along them only, so an anchor
  across one (`anchors.verticalCenter` in a `Row`) holds there; the pass
  keeps such a file and says so, because the lowered child is placed
  across by `Layout.alignment`, which rounds a centre where the anchor
  does not.
- `LayoutItemProxy` (one item shown in two layouts) is not needed: a Box's
  direction is a binding.

`calqlatr` and `rssnews` turn with the window's aspect. Qt's calqlatr has
two layouts and a proxy; Ljós's has one Box:

```qml
// Qt (calqlatr/Main.qml, abridged)                    // Ljós
ColumnLayout { visible: root.isPortraitMode            Box {
    LayoutItemProxy { target: calcDisplay                  anchors.fill: parent
        Layout.minimumHeight: calcDisplay.minHeight }      direction: width < height ? Box.Column : Box.Row
    LayoutItemProxy { target: numberPad                    Display { Layout.fillWidth: true; Layout.fillHeight: true
        Layout.alignment: Qt.AlignHCenter }                          Layout.minimumHeight: minHeight }
}                                                          NumberPad { Layout.alignment: Align.Center }
RowLayout { visible: !root.isPortraitMode              }
    LayoutItemProxy { target: calcDisplay }
    LayoutItemProxy { target: numberPad
        Layout.alignment: Qt.AlignVCenter }
}
```

`rssnews`'s Qt version swaps width, height, orientation and four anchors on
`isPortrait`, in four objects. Ljós's swaps one `direction`.

**Cost.** A Box with no fill child does what a positioner does: one pass
over its children, with no per-child allocation. One that fills does what a
Layout does, one polish per change. The D1 spike (branch `dialect/d1-layout`)
measures both with budget cases.

**Lowering.**

- `Row`/`Column` → `Row`/`Column`. A positioner child with `Layout.*` keeps
  the file, since Qt ignores it and Ljós would honour it.
- `RowLayout`/`ColumnLayout` → `Row`/`Column`, unless a child sets an
  explicit size with no `preferred*` and an implicit size not statically 0.
  That child is the case where the rules differ, and its file is kept, with
  that reason. A child whose size comes from its content (a `Text`, an
  implicit size the pass cannot read or that is a fraction) is lowered: the
  `Row` or `Column` rounds it up as `RowLayout` does. In a `ColumnLayout`
  such a child that says no `Layout` gets `Layout.fillWidth: false`, so the
  `Column` is a layout and rounds; in a `RowLayout` the `Layout.alignment`
  the pass writes does that. An explicit, preferred, least or largest size
  that is not a whole number keeps the file.
- A `RowLayout` nested in a `ColumnLayout` grows across the column where a
  child of it fills with no maximum, and otherwise keeps its own width; the
  pass writes `Layout.fillWidth: true` on the lowered `Row` there, and
  `Layout.fillHeight: true` on a `Column` in a `Row`. A nested layout with
  a size, an implicit, preferred, least or largest size, anchors, a fill
  the pass cannot read, a child that fills up to a maximum (Qt's grows to
  it, a `Row` that fills does not stop there), or a `fill*: true` where
  nothing in it grows keeps the file, as does a nested `GridLayout` or
  `StackLayout`. A child with `visible: false` grows nothing, as neither
  layout gives it room, and one that fills and is shown by a binding keeps
  the file. So does a delegate that fills, of a `Repeater` whose model the
  file does not show to have an entry (a number of at least 1, or an array
  that holds one): with none, Qt's layout does not grow.
- `GridLayout` → `Grid`, kept when a child sets an explicit size with no
  `preferred*` and an implicit size not statically 0, as `RowLayout` is.
  The pass writes out Qt's `columnSpacing: 5` and `rowSpacing: 5`, and
  `Qt.AlignVCenter` on a child, as for `RowLayout`. It keeps the file where
  Qt's room is spread on either axis that no child fills, and where Qt's
  `GridLayout` reads it otherwise (below). `GridLayout.TopToBottom` in a
  script is `Grid.TopToBottom`.
- `Grid` → `Grid`, kept when a child has `Layout.*`, as `Row` is, and where
  Qt's positioner and Ljós's `Grid` place children apart (below).
- `StackLayout` → `Pages`, as written. A file that names `StackLayout`'s
  attached object (`StackLayout.isCurrentItem`, `StackLayout.index`) is
  kept until `Pages` has one.
- `Flow` → `Flow`, kept when a child has `Layout.*` or anchors, or the
  Flow sets `add`, `move` or `populate`.
- `LayoutItemProxy` keeps its file. It needs a person to merge the two
  layouts into one Box.
- A lowered file that names any of these imports `Ljos.Layout`. A
  `QtQuick.Layouts` line becomes `import Ljos.Layout`, and otherwise
  `; import Ljos.Layout` goes on the `import Ljos.Ui` line, so no
  line moves.

**Grid.** Ljós's `Grid` is a Box in two axes with the one size rule: a
child keeps its own size, `Layout.row`, `column`, `rowSpan`, `columnSpan`
and `fill*` are honoured, and room left over stays at the end. Its
counts are `columns` and `rows`; with only one set, the other is as many
as the children need. With neither, it is one line along `flow`. Qt's
`Grid` positioner differs in four places, measured with qml6:

| Qt's `Grid` | Lowering |
|---|---|
| neither count set: 4 columns | writes `columns: 4` |
| a child beyond rows × columns is left where it is | keeps the file |
| a line along the flow it does not fill counts the spacing after its last child | keeps the file when that spacing is not 0 |
| a line across the flow it leaves empty counts its spacing | keeps the file when that spacing is not 0 |

The pass counts the children a file shows: a `visible: false` child or a
literal zero size is skipped, as Qt skips it, and a bound `visible` or a
size it cannot read gives a range. It keeps the file if any count in the
range differs. A `Repeater` child keeps it unless every spacing is 0 and
only one count is set. `layoutDirection` and the item alignments keep the
file until they are measured.

**GridLayout.** Qt's `GridLayout` sizes its cells by the same rules as its
line layouts, which Ljós's `Grid` follows: qml6 places a two-column grid of
fill, fixed and fill-height children, two fill children sharing a row, and
a top-to-bottom grid of two rows, as Ljós does
(features/layouts/ljos-grid-pages.feature). It differs where:

| Qt's `GridLayout` | Lowering |
|---|---|
| `rows` along a left-to-right flow is ignored: `rows: 2` puts three children in one row | keeps the file |
| `columns` along a top-to-bottom flow is ignored: `columns: 2` puts three children in one column (qml6) | keeps the file |
| `Layout.row`, `column`, `rowSpan`, `columnSpan` place a child in a cell the pass does not follow yet | keeps the file |
| `layoutDirection`, `uniformCellSizes`, `uniformCellWidths`, `uniformCellHeights` | keeps the file |
| a `flow` the pass cannot read | keeps the file |

DECISION: `Pages` is a face of `StackLayout` under Ljós's name, as
`PointerArea` is of `MouseArea`: `currentIndex` and `count` as they are, and
a hidden child keeps its own size (qml6). It has no attached object yet, so
a file that reads one is kept.

DECISION: a `GridLayout` is lowered only where its `flow` and counts say
what Ljós's `Grid` reads. Qt's ignores the count across its flow, and a
`Grid` that read it would place children apart, so the pass keeps the file
rather than drop the count, which would change what a person reads.

DECISION: Ljós's `Grid` is a one-rule native, not Qt's positioner. A
positioner that never sizes a child is a `Grid` with no `fill`, and the
four differences above are corner cases the lowering keeps.

DECISION: Ljós's `Grid` with no count is one line. Qt's 4 is a magic
number, and the pass writes it out.

DECISION: `Flow` is Qt's `Flow`, taken whole into `Ljos.Layout`. Its
transitions and `Layout.*` children keep the file until the Box rules
reach a wrapping line.

DECISION: the `QtQuick.Layouts` import line always becomes
`import Ljos.Layout`, even when the file names nothing on it, since a
sibling's root may be a `Row` that takes `Layout.*`.

DECISION: an explicit size is a request in a Ljós Box, as it is in a
positioner. Qt's Layouts ignoring it is the surprise this removes. The
lowering keeps a file only where that changes a number.

### 4.7 Input: one pointer stream, gestures as children

**What apps say.** "When this is tapped", "this can be dragged", "this
zooms". The corpus has two input APIs for it:

| | Uses | Files |
|---|---:|---:|
| `MouseArea` | 61 | 47 |
| of which `anchors.fill: parent` | 52 | |
| of those, no children (no `Connections`, `PinchArea` or `DropArea` inside) | 45 | |
| `TapHandler` / `DragHandler` / `PinchHandler` / `HoverHandler` / `WheelHandler` | 29 / 10 / 3 / 3 / 5 | |
| `PinchArea`, `MultiPointTouchArea` | 2, 0 | |
| `onClicked` | 196 | 81 |
| `onPressed` / `onReleased` / `onPositionChanged` | 13 / 5 / 5 | |
| `preventStealing`, `propagateComposedEvents`, `mouse.accepted` | 5, 0, 0 | |

52 of the 61 MouseAreas fill their parent, and 45 of those have nothing
inside. They are an item made only to say "the parent is tappable", with a
geometry that has to be kept equal to the parent's.

**The design.** A gesture is a child object of the item it acts on, not an
item: `Tap`, `Drag`, `Pinch`, `Hover`, `Wheel`, `LongPress`. All of them
read one pointer stream (mouse, touch and pen alike). Each gesture
recognises from that stream, and the first one to recognise takes the
pointer. Gestures are offered a press from the inside out: the hit test
finds the deepest item under the pointer, that item's gestures see it
first, then its parent's, and so on up. A gesture that takes the pointer
hides it from every gesture further out, as an accepted press does in Qt.
So a gesture on an item never covers that item's children. `takeOver: true` lets a gesture win over one that has already
started (a pinch over a drag), which replaces `preventStealing` and
`grabPermissions`. Events are typed (`TapEvent`, `DragEvent`) and carry
`position`, `modifiers`, `button` and `device`.

```qml
// Qt (photosurface.qml, abridged)                   // Ljós
PinchHandler {                                       Pinch {
    minimumRotation: -360; maximumRotation: 360          rotation { minimum: -360; maximum: 360 }
    minimumScale: 0.1; maximumScale: 10                  scale { minimum: 0.1; maximum: 10 }
    grabPermissions: PointerHandler.CanTakeOverFromAnything   takeOver: true
}                                                    }
DragHandler { id: dragHandler                        Drag { momentum: Drag.Momentum.Ease }
    onActiveChanged: { if (!active) anim.restart(     Hover { id: hover }
        centroid.velocity) } }                       Wheel { modifiers: Modifier.Control; drives: "rotation" }
MomentumAnimation { id: anim; target: frame }
HoverHandler { id: hoverHandler }
WheelHandler { acceptedModifiers: Qt.ControlModifier
    property: "rotation" }
```

Qt's version needs a 40-line `MomentumAnimation` file to keep a flung photo
moving. Ljós's `Drag.momentum` is that, built in, on the animation clock.

**Cost.** A gesture is not an item: no geometry, no scene node, no hit-test
rectangle of its own. A tap on a cell of `calqlatr`'s pad does one hit test
against the cell, where Qt's version tests the cell and then its
MouseArea.

**Lowering.**

- `MouseArea { anchors.fill: parent; onClicked }` with no children →
  `Tap { gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0;
  onTapped }` in its place, and its `cursorShape` (with its `enabled`) a
  `Hover` beside it. Built. qml6 says that `Tap` clicks as the area does:
  let go inside wherever the pointer went in between, however long it was
  held, twice for a double-click, not for the right button unless
  `acceptedButtons` (→ `buttons`) says so, and nothing below hears it. A
  gesture on the parent sees a press after the parent's children, where
  the MouseArea sees it before the siblings declared ahead of it (it is
  drawn over them). The two agree unless something that takes the pointer
  (a MouseArea, a handler, a view, a text input, a `Loader`, a type the
  pass does not know) is declared before it, a sibling or the area sets
  `z`, or the parent has a `containmentMask`. The area then stays a
  `PointerArea` and the file is still lowered. So does an area that does
  more than click: one with members other than `id`, `objectName`,
  `anchors.fill`, `onClicked`, `enabled`, `acceptedButtons` and
  `cursorShape`, whose `id` is read (an alias, `area.pressed`), whose
  handler reads `mouse` or a member of the area, `Tap` or `Hover`
  unqualified, or whose parent is not an `Item`, `Rectangle` or `Image`.
  In the qtdoc corpus every file with such an area is kept for another
  type today, so none is lowered yet.
- `hoverEnabled` + `containsMouse` → `Hover` + `hover.hovered`. Not built:
  such an area stays a `PointerArea`.
- `drag.target` stays a `PointerArea`. qml6 moves a MouseArea's target
  the drag threshold short of the pointer, where a `DragHandler` moves it
  with the pointer, so `Drag` would move it further.
- `*Handler` → the gesture of the same kind, with members renamed.
  Built: `TapHandler` → `Tap`, `HoverHandler` → `Hover`, `DragHandler` →
  `Drag`, `PointHandler` → `Pointer` and `WheelHandler` → `Wheel`. What a handler
  accepts is what the gesture takes, named as a pointer event names what
  it carries: `acceptedButtons` → `buttons`, `acceptedModifiers` →
  `modifiers`, `acceptedDevices` → `devices`, `acceptedPointerTypes` →
  `pointerTypes`, with their change signals and handlers. An enum read
  through the handler's name follows it (`TapHandler.WithinBounds` →
  `Tap.WithinBounds`), and `PointerDevice` is in `Ljos.Ui` as it is.
  Such a member read off an object the pass cannot type (a `QtObject`
  property) keeps the file, as `color` read off an `Item` does. A new
  name read unqualified inside the object whose Qt type has no member of
  that name (`buttons` in a `TapHandler`'s handler, where Qt finds the
  root's `buttons`, or `fill` in a `Rectangle`'s binding) keeps the file:
  once lowered it would be the object's own.
- A MouseArea with children, a geometry of its own, or `mouse.accepted`
  keeps its file as a `PointerArea` (phase 1's face) until M3.

DECISION: `grabPermissions` and `PointerHandler`'s enum keep the file
until `takeOver` is built: Qt's flags say more than the one boolean, so
there is no one-for-one lowering. `cursorShape` keeps its name, which
`PointerArea` shares, and `gesturePolicy` its name and keys.

DECISION: a gesture is a face of its handler, so a member a base of the
handler declares (`HoverHandler`'s `acceptedDevices` is `PointerHandler`'s)
is renamed on the face too, and the face is described with that base's
members as its own: a description names each member once.

DECISION: Qt's attached `Drag` (drag and drop, 7 uses in 3 files) is not
the `Drag` gesture. It becomes `DragSource` and `DropArea` in `Ljos.Ui`,
since moving an item and carrying data are two different things. Built:
`Drag.active`, `Drag.hotSpot` and the rest, and `Drag.Automatic` read
through the type, lower to `DragSource`'s; `DropArea` is as it is. An
item's `Drag.active` and `DragSource.active` are one value.
`.Drag` read off an object is renamed only when the object is an item (an
id, `parent`): only an item has the attached `Drag`. Off a value
(`"x".Drag`) it is the value's and stays as written. Off something the
pass can't type (`config.Drag.speed`, `config` a JavaScript object), or
off an object that is no item, it keeps the file.

DECISION: `PointHandler` is `Pointer`, the one point on its item it
follows, as `Tap` and `Hover` are named for what they see.

DECISION: a filling MouseArea is a `Tap` with `ReleaseWithinBounds` and
no long press, not a plain `Tap`: a plain one gives up a press let go
outside or held past `longPressThreshold`, which the area still clicks
for. The two members say so in the lowered file instead of a new default.

### 4.8 Models and delegates

**What apps say.** "Show this list", "show these rows, and keep them when
they change", "the user picked this one". The corpus:

| | Uses | Files |
|---|---:|---:|
| `ListModel` / `ListElement` | 38 / 163 | 25 / 12 |
| `model: [` (an array literal) / `model: N` | 14 / 6 | |
| `Repeater` / `ListView` / `GridView` / `PathView` | 35 / 29 / 1 / 2 | |
| `modelData` / `model.<role>` | 88 / 17 | 27 / 9 |
| `required property` | 314 | 128 |
| `.append({` / `.get(i)` / `.remove(` / `.clear()` | 17 / 38 / 9 / 20 | |
| `currentIndex` | 94 | 26 |

Most ListModels (12 files' worth of 163 ListElements) are constant tables
written as objects. Their roles are strings that the delegates' `required`
properties have to match by hand.

**The design.**

- **Static rows are a typed list.** `readonly property list<Zones.Zone> zones: [...]`
  with `model: zones`, where the row type is a TS interface. The view's
  delegate gets `required property Zones.Zone modelData`, and the checker
  checks every field read against it.
- **A view takes any `list<T>` and a `key`.** `key: "id"` makes the view
  diff a new list against the old one by key. A row that stays keeps its
  delegate (and its state and animations), a new key gets a new delegate
  with the view's `enter` transition, and a removed one plays `exit`
  before it is destroyed. Assigning a new list (`game = move.game`) is the
  ordinary way to change a model, and a reducer in TS returns a new list.
- **`ListModel` stays** for in-place edits (`append`, `move`) that another
  part of the app observes. Its row type is inferred from its
  `ListElement`s or declared with `ListModel { row: Calc.Line }`.
- **Delegates' `required` properties are checked** against the row type,
  so a misspelt role is a check error. `modelData` is kept as the name of
  the row.
- **The selection is the view's.** `currentIndex` is what is chosen, and
  everything else is derived from it, never a second copy kept in step by
  click handlers (rssnews's `currentFeed`).
- `ListView.follow: ListView.Follow.End` keeps a log or a calculator tape
  at its end. It replaces `onCountChanged: positionViewAtEnd()`.

```qml
// Qt (clocks/Main.qml)                              // Ljós
ListView {                                           readonly property list<Zones.Zone> zones: [
    model: ListModel {                                   { city: "New York", shift: -4 },
        ListElement { city: "New York"; shift: -4 }      { city: "London", shift: 0 },
        ListElement { city: "London"; shift: 0 }         ...
        ...                                          ]
    }                                                ListView { model: zones; key: "city"
    delegate: Clock { city: model.city;                  delegate: Clock {} }
                      shift: model.shift }           // Clock.qml
}                                                    required property Zones.Zone modelData
// Clock.qml: property string city; property real shift
```

`samegame` shows a keyed list doing what Qt's version does by hand
(§4.11).

**Cost.** A static list is a constant: nothing to build at start, where a
`ListModel` builds a role table and its rows. Diffing by key is O(n) in the
list's length per change. Keeping lists as persistent vectors (M5) would
make it O(changes). A keyed view creates and destroys only what changed,
where Qt's views reset when a model is replaced.

**Lowering.**

- A `ListModel` of constant `ListElement`s that nothing edits → a typed
  list. Built (`features/models/ljos-constant-rows.feature`). One with an
  id, a child of the file's root, becomes `readonly property
  list<Rows.<Id>Row> <id>: [...]` (a trailing `Model` dropped from the
  name), and one written in a view's `model:` becomes `readonly property
  list<Rows.<ViewId>Row> rows: [...]; model: rows` in that view (`Row`
  where the view has no id). The row types are written to `<File>.rows.ts`
  beside the file, imported `as Rows`, each field `readonly`. An element
  that leaves a role out is given that role's default as Qt reads it
  (`""`, `0`, `false`), and a delegate's `required` properties stay, since
  a list's objects give them as a model's roles do. The `id:` line stays,
  empty, so the lowered file keeps its lines. A ListModel stays one where
  a script could edit it or read it as a model: its id is read other than
  as a view's `model:`, a view's model is read off it anywhere in the file
  (`list.model.count`, `ListView.view.model`), a value is not a literal
  (`qsTr`), the view has sections (`section.property` or `section { … }`),
  a delegate reads a role by its bare name, writes one, or reads `model`,
  or the file has other local types, inline components or a `Loader`.
- `model.<role>` → `modelData.<role>` where the file shows a model of
  several roles; `model.index` → `index` and `model.modelData` →
  `modelData` in any delegate the file shows. A delegate is one written
  in a view's `delegate:` (or its default property), or a `Component` a
  view is given by id (`delegate: row`) that nothing else in the file
  names; one named elsewhere too may be made of other rows. Where the file
  shows no view, or not the model, the file is kept, with the name to
  declare `required`.
- `import QtQml.Models` → `import Ljos.Models`. A lowered file that makes a
  `ListModel`, `ListElement` or `Instantiator` gets `import Ljos.Models`.
- `XmlListModel` → `XmlModel` with a generated decoder.
- `FolderListModel` → `FolderModel`.

DECISION: typed lists with keys are the default, and ListModel is for
shared, edited lists. A ListModel's row type is its `row:`, or else the
union of its `ListElement`s' roles. Qt reads a role an element leaves out
as its type's default (`""`, `0`, `false`; checked on Qt 6.11), not
`undefined`, so a constant table whose elements differ lowers to a typed
list whose every field is set, the missing ones to that default, and no
field is optional.

DECISION: a delegate's `modelData` is its row. Of an array or a count it
is the element, of a model of one role that role's value, and of a model
of several roles the row object itself, and a role written through it
(`modelData.shift = 1`) is written to the model. A delegate written in
Ljós's API reads it so whether or not it declares it `required`; Qt gives
the row only to a required `modelData`, leaves the context's undefined
and refuses writes to it, and a delegate of a Qt file keeps that. The
pass writes `modelData.<role>` only where `modelData` is that row: a
`ListModel` of several roles written in the file, given inline or by id.
A ListModel of one role with an id is not known to stay one, since a
script may `append` rows of other roles, so `model.<role>` of it keeps
the file.

DECISION: outside a delegate the file shows, nothing of a row is
guessed. A file that reads `model.index` or `model.modelData` with no view
around it is a delegate only if what makes it makes it one, so it is kept
as a role read there is, not lowered to `index` or `modelData`.

DECISION: a name the file declares is the file's own. A `property var
model` (a delegate's `required property var model` too), or a script's
`model`, `index` or `modelData` in a delegate that reads the row, is not
the row: the file is kept where a rename would read it instead.

DECISION: a duplicate `key` is a check error in a constant list. At run
time rows that share a key are matched in order (the second row with key
`k` keeps the delegate the second one had), and the view logs it once.

DECISION: keys are compared with their type: the key `1` and the key
`"1"` are two keys. A row that has no field of the key's name has the key
`undefined`, which rows without it share.

DECISION: `key`, `enter` and `exit` are on the native `Repeater` and
`ItemView`, so a Qt file that sets them runs them too, as an extension;
Qt has none of them. `Ljos.Ui`'s `ListView` is a face of Qt's that calls
`add` and `remove` `enter` and `exit`; the attached `ListView.onAdd` and
`ListView.onRemove` keep their names.

DECISION: an `exit` runs only on a row the list removed. A new model, a
new delegate, or the view's end destroys every delegate at once.

DECISION: a `Repeater` plays `enter` and `exit` only while it is keyed:
given a `key` and a list. The insert or removal of a model it does not
key has no row that the dialect promises is the same one, so its items
come and go at once.

DECISION: the pass renames `add` and `remove` of a `ListView` as
properties only: `lm.remove(0)` is a call and stays, since every member
the pass renames is a property.

### 4.9 States

**What apps say.** "In this condition it looks like this", and sometimes
"go to that state now". The corpus says the first far more often:

| | Uses | Files |
|---|---:|---:|
| `states: [` / `State {` | 90 / 322 | 82 / 90 |
| `when:` | 186 | 56 |
| `.state = "…"` (imperative) | 37 | 8 |
| `state === "…"` (read back) | 13 | 2 |
| `extend:` | 95 | 13 |
| `PropertyChanges` with `target:` | most of the older files | |
| `transitions:` with `from`/`to` strings | 23 | 12 |
| `StateChangeScript` / `AnchorChanges` / `ParentChange` | 7 / 14 / 0 | |

**The design.**

- **`state` is typed as the union of the names a file declares.** Assigning
  or comparing a name that is not declared is a check error, as are
  `Transition.from`/`to` and an animation's `properties: "…"` naming
  something that does not exist. Phase 1's checker already reports
  `` `state` is a string; this gives it a number``.
- **`when:` is the preferred form.** A state follows from the data, and a
  person writes the condition once. The imperative `.state = "…"` stays,
  typed.
- **Only the `PropertyChanges { id.prop: … }` form.** `target:` and
  `explicit` lower to it.
- `AnchorChanges` stays for free parents. `StateChangeScript` is kept, and
  a keyed view's `enter`/`exit` replaces its usual use (a delegate that
  destroys itself).

```qml
// Qt (calqlatr/CalculatorButton.qml)            // Ljós
function getBackgroundColor() {                  states: [
    if (button.dimmable && button.dimmed)            State { name: "dimmed"; when: button.dimmable && button.dimmed
        return backgroundColor;                          PropertyChanges { label.color: … } },
    if (button.pressed) return accentColor;          State { name: "pressed"; when: tap.pressed
    return backgroundColor;                              PropertyChanges { button.fill: button.accent
}                                                            button.border.color: button.accent
function getBorderColor() { … }                              label.color: "#222222" } },
function getTextColor() { … }                        State { name: "hovered"; when: hover.hovered
background: Rectangle {                                  PropertyChanges { … } }
    color: button.getBackgroundColor()           ]
    border.color: button.getBorderColor() }      // `state = "presed"` is a check error
```

Qt's button re-derives each of its three colours from `pressed`,
`hovered` and `dimmed` in a function of its own. Ljós's names the four
looks once, each with the condition it follows from. The first state
whose `when:` holds applies.

DECISION: no general state-machine type. 37 imperative assignments in 8
files do not pay for a second model of states. The typed union and
`when:` cover them, and a TS reducer (calqlatr's `press`) covers the rest.

**Built** (features/states/ljos-states.feature):

- The pass writes `PropertyChanges { target: face; color: … }` as
  `PropertyChanges { face.fill: … }`. The target's binding goes, and its
  line is left empty when it had one of its own. Each change takes the id,
  renamed members (`color` → `fill`) and handlers (`face.onFillChanged`)
  too, and a dotted one (`face.border.width`). `explicit` and
  `restoreEntryValues` stay members of the `PropertyChanges`. qml6 runs
  both forms the same, handlers included.
- The checker types an object's `state` as `""` or one of the names its
  `states` give, where the file writes them. A binding or an assignment of
  another name is `ts-assign-type`, and so is a `===` or `!==` of strings
  written out that share none, and a state's `extend` naming one not
  declared.

DECISION: a target that is not an id (a property, `parent`, a path) stays
a target. The id form has no spelling for it, and `Ljos.Ui`'s
`PropertyChanges` still has `target`, so the file is lowered rather than
kept. In the corpus, coffee's `ApplicationFlow.qml` targets aliases this
way.

DECISION: a group (`font { … }`) or an attached member (`Layout.…`) set
through a target keeps the file. qml6 takes `face.Layout.preferredWidth:`,
but Ljós's id form is not yet held to it; this keeps 2 corpus files
(`TabMenu.ui.qml`, `HomeForm.ui.qml`).

DECISION: a state named by an expression leaves `state` a string, as does
an object whose states another file declares: what they are is known only
as it runs, or there.

DECISION: only strings written out are said never to be equal, as
TypeScript says of types with no overlap; any other comparison is let by.

DECISION: a `string` goes into `state`, where TypeScript would refuse it:
Qt's apps keep the next state in a string property (coffee's
`ApplicationFlow.qml` sets `state = previousState`). Only a name written
out, or a union of them, that the file does not declare is an error.

Remaining: `Transition.from`/`to` (a comma list, as qml6 reads it) and an
animation's `properties` are checked once `Transition` is in `Ljos.Ui`.

**Cost.** Unchanged from Qt's model. A `when:` is a binding, and a typed
name is compiled to an index where the union is closed.

### 4.10 Focus and keys

**What apps say.** "This takes keys first", "these keys do that". The
corpus is light on it: `focus: true` 18, `forceActiveFocus` 11,
`FocusScope` 2, `Keys.onPressed` 5, and `Qt.Key_*` 78 uses in 5 files,
mostly long `switch (event.key)` tables.

**The design.**

- A typed `KeyEvent` (`key: Key`, `text`, `modifiers: Modifier`, `repeat`)
  and a `Key` enum in `Ljos.Ui` (`Key.Digit0`, `Key.Enter`, `Key.Escape`).
- **Key tables are data.** A TS `Map<Key, Action>` from keys to what they
  do replaces a chain of `if`s:

  ```qml
  // Qt (calqlatr/Main.qml: 21 cases)                  // Ljós
  Keys.onPressed: function (event) {                   Keys.onPressed: (event: KeyEvent) => {
      switch (event.key) {                                 const key = Calc.fromKey(event.key)
          case Qt.Key_0: appState.digitPressed("0"); break;  if (key !== undefined) press(key)
          case Qt.Key_1: appState.digitPressed("1"); break;  }
          ...                                          // calc.ts: fromKey = (k: Key) => keys.get(k)
  ```

- `Shortcut { keys: "Ctrl+Q"; onActivated }` stays, with its sequence
  checked at compile time.
- `KeyNavigation.*` is kept as it is (0 uses in the corpus).

**Option, not decided (§18):** every component's root is an implicit focus
scope. That makes a component's `focus: true` local to it, which is what
the 2 FocusScopes and most `forceActiveFocus` calls are working around.
Lowering writes `Focus.scope: false` on a root that relies on Qt's flat
focus, so nothing changes for a lowered file.

### 4.11 Creation

**What apps say.** "Show one of these here", "make one of these for each
row", and rarely "make one now". The corpus:

| | Uses | Files |
|---|---:|---:|
| `Loader` / `sourceComponent` | 8 / 2 | |
| `source: "…qml"` (a file named by a string) | 563 | 150 |
| `Component {` / inline `component X:` | 20 / 15 | |
| `createObject(` / `Qt.createComponent` / `createQmlObject` | 19 / 15 / 9 | 6 / 3 / 2 |
| `destroy(` | 6 | 6 |
| `StackView` / `.push(` | 18 / 60 | 14 / 18 |

**The design.**

- **A component type is a value.** `Loader { component: Page }`,
  `property Component block: Block` (a `Component<Block>`), and
  `Block.createObject(parent, { column: 2 })` with its properties typed.
  No file is named by a string in a Ljós file.
- **Declarative creation is preferred.** A keyed `Repeater` over a list
  creates, moves and removes objects as the list changes, with `enter` and
  `exit` transitions. `samegame` makes every block this way:

  ```qml
  // Qt (samegame.js)                                // Ljós (GameArea.qml)
  component = Qt.createComponent(blockSrc)           Repeater {
  var dynamicObject = component.createObject(            model: area.game.blocks
      gameCanvas, { type: Math.floor(...),               key: "id"
      x: column*gameCanvas.blockSize, ... })             delegate: area.block
  board[index(column,row)] = dynamicObject               exit: Transition {
  ...                                                        ScriptAction { script: target.burst() }      
  board[i].dying = true                                      NumberAnimation { property: "opacity"; to: 0; duration: 200 }
  board[i].destroy(1000)                                     PauseAnimation { duration: 800 }
  ...                                                    }
  obj.x = (column - fallDist) * blockSize            }
                                                     // Block.qml: x: column * size  (a binding; Behavior animates it)
  ```

  In a view's `enter` and `exit`, `target` is the delegate, typed by the
  view's `delegate` (here `Component<Block>`, so a `Block`). No cast.
  The board is a typed list in `game.ts`. The view owns the objects, so
  nothing leaks a block or touches one after it is destroyed.
- `StackView` stays in `Ljos.Controls`, with `push(Page, { … })` typed by
  the component.
- `createQmlObject` (a string of QML compiled at run time) is compat only.

**Cost.** A keyed Repeater creates only what is new and keeps only what is
alive. Qt's samegame keeps a JS array of live items beside the scene, and
every move is a JS write per block.

**Lowering.** `Loader { source: "X.qml" }` with a static string →
`Loader { component: X }`. `sourceComponent` → `component`.

Built (phase 1): `Ljos.Ui`'s `Loader` has `component` for Qt's
`sourceComponent` (and `componentChanged`, `onComponentChanged`). In a file
of Ljós's API, a type named where a component goes — `component: Page`,
`property Component shown: Page` — is a component of that type, as
`Page {}` written there is. The compat pass writes
`source: "Page.qml"` (or `"./Page.qml"`) as `component: Page` when
`Page.qml` is beside the file and `Page` is a type's name (a capital,
then letters, digits or `_`; not `page.qml`, `my-page.qml` or
`Page.ui.qml`). It keeps the file when the string is made as it runs
(`page + ".qml"`, `Qt.resolvedUrl(…)`), names a file elsewhere
(`"pages/Page.qml"`, `qrc:`), when the Loader is given a `sourceComponent`
too, or when its `source` is also read, set or handled (`loader.source`,
`setSource`, `onSourceChanged`, a bare `source` in its own bindings or,
for the root of the file or of an inline component, anywhere in its scope,
a `Connections` or `PropertyChanges` aimed at it): after lowering it is
empty. A `source` a script binds itself, a parameter or a local, is the
script's and not the Loader's, as it is for every name the pass reads. It keeps the file, too, when the Loader's `sourceComponent` is read
or heard (`loader.sourceComponent`, `onSourceComponentChanged`): Qt's stays
empty beside a `source`, and the lowered one is the component. `ljos check --tsc` and the authoring tools read `component: Page`
as the compiler does.

DECISION: `Ljos.Ui`'s `Loader` keeps `source` beside `component`. A face
renames members and cannot leave one out, and `setSource` is how a file
loaded by name gets its properties. "No file is named by a string" holds
for what the compat pass writes and is for `ljos check` to say, not yet
for the type.

DECISION: a type named where a component goes is a component only in a
file that imports `Ljos.Ui`. In Qt's QML the same text is an error, and a
file written for Qt keeps it.
`Qt.createComponent("X.qml").createObject(p, {…})` with a static string →
`X.createObject(p, {…})`. A URL built at run time, `createQmlObject` and
`destroy()` keep their files (R1).

DECISION: `destroy()` is not in a Ljós file. An object a view made is
removed by its model, and one made by `createObject` is owned by its parent
and removed with `parent.remove(child)`.

**Creation order.** An object's initial properties, aliases and bindings
are settled together, and only then do its change handlers run, in height
order, before `Component.onCompleted`. A handler sees every binding of the
object with its first value. Qt runs an initial property's handler as the
property is written, while its bindings are still pending, and a binding
the handler reads is run there. DECISION (#331): Ljós keeps its batched
order. Qt's belongs to compat, as each quirk marked `// Qt 6.11:` does,
and `features/components/creation-order.feature` records both logs.

### 4.12 Theming

**What apps say.** "Use the app's colours, spacing and type". The corpus
says it by hand: 880 colour literals (`"#…"`) in 198 files, a `Style` or
`Theme` singleton in 86 files (413 uses) that each app writes for itself,
`palette.*` 41 times, and `Material.*` 112 times.

**The design.** `Ljos.Design` gives every app the same roles (ADR 0003):

- `Theme.color.surface`, `.onSurface`, `.primary`, `.pressed`, `.hover`, …
- `Theme.space.s/m/l`, `Theme.type.body/title`, `Theme.radius.m`
- `Theme.motion.short/medium` (durations that honour reduced motion)

A theme is a `theme.json` that `ljos run` reloads live. Controls are built
on the roles, so a theme restyles every control and every app that uses
roles in one place.

**Lowering.**

- `palette.<role>` → `Theme.color.<role>` where they map one to one.
- `Material.*` and `Universal.*` keep their files, since a style's own
  attached API is compat.
- Colour literals stay. They are legal, and a person moves them to roles.

**Cost.** A role is a property of one singleton: one binding read, the same
as an app's own Style singleton.

### 4.13 Text

**What apps say.** "Show this string, this big, aligned so, wrapped so,
translated". The corpus has `Text` 189, `Label` 105, `font.pixelSize` 272
against `font.pointSize` 18, `horizontalAlignment` 126, `wrapMode` 77,
`elide` 20, `qsTr(` 445, `.arg(` 50, and 18 strings concatenated into a
translated text.

**The design.**

- `font.size` is in logical pixels, and there is no `pointSize`: two units
  for one thing is a bug source (18 pointSizes against 272 pixelSizes).
  `font.weight: Font.Weight.Bold` replaces `font.bold`.
- `align: Align.Center` is one shared `Align` enum for text, layouts and
  images, with `horizontalAlignment`/`verticalAlignment` as its axes.
- Enums are their type's: `wrap: Text.Wrap.Word`, `elide: Text.Elide.End`,
  `format: Text.Format.Rich`, `style: Text.Style.Raised`.
- `Text.color` is the glyph colour, and keeps its name: §4.2's rule renames
  only the member that names something else on another type
  (`Rectangle.color` is a fill).
- `qsTr("%1 of %2").arg(a).arg(b)` has its `.arg` count checked against
  the placeholders. Plurals use `qsTr("%n minutes ago", "", n)`, and
  concatenating a number into a translated string is a check warning
  (rssnews's `'' + minutes + ' ' + qsTr("minutes ago")`).

**Option, not decided (§18):** a tagged template
``tr`${n} minutes ago` `` that extracts to the same catalogue, checked by
construction.

**Lowering.**

- `font.pixelSize` → `font.size`.
- `font.pointSize: n` → `font.size: Screen.points(n)`. Points to pixels is
  the screen's logical DPI / 72, known only at run time and different per
  screen, so the lowering writes the conversion, not a number. It is
  reactive on the item's screen, so text moved to another screen resizes
  as it does in Qt (§4.3). A file that names a `Screen` of its own, or
  imports a directory or module that may have one, is kept.
- `font.bold: true` → `font.weight: Font.Weight.Bold`.
- `wrapMode: Text.WordWrap` → `wrap: Text.Wrap.Word`, and every other enum
  the same way, by its owner in the table.

Built (phase 1): `Ljos.Ui`'s `Text` calls `horizontalAlignment`,
`wrapMode` and `textFormat` `align`, `wrap` and `format`, with their change
handlers (`onAlignChanged`); `elide` keeps its name. A font's pixel size is
also its `size`. The pass renames these wherever a member is named: a
binding, a group (`font { pixelSize: 12 }`), a `PropertyChanges`
(`label.font.size: 40`), a handler and a script. `font.bold: true` lowers
to `font.weight: Font.Bold`, `false` to `Font.Normal`, and an expression
`e` to `(e) ? Font.Bold : Font.Normal`. `font.pointSize: n` lowers to
`font.size: Screen.points(n)`, in a binding and a group.

DECISION: `font.pointSize` always lowers, though `font.size` is a whole
number of pixels: a fractional size rounds, where Qt's points draw a
fraction. At 96 dots to the inch the common sizes (9, 12, 18) are whole.

DECISION: the enums keep Qt's names for now. `align: Text.AlignRight`,
`wrap: Text.WordWrap` and `Font.Bold` are what a Ljós file says until the
enum cleanup (M3); `Text.Wrap.Word`, `Font.Weight.Bold` and the shared
`Align` above are where it goes.

DECISION: the font is one value type for the whole engine, not one per
module, so `size` is a second name for `pixelSize` on every font, and a Qt
file can say `font.size` too. A Ljós file is not yet held to `size` and
`weight`; that waits for natives to take the API's names (M5).

A file is kept, for now, when:

- a script reads or writes `.bold` of a font (Ljós's font has only a
  `weight`, and a read of it is not a boolean);
- it sets both `font.bold` and `font.weight` (in Ljós they are one
  `weight`), in whichever form: `font.bold` beside `font { weight: 300 }`
  is both;
- it sets `font.bold` to a block;
- a script reads or writes `.pointSize` of a font (Ljós's font has only
  a `size` in pixels);
- it sets both `font.pointSize` and `font.pixelSize` (one `size`);
- it sets `font.pointSize` to a block;
- it sets `font.pointSize` where `Screen` may be another type: one of the
  file's own, one beside it, or one of what it imports;
- it reads `.pixelSize`, `.bold` or `.pointSize` off something whose type
  is not known but that may be a font: a member called `font`
  (`someItem.font.bold`) or a property the file declares as a `font`, a
  `var` or an `alias`, by its name or as a member (`property var face`,
  `face.pixelSize`, `root.face.bold`; `property alias face: label.font`,
  `label.face.pixelSize`); or it reads
  `font["pixelSize"]`.

DECISION: off anything else, such a name is not a font's, and is left as
written: a row's role (`model.bold`), a script's object
(`({ bold: true }).bold`), a function's parameter. The pass does not follow
what a function is given or a local is assigned, so a font passed to a
function and read there as `f.bold` is left as written.

`fillMode` → `fit` is `Image`'s, and `TextInput`'s and `TextEdit`'s
renames are theirs: they follow when those types are in `Ljos.Ui`.

### 4.14 Every Qt construct, and where it goes

| Qt | Ljós | Lowering |
|---|---|---|
| `Row`, `Column` | `Row`, `Column` (a Box) | mechanical; kept when a child has `Layout.*` (§4.6) |
| `RowLayout`, `ColumnLayout` | `Row`, `Column` | mechanical; kept when an explicit size meets a non-zero implicit size |
| `GridLayout`, `Grid` | `Grid` | mechanical; kept as `RowLayout` and `Row` are, and where Qt's `GridLayout` or `Grid` places apart (§4.6) |
| `StackLayout` | `Pages` | mechanical; kept when its attached object is read |
| `Flow` | `Flow` | mechanical; kept for a `Layout.*` or anchored child, or a transition |
| `LayoutItemProxy` | a Box whose `direction` is bound | DECISION: kept; a person merges the layouts |
| `anchors.*` on a free parent | the same | none |
| `anchors.*` on a layout child | `Layout.alignment`/margins | DECISION: kept (Qt overrides them at run time, so their effect is unclear) |
| `MouseArea` filling its parent, no children | `Tap` and `Hover` | mechanical where it only clicks and asks for a cursor; a `PointerArea` when something before it takes the pointer, `z` is set, it drags, hovers, reads `mouse` or has its `id` read (§4.7) |
| other `MouseArea` | `PointerArea` | mechanical (phase 1), gestures M3 |
| `*Handler` | `Tap`, `Drag`, `Pointer`, `Pinch`, `Hover`, `Wheel`, `LongPress` | mechanical, by member (§4.7); `Tap`, `Hover`, `Drag`, `Pointer` and `Wheel` built |
| `PinchArea`, `MultiPointTouchArea` | `Pinch`, gestures | DECISION: kept until M3 |
| attached `Drag`, `DropArea` | `DragSource`, `DropArea` | mechanical; built |
| `Flickable` | `Flickable` | none |
| `Image`, `Window`, `Screen`, `TextInput`, `TextEdit`, `TextMetrics`, `FontMetrics`, `Shortcut`, attached `Keys` and `Accessible` | the same | none; a `QtQuick.Window` import leaves its line empty, since its types are `QtQuick`'s |
| `ListModel` of constants | `list<T>` | mechanical, with a generated row type in `<File>.rows.ts` (§4.8). Built |
| `ListModel` edited | `ListModel` | none |
| `XmlListModel`, `FolderListModel` | `XmlModel` (decoder), `FolderModel` | mechanical |
| `model.<role>` | `modelData.<role>` | mechanical |
| `DelegateModel`, `ObjectModel`, `Package` | — | DECISION: kept (0–1 uses) |
| `State`, `PropertyChanges { target: }` | `PropertyChanges { id.prop: }` | mechanical when the target is an id; another target stays; a group or attached member set through a target keeps the file (§4.9) |
| `StateGroup` | the same | none |
| `ParentChange` | — | DECISION: kept (0 uses) |
| `Loader { source: "X.qml" }` | `Loader { component: X }` | mechanical when the string is static and `X.qml` is beside the file; built |
| `Loader { sourceComponent: c }` | `Loader { component: c }` | mechanical; built |
| `Qt.createComponent(...).createObject` | `X.createObject` | mechanical when the string is static |
| `createQmlObject`, `destroy()` | — | DECISION: kept (R1) |
| `Component { id }` | an inline `component X:` or an inline delegate | mechanical |
| `palette.*` | `Theme.color.*` | mechanical where 1:1 |
| `Material.*`, `Universal.*` | — | DECISION: kept (a style's own API) |
| `font.pixelSize`, `font.bold` | `font.size`, `font.weight` | mechanical; `font.bold` lowers to `Font.Bold`/`Font.Normal`; kept when a script reads or writes `.bold`, both `bold` and `weight` are set (in any form), or a font member is read off a `font`, `var` or `alias` whose type is not known; built |
| `font.pointSize` | `font.size: Screen.points(…)` | mechanical (a run-time conversion); a script's read or write keeps the file |
| `horizontalAlignment`, `wrapMode`, `elide`, `textFormat` (on `Text`) | `align`, `wrap`, `elide`, `format` | mechanical; enum values keep Qt's names until M3; built |
| `fillMode` (on `Image`), and `TextInput`'s and `TextEdit`'s text members | `fit`, and `Text`'s names | mechanical once the type is in `Ljos.Ui` |
| `Qt.*` globals | `Ljos.Core`, `App`, value types (§4.3) | mechanical, by member |
| `date`, `Date` | `Instant`, `Locale` | mechanical when only stored or compared, else kept |
| `Screen.primaryOrientation` | the window's aspect | DECISION: kept; the meaning differs on desktops |
| `Screen.pixelDensity` | — | DECISION: kept; it is physical pixels per mm, not the logical DPI `Screen.points` uses, and the corpus has no use of it |
| `property var` | `property unknown`, or a type | mechanical |
| `Qt.labs.platform`, `StandardPaths` | `Ljos.Platform` | mechanical |
| `WorkerScript` | — | DECISION: kept (0 uses); a TS worker module later |
| `Canvas`, `Shape` | the same | none; `QtQuick.Shapes` import lowered (§4.2) |
| `QtQuick.Controls.Basic` | `Ljos.Controls.Basic` | mechanical, for the controls the module has (§4.16) |
| `QtQuick.Controls` | `Ljos.Controls.Basic` | mechanical where the program starts with Basic; kept with another style (§4.16) |
| `QtQuick.Controls.Material`, `.Fusion`, `.Universal`, `.impl`, `QtQuick.Templates` | — | DECISION: kept (a style's own API, or the parts a style is built from) |
| `QtQuick.Dialogs` | `Ljos.Controls.Basic` | mechanical for `FileDialog` and `FolderDialog`; kept for the others (§4.16) |

### 4.15 What this changes for the engine

- **Box** is one native arrangement with the positioner fast path. It is the
  D1 spike.
- **Gestures** are recognisers on one pointer stream, attached to an item's
  hit test. They have no items and no geometry.
- **Keyed views** diff a list by key. `enter`/`exit` live on the view, which
  keeps an exiting delegate until its transition ends. Built
  (`features/views/keyed-views.feature`): a `Repeater` or `ListView` given
  a `key` holds its rows as a model of its own
  (`crates/ljos_engine/src/keyed.rs`), and a new list is told to it as the
  rows removed, moved, inserted and changed, so a row that stays keeps its
  delegate and reads its new fields. The rows of a longest run already in
  the new order stay where they are and only the others are moved, so a
  row taken from the front to the end is one move, not one per row it
  passes. A keyed `Repeater`'s `enter` runs on
  the item of a row inserted once it has made its first items, and its
  `exit` on the item of a row removed, which stays, out of `count`, until
  the transition ends. Each item has its own run of the transition, so
  rows that come or go in one change each play it to the end; ended on
  the way, an `exit` still lets its item go. A row removed on its way
  in ends its `enter` where it is and plays its `exit`. `Ljos.Ui`'s `ListView` calls Qt's `add` and
  `remove` `enter` and `exit`. The scenarios of `enter` and `exit` in a
  Ljós file wait for `Transition` in `Ljos.Ui`.
- **Clock** is one timer per resolution, scheduled by its readers. A
  reader whose only effect is drawing does not count while it is hidden;
  a handler, a `when:` or a non-visual object always counts, so a hidden
  page's timeout still fires. With no reader that counts, there is no
  timer (rule 6).
- **Decoders** run at a model's boundary in tier 2. The rows they return
  are values.
- **Theme** is a singleton with a live-reloaded source.

### 4.16 Controls

A file in Ljós's API imports its controls from `Ljos.Controls`, and gets
Ljós's own style: compact controls (28 px high, 13 px type, a radius of 8)
drawn with the roles of one `Theme` singleton:

```qml
import Ljos.Ui
import Ljos.Controls

Item {
    Button { text: "Save"; ToolTip.text: "Writes the file"; ToolTip.delay: 500 }
    Button { text: "Send"; primary: true }
    Label { text: "Ready" }
}
```

The style has `Button` (`primary`, `destructive`, `subtle`, `chevron`,
`iconName`), `Switch`, `CheckBox`, `ComboBox` (`outline`), `TextField`,
`SpinBox`, `Slider`, `Menu`, `MenuItem` (`shortcut`, `iconSpace`,
`destructive`, `iconName`), `MenuSeparator`, `Label`, `ToolTip` and `Icon`
(Lucide's marks, under Lucide's ISC licence). It has every control
`Ljos.Controls.Basic` has, so a file moves from one to the other by its
import alone: `TextArea`, `Dialog`, `ItemDelegate`, `Popup`, `ScrollBar`,
`ScrollView` and the K6 controls below are Basic's until the style draws
its own, and `FileDialog`, `FolderDialog`, `AbstractButton` and
`ButtonGroup` are the same types in both.

The theme's roles are `canvas`, `surface`, `surfaceRaised`,
`surfaceOverlay`, `input`, `border`, `text`, `textMuted`, `placeholder`,
`iconMuted`, `accent`, `accentForeground`, `accentSurface`, `accentHover`,
`accentPressed`, `focus`, `link`, `errorForeground`, `warningForeground`,
`destructive`, `destructiveForeground`, `destructiveHover` and
`destructivePressed`. All are derived from three:
`canvas`, `text` and `accent`, which follow `Ljos.Design`'s `Colors` in
light and dark. The fills are the canvas mixed toward the text, each only
as far as the text keeps 4.5:1 on it (`lift`), so a text only just
legible on the canvas leaves room for the roles reached toward it; each role
that is read is then moved toward the text by `reach(from, to, against,
ratio)` until it meets its WCAG ratio against every fill it is drawn on:
3:1 for `input` on every ground, 3:1 for `focus` on every fill (a
highlighted row's `accentSurface` too), 4.5:1 for `link` on every ground,
4.5:1 for `textMuted` and `placeholder`, 3:1 for `iconMuted`, 4.5:1 for
the error and warning colours, and `accentForeground` is black or white,
whichever is further from the accent. A filled button's hovered and
pressed fills are moved away from its text (`shade`), so a state only adds
contrast, and a checked button has a ring in its text's colour inside its
edge, so its state is not its colour alone; a checkable one is padded
for the ring whether checked or not. `ljos_design::roles` derives
the same in Rust; its `PROMISES` list every pair of roles the style draws
one on the other, and its tests hold every palette of `Ljos.Design`, and
apps' own three colours, to them. Every role is a
property an app may set (`Theme.input = "#b00020"`); a role set is the one
drawn, and the others still follow the three.

Motion is `Ljos.Design`'s `Motion`: a pressed button gives a little
(`Motion.pressScale`), a switch's thumb slides in `Motion.moveSmall` and a
menu fades in. With reduced motion nothing moves and a change still shows:
the button keeps its size and its fill says it is pressed, and the thumb is
at its end at once while the track's colour still fades. Each control has
a ring of the `focus` role where a key brought the keyboard, and the style
passes `ljos_audit` in light and dark.

A file that wants Qt's Basic look imports `Ljos.Controls.Basic`. It grows a
slice at a time. Each control in it is the Basic
style's, as `QtQuick.Controls.Basic` has it: the same style file over the
same template, made, drawn and taking input as Qt's, so a lowered file and
the file as written are the same program and draw the same pixels. What
the module has is in `ljos.txt` (written by the own_types test), and the
compat pass lowers a file only when every control it names is there.

| Slice | Controls | State |
|---|---|---|
| K1 | `Label`, `AbstractButton`, `Button`, attached `ToolTip`, `ScrollBar` (and its attached form), `ScrollView`, `Popup` | built |
| K2 | `TextField`, `TextArea` | built |
| K3 | `Menu`, `MenuItem`, `MenuSeparator`, `Dialog`, `ItemDelegate` | built |
| K4 | `CheckBox`, `Switch`, `ComboBox`, `SpinBox`, `Slider` | built |
| K5 | `FileDialog`, `FolderDialog` (`QtQuick.Dialogs`') | built |
| K6 | `ApplicationWindow`, `Control`, `Pane`, `Frame`, `GroupBox`, `Page`, `ToolBar`, `ToolButton`, `RoundButton`, `DelayButton`, `ProgressBar`, `BusyIndicator`, `TabBar`, `TabButton`, `SwipeView`, `RadioButton`, `ButtonGroup` | built |

Lowering (§6.1): `import QtQuick.Controls.Basic` becomes
`import Ljos.Controls.Basic` on its line, and a second import of the controls
becomes an empty line. `import QtQuick.Controls` is the style the program
starts with (`QT_QUICK_CONTROLS_STYLE`, then the file
`QT_QUICK_CONTROLS_CONF` names, then the platform's: Fusion on Linux), so
it is lowered only where that is Basic. `ljos migrate --style NAME` says
what the program starts with; without it, migrate takes what a program
started there would get. A control the module has not taken yet, or one of
its enums (`Dial.SnapAlways`), keeps the file with "`Dial` is not in
Ljos.Controls.Basic yet". A `Rectangle` given to a control (`background:`) is
still a `Rectangle`, so its `color` becomes `fill`; a member read off what
the style gave (`b.background.color`) keeps the file, because that may be
a `Rectangle`.

Cost: `Ljos.Controls.Basic` costs nothing at run time: it registers the
files the Basic style already compiled, under a second name. Ljós's style
is compiled with the controls; its `Theme` is one object, and a role is a
binding that is evaluated again only when one of the three it reads
changes.

The file and folder dialogs are `QtQuick.Dialogs`' own types, registered
in `Ljos.Controls` and `Ljos.Controls.Basic` under a second name: the system's dialog where it has
one, else the dialog Ljós draws. `import QtQuick.Dialogs` is lowered as
one more import of the controls: it becomes `import Ljos.Controls.Basic` on its
line when the file imports no other controls, and an empty line when it
does. `ColorDialog`, `FontDialog` and `MessageDialog`, and their enums
(`MessageDialog.Ok`), keep the file with "`ColorDialog` is not in
Ljos.Controls.Basic yet"; the dialogs under a name (`as D`) keep it as an aliased
controls import does. Since the lowered file imports `Ljos.Controls.Basic`, a file of
the project named as one of its types (`Button.qml` beside a file of
dialogs) keeps the file (§6.3).

DECISION: the dialogs come into the controls as they are, not into a
module of their own; Qt's names stay, since §4.14 renames none of them.

DECISION: `ApplicationWindow` is in `Ljos.Controls.Basic` and
`Ljos.Controls`, as Basic's file, not in `Ljos.Ui` beside `Window`: what
it adds to a window (the header, footer and menu bar laid out around its
content, and the font and palette its controls take) is the controls',
and its background is the style's. A window without controls is
`Ljos.Ui`'s `Window`. Its `color` is a window's, not a fill, as
`Window`'s is (`rules/window.rs`).

DECISION: a file chooses its style by its import, and nothing else:
`import Ljos.Controls` is Ljós's style, `import Ljos.Controls.Basic` is
Basic's. There is no style setting to read at start, so a file draws the
same wherever it runs, and a tool reads the style off the file. The compat
pass and `ljos migrate` lower Qt's Basic controls to
`Ljos.Controls.Basic`, so a lowered file keeps the look it had; moving it
to Ljós's style is one edit, its import.

DECISION: the roles' fills lift toward the text in both schemes (a light
canvas darkens, a dark one lightens), so `surfaceRaised` is never nearer
the canvas than `surface`, and one derivation serves light and dark.

DECISION: the controls keep Qt's member and enum names
(`ScrollBar.AlwaysOn`, `Label.color` as the glyph colour, as on `Text`)
until M3's enum cleanup and §4.13's text renames reach them.

DECISION: `Ljos.Controls` does not bring `Ljos.Ui` with it; a file imports
both, as a Qt file imports `QtQuick` and the controls.

DECISION: a controls import under a name (`as C`) keeps the file, as an
aliased `QtQuick` does.

Not yet: in a file in Ljós's API (checked, no `pragma Scripts:
JavaScript`), the checker does not follow an attached object read off
another object (`save.ToolTip.text`). Read on the object itself
(`ToolTip.text` inside the `Button`), it checks. The generated `.d.ts`
(§4.4) covers `Ljos.Ui` only.

`TextEdit`, `TextInput`, the validators (`IntValidator`,
`DoubleValidator`, `RegularExpressionValidator`) and `Accessible` are in
`Ljos.Ui` as they are (§4.14). So a text area's `wrapMode: TextEdit.Wrap`,
a text field's `echoMode: TextInput.Password` and its `validator:` lower
with the file, and in a file in Ljós's API the a11y rule's
`Accessible.name` remedy checks.

Not yet: an `ItemDelegate`'s `implicitHeight` is 40 in Ljós and 42 in
Qt. Qt's is its line of text (18) plus 24 of padding; Ljós's line is
shorter, so the background's 40 is the larger. It is the same in a lowered
file and in the file as written, so lowering does not change it.

### 4.17 Ergonomics: what this part adds

Sections 4.17 to 4.21 are the ergonomics amendment (2026-11-12). They
take ideas from functional programming (pattern matching, pipes), from
React's hooks and from lit.dev's reactive controllers and `@lit/task`,
and fit them to Ljós's one reactive system: properties and bindings.

| Slice | Builds | Section |
|---|---|---|
| **S1** | no `Qt` in a file of Ljós's API; every `Qt` member and Qt-named enum given a Ljós home; `ljos migrate` rewrites each | §4.3.1 |
| **S2** | `fetch`, `Request`, `Response`, `Headers`, `AbortController`, `AbortSignal`, on `Platform::request`, with a reply that wakes the engine and nothing that polls | §4.19 |
| **S3** | `Controller`: a non-visual object with a host and a lifecycle | §4.20 |
| **S4** | `Task`, a Controller that runs async work from its arguments | §4.19 |
| **S5** | `match` and `is` | §4.18 |
| **S6** | the pipe, `\|>` | §4.17.1 |
| **S7** | the desugar for `ljos check --tsc`, the MCP tools and `ljos lsp` | §4.21 |

They merge in that order. Each is its own PR, with its features, a
`docs/site/` page in the new dialect, its `ljos migrate` coverage and its
`api_status` rows (§4.21).

Five handlers from the corpus are rewritten here, before the API is
locked, to test it against real code: `todolist`'s request (§4.19),
`lightningviewer`'s button look and its permission switch (§4.18),
`rssnews`'s "how long ago" (§4.17.1) and `osmbuildings`' camera
controller (§4.20).

#### 4.17.1 The pipe: `|>`

**What apps say.** "Take this, then do that to it, then that." The corpus
writes it inside out, so the first step is read last:

```qml
// Qt (toycustomizer/qml/OrderItem.qml)
property real __discountPercent: Math.round(Math.max(oldPrice - newPrice, 0) / oldPrice * 100)

// Ljós
readonly property real discountPercent:
    oldPrice - newPrice |> Math.max(%, 0) / oldPrice * 100 |> Math.round(%)
```

**The design.** The pipe is the Hack-style pipe of TC39's proposal, as
the proposal specifies it at commit
[`5ac4dfd`](https://github.com/tc39/proposal-pipeline-operator/tree/5ac4dfd624e68b649e6f947885bf5ef67bac19f4)
(2023-11-30, stage 2), with `%` as the topic:

- `head |> body` evaluates `head`, then `body` with `%` bound to its value.
  The body is any expression, so a step can call a function (`f(%, a)`),
  a method (`%.trim()`), an operator (`% * 2`), `await %`, or build a
  value (`[%, %]`, `` `${%} items` ``).
- The body must use `%`; one that does not is a syntax error
  (`` the pipe's body does not use its topic `%` ``).
- `%` outside a pipe body is a syntax error.
- The pipe has the precedence of `=>` and assignment, and chains left to
  right: `a |> f(%) |> g(%)` is `g(f(a))`.
- A body that is an unparenthesised arrow function, conditional
  (`?:`), assignment or `yield` is a syntax error; parenthesised, it is
  allowed: `x |> (% > 0 ? % : 0)`.
- A function or arrow inside a body may read `%`, which is the topic of
  the pipe around it. A pipe inside a body has its own topic for its own
  body; its head reads the outer one.
- `%` in operand position is the topic; in operator position it is the
  remainder, as in `% % 2`.

The proposal's later ideas (`+>`, `|?>`, `|>>`) are not in it, so Ljós
has none of them.

`rssnews`' delegate shows how long ago an article came out. Qt's version
rebuilds the date string by hand and threads `result` through nested
`if`s. The Ljós version pipes the minutes into a `match` whose arms are
guards, Elixir's `cond` in TC39's form (§4.18); `Instant` and `Clock` are
§4.3's:

```qml
// Qt (rssnews/content/NewsDelegate.qml, abridged)
function timeSinceEvent(pubDate) {
    var result = pubDate;
    var d = pubDate.replace(',','').split(' ');
    if (d.length != 6)
        return result;
    var date = new Date([d[0], d[2], d[1], d[3], d[4], 'GMT' + d[5]].join(' '));
    if (!isNaN(date.getDate())) {
        let age = new Date() - date;
        let minutes = Math.floor(Number(age) / 60000);
        if (minutes < 1440) {
            if (minutes < 2)
                result = qsTr("Just now");
            else if (minutes < 60)
                result = '' + minutes + ' ' + qsTr("minutes ago")
            else if (minutes < 120)
                result = qsTr("1 hour ago");
            else
                result = '' + Math.floor(minutes/60) + ' ' + qsTr("hours ago");
        }
        else {
            result = date.toDateString();
        }
    }
    return result;
}

// Ljós
function timeSinceEvent(pubDate: string): string {
    const published = Instant.parseRfc2822(pubDate)
    if (published === undefined) return pubDate
    return Clock.now(Clock.Resolution.Minute).since(published).minutes |> match (%) {
        when if (% < 2):    qsTr("Just now");
        when if (% < 60):   qsTr("%1 minutes ago").arg(%);
        when if (% < 120):  qsTr("1 hour ago");
        when if (% < 1440): Math.floor(% / 60) |> qsTr("%1 hours ago").arg(%);
        default:            Locale.current.formatDate(published, Locale.Format.Long);
    }
}
```

The Ljós version is also reactive: it reads `Clock.now` once a minute
(`Clock.Resolution.Minute` joins §4.3's `Second`), so the delegate's text
keeps itself current where Qt's is computed once.
And the translated strings now carry their number (`%1`), which Qt's
concatenation could not give a translator.

**Lowering.** The pipe is compiled away (§4.21): the program that runs
has no pipe in it. A chain is one arrow, called where it is written, whose
head and steps are `const`s the next step reads:

```js
// written
oldPrice - newPrice |> Math.max(%, 0) / oldPrice * 100 |> Math.round(%)
// what runs (tier 2, tsc and ljos-web read this)
(() => { const $0 = oldPrice - newPrice; const $1 = Math.max($0, 0) / oldPrice * 100; return Math.round($1);})()
```

The parts keep their order and their lines, so the lowered text reads as
the source does and a position in it maps back to the `%` it stands for.
A topic is named by how many bodies deep it is, so a pipe inside a body
declares a name its head does not read. The names the lowering declares,
the topics and a `match`'s `$s`, start with one `$` more than any name
the file writes, so a body never reads one of them in place of the
file's own: in a file that has a `$0`, the topics are `$$0`, `$$1`, ….
A pipe that awaits is an `async`
arrow, awaited; one that yields is a generator delegated to, as a `match`
is (§4.18). A pipe that starts a statement is written after a `;`, so it
never calls the line before it. Tier 1 compiles an arrow called where it
is written in place: the topics are registers, and nothing is made or
called.

**Types.** The checker types `%` as the head's type and each step as its
body's, so `"3" |> Math.max(%, 0)` is `ts-assign-type` at the `%`.

DECISION: the pipe follows TC39's Hack proposal exactly, as it stands at
commit `5ac4dfd624e68b649e6f947885bf5ef67bac19f4` (2023-11-30): `%` as
the topic, the proposal's early errors, and none of its deferred
operators. Where the proposal changes, Ljós moves the pin and follows it
(R12).

DECISION: a chain compiles to one arrow with a `const` for each step,
line for line, not to an arrow for each step and not to temporaries
hoisted into the function: each evaluation has its own `const`s, so a
closure that reads the topic keeps the value it saw, and one closure
stands for the whole chain. The budget gate (§11) measures the cost, and
tier 1 removes it.

### 4.18 `match` and `is`

**What apps say.** "Which of these is it?" The corpus says it with
`switch` on a status, chains of `if`/`else if` on a number, and
`[a && "x", b && "y"].filter(Boolean).join("_")` for combinations.

**Where it comes from.** Four languages were read for this section, as
they stand on 2026-11-12: ReScript's `switch`, Elixir's `case` and
`cond`, TC39's pattern-matching proposal (stage 1, at commit
[`ba73ffe`](https://github.com/tc39/proposal-pattern-matching/tree/ba73ffe42f30f0cf56ece28bebdc28f955f2a52e),
2025-10-03), and Go. The syntax is TC39's, exactly, since a Ljós script
is TypeScript and adds nothing TC39 has not proposed (§1). The others
decide what the checker holds a `match` to and how it says so:

| Idea | ReScript | Elixir | TC39 | Go | Ljós takes |
|---|---|---|---|---|---|
| The form | `switch x { \| p => e }`, an expression | `case x do p -> e end`, an expression | `match (x) { when p: e; default: e; }`, an expression | `switch`, a statement; no patterns | TC39's form, as an expression that may stand as a statement |
| No case matches | cannot happen: the compiler warns first | `CaseClauseError` | `TypeError` | nothing runs | TC39's `TypeError`, which the checker makes unreachable: a `match` never gives `undefined` for want of a case |
| Exhaustiveness | checked; the warning shows a case left out ("You forgot to handle a possible case here, for example: …") | not checked | not specified ("a development-time error is less costly than a production-time error") | not checked (an `exhaustive` linter exists outside the language) | checked, as ReScript does, and the message shows a case left out (below) |
| A catch-all | `_`, with the advice not to use it at the top, since it turns the check off | `_` | `default:` | `default:` | `default:`, and the same advice in the site's page: on a union, leaving `default` out is what lets the checker find a new member |
| Literals | yes | yes | `SameValue`, but the pattern `0` matches `-0` too | `case 1, 2:` compares with `==` | TC39's |
| Nested destructuring | yes | yes | array and object patterns, nested | none | TC39's |
| Or-patterns | `\| A \| B =>`, also inside a record | none (one clause per pattern, or a guard with `or`) | `or`, anywhere in a pattern, with `and` and `not` | `case a, b:` | TC39's `or`, `and`, `not` |
| Guards | `\| p if cond =>` | `p when cond ->`; an error in a guard fails the guard | `p and if (cond)`: a guard is a pattern | none | TC39's guard pattern; an error in a guard is an error, as TC39 has it, and the checker requires a `boolean` (`ts-condition-boolean`) |
| Comparing with a variable | a name always binds, so comparing needs a guard | a name binds; the pin `^x` compares | a name compares (by `SameValue`, or calls it if it is a function); `let x` binds | `case x:` compares | TC39's: a bare name compares, as Elixir's pin does, and `let`/`const` binds |
| Type tests | variant tags | `when is_binary(x)` | a predicate function, or `String`, `Number`, `Boolean` as a pattern | the type switch, `switch v := x.(type)` | TC39's predicates and primitive constructors; a type guard (§4.4) as a pattern narrows |
| A chain of conditions | `if`/`else` | `cond`, which raises `CondClauseError` when none holds | `when if (cond):`, a guard alone | the tagless `switch { case c: }` | TC39's guard alone, `when if (cond):`, with no form of its own (DECISION below) |
| Destructuring in `let` and parameters | yes | `=` is a match | JavaScript's | none | JavaScript's, as it is; a test that can fail is `is` (below) |

Go was checked because a change there was remembered: Go has added no
pattern matching or destructuring. The proposals for sum types
(golang/go#19412, #57644, #76949, the last with a match statement) are
open; the one for limited pattern matching in error handling (#65266)
was closed with Go's 2025 decision to add no new error-handling syntax.
Go's tagless `switch` is its `cond`, and its type switch its type test;
neither is exhaustive.

**The design.** `match` is the expression of TC39's proposal:

```js
match (subject) {
    when pattern: expression;
    when pattern and if (condition): expression;
    default: expression;
}
```

The arms are tried in order, and the first whose pattern matches the
subject gives the value. With no match and no `default`, it throws a
`TypeError`, as the proposal says; the checker makes that unreachable
(below). Each arm is a block scope of its own: what its pattern binds is
seen by its condition and its expression, and by nothing outside.
Patterns:

| Pattern | Matches | Example |
|---|---|---|
| literal | the same value, by `SameValue` (`NaN` matches `NaN`), except that the pattern `0` matches `-0` too; `+0` and `-0` match only themselves. No coercion: `1` does not match `"1"` | `when "checked":`, `when 0:`, `when true:`, `when undefined:`, `when NaN:` |
| variable | a name, `a.b` or `a[b]` in scope: an object with a `[Symbol.customMatcher]` method is asked, with the hint `"boolean"`; a function is called with the subject and matches if it returns `true`; anything else matches the same value (`SameValue`) | `when Permission.Status.Granted:`, `when selected:`, `when isRow:`, `when Even:` |
| extractor | what a custom matcher or function, called with the hint `"list"`, takes apart: it matches where the call gives a list (not `false`) whose items match the patterns, as an array pattern's do | `when Point(let x, 0):`, `when Option.Some(let v):`, `when String(let s):` |
| primitive type | a value of that type | `when String:`, `when Number:`, `when Boolean:` |
| regular expression | a string the expression finds a match in | `` when /^https?:/: `` |
| binding | anything, and names it | `when let n:`, `when const n:` |
| array | an array of exactly this length; `...` or `...let rest` for "at least" | `when [let x, let y]:`, `when [0, ...let rest]:` |
| object | an object with these keys, each matching its pattern; `key` alone tests that the key is there, `let key` binds it | `when { status: 200, let body }:` |
| `and`, `or`, `not` | both, either, neither; mixing two needs parentheses | `when 301 or 304:`, `when not undefined:`, `when { status: "done" or "failed" }:` |
| guard | anything, where the condition is `true` | `when let n and if (n > 0):`, `when if (minutes < 2):` |

A bare name in a pattern compares; it never binds. That is Elixir's pin
(`^x`) by default, and the reverse of ReScript, where `x` binds and
comparing needs a guard. A name that is not in scope is an error, whose
message gives the fix for the other reading: `` `x` is not defined; to
bind the value, write `let x` ``.

`x is pattern` is the same test as a boolean. It narrows `x` where it is
`true`, and what it binds is in scope in the block around it, as the
proposal has it:

```ts
if (reply is { status: 200, let body }) {
    show(body)
}
```

A read of a binding where the checker cannot prove the `is` was `true`
(in the `else`, or after the `if`) is an error, `ts-is-binding`; in
JavaScript it would throw a `ReferenceError`.

**Custom matchers and extractors.** An object literal may have a method
whose key is `Symbol.customMatcher`, one of the three symbols a script
names (§5.1). A pattern that names the object asks it, as the proposal's
`InvokeCustomMatcher` does: it is called with the subject and a hint,
`"boolean"` where the pattern is the bare name and `"list"` where it is
an extractor. `false` is no match; for the hint `"list"`, anything else
must be iterable, or it is a `TypeError`, and its items are matched as
an array pattern's. As an extractor, a function in place of the object
is called with the subject and `"list"`, so a script's predicate can be
one too; as a bare name it is called with the subject alone:

```ts
const Point = {
    [Symbol.customMatcher](subject: unknown, hint: string): false | number[] {
        return match (subject) {
            when { x: Number and let x, y: Number and let y }: [x, y];
            default: false;
        }
    },
}

const where = match (p) {
    when Point(let x, 0): `on the x axis at ${x}`;
    when Point(...let xy): `at ${xy}`;
    default: "nowhere";
}
```

The checker types an extractor's items by what the matcher returns,
less `boolean`; one with no custom matcher and that is not a function is
an error at the pattern, whose message gives the fix. An extractor arm
removes nothing from its subject's type, as a predicate's does, so a
`match` with one needs a `default` unless the other arms cover the
subject (`ts-match-exhaustive`).

What Ljós leaves out of the proposal, and why:

- **A class as a pattern**, and a class's custom matcher. There are no
  classes (`ts-forbidden`); a plain object's matcher does what a class's
  static one would.
- **A regular expression as an extractor** (`when /(\d+)/(let n):`), and
  the matchers the proposal gives `RegExp`, `Array`, `Map`, `Set`,
  `Promise`, `Error` and the typed arrays. The primitive constructors'
  are in (`when String(let s):`); the rest are a predicate and a binding.
- **`var` bindings** (`ts-no-var`) and `null` patterns (`ts-no-null`), as
  everywhere in a script. `let` and `const` bind alike, since a binding
  in a pattern is never written.
- **The proposal's possible future enhancements**: relational patterns
  (`when < 10:`), `void` patterns, `async match`, defaults, chained
  guards and `catch` integration. They are not in the proposal, so Ljós
  has none of them; a guard says what a relational pattern would.

**Exhaustiveness.** A `match` with no `default` is accepted only where
the checker proves it covers its subject (`ts-match-exhaustive`, §5.1):
the literals of a union of literals (`boolean` is `true | false`), the
keys of a plain enum (§4.3.1; an enum of flags is not a union), and the
tags of a union of records discriminated by one literal key (`{ kind:
"a", … } | { kind: "b", … }`). Each arm narrows the subject for its
condition and its expression, and removes from the subject's type, for
the arms after it, what its pattern matches whole. An arm with a guard
or a predicate removes nothing, since the checker cannot know when the
condition holds. On any other subject (a `number`, a `string`, an enum of
flags) a `match` needs a `default`, and the message says so: `` `match`
on `number` needs a `default:` ``. This is stricter than the proposal,
whose `match` throws at run time; Ljós makes that throw unreachable, as
it does a `switch`'s missing case.

The message shows a case left out, as ReScript's does, written as a
pattern that would cover it:

```text
Main.qml:14:11: error: `match` does not cover every case of `weather.state`; for example, this case is not handled: `when { status: "failed" }:` [ts-match-exhaustive]
```

DECISION: the message is one line, as every finding `ljos check` prints
is, so that tools that read findings line by line read this one too.

**As ReScript and Elixir write it.** A `Task`'s state (§4.19) taken
apart, with nesting, `or`, a guard and no `default`, so that the checker
holds it to every status:

```qml
Text {
    text: match (feed.state) {
        when { status: "initial" or "pending" }:                      qsTr("Loading…");
        when { status: "done", value: [] }:                           qsTr("Nothing new");
        when { status: "done", value: [let only] }:                   only.title;
        when { status: "done", let value } and if (value.length > 99): qsTr("99+ new");
        when { status: "done", let value }:                           qsTr("%1 new").arg(value.length);
        when { status: "failed", error: { name: "AbortError" } }:     "";
        when { status: "failed" }:                                    qsTr("Could not load");
    }
}
```

The fifth arm has no guard and matches every `done` state whole, so after
it `done` is covered; the last arm covers `failed`. Leave it out, and the
message above names `{ status: "failed" }`.

Comparing with a value in scope, Elixir's pin, is a bare name:

```qml
// Elixir: case row do %{key: ^selected} -> :highlight; … end
color: match (row) {
    when { key: selected }:                             "steelblue";
    when { let key } and if (key.startsWith("draft/")): "gray";
    default:                                            "black";
}
```

A chain of conditions with nothing to take apart, Elixir's `cond` and
Go's tagless `switch`, is a `match` whose arms are guards alone, on the
value they test. `default:` is `cond`'s `true ->`, and since guards
remove nothing, the checker asks for it (`osmbuildings`' camera, §4.20):

```qml
readonly property real clipNear: match (camera.z) {
    when if (camera.z < 1):   0.01;
    when if (camera.z < 100): 0.1;
    default:                  1;
}
```

`lightningviewer`'s button picks an image for its look from four flags,
by joining the names of those that hold:

```qml
// Qt (lightningviewer/LightningViewerStyle/Button.qml)
readonly property string __currentState: [
    control.checked && "checked",
    !control.enabled && "disabled",
    control.enabled && !control.down && control.hovered && "hovered",
    control.down && "pressed"
].filter(Boolean).join("_") || "normal"

// Ljós
readonly property string look: match (control) {
    when { enabled: false }: "disabled";
    when { down: true }:     "pressed";
    when { hovered: true }:  "hovered";
    default:                 "normal";
} |> (control.checked ? (% === "normal" ? "checked" : `checked_${%}`) : %)
```

The `match` says that the four looks exclude one another and in which
order they win, which the Qt version leaves to the reader. (Qt's version
can also give `disabled_pressed`, for which the style has no image; the
match cannot.)

`lightningviewer`'s view asks for the location permission when it has
none:

```qml
// Qt (lightningviewer/LightningViewerModule/LightningView.qml)
onRecenterRequested: {
    switch (permission.status) {
    case Qt.Granted:
        mapView.recenter();
        break;
    case Qt.Denied:
        permissionMessageDialog.open();
        break;
    default:
        permission.request();
        break;
    }
}

// Ljós
onRecenterRequested: match (permission.status) {
    when Permission.Status.Granted:     mapView.recenter();
    when Permission.Status.Denied:      permissionMessageDialog.open();
    when Permission.Status.Undetermined: permission.request();
}
```

`Qt.Granted` is the `Qt` global's (§4.3.1), and the Ljós file names the
enum it belongs to. With no `default`, the checker holds the `match` to
all three statuses, so a fourth added to `Permission.Status` is an error
here, where Qt's `default:` would quietly ask again. A `match` whose
arms are calls is a statement as well as an expression.

**Lowering.** `match` is compiled away (§4.21), line for line, to an
arrow of the subject made of `if`s, called at once:

```js
// written
match (permission.status) {
    when Permission.Status.Granted:     mapView.recenter();
    when Permission.Status.Denied:      permissionMessageDialog.open();
    when Permission.Status.Undetermined: permission.request();
}
// what runs
(() => { const $s = permission.status;
    if ($is($s, Permission.Status.Granted)) return mapView.recenter();
    if ($is($s, Permission.Status.Denied)) return permissionMessageDialog.open();
    if ($is($s, Permission.Status.Undetermined)) return permission.request();
    return $nomatch($s as never); })()
```

DECISION: the subject is the first statement of the arrow's body, so it
stays where it is written: the lowering keeps the source's order, a
finding in the subject is reported on its own line, and an `await` in
it is the arrow's own. A `match` or `is` that awaits, in its subject or
an arm, is an `async` arrow, awaited: `(await (async () => { … })())`.
The checker types `$s` as the subject, unwidened, and a `match` as the
union of its arms, as `?:` is: `match (thick) { when true: 3; when
false: 1; }` is `3 | 1`, so it goes where a `1 | 3` does.

DECISION: no arrow can `yield`, so a `match` or `is` that yields, in a
QML script of Qt's API, is a generator delegated to and handed `this`
and `arguments`: `(yield* (function* () { … }).apply(this, arguments))`,
an `async function*` in an `async` generator. `yield*` passes on each
value it yields and each sent back, and is what the generator returns:
the arm's value. A generator has its own `super` and `new.target`, so
one of them in a `match` or `is` that yields is an error that says to
read it into a name first. One that does not yield stays an arrow, which
shares them.

- A literal lowers to `===`, which is the proposal's test for every
  literal but `NaN`, `+0` and `-0`; those lower to `Object.is`.
- A variable pattern lowers to `$is($s, p)`, which does the proposal's
  test at run time: a custom matcher is called with the hint
  `"boolean"`, a function with the subject, and anything else is compared
  by `Object.is`. Which it is depends on the value, not the
  syntax, and the desugar reads only syntax. The checker knows the type,
  so tier 1 compiles an enum key to `===` and a predicate to a call.
- `String`, `Number` and `Boolean` lower to `typeof`; a regular
  expression to its `test`, on a subject the checker has typed `string`.
  A regular expression with the `g` or `y` flag is an error in a pattern:
  it keeps its place between tests, so the same string could match once
  and then not.
- An object pattern tests each key as the proposal's `HasProperty` does,
  and reads each value once in a `match`, as the proposal's caching has
  it. A binding lowers to a `const` in the arm's block.
- An extractor lowers to `$extract($s, p, n, reads)`, which calls the
  matcher with the hint `"list"` and reads what it gives into an array,
  or gives `undefined` for no match; its items are then tested as an
  array pattern's, on a temporary `$x0` that the arm's arrow (or, for
  `is`, the statement) declares. `n` is the count of item patterns and
  `reads` says how many items the proposal reads: `n + 1` with no rest,
  to find a list too long; `n` with `...`; all with `...let`. An
  iterator read short is closed at once.
- A guard lowers to its condition, after the arm's bindings.
- Where the subject is a local name, the desugar tests the name itself,
  not a copy, so `tsc` narrows what the arm reads as the checker does.
- `x is pattern` lowers to the same test, with its bindings declared at
  the start of the statement that holds it: a `let` of the block around
  it (a `var` pattern's, a `var`), declared once however many of the
  block's `is`es bind the name. A `switch`'s clauses are one scope that
  a jump enters at any clause, where a `let` written in one would be in
  its dead zone in the next; an `is` in a clause declares its names
  before the `switch`, its label too, in the block around it. A name the
  block declares itself (a module's imports too), or a parameter at a
  function's or a `catch`'s top, is `ts-is-binding` at the name the
  pattern binds, as the two would be one name. So is a name a scope
  between declares, which would take the assignment: the `switch`'s
  clauses, a `for` head around the `is`, a `catch` parameter whose
  default holds it, a function's parameters where it is in them or in an
  arrow's body that is not a block, and the names a `match` arm binds,
  where it is in the arm's guard or value. A `var` there clashes with
  what a `let` binds, as one beside does, and is one variable with what a
  `var` binds. Where scopes between nest, the innermost that declares the
  name is the one said.

`$is` is declared to `tsc` as a type guard of its subject (`$is<T>(s:
unknown, p: (v: unknown) => v is T): s is T`, and `$is<T>(s: unknown, p:
T): s is T` for a value), and as a plain `boolean` for an object with a
custom matcher. `$extract` is declared as giving the matcher's return
type less `boolean`, read as the array it is made into (an array or a
tuple as it is, another iterable object as an array of its items, and
nothing for a primitive, which the checker refuses as no list even where
it is iterable, as a string is), or `undefined`, and `[string]`, `[number]` or
`[boolean]` for the primitive constructors. TypeScript's lib has no
`Symbol.customMatcher` yet, so a file that names it, desugared or a
generated `.lts` declaration, gets `declare global { interface
SymbolConstructor { readonly customMatcher: unique symbol; } }` after its
last line, with `export {};` to make it a module. `$nomatch` throws the proposal's `TypeError`,
which the checker has made unreachable. The desugar writes it as
`$nomatch($s as never)`, declared `(x: never) => never`: TypeScript's
narrowing is weaker than the checker's on nested patterns and guards,
and H1 promises that `tsc` accepts every file the checker accepts
(§4.21), so the checker, not `tsc`, is what finds a missing case. The
cast is text the desugar wrote, to which `ts-no-cast` does not apply
(§4.21). A binding of an `is` that the pattern did not reach is
`undefined` in what runs, where JavaScript would throw; `ts-is-binding`
makes the difference unreachable.

DECISION: `match` and `is` follow the proposal at commit
`ba73ffe42f30f0cf56ece28bebdc28f955f2a52e` (2025-10-03), its README and
its spec text, and the proposal wherever it changes, by moving the pin
(R12): `when pattern:` arms
ended by `;`, a guard as the pattern `if (condition)` joined by `and`,
variable patterns that compare or call, the primitive constructors and
regular expressions as patterns, and bindings in `is` scoped to the
block around it, custom matchers on plain objects and extractors (since
2026-11-13). Classes' matchers, the built-in matchers but the primitive
constructors', and the proposal's possible future enhancements,
relational patterns among them, are not in it.

DECISION: an extractor follows `InvokeCustomMatcher` at the pin, with
five choices of its own. A function named bare is called with the
subject alone, as before matchers came: a predicate with an optional
second parameter would otherwise be given `"boolean"` for it, unseen by
the checker or `tsc`. The list is read into an array before its items
are tested, as the proposal's caching allows; an iterator read short is
closed at once. The receiver is the matcher object, and the proposal's
third argument, the receiver of a member path, is not passed, since a
script has no `this` to give it. A variable pattern that is neither a
matcher nor a function keeps `Object.is`, the proposal's `SameValue`.
The primitive constructors' matchers are the helpers' own, as QuickJS
has no `Symbol.customMatcher`; the prelude makes the symbol.

DECISION: `cond`-style chains get no form of their own. A guard alone,
`when if (condition):`, is TC39's own way to say one, `default:` is
`cond`'s `true ->`, and the checker's rule that guards remove nothing
gives Elixir's `CondClauseError` at check time, as an error that the
`match` needs a `default:`. The subject is the value the conditions
test, so that a reader sees what is being decided; `match (true)` is
allowed and means the same.

DECISION: exhaustiveness is checked as ReScript checks it, and the
message shows a case left out as a pattern; a `match` never gives
`undefined` for want of a case, as Elixir's never does.

DECISION: `match` is an expression and may stand as a statement, so it
replaces `switch` in a handler too. `switch` stays, with its own
exhaustiveness rule.

### 4.19 Fetching: `fetch` and `Task`

**What apps say.** "Get this from the network and show it, or say that
it failed." The corpus has one way: `XMLHttpRequest` with an
`onreadystatechange` that checks `readyState == 4` and the status by hand.

**`fetch`.** A file has the WHATWG `fetch` and the objects around it, as
browsers, Deno and Node have them, with the same names and meanings:

| | Ljós has |
|---|---|
| `fetch(input, init?)` | `input` a URL string or a `Request`; `init` with `method`, `headers`, `body` (a string, an `ArrayBuffer` or a `Uint8Array`) and `signal`. Gives a `Promise<Response>` |
| `Request` | `new Request(url, init?)`; `url`, `method`, `headers` |
| `Response` | `ok`, `status`, `statusText`, `url`, `redirected`, `headers`, `bodyUsed`; `json()` (a `Promise<unknown>`), `text()`, `arrayBuffer()`; the body is read once, as WHATWG says. `Response.error()`, a reply of no request: `status` 0, an empty body |
| `Headers` | `get`, `has`, `set`, `append`, `delete`, `forEach`, `entries`; names compared without case |
| `AbortController`, `AbortSignal` | `abort(reason?)`; `aborted`, `reason`, `throwIfAborted()`, `onabort`, `AbortSignal.timeout(ms)`, `AbortSignal.any(signals)` |
| `DOMException` | what an abort or a deadline rejects with: `name`, `message`, `code` |
| `ArrayBuffer`, `Uint8Array` | the language's own: a body of bytes, and what `arrayBuffer()` gives |

`json()` gives `unknown`, as `JSON.parse` does (§4.3), for a decoder to
narrow. A network failure rejects with a `TypeError`, an abort with the
signal's reason (a `DOMException` named `"AbortError"` when none is given),
as WHATWG says. What is not there, such as streams on `body`, `FormData`,
`Blob`, `credentials` and `mode`, is not in the declarations, so the
checker and `tsc` reject it rather than a run.

Under it is `Platform::request`, which `XMLHttpRequest` uses today: the
request runs on a thread of the platform, and its reply comes back as a
platform job through the bounded inbox (rule 7). The inbox wakes the
engine when its first job arrives, the frame delivers it before timers and
promise jobs, and the job settles the promise, whose reactions run in the
same turn. Nothing waits on a timer or polls. While a request is out, the
engine is idle: no frame and no timer deadline. A feature holds that
(S2). `Platform::request` gains a cancel token: an abort settles the
promise at once, and the request's thread stops at its next read instead
of finishing a reply nobody wants.

**`XMLHttpRequest`** stays for compat only. A file of Ljós's API does not
see it (the checker says `` use `fetch` ``). `ljos migrate` rewrites the
corpus's shape of it (an async `open`, `setRequestHeader`, an
`onreadystatechange` that tests `readyState` against 4 and then
`status`, reading `responseText` or `response`, and `send(body)`) into
`fetch` with `then`. Anything else (a synchronous `open`, `onprogress`,
`responseXML`, another `readyState`, the object stored or passed on) keeps
the file, with the reason and line.

**`Task`.** The usual job is "run this when these change, and show where
it got to". `Task`, after `@lit/task`, is that, declared:

```qml
Task {
    id: weather
    args: [city.text]
    run: async (name: string, signal: AbortSignal): Promise<Forecast> =>
        Forecast.decode(await (await fetch(`https://example.org/w/${name}`, { signal })).json())
}
Text {
    text: match (weather.state) {
        when { status: "initial" }:         "";
        when { status: "pending" }:         qsTr("Loading…");
        when { status: "done", let value }: value.summary;
        when { status: "failed" }:          qsTr("No forecast");
    }
}
```

- `run` is an async function of `args`' elements and then an
  `AbortSignal`. Its type gives the Task's: `args` is the tuple of its
  parameters before the signal, and `value` is what its promise gives.
- When `args` changes, by element with `!==`, the Task aborts the run in
  progress through its signal and starts another. The value of a run that
  was superseded is dropped, even when it comes back.
- `state` is where the Task got to, as one value a `match` can take
  apart: `{ status: "initial" } | { status: "pending"; value: T |
  undefined } | { status: "done"; value: T } | { status: "failed"; error:
  unknown; value: T | undefined }`, where a pending or failed state keeps
  the last run's value. Matching `status: "done"` gives a `value` typed
  `T`, with no `undefined` to test for. `status`, `value` (`T |
  undefined`) and `error` are the same, read one at a time, for a binding
  that wants only one (`visible: weather.status === "pending"`). All four
  are properties, so bindings read them.
- The signals `done(value)` and `failed(error)` are for effects a binding
  cannot say.
- `autoRun` (`true`) runs on a change of `args`; with it `false`, the Task
  runs only when told: `start()` runs it with `args`' own, and
  `start(…)` with what it is given, every argument before the signal,
  optional ones too. `abort()` aborts the run in progress: its signal is
  aborted, and the Task has failed at once, with the signal's reason (an
  `AbortError`) as its `error` and the last run's value kept.
- A run superseded by a change of `args` does not fail: the Task stays
  pending, for the run that replaced it. A run superseded before it was
  called (`args` changed twice in one turn) is never called. `args` made again with the same
  elements runs nothing.
- `run` may be a plain function: what it returns is the value, and what it
  throws is the error, as from an async one.
- A change of where the Task got to is one step: `state`, `status`,
  `value` and `error` change together, each notifies at most once, and a
  binding that reads two of them is evaluated once.
- A Task is a Controller (§4.20) of the item it is declared in. When that
  item goes, the run in progress is aborted. So a delegate scrolled out of
  a list cancels its request.
- The checker types a Task from its `run`: an `args` written as an array
  is held to the run's parameters, element by element and in number
  (`ts-assign-type`), as is what `start(…)` is given: none of the
  arguments, or all of them; `value`, `done` and
  `state` carry what the run's promise gives; and a `switch` or an `if` on
  `state.status` narrows `state`. `ljos check --tsc` declares the same
  Task to TypeScript.

`todolist` adds a random task from a web API when a button is pressed:

```qml
// Qt (todolist/content/HomePage.qml, abridged)
function sendHttpRequest() : void {
    var http = new XMLHttpRequest()
    var url = "https://www.boredapi.com/api/activity";
    http.open("GET", url, true);
    http.setRequestHeader("Content-type", "application/json");
    http.setRequestHeader("Connection", "close");
    http.onreadystatechange = function() {
        if (http.readyState == 4) {
            if (http.status == 200) {
                let object = JSON.parse(http.responseText.toString());
                let currentDate = new Date()
                let format = Qt.locale().dateFormat(Locale.LongFormat)
                addTask(todayTasksModel, object.activity,
                        currentDate.toLocaleDateString(Qt.locale(), format), "","")
            } else {
                console.log("Failed to fetch a random TODO item; the API provider might be down")
            }
        }
    }
    http.send();
}

// Ljós
Task {
    id: randomActivity
    autoRun: false
    run: async (signal: AbortSignal): Promise<string> => {
        const reply = await fetch("https://www.boredapi.com/api/activity", { signal })
        if (!reply.ok) throw new Error(`status ${reply.status}`)
        return Activity.decode(await reply.json()).activity
    }
    onDone: (activity: string) => root.addTask(todayTasksModel, activity,
        Locale.current.formatDate(Clock.now(Clock.Resolution.Second), Locale.Format.Long), "", "")
    onFailed: console.log("Failed to fetch a random TODO item; the API provider might be down")
}
// the button: onTapped: randomActivity.start()
```

`Activity` is a small TypeScript module with the decoder (§4.4). The Qt
version treats a network error and a 404 alike only by luck (`readyState`
reaches 4 for both), and goes on running if the page is closed; the Task
says both, and its run is aborted with the page.

**On the web.** ljos-web's targets have `fetch` of their own. Ljós's
declarations are a subset of WHATWG's, so a script that checks runs the
same against the browser's `fetch`, which ljos-web uses as it is. Two
differences are the browser's and are documented on the site's page: CORS
applies there and not on the desktop, and a browser fetch cannot set some
headers (`Connection`, `Host`). `Task` and `Controller` are engine types,
built on properties and the object tree, which ljos-web has in its own
runtime; it provides them with the same features. It has neither yet. Its
runtime must provide `Controller` (a host, and attached and detached as
the host comes and goes) and, on it, `Task` as above: runs numbered so a
superseded result is dropped, the browser's `AbortController` for each
run, the four parts set as one step, and the run aborted when the host
goes.

DECISION: `fetch` and its objects are globals of every file, as on every
JavaScript platform, and not names of `Ljos.Core`: they mean what they mean
everywhere, and coding models write them unprompted. `XMLHttpRequest` is
compat only.

DECISION: `Task` is in `Ljos.Core`, and is a Controller.

DECISION: a Task's progress is one discriminated `state` as well as
`status`, `value` and `error`, so a `match` on it gives a `value` that is
never `undefined` where the run is done.

DECISION: a Task's `error` is `unknown`, as a `catch` binding is in
TypeScript: what a run throws need not be an `Error`. A script narrows it
(§4.4) before reading its `message`.

DECISION: `abort()` fails the Task with its signal's reason, so `failed`
and `state` say that the run did not finish; a run superseded by `args`
leaves the Task pending, since another run is under way.

DECISION: an `args` that is not an array written in place (one a function
gives, say) is checked as what it is, against nothing: the subset has no
tuple type to hold it to. A `run` whose return type is not written, and
which returns what a function of the file gives, gives `unknown`: the
examples write the run's return type.

DECISION: `Response.json()` gives `unknown`; a decoder narrows it.

DECISION: `Response.url` is the address the reply came from, after any
redirect the platform followed, and `redirected` says whether one was
followed, as WHATWG has it: the request's address written its own way
(`https://example.org` for `https://example.org/`) is not a redirect.

DECISION: a method is one of those the platform sends (`GET`, `HEAD`,
`POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `PROPFIND`); another rejects
with a `TypeError` before anything is sent, as a forbidden one does.

DECISION: a request that cannot be made rejects with a `TypeError` whose
message is `fetch failed: ` and what the platform says, as Node and Deno
word theirs.

DECISION: in the checker, `Headers.entries()` is a readonly array of
readonly pairs and `Headers.get` gives `string | null`, as lib.dom says;
an init's `headers` object is not checked to have string values, which a
value that is not one becomes, as WHATWG has it. `tsc` reads the same
names from Ljós's declarations, not lib.dom, so what the subset leaves out
(`mode`, say) is unknown to both.

DECISION: an `async` function's written return type is a `Promise<T>`, and
what it returns is checked against `T` once awaited, as TypeScript does.

DECISION: the pass keeps every line of a request it writes as `fetch`,
as it does every edit: the statements the call gathers are emptied, and
`ljos migrate --write` takes the emptied lines out. A request whose
address, header or body is written over lines keeps the file, as the call
it gathers them into is one line.
Its `fetch(…).catch(() => Response.error()).then(async (http: Response) =>
{…})` gives the handler a reply even where the request could not be made,
whose `status` is 0 and whose body is empty, as the request's were.
A `responseType` of `"json"` is read with `json().catch(() => null)`:
the request's `response` was null where the body was not JSON, and the
handler's own check of `status` still runs.
DECISION: `fetch` and its classes are found after the file's own names: a
function `fetch` of the component, or a type `Headers` or `Request` of the
project, is that one, in the checker and in the program alike, as those
names were there before `fetch` was. The globals of TYPES.md (`Math`,
`JSON` and the rest) keep their place before them.

### 4.20 Controllers

**What apps say.** "This item also does that": follows the pointer,
orbits a camera, loads its data, keeps its size in step with something
else. Qt's QML says it with an invisible `Item` that fills its parent, a
`Connections`, or a block of handlers copied into each file. React says
it with hooks, and lit with reactive controllers.

**The design.** A `Controller` is a non-visual object declared inside an
item. That item is its **host**. It has:

- `host`, the item it is declared in (an `Item`, readonly). A popup is
  not an item, but what is declared in it goes into its content item, so
  that item is the host of a Controller or a gesture declared in a popup,
  and when the popup's content item is replaced the new one is;
- a lifecycle, as signals: `attached(host)` once its host is complete,
  or when it is put into a host; `detached()` when the host goes, or the
  controller is taken out (a `Loader` unloading, a delegate removed), or
  it leaves its host for a popup's new content item, where it is
  attached again;
  `updated()` once after `attached`, and then once after each turn in
  which a property of the host changed, after bindings settle and before
  drawing;
- properties, functions, signals and children of its own, as any object
  has. Its children are its host's too: a gesture (§4.7) or a `Task`
  declared in a Controller acts on the Controller's host.

A Controller is a file whose root is `Controller`, or a TypeScript module
(below). What it adds is properties, so it is read and bound as any object
is, through its id. There is one reactive system, the engine's, and there
are no rules about the order of calls, as React's hooks have: a
Controller's state is its properties, and its effects are bindings and the
three signals.

`updated()` costs nothing unless a Controller handles it; the engine
calls it only for Controllers that do. What `updated` handlers write does
not arm `updated` again: a handler sees the changes of the turn before
it, not its own, so it can never keep the engine awake (rule 6). The
bindings that read what it wrote update as usual. Where a property
follows from the host, a binding says it better than `updated` does:
`readonly property bool compact: host.width < width`. `updated` is for
the effects a binding cannot say.

`osmbuildings` drives its camera with an `Item` that fills the view, to
hold two `DragHandler`s, a pinch, a wheel and the clip planes:

```qml
// Qt (osmbuildings/OSMCameraController.qml, abridged)
Item {
    id: root
    required property Node origin
    required property Camera camera
    property bool panEnabled: true
    implicitWidth: parent.width
    implicitHeight: parent.height

    Connections {
        target: root.camera
        Component.onCompleted: { onZChanged() }
        function onZChanged() {
            let distance = root.camera.z
            if (distance < 1) {
                root.camera.clipNear = 0.01
                root.camera.clipFar = 100
            } else if (distance < 100) {
                root.camera.clipNear = 0.1
                root.camera.clipFar = 1000
            } else {
                root.camera.clipNear = 1
                root.camera.clipFar = 10000
            }
        }
    }
    DragHandler {
        target: null
        acceptedModifiers: Qt.NoModifier
        acceptedButtons: Qt.RightButton
        onCentroidChanged: root.mouseMoved(Qt.vector2d(centroid.position.x, centroid.position.y), false)
    }
    // …another DragHandler, a PinchHandler, a WheelHandler, a Timer
}
// Main.qml: OSMCameraController { origin: originNode; camera: cameraNode }

// Ljós (OrbitCamera.qml, abridged)
Controller {
    id: orbit
    required property Node origin
    required property Camera camera
    property bool panEnabled: true

    readonly property real clipNear: match (camera.z) {
        when if (camera.z < 1):   0.01;
        when if (camera.z < 100): 0.1;
        default:                  1;
    }
    readonly property real clipFar: clipNear * 10000

    Drag {
        modifiers: Modifier.None
        buttons: MouseButton.Right
        onCentroidChanged: orbit.rotate(vector2d(centroid.position.x, centroid.position.y))
    }
    // …another Drag, a Pinch, a Wheel
}
// Main.qml: View3D { PerspectiveCamera { id: cameraNode; clipNear: orbit.clipNear
//                                        clipFar: orbit.clipFar }
//                    OrbitCamera { id: orbit; origin: originNode; camera: cameraNode } }
```

The controller has no geometry to keep equal to its parent's, its
gestures act on the view it is declared in, and the clip planes are
bindings the camera reads, not writes made from a handler that has to be
called once by hand at the start.

**In TypeScript.** A module exports a Controller made by `controller`,
with typed properties, bindings (a function of the host and the
Controller, re-evaluated when what it reads changes, as a QML binding
is), and its lifecycle as functions. `attached` may return a function,
which runs on `detached`, as React's effects return their cleanup:

```ts
// breakpoint.ts
export const Breakpoint = controller({
    properties: { width: 600 },
    bindings: { compact: (host, self) => host.width < self.width },
})
```

```qml
import "breakpoint.ts" as Size

Item {
    Size.Breakpoint { id: breakpoint; width: 720 }
    Column { visible: breakpoint.compact }
}
```

The module form has no `class` and no `this`, which the subset forbids
(§5.1): `self` is the Controller's typed properties, and `host` its host.
The QML form and the module form make the same native object, and a
module's Controller is named in QML qualified by its import, as a
module's types are (§14, Q5). `controller` is a global of every `.ts`
module, declared with the generated declarations (§4.4), as `later` is a
name of every Ljós file: a module imports nothing to write a Controller.
`host` is typed `Item` by `controller`'s signature, so the module names
no type it does not use. The front end reads the call and makes the
module's export a Controller type (`breakpoint.ts/Breakpoint`), with a
property for each of `properties` and a readonly one for each of
`bindings`, typed from their values and return types; the keys it takes
are the five above, and any other is an error. `properties` and
`bindings` are objects written in the call, each member `name: value`:
the type is made from the names written there, so an object made
elsewhere, a spread, a computed name or an accessor is an error rather
than a member the type would not have. That type is a QML file
the compiler makes beside the module, whose root is a `Controller`: each
property, a readonly property per binding that calls the binding with the
host and the Controller, and handlers that call the lifecycle's
functions. So the front end, the checker and the runtime read it as any
file, and the module's code runs as the module's.

**What becomes a Controller.** The gestures (`Tap`, `Drag`, `Pinch`,
`Hover`, `Wheel`, `Pointer`) are child objects that find their item when
they complete (§4.7), which is what a Controller is: they become
Controllers in S3, with `host` for what they act on. `Task` is one from
the start (S4). Both are the proof that the seam carries real types.

**Under it.** A Controller is a native `QtObject` derivative in
`Ljos.Core`, placed in its host's `resources` as gestures are today. The
engine calls `attached` from its completion, and `detached` from the
host's destruction or the controller's removal from the host's
`resources`, through the hooks that gestures and `Component.onDestruction`
use (`ljos_engine/src/hooks.rs`, `Engine::on_destroyed`). A Controller's
bookkeeping per engine is an `Engine::extension`. `updated` is a step of
the scene's frame, before polish, so what its handlers write is laid out
in the same frame; the change that arms it asks the scene for a frame, as
any change does, so nothing polls for it.

ljos-web has no engine of Ljós's: its runtime owns objects as a reactive
graph, with an owner per object whose cleanups run when it is disposed. A
Controller there is the same IR object with the same three signals.
`attached` runs once the host's owner has mounted it, `detached` from the
cleanup of the Controller's own owner, which the host's disposal runs
first, and `updated` is queued on the runtime's batch end (a microtask)
for Controllers that handle it, once per batch that wrote a property of
the host, so the browser's frame is never asked for one. What
`attached` returns is that owner's cleanup. A gesture is a Controller
there as here, its listeners added to the host's element when it is
attached and removed when it is detached. The module form is the same
erased script, with `controller` the same function, so ljos-web reads a
module's Controller as the front end describes it and needs no code of
its own for it.

DECISION: a Controller's host is the item it is declared in, and a
Controller declared inside another shares that one's host. One declared
in a popup is hosted by the popup's content item, and moves to the one
that replaces it, `detached` from the old and `attached` to the new, so
that its gestures act on what is shown. A Controller
outside an item (held by an `Object`) has no host and `attached` does
not run; the checker says so in a file of Ljós's API
(`controller-host`). One declared directly in a `Window` or an
`ApplicationWindow` acts on the window, as a handler declared there acts
on its content item in Qt, and the checker takes it with or without
Qt's table of types.

DECISION: the lifecycle is three signals (`attached`, `detached`,
`updated`), handled with `on…` in QML and given as functions in a module.
`updated` runs once after `attached`, and what its handlers write does
not run it again.

DECISION: a `match` with no `default` is accepted only where the checker
proves it covers its subject, and the desugar casts `$nomatch`'s argument
to `never`, so `tsc` accepts what the checker accepts (§4.18).

DECISION: Controllers are written in QML or in a TypeScript module, and
both make the same object; the module form has no class.

DECISION: `controller` is a global of every `.ts` module, as `fetch` is of
every file and `later` of every Ljós file, not an export of a module to
import: a Controller module imports nothing. `host` is typed `Item` by its
signature; a module that wants more of the host narrows it.

DECISION: the gestures' base, `PointerHandler`, is a Controller, so Qt's
handlers have `host` and the lifecycle as well as `parent`; it is a
member more, and Qt's names keep meaning what they did.

### 4.21 Compiled away: how the tools see the new syntax

Note 11's H2 was rejected because new syntax would break `tsc`, editors
and coding models, which read only TypeScript. The amendment keeps what
that objection protects, by compiling the syntax away before any of them
read it:

- **One desugar, in the front end.** `qml_syntax::desugar(text)` takes a
  script's text and gives plain TypeScript text, line for line (a line of
  the input is the same line of the output), with a span map back to the
  input. It is pure Rust with no engine, so it builds to wasm, and CI's
  `check:wasm` builds it (S5).
- **Every consumer reads its output.** The compiler (tiers 1 and 2), the
  checker, `ljos check --tsc`, the MCP tools, `ljos lsp` and ljos-web
  desugar first. The checker types the desugared script and reports at
  the input's spans; rules about how a person writes (`ts-no-null`,
  `ts-forbidden`, `ts-no-var`, and `ts-no-cast`, since the one cast the
  desugar writes is `$nomatch`'s `as never`, §4.18) do not apply to the
  text the desugar writes, rules about types do.
- **The source keeps the syntax.** The formatter, `qml_edit` and Studio
  read and write the text as written, and must round-trip it (S5, S6).
- **The parser.** `vendor/oxc_parser` reads `|>`, `%`, `match`, `when` and
  `is` when it reads a QML file, a QML script (`parse_qml`,
  `parse_qml_script`) or an `.lts` module, and nowhere else. How it
  records them without a fork of `oxc_ast` is S5's to decide, in
  `vendor/FORK.md`.
- **`.lts` modules take the syntax; `.ts` modules keep to TypeScript.**
  A module that uses `match`, `is` or `|>` ends in `.lts`. A `.ts` module
  stays a file that `tsc`, `tsserver` and editors read as they are, so
  the extension says which tools can read a file without Ljós. The new
  syntax in a `.ts` module is `ts-lts-syntax`: the message says why, and
  the fix renames the file to `.lts` and rewrites every import that
  names it (in QML files, scripts, `qmldir` files and modules). `ljos
  check` prints the fix, the MCP tools apply it, `ljos lsp` offers it as
  a code action, and `ljos migrate` makes the same rename. Script text
  copied out of a QML file into a module so keeps working, and never
  silently breaks `tsc`.
- **`.lts` is a module like any other.** Module resolution, `qmldir`,
  the importers, `ljos check`, `ljos check --tsc`, the MCP tools, `ljos
  lsp`, `ljos types` and ljos-web read `.lts` wherever they read `.ts`,
  and desugar it first. S5 builds `.lts` and `ts-lts-syntax` with
  `match`; S6 adds the pipe; S7 brings `--tsc`, MCP and `ljos lsp` to it.
- **One module name, one file.** `width.ts` and `width.lts` in the same
  directory are `module-clash`, an error of `ljos check` on both files:
  a `qmldir` entry, a reader and TypeScript would each have to pick one.
  So `ts-lts-syntax`'s rename never lands on an existing file; where a
  `width.lts` is already beside `width.ts`, the fix is not offered and the
  message names the file in the way.
- **Stock TypeScript reads a `.ts` module that imports an `.lts` one.**
  `ljos check` emits a declaration for every `.lts` module. A
  declaration has only types, so it has no `match` or `|>`.
  - **Explicit export types.** An `.lts` module's exports carry the
    types TypeScript's `isolatedDeclarations` asks for: a function's
    parameters and result, and a `const` whose type is not plain from
    its literal. One without is `lts-export-type`. The declaration is
    then TypeScript's own declaration emit of the desugared text (the
    isolated-declarations transform, which needs no checker), never the
    checker's inferred types. `tsc` and `--tsc` therefore read the same
    types, even where the checker's rules (`ts-readonly`) see more.
  - **Where.** In the project's generated directory, `.ljos/types/`, at
    the module's path relative to the project root and under the name
    `allowArbitraryExtensions` looks for: `src/width.lts` is declared in
    `.ljos/types/src/width.d.lts.ts`. The project root is `ljos check
    --project <dir>`, or else the nearest directory with a
    `tsconfig.json`, from the files checked upward, or else the deepest
    directory that holds all of them. The search starts at the files,
    not the current directory, so where `ljos check` runs does not move
    the root. `.ljos/` holds only generated files and a `.gitignore` of
    `*`, which `ljos check` writes.
  - **The tsconfig.** `ljos check` writes `.ljos/tsconfig.base.json` with
    exactly `{ "compilerOptions": { "allowArbitraryExtensions": true,
    "rootDirs": ["..", "./types"] } }`, whose relative paths TypeScript
    reads from `.ljos/`. The importer's specifier stays `./width.lts`,
    unrewritten, and `rootDirs` resolves it to the declaration.
    `ljos check` writes nothing outside `.ljos/`, so it never creates
    or edits a project's `tsconfig.json`, and a check from a
    subdirectory leaves no file behind to become a root. In a project
    with an `.lts` module, `ljos check` warns `tsconfig-lts` when the
    root has no `tsconfig.json`, when the project's does not extend the
    base, or when it sets `rootDirs` itself (`extends` does not merge
    arrays). The fix, which `ljos migrate` applies as the MCP tools and
    the LSP's code action do, is the least edit that reaches the base:
    - no `tsconfig.json`: write one at the root that holds only
      `"extends": "./.ljos/tsconfig.base.json"`;
    - no `extends`: add that line as the first key;
    - an `extends` already (one path, or an array, TS 5.0 and later):
      make it an array that keeps the project's entries and ends with
      `"./.ljos/tsconfig.base.json"`, so the base's `rootDirs` is the
      one that holds;
    - its own `rootDirs`: append `"."` and `".ljos/types"` to it, after
      the project's own entries.
    The fix keeps the file's other keys, order and comments. A project
    with no `.lts` module is never warned.
  - **Fresh.** Every `ljos check` writes each declaration from the
    module as it is, and only when the text changes, so a watcher is not
    woken for nothing. `ljos lsp` rewrites a module's declaration when
    the module is saved, so an editor of the importer sees the change at
    once. A declaration is deleted only when its module's file is no
    longer on disk, never because a check left the module out, so a
    check of some files never deletes another's; once a project's
    modules are all gone, the next check deletes their declarations.
    Each project of `ljos check --tree` has its own `.ljos/`, under its
    own directory. `ljos types` writes the same files, and `ljos migrate
    --write` writes them with the `tsconfig.json` it fixes, so the base
    it extends is there.
  - **`--tsc`** gives TypeScript the same declarations, built in memory
    from the text it checks, so it is never stale, and the same
    `rootDirs`. It works in a directory of its own, never the project's:
    `m/` holds the desugared `.ts` modules, `types/` the declarations,
    and each `.lts` module's desugared text is a root file named
    `check/src/width.lts.body.ts`, with `m`, `types` and `check` as its
    `rootDirs`, so the module's own relative imports resolve as from its
    source directory. No specifier names a `.body.ts` file, so
    `./width.lts` has one target, the declaration, under stock `tsc` and
    under `--tsc` alike. Under `deno check`, which has no `rootDirs`, an
    import map in its `deno.json` sends each of those specifiers where
    `rootDirs` would. The desugar's helpers, `$is`, `$extract` and `$nomatch`,
    are declared after the last line of each desugared file, as §4.18
    types them, so no line moves. TypeScript's findings are reported at
    the input's spans, through the span map; one in a declaration is
    reported at its module unless the module has findings of its own.
    `crates/ljos_cli/tests/tsc_accepts.rs` holds M7's exit to this: the
    files of every scenario under `features/` with the new syntax in
    them that `ljos check` accepts pass `--tsc` with nothing said, and
    `tests/docs_site.rs` holds docs/site's examples to the same, where a
    TypeScript is installed.
- **The MCP tools.** `qml_check` and `qml_lint` take `.ts` and `.lts`
  modules as well as QML files, and a directory gives its `.qml` and
  `.lts` files; each finding is at its place in the file the agent
  wrote. `module_rename_lts` makes `ts-lts-syntax`'s rename, with the
  imports that name the module, as `ljos migrate` does; without `write`
  it only says what it would rename.
- **`ljos lsp`.** No editor reads QML scripts or `.lts` modules as
  TypeScript, so Ljós has its own language server: `ljos lsp`, over
  stdio. It answers `initialize`, `shutdown` and `exit`, keeps the text
  of each file the editor has open (`didOpen`, `didChange` with whole
  texts, `didClose`), and publishes the compiler's findings for a file
  on open and on each change, at the editor's line and UTF-16 column.
  The messages that came while it worked are handled together, and a
  file opened or changed is checked once, before the next request is
  answered: a burst of keystrokes is one check, of the last text. It
  reads the editor's text, not the disk's: the project is read once and
  kept, with every open file's text over its file's, so a keystroke
  checks one file against what the editor has of the others and reads
  nothing. It is read again after `didSave`, `didClose` or
  `workspace/didChangeWatchedFiles`, and after a save or a watched
  change every open file is checked again, as one may import what
  changed. A project it cannot read is said in a `window/showMessage`,
  once until it reads it, and nothing is published for it. On `didSave`
  of an `.lts` module it writes the project's declarations as `ljos
  check` does. For
  a `.ts` module with the new syntax, `codeAction` offers the rename to
  `.lts` as one workspace edit: the file's rename and the new text of
  each importer, made from the editor's text of an importer it has open
  and for that text's version, so an editor that has moved on refuses
  it rather than lose what it has. It does nothing between messages. Lints, members and
  `--tsc` stay `ljos check`'s, so a keystroke never waits on TypeScript.
- **What ljos-web calls.** ljos-web reads QML with `qml_syntax::parse`,
  which keeps the syntax as written, so it reads `match`, `is` and `|>`
  as the editor wrote them. To run them it calls what the compiler
  calls, from `qml_syntax`, which builds to wasm:
  - `qml_syntax::parse_lowered(allocator, source)` for a QML file: the
    tree of the desugared text, which is `source` itself when nothing
    in it needs the desugar, and the `SpanMap` back to `source`;
  - `qml_syntax::desugar(text)` for an `.lts` module, or a `.ts` one,
    before it is run or given to TypeScript: plain TypeScript, line for
    line, with its `SpanMap` and the syntax errors at the input's spans;
  - `SpanMap::start(offset)` to show a finding or a stack frame of the
    desugared text where it is in the source.
  It resolves `./width.lts` as it resolves `./width.ts`, and an `.lts`
  module needs nothing of `ljos_compiler`. The `.d.lts.ts` declarations
  are for stock TypeScript; ljos-web does not need them.

DECISION: the new syntax is in QML files' scripts and `.lts` modules, and
a `.ts` module with it is an error whose fix renames the file. The
extension tells every tool, and every person, whether a file is plain
TypeScript; a `.ts` file that stock `tsc` cannot read would break that
promise silently.

H1 holds in this form: **every program the checker accepts is, after the
desugar, valid TypeScript with the same meaning.**

**Every new API ships with:**

- features in `features/` (the scenario first, red, then the code);
- a page on `docs/site/` in the new dialect, whose `qml` blocks that
  import `Ljos.Ui` pass `ljos check`; S1 adds the test that holds every
  page to that (`cargo test -p ljos_cli --test site_docs`, run with the
  workspace's tests);
- `ljos migrate` coverage where there is something of Qt's to rewrite:
  every `Qt` member and Qt-named enum (S1) and `XMLHttpRequest`'s shape
  (S2). `match`, `|>`, `Task` and `Controller` have no Qt counterpart to
  rewrite from; migrate does not write them;
- `api_status` rows: the Ljós types reach `ljos.txt`, which `api_status`
  reads, and each Qt member given a home has a row in `ledger/` naming the
  home.

### 4.22 Resources: `using`

A `using` declaration is Explicit Resource Management's, stage 4 (§1),
unchanged: what it names is disposed of when the block around it ends,
however it ends, the last declared first. `await using` awaits
`[Symbol.asyncDispose]()`, or calls `[Symbol.dispose]()` where there is
no asynchronous one. A `DisposableStack` holds several resources and
hands them on with `move()`; an `AsyncDisposableStack` is its
asynchronous form. An error while disposing is kept with the one it
follows as a `SuppressedError`. Nothing is compiled away: the desugar
leaves a `using` declaration as it is, and tier 2 runs it.

```ts
export function rows(path: string): number {
  using file = open(path);
  using stack = new DisposableStack();
  const cursor = stack.use(file.cursor());
  stack.defer((): void => log("closed " + path));
  return cursor.count();
}
```

**What the checker holds.** In a file written in Ljós's API, what a
`using` declaration is given, each item a `for (using x of …)` takes
among it, is disposable, `null` or `undefined`
(`ts-assign-type`, at the value):

- an object whose `[Symbol.dispose]` member is a method that takes no
  argument it must be given;
- for `await using`, that, or an object with an `[Symbol.asyncDispose]`
  method;
- a `DisposableStack`; for `await using`, an `AsyncDisposableStack` too.

What is disposed of only asynchronously, given to `using`, is an error
whose fix is `await using`.

The key of a method in an object literal may be `Symbol.dispose` or
`Symbol.asyncDispose`, as it may be `Symbol.customMatcher` (§4.18);
`Symbol` is named nowhere else (`ts-forbidden`, §5.1).
`DisposableStack` and `AsyncDisposableStack` are built-in classes, as
`AbortController` is: `new` takes no argument, `disposed` is a
`boolean`, `defer(f)` takes a function of no argument, `move()` gives the
same class, `dispose()` gives `void` and `disposeAsync()` a
`Promise<void>`. `use(value)` and `adopt(value, onDispose)` are typed at
the call: each gives the type of `value`, `onDispose` takes it, and what
`use` is given must be disposable as a `using` declaration's value must.

**`--tsc` and `.lts` declarations.** TypeScript checks with the library
`esnext.disposable` beside `es2022`, which declares `Symbol.dispose`,
the stacks and `SuppressedError`. A project's own `tsconfig.json` names
its `lib`: one that reads a module with `using` adds
`"esnext.disposable"`. A generated declaration keeps a
`[Symbol.dispose]()` member as written.

DECISION: `.ljos/tsconfig.base.json` does not set `lib`. A project that
sets its own `lib` replaces the base's, and one that sets none would be
given a library it did not choose; the library is the project's choice.

DECISION: `use` and `adopt` are generic at the call rather than declared
with a type parameter: the checker has no generic methods on its
built-in classes, and the call is where the type is known.

DECISION: tier 1 and AOT refuse a `using` declaration, as they refuse
`try` and `throw`: disposal is a `finally` with an error kept aside. A
binding or handler that uses one runs in tier 2.

## 5. The checker

### 5.1 Rules

Each rule is an error with a code that `ljos check` and the MCP tools
print. Those the checker builds are marked ✓.

| Code | Rule |
|---|---|
| ✓ `ts-assign-type` | A value goes only where its type does. A string goes to a `color`. |
| ✓ `ts-no-null` | No `null`. Absence is `undefined`, written `T \| undefined` or `x?: T`. |
| ✓ `ts-loose-equality` | No `==` / `!=`. |
| ✓ `ts-condition-boolean` | `if`, `while`, `for`, `?:` and `!` take a boolean, with no truthiness. |
| ✓ `ts-no-var` | No `var`. |
| ✓ `ts-no-any` | No `any`, written or inferred. A parameter without a type is an error. |
| ✓ `ts-no-cast` | No `as`: a decoder narrows; nothing casts (§4.4). No `!`. |
| ✓ `ts-switch-exhaustive` | A `switch` on a union covers every case. |
| ✓ `ts-match-exhaustive` | A `match` with no `default` covers every case of its subject, which must be a union of literals, a plain enum or a discriminated union; on any other subject it needs a `default` (§4.18). |
| ✓ `ts-is-binding` | A binding of `x is pattern` is read only where the `is` is known `true` (§4.18). |
| `qt-global` | No `Qt` in a file of Ljós's API; the fix names the member's home (§4.3.1). |
| ✓ `ts-lts-syntax` | No `match`, `is` or `\|>` in a `.ts` module; the fix renames it to `.lts` and rewrites the imports that name it (§4.21). |
| ✓ `module-clash` | No `name.ts` and `name.lts` in the same directory: one module name, one file (§4.21). |
| `lts-export-type` | An `.lts` module's exports carry the types `isolatedDeclarations` asks for, so its declaration is TypeScript's own emit (§4.21). |
| `tsconfig-lts` | Warning: a project with an `.lts` module has no `tsconfig.json`, or one that does not extend `.ljos/tsconfig.base.json`, or one that sets `rootDirs` without its two entries; `ljos migrate` applies the fix (§4.21). |
| `lts-generated` | The generated directory, `.ljos/`, cannot be written: said with the cause, after what the check found, which is still reported; or an `.lts` module is outside the project root it is checked with, so it has no place there and nothing is written (§4.21). |
| ✓ `ts-readonly` | Records and arrays are readonly unless declared mutable. Writing a QML property or a model is allowed (Q6, §14). |
| ✓ `ts-index-undefined` | Indexing gives `T \| undefined` (`noUncheckedIndexedAccess`). |
| ✓ `ts-optional-exact` | An optional member is not set to `undefined` explicitly (`exactOptionalPropertyTypes`). |
| ✓ `ts-variance` | Method parameters are contravariant, and mutable arrays are invariant. |
| ✓ `ts-forbidden` | No `class`, `this` outside QML's scope object, prototypes, `delete`, `for…in`, `with`, `eval`, or `arguments`. |
| ✓ `ts-boundary` | A compat `.js` module is called only through a `.d.ts` declaration (Q7, §14). |

The names a TypeScript module exports are held to two more: `unknown-member`
for a value it does not export (`Logic.total`), and `unknown-type` for a
type it does not (`property Logic.Rows`).

A JavaScript file a file in Ljós's API imports (`import "calc.js" as
Calc`), or a TypeScript module imports (`import { add } from
"./calc.js"`), is typed by the declarations beside it, as TypeScript
looks for them: `calc.d.ts` beside `calc.js`, `calc.d.mts` beside
`calc.mjs`. They are read as a module is, and held to the same rules:
what they leave untyped is `ts-no-any`, and a name they do not export is
`unknown-member`. `ljos check --tsc` gives TypeScript the declarations
too. With none, each name a file reads of it, however it reads it
(`Calc.x`, `Calc["x"]`, `const { x } = Calc`), and each name a module
imports, is `ts-boundary`, and the fix names the file to write. A
`.d.ts` with no JavaScript file of its own beside it is read by no
loader. What has
no fixed shape is declared `unknown`, for a decoder to narrow (§4.4). A
file in Qt's QML calling the same JavaScript file is compat code, and is
not checked.

Not built yet, though the rules above cover them:

- a value type's readonly fields: `pos.x = 1` on a `point` is not
  reported;
- a QML property declared `readonly` and assigned from a script is not
  reported, as the type table gives the checker no readonly flag (the
  `.d.ts` of `ljos types` has one);
- `PointerArea`'s handlers give their event as `QtObject` in the type
  table, so reading `event.x` is `ts-no-any` until the table types it;
- a hot reload keeps the declarations the session started with: an
  edited `.d.ts` is read when the app starts again.

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

### 5.3 What the checker knows

Every expression of a script has a type of TYPES.md's mapping: literal
types, primitives, unions, `undefined`, `unknown`, QML object types by
their prototype chain, object literal types, arrays (readonly where they
are declared so), functions and enums. It reads them from:

- literals, templates, array and object literals;
- names: ids, properties in scope, locals (a `const` keeps its literal
  type, a `let` widens it) and parameters, which hide what QML has of
  their name;
- members of QML objects (the type table and the file's declarations),
  of records, arrays, strings, `Map`, `Math`, `Number`, `console` and
  `Qt`'s enums;
- calls, by the function's parameters and return type: written,
  inferred from its body, or the signal's;
- operators, `&&`, `||`, `??` and `?:`;
- arrows and functions, whose untyped parameters take the types of where
  the function goes (a handler's signal, a callback's array);
- narrowing by `=== undefined`, `typeof`, a literal, a record's field, and
  by assignment, and `?.`;
- what a TypeScript module the file imports exports, values and types,
  read from the module's own declarations when a name first reaches it.
  A module that imports itself, directly or not, is let by where the
  cycle closes.

What it cannot type is an error (`ts-no-any`), at the place the type is
missing, with what gives it one. A method the type table names takes
anything, and gives what the table says it does: Ljós's own table says it
for a method that gives a `bool`, a number or a string (`method points
returns:real`), and gives `unknown` for any other; Qt's table never says. It lets by, without a type, a model's
role, a member only the running program knows, and an alias (whose type is
its target's).

How it holds the other rules:

- `ts-no-cast`: `as T`, `<T>x` and `x!` are errors, at the `as`, the
  `<T>` or the `!`. `as const` asserts nothing and is allowed. A
  decoder's result is no exception: a decoder narrows, and nothing
  casts (§4.4).
- `ts-switch-exhaustive`: a `switch` with no `default` on a union of
  literals (strings, numbers, booleans, `undefined`) or on a plain enum of
  Ljós's API (§4.3.1) names the cases it leaves out. A switch on a wider
  type, on an enum of flags, or on an enum of a kept Qt file, whose keys Qt
  combines as flags, is not held to it.
- `ts-readonly`: a member written `readonly`, an element of a
  `readonly T[]` (a `list<T>` property reads as one) and the array
  methods that change an array (`push`, `splice`, …) on one are errors.
  A record or array is mutable where it is written without `readonly`, as
  in TypeScript, so that `tsc` agrees with the checker. A record with a
  readonly member does not go where that member may be written, which
  TypeScript lets by. A QML property, a `list<T>` property given a new
  array, and a model stay writable (Q6).
- `ts-index-undefined`: `xs[i]` is `T | undefined`. Where it goes to a
  place that takes `T` only, or a member of it is read or it is called,
  the error says it may be past the end. Reading a member of, or calling,
  any other value that may be `undefined` is `ts-assign-type`.
- `ts-optional-exact`: an optional member given a value that may be
  `undefined`, in an object literal or by assignment, unless its type
  says `undefined` itself.
- `ts-variance`: a mutable array takes only its own element type, so a
  `number[]` is not a `(number | string)[]`; a readonly array is
  covariant. A function's parameters, a method's among them, are
  contravariant.
- `ts-forbidden`: `class`, `this` in a `function` of a script's own (in
  QML's scope and its arrows, `this` is the scope object), `prototype`
  and `__proto__`, `delete`, `for…in`, `with`, `eval` and `arguments`.
  `Symbol` too, but for `Symbol.customMatcher`, `Symbol.dispose` and
  `Symbol.asyncDispose`, each as the key of a method in an object literal
  (§4.18, §4.22).

## 6. Compat: the lowering pass

### 6.1 Per file

`qml_compat::lower(text, locals)` parses the file and walks it. Each edit
is to a name of the Ljós API, and the text keeps every line. The edits are:

- an import, `QtQuick` → `Ljos.Ui`, `QtQml` → `Ljos.Core`, and
  `QtQuick.Layouts` → `Ljos.Layout` (or `; import Ljos.Layout` on the
  import's line when a file names `Row`, `Column`, `Grid` or `Flow` and has
  no Layouts import). A file that has `QtQml` from `QtQuick` and names one
  of its types (`Timer`, `Component.onCompleted`, or a type it writes:
  `property QtObject o`, `signal picked(QtObject o)`) gets
  `; import Ljos.Core` on the import's line. A file of `QtQml` alone is
  lowered to `Ljos.Core` alone. A `QtQuick.Shapes` import is left an empty
  line, since `Ljos.Ui` has the shapes. A module imported twice is lowered
  on the last import's line (the first, for the controls), and the other
  import is left an empty line: no import of Qt's is left in a lowered file;
- a type, `MouseArea` → `PointerArea`, made or written
  (`property MouseArea area`);
- a member, its handler (`onColorChanged`), `PropertyChanges` and
  `Connections` on a known target, a value source's target (`… on color`),
  and `id.member` where the target's type is known;
- `PropertyChanges { target: id; prop: … }` → `PropertyChanges { id.prop: … }`
  (§4.9);
- `property var` / `variant` → `property unknown`.

`QtQuick.Layouts`, `QtQuick.Shapes`, the controls (`QtQuick.Controls.Basic`,
and `QtQuick.Controls` where the program starts with Basic),
`QtQuick.Dialogs`' file and folder dialogs and `QtQml` are lowered (§4.6,
§4.14, §4.16). `QtQuick.Window` leaves its line empty, since `Ljos.Ui` has
what it names. A module of the project, and a directory imported by path,
are files of the project: the file is lowered with them (§6.2). An import
of the program's own types stays as written, beside `import Ljos.Ui`, where
the pass is given their description (`locals.natives()`). Any other module
import (`QtQuick.Controls.Material`, an aliased `QtQuick`, `QtQuick.Shapes`,
`QtQuick.Window` or `QtQml`, a module of the project imported under a name,
or one whose types are native and not described) keeps the file. So does a
type of `QtQuick.Shapes` that `Ljos.Ui` doesn't have, made or read through
(`` `X` is not in Ljos.Ui yet ``).

The description is what the checker reads (`ljostypes.json`, §4.4). A
lowered run passes its oracle's (`ljos_compiler::OracleNatives`), so a
file is lowered against the types it is checked against. `ljos migrate`
reads the file `--types` names, else the `ljostypes.json` of the first
path it is given, and no other. A module of the toolkit's table is never
the program's, whatever it registers in it. Of the imports that have a
type's name, the last is the one it is, as the loader takes the last: a
program's `Text` imported after `QtQuick`, or its `Button` after
`QtQuick.Controls`, is the program's, its members keep their names, and a
type Ljós renames keeps its name; imported before them, Qt's is. A type the file names that is
neither Qt's, nor a file of the project, nor in the description keeps the
file.

A file is lowered *whole* or *kept*. The pass keeps the file as written,
and says why with a byte offset, when anything in it names what the API
has not taken yet. That includes a type, a member, a dynamic name (a string
built at run time and used as a member), or a script that reaches
something it cannot follow. A kept file is not an error: it runs as
written, on the same runtime.

DECISION: a local or a method with the name of a member Ljós renames
(`color`, `add`, `remove`) keeps the file only where that member is what
the name could read: in an object, or a file whose root, of a type that
renames it or may be one. `function remove(i)` on an `Item` is the
`Item`'s own and is lowered as it is.

`locals` tells the pass what the project is around the file, as the
project's loader finds it (`qml_front`'s modules and import path):

- the root type of a file beside it, so `Button { color: "blue" }` on a
  `Button.qml` whose root is a `Rectangle` is lowered as a `Rectangle`
  member;
- whether a module it imports is the project's, with QML files, or the
  project's with native types only;
- the root type of each type a module of the project, or a directory
  imported by path, brings. A name two imports have is the last one's, as
  the loader resolves it: `Text` of a project module imported after
  `QtQuick` is the project's.

A type of the project read in a script (`Theme.accent`) is a singleton: the
one object its file makes, of its root's type and of no type built on it.

### 6.2 By group

A file kept keeps the files it uses. Otherwise a kept `Main.qml` would set
`color` on a lowered `Button` that now has `fill` only, which the sweep
found in snippet-components-application before grouping.
`qml_compat::references` collects what a file may name:

- type names;
- property types;
- `.qml` string literals;
- directories imported by path;
- modules imported by name. A module the project's loader finds (a
  `qmldir` or a CMake module with QML types) brings its files, wherever
  they are (`Alone::uses`). Beside that, a directory whose path ends in the
  name's parts (`import My.Controls` is any `…/My/Controls/`), as an import
  path finds it, ties every file below it to the group;
- capitalised identifiers, such as an enum's owner.

`qml_compat::group` follows those names from each kept file: every file a
kept file uses, and every file that one uses, is kept with the reason "it
is used by `Main.qml`, which is kept: …". A file left as it is, with no
import the pass lowers, goes on in Qt's names, so the files it uses are
reached through it, and it stays as it is.

The other way does not keep: a lowered file may use a kept one. The pass
reads the kept file's root as Qt's (`Root::kept`) and binds each name on
it as written, `color` on a kept `Rectangle` and on what it holds, since
a Ljós file reads a kept type's names as its file declares them. What
the kept type has is not known, so a name a script reads unqualified in an
object of it may be that object's or the root's: where the root's lowering
renames it, the file is kept. A kept
`QtQuick3D` scene keeps the files it uses, not the application around it,
so an application is migrated a file at a time. `Lowering::settle`
lowers again the users of a file whose kept-ness changed, and their users
in turn, until nothing does. Two files can turn each other over: `Main.qml` sets `color` on a
`Swatch` whose root is an `Item`, so against a lowered `Swatch` it is kept,
which keeps `Swatch`, against which `Main.qml` is lowered with Qt's names,
which lets `Swatch` be lowered again. A file that turns twice in one settle
is held kept as it last was, and both are kept.

A file lowered that way still binds Qt's names on the kept type: `Main.qml`
in Ljós's API holds `Dial { color: "#c03030" }`. Lowering `Dial.qml` later
would rename `color` to `fill` under it, so a file in Qt's QML stays kept
while a file in Ljós's API (one that imports a `Ljos` module) binds on its
type a name its lowering renames, or that name's handler: in a binding, a
`Behavior on color`, a grouped property, or a `PropertyChanges` or `Binding`
whose `target` is an id of the file. The pass follows
the file's root through the project's files it is built on, while they are
lowered too, to the Qt type whose renames say so, and a property the file
declares renames nothing. The reason names the file, the name and what
Ljós calls it: "`Main.qml`, in Ljós's API, binds `color` on it, which
Ljós's `Rectangle` calls `fill`: rename it there as this file is lowered".
What that file's scripts set on the type is not read, nor a `target` that
is no id of the file; `ljos check` says it once the type is lowered.

DECISION: grouping is conservative. A name that might be a sibling ties
the two, even when it is a coincidence. The cost of a false tie is a file
run as written, and the cost of a missed tie is a wrong program.

DECISION: a lowered file may use a type defined by a kept file of its
project (G5). `ljos check`, the compiler and the runtime read a kept
type's names as its Qt file declares them, in a Ljós file too.

DECISION: lowering a kept file never renames what a file in Ljós's API
binds on it. The pass lowers a file at a time and leaves the dialect file
as its author has it, so the kept file waits for the rename by hand: the
reason says which and to what.

### 6.3 What keeps a file

| Kept because | Phase |
|---|---|
| A type or module the API has not taken (`Column`, `Behavior`, `Dial`, `QtQuick.Controls.Fusion`, …) | Lowered as the API grows (M1, M6) |
| `import QtQuick.Controls` in a program that starts with a style other than Basic | Lowered once Ljós has that style's controls (M6) |
| A module of the project whose types are native (C++ with no QML file here) | Kept while its types are not described to the pass |
| A module of the project imported under a name (`import Gauges as G`) | Kept until the pass follows a qualified type |
| A file in Ljós's API binds on the file's type a name lowering renames (`color` on a `Rectangle`) | Lowered with that binding renamed to Ljós's name (`fill`) by hand, as the reason says |
| `Qt.createQmlObject` (QML as text) | Kept. The text is lowered only if the pass runs at that call too (M5) |
| A name built at run time (`obj[name]`, `"col" + "or"`) | Kept for good. A static analysis cannot follow it |
| `.bold` of a font read or written by a script, `font.bold` with `font.weight` (in any form), `.pointSize` read or written by a script, `font.pointSize` with `font.pixelSize`, or a font member read off a `font`, `var` or `alias` whose type is not known (§4.13) | Kept until the pass knows more types (M4) |
| A `.qml` URL built at run time (template literal, concatenation) | Kept when the file is seen. A group tie through it is missed (risk R1) |
| `property var` whose type is not known | Kept. A person types it, or makes it `property unknown` and decodes it (§4.4) |
| A name the file's Qt imports lack (a file of the project, or one the context gives) that a Ljós module the lowered file imports has (`Box.qml` beside a file, which `Ljos.Ui` would shadow; `Button.qml` beside one that imports `QtQuick.Dialogs`, which `Ljos.Controls.Basic` would) | Kept for good. An import is found before the files beside it, so lowering would change the type; renaming the project's file is the author's call |
| An import of the program's own types with no description of them, or a type the description lacks | Lowered once the program describes it (`ljostypes.json`) |
| A member Ljós renames, on a program's type built on the toolkit type that has it (`color` of a type built on `Rectangle`) | Lowered once a program's types get Ljós faces |

### 6.4 Coverage today

`ljos migrate` over the corpus, each example as its own project (one
`ljos migrate <example>` per directory of `examples/demos` and
`examples/tutorials`, then one over `doc` and `tests`), 2026-11-07:

```
corpus/qtdoc/examples:  495 files, 13 lowered, 467 kept
corpus/qtdoc (all):     524 files, 24 lowered, 485 kept
```

One project per example matters: run over all of `examples` at once, a
module of one example (`content`) ties every `…/content/` directory of the
others by the directory rule (§6.2), and nothing is lowered.

Before the project's modules were lowered, the same count was the same 13
lowered and 472 kept. 25 files were kept by an import of their own
project's module (`Thermostat`, `content`, `ToDoList`, `StocQt`, `Config`,
…). Those now get past their imports, and every one is still kept by
something else: a Controls or 3D import, a type not in `Ljos.Ui` yet, or a
file of their group. Coverage is low on purpose, because the slice is
`Rectangle`, `MouseArea`, the arrangement and the layouts.

Why the 467 example files are kept:

| Reason | Files |
|---|---:|
| a module import: `QtQuick.Controls` (90), `QtQuick3D` (86), `QtQuick.Controls.Basic` (43), `QtQuick.Templates` (15), `QtQuick.Controls.Fusion` (15), `QtQuick.Particles` (12), `QtMultimedia` (7), `QtCore` (5), … | 309 |
| a file of its group is kept | 76 |
| a type not in `Ljos.Ui` yet: `Image`, `Window`, `Screen`, `ListView`, `Behavior`, `GridLayout`, … | 63 |
| a layout the arrangement would place otherwise, or a child whose size in it is not known | 12 |
| a member read off, or set on, something whose type is not known | 6 |
| a module of the project whose types are native | 1 |

Recounted with `Ljos.Controls` (K1, 2026-11-07). One run over
`corpus/qtdoc/examples` is one group today: modules imported by name tie
the examples together, and a kept `QtQuick3D` file keeps the rest. So the
examples are counted one example at a time, as an app is migrated:

```
one run over examples:          495 files,  0 lowered, 485 kept
each example, --style Basic:    495 files, 13 lowered, 472 kept
each example, --style Fusion:   495 files, 13 lowered, 472 kept
```

With Basic, no example file is kept by its controls import any more. What
the controls keep instead (recounted after K4; the totals above did not
move). Three files are no longer kept for a control K4 adds: two are
still kept, now by their group, and one lowers, so the rows sum to 113,
not 114:

| Reason | Files |
|---|---:|
| a file of its group is kept | 73 |
| `QtQuick.Controls.Fusion` (15), `.impl` (7), `.Material` (5) | 27 |
| a control not in `Ljos.Controls.Basic` yet: `StackView` (4), `ApplicationWindow` (4), `RoundButton` (3), `Pane`, `Page` (1 each) | 13 |

With `QtQuick.Dialogs` lowered (K5), each example with `--style Basic` is
495 files, 14 lowered, 471 kept, as it was at K1's last commit (which
lowers 14, one more than the count above). The six example files that import
it are kept by their next import: `QtCore` (2), `QtQuick.Controls.Fusion`
(2), and a module of the example's own (2).

With Fusion (the Linux default), 89 files are kept by `import
QtQuick.Controls`. The lowered count does not move yet: the files the
import let through are kept by the other types they name, or by their
group.

`Qt.*` keeps no file today: `Qt` is a global of every file, and a member
`Ljos.Core` has is lowered to it where its name is free (§4.3). Modules, not
globals, decide coverage. Phase 2 starts with them (§17).

With `Qt.*` lowered by member (§4.3) the counts are the same. One file's
edits change: car-configurator's `CameraReset.qml` writes `vector3d(…)` for
its two `Qt.vector3d(…)` and imports `Ljos.Core` (3 edits, was 1). The one
`Qt` member left in a lowered file is `Qt.createComponent`, which waits for
§4.11. No example the render sweep runs has a lowered file whose text
changed.

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
  wasm, and the desugar of `match` and `|>` (`qml_syntax`, §4.21) on the
  same terms. Its `fetch` is the browser's, of which Ljós's declarations
  are a subset; `Task` and `Controller` are its runtime's (§4.19).

## 8. `ljos migrate`

```
ljos migrate [--write] [--style NAME] [--types FILE] PATH…
would lower qml/SimpleButton.qml (5 edits)
kept qml/Main.qml:12:5: `Column` is not in Ljos.Ui yet
2 files, 1 lowered, 1 kept
```

- It runs the same pass and the same grouping as `--compat lower`, and a
  migrated file compiles to the program a lowered run compiled. The text
  is laid out as a file written in Ljós's API is: each import on a line of
  its own, without the lines the pass emptied of a Qt import (a lowered run
  keeps every line where it was, so that its findings point at the file as
  written).
- Each directory named is read as a project, as a run reads it, so a
  module of the project and a directory imported by path are files to the
  files that import them. A file named alone is of the project of its
  directory (a bare `Main.qml`, of the directory the command runs in).
  A file that was not named is kept, and so is each named file it uses,
  saying which file to name too (``it is used by `Main.qml`, which is
  kept: it is not named: name it too``), so a file is never renamed to
  match one that stays Qt's. A named file that uses one not named is
  lowered, keeping Qt's names on that one's type (§6.2).
  Give it an application's directory, not a tree of several:
  the group's directory rule (§6.2) ties one application's module to
  another's directory of the same name.
- A file of Ljós's API is nothing to migrate. A module of Ljós's own
  (`Ljos.Design`, a shell's `Ljos.Blocks`) imported beside Qt's is Ljós's
  API already: its import stays as written, what the file names of it
  keeps its names, and the Qt imports and what they name are lowered, as
  beside a module of the program's. A module the pass writes for Qt's
  (`Ljos.Ui`, `Ljos.Core`, `Ljos.Models`, `Ljos.Layout`,
  `Ljos.Controls.Basic`) imported beside them keeps the file: its types
  would stand for Qt's of the same name, whose members are then not
  renamed, and the lowered file would import it twice.
- A lowered file says its scripts are still Qt's JavaScript:
  `pragma Scripts: JavaScript`, on the import's line so that every line
  stays where it was. The checker does not hold such a file to its rules,
  so `==`, `null`, `var` and `if (count)` keep compiling as they did.
  Taking the pragma out is the step that types a file, and `ljos check`
  then says what to change. Renaming the API and typing the scripts are
  two steps, and only the first is mechanical. Migrate takes the second
  where it changes nothing: it compiles each lowered file as it will be
  written, with the pragma and without, and leaves the pragma out where
  the checker finds nothing more without it (the pragma only turns the
  rules off, so the program is the same). Where the checker does find
  more, the pragma stays on a line of its own and migrate says how many
  findings typing the file has to settle
  (`scripts of Main.qml stay JavaScript: typed, ljos check finds 2`).
- From S1, it gives every `Qt` member and every key of a `Qt` enum its
  home (§4.3.1), or keeps the file and says which member kept it. From S2,
  it rewrites `XMLHttpRequest`'s usual shape into `fetch`, or keeps the
  file and says why (§4.19). It never writes `match`, `|>`, `Task` or a
  Controller: those are a person's step, as typing is.
- Without `--write` it says what it would do. With `--write` it writes in
  place.
- A file whose constant ListModels became typed lists gets their row types
  in `<File>.rows.ts` beside it (§4.8), said as `would write` or `wrote`
  under the file's line. Where that module is there with other bytes, or
  cannot be read, the file is not written and migrate says why, so a
  hand-written module is never overwritten. A compat run (`--compat
  lower`) holds the module in the project the same way. Where the project
  has a script of that path of its own, the script stays and the file is
  kept as written, as migrate keeps it, since its lowered text would import
  rows the script does not declare.
- `--types` names the description of the program's own types, as
  `ljos run --types` does. Without it, migrate reads the `ljostypes.json`
  of the first path given, if it has one; for a file, that is the file's
  directory. The other paths are not looked in (§6.1).
- Exit status 0 means every file was read. 2 means a file could not be
  read or written, or the description could not be read; in that case
  nothing is migrated, and the error names the description with its
  directory.
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

### 10.1 Working on the dialect in parallel

Lanes build areas side by side, and each PR touches only its own area's
files, so that one merging does not put the others in conflict:

- **Renames and rules** go only in your area's
  `crates/qml_compat/src/rules/<area>.rs`, and a rule's code in a module
  beneath it (`rules/text/font.rs`). A new area is three new
  files, `rules/<area>.rs`, `tests/<area>.rs` and
  `docs/dialect/status/<area>.md`, and one line in `AREAS`
  (`rules/mod.rs`); a unit test holds the three lists equal. A new rule
  is one line in `RULES` there. A member Ljós calls by another name is
  renamed once, by the runtime's face of the type (`ljos_quick`'s
  `ljos_ui.rs`), and Ljos.Ui's description carries it to the pass as
  `was=` (`crates/qt_types/src/ljos.txt`); an area writes only an empty
  row, for a type that would take the renames of what it is built on
  (`Window`, `rules/window.rs`). A type
  is renamed in one area, and a type has one rule; a unit test holds
  both.
- **Tests** go only in `crates/qml_compat/src/tests/<area>.rs`. Shared
  helpers stay in `tests/mod.rs`.
- **Pass logic** for an area goes in its rule, as `rules/text/font.rs`
  does for fonts, `rules/loader/source.rs` for a `Loader`'s `source`,
  `rules/layout/arrange.rs` for positioners and layouts,
  `rules/gestures/gesture.rs` for a `MouseArea` that fills its parent, and
  `rules/states/changes.rs` and `rules/core/connections.rs` for what a
  `PropertyChanges` and a `Connections` are aimed at: a rule answers for a
  type's members, its objects, their children and what they are aimed at,
  and asks the file only through `rules::File`.
- **Status** goes only in `docs/dialect/status/<area>.md`. The Status line
  at the top of this file and §6.4's numbers are the track lead's, after
  merges; a lane puts its corpus counts in its PR body.
- **`crates/qt_types/src/ljos.txt` is generated.** On a conflict, take
  either side and regenerate it with
  `LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types`; never
  merge it by hand.
- **A PR is done when it can merge**: `gh pr view N --json
  mergeable,mergeStateStatus` says `MERGEABLE` and `CLEAN`, not when it is
  pushed.

## 11. Performance budget

| Where | Budget | Phase 1 |
|---|---|---|
| Run time, compat file | **0** vs today: same IR, same natives, same indices | A face shares the native's indices. A renamed member is linked by name once at compile time |
| Run time, Ljós file | ≤ compat, and less where types let tier 1 specialise | equal (no specialisation yet) |
| Load time, `--compat keep` (default) | **0** | the pass does not run |
| Load time, `--compat lower` | one parse and one walk per file, plus a walk of what the kept files use. Under 5 % of today's compile of the file | `ljos migrate corpus/qtdoc` (524 files, the same per-file pass, `references`, and `group`) takes 0.26 s in a **debug** build, about 0.5 ms per file. It is not a gate yet (M1) |
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

- `import QtQuick` → `pragma Scripts: JavaScript; import Ljos.Ui` (module,
  and the scripts are still JavaScript, §8);
- `MouseArea {` → `PointerArea {` (renamed type);
- `color: buttonColor` → `fill: buttonColor` on the root `Rectangle`
  (cleaned member);
- two `PropertyChanges { target: button; color: … }` → `fill: …` (the
  member followed into states).

The typed property is `property color buttonColor`. With the pragma taken
out, the checker types `fill`, `buttonColor` and `state`. The TS handler is
`onPressed: button.state = "PRESSED"`, which passes `ts-assign-type`. A
copy with `button.state = 1` is reported.

**Result** (gate output in §12.1):

- Kept, lowered and migrated renders are identical PNGs.
- Each scores `content 0.946, pixels 0.9756` against qml6. The differences
  are font raster only, and identical for all three.
- `ljos check` on the migrated file, and on it with the pragma taken out:
  0 errors.
- The mutated copy, without the pragma: `` `state` is a string; this gives it a number
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

Each area's status, what is built and what remains, is in
`docs/dialect/status/<area>.md` ([ui](dialect/status/ui.md),
[layout](dialect/status/layout.md), [types](dialect/status/types.md),
[controls](dialect/status/controls.md) and the rest), written only by the
lane building that area. The Status line at the top of this file and the
numbers in §6.4 are updated by the track lead after merges, never by a lane
PR: a lane puts its corpus counts in its PR body.

| M | Builds | Exit criteria |
|---|---|---|
| **M1** Ui and Layout | `ljos run --compat`; `Ljos.Layout` (Row/Column/Grid/Flow and `QtQuick.Layouts`); `Ljos.Ui` items the corpus keeps for (`Image`, `Behavior` and animations, `ListView`, `Flickable`, `Repeater`, `Loader`, `Window`, `Screen`); the gestures, for the `*Handler`s (§4.7); a load-time budget gate. Built: `Tap`, `Hover`, `Drag`, `Pointer` and `Wheel` (`TapHandler`, `HoverHandler`, `DragHandler`, `PointHandler`, `WheelHandler`), by member; `DragSource` (the attached `Drag`); a filling `MouseArea` that clicks as a `Tap` and `Hover`. Remaining: `Pinch`, `LongPress`, a MouseArea's `drag.*` and `hoverEnabled`, `takeOver` | the features pair for each rename; compat sweep identical; ≥ 25 % of corpus example files lowered |
| **M2** Core | `Ljos.Core` (`Timer`, `Component`, `Connections`, value-type constructors, `App`, `later`), with `Qt.*` lowered to it and `Ljos.Ui`'s `QtQml` import dropped | sweep identical; no lowered file reaches the `Qt` object |
| **M3** Tools | migrate on `qml_edit`; MCP `migrate`, `explain`; Studio shows the API; enum cleanup (`Qt.Align*`) | migrate round-trip on the corpus re-lowers to itself; Studio edits a Ljós file losslessly |
| **M4** Types | TS modules imported by QML; `property T` of TS types; generated `.d.ts`; decoders; every rule of §5.1; tier-1 typed opcodes | a Ljós file passes `tsc --noEmit` with the generated `.d.ts` iff the checker accepts its scripts' types; tier-1 coverage of the features' Ljós files 100 % |
| **M5** Natives renamed | natives take the API's names; faces removed; `QtQuick` becomes a table only the pass reads; `createQmlObject` text lowered at the call | no Qt name in `ljos_engine`/`ljos_quick` outside the table; budget test equal or better |
| **M6** Controls | `Ljos.Controls` and styles | Controls corpus lowered; qml6 parity on its features |
| **M7** Ergonomics | S1 no `Qt` (§4.3.1); S2 `fetch` (§4.19); S3 `Controller` (§4.20); S4 `Task` (§4.19); S5 `match`, `is` (§4.18) and `.lts` modules (§4.21); S6 `\|>` (§4.17.1); S7 the desugar in `ljos check --tsc`, MCP and `ljos lsp` (§4.21). One PR each, merged in that order | no lowered or Ljós file reaches `Qt`; a pending `fetch` leaves the engine idle; the gestures and `Task` are Controllers; `tsc` accepts every desugared Ljós file the checker accepts; the desugar builds to wasm; every new API has a site page that passes `ljos check` |

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

- A Ljós file, or a TS module, may call a compat `.js` module only
  through the declarations beside it, as TypeScript finds them
  (`calc.d.ts` beside `calc.js`, `calc.d.mts` beside `calc.mjs`).
  What has no fixed shape is declared `unknown` there, and a decoder
  narrows it. With none, each use is `ts-boundary`, and the fix names the
  file to write (§5.1). DECISION: no `.d.ts` is generated for a `.js`:
  what it gives is what its author knows, and inferring it would be the
  `any` the subset refuses.
- A compat file may call Ljós TS freely. It is erased to JS and has no
  runtime checks to break.
- At run time both are the same tier 2 (TS is erased), so the boundary is
  checked statically only and costs nothing.
- A lowered or migrated file is compat code, with no checker: it says
  `pragma Scripts: JavaScript`. A file becomes Ljós code when a person
  takes the pragma out.

## 15. Decisions

- DECISION: the module set is `Ljos.*`, one module per concern
  (`Ljos.Ui`, `Ljos.Layout`, `Ljos.Controls`, `Ljos.Models`, `Ljos.Core`,
  `Ljos.Blocks`).
- DECISION: in phase 1 a renamed type is a *face* of today's native
  (`MetaBuilder::face` + `rename`). It has the same native and indices, so
  a renamed member costs nothing at run time. M5 renames the natives
  themselves.
- DECISION: the checker runs only on files that import `Ljos.Ui`
  themselves and do not say `pragma Scripts: JavaScript`. The pass writes
  the pragma into every file it lowers, so a migrated file compiles as the
  file did (PR #184 review). Migrate does not refuse a file the checker
  would reject, nor rewrite `==` to `===`: either would change what runs or
  how much migrates, and typing is a person's step.
- DECISION: a file is lowered all-or-nothing, and a kept file keeps the
  files it uses. Grouping is conservative.
- DECISION: `Qt.createQmlObject` keeps its file until M5.
- DECISION: phase 1 exposes compat on `ljos render --compat keep|lower` and
  `ljos migrate`. `ljos run --compat` is M1 (built without `--watch`). The
  default is `keep`.
- DECISION: no Qt corpus file is copied into the repository. Tests read the
  corpus at run time and skip when it is absent. Features use fixtures
  written for them, and the spike copies into scratch.
- DECISION: qml6's observed behaviour (three repaints for a click) is the
  oracle, over a guess.
- DECISION: TS types are named in QML qualified by their import, and QML
  and native types reach TS as generated `.d.ts`.
- DECISION: properties and models stay writable in handlers, and only
  values are readonly (Q6).
- DECISION: compat JS is called from TS only through declarations (Q7),
  the `.d.ts` beside the `.js`, which a person writes.
- DECISION (§4.4): a decoder narrows; nothing casts. A decoder is a type
  guard, an assertion function, or a plain typed function that narrows what
  it is given and returns a typed value or a fallback: user code with no
  runtime check of the checker's, and `unknown` narrows as TypeScript 5 narrows it, which the
  tsc oracle holds the checker to.
- DECISION: phase 2 builds modules before globals: `Ljos.Layout` and
  the kept-for `Ljos.Ui` types (M1) before `Ljos.Core` (M2). Module imports
  keep 364 of 462 kept example files, and `Qt.*` keeps none.
- DECISION (§4.1): `Ljos.Core` exports `QtQml`'s types under their own
  names, and brings `QtQml.Models`; a lowered file has the models from
  `Ljos.Models`. `Qt` is a
  global of every file. `Ljos.Ui` imports nothing.
- DECISION (§4.3): `Ljos.Core`'s functions and `App` are names of a file
  that imports it, after everything else in sight; colours are made with
  bare `rgba(…)`. `Qt.*` is lowered to them by member only where the new
  name is free, and `Qt.darker`/`lighter`/`alpha`/`tint` and `Qt.binding`
  stay Qt's in a lowered file.
- DECISION: the edits are text replacements at spans that keep every line,
  and they move onto `qml_edit` in M3.
- DECISION (§4.6): one arrangement, the Box, with one size rule per axis:
  `Layout.preferred*`, else an explicit size, else the implicit size.
  Anchors on a Box child are a check error.
- DECISION (§4.6): `Ljos.Layout` holds `Box`, `Row`, `Column`, `Grid`,
  `Flow` and `Layout`; `Ljos.Ui` no longer does. `Grid` is a one-rule
  native that is one line with no count, and the pass keeps a file where
  Qt's `Grid` positioner places a child elsewhere.
- DECISION (§4.6): `Pages` is a face of `StackLayout`, and `GridLayout`
  lowers to `Grid` only where its `flow` and counts say what `Grid` reads.
- DECISION (§4.7): gestures are child objects that read one pointer stream,
  not items. Qt's drag-and-drop `Drag` becomes `DragSource`/`DropArea`.
- DECISION (§4.8): typed lists with a `key` are the default model, and
  `ListModel` is for shared, edited lists, typed by its first element.
- DECISION (§4.9): `state` is the union of declared names, and there is no
  general state-machine type.
- DECISION (§4.3): there is no `date` in a Ljós file; `Instant`, `Clock` and
  `Locale` replace it.
- DECISION (§4.11): a component type is a value, no file is named by a
  string, and `destroy()` is not in a Ljós file.
- DECISION (§4.14): every construct the table marks "kept" keeps its file,
  for the reason given there.
- DECISION (§1, §4.21): the non-goal "no new syntax" is reversed. `match`
  and the Hack pipe `|>` are syntax of a QML file's scripts and of `.lts`
  modules, as TC39's proposals define them, compiled away by one desugar
  before the compiler, the checker, `tsc` or an editor reads them.
- DECISION (§4.21): a module that uses the new syntax ends in `.lts`,
  and is imported, typed and checked as a `.ts` module is. A `.ts` module
  stays plain TypeScript: the syntax in one is `ts-lts-syntax`, whose fix
  (and `ljos migrate`) renames it to `.lts`. `ljos check` emits each
  `.lts` module's declaration under `.ljos/types/`, TypeScript's own
  isolated-declarations emit (so exports carry their types,
  `lts-export-type`), so a `.ts` module that imports one still reads in
  stock `tsc` through a tsconfig that extends `.ljos/tsconfig.base.json`
  (`tsconfig-lts`). The project lead chose
  this over both QML-scripts-only and the syntax in `.ts` modules.
- DECISION (§4.17.1): the pipe is Hack-style with `%` as its topic, as
  TC39's proposal has it at commit `5ac4dfd` (2023-11-30).
- DECISION (§4.18): `match` and `is` follow TC39's proposal at commit
  `ba73ffe` (2025-10-03): guards are the pattern `and if (…)`, a bare name compares
  (Elixir's pin), predicates and the primitive constructors are
  patterns, and bindings in `is` are scoped to the block around it.
  Custom matchers on plain objects and extractors are in (2026-11-13);
  classes' matchers and the proposal's future enhancements (relational
  patterns among them) are not.
- DECISION (§4.18): `cond`-style chains get no form of their own: a
  guard alone, `when if (…):`, with `default:` as `cond`'s `true ->`.
- DECISION (§4.18): exhaustiveness is checked as ReScript checks it, and
  the message shows a case left out as a pattern.
- DECISION (§4.19): async work is WHATWG's `fetch` with `async`/`await`,
  over `Platform::request`, with nothing that polls; `Task` declares it.
  `XMLHttpRequest` is compat only.
- DECISION (§4.20): hooks are Controllers, attached to their host, on the
  one property and binding system, in QML or a TypeScript module.
- DECISION (§4.3.1): no `Qt` in a file that imports `Ljos.Ui`; every
  member has a home in Ljós's API and `ljos migrate` writes it.
- DECISION (§4.20, the project lead's, 2026-11-12): a Controller's
  lifecycle is named `attached`, `detached` and `updated`, handled with
  `onAttached`, `onDetached` and `onUpdated`; lit's
  `hostConnected`/`hostDisconnected`/`hostUpdated` are not taken.
- DECISION (§4.1, §4.3.1): `QtObject`'s name is `Object`. It is named only
  where a type goes, so `Object.keys(…)` in a script stays JavaScript's
  (the project lead's answer to §18, 2026-11-12).
- DECISION (§4.3.1): `Qt.binding`'s home is `bind(f)` in `Ljos.Core`,
  assignable to a property of `f`'s return type (the project lead's
  answer to §18, 2026-11-12).
- DECISION (§4.19): a Task's function is `run:` and the call that runs it
  is `start()`; lit's `task:` and `run()` are not taken. `run` reads as
  what the Task does, and `start` cannot be mistaken for it.

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
- **R5: checker soundness.** Since `ts-no-any`, what the checker cannot type is an error, except for a model's role, a member only the running program knows and an alias.
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
- **R12: TC39 moves.** Both proposals can change: pattern matching is
  at stage 1 and the pipe at stage 2. Each is pinned to a commit of its
  repository: pattern matching to
  `ba73ffe42f30f0cf56ece28bebdc28f955f2a52e` (2025-10-03, §4.18), the
  pipe to `5ac4dfd624e68b649e6f947885bf5ef67bac19f4` (2023-11-30,
  §4.17.1). Ljós follows them by moving a pin: a change is a desugar
  change and a `ljos migrate` rewrite of the old form, so files never
  fork from the proposal. A pin moves in its own change, which says what
  the proposal changed between the two commits.
- **R13: the cost of an arrow per step.** A pipe or a `match` compiled to
  an arrow called at once allocates a closure in tier 2. The budget gate
  (§11) measures it; tier 1 gives the topic a register.
- **R14: columns after the desugar.** The desugar keeps lines, not
  columns. Findings go through its span map; a tool that reads the
  desugared text without the map is off by columns, as in R3.
- **R15: a fetch that is never answered.** A request with no timeout holds
  a platform thread until the OS gives up. `AbortSignal.timeout(ms)` is
  the remedy, and the site's page uses it.
- **R16: one thread per request.** `Platform::request` starts an OS thread
  for each, which a burst of requests multiplies. A pool is the fix when a
  measurement asks for it.
- **R17: `.lts` is Ljós's.** Stock editors and `tsc` do not read an
  `.lts` module; without `ljos lsp` an editor shows it as plain text or
  with errors at the new syntax. `ljos check --tsc` and `ljos lsp` are
  the way in, and the site's editor page says how to set them up. A
  `.ts` module never needs them, including one that imports an `.lts`
  module: it reads in stock `tsc` and editors through the declarations
  `ljos check` emits under `.ljos/types/` and the base tsconfig its
  `tsconfig.json` extends (§4.21). Until the first `ljos check` there
  are none, and the import is not found. A project without a
  `tsconfig.json`, or with one that drops the base, is `tsconfig-lts`,
  which `ljos migrate` fixes. Between a save and the next check (or
  `ljos lsp`'s rewrite on save), an editor sees the module's types as
  they were.
- **R11: a Ljós file using a kept Qt sibling.** It sees Qt's names on it.
  The checker types it by Qt's table. That works, but the file mixes two
  APIs until the sibling migrates.

## 17. What phase 2 builds first

M1, in this order, from §6.4's evidence:

1. `ljos run --compat`, so a lowered app can be driven, not only rendered
   (built, without `--watch`);
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

## 18. Choices still open

Each changes how apps are written. A recommendation is given, but the
choice is the project lead's.

| Choice | Options | Recommendation |
|---|---|---|
| Focus scopes (§4.10) | (a) every component root is an implicit scope; (b) explicit `FocusScope`, as Qt | (a): it is what the corpus's `forceActiveFocus` calls work around, and lowering writes `Focus.scope: false` where it would differ |
| Translated text (§4.13) | (a) `qsTr` with `.arg` counts checked; (b) also a tagged template ``tr`…` ``; (c) only the template | (a) now, (b) later: (a) lowers with no edit, and (b) is sugar over the same catalogue |
| Static rows (§4.8) | (a) typed lists with `key`, ListModel for edits; (b) a typed ListModel for everything | (a): constant tables cost nothing at start, and reducers return lists |
| States (§4.9) | (a) typed unions plus `when:`; (b) a state-machine type | (a): 37 imperative assignments in 8 files do not pay for (b) |
| Lists as persistent vectors (§4.8) | (a) plain arrays, diffed in O(n); (b) persistent vectors, O(changes) | (a) now, measure, then (b) if a view of thousands of rows shows it |
