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
