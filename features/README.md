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
| `window/` | `Window` as a window: showing and hiding, `visibility`, size limits, `close()` and `closing`, which window is active, `transientParent`, `screen`, `Application`, `Qt.quit()` |
| `input/`, `focus/` | pointer, touch, keyboard, focus scopes, IME |
| `text/`, `animation/`, `i18n/`, `rendering/` | text, animation and timers, localisation, pixels |
| `states/` | `State` and what it changes (`PropertyChanges`, `AnchorChanges`, `ParentChange`, `StateChangeScript`), `StateGroup`, the `Binding` element; a `Transition` between states is under `animation/` |
| `shapes/` | `QtQuick.Shapes`: `Shape`, `ShapePath` (strokes, joins, caps, dashes, fill rules), the linear, radial and conical fill gradients, `Shape.FillContains`; `Path` and its elements, `PathSvg` data and its errors |
| `text-input/` | `TextInput` and `TextEdit`: typing, keys, selection, the clipboard, undo, validators, echo modes, composing with an input method |
| `a11y/` | accessibility tree, names, roles, actions |
| `offscreen/` | resource policy: delegates near the viewport, freezing, memory |
| `hot-reload/`, `network/`, `mobile/` | dev loop, network, phones |
| `runtime/` | faults (a script's exception, a panic in a native type, a script that is interrupted), `ErrorBoundary` and `Loader` as the boundaries that contain them, `Component.incubateObject` |
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
| Build | `@no-qt-table`: what only a build says that reads QML against Ljós's own description of its types alone (`LJOS_QT_TYPES=none`, or a checkout where `mise run types` never ran). A build with a table of Qt's types does not run it. |

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
| `mise run features:acceptance <suite> [--path DIR] [--raise]` | an external suite's scenarios on Ljós, held by its `RATCHET` (see below) |
| `mise run features:acceptance-test` | the acceptance mechanism's own tests, on the example suite |
| `mise run gallery` | the qtdoc examples on Ljós against qml6 pictures, held by `corpus/expected.json` (see below) |

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
| `Given the file "X.qml":` + doc string | make `X.qml` available; nothing is created yet. The name may be a path (`kit/Badge.qml`), and a file named `qmldir` (`kit/qmldir`) describes the module of its directory |
| `Given the translation file "X.ts":` + doc string | make the translations of a Qt Linguist file (doc string labelled `xml`) available for the language its `language` attribute names; give it before the QML |
| `Given an assistive technology is listening` | the accessibility tree is read from here on, so later steps see it follow each change |
| `Given the system setting "color scheme" is "dark"` | what the system says before the application starts: `"color scheme"` (`"light"`, `"dark"`), `"contrast"` (`"normal"`, `"high"`), `"reduced motion"` and `"screen reader"` (`true`, `false`), `"font scale"` (a number; `1` is the size the application was written for) (`@ljos-only`: the oracle cannot be given a system) |
| `Given a W by H image file "X.png" filled with "#rrggbb"` | make a one-colour image (`.png` or `.jpg`) available beside the QML files |
| `Given a file "X.png" that is not an image` | make a file with that name and no image inside available |
| `Given "text" was copied in another application` | the clipboard holds the text when the scenario starts |
| `Given the HTML "html" was copied in another application` | the clipboard holds formatted text, as a rich text editor elsewhere copies it: the HTML (single quotes for attributes), and its characters as plain text; give it after the QML |
| `Given the font file "X.ttf"` | make a font file of `features/text/fonts` available beside the QML files under its own name |
| `Given a file "X.ttf" that is not a font` | make a file with that name and no font inside available |
| `Given the text file "X":` + doc string | make a text file available beside the QML files, each line trimmed of the scenario's indentation |
| `Given the user has the text file "path":` + doc string | a text file under the user's home directory (`.config/tool.conf`), which is `/home/user` in a scenario (`@ljos-only`: the oracle's home is the real one) |
| `Given the system clock reads "2026-10-11T17:57:09Z"` | the time a script's `new Date()` and `Date.now()` see when the application starts (an RFC 3339 time); it then moves with application time, so `1 second passes` moves it by a second (`@ljos-only`: the oracle's clock is the machine's) |
| `Given the system cannot take its dialogs away` | a dialog the system shows stays up when the application closes it, until the user ends it (`@ljos-only`) |
| `Given the system asks the user before it grants a permission` | a permission starts undetermined and `request()` asks the user, as a phone does; give it before the QML (`@ljos-only`: desktop Linux grants everything) |

### When

| Step | Meaning |
|---|---|
| `When "X.qml" is loaded` | create an instance of `X.qml`; objects become findable by `objectName` |
| `When the UI language is set to "…"` | what assigning `Qt.uiLanguage` does (`"is_IS"`, `"de"`, `""`) |
| `When the system setting "font scale" changes to 1.5` | the user changes a setting of the system while the application runs; the settings and values of `Given the system setting` (`@ljos-only`) |
| `When the property "p" of "o" is set to V` | assign from JavaScript (breaks a binding) |
| `When the property "p" of "o" is bound to "expr"` | assign `Qt.binding(() => expr)`; `expr` names `o`'s properties unqualified |
| `When the method "m" of "o" is called` | call with no arguments; calling a signal emits it |
| `When the method "m" of "o" is called with V, V` | call with arguments |
| `When N milliseconds pass` / `When N second(s) pass` | application time passes (`1 second passes`, `0.5 seconds pass`) |
| `When the user clicks the "role" "name"` | click the control with that accessible role and name |
| `When an assistive technology focuses the "role" named "name"` | what a screen reader's "move the focus here" does; an item may refuse |
| `When an assistive technology asks the "role" named "name" to increase` | any other request of a screen reader: `press`, `increase`, `decrease`, `take the value N`, `expand`, `collapse`, `show itself`, `scroll down` (or `up`, `left`, `right`) and `scroll down a page`; an item may refuse (`@ljos-only`) |
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
| `When a native handler of "o" panics with "message"` | a method of a native type panics while it runs for that object, as a bug in the type would (`@ljos-only`: faults and their boundaries are Ljós's own) |
| `When a reload happens` | the quiet time ends now: what was saved is applied, or refused with a reload error (`@ljos-only`) |
| `When the tests are run` | run every `tst_*.qml` file given with `Given the file` with the test runner (`ljos test`; `qmltestrunner` on Qt); nothing is loaded as an application |
| `When the tests are run with the import path "dir"` | the same, with a directory of test-only modules given to the runner (`-import`) |
| `When the tests matching "Case::test_x" are run` | only the named test function (`Case::function`), or one of its data rows (`Case::function:tag`) |
| `When the files are checked` | read every file given with `Given the file` as one project with `ljos check`; nothing is loaded or run (`@ljos-only`: Qt has no program of the kind) |
| `When the user chooses "path" in the system dialog` | the user picks a file or folder in the dialog the system shows (`@ljos-only`: the oracle cannot answer a dialog the system shows) |
| `When the user chooses the files "path, path" in the system dialog` | the user picks several files, named with `", "` between them (`@ljos-only`) |
| `When the user answers "button" in the system dialog` | the user presses the button with that text in the message the system shows (`@ljos-only`) |
| `When the user dismisses the system dialog` | the user closes the dialog the system shows without choosing (`@ljos-only`) |
| `When "text" is copied in another application` | another application puts the text on the clipboard while this one runs; it is not told, and finds out when it asks |
| `When the user grants the "name" permission` / `denies` | the user answers the system's question for a permission: `"camera"`, `"microphone"`, `"location"`, `"calendar"`, `"contacts"`, `"bluetooth"` (`@ljos-only`) |

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
| `Then the pixel at X, Y of the window is "#rrggbb"` / `is within T of "#rrggbb"` | one pixel of the window's picture, laid over black; `T` is how far each of red, green and blue may be off (a blend is rounded differently by each painter) |
| `Then the accessibility tree has a "role" named "name"` | role names are lower case: `button`, `checkbox`, `label` … |
| `Then the accessibility tree has no "role" named "name"` | |
| `Then the object "o" is exposed as a "role" named "name"` | the node whose `objectName` is `o` |
| `Then the "role" named "name" is described as "text"` | its accessible description; `""` when it has none |
| `Then the "role" named "name" has the value "text"` | what the node holds besides its name: the text of an editor; `""` when it has none |
| `Then the "role" named "name" has "text" selected` | the part of its value that is selected; `""` when none is |
| `Then the clipboard holds "text"` | the text a paste would insert |
| `Then the text file "X" has the line "text"` | a file the application wrote beside the QML files (or `Given` there) has that whole line, once what it writes in the background is on disk (`@ljos-only`: the oracle's files cannot be read back) |
| `Then the text file "X" does not exist` | the application has not written that file (`@ljos-only`) |
| `Then the system shows a "kind" dialog titled "title"` | the system's own dialog is up: `"open file"`, `"open files"`, `"save file"`, `"folder"`, `"message"`, `"color"`, `"font"` (`@ljos-only`) |
| `Then the system shows no dialog` | no dialog of the system's is up (`@ljos-only`) |
| `Then the system asks the user for the "name" permission` | the system has asked the user for that permission and has no answer yet (`@ljos-only`) |
| `Then the system asks the user for no permission` | no question about a permission is waiting (`@ljos-only`) |
| `Then the "role" named "name" is inside the "role" named "name"` | the second is its parent in the accessibility tree; the top is the `"window"` named `""` |
| `Then the accessibility tree's focus is the "role" named "name"` | the one node a screen reader reads as having the keyboard; the `"window"` named `""` when no item has it |
| `Then the "role" named "name" is checked` / `is not checked` | a state a screen reader reads of the node: `checkable`, `checked`, `partially checked`, `pressed`, `expandable`, `expanded`, `selectable`, `selected`, `read-only`, `disabled`, and `live` (a change of it is said without the user going to it) or `assertive` (said at once) (`@ljos-only`: what Qt hands an assistive technology cannot be read from QML) |
| `Then the "role" named "name" is item N of M` | its place among the ones it is listed with, from 1, and how many there are (`@ljos-only`) |
| `Then the "role" named "name" is not an item of a set` | it tells no place (`@ljos-only`) |
| `Then the "role" named "name" is at N in the range N to N` | the value of a slider, a spin box or a scroll bar, and its least and greatest (`@ljos-only`) |
| `Then the "role" named "name" has no range` | it tells no value of that kind (`@ljos-only`) |
| `Then the "role" named "name" is labelled by the "role" named "name"` | the second is what names the first (`Accessible.labelledBy`) (`@ljos-only`) |
| `Then the "role" named "name" is labelled by nothing` | (`@ljos-only`) |
| `Then the "role" named "name" is called a "words"` | what a screen reader says the node is when its role has no name of its own on the platform (`@ljos-only`) |
| `Then the reload error matches "regex"` | the message of the last reload that failed, which stays until a reload succeeds (`@ljos-only`) |
| `Then there is no reload error` | no reload has failed, or one succeeded since (`@ljos-only`) |
| `Then the reload error is in "X.qml" at line N` | where the mistake is (`@ljos-only`) |
| `Then N reloads have happened` / `Then 1 reload has happened` | changes applied since the application started (`@ljos-only`) |
| `Then the application was asked to quit` / `Then the application was not asked to quit` | whether `Qt.quit()` was called, which a host answers by ending (`@ljos-only`: the oracle is itself the application that would end) |
| `Then the application has no fault` | no fault got past every boundary, or the one that did was cleared (`@ljos-only`) |
| `Then the application fault is a "kind" matching "regex"` | the fault no boundary contained, which the host shows: its kind (`"JsException"`, `"JsInterrupted"`, `"JsOutOfMemory"`, `"Panic"`, `"Startup"`) and its message (`@ljos-only`) |
| `Then the test report is:` + doc string | everything the test runner printed, without its banner lines, the directory of the files, the runner's own name in front of a test, and the time after `skipped` |
| `Then the test report has the lines:` + doc string | those lines are in the report, one after the other |
| `Then the test run exits with status N` | the runner's exit status: the number of failed tests |
| `Then the test results' "/pointer" is V` | a value of the machine-readable report, by JSON pointer (`@ljos-only`) |
| `Then the check reports:` + doc string | everything `ljos check` said, in order: one line for each thing it found (`file:line:column: error: ...` or `warning: ...`), its count of files, errors and warnings, and `ljos: N errors` when there are errors (`@ljos-only`) |

### The inspector (`tooling/mcp/`; all `@ljos-only`)

An agent asks the development build's inspector (ARCHITECTURE.md 19.4) through a real MCP client.
In a quoted text `\"` is a quote. A locator is written as the agent writes it
(`role=button[name=\"Save\"]`, `#objectName`, `text=…`, `ref=e3`). A `"/pointer"` is a JSON pointer
into the answer's structured content.

| Step | Meaning |
|---|---|
| `Given the inspector is connected` | an agent with the `inspect` capability is connected; before or after the QML |
| `Given the inspector is connected and masks every editable value` | the same, started so that the value of every editable field is masked, not only passwords |
| `Given the inspector is connected with the capabilities "inspect, input"` | the agent was granted those capabilities (`--mcp=` names them: `inspect`, `input`, `mutate`, `eval`); `… and masks every editable value` as above |
| `Given the files are watched` | the host watches the files, so that `app_reload` can be answered and reloads are told; the scenario also notes what a reload needs from the first load on |
| `Given the host says the application has N windows` | what the host tells the inspector about its windows (`Inspector::set_windows`); `app_status` gives the count and says only the primary is inspected |
| `Given the inspector is served on a socket` / `When the inspector is stopped` | the same on a Unix socket in a directory of the scenario; stopped as with the application |
| `When the inspector is asked to listen on "addr" with the word for a network` / `without …` | `--mcp-bind` with or without `--mcp-insecure-lan`. An address of this machine is served; one beyond it is only checked |
| `When the inspector is asked to listen on a socket whose path is N characters long` / `… as long as the system allows` | `--mcp-socket` with a path of that length, made in a short directory of its own (never under `TMPDIR`). Too long is refused with the path, its length and the limit; the longest allowed is served |
| `Given the inspector is served on the loopback address` | the inspector also listens on `127.0.0.1` with a token file, as `ljos run --mcp` serves it |
| `When the agent requests a snapshot` / `Given the agent has requested a snapshot` | `app_snapshot` |
| `When the agent requests a snapshot of at most N nodes` / `Given the agent has requested a snapshot of at most N nodes` | `app_snapshot` with `max_nodes` |
| `When the agent requests the next part of the snapshot` | `app_snapshot` with the cursor the cut snapshot gave |
| `When the agent requests a snapshot below "locator"` | `app_snapshot` of one subtree |
| `When the agent requests a snapshot N levels deep` | `app_snapshot` with `depth` |
| `When the agent requests what changed since the last snapshot` | `app_snapshot` with `since` |
| `When the agent searches for "locator"` / `Given the agent has searched for "locator"` | `app_find` |
| `When the agent inspects "locator"` | `app_inspect` |
| `When the agent inspects every property of "locator"` | `app_inspect` with `all`, which also lists properties at their default |
| `When the agent inspects the reference of the "role" named "name"` | `app_inspect` with the reference a snapshot or a search gave for that item |
| `When the agent calls the tool "name"` / `Given the agent has called the tool "name"` | any tool, without arguments |
| `When the agent calls the tool "name" with:` + doc string | any tool, with the arguments in the doc string (labelled `json`) |
| `When the agent calls the tool "name" on the reference of the "role" named "name"` (also `… with:` + doc string) | any tool with `target` set to the reference a snapshot or a search gave for that item; the doc string holds the other arguments |
| `When the agent calls the tool "app_batch" with N steps of "tool"` | a batch of N steps that press a key, for the limit on steps |
| `When the agent asks for the log since its last request` | `app_logs` from where the last `app_logs` answer ended |
| `When the agent reads the resource "ljos://…"` | an MCP resource |
| `When a client asks on the socket for a snapshot with the right token` | as below, on the Unix socket; also `no token`, `a wrong token` |
| `When a client asks for a snapshot with the right token` | a plain HTTP client of the served inspector calls `app_snapshot`. Also `no token`, `a wrong token`, `the token of an older start`; and `… from the origin "https://…"` or `… for the host "name"` after the token |
| `When the method "m" of "o" is called N times` | the method, N times over |
| `Then the snapshot is:` + doc string | the whole outline (labelled `text`), or the whole list of changes |
| `Then the snapshot contains "text"` / `does not contain "text"` | part of the outline |
| `Then the snapshot matches "regex"` / `does not match "regex"` | |
| `Then the snapshot has a "role" named "name" with a reference` / `Then the snapshot has no "role" named "name"` | one line of the outline |
| `Then the snapshot lists N nodes` | how many items this answer lists |
| `Then the snapshot is complete` / `Then the snapshot is cut short` | whether there is a next part |
| `Then the search finds N nodes` / `Then the search finds a "role" named "name"` | |
| `Then the answer's "/pointer" is V` / `matches "regex"` / `is at least N` | a value of the last answer |
| `Then the answer's "/pointer" has N entries` / `is absent` | a list or an object of N entries; nothing at all |
| `Then the answer is at most N bytes` | each copy of the answer, the data and the text, is that small |
| `Then the answer does not contain "text"` | neither its text nor its structured content |
| `Then the inspected property "p" is "provenance"` | `"assigned"`, `"bound"`, `"animated"` or `"default"` |
| `Then the inspected property "p" is assigned by "devtools"` / `is not assigned by "devtools"` | who assigned a property that was assigned: the agent, through `app_set_property` |
| `Then the inspected property "p" has the value "text"` / `was written at line N` / `is bound at tier T` | the tier is `0`, `1` or `js` |
| `Then the answer has no inspected property "p"` | |
| `Then the agent is refused with "code"` | the error code of 19.4: `"not_found"`, `"ambiguous"`, `"stale_ref"`, `"invalid_argument"`, `"capability_denied"`, … |
| `Then the agent is told the reference is stale` | refused with `"stale_ref"` |
| `Then the refusal mentions "text"` / `Then the refusal names N candidates` | what the refusal says to do next |
| `Then the agent is offered the tools "a, b, c"` | the tool list, in order |
| `Then every tool the agent is offered is marked read-only` | |
| `Then the inspector's instructions match "regex"` | what the server tells an agent when it connects |
| `Then the answer has a PNG picture W by H pixels` / `Then the pixel at X, Y of the picture is "#rrggbb"` | the picture of `app_screenshot` |
| `Then the resource contains "text"` | |
| `Then the client is answered` / `Then the client is refused with status N` | the HTTP client's answer |
| `Then the client's answer contains "text"` / `does not contain "text"` | |
| `Then the token file can be read by its owner alone` / `Then the token file names an address on this machine` | |
| `Then the socket can be used by its owner alone` / `Then the socket is gone` / `Then the token file is gone` | the Unix socket's mode; what the stopped inspector left in the file system |
| `Then the inspector is refused with "text"` / `Then the inspector is served` / `Then the inspector is allowed to listen there` / `Then the warning mentions "text"` | what asking for an address came to |
| `Then the application is left idle` | no request waits, no frame is asked for and nothing woke the application since it last ran |
| `Then the accessibility tree is not being kept` | the tree the inspector read is no longer followed |

### The authoring tools (`tooling/mcp/author-*.feature`, `tooling/fmt.feature`; all `@ljos-only`)

An agent asks `ljos mcp` (ARCHITECTURE.md 19.3) through a real MCP client, or an author runs `ljos`
in a terminal. There is no application: the files the scenario gives (`Given the file "X":`, with
any name, written without the indent of its doc string; one whose name ends in `.sh` may be run) are
a project in a directory of its own. Once the tools are connected, the agent's steps of the
inspector above (`When the agent calls the tool …`, `When the agent reads the resource …`,
`Then the answer's "/pointer" …`, `Then the agent is refused with …`, `Then the refusal mentions …`,
`Then the agent is offered the tools …`, `Then the resource contains …`) speak to them.

| Step | Meaning |
|---|---|
| `Given the authoring tools are connected` | `ljos mcp` is started on the scenario's files with no capability; give the files first |
| `Given the authoring tools are connected with the capabilities "fs, run"` | the same, started with `--caps` |
| `Given the authoring tools are connected with the capabilities "run" and the features runner "runner.sh"` | the same, started with `--features-runner` naming a file of the scenario |
| `Given the agent has called the tool "name" with:` + doc string | a call whose answer a later step builds on |
| `When the agent asks for the prompt "name"` / `When the agent asks for the prompt "name" with:` + doc string | an MCP prompt, with the arguments in the doc string (labelled `json`) |
| `When the author runs "ljos fmt --check ."` / `Given the author has run "…"` | the command line, run in the project's directory; it starts with `ljos` |
| `Then the answer's "/pointer" is:` + doc string | a text of the last answer, whole (a diff, a formatted file) |
| `Then the prompt contains "text"` | part of the prompt's text |
| `Then the prompt names the tools "a, b, c"` | the tools the prompt names in backticks, in the order it first names them; each must be a tool the agent is offered |
| `Then the agent is offered the prompts "a, b, c"` | the prompt list, in order |
| `Then the authoring tools' instructions match "regex"` | what the server tells an agent when it connects |
| `Then the file "X" is:` + doc string | the whole text of a file of the project, as it is now |
| `Then the command prints:` + doc string | everything the command wrote to its output |
| `Then the command exits with status N` | |
| `Then the command's output mentions "text"` / `does not mention "text"` | in what it wrote to its output or as errors |

### The `top` overlay (`tooling/top.feature`; all `@ljos-only`)

A development build's overlay (ARCHITECTURE.md 19.10) of where the last frame's time went. The steps
come after the QML. Showing it runs no frame: it is given the frames the application runs anyway.

| Step | Meaning |
|---|---|
| `When the top overlay is shown` / `When the top overlay is hidden` | as `ljos run --top` shows it; hiding forgets what it recorded |
| `Then the top overlay shows nothing` | it is hidden |
| `Then the top overlay's title is "text"` / `Then the top overlay's title matches "regex"` | its first line: `no frame yet`, or the frame's number and time |
| `Then the top overlay lists the lanes "a, b, c"` | the lanes of its rows, in order |
| `Then the top overlay has charged "owner"` / `Then the top overlay has charged nobody` | an owner charged for scheduled work in any frame since it was shown: `application`, `inspector`, or a boundary by type and name (`Loader \"loader\"`) |
| `Then the top overlay keeps no frame coming` | with it shown, the scene asks for no frame and for no wake |

### Planned (backlog scenarios may use them; not built yet)

`When the agent clicks the reference of the "role" named "name"` · `When the object "o" is destroyed` ·
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

## External suites and the gallery

Two checks sit beside the ledger. Neither adds scenarios to `features/`.

**External acceptance suites** (ARCHITECTURE.md 18.4) run somebody else's Gherkin features, read in
place from their repository, on the Ljós backend. Ljós keeps `acceptance/<suite>/PIN` (which revision),
`features.globs` (which files), `RATCHET` (which scenarios must pass) and `steps/` (an adapter crate
that maps the suite's own step wording onto `Backend`). `mise run features:acceptance <suite>` fails
when a held scenario fails; `--raise` adds what passes now and never lowers. Scenarios tagged
`@backlog-ljos` are skipped and counted. The layout, the file formats and how to add a suite are in
`acceptance/README.md`; `acceptance/example/` is a small working suite.

**The qtdoc gallery** (18.1, 18.6) renders the examples listed in `corpus/examples.json` on Ljós
(R backend, no window) and on `qml6` (offscreen), scores each pair with `qml-compare` (`pixels` and
`content`) and holds `corpus/expected.json`. `mise run gallery:reference` takes the Qt pictures
(local only, never committed); `mise run gallery [--raise]` renders, scores and reports. See
`corpus/README.md`.
