# Sources:
#   docs/DIALECT.md (§4.4: `unknown` enters at a boundary through a decoder; §5.1: ts-no-any, ts-no-cast, ts-boundary)
#   https://www.typescriptlang.org/docs/handbook/2/narrowing.html
#   https://www.typescriptlang.org/docs/handbook/2/narrowing.html#using-type-predicates
#   https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-9.html (unlisted properties with `in`)
# `unknown`, decoders and the typed boundary are Ljós's own API: Qt's QML has nothing to compare them with.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Decoders: what is unknown is narrowed before it is used
  A value whose shape the checker cannot see is `unknown`: a `property
  unknown`, what `JSON.parse` gives, what comes back from a JavaScript
  file. Nothing casts it. A script narrows it with what TypeScript
  narrows with, `typeof`, `=== null`, `in` and `Array.isArray`, or hands
  it to a decoder: a function a TypeScript module exports that says what
  it found, `v is Row`, or throws when it is not there, `asserts v is
  Row`. What has been narrowed is typed from there on.

  Rule: A script narrows unknown with what TypeScript narrows it with

    Scenario: typeof, a comparison with null, in and Array.isArray narrow an unknown value
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        Item {
          property unknown given
          property string label: ""
          property real total: 0
          function take(): void {
            if (typeof given === "object" && given !== null && "label" in given && typeof given.label === "string") { label = given.label }
          }
          function sum(v: unknown): number {
            if (!Array.isArray(v)) { return 0 }
            let all: number = 0
            for (const x of v) { if (typeof x === "number") { all = all + x } }
            return all
          }
          function count(v: unknown): void {
            if (typeof v !== "object" || v === null || !("count" in v)) { return }
            if (typeof v.count === "number") { total = v.count }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A member of an unknown value, and one of an object not known to have it, say how to narrow
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        Item {
          property unknown given
          property string label: ""
          function take(): void { label = given.label }
          function look(): void { if ("label" in given) { label = "found" } }
          function half(): void { if (typeof given === "object" && given !== null) { label = given.label } }
          function loose(): void { if (typeof given === "object" && given !== null && "label" in given) { label = given.label } }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Show.qml:5:49: error: `label` has no type: `given` is unknown: narrow it first (`typeof given === "object" && given !== null && "label" in given`), or decode it with a type guard [ts-no-any]
        Show.qml:6:50: error: `given` has no type: it is unknown, and `in` looks into an object: check `typeof given === "object" && given !== null` first [ts-no-any]
        Show.qml:7:100: error: `label` has no type: `given` is an object not known to have it: check `"label" in given` first [ts-no-any]
        Show.qml:8:115: error: `label` is a string; this gives it an unknown value [ts-assign-type]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: Null ruled out first, or by an early return, still lets typeof narrow to an object
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        Item {
          function first(v: unknown): string {
            if (v !== null && typeof v === "object" && "tag" in v && typeof v.tag === "string") { return v.tag }
            return ""
          }
          function early(v: unknown): string {
            if (v === null) { return "" }
            if (typeof v === "object" && "tag" in v && typeof v.tag === "string") { return v.tag }
            return ""
          }
          function bare(v: unknown): real {
            if (v !== null) { return v.size }
            return 0
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Show.qml:13:38: error: `v` may be undefined: compare it with `undefined` first [ts-assign-type]
        Show.qml:13:40: error: `size` has no type: `v` is unknown: narrow it first (`typeof v === "object" && v !== null && "size" in v`), or decode it with a type guard [ts-no-any]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: instanceof is not in the subset
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        Item {
          property unknown given
          property bool listed: false
          function take(): void { listed = given instanceof Array }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Show.qml:5:50: error: `instanceof` is not in Ljós's API: narrow with `typeof`, `in`, `Array.isArray` or a type guard [ts-forbidden]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A TypeScript module narrows an unknown value as a script does
      Given the file "read.ts":
        """ts
        export function label(v: unknown): string {
          if (typeof v === "object" && v !== null && "label" in v && typeof v.label === "string") { return v.label }
          return ""
        }
        export function size(v: unknown): number { return v.size }
        """
      And the file "Show.qml":
        """qml
        import Ljos.Ui
        import "read.ts" as Read
        Item {
          property string shown: Read.label({ label: "one" })
        }
        """
      When the files are checked
      Then the check reports:
        """
        read.ts:5:61: error: `size` has no type: `v` is unknown: narrow it first (`typeof v === "object" && v !== null && "size" in v`), or decode it with a type guard [ts-no-any]
        2 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A handler narrows an unknown value it is given, and shows it
      Given the file "read.ts":
        """ts
        export function label(v: unknown): string {
          if (typeof v === "object" && v !== null && "label" in v && typeof v.label === "string") { return v.label }
          return "none"
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "read.ts" as Read
        Item {
          objectName: "root"
          property unknown given: ({ label: "one" })
          property string shown: ""
          property string other: ""
          property real total: 0
          function show() {
            shown = Read.label(given)
            other = Read.label(3)
            const all: unknown = [1, 2, "three"]
            if (Array.isArray(all)) { for (const x of all) { if (typeof x === "number") { total = total + x } } }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "one"
      And the property "other" of "root" is "none"
      And the property "total" of "root" is 3

  Rule: A type guard and an assertion function narrow what they are given

    Background:
      Given the file "row.ts":
        """ts
        export interface Row { tag: string; count: number }
        export function isRow(v: unknown): v is Row {
          return typeof v === "object" && v !== null && "tag" in v && typeof v.tag === "string" && "count" in v && typeof v.count === "number"
        }
        export function row(v: unknown): asserts v is Row {
          if (!isRow(v)) { throw new Error("not a row") }
        }
        """

    Scenario: A guard narrows in if, &&, ?:, !, an early return and filter, and an assertion narrows what follows it
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        import "row.ts" as Rows
        Item {
          property unknown given
          property real shown: 0
          property string label: ""
          function isBig(v: unknown): v is number { return typeof v === "number" && v > 10 }
          function viaIf(): void { if (Rows.isRow(given)) { shown = given.count } }
          function viaAnd(): void { label = Rows.isRow(given) && given.tag === "a" ? "a" : "other" }
          function viaReturn(v: unknown): number { if (!Rows.isRow(v)) { return 0 } return v.count }
          function viaElse(v: unknown): string { return !Rows.isRow(v) ? "none" : v.tag }
          function viaFilter(all: readonly unknown[]): number { const rows: readonly Rows.Row[] = all.filter(Rows.isRow); return rows.length }
          function viaAssert(v: unknown): number { Rows.row(v); return v.count }
          function viaOwn(v: unknown): number { return isBig(v) ? v : 0 }
        }
        """
      When the files are checked
      Then the check reports:
        """
        2 files, 0 errors, 0 warnings
        """

    Scenario: What a guard has not said, a guard that gives no boolean, and one that narrows to what cannot be are reported
      Given the file "bad.ts":
        """ts
        export function isTag(v: unknown): v is string { return 1 }
        export function isCount(v: string): v is number { return false }
        """
      And the file "Show.qml":
        """qml
        import Ljos.Ui
        import "row.ts" as Rows
        import "bad.ts" as Bad
        Item {
          property unknown given
          property real shown: 0
          function missed(): void { if (!Rows.isRow(given)) { shown = 1 } shown = given.count }
          function wrong(v: unknown): void { Rows.row(v); shown = v.tag }
          function rows(all: readonly unknown[]): void { const kept: readonly Rows.Row[] = all.filter(Bad.isTag) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Show.qml:7:89: error: `count` has no type: `given` is unknown: narrow it first (`typeof given === "object" && given !== null && "count" in given`), or decode it with a type guard [ts-no-any]
        Show.qml:8:67: error: `shown` is a real; this gives it a string [ts-assign-type]
        Show.qml:9:92: error: `kept` is a readonly { tag: string; count: number }[]; this gives it a readonly string[] [ts-assign-type]
        bad.ts:1:65: error: this function returns a boolean; this gives it a number [ts-assign-type]
        bad.ts:2:45: error: `v is number` narrows `v`, a string, to what it cannot be: a number [ts-assign-type]
        3 files, 5 errors, 0 warnings
        ljos: 5 errors
        """

    Scenario: An assertion function the component declares narrows nothing, as TypeScript says
      Given the file "Show.qml":
        """qml
        import Ljos.Ui
        import "row.ts" as Rows
        Item {
          property real shown: 0
          function check(v: unknown): asserts v is number { if (typeof v !== "number") { throw new Error("no") } }
          function own(v: unknown): void { check(v); shown = v }
          function nested(v: unknown): void {
            function local(w: unknown): asserts w is number { if (typeof w !== "number") { throw new Error("no") } }
            local(v)
            shown = v
          }
          function imported(v: unknown): void { Rows.row(v); shown = v.count }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Show.qml:6:44: error: `check` asserts, and TypeScript narrows by an assertion only through names declared with a type, which `check` is not (TS2775): move it into a TypeScript module the file imports [ts-assign-type]
        Show.qml:6:62: error: `shown` is a real; this gives it an unknown value [ts-assign-type]
        2 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A handler decodes what it is given with a guard and an assertion
      Given the QML:
        """qml
        import Ljos.Ui
        import "row.ts" as Rows
        Item {
          objectName: "root"
          property unknown given: ({ tag: "a", count: 4 })
          property unknown bad: ({ tag: "a" })
          property real shown: 0
          property real fallback: 0
          property string caught: ""
          function show() {
            if (Rows.isRow(given)) { shown = given.count }
            const rows: readonly Rows.Row[] = [given, bad, 3].filter(Rows.isRow)
            fallback = rows.length
            try { Rows.row(bad); fallback = 99 } catch (e) { caught = "not a row" }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is 4
      And the property "fallback" of "root" is 1
      And the property "caught" of "root" is "not a row"
