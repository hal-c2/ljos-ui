# Sources:
#   docs/DIALECT.md (§4.4: TypeScript modules)
#   https://www.typescriptlang.org/docs/handbook/2/modules.html
#   https://www.typescriptlang.org/tsconfig/#erasableSyntaxOnly
# Qt imports only JavaScript: a `.ts` module is Ljós's own, so there is nothing to compare it with.

@ljos-only @headless
Feature: TypeScript modules imported by a QML file
  A file imports a TypeScript module as it imports a script, with
  `import "filter.ts" as Logic`, and reaches what the module exports
  through the name. The module's types are only for the checker: they
  are erased before it runs, and what is left runs as a script does. A
  module imports another with `import { x } from "./other.ts"`.

  Background:
    Given the file "filter.ts":
      """ts
      export interface Row { tag: string; count: number }
      export type Tag = "a" | "b"
      const rows: readonly Row[] = [{ tag: "a", count: 2 }, { tag: "b", count: 3 }]
      export function only(tag: Tag): number {
        let total: number = 0
        for (const row of rows) { if (row.tag === tag) { total = total + row.count } }
        return total
      }
      export const label: string = "rows"
      """

  Scenario: A handler of a file in Ljós's API calls a function the module exports
    Given the QML:
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        objectName: "root"
        property real shown: 0
        property string title: Logic.label
        function choose() { shown = Logic.only("a") }
      }
      """
    When the method "choose" of "root" is called
    Then the property "shown" of "root" is 2
    And the property "title" of "root" is "rows"

  Scenario: A module imports what another module exports
    Given the file "sum.ts":
      """ts
      import { only } from "./filter.ts"
      import * as Filter from "./filter.ts"
      export function both(): number { return only("a") + Filter.only("b") }
      """
    And the QML:
      """qml
      import Ljos.Ui
      import "sum.ts" as Sum
      Item {
        objectName: "root"
        property real total: Sum.both()
      }
      """
    Then the property "total" of "root" is 5

  Scenario: A file in Qt's QML imports a module too
    Given the QML:
      """qml
      import QtQuick
      import "filter.ts" as Logic
      Item {
        objectName: "root"
        property int shown: Logic.only("b")
      }
      """
    Then the property "shown" of "root" is 3

  Scenario: A type the module erases does not join the lines around it
    Given the file "steps.ts":
      """ts
      let count: number = 0
      function bump(): number { count = count + 1; return count }
      export const once = bump
      type Step = number
      (bump)()
      export function times(): number { return count }
      """
    And the QML:
      """qml
      import Ljos.Ui
      import "steps.ts" as Steps
      Item {
        objectName: "root"
        property real shown: Steps.times()
      }
      """
    Then the property "shown" of "root" is 1

  Scenario: The checker types what a module exports
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property real shown: Logic.only("a")
        property string title: Logic.label
        function choose() { shown = Logic.only("b") }
      }
      """
    When the files are checked
    Then the check reports:
      """
      2 files, 0 errors, 0 warnings
      """

  Scenario: A call of what a module exports is checked against its parameters
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property real shown: Logic.only("c")
        property string title: Logic.only("a")
        function choose() { shown = Logic.only("a", "b") }
      }
      """
    When the files are checked
    Then the check reports:
      """
      Pick.qml:4:41: error: `tag` is `"a" | "b"`; this gives it `"c"` [ts-assign-type]
      Pick.qml:5:32: error: `title` is a string; this gives it a number [ts-assign-type]
      Pick.qml:6:37: error: this call takes 1 arguments, and is given 2 [ts-assign-type]
      2 files, 3 errors, 0 warnings
      ljos: 3 errors
      """

  Scenario: A name the module does not export is reported
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property real shown: Logic.total("a")
        property real many: Logic.rows.length
      }
      """
    When the files are checked
    Then the check reports:
      """
      Pick.qml:4:36: error: `total` is not exported by filter.ts: it exports `only` and `label` [unknown-member]
      Pick.qml:5:35: error: `rows` is not exported by filter.ts: export it there (`export const rows`) [unknown-member]
      2 files, 2 errors, 0 warnings
      ljos: 2 errors
      """

  Scenario: A module's own errors are reported at the module's lines
    Given the file "bad.ts":
      """ts
      export const limit: number = "10"
      export enum Side { Left, Right }
      export function half(n: number): string { return n / 2 }
      """
    And the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "bad.ts" as Bad
      Item {
        property real shown: Bad.limit
      }
      """
    When the files are checked
    Then the check reports:
      """
      bad.ts:1:36: error: `limit` is a number; this gives it a string [ts-assign-type]
      bad.ts:2:14: error: an `enum` is not only a type, and does not erase: write a union of strings (`type Mode = "a" | "b"`) [ts-forbidden]
      bad.ts:3:56: error: this function returns a string; this gives it a number [ts-assign-type]
      2 files, 3 errors, 0 warnings
      ljos: 3 errors
      """

  Scenario: A module a script imports is checked as one the file imports is
    Given the file "bad.ts":
      """ts
      export enum Side { Left, Right }
      export const limit: number = "10"
      """
    And the file "helper.js":
      """js
      .import "bad.ts" as Bad
      function limit() { return Bad.limit }
      """
    And the file "Pick.qml":
      """qml
      import QtQuick
      import "helper.js" as Helper
      Item {
        property var shown: Helper.limit()
      }
      """
    When the files are checked
    Then the check reports:
      """
      bad.ts:1:14: error: an `enum` is not only a type, and does not erase: write a union of strings (`type Mode = "a" | "b"`) [ts-forbidden]
      bad.ts:2:36: error: `limit` is a number; this gives it a string [ts-assign-type]
      2 files, 2 errors, 0 warnings
      ljos: 2 errors
      """

  Scenario: A file that reaches a module with an error does not load, and says where in the module
    Given the file "bad.ts":
      """ts
      export const limit: number = "10"
      export enum Side { Left, Right }
      """
    And the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "bad.ts" as Bad
      Item {
        property real shown: Bad.limit
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Loader { objectName: "loader"; source: "Pick.qml" }
      }
      """
    Then the property "status" of "loader" is 3
    And the log has a warning matching "Pick\.qml: bad\.ts:1:36: `limit` is a number; this gives it a string"

  Scenario: A test file that reaches a module with an error is a failed test, at the module's line
    Given the file "bad.ts":
      """ts
      export const limit: number = "10"
      """
    And the file "tst_limit.qml":
      """qml
      import QtQuick
      import QtTest
      import "bad.ts" as Bad
      TestCase {
        name: "Limit"
        function test_it() { compare(Bad.limit, "10") }
      }
      """
    When the tests are run
    Then the test report is:
      """
      FAIL!  : tst_limit::compile() `limit` is a number; this gives it a string
         Loc: [bad.ts(1)]
      Totals: 0 passed, 1 failed, 0 skipped
      """

  Scenario: An edit that breaks a module is refused, at the module's line, and the file runs as it was
    Given the QML:
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        objectName: "root"
        property real shown: Logic.only("a")
      }
      """
    When the file "filter.ts" is changed to:
      """ts
      export type Tag = "a" | "b"
      export enum Side { Left, Right }
      export function only(tag: Tag): number { return tag === "a" ? 7 : 0 }
      """
    And a reload happens
    Then the reload error is in "filter.ts" at line 2
    And the property "shown" of "root" is 2

  Scenario: What a module imports from another is checked at the module's lines
    Given the file "sum.ts":
      """ts
      import { only, type Row } from "./filter.ts"
      import * as Filter from "./filter.ts"
      export function both(): number { return only("a") + Filter.only(2) }
      export const first: Row = { tag: "a", count: "2" }
      """
    And the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "sum.ts" as Sum
      Item {
        property real total: Sum.both()
      }
      """
    When the files are checked
    Then the check reports:
      """
      sum.ts:3:71: error: `tag` is `"a" | "b"`; this gives it `2` [ts-assign-type]
      sum.ts:4:33: error: `first.count` is a number; this gives it a string [ts-assign-type]
      3 files, 2 errors, 0 warnings
      ljos: 2 errors
      """

  Scenario: A property typed by the module holds what it is given
    Given the QML:
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        objectName: "root"
        property Logic.Row picked: ({ tag: "b", count: 3 })
        property list<Logic.Row> kept: []
        property Logic.Tag tag: "a"
        property real shown: 0
        property real fired: 0
        signal chosen(row: Logic.Row)
        onChosen: row => { fired = row.count }
        function keep(row: Logic.Row): void { kept = [row]; tag = "b"; shown = row.count; chosen(row) }
        function choose() { keep(picked) }
      }
      """
    When the method "choose" of "root" is called
    Then the property "shown" of "root" is 3
    And the property "tag" of "root" is "b"
    And the property "fired" of "root" is 3

  Scenario: What a declaration typed by the module is given is checked
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property Logic.Tag tag: "c"
        property Logic.Row picked: ({ tag: "a", count: "1" })
        property list<Logic.Row> kept: [picked, 2]
        signal chosen(row: Logic.Row)
        function keep(row: Logic.Row): void { tag = row.tag; chosen(row.count) }
      }
      """
    When the files are checked
    Then the check reports:
      """
      Pick.qml:4:33: error: `tag` is `"a" | "b"`; this gives it `"c"` [ts-assign-type]
      Pick.qml:5:36: error: `picked.count` is a number; this gives it a string [ts-assign-type]
      Pick.qml:6:40: error: `kept` is a list<Logic.Row>; this gives it a (number | { tag: string; count: number })[] [ts-assign-type]
      Pick.qml:8:53: error: `tag` is `"a" | "b"`; this gives it `string` [ts-assign-type]
      Pick.qml:8:69: error: `row` is an object; this gives it a number [ts-assign-type]
      2 files, 5 errors, 0 warnings
      ljos: 5 errors
      """

  Scenario: A type the module does not export is reported
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property Logic.Rows many
        function keep(row: Logic.Line): void { }
      }
      """
    When the files are checked
    Then the check reports:
      """
      Pick.qml:4:24: error: `Rows` is not a type filter.ts exports: it exports the types `Row` and `Tag` [unknown-type]
      Pick.qml:5:34: error: `Line` is not a type filter.ts exports: it exports the types `Row` and `Tag` [unknown-type]
      2 files, 2 errors, 0 warnings
      ljos: 2 errors
      """

  Scenario: TypeScript reads a declaration typed by the module as the checker does
    Given the file "Pick.qml":
      """qml
      import Ljos.Ui
      import "filter.ts" as Logic
      Item {
        property Logic.Row picked: ({ tag: "a", count: 1 })
        function keep(row: Logic.Row): void { picked = row.tag }
      }
      """
    When the files are checked with TypeScript
    Then TypeScript reports the lines:
      """
      Pick.qml:5:47: error: Type 'string' is not assignable to type 'Row'. [TS2322]
      """
