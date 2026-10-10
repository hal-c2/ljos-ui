# 09 — BDD with Gherkin `.feature` files for Ljós UI

Research date 2026-10-10. HAL-C2 was read only; no Qt or Slint source was read. Qt 6.11.2 was run as a black box to check the quoted values. Crate facts are from crates.io, GitHub and docs.rs (raw notes: `data/09-rust-bdd-notes.md`).

## 1. Summary

1. **HAL-C2's BDD is a ledger first and a test suite second.** `features/` has 250 files, 5,481 scenarios (616 outlines, 249 `Rule`s) and about 19,000 step lines. Surface tags (`@mc @desktop @mobile @tui @shared`) say where a scenario must hold; status tags (`@backlog`, `@backlog-<surface>`, `@dropped`, `@blocked`) say whether it holds yet; a `# Sources:` header makes the ledger auditable. Four runners in four languages share nothing.
2. **The desktop step code mostly does not drive the UI.** About 3,700 steps sit in 156 C++ files. 97 files call `Shell.dispatch`/`state`; only 31 synthesise input and 15 load a QML "brick". **No step uses an accessible name**; elements are found by `objectName`. The phone has 143 steps.
3. **Runner: `cucumber` 0.23.0 with `gherkin` 0.16.0.** `World` has no `Send` bound and steps run single-threaded, so an egui `Harness` or rquickjs `Context` (both `!Send`) works as World state. Filter `@backlog-*` in a wrapper `Parser`, because `--tags` replaces cucumber's `filter_run` closure. `rstest-bdd` is rejected (13 stars, no tag filtering, no `Rule`).
4. **Drive the UI through `egui_kittest` 0.36.2.** Find elements by AccessKit role and name; time is virtual (`step_dt`). A scenario that cannot find "button Save" fails, which enforces accessibility.
5. **Put a thin `Backend` trait under the steps.** One implementation is Ljós; the other is **real Qt**, via a generated driver QML run by `qml6` offscreen. I checked this works: about 0.1 s per value probe, 0.24 s with a screenshot. A `@qt-parity` scenario must pass on both, so Ljós does not test only its own assumptions.
6. **HAL-C2's ledger is Ljós's acceptance suite, but only the QML-facing part is portable.** State/dispatch steps belong to the native layer. Select with a runner-owned allowlist and ratchet over the existing `@desktop`/`@mobile`/`@shared` tags; do not add a tag to 2,000 lines in someone else's repo.

---

## 2. Detailed findings

### 2.1 How HAL-C2 does BDD today

**Conventions** (`features/README.md`):
- One directory per product domain (`threads composer timeline navigation …  parity`); surfaces are tags, not directories.
- Writing rules: declarative, in the user's words, never naming widgets; one behaviour per scenario; `Scenario Outline` over copies; reverse, failure and offline paths are scenarios; every file opens with `# Sources:`.
- Ledger rule: every user page, setting, RPC method, command, event and keybinding must be named in some `# Sources:`. `parity/*.feature` are generated outlines (`rpc.feature`: 169 methods in one Examples table).
- Tag counts (tag lines): `@mc` 2,049, `@desktop` 1,775, `@tui` 835, `@mobile` 791, `@shared` 191, `@backlog` 1,248, `@backlog-mobile` 475, `@dropped` 108, `@blocked` 11.
- An absent status tag means "passes on every one of its surfaces now". Each runner treats `@backlog-<its surface>` like `@backlog`. Backlog scenarios are skipped by default and run only with `--backlog`, where they are expected to fail.

**Four independent runners**, one per surface:

| Surface | Runner | Step style | Scale |
|---|---|---|---|
| MC | Elixir `Cucumber` lib, `mix features` | `step "a connected environment {string}", ctx` | about 5,900 step lines, 143 `.exs` |
| TUI | `bun test`, `@cucumber/gherkin` + home-grown registry | `step("…{string}…", async ctx => …)` | 2,174 steps, 73 files |
| Qt desktop | `tst_Features.cpp`, a **hand-written C++ Gherkin parser** (242 lines) | `step(regex, (World&, Captures, Table))` | 3,706 steps, 156 files |
| QML phone | same `Runner.h`, own `World` | same | 143 steps |

Details worth copying:
- The desktop runner makes one QTest row per scenario. `HAL_C2_FEATURES="glob[:Scenario name*] …"` narrows a run and `HAL_C2_BACKLOG=1` inverts it to backlog-only.
- **What a run delivers is a hand-curated allowlist** (`kDefaultGlobs`, about 200 entries such as `navigation/windows.feature:Zooming the app*`), not tags alone. Tags give surface and status; the allowlist says what this implementation has taken on.
- The C++ parser expands outlines, merges Feature/Rule/Examples tags, and **ignores doc strings**. `Step.outcome` records whether a step is under a `Then`, so one phrase can both set and check.
- **Determinism comes from the World, not sleeps.** `World` builds the whole shell in-process (about 60 controllers). `setTime()` injects a clock, and `sync()` is a `test.barrier` RPC round trip. `FakeMc` is a real `QWebSocketServer` (632 lines) with per-domain `Extension`s and `hold()/answerHeld()` to defer answers, so races are set up on purpose.
- `tst_Scenarios.qml` (354 `test_*` functions in 40 files) hand-mirrors `qt-scenarios.feature` through a `Shell` test double. The two drifted apart: a warning against two copies.

**How the steps touch the UI.** The step vocabulary is very wide and rarely reused: 13,661 distinct step texts after masking quoted strings, `<params>` and digits (1,005 in `mobile/`). The most repeated step appears about 90 times. The desktop step files break down as follows:

| Interaction | Evidence |
|---|---|
| Contract level: `Shell.state[key]` read, `dispatch(action,payload)` | 97 of 156 files, 527 `dispatch(` calls |
| Real input events (`QTest::mouseClick/keyClick`) | 31 files, 65 calls |
| Item lookup by `objectName` / `findChild` | 51 files, 111 and 19 references |
| A QML "brick" loaded offscreen (`Brick.h`) | 15 files |
| `Accessible` / `QAccessible` | **0** (while 80 QML files set `Accessible.name`) |

The phone's `World::tap(objectName)` sends `QTest::touchEvent` press and release at the item's centre (40 calls) against the app's real root.

**mise tasks.** `features` runs the four suites and prints `ok` / `FAILED` per task, continuing past failures. `features:mc <globs>` requires globs (with `:all` per directory, since one run is slow); there are also `features:tui`, `features:desktop`, `features:mobile [--backlog]`, and `features:pick [count] [@tag] [-@tag] [globs]`. `pick` is an awk script that samples backlog scenarios weighted 4:2:1 by priority, honours `@blocked`, and treats `@shared` as every client. It hands an agent the next unit of work.

### 2.2 Rust Gherkin tooling (Oct 2026)

- **`cucumber` 0.23.0** (2026-04-23; MSRV 1.88; MIT/Apache; 746 stars, last push 2026-07-27, 2–3 releases a year). Features: `macros` (default), `output-junit`, `output-json`, `libtest`, `timestamps`, `tracing`. tokio is optional; `futures` is enough.
  - **World**: `trait World: Sized + 'static`, with `#[derive(World)]` and `#[world(init = fn)]`. A fresh World is made per scenario and per retry. There is no `Send` bound. `runner::Basic` is `!Send`, scenarios are interleaved cooperatively on one task, and only a retry timer spawns a thread. Default concurrency is 64 at `.await` points. With sync steps nothing interleaves, but set `max_concurrent_scenarios(1)` while any state is process-global.
  - **Steps**: `#[given|when|then]` with a literal, `regex = …` or `expr = …` (Cucumber Expressions, compile-time validated); custom `#[derive(Parameter)]` types; `step: &Step` exposes `docstring` and `table`. Steps are plain `fn` pointers, so state lives in the World. Ambiguity is a run-time failure; undefined steps are skipped unless `.fail_on_skipped()`.
  - **Gherkin coverage**: `Rule`, `Background`, outlines (placeholders substituted in steps, docstrings and tables; Examples tags merged), docstrings with a content-type label, tables, `# language:`.
  - **Tags and filtering**: `--tags` takes upstream tag expressions, and 0.23 fixed their precedence. `@serial`, `@retry(3).after(1s)` and `@allow.skipped` are built in; `--name` takes a regex and `-i` a glob. **Gotcha:** in `filter_run`, `--name` or `--tags` *replaces* the closure, and the two conflict with each other. A default exclusion for `@backlog-*` therefore cannot live in the closure.
  - **Writers**: basic and summarize; `JUnit`, `Json` and `Libtest` behind features; `Tee` for several at once; `repeat_failed()`. CLI is extensible via `cli::Compose`.
- **`gherkin` 0.16.0** (peg-based; the AST has Feature, Rule, Background, Scenario, Examples, Step with `docstring` and `table`). It also exports `TagOperation`, which parses `@a and not @b`. A fully custom runner on top of it would be a few hundred lines but would lose retries, writers and concurrency.
- **Alternatives**: `rstest-bdd` 0.6.0 (13 stars, one maintainer, no documented tag filtering or `Rule`); `gherkin-cargo-test` 0.11.0 (about 1k downloads); `cucumber_rust` is dead (2021).

### 2.3 Driving egui from steps

- **egui and egui_kittest 0.36.2** (2026-09-08; Rust 1.95; `kittest` 0.4.0, 0.5.0 released 2026-10-05). There is no `Harness::new`; use `new_ui`, `new_ui_state(|ui, &mut S|, S)`, `new_eframe(|cc| App)` (feature `eframe`) or `builder().with_size()…with_step_dt()…build_*`. A `Harness` is `!Send`.
- **Queries**: `get_by_label`, `get_by_role(accesskit::Role)`, `get_by_role_and_label`, `get_by_value`, `get_by(By::new().predicate(…))`, plus `get_all_*` and `query_*`. A `get_*` matching zero or several nodes panics. `node.accesskit_node()` exposes `author_id`, which can carry QML's `objectName`.
- **Actions**: `node.click()` (`click_accesskit()` works off-screen), `hover()`, `focus()`, `type_text()`, `scroll_*`; `key_press(Key)` on `Harness`. Events are queued, so call `run()` or `step()` afterwards.
- **Time is virtual.** Each frame advances `predicted_dt = step_dt` (0.25 s) and `animation_time` is forced to 0. `run()` loops until no *immediate* repaint is requested and panics after `max_steps` (default 4). **`request_repaint_after(d)` stops `run()`**, so timers need `run_steps(n)`. Ljós therefore needs its own injectable `Clock` (06 §2.4).
- **Snapshots**: `snapshot("name")` needs features `snapshot` and `wgpu` (software adapter preferred); defaults are threshold 0.6, `max_failed_pixels = 0`, `tests/snapshots/`, overridable in `kittest.toml`. `UPDATE_SNAPSHOTS=1` rewrites failures, `=force` all. egui keeps its PNGs in git LFS and prefers non-image tests.
- **AccessKit**: the tree exists once `ctx.enable_accesskit()` is called (the harness does). `ui.label` becomes `Role::Label` with text in `value`; `ctx.accesskit_node_builder(id, |n| …)` customises nodes. **Custom-painted widgets are invisible to kittest unless they register a node**, so every Ljós item type must.
- **Windows**: the harness drives only the root viewport; a second OS window needs its own harness.
- **Android**: `accesskit` with `android-native-activity` is a `compile_error`, so the accessible build must use `android-game-activity`. kittest has no on-device story (cross-compiling and `adb shell` is UNVERIFIED).
- **rquickjs 0.14.0**: `Runtime` and `Context` are `!Send` without the experimental `parallel` feature; leave it off and keep them in the World.

### 2.4 Running real Qt as an oracle (tried here)

On Qt 6.11.2, `QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen qml6 probe.qml` prints `console.warn` to stderr. Without the variable, output is swallowed under journald, so the driver must set it. Measured:
- **Binding semantics**: `a=1, b=a+1`. Setting `a=5` gives `b=6`. A `Binding{when:}` active then inactive restores the original binding (`b=7` after `a=6`). Assigning `b=100` breaks the binding (`a=8` leaves `b=100`). `b=Qt.binding(…)` restores it (`a=9` gives `b=10`).
- **`ListView` with `cacheBuffer:0`, 10,000 rows, 300 px viewport, 30 px delegates**: 11 live delegates, and 12 after `positionViewAtIndex(5000)` (the destroy is deferred). So "at most 13", not an exact count.
- **`Accessible.name` on `Button{text:"Save"}` reads `""`** and `Accessible.role` reads 0 through QML. Defaults exist only in the platform `QAccessible` interface, so observing them needs a small C++ helper (`QAccessible::queryAccessibleInterface`, public API). QML reads verify only explicit overrides (`Accessible.name:"Save draft"` reads back).
- **Timing**: about 0.1 s per probe process, 0.24 s with `grabToImage` (`QT_QUICK_BACKEND=software`). Qt has no public fake clock, so only end states can be compared.

---

## 3. Concrete recommendations

### 3.1 Runner and layering

```
crates/ljos_bdd/           # library: World, Backend trait, step defs (QML-author vocabulary)
  src/backend/ljos.rs      # kittest Harness + ljos_runtime
  src/backend/qt.rs        # generates driver.qml, spawns qml6, parses JSON from stderr
tests/features.rs          # [[test]] harness=false; cucumber + gherkin
acceptance/hal-c2/         # section 3.4
features/                  # Ljós toolkit ledger (3.2)
```

- **`cucumber` 0.23 + `gherkin` 0.16.** `fn main(){ futures::executor::block_on(LjosWorld::cucumber().max_concurrent_scenarios(1).fail_on_skipped().with_parser(BacklogFilter::new(basic)).with_cli(Compose<Opts, Extra>).run_and_exit("features")) }`.
  - `BacklogFilter` wraps `parser::Basic`. It drops scenarios tagged `@dropped`, `@blocked`, `@backlog` and `@backlog-<this-surface>`. With `--backlog` it keeps *only* those, matching HAL-C2's inversion. Because it runs before `--tags`/`--name`, those flags still compose.
  - Report with `output-junit` through `Tee` to `target/features/junit.xml`. Fallback if `Parser` wrapping fights the generics: a runner over `gherkin` plus `TagOperation`, about 400 lines.
- **Steps address things the way a QML author does, not the way egui does:**
  ```gherkin
  Given the QML:                       # docstring becomes Main.qml (also: Given the file "Card.qml":)
  When the property "a" of "root" is set to 5
  Then the property "b" of "root" is 6
  When 1 second passes                 # n × step_dt; never "N frames"
  When the user clicks the button "Save"      # role + accessible name
  Then the accessibility tree has a button named "Save"
  ```
  - **Objects are addressed by `objectName`, interactive elements by role and name.** Map `objectName` to AccessKit `author_id`, and Qt `Accessible.Role` to `accesskit::Role` in one tested table.
  - Observe **inside QML** where possible (a `created` counter bumped in `Component.onCompleted`, `console.warn` as the log), so the same step works on the Qt backend and the step set stays small (target about 80).
- **A single `Backend` trait** is the seam: `load(files)`, `set(path, prop, value)`, `get(path, prop)`, `advance(ms)`, `click(role, name)`, `key(chord)`, `a11y_tree()`, `log()`, `grab()`. Scenario steps never see egui or kittest types.
- **Other test kinds.** Gherkin says what is promised; `cargo test` covers parser, IR and bridge internals; `proptest` covers invariants (a random `set` sequence leaves every binding equal to naive re-evaluation; print-then-parse round trips; layouts stay within constraints); kittest PNGs cover pixels, sparingly. A scenario may carry a `# Property: <path>` comment pointing at its proptest.

### 3.2 Ljós's own `features/` tree

One directory per language or toolkit area, not per surface:
`language/ bindings/ components/ items/ layouts/ views/ controls/` (one file per control, Basic style first) `input/ focus/ text/ animation/ a11y/ i18n/ rendering/ offscreen/ hot-reload/ runtime/ network/ mobile/ tooling/ parity/` (generated outlines, 3.5).

**Tags** (applied to Feature, Rule, Scenario or Examples, as in HAL-C2):
- Surface: `@headless` (default, kittest software rendering), `@desktop`, `@android`, `@ios`, `@wgpu` (needs a snapshot renderer). No surface tag means `@headless`.
- Status, borrowing HAL-C2's semantics: none = passes everywhere it applies; `@backlog`; `@backlog-android` and so on; `@dropped` (with a comment stating the reason, e.g. Qt3D or WebEngine); `@blocked` (with the comment naming what it waits for); `@priority-high|low`.
- Oracle: `@qt-parity` means the expected values hold on real Qt and the scenario runs on both backends. `@qt-diverges` is a deliberate difference (with a comment), Ljós only. `@ljos-only` marks extensions such as hot reload. `@qt-pixels` compares against a PNG reference. Anything with none of these is "our assumption"; `features:lint` reports the verified share.
- Ledger key: `@api-QtQuick.Rectangle.radius` (see 3.5). `@a11y` is cross-cutting.

**QML in scenarios.** Doc strings with a language label (` """qml `); `Given the file "Card.qml":` for extra files; `Scenario Outline` placeholders inside doc strings for variants; data tables for property sets and expected a11y trees. Keep snippets to 25 lines and one concept. Every file starts with `# Sources:` naming Qt *documentation* URLs and corpus examples, never source code, and a `@qt-parity` file names the Qt version it was verified on.

**Writing rules adapted for a toolkit:** the user is a QML author, so language terms (binding, property, delegate, anchor, signal) are fine; never name egui, kittest, frames, widgets or crates; time is "1 second passes"; observe through properties, the log, the accessibility tree and object counts; failure paths (binding loop, bad type, handler exception) and reverse paths (unbind, unload) are scenarios.

**Four example files:**

`features/bindings/reevaluation.feature`
```gherkin
# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Property bindings
  A bound property follows the properties it reads until something assigns to it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item { id: root; objectName: "root"
        property int a: 1
        property int b: a + 1
        property bool boosted: false
        Binding { target: root; property: "b"; value: root.a * 10; when: root.boosted }
      }
      """

  Scenario: A binding follows its dependencies
    When the property "a" of "root" is set to 5
    Then the property "b" of "root" is 6

  Scenario: Assigning a value breaks the binding
    When the property "b" of "root" is set to 100
    And the property "a" of "root" is set to 8
    Then the property "b" of "root" is 100

  Scenario: Qt.binding puts a binding back
    When the property "b" of "root" is bound to "a + 1"
    And the property "a" of "root" is set to 9
    Then the property "b" of "root" is 10

  Scenario Outline: A Binding element overrides while its condition holds
    When the property "boosted" of "root" is set to true
    And the property "a" of "root" is set to <a>
    Then the property "b" of "root" is <boosted b>
    When the property "boosted" of "root" is set to false
    Then the property "b" of "root" is <restored b>

    Examples:
      | a | boosted b | restored b |
      | 5 | 50        | 6          |
      | 6 | 60        | 7          |

  Scenario: Two bindings that read each other are reported, not looped forever
    Given the file "Loop.qml":
      """qml
      import QtQuick
      Item { property int x: y + 1; property int y: x + 1 }
      """
    When "Loop.qml" is loaded
    Then the log has a warning matching "Binding loop detected"
```

`features/offscreen/list-delegates.feature`
```gherkin
# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer, reuseItems)
#   docs/research/07-hal-c2-requirements.md (Timeline: variable-height ListView)
# Delegate counts are bands, never exact: Qt itself gives 11 then 12 for this case.

@qt-parity
Feature: Delegates exist only near the viewport
  A long list costs what is visible, not what it holds.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item { id: root; objectName: "root"; width: 200; height: 300
        property int created: 0
        property int destroyed: 0
        readonly property int live: created - destroyed
        ListView { objectName: "list"; anchors.fill: parent; cacheBuffer: 0; model: 10000
          delegate: Rectangle { width: 200; height: 30
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++ } }
      }
      """

  Scenario: Only the visible rows get delegates
    Then the property "live" of "root" is between 10 and 13
    And the property "count" of "list" is 10000

  Scenario: Jumping far away does not keep the old rows alive
    When the method "positionViewAtIndex" of "list" is called with 5000 and "ListView.Beginning"
    Then the property "live" of "root" is between 10 and 13
```

`features/hot-reload/preserving-state.feature`
```gherkin
# Sources:
#   docs/research/07-hal-c2-requirements.md §2.2 (shell.qml hot reload, fallback, error overlay)

@ljos-only
Feature: Editing a running application
  Saving a file updates the running window. What the user has typed or scrolled stays.

  Background:
    Given the application "Main.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column { objectName: "root"
        property int saves: 0
        Label { text: "Hello" }
        TextField { objectName: "name"; Accessible.name: "Name" }
        Component.onCompleted: saves++
      }
      """
    And the user types "Ada" into the text field "Name"

  Scenario: An edited binding takes effect and unchanged items keep their state
    When "Main.qml" is saved with "Hello" replaced by "Hi"
    Then the accessibility tree has a label "Hi"
    And the text field "Name" contains "Ada"
    And the property "saves" of "root" is 1

  Scenario: A syntax error keeps the last good version running
    When "Main.qml" is saved with "Column {" replaced by "Column {{"
    Then the accessibility tree has a label "Hello"
    And the diagnostics list an error at "Main.qml" line 4

  @backlog
  Scenario: Fixing the error clears the diagnostic
    Given "Main.qml" was saved with "Column {" replaced by "Column {{"
    When "Main.qml" is saved with "Column {{" replaced by "Column {"
    Then the diagnostics are empty
    And the text field "Name" contains "Ada"
```
(Reload granularity is a design question; see section 4.)

`features/a11y/names-and-roles.feature`
```gherkin
# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquickcontrols-index.html
# Explicit overrides read back through QML on Qt; defaults need the C++ QAccessible helper.

@a11y
Feature: Controls are reachable by what they are and what they are called
  A screen reader user finds the control by its role and name, and so do these scenarios.

  @qt-parity
  Scenario: A button is named by its text
    Given the QML:
      """qml
      import QtQuick.Controls
      Button { text: "Save" }
      """
    Then the accessibility tree has a button named "Save"

  @qt-parity
  Scenario: Accessible.name overrides the text
    Given the QML:
      """qml
      import QtQuick.Controls
      Button { text: "Save"; Accessible.name: "Save draft" }
      """
    Then the accessibility tree has a button named "Save draft"
    And the accessibility tree has no button named "Save"

  @qt-parity
  Scenario Outline: Each control exposes its role
    Given the QML:
      """qml
      import QtQuick.Controls
      <type> { Accessible.name: "Thing" }
      """
    Then the accessibility tree has a <role> named "Thing"

    Examples:
      | type     | role      |
      | Button   | button    |
      | CheckBox | check box |
      | Slider   | slider    |

  Scenario: The user acts on a control through its name
    Given the QML:
      """qml
      import QtQuick.Controls
      Button { text: "Save"; property int n: 0; objectName: "b"; onClicked: n++ }
      """
    When the user clicks the button "Save"
    Then the property "n" of "b" is 1
```

### 3.3 Workflow: mise, CI and coexistence

Follow 06 §3.6 (bash tasks in `mise-tasks/` with `#MISE` headers and `set -euo pipefail`):

| Task | Does |
|---|---|
| `features [--backlog]` | Run every runner, continue past failures, print `ok/FAILED` per task. |
| `features:run <globs…> [-t '@expr'] [--backlog]` | One directory, file, or `file:Name*` (HAL-C2's convention); wraps `cargo test --test features`. `:all` shards per top-level directory. |
| `features:pick [n] [@tag] [-@tag] [globs]` | Port the awk script unchanged. |
| `features:lint` | Dry run: undefined/ambiguous steps, duplicate names, missing `# Sources:`, `@qt-parity` without a Qt version comment; prints distinct steps per 100 scenarios. |
| `features:ledger` | Check `@api-…` tags against `ledger/qtquick.toml` (3.5). |
| `features:parity [--capture]` | Run `@qt-parity` on the Qt backend (Qt ≥ 6.11); `--capture` regenerates `@qt-pixels` references. |
| `features:android` | Cross-build and run on an emulator via adb (UNVERIFIED). |

Gherkin runs alongside `cargo test`, `proptest` and snapshots, not instead of them.

**CI**: (1) `features` on headless Linux, sharded by directory, JUnit published; wgpu's software adapter needs no GPU. (2) `features:parity` nightly and on any change to a `@qt-parity` file, Qt version pinned. (3) `features:android` on an emulator. Gate pull requests on a committed `features/RATCHET` listing passing scenario ids, so no previously passing scenario regresses.

### 3.4 HAL-C2's ledger as Ljós's acceptance suite

**Principle:** one feature tree, a new implementation. Ljós replaces Qt on the desktop and phone surfaces, so scenarios keep `@desktop`/`@mobile`/`@shared` and the Ljós runner is another runner for them.

- **Selection.** Reuse HAL-C2's own mechanism: a runner-owned allowlist of globs and `file:Name*` filters (the equivalent of `kDefaultGlobs`), in `acceptance/hal-c2/features.globs`, plus the committed ratchet. The README's rule "each runner treats its own surface's tag like `@backlog`" would otherwise need `@backlog-ljos` on thousands of lines before the first scenario can run. Reserve a `@backlog-ljos` tag for scenarios known to be unportable for a stated reason (the Ghostty terminal plugin, H.264 `DeviceScreen`). If Ljós later *becomes* the desktop, the allowlist becomes "everything" and `@backlog-desktop` takes over.
- **Where.** `acceptance/hal-c2/` in ljos-ui, reading `HAL_C2_FEATURES_DIR` (default `../hal-c2/features`) with a pinned commit in `PIN`. Never copy the `.feature` files. Move the crate into HAL-C2 as `apps/desktop-ljos` once Ljós is its client.
- **Reuse of the fake MC.** `FakeMc` is C++ and in-process. There are three options:
  - (a) Port `FakeMc` and its `Extension`s to Rust on `tokio-tungstenite` (about 630 lines core plus per-domain fakes). It keeps determinism (`hold`/`answerHeld`, `sync`).
  - (b) Run the real Elixir MC: faithful but slow, with no hold/answer control.
  - (c) Run the C++ `FakeMc` as a subprocess: cheap, but adds Qt to Ljós CI.
  - **Recommendation:** (a), in stages, with (b) for a few smoke scenarios.
- **Porting the step definitions.** Do not translate 3,706 C++ steps one for one. Split by what they touch:
  - **State/dispatch steps (about 97 of 156 files)** test HAL-C2's native controllers, not the UI. They move with the controllers when those are rewritten in Rust and can run headless against the Rust fake MC. They are acceptance for the native layer (07 §2.1), not for Ljós.
  - **UI steps (about 51 files using `objectName`, 15 bricks, 31 input)** are the Ljós-relevant part. Port the helpers first (`find(objectName)` becomes a path query, `QTest::mouseClick` becomes `Node::click()`, `keyClick` becomes `key_press`), then reimplement domain by domain, moving from `objectName` to role plus name as QML gains `Accessible.name` (HAL-C2 already has 297 uses).
  - **Phone**: 143 steps, a good first milestone; they use touch, so add a backend `touch` emitting `egui::Event::Touch`.
- **Staging**: S1 the bricks (with a Rust `Shell` test double); S2 the phone's feature files; S3 desktop `native-*.feature`; S4 remaining `@shared` timeline scenarios. Each stage ends with a ratchet increase.
- Copy `HAL_C2_SHOTS` (a PNG after every step) as `LJOS_SHOTS=dir`; it makes failures readable.

### 3.5 Qt parity and the API ledger

**Dual-backend execution** is feasible and cheap. The `Qt` backend turns the scenario's steps into a JSON program and runs it in a generated driver:
- `driver.qml` loads the Given files through a `Loader` or `Component.createObject`, interprets ops `set / bind / call / wait / read / grab` (a plain switch; no `eval`), and prints one JSON array to stderr with `QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software`, `TZ=UTC`, `LANG=en_US.UTF-8` (the environment the gallery's `reference.mjs` already uses).
- Rust parses the observations and asserts the `Then`s. Each scenario takes about 0.1–0.3 s. The Qt backend needs no GPU.
- Estimated cost: about 700 lines of Rust and 150 of QML, plus Qt in the nightly job.
- It supports value, tree and object-count checks. It does **not** support animation midpoints (use end states), `Accessible` defaults (needs a roughly 100-line C++ `QAccessible` helper) or text pixels.

**Pixels** follow the qml-solid gallery: `@qt-pixels` scenarios compare `grab()` with `features/**/ref/*.png` captured by `features:parity --capture`, using the `qml_compare` crate planned in 06 §3.5 (pixelmatch port, `content` score, ratchet) rather than kittest's strict `max_failed_pixels = 0`. kittest PNGs stay for Ljós-internal regressions.

**API ledger.** Generate `ledger/qtquick.toml` from the `qmltypes` tool (06 §3.1) with one row per type, property, signal and method. Status values are `supported | partial | dropped | unsupported`, and `dropped` needs a reason. Sort rows by use in HAL-C2 and the corpus (08). Every `supported` or `partial` row must be named by at least one `@api-<Module>.<Type>.<member>` tag, or `features:ledger` fails (HAL-C2's ledger rule applied to API rows). Generate `features/parity/qtquick-types.feature` as outlines ("`<type>` exposes `<property>` with default `<default>`") whose defaults come from the Qt backend, not a hand-typed table.

---

## 4. Risks and open questions

1. **Cucumber filtering is easy to get subtly wrong.** A naive `filter_run` silently runs `@backlog` scenarios once `--tags` is passed. Test the wrapper `Parser` itself.
2. **cucumber-rs moves slowly** (2–3 releases a year). The exit cost is only the step macros, since `World` and `Parser` are small traits.
3. **kittest is pre-1.0 and churns with egui.** Hide it behind `Backend` and pin versions. Treat "each item type registers an a11y node" as the first conformance test.
4. **Android accessibility constrains the entry point** (`android-game-activity` only). Verify before promising `@android @a11y`.
5. **Virtual time is egui's, not Qt's.** `run()` stops on `request_repaint_after`, so every QML `Timer` and animation must go through one injectable Clock, or `When 1 second passes` is a lie.
6. **Qt parity is bounded.** No platform a11y defaults through QML, no fake clock, no text pixels. A scenario that cannot run on Qt must say why (`@qt-diverges` or `@ljos-only`).
7. **Step-vocabulary sprawl.** HAL-C2 has about 13.7k distinct step texts for 19k steps. `features:lint` should report the ratio, and agent authors should get the step catalogue in their prompt.
8. **Allowlist drift.** Bump the HAL-C2 pin deliberately and let the ratchet show regressions.
9. **Two sources of truth** (Gherkin and `tst_*.qml`) already diverged in HAL-C2. New behaviour goes in `.feature` files only.
10. **Open questions.** Should `@qt-parity` be the default for `language/` and `bindings/`? What reload granularity is promised (file, component, binding)? Can CI runners install Qt for the nightly job?

---

## 5. Sources

HAL-C2 (read only): `features/README.md`; `features/threads/snooze.feature`, `mobile/accessibility.feature`, `desktop/native-sidebar.feature`, `navigation/qt-scenarios.feature`, `parity/rpc.feature`, `timeline/streaming.feature`, `tui/threads.feature`; `apps/desktop-qt/tests/native/{tst_Features.cpp, features/Runner.{h,cpp}, Harness.h, World.{h,cpp}, FakeMc.{h,cpp}, Brick.h}`; `apps/desktop-qt/tests/tst_Scenarios.qml`; `apps/mobile-qt/tests/{tst_Features.cpp, PairableMc.h, features/}`; `apps/tui/features/{runner.ts, steps.ts}`; `apps/server-ex/test/steps/common_steps.exs`, `mix.exs`; `mise-tasks/features/*`; `docs/internals/desktop-qt.md`.

Ljós research: `06-qml-solid-reuse.md` §2.4, §3.5, §3.6; `07-hal-c2-requirements.md`; `08-qml-usage-inventory.md`; `data/09-rust-bdd-notes.md`. qml-solid: `examples/gallery/tests/compare.js`, `report.spec.js`, `selftest/`.

External: crates.io and docs.rs for `cucumber` 0.23.0, `gherkin` 0.16.0, `rstest-bdd` 0.6.0, `egui_kittest` 0.36.2, `kittest` 0.4.0/0.5.0, `rquickjs` 0.14.0; https://github.com/cucumber-rs/cucumber and https://cucumber-rs.github.io/cucumber/current/; https://github.com/emilk/egui/tree/main/crates/egui_kittest; https://github.com/rerun-io/kittest; Qt documentation pages named in the example `# Sources:` headers.

Experiments run locally (Qt 6.11.2, `/tmp/ljos-parity/*.qml`): section 2.4.
