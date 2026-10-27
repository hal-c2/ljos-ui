---
title: Testing
description: Test QML applications with tst_*.qml files and ljos test, render deterministic pictures with ljos render, and write Gherkin behaviour scenarios.
section: start
order: 6
---

# Testing

This page shows you how to run Qt Quick Test (`tst_*.qml`) suites with `ljos test`, capture deterministic headless screenshots with `ljos render`, and write Gherkin behaviour scenarios for your application.

## QML unit tests with `ljos test`

Files named `tst_*.qml` that use `import QtTest` and `TestCase` run unchanged on Ljós. `ljos test` executes them headlessly on a virtual clock and prints the output in the same format as Qt's `qmltestrunner`.

### Writing a `TestCase`

Create a file named `tst_click.qml`:

```qml
import QtQuick
import QtTest

Item {
    width: 200
    height: 200

    MouseArea {
        id: area
        x: 20
        y: 30
        width: 100
        height: 80
        property point at
        onClicked: (mouse) => at = Qt.point(mouse.x, mouse.y)
    }

    Item {
        id: box
        signal poked
    }

    SignalSpy {
        id: spy
        target: box
        signalName: "poked"
    }

    TestCase {
        name: "Click"
        when: windowShown

        function test_middle() {
            mouseClick(area)
            compare(area.at, Qt.point(50, 40))
        }

        function test_position() {
            mouseClick(area, 3, 4)
            compare(area.at, Qt.point(3, 4))
        }

        function test_spy() {
            box.poked()
            compare(spy.count, 1)
            spy.clear()
        }
    }
}
```

Run `ljos test` in the directory (or pass specific files or directories):

```sh
ljos test tst_click.qml
```

```text
PASS   : Click::initTestCase()
PASS   : Click::test_middle()
PASS   : Click::test_position()
PASS   : Click::test_spy()
PASS   : Click::cleanupTestCase()
Totals: 5 passed, 0 failed, 0 skipped
```

### Checks, input and options

Inside a `TestCase` function you can use:

- **Assertions:** `compare(actual, expected)`, `verify(condition, message)`, `fuzzyCompare(actual, expected, delta)`, `tryCompare(obj, property, expected, timeout)`, `tryVerify(function, timeout)` and `fail(message)`.
- **Signals:** `SignalSpy` to count signal emissions and inspect signal arguments.
- **Input synthesis:** `mouseClick`, `mousePress`, `mouseRelease`, `mouseMove`, `keyClick`, `keyPress`, `keyRelease` and `TouchEventSequence`. Input events travel through the same hit-testing and focus paths as real user input, so clicking a `MouseArea` with no coordinates clicks its centre `(width / 2, height / 2)`.
- **Time:** `wait(ms)` and `waitForRendering(item)` advance the virtual clock without sleeping in real wall-clock time.

Use `--filter` to run a single case, function or data tag, `--import` to add module search paths, and `--json` to save a machine-readable report:

```sh
ljos test --filter Click::test_middle --json results.json .
```

The exit status of `ljos test` is the number of failed tests (or `0` when all pass).

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
