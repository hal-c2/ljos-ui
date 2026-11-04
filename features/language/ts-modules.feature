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
      """
      export interface Row { tag: string; count: number }
      export type Tag = "a" | "b"
      const rows: readonly Row[] = [{ tag: "a", count: 2 }, { tag: "b", count: 3 }]
      export function only(tag: Tag): number {
        let total: number = 0
        for (const row of rows) { if (row.tag === tag) { total = total + row.count } }
        return total
      }
      export const label = "rows" as string
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
      """
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
