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
| `Ljos.Ui` | items, visuals, gestures, states, text, images | `QtQuick` |
| `Ljos.Layout` | `Box`, `Row`, `Column`, `Grid`, `Flow`, `Pages`; `Layout.*` | `QtQuick.Layouts`, positioners |
| `Ljos.Controls` | the controls, built on `Ljos.Design` roles; today Basic's (§4.16) | `QtQuick.Controls`, `QtQuick.Controls.Basic` |
| `Ljos.Models` | `ListModel`, `XmlModel`, `FolderModel`, `TreeModel`; keyed views | `QtQml.Models`, `XmlListModel`, `FolderListModel` |
| `Ljos.Core` | `Timer`, `Component`, `Connections`, `Binding`, `Clock`, `Instant`, `Locale`, `App`, `Random`, value types (§4.3) | `QtQml`, the `Qt` namespace |
| `Ljos.Design` | `Theme` roles: colour, space, type, radius, motion (§4.12) | `palette`, `Material.*`, hand-rolled `Style` singletons |
| `Ljos.Platform` | `StandardPaths`, `FolderDialog`, `FileDialog`, notifications | `Qt.labs.platform`, `QtCore.StandardPaths` |
| `Ljos.Blocks` | functional blocks and shells (ADR 0003) | (Ljós's own, already built) |

DECISION: the module set is `Ljos.*`, one module per concern, named in the
singular. `Ljos.Ui` is the first and the only one phase 1 builds. It
imports `QtQml` for `Component`, `Timer` and the `Qt` namespace until
`Ljos.Core` exists.

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
  as they are.

It is described in `crates/qt_types/src/ljos.txt`, so `ljos check` and the
lowering know it with no Qt installed.

### 4.3 Globals

The `Qt` object (815 uses in 114 files) is one namespace for vectors,
colours, factories, time and the app. It is split by what its members are,
and nothing is global: a Ljós file sees only what it imports, plus the
ECMAScript built-ins the subset allows (`Math`, `JSON` through decoders,
`String`, `Number`, `Array` read-only methods, `Map`).

| Qt | Ljós | Kind | Cost |
|---|---|---|---|
| `Qt.vector2d/3d/4d`, `quaternion`, `matrix4x4`, `point`, `size`, `rect` | constructors: `vector3d(x, y, z)` in `Ljos.Core` | pure, tier 1 | a value, no host call |
| `Qt.rgba`, `hsla`, `darker`, `lighter`, `tint` | `color.rgba(…)`, `c.darker(f)` | pure, tier 1 | folded at compile time when constant |
| `new Date()`, a `Timer` that reads it | `Clock.now(Clock.Resolution.Second)`: a reactive `Instant` | a source | one wake-up per resolution tick for the whole app while anything reads it; a reader whose only effect is drawing is skipped while hidden, and any other reader (a handler, a `when:`, a non-visual object) keeps it ticking |
| `Date` values, `Date.parse` | `Instant` (UTC ms), `Instant.parseRfc2822`, `Instant.parseIso`, `i.since(j)` (a `Duration`) | pure | a number |
| `Qt.formatDateTime`, `Qt.locale()`, `toLocaleString` | `Locale.current.formatDate(i, Locale.Format.Short)` | pure, reactive on the locale | |
| `Qt.resolvedUrl` | `url("…")`, resolved against the file statically | compile time | none |
| `font.pointSize` | `Screen.points(pt)`: pixels for `pt` points on the item's screen (logical DPI / 72); arrives with `Screen` in M1 | a source, reactive on the item's screen | a multiply; re-evaluated only when the item moves to another screen or that screen's DPI changes |
| `Qt.binding` | a `Binding` object, or a state | declarative | |
| `Qt.createComponent`, `createQmlObject` | a component type is a value (§4.11) | typed | |
| `Qt.quit`, `exit`, `application.*`, `openUrlExternally` | `App.quit()`, `App.arguments`, `App.displayName`, `App.openUrlExternally(u)` | effects, typed | |
| `Qt.platform`, `styleHints`, `inputMethod` | `App.platform`, `Theme.motion`, `App.inputMethod` | reactive | |
| `Math.random` for a range | `Random.between(lo, hi)`, `Random.pick(list)`; `Math.random` stays | effect | |
| `Qt.callLater` | `later(() => …)` | scheduled, no polling | |
| `StandardPaths`, `FolderDialog` | `Ljos.Platform` | effects | |

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

DECISION: there is no `date` type in a Ljós file. An `Instant` is a number
of UTC milliseconds with methods, and a calendar date is formatted through
`Locale`. Qt's `date` lowers to `Instant` where it is only stored or
compared, and keeps its file where it uses `Date`'s local-time getters.

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
- A type predicate (`x is T`) parses, and is typed as a boolean.

The other direction is generated. Each QML component and native type has a
`.d.ts` declaration, so a TS module can take `Card` or `ListView` as a
parameter type. The declarations come from the same table `ljos check`
reads (§10, `ljos types --dts`), so `tsc` and an editor see what the
checker sees.

`var` is not a type in a Ljós file. A value of unknown shape enters at a
boundary through a declared decoder (note 11 §3.7). The decoder's result is
the only cast the checker allows. `rssnews` shows this at the network
boundary:

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
- `spacing` and `padding` (with sides) are on the container. A child's
  `Layout.margins` are its own.
- **Anchors are for a free parent only:** an `Item`, a `Rectangle`, or any
  non-arrangement. Anchors on a child of a Box are a check error
  (`layout-anchored`: "`anchors.left` on a child of a `Row` is overridden
  by it; use `Layout.alignment` or move it out of the Row"). Qt only warns,
  at run time. Overlap (z-stacking, `anchors.fill: parent`, a badge on a
  corner) is what anchors are for, and they stay.
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
  that reason.
- `GridLayout` → `Grid`, and `Grid` → `Grid`, kept under the same
  conditions as their row and column forms: a `Grid` positioner child with
  `Layout.*`, or a `GridLayout` child with an explicit size, no
  `preferred*` and an implicit size not statically 0.
- `StackLayout` → `Pages`.
- `Flow` → `Flow`.
- `LayoutItemProxy` keeps its file. It needs a person to merge the two
  layouts into one Box.

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
  `Tap { onTapped }` on the parent. A gesture on the parent sees a press
  after the parent's children, where the MouseArea sees it before the
  siblings declared ahead of it (it is drawn over them). The two agree
  unless an interactive sibling (a MouseArea, a handler, a control, a
  view) is declared before the MouseArea or the MouseArea sets `z`. Such
  a file is kept with that reason. 43 of the 45 lower; the two kept are
  `samegame/content/Button.qml` (a sibling before it) and
  `lightningviewer/SwitchMap.qml` (`z`).
- `hoverEnabled` + `containsMouse` → `Hover` + `hover.hovered`.
- `drag.target` → `Drag`.
- `*Handler` → the gesture of the same kind, with members renamed.
  Built: `TapHandler` → `Tap` and `HoverHandler` → `Hover`. What a handler
  accepts is what the gesture takes, named as a pointer event names what
  it carries: `acceptedButtons` → `buttons`, `acceptedModifiers` →
  `modifiers`, `acceptedDevices` → `devices`, `acceptedPointerTypes` →
  `pointerTypes`, with their change signals and handlers. An enum read
  through the handler's name follows it (`TapHandler.WithinBounds` →
  `Tap.WithinBounds`), and `PointerDevice` is in `Ljos.Ui` as it is.
  Such a member read off an object the pass cannot type (a `QtObject`
  property) keeps the file, as `color` read off an `Item` does.
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
since moving an item and carrying data are two different things.

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
  list. The row type is written into a `.ts` module beside the file, and
  each delegate's required properties become `modelData.<field>`. A
  ListModel that is edited stays a ListModel.
- `model.<role>` → `modelData.<role>`.
- `XmlListModel` → `XmlModel` with a generated decoder.
- `FolderListModel` → `FolderModel`.

DECISION: typed lists with keys are the default, and ListModel is for
shared, edited lists. A ListModel's row type is its `row:`, or else the
union of its `ListElement`s' roles, each optional where an element leaves
it out (Qt reads a missing role as `undefined`). A delegate that reads an
optional role has to say what it does without it. A constant table whose
elements differ lowers to a typed list with those optional fields.

DECISION: a duplicate `key` is a check error in a constant list. At run
time rows that share a key are matched in order (the second row with key
`k` keeps the delegate the second one had), and the view logs it once.

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
`Qt.createComponent("X.qml").createObject(p, {…})` with a static string →
`X.createObject(p, {…})`. A URL built at run time, `createQmlObject` and
`destroy()` keep their files (R1).

DECISION: `destroy()` is not in a Ljós file. An object a view made is
removed by its model, and one made by `createObject` is owned by its parent
and removed with `parent.remove(child)`.

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
  as it does in Qt (§4.3). `Screen` is not in `Ljos.Ui` until M1, so until
  then a file that sets `pointSize` is kept.
- `font.bold: true` → `font.weight: Font.Weight.Bold`.
- `wrapMode: Text.WordWrap` → `wrap: Text.Wrap.Word`, and every other enum
  the same way, by its owner in the table.

### 4.14 Every Qt construct, and where it goes

| Qt | Ljós | Lowering |
|---|---|---|
| `Row`, `Column` | `Row`, `Column` (a Box) | mechanical; kept when a child has `Layout.*` (§4.6) |
| `RowLayout`, `ColumnLayout` | `Row`, `Column` | mechanical; kept when an explicit size meets a non-zero implicit size |
| `GridLayout`, `Grid` | `Grid` | mechanical; kept as `RowLayout` and `Row` are |
| `StackLayout` | `Pages` | mechanical |
| `Flow` | `Flow` | mechanical |
| `LayoutItemProxy` | a Box whose `direction` is bound | DECISION: kept; a person merges the layouts |
| `anchors.*` on a free parent | the same | none |
| `anchors.*` on a layout child | `Layout.alignment`/margins | DECISION: kept (Qt overrides them at run time, so their effect is unclear) |
| `MouseArea` filling its parent, no children | `Tap`/`Hover`/`Drag` | mechanical (43 of 45); kept when an interactive sibling is declared before it or it sets `z` (§4.7) |
| other `MouseArea` | `PointerArea` | mechanical (phase 1), gestures M3 |
| `*Handler` | `Tap`, `Drag`, `Pinch`, `Hover`, `Wheel`, `LongPress` | mechanical, by member (§4.7); `Tap` and `Hover` built |
| `PinchArea`, `MultiPointTouchArea` | `Pinch`, gestures | DECISION: kept until M3 |
| attached `Drag`, `DropArea` | `DragSource`, `DropArea` | mechanical |
| `Flickable` | `Flickable` | none |
| `ListModel` of constants | `list<T>` | mechanical, with a generated row type |
| `ListModel` edited | `ListModel` | none |
| `XmlListModel`, `FolderListModel` | `XmlModel` (decoder), `FolderModel` | mechanical |
| `model.<role>` | `modelData.<role>` | mechanical |
| `DelegateModel`, `ObjectModel`, `Package` | — | DECISION: kept (0–1 uses) |
| `State`, `PropertyChanges { target: }` | `PropertyChanges { id.prop: }` | mechanical |
| `StateGroup` | the same | none |
| `ParentChange` | — | DECISION: kept (0 uses) |
| `Loader { source: "X.qml" }` | `Loader { component: X }` | mechanical when the string is static |
| `Qt.createComponent(...).createObject` | `X.createObject` | mechanical when the string is static |
| `createQmlObject`, `destroy()` | — | DECISION: kept (R1) |
| `Component { id }` | an inline `component X:` or an inline delegate | mechanical |
| `palette.*` | `Theme.color.*` | mechanical where 1:1 |
| `Material.*`, `Universal.*` | — | DECISION: kept (a style's own API) |
| `font.pixelSize`, `font.bold` | `font.size`, `font.weight` | mechanical |
| `font.pointSize` | `font.size: Screen.points(…)` | kept until `Screen` lands (M1), then mechanical (a run-time conversion) |
| `horizontalAlignment`, `wrapMode`, `elide`, `textFormat`, `fillMode` | `align`, `wrap`, `elide`, `format`, `fit` | mechanical |
| `Qt.*` globals | `Ljos.Core`, `App`, value types (§4.3) | mechanical, by member |
| `date`, `Date` | `Instant`, `Locale` | mechanical when only stored or compared, else kept |
| `Screen.primaryOrientation` | the window's aspect | DECISION: kept; the meaning differs on desktops |
| `Screen.pixelDensity` | — | DECISION: kept; it is physical pixels per mm, not the logical DPI `Screen.points` uses, and the corpus has no use of it |
| `property var` | `property unknown`, or a type | mechanical |
| `Qt.labs.platform`, `StandardPaths` | `Ljos.Platform` | mechanical |
| `WorkerScript` | — | DECISION: kept (0 uses); a TS worker module later |
| `Canvas`, `Shape` | the same | none |
| `QtQuick.Controls.Basic` | `Ljos.Controls` | mechanical, for the controls the module has (§4.16) |
| `QtQuick.Controls` | `Ljos.Controls` | mechanical where the program starts with Basic; kept with another style (§4.16) |
| `QtQuick.Controls.Material`, `.Fusion`, `.Universal`, `.impl`, `QtQuick.Templates` | — | DECISION: kept (a style's own API, or the parts a style is built from) |

### 4.15 What this changes for the engine

- **Box** is one native arrangement with the positioner fast path. It is the
  D1 spike.
- **Gestures** are recognisers on one pointer stream, attached to an item's
  hit test. They have no items and no geometry.
- **Keyed views** diff a list by key. `enter`/`exit` live on the view, which
  keeps an exiting delegate until its transition ends.
- **Clock** is one timer per resolution, scheduled by its readers. A
  reader whose only effect is drawing does not count while it is hidden;
  a handler, a `when:` or a non-visual object always counts, so a hidden
  page's timeout still fires. With no reader that counts, there is no
  timer (rule 6).
- **Decoders** run at a model's boundary in tier 2. The rows they return
  are values.
- **Theme** is a singleton with a live-reloaded source.

### 4.16 Controls

A file in Ljós's API imports its controls from `Ljos.Controls`:

```qml
import Ljos.Ui
import Ljos.Controls

Item {
    Button { text: "Save"; ToolTip.text: "Writes the file"; ToolTip.delay: 500 }
    Label { text: "Ready" }
}
```

The module grows a slice at a time. Each control in it is the Basic
style's, as `QtQuick.Controls.Basic` has it: the same style file over the
same template, made, drawn and taking input as Qt's, so a lowered file and
the file as written are the same program and draw the same pixels. What
the module has is in `ljos.txt` (written by the own_types test), and the
compat pass lowers a file only when every control it names is there.

| Slice | Controls | State |
|---|---|---|
| K1 | `Label`, `AbstractButton`, `Button`, attached `ToolTip`, `ScrollBar` (and its attached form), `ScrollView`, `Popup` | built |
| K2 | `TextField`, `TextArea` | |
| K3 | `Menu`, `MenuItem`, `MenuSeparator`, `Dialog`, `ItemDelegate` | |
| K4 | `CheckBox`, `Switch`, `ComboBox`, `SpinBox`, `Slider` | |

Lowering (§6.1): `import QtQuick.Controls.Basic` becomes
`import Ljos.Controls` on its line, and a second import of the controls
becomes an empty line. `import QtQuick.Controls` is the style the program
starts with (`QT_QUICK_CONTROLS_STYLE`, then the file
`QT_QUICK_CONTROLS_CONF` names, then the platform's: Fusion on Linux), so
it is lowered only where that is Basic. `ljos migrate --style NAME` says
what the program starts with; without it, migrate takes what a program
started there would get. A control the module has not taken yet, or one of
its enums (`Dial.SnapAlways`), keeps the file with "`Dial` is not in
Ljos.Controls yet". A `Rectangle` given to a control (`background:`) is
still a `Rectangle`, so its `color` becomes `fill`; a member read off what
the style gave (`b.background.color`) keeps the file, because that may be
a `Rectangle`.

Cost: none at run time. The module registers the files the Basic style
already compiled, under a second name.

DECISION: `Ljos.Controls` draws Basic, not `Ljos.Design` roles. Building
on the roles is a rewrite of the style, so it stays in M6.

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
| ✓ `ts-no-cast` | No `as` except to a declared decoder's result. No `!`. |
| ✓ `ts-switch-exhaustive` | A `switch` on a union covers every case. |
| ✓ `ts-readonly` | Records and arrays are readonly unless declared mutable. Writing a QML property or a model is allowed (Q6, §14). |
| ✓ `ts-index-undefined` | Indexing gives `T \| undefined` (`noUncheckedIndexedAccess`). |
| ✓ `ts-optional-exact` | An optional member is not set to `undefined` explicitly (`exactOptionalPropertyTypes`). |
| ✓ `ts-variance` | Method parameters are contravariant, and mutable arrays are invariant. |
| ✓ `ts-forbidden` | No `class`, `this` outside QML's scope object, prototypes, `delete`, `for…in`, `with`, `eval`, or `arguments`. |
| `ts-boundary` | A compat `.js` module is called only through a `.d.ts` declaration (Q7, §14). |

The names a TypeScript module exports are held to two more: `unknown-member`
for a value it does not export (`Logic.total`), and `unknown-type` for a
type it does not (`property Logic.Rows`). `ts-boundary` waits for the
decoders (M4, T3b).

Not built yet, though the rules above cover them:

- a value type's readonly fields: `pos.x = 1` on a `point` is not
  reported;
- a QML property declared `readonly` and assigned from a script is not
  reported, as the type table gives the checker no readonly flag (the
  `.d.ts` of `ljos types` has one);
- `PointerArea`'s handlers give their event as `QtObject` in the type
  table, so reading `event.x` is `ts-no-any` until the table types it.

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
missing, with what gives it one. A method the type table names without
types takes and gives `unknown`. It lets by, without a type, a model's
role, a member only the running program knows, and an alias (whose type is
its target's).

How it holds the other rules:

- `ts-no-cast`: `as T`, `<T>x` and `x!` are errors, at the `as`, the
  `<T>` or the `!`. `as const` asserts nothing and is allowed. Until
  decoders (M4), no cast is a decoder's result.
- `ts-switch-exhaustive`: a `switch` with no `default` on a union of
  literals (strings, numbers, booleans, `undefined`) names the cases it
  leaves out. A switch on a wider type, or on a QML enum, whose keys Qt
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

A `QtQml` import stays. `QtQuick.Layouts` and the controls
(`QtQuick.Controls.Basic`, and `QtQuick.Controls` where the program starts
with Basic) are lowered (§4.6, §4.16). Any other module import
(`QtQuick.Controls.Material`, an aliased `QtQuick`) keeps the file.

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
- modules imported by name: a module of the project is a directory whose
  path ends in the name's parts (`import My.Controls` is any `…/My/Controls/`),
  as an import path finds it, and every file below it is of the group;
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
| A type or module the API has not taken (`Column`, `Behavior`, `Dial`, `QtQuick.Controls.Fusion`, …) | Lowered as the API grows (M1, M6) |
| `import QtQuick.Controls` in a program that starts with a style other than Basic | Lowered once `Ljos.Controls` has that style (M6) |
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
the controls keep instead:

| Reason | Files |
|---|---:|
| a file of its group is kept | 71 |
| `QtQuick.Controls.Fusion` (15), `.impl` (7), `.Material` (5) | 27 |
| a control not in `Ljos.Controls` yet: `StackView` (4), `ApplicationWindow` (4), `RoundButton` (3), `TextField`, `Switch`, `Slider`, `Pane`, `Page` (1 each) | 16 |

With Fusion (the Linux default), 89 files are kept by `import
QtQuick.Controls`. The lowered count does not move yet: the files the
import let through are kept by the other types they name, or by their
group.

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
ljos migrate [--write] [--style NAME] PATH…
would lower qml/SimpleButton.qml (5 edits)
kept qml/Main.qml:12:5: `Column` is not in Ljos.Ui yet
2 files, 1 lowered, 1 kept
```

- It runs the same pass and the same grouping as `--compat lower`. A
  migrated file is the text a lowered run compiled, byte for byte, and it
  compiles to the same program.
- A lowered file says its scripts are still Qt's JavaScript:
  `pragma Scripts: JavaScript`, on the import's line so that every line
  stays where it was. The checker does not hold such a file to its rules,
  so `==`, `null`, `var` and `if (count)` keep compiling as they did.
  Taking the pragma out is the step that types a file, and `ljos check`
  then says what to change. Renaming the API and typing the scripts are
  two steps, and only the first is mechanical.
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

| M | Builds | Exit criteria |
|---|---|---|
| **M1** Ui and Layout | `ljos run --compat`; `Ljos.Layout` (Row/Column/Grid/Flow and `QtQuick.Layouts`); `Ljos.Ui` items the corpus keeps for (`Image`, `Behavior` and animations, `ListView`, `Flickable`, `Repeater`, `Loader`, `Window`, `Screen`); the gestures, for the `*Handler`s (§4.7); a load-time budget gate. Built: `ljos run --compat keep\|lower` without `--watch` (a watched run refuses `lower`: its saves are recompiled as written); `Tap` and `Hover` (`TapHandler`, `HoverHandler`), by member. Remaining: `--watch` with `lower`; `Drag`, `Pinch`, `Wheel`, `LongPress`, a filling `MouseArea` as gestures, `DragSource`, `takeOver`; then the rest | the features pair for each rename; compat sweep identical; ≥ 25 % of corpus example files lowered |
| **M2** Core | `Ljos.Core` (`Timer`, `Component`, `Connections`, value-type constructors, `App`, `later`), with `Qt.*` lowered to it and `Ljos.Ui`'s `QtQml` import dropped | sweep identical; no lowered file reaches the `Qt` object |
| **M3** Tools | migrate on `qml_edit`; MCP `migrate`, `explain`; Studio shows the API; enum cleanup (`Qt.Align*`) | migrate round-trip on the corpus re-lowers to itself; Studio edits a Ljós file losslessly |
| **M4** Types | TS modules imported by QML; `property T` of TS types; generated `.d.ts`; decoders; every rule of §5.1; tier-1 typed opcodes. Built: TS modules, `property T` of their types, `tsc` given the modules (§4.4). Remaining: decoders and `JSON.parse` as `unknown`, `ts-boundary` (T3b); tier-1 typed opcodes | a Ljós file passes `tsc --noEmit` with the generated `.d.ts` iff the checker accepts its scripts' types; tier-1 coverage of the features' Ljós files 100 % |
| **M5** Natives renamed | natives take the API's names; faces removed; `QtQuick` becomes a table only the pass reads; `createQmlObject` text lowered at the call | no Qt name in `ljos_engine`/`ljos_quick` outside the table; budget test equal or better |
| **M6** Controls | `Ljos.Controls` and styles. Built: the module, with Basic's `Label`, `AbstractButton`, `Button`, `ToolTip`, `ScrollBar`, `ScrollView`, `Popup` (K1); `QtQuick.Controls.Basic` lowered to it, and `QtQuick.Controls` where the program starts with Basic (§4.16). Remaining: the other controls (K2–K4 and the rest), the controls on `Ljos.Design` roles, the other styles | Controls corpus lowered; qml6 parity on its features |

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
- DECISION: a file is lowered all-or-nothing, and so is a group of files
  that name one another. Grouping is conservative.
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
- DECISION: compat JS is called from TS only through declarations (Q7).
- DECISION: phase 2 builds modules before globals: `Ljos.Layout` and
  the kept-for `Ljos.Ui` types (M1) before `Ljos.Core` (M2). Module imports
  keep 364 of 462 kept example files, and `Qt.*` keeps none.
- DECISION: the edits are text replacements at spans that keep every line,
  and they move onto `qml_edit` in M3.
- DECISION (§4.6): one arrangement, the Box, with one size rule per axis:
  `Layout.preferred*`, else an explicit size, else the implicit size.
  Anchors on a Box child are a check error.
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
