# Ljós behaviour specifications

Everything Ljós promises to a QML author is written here in Gherkin (ARCHITECTURE.md §18). This
tree is the source of truth for behaviour; Rust unit tests cover internals only. New behaviour starts
as a scenario here, then code.

## Directories

One directory per language or toolkit area, not per surface. Surfaces are tags.

| Directory | Covers |
|---|---|
| `language/` | QML language semantics: types, imports, JS expressions, signals |
| `bindings/` | property bindings, `Qt.binding`, `Binding`, loops |
| `components/` | component files, `Loader`, `Component`, inline components |
| `items/` | `Item`, `Rectangle`, geometry, visibility, opacity, clipping |
| `layouts/` | anchors, `Row`/`Column`/`Grid`, `QtQuick.Layouts` |
| `views/` | `ListView`, `GridView`, `Repeater`, models, delegates |
| `models/` | `ListModel`, what can be a model, what a delegate sees of its row, `Repeater`, `Instantiator` |
| `controls/` | one file per Qt Quick Controls type |
| `input/`, `focus/` | pointer, touch, keyboard, focus scopes, IME |
| `text/`, `animation/`, `i18n/`, `rendering/` | text, animation and timers, localisation, pixels |
| `text-input/` | `TextInput` and `TextEdit`: typing, keys, selection, the clipboard, undo, validators, echo modes, composing with an input method |
| `a11y/` | accessibility tree, names, roles, actions |
| `offscreen/` | resource policy: delegates near the viewport, freezing, memory |
| `hot-reload/`, `runtime/`, `network/`, `mobile/` | dev loop, scheduler and faults, network, phones |
| `tooling/` | MCP servers (`tooling/mcp/`), CLI |
| `parity/` | generated outlines (API ledger); never hand edited |

## Writing rules

- The reader is a QML author. Binding, property, delegate, anchor and signal are fine words.
- Never mention egui, kittest, frames, widgets, draw calls or crates. `mise run features:lint` flags them.
- Time is written "1 second passes" or "500 milliseconds pass".
- Observe through properties, the log, the accessibility tree and object counts.
- One behaviour per scenario. Use a `Scenario Outline` instead of copies.
- Reverse and failure paths are scenarios of their own: unbind, a file that does not parse, a binding loop.
- QML goes in a doc string labelled `qml`, at most 25 lines, one concept.
- Counts of live objects are bands (`at most 14`), never exact numbers Qt itself does not guarantee.
- Every file starts with a `# Sources:` block naming Qt *documentation* URLs or Ljós research
  reports. Never Qt or Slint source code (CLEAN_ROOM.md).
- A file with `@qt-parity` also names the Qt version it was verified against, on its own header line:
  `# Verified on Qt 6.11.2 (qml6, offscreen).`

```gherkin
# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Property bindings
```

## Tags

Tags on a `Feature`, `Rule` or `Examples` table apply to everything below them.

| Group | Tags |
|---|---|
| Surface | `@headless` (default when no surface tag), `@desktop`, `@android`, `@ios`, `@wgpu`. The headless runner skips scenarios that name only other surfaces. |
| Status | none = passes everywhere it applies · `@backlog` (passes nowhere yet) · `@backlog-<surface>` (does not pass on that surface yet) · `@dropped` (decided against; a comment gives the reason) · `@blocked` (a comment names what it waits for) · `@priority-high` / `@priority-low` |
| Oracle | `@qt-parity` · `@qt-diverges` · `@ljos-only` · `@qt-pixels` |
| Ledger | `@api-<Module>.<Type>.<member>` · `@a11y` |

**Status, as the runner applies it.** A normal run drops `@backlog`, `@backlog-<own surface>`,
`@dropped` and `@blocked`. `--backlog` runs only the backlog (`@backlog*` that is neither `@dropped`
nor `@blocked`). A `@backlog-android` scenario still runs headless: it only parks the Android surface.

**Oracle tags.**

- `@qt-parity`: the expectation was checked against real Qt and the scenario runs on both
  backends. This is the default for `language/` and `bindings/` (decision D14). Any difference needs `@qt-diverges`.
- `@qt-diverges`: a deliberate difference from Qt, runs on Ljós only. A comment line above it gives the reason.
- `@ljos-only`: an extension Qt does not have (hot reload, MCP), or something the Qt oracle cannot observe.
- `@qt-pixels`: compared with a PNG reference captured from Qt.
- No oracle tag: "our assumption". `features:lint` reports the share that is verified.

A `@backlog @qt-parity` scenario still runs on Qt in `features:parity`: the oracle verifies the
expectation before Ljós implements it.

## `# Sources:` rule

The first comment block of every file is a `# Sources:` block, one source per line. Comments that
follow it (such as `# Verified on Qt 6.11.2`) belong to the same header.

## Running

| Command | Does |
|---|---|
| `mise run features` | all non-backlog scenarios on the backend named by `LJOS_BDD_BACKEND` (default `ljos`) |
| `mise run features -- --backlog` | only backlog work |
| `mise run features:run [--backend qt] items bindings/loops.feature -t "@qt-parity" -n "regex"` | part of the ledger |
| `mise run features:parity` | every `@qt-parity` scenario on real Qt (`qml6`) |
| `mise run features:lint` | structure, undefined or ambiguous steps, sprawl |

`LJOS_BDD_BACKEND` is `ljos` (default), `qt`, or `both`. In `both`, a `@qt-parity` scenario runs on
Qt and Ljós and passes only if both do; scenarios without that tag run on Ljós only. While the Ljós
engine is not built, the `ljos` backend reports every operation as `PENDING: not implemented yet`
and those scenarios fail. Results go to `target/features/junit.xml`.

The ledger is not part of `cargo test --workspace` (its test target sets `test = false`); the
`mise` tasks run it explicitly with `--test features`.

## Step catalogue

Use these phrasings exactly. A scenario that needs something else first adds a step here and in
`crates/ljos_bdd/src/steps.rs`; `features:lint` fails on a phrasing that is not here.
`"…"` is a quoted string, `V` a value: `5`, `0.5`, `true`, `false`, `null`, `"text"` or `[1, 2]`.
Objects are named by `objectName`; `a/b` finds `b` below `a`. Properties may be dotted
(`anchors.margins`).

### Given

| Step | Meaning |
|---|---|
| `Given the QML:` + doc string | write the source as `Scenario.qml` and create it |
| `Given the file "X.qml":` + doc string | make `X.qml` available; nothing is created yet |
| `Given the translation file "X.ts":` + doc string | make the translations of a Qt Linguist file (doc string labelled `xml`) available for the language its `language` attribute names; give it before the QML |
| `Given an assistive technology is listening` | the accessibility tree is read from here on, so later steps see it follow each change |
| `Given a W by H image file "X.png" filled with "#rrggbb"` | make a one-colour image (`.png` or `.jpg`) available beside the QML files |
| `Given a file "X.png" that is not an image` | make a file with that name and no image inside available |
| `Given "text" was copied in another application` | the clipboard holds the text when the scenario starts |

### When

| Step | Meaning |
|---|---|
| `When "X.qml" is loaded` | create an instance of `X.qml`; objects become findable by `objectName` |
| `When the UI language is set to "…"` | what assigning `Qt.uiLanguage` does (`"is_IS"`, `"de"`, `""`) |
| `When the property "p" of "o" is set to V` | assign from JavaScript (breaks a binding) |
| `When the property "p" of "o" is bound to "expr"` | assign `Qt.binding(() => expr)`; `expr` names `o`'s properties unqualified |
| `When the method "m" of "o" is called` | call with no arguments |
| `When the method "m" of "o" is called with V, V` | call with arguments |
| `When N milliseconds pass` / `When N second(s) pass` | application time passes (`1 second passes`, `0.5 seconds pass`) |
| `When the user clicks the "role" "name"` | click the control with that accessible role and name |
| `When an assistive technology focuses the "role" named "name"` | what a screen reader's "move the focus here" does; an item may refuse |
| `When an assistive technology asks the "role" named "name" to increase` | any other request of a screen reader: `increase`, `decrease`, `take the value N`, `scroll down` (or `up`, `left`, `right`) and `scroll down a page`; an item may refuse (`@ljos-only`) |
| `When an assistive technology selects characters N to M of the "role" named "name"` | a screen reader selects in the node's text, from the first character to the second, which may be before it (`@ljos-only`) |
| `When an assistive technology replaces the selection of the "role" named "name" with "text"` | a screen reader types over the selection (`@ljos-only`) |
| `When an assistive technology sets the text of the "role" named "name" to "text"` | a screen reader types over the whole text (`@ljos-only`) |
| `When the user presses "chord"` | press and release a key chord: `"Ctrl+S"`, `"Shift+Tab"`, `"Return"`, `"A"` (key names as in a `Shortcut` sequence) |
| `When the user holds down "chord" until it repeats N times` | one press, `N` auto-repeated presses, one release |
| `When the window loses focus` / `When the window gains focus` | another window takes the keyboard, or gives it back |
| `When the user touches X, Y` | touch at a position |
| `When the user presses the pointer at X, Y` / `releases the pointer at X, Y` | the two halves of a click, for what happens while the button is held |
| `When the user moves the pointer to X, Y` | move the pointer, with the button held if it was pressed and not released (a drag), without it otherwise (hover) |
| `When the user types "text"` | one key press per character, with Shift held for a capital letter; on Qt only Latin-1 characters |
| `When the user double-clicks at X, Y` | two clicks at one place in quick succession |
| `When the user drags from X, Y to X, Y` | press, move with the button held, release |
| `When the user turns the wheel N notch(es) down at X, Y` | a mouse wheel turns with the pointer at a position; `down` (or `up`, `left`, `right`) is where the view goes over the content, so `down` is towards the content's end (`@ljos-only`: the oracle's input channel has no wheel) |
| `When the user scrolls N pixels down at X, Y` | a touchpad, or another device that scrolls by the pixel, scrolls that far, in the same four directions (`@ljos-only`) |
| `When the input method composes "text"` | an input method shows the text at the cursor as what is being composed; `""` ends the composition with nothing typed (`@ljos-only`: the oracle has no input method) |
| `When the input method commits "text"` | an input method ends the composition and types the text (`@ljos-only`) |
| `When the file "X.qml" is changed to:` + doc string | a file of the running application is saved with that text, or made if there was none; it is applied once the files have been quiet for 120 milliseconds (`@ljos-only`: hot reload is a Ljós extension) |
| `When the file "X.qml" is removed` | the file is deleted while the application runs (`@ljos-only`) |
| `When a reload happens` | the quiet time ends now: what was saved is applied, or refused with a reload error (`@ljos-only`) |

### Then

| Step | Meaning |
|---|---|
| `Then the property "p" of "o" is V` | equals; numbers compare numerically, colours are lower-case hex |
| `Then the property "p" of "o" is not V` | differs |
| `Then the property "p" of "o" is at least N` / `is at most N` | numeric bound |
| `Then the property "p" of "o" is within T of N` | numeric, give or take `T` (values an easing curve computes) |
| `Then setting the property "p" of "o" to V is rejected` | the application refuses the assignment |
| `Then loading "X.qml" is rejected` | the file does not load (syntax error, unknown type) |
| `Then the object "o" exists` / `does not exist` | lookup by `objectName` |
| `Then the log has a warning matching "regex"` | some warning matches |
| `Then the log has no warning matching "regex"` / `Then the log has no warnings` | none do |
| `Then the accessibility tree has a "role" named "name"` | role names are lower case: `button`, `checkbox`, `label` … |
| `Then the accessibility tree has no "role" named "name"` | |
| `Then the object "o" is exposed as a "role" named "name"` | the node whose `objectName` is `o` |
| `Then the "role" named "name" is described as "text"` | its accessible description; `""` when it has none |
| `Then the "role" named "name" has the value "text"` | what the node holds besides its name: the text of an editor; `""` when it has none |
| `Then the "role" named "name" has "text" selected` | the part of its value that is selected; `""` when none is |
| `Then the clipboard holds "text"` | the text a paste would insert |
| `Then the "role" named "name" is inside the "role" named "name"` | the second is its parent in the accessibility tree; the top is the `"window"` named `""` |
| `Then the accessibility tree's focus is the "role" named "name"` | the one node a screen reader reads as having the keyboard; the `"window"` named `""` when no item has it |
| `Then the reload error matches "regex"` | the message of the last reload that failed, which stays until a reload succeeds (`@ljos-only`) |
| `Then there is no reload error` | no reload has failed, or one succeeded since (`@ljos-only`) |
| `Then the reload error is in "X.qml" at line N` | where the mistake is (`@ljos-only`) |
| `Then N reloads have happened` / `Then 1 reload has happened` | changes applied since the application started (`@ljos-only`) |

### Planned (backlog scenarios may use them; not built yet)

`Given the inspector is connected` · `Given the agent has requested a snapshot` ·
`When the agent requests a snapshot` · `When the agent clicks the reference of the "role" named "name"` ·
`When the object "o" is destroyed` · `Then the snapshot has a "role" named "name" with a reference` ·
`Then the snapshot does not contain "text"` · `Then the agent is told the reference is stale` ·
`Then the signal "s" of "o" has fired once`.

## Adding a backend operation

Steps call the `Backend` trait in `crates/ljos_bdd/src/backend/mod.rs`; they never see egui or kittest
types. The Qt oracle (`backend/qt.rs`) replays the scenario's operations in a generated QML driver run
by `qml6` for every question a step asks (about 0.1–0.3 s each), so keep `@qt-parity` scenarios short.
It cannot observe animation midpoints, default accessible names or text pixels; such scenarios are
`@ljos-only` or `@qt-diverges`.

A scenario that presses keys or clicks runs `qml6` on Qt's `vnc` platform, the one that takes input.
That server has no password and listens on every interface, so the run gets a private network
namespace and a relay to a unix socket. This needs Linux with `unshare`, `ip` and `socat`, and
unprivileged user namespaces. Elsewhere such a scenario fails with a message that says so;
`LJOS_BDD_OPEN_VNC=1` runs the server on an open port of the host instead.

A pointer that is pressed, moved and released in steps of its own is sent to Qt one event a step, in
real time. Qt measures how fast a drag was going when it was let go, so a scenario that does not mean
a flick lets time pass before the release (`And 500 milliseconds pass`). The input channel has no
wheel: wheel steps are `@ljos-only`.
