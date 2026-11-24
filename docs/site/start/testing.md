---
title: Testing
description: Test applications with TypeScript files that drive them as a user or an agent does, on a virtual clock, with ljos test; render deterministic pictures with ljos render; write Gherkin behaviour scenarios.
section: start
order: 7
---

# Testing

This page shows you how to test an application with Ljos.Test: TypeScript files that drive it as a user or an agent would, on a clock only the test moves. It also covers deterministic pictures with `ljos render`, Gherkin behaviour scenarios, and running existing Qt Quick Test (`tst_*.qml`) suites.

## Tests in TypeScript with `ljos test`

A test file is a TypeScript module whose name ends in `.test.ts`. It imports `test` and `expect` from `ljos:test`, names the QML file it tests, and acts on it through locators: the same way of naming an item that the MCP inspector, an agent and `ljos observe` use. Every test gets a fresh application, headless, on a virtual clock.

Given this `Main.qml`:

```qml group=counter
import QtQuick
import QtQuick.Controls

Item {
    width: 200
    height: 120
    property int count: 0
    Button {
        text: "Add"
        onClicked: count++
    }
    Text {
        objectName: "count"
        y: 80
        text: count
    }
}
```

write `counter.test.ts` beside it:

```ts file=counter.test.ts group=counter
import { test, expect } from "ljos:test";

test.use({ app: "./Main.qml" });

test("a click counts", async (app) => {
  await app.click('role=button[name="Add"]');
  await expect(app.find("#count")).toHaveText("1");
});
```

and run `ljos test` in the directory (or give it files or directories):

```sh
ljos test
```

```text
TAP version 14
ok 1 - counter.test.ts > a click counts
1..1
# pass 1
# fail 0
# skip 0
```

The report is [TAP version 14](https://testanything.org/tap-version-14-specification.html). A failure says what and where:

```text
not ok 1 - counter.test.ts > a click counts
  ---
  message: 'expected #count to have text "1"; it was "0" after 5000 ms'
  at: counter.test.ts:7
  ...
```

### Finding and acting

- **Locators** are text: `#count` (an `objectName`), `role=button[name="Add"]` (a role and an accessible name), `text=Hello`, and `a >> b` for `b` inside `a`. `app.find(locator)` gives a `Locator`, which finds the item again on every use; `nth(i)` and `find(inner)` narrow it.
- **Acts:** `click`, `type`, `press` (keys such as `"Ctrl+A"`) and `scroll`, on the app with a locator or on a locator. They go through the same hit testing and focus as a person's input, and an act the person could not do is refused: a disabled button is not clicked, and the test fails saying so.
- **Reads:** `text()`, `property(name)`, `count()`, `exists()` and `inspect()` on a locator; `snapshot()` (the outline an agent reads) and `logs()` on the app. `app.call(tool, args)` reaches any other inspector tool.

### Time

Nothing in a test sleeps. The application's clock moves only when the test lets it:

- `await app.advance(1000)` lets a second pass: timers fire and animations run.
- `expect(locator)` matchers retry: they check once, then let the clock run a frame (16 ms) at a time until they hold, up to their timeout (5000 ms of the application's time, `{ timeout }` to change it). `toHaveText`, `toHaveProperty`, `toHaveCount`, `toExist`, `toBeVisible`, `toBeEnabled`, `toBeChecked` and `toBeFocused` work this way. An item that is not there has no text and is not shown or enabled, so `.not` holds for it; a locator that finds more than one item where one is needed fails, with or without `.not`.
- `app.waitFor(fn)` and `expect.poll(fn).toBe(x)` retry a condition of your own the same way, and retry when `fn` throws. `expect.poll` lets the clock of the application the test started last run.

A test therefore gives the same result on every machine and runs as fast as its own work. Value matchers (`expect(x).toBe(y)`, `toEqual`, `toMatch`, `toContain`, `toBeGreaterThan`, `toBeCloseTo`, `toThrow` and the rest, each with `.not`) check once.

### Pictures

`await expect(app).toMatchPicture("counter.png")`, or `expect(locator)` for one item, compares what the window shows with `__pictures__/counter.png` beside the test. The first run writes the picture and fails, so that you look at it; later runs are held to it. A picture that differs fails and is written beside it as `counter.actual.png`. Run `ljos test --update-pictures` to keep the new pictures. `{ tolerance, maxDiffPixels }` allow for small differences.

### Organising tests

- `describe(name, fn)` groups tests; their names read `file > group > test`.
- `test.skip` reports a test without running it; `test.only` runs only the tests marked so in a file.
- `test.use({ app, width, height, timeout })` sets what a file's tests start; `launch(path)` starts a second application inside a test. The path is relative to the test file, and the application must be inside the directory `ljos test` was given, as the files it imports are.
- `describe`'s function adds its tests at once, so it is not `async`.
- A test file may import its own `.ts` helpers beside it (`import { addTimes } from "./steps.ts"`). `ljos:test` is the only other module it may import.
- For `tsc` and your editor, the types are in `ljos-test.d.ts` (`crates/ljos_test/src/script/ljos-test.d.ts` in the Ljós repository); put it beside your tests.

### Options and CI

```sh
ljos test --filter "a click" --junit results.xml --json results.json tests/
```

- `--filter TEXT` runs the tests whose full name contains the text.
- `--junit FILE` writes JUnit XML for CI; `--json FILE` the machine-readable report.
- The exit status is the number of failed tests (`0` when all pass).

## Existing Qt Quick Test suites

Files named `tst_*.qml` that use `import QtTest` and `TestCase` run unchanged under the same `ljos test`, on the same virtual clock, and print what Qt's `qmltestrunner` prints (`PASS   : Case::test_x()`, `Totals: …`). This keeps a migrating application's suites running; new tests are better written in TypeScript, because a `TestCase` runs inside the application and can reach what no user could. `compare`, `verify`, `tryCompare`, `SignalSpy`, `mouseClick`, `keyClick`, `wait` and the rest are supported; `--filter Case::function` and `--import DIR` work as with Qt's runner.

## Deterministic pictures with `ljos render`

When you want to verify how a component looks or capture a reference image in CI without a display server, use `ljos render`:

```sh
ljos render . --entry Main.qml --size 300x200 --settle 500 --out shot.png
```

`ljos render` is deterministic by default:

- **Virtual clock:** `--settle <MS>` (default `1500`) advances the virtual clock so timers and entry animations reach their target state before the frame is drawn.
- **Pinned wall clock:** `--now <RFC3339>` (for example, `--now 2026-03-30T14:00:00Z`) pins the timestamp seen by JavaScript `Date` at start-up and advances it by `--settle`.
- **Bundled font:** Unless you pass `--system-fonts`, Ljós renders text using only its bundled font so glyphs match across Linux, macOS and CI runners.
- **Offline by default:** Network fetches fail immediately unless you pass `--network`.

## Gherkin behaviour scenarios

In addition to `tst_*.qml`, Ljós uses Gherkin `.feature` files (run with `cucumber` through `crates/ljos_bdd`) to specify user-visible application behaviour.

### How scenarios address the UI

Behaviour steps never refer to internal rendering types. Instead, they interact with your QML scene the same way a person using a screen reader or a coding agent does:

- **Interactive controls** are addressed by **role and accessible name** (such as `When the user clicks the "button" "Save"`). Clicking a control moves the pointer to the centre of the control and dispatches the click through the input router; disabled controls and items hidden behind disabled parents are not clicked.
- **Objects and properties** are read or written by `objectName` path (such as `Then the property "saved" of "root" is 1`).
- **Time** advances on the virtual clock with steps such as `When 500 milliseconds pass`.

Here is a complete example from the test suite:

```gherkin
Feature: Input aimed at a control goes where a hand could put it

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        id: root; objectName: "root"
        width: 300; height: 200
        property int under: 0
        property int saved: 0
        MouseArea { anchors.fill: parent; onClicked: root.under++ }
        Column {
          Button { objectName: "save"; text: "Save"; onClicked: root.saved++ }
          Button { objectName: "locked"; text: "Locked"; enabled: false; onClicked: root.saved++ }
        }
      }
      """

  Scenario: A click goes to the control's middle, with the pointer moved there
    When the user clicks the "button" "Save"
    Then the property "saved" of "root" is 1
    And the property "hovered" of "save" is true
    And the property "under" of "root" is 0

  Scenario: A disabled control is not clicked, nor what is under it
    When the user clicks the "button" "Locked"
    Then the property "saved" of "root" is 0
    And the property "under" of "root" is 0
```

### Running scenarios

In the Ljós workspace, run scenarios with `mise`:

```sh
mise run features:run features/tooling/scene-driver.feature
```

Set `LJOS_SHOTS=<dir>` when running scenarios to save a rendered PNG after every step, which makes diagnosing a failing step straightforward.

### Bringing an external acceptance suite

If you are migrating an existing application that already has its own Gherkin suite, you can run its UI scenarios against Ljós without modifying the suite's `.feature` files. An acceptance directory under `acceptance/<suite>/` holds:

- `PIN`: the application repository and commit hash;
- `features.globs`: an allowlist of `.feature` files or `file.feature:ScenarioName*` globs;
- `RATCHET`: the list of scenario IDs that currently pass;
- a small `steps` adapter crate that maps the application's UI steps (`find`, `click`, `key`, `touch`, property reads) to Ljós's `Backend` trait.

Run the suite against its ratchet with:

```sh
mise run features:acceptance <suite>
```

Passing `--raise` adds newly passing scenarios to `RATCHET` and never removes existing ones.

## See also

- [Command-line reference](./cli.md)
- [MCP servers for agents](./mcp.md)
- [Observer](./observe.md)
