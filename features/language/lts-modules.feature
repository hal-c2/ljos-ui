# Sources:
#   docs/DIALECT.md (§4.4: TypeScript modules; §4.21: compiled away; §5.1: ts-lts-syntax)
#   https://github.com/tc39/proposal-pattern-matching
#   https://github.com/tc39/proposal-pipeline-operator
# Qt imports only JavaScript: `.lts` and `.ts` modules are Ljós's own, so there is nothing to compare with.

@ljos-only @headless
Feature: `.lts` modules hold the new syntax, and `.ts` modules stay TypeScript
  A module that uses `match`, `is` or `|>` ends in `.lts`, for "Ljós
  TypeScript". It is imported, typed and checked as a `.ts` module is,
  and the desugar compiles its syntax away before anything reads it. A
  `.ts` module stays a file that stock `tsc` and editors read, so the new
  syntax in one is an error whose fix renames the file to `.lts`.

  # S5 builds `.lts` with `match`; S6 adds `|>` to it.
  @backlog
  Scenario: An `.lts` module with a `match` compiles and runs
    Given the file "size.lts":
      """ts
      export type Size = "small" | "large"
      export function margin(size: Size): number {
        return match (size) {
          when "small": 4;
          when "large": 12;
        }
      }
      """
    And the QML:
      """qml
      import Ljos.Ui
      import "size.lts" as Size
      Item {
        objectName: "root"
        property real gap: Size.margin("large")
      }
      """
    Then the property "gap" of "root" is 12

  @backlog
  Scenario: The new syntax in a `.ts` module is an error that names the rename
    Given the file "size.ts":
      """ts
      export type Size = "small" | "large"
      export function margin(size: Size): number {
        return match (size) {
          when "small": 4;
          when "large": 12;
        }
      }
      """
    And the file "Pad.qml":
      """qml
      import Ljos.Ui
      import "size.ts" as Size
      Item {
        property real gap: Size.margin("large")
      }
      """
    When the files are checked
    Then the check reports:
      """
      size.ts:3:10: error: `match` is Ljós syntax, which stock TypeScript tools cannot read in a `.ts` file; rename `size.ts` to `size.lts` [ts-lts-syntax]
      2 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  @backlog
  Scenario: Migrating renames such a module to `.lts` and the imports that name it
    Given the file "size.ts":
      """ts
      export function half(n: number): number {
        return n |> % / 2
      }
      """
    And the file "Pad.qml":
      """qml
      import Ljos.Ui
      import "size.ts" as Size
      Item {
        property real gap: Size.half(8)
      }
      """
    When the files are migrated and checked
    Then the check reports:
      """
      2 files, 0 errors, 0 warnings
      """
    And the file "Pad.qml" is:
      """qml
      import Ljos.Ui
      import "size.lts" as Size
      Item {
        property real gap: Size.half(8)
      }
      """

  @backlog
  Scenario: `.ts` and `.lts` modules import each other's values and types
    Given the file "kinds.ts":
      """ts
      export type Kind = "dot" | "dash"
      export const fallback: Kind = "dot"
      """
    And the file "width.lts":
      """ts
      import { type Kind, fallback } from "./kinds.ts"
      export type Width = 1 | 3
      export function width(kind: Kind | undefined): Width {
        return match (kind ?? fallback) {
          when "dot": 1;
          when "dash": 3;
        }
      }
      """
    And the file "line.ts":
      """ts
      import { type Width, width } from "./width.lts"
      export function total(): number {
        const thin: Width = width("dot")
        return thin + width("dash")
      }
      """
    And the QML:
      """qml
      import Ljos.Ui
      import "line.ts" as Line
      Item {
        objectName: "root"
        property real total: Line.total()
      }
      """
    Then the property "total" of "root" is 4

  @backlog
  Scenario: A type an `.lts` module exports is checked where a `.ts` module uses it
    Given the file "width.lts":
      """ts
      export type Width = 1 | 3
      export function width(thick: boolean): Width {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "line.ts":
      """ts
      import { type Width } from "./width.lts"
      export const wide: Width = 2
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "line.ts" as Line
      Item {
        property real w: Line.wide
      }
      """
    When the files are checked
    Then the check reports:
      """
      line.ts:2:28: error: `wide` is `1 | 3`; this gives it `2` [ts-assign-type]
      3 files, 1 errors, 0 warnings
      ljos: 1 errors
      """
