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

    Scenario: `!` takes a boolean, as a condition does
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property int clicks: 0
          property bool opened: !clicks
          property bool closed: !opened
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:34: error: `!` needs a boolean, and `clicks` is a number: compare it [ts-condition-boolean]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
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
          Text { text: label.length; align: Text.AlignHCenter; verticalAlignment: Text.Fit }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:32: error: `level` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:5:18: error: `width` is a real; this gives it a string [ts-assign-type]
        Gauge.qml:6:24: error: `text` is a string; this gives it a number [ts-assign-type]
        Gauge.qml:6:83: error: `verticalAlignment` is a Text.VAlignment; this gives it `Text.Fit` [ts-assign-type]
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
        Gauge.qml:5:81: error: `size` has no type: `given` is unknown: narrow it first (`typeof given === "object" && given !== null && "size" in given`), or decode it with a type guard [ts-no-any]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: An attached object read off an item is of the type attached
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          Rectangle { id: card }
          function show() { card.DragSource.active = true; console.log(card.DragSource.nope) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:88: error: `nope` has no type: a DragAttached has no member of that name [ts-no-any]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
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

    Scenario: What the test of `?:` reads has a type too
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Rectangle {
          fill: area.hovered ? "#eeeeee" : "#ffffff"
          PointerArea { id: area; hoverEnabled: true }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:22: error: `hovered` has no type: a PointerArea has no member of that name [ts-no-any]
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

  Rule: A script keeps to the rest of the subset's rules

    Scenario: `as` and `!` are reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property unknown last
          function read(given: number | undefined): void { level = last as number; level = given!; level = <number>last }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:73: error: `as` tells the checker a type it cannot see: narrow the value with `typeof` or a comparison [ts-no-cast]
        Gauge.qml:5:97: error: `!` tells the checker a value is not undefined: compare it with `undefined` first [ts-no-cast]
        Gauge.qml:5:108: error: `<number>` tells the checker a type it cannot see: narrow the value with `typeof` or a comparison [ts-no-cast]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A narrowed value, and `as const`, need no cast
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property unknown last
          function read(given: number | undefined): void { if (typeof last === "number") { level = last } if (given !== undefined) { level = given } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A switch on a union that leaves a case out is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string mode: "idle"
          function step(kind: "start" | "stop" | "pause" | "skip", on: boolean): void {
            switch (kind) { case "start": mode = "running"; break; case "stop": mode = "idle"; break }
            switch (on) { case true: mode = "on" }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:13: error: this `switch` leaves out `"pause"` and `"skip"`: add a case for each, or a `default` [ts-switch-exhaustive]
        Gauge.qml:6:13: error: this `switch` leaves out `false`: add a case for it, or a `default` [ts-switch-exhaustive]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A switch that covers every case, or has a default, has nothing to report
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string mode: "idle"
          function step(kind: "start" | "stop", on: boolean): void {
            switch (kind) { case "start": mode = "running"; break; case "stop": mode = "idle"; break }
            switch (on) { case true: mode = "on"; break; case false: mode = "off" }
            switch (mode) { case "idle": break; default: mode = "idle" }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: Cases that fall through share a body that sees the subject as any of their labels
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string mode: "idle"
          function onlyTodo(s: "todo"): void { mode = s }
          function onlyDoing(s: "doing"): void { mode = s }
          function step(status: "todo" | "doing" | "done"): void {
            switch (status) { case "todo": case "doing": if (status === "todo") onlyTodo(status); break; default: break }
            switch (status) { case "todo": case "doing": onlyDoing(status); break; case "done": onlyTodo(status); break }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:8:68: error: `s` is `"doing"`; this gives it `"todo" | "doing"` [ts-assign-type]
        Gauge.qml:8:106: error: `s` is `"todo"`; this gives it `"done"` [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A case that falls through carries what its body made of the subject
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string mode: "idle"
          function onlyDoing(s: "doing"): void { mode = s }
          function onlyNotDone(s: "todo" | "doing"): void { mode = s }
          function step(given: "todo" | "doing" | "done"): void {
            let status: "todo" | "doing" | "done" = given
            switch (status) { case "todo": status = "doing"; case "doing": onlyDoing(status); break; default: break }
            switch (status) { case "todo": status = "done"; case "doing": onlyNotDone(status); break; default: break }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:9:87: error: `s` is `"doing" | "todo"`; this gives it `"doing" | "done"` [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A variable widens a written literal, and keeps a type it is given
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property string mode: "idle"
          function onlyKnown(s: "todo" | "doing" | "done"): void { mode = s }
          function step(given: "todo" | "doing" | "done", on: boolean): void {
            let copied = given
            onlyKnown(copied)
            const first = "todo"
            let started = first
            let picked = on ? "todo" : "done"
            onlyKnown(started)
            onlyKnown(picked)
            let either = on ? given : 0
            if (typeof either === "string") onlyKnown(either)
            let below = on ? given : -1
            if (typeof below === "string") onlyKnown(below)
            const kept = on ? given : -1
            let again = kept
            if (typeof again === "string") onlyKnown(again)
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:11:23: error: `s` is `"todo" | "doing" | "done"`; this gives it `string` [ts-assign-type]
        Gauge.qml:12:23: error: `s` is `"todo" | "doing" | "done"`; this gives it `string` [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A write to a readonly member or array is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property list<real> levels
          function add(row: { readonly id: number; name: string }, xs: readonly number[]): void { row.id = 2; row.name = "x"; xs.push(1); xs[0] = 2; levels.push(3) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:103: error: `id` is readonly: build a new object with the value it should have [ts-readonly]
        Gauge.qml:4:130: error: `push` changes the array, and it is a readonly number[]: build a new one [ts-readonly]
        Gauge.qml:4:139: error: `xs` is a readonly number[]: build a new array rather than change it [ts-readonly]
        Gauge.qml:4:157: error: `push` changes the array, and it is a readonly number[]: build a new one [ts-readonly]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: A mutable object or array, and a property of a list given a new one, may be written
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property list<real> levels
          function add(row: { id: number }, xs: number[]): void { row.id = 2; xs.push(1); xs[0] = 2; levels = [...levels, 3] }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A record with a readonly member is not given where the member may be written
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function rename(row: { readonly id: number; name: string }): void { const next: { id: number; name: string } = row; const seen: { readonly id: number } = row }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:122: error: `next` may have its `id` written; this gives it one whose `id` is readonly: build a new object, or make the place readonly [ts-readonly]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: An element read by index may be undefined
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function first(xs: readonly number[]): void { level = xs[0]; const n: number = xs[1] }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:65: error: `xs[0]` may be undefined, as an index may be past the end: check it first [ts-index-undefined]
        Gauge.qml:4:90: error: `xs[1]` may be undefined, as an index may be past the end: check it first [ts-index-undefined]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: An element read by index and checked has nothing to report
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function first(xs: readonly number[]): void { const n = xs[0]; if (n !== undefined) { level = n } level = xs[1] ?? 0 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A member of what may be undefined, and a call of it, are reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function first(xs: readonly string[], done?: () => void): void { level = xs[0].length; done() }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:84: error: `xs[0]` may be undefined, as an index may be past the end: check it first [ts-index-undefined]
        Gauge.qml:4:98: error: `done` may be undefined: compare it with `undefined` first [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: What may be undefined, checked first, may be read and called
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function first(xs: readonly string[], done?: () => void): void { const x = xs[0]; if (x !== undefined) { level = x.length } if (done !== undefined) { done() } done?.() }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: An optional member set to undefined is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function tag(given: string | undefined): void { const a: { name?: string } = { name: undefined }; let b: { name?: string } = {}; b.name = given }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:96: error: `name` is optional: leave it out rather than set it to undefined [ts-optional-exact]
        Gauge.qml:3:149: error: `name` is optional: leave it out rather than set it to undefined [ts-optional-exact]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: An optional member left out, or one whose type says undefined, has nothing to report
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function tag(given: string | undefined): void { let b: { name?: string } = {}; b = given === undefined ? {} : { name: given }; const c: { name?: string | undefined } = { name: given } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A mutable array, or a method, given where variance refuses it is reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function share(counts: number[]): void { const both: (number | string)[] = counts; const take: { f(x: number | string): void } = { f: (x: number): void => {} } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:86: error: `both` is a (number | string)[]; this gives it a number[], and a mutable array holds only its own type: copy it, or make the place readonly [ts-variance]
        Gauge.qml:3:145: error: `f` is `(x: number | string) => void`; this gives it `(x: number) => void`, and a function must take everything its place may be given [ts-variance]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A readonly array, and a method that takes more, are given where they go
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function share(counts: number[]): void { const seen: readonly (number | string)[] = counts; const take: { f(x: number): void } = { f: (x: number | string): void => {} } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: Classes, `this` in a function, prototypes, `delete`, `for…in`, `eval` and `arguments` are reported
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function odd(row: { id?: number }): void { delete row.id; for (const key in row) {} const f = function (): void { this.width = 1 } }
          function worse(): number { eval("1"); return arguments.length }
          function worst(): void { class Box {} const p = level.prototype }
          property real level: 0
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:3:54: error: `delete` is not in Ljós's API: make the member optional, and build the object without it [ts-forbidden]
        Gauge.qml:3:69: error: `for…in` is not in Ljós's API: write `for…of` over an array or a `Map` [ts-forbidden]
        Gauge.qml:3:125: error: `this` is not in Ljós's API: in a `function` it is whatever calls it; name the object by its id [ts-forbidden]
        Gauge.qml:4:38: error: `eval` is not in Ljós's API: write the code it would run [ts-forbidden]
        Gauge.qml:4:56: error: `arguments` is not in Ljós's API: write a rest parameter (`...args: number[]`) [ts-forbidden]
        Gauge.qml:5:36: error: `class` is not in Ljós's API: write a function, or a component [ts-forbidden]
        Gauge.qml:5:65: error: `prototype` is not in Ljós's API: write a function that takes the object [ts-forbidden]
        1 files, 7 errors, 0 warnings
        ljos: 7 errors
        """

    Scenario: `this` in QML's scope, and in an arrow there, is the scope object
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function grow(by: number): void { this.width = this.width + by; const ys = [1, 2]; for (const y of ys) { by = y } const f = (): void => { this.height = by } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
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
