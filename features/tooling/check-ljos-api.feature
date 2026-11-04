# Sources:
#   docs/DIALECT.md (the checker's rules and how it says them)
#   https://www.typescriptlang.org/docs/handbook/2/everyday-types.html
#   https://www.typescriptlang.org/docs/handbook/2/narrowing.html
# `Ljos.Ui` and its checker are Ljós's own: Qt has nothing to compare them with.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Checking the scripts of a file written in Ljós's API
  A file that imports `Ljos.Ui` has its scripts held to the strict
  subset of TypeScript Ljós's API is written in. A value goes only where
  its type does, a condition is a boolean, there is no `null`, `==` or
  `var`, and each is an error before anything runs. A file in Qt's QML
  is not held to them, and neither is one that says its scripts are
  still JavaScript: `ljos migrate` writes that, and a file is typed by
  taking it out.

  Rule: A script of a file in Ljós's API is held to the subset

    Scenario: A script that keeps to the subset has nothing to report
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property bool full: level >= 1
          function fill(by: real) { if (full === false) { level = level + by } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A string written to a number is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function reset() { level = "0" }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:38: error: `level` is a real; this gives it a string [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: What the subset leaves out of JavaScript is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property bool empty: level == 0
          function clear() { var was = level; if (level) { level = 0 } }
          property unknown last: null
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:32: error: `==` converts what it compares; write `===` [ts-loose-equality]
        Gauge.qml:5:30: error: `var` is the function's, wherever it is written; write `let` or `const` [ts-no-var]
        Gauge.qml:5:51: error: `if` needs a boolean, and `level` is a number: compare it [ts-condition-boolean]
        Gauge.qml:6:34: error: `null` is not a value here: a value that may be missing is optional (`?`) [ts-no-null]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: A name a script declares is the script's, not the property it shadows
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property string mode: ""
          signal moved(open: bool)
          property real open: 0
          function choose(level: bool) { if (level) { mode = "full" } }
          function count() { let mode = 0; mode = 1; return mode }
          onMoved: if (open) { level = 1 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

  Rule: The checker knows the type of every value a script makes

    Scenario: A local takes the type of what it starts as
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property string label: "gauge"
          function show() { const text = label + "!"; let count = 0; count = text; level = text }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:78: error: `count` is a number; this gives it a string [ts-assign-type]
        Gauge.qml:5:92: error: `level` is a real; this gives it a string [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A parameter hides the property of its name
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string level: ""
          function set(level: real) { level = 2 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A let hides the property of its name in its block only
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function name() { if (level > 1) { let level = "full"; level = "empty" } level = "none" }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:92: error: `level` is a real; this gives it a string [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: Members and calls have the types of what they read and give
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          id: root
          property real level: 0
          property string name: ""
          function grow(by: real): real { return Math.max(level, by) }
          function rename() { name = grow(2); root.level = name.length; level = name.toUpperCase() }
          PointerArea { onClicked: root.width = root.name }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:7:38: error: `name` is a string; this gives it a number [ts-assign-type]
        Gauge.qml:7:81: error: `level` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:8:49: error: `width` is a real; this gives it a string [ts-assign-type]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: Arrays, arrows, templates and object literals are typed
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real total: 0
          property string label: ""
          function sum(values: readonly real[]) {
            total = values.reduce((sum, value) => sum + value, 0)
            label = values.map((value) => `${value} cm`).join(", ")
            total = values.filter((value) => value > 1).length
            const box = { width: 2, label: "two" }
            total = box.label
            label = values.map((value) => value * 2)
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:10:21: error: `total` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:11:21: error: `label` is a string; this gives it a readonly number[] [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: An operator gives the type of what it makes, and takes only what it works on
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property bool full: false
          property string label: full ? 1 : "none"
          property real twice: level * "2"
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:34: error: `label` is a string; this gives it a number or a string [ts-assign-type]
        Gauge.qml:6:40: error: `*` needs numbers, and this is a string [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A comparison with undefined and typeof narrow what a name is
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property unknown given
          function follow(next: real | undefined) { if (next !== undefined) { level = next } level = next }
          function take() { if (typeof given === "number") { level = given } level = given }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:102: error: `level` is a real; this gives it a number or undefined [ts-assign-type]
        Gauge.qml:6:86: error: `level` is a real; this gives it an unknown value [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: Optional chaining gives undefined where the object may be missing
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function follow(other?: Item) { level = other?.width ?? 0; level = other?.width }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:78: error: `level` is a real; this gives it a number or undefined [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A binding gives its property the type of its value
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string label: "full"
          property real level: label
          width: label + "!"
          Text { text: label.length; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.Fit }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:32: error: `level` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:5:18: error: `width` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:6:24: error: `text` is a string; this gives it a number [ts-assign-type]
        Gauge.qml:6:97: error: `verticalAlignment` is a Text.VAlignment; this gives it `Text.Fit` [ts-assign-type]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

  Rule: Nothing a script reads is without a type

    Scenario: A parameter, or a local, with nothing to type it is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function grow(by) { level = level + by }
          function start() { let total; const none = []; level = total }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:25: error: `by` has no type: write its type (`by: number`, say) [ts-no-any]
        Gauge.qml:5:34: error: `total` has no type: write its type or give it a value [ts-no-any]
        Gauge.qml:5:54: error: `[]` has no type: write the array's type where it goes (`const xs: number[] = []`) [ts-no-any]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A property of `var`, a signal parameter without a type and `any` are reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property var given
          property unknown kept
          signal moved(by)
          function take(value: any) { kept = value }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:20: error: `given` has no type: write `property unknown` or a type [ts-no-any]
        Gauge.qml:5:24: error: `by` has no type: write its type (`by: real`, say) [ts-no-any]
        Gauge.qml:6:32: error: `any` has no type: write `unknown` or a type [ts-no-any]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A name not in scope, and a member its value does not have, have no type
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property unknown given
          function show() { level = missing; level = "full".size; level = given.size }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:37: error: `missing` has no type: it is not in scope [ts-no-any]
        Gauge.qml:5:61: error: `size` has no type: a string has no member of that name [ts-no-any]
        Gauge.qml:5:81: error: `size` has no type: an unknown value has no member of that name [ts-no-any]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A handler's parameters, and a callback's, take their types from where they go
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          signal moved(by: real)
          onMoved: (by) => { level = by }
          onWidthChanged: level = width
          function scaled(values: readonly real[]): readonly real[] { return values.map((value) => value * 2) }
          PointerArea { onClicked: level = parent.width }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A handler's parameter has its signal's type where it is used
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string label: ""
          signal moved(by: real)
          onMoved: label = by
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:28: error: `label` is a string; this gives it a number [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: An object spread into another gives its members, and what follows wins
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property string label: ""
          function merge(base: { readonly level: real; readonly label: string }, change: { readonly level?: real }) {
            const next = { ...base, ...change }
            level = next.level
            label = { ...base, label: 2 }.label
            label = { ...level }.label
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:8:21: error: `label` is a string; this gives it a number [ts-assign-type]
        Gauge.qml:9:23: error: `...level` has no type: only an object of a `{ … }` type spreads [ts-no-any]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

  Rule: A file in Qt's QML is not held to it

    Scenario: The same script in Qt's QML has nothing to report
      Given the file "Gauge.qml":
        """qml
        import QtQuick
        Item {
          property real level: 0
          property bool empty: level == 0
          function clear() { var was = level; if (level) { level = 0 } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A file whose scripts are still JavaScript is not held to it
      Given the file "Gauge.qml":
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        Item {
          property real level: 0
          property bool empty: level == 0
          function clear() { var was = level; if (level) { level = 0 } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A file migrated from Qt's QML compiles as it did
      Given the file "Gauge.qml":
        """qml
        import QtQuick
        Rectangle {
          property real level: 0
          property bool empty: level == 0
          color: level == 0 ? "red" : "green"
          function clear() { var was = level; if (level) { level = null } }
        }
        """
      When the files are migrated and checked
      Then the check reports:
        """
        lowered Gauge.qml (2 edits)
        1 files, 1 lowered, 0 kept
        1 files, 0 errors, 0 warnings
        """
