# Sources:
#   docs/DIALECT.md (§4.4: TypeScript modules; §4.21: compiled away; §5.1: ts-lts-syntax, module-clash, lts-export-type)
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

  Scenario: The new syntax in a `.ts` module is an error that names the rename
    Given the file "size.ts" without its indentation:
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

  Scenario: Migrating renames such a module to `.lts` and the imports that name it
    Given the file "size.ts" without its indentation:
      """ts
      export function half(n: number): number {
        return n |> % / 2
      }
      """
    And the file "line.ts" without its indentation:
      """ts
      import { half } from "./size.ts"
      export const quarter: number = half(half(8))
      """
    And the file "qmldir" without its indentation:
      """
      Size 1.0 size.ts
      """
    And the file "Pad.qml" without its indentation:
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
      renamed size.ts to size.lts, and the imports that name it in Pad.qml, line.ts, qmldir; files outside . were not looked in
      1 files, 0 lowered, 0 kept
      2 files, 0 errors, 0 warnings
      """
    And the migrated file "line.ts" is:
      """ts
      import { half } from "./size.lts"
      export const quarter: number = half(half(8))
      """
    And the migrated file "qmldir" is:
      """
      Size 1.0 size.lts
      """
    And the migrated file "Pad.qml" is:
      """qml
      import Ljos.Ui
      import "size.lts" as Size
      Item {
        property real gap: Size.half(8)
      }
      """

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
    And the file "line.ts" without its indentation:
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
    And the file "tsconfig.json":
      """json
      { "extends": "./.ljos/tsconfig.base.json" }
      """
    When the files are checked
    Then the check reports:
      """
      line.ts:2:28: error: `wide` is `1 | 3`; this gives it `2` [ts-assign-type]
      3 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  # S7: `--tsc` types the importer against the `.lts` module's declaration.
  Scenario: TypeScript types a `.ts` module against the `.lts` module it imports
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
    And the file "line.ts" without its indentation:
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
    When the files are checked with TypeScript
    Then TypeScript reports the lines:
      """
      line.ts:2:14: error: Type '2' is not assignable to type 'Width'. [TS2322]
      """

  # S5: one module name, one file.
  Scenario: A `.ts` and an `.lts` module of the same name are an error
    Given the file "size.ts":
      """ts
      export const gap: number = 4
      """
    And the file "size.lts":
      """ts
      export const gap: number = 12
      """
    And the file "Pad.qml":
      """qml
      import Ljos.Ui
      import "size.lts" as Size
      Item {
        property real gap: Size.gap
      }
      """
    And the file "tsconfig.json":
      """json
      { "extends": "./.ljos/tsconfig.base.json" }
      """
    When the files are checked
    Then the check reports:
      """
      size.lts:1:1: error: `size.ts` and `size.lts` are one module name; keep one file [module-clash]
      size.ts:1:1: error: `size.ts` and `size.lts` are one module name; keep one file [module-clash]
      3 files, 2 errors, 0 warnings
      ljos: 2 errors
      """

  # S7: the declaration stock TypeScript reads for an `.lts` module.
  Scenario: Checking emits a declaration for an `.lts` module in the generated directory
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
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      { "extends": "./.ljos/tsconfig.base.json" }
      """
    When the files are checked
    Then the check reports:
      """
      2 files, 0 errors, 0 warnings
      """
    And the file ".ljos/types/width.d.lts.ts" is:
      """ts
      // Generated by ljos check from width.lts; do not edit.
      export type Width = 1 | 3;
      export declare function width(thick: boolean): Width;
      """

  # S5: the rename is withheld when its target is taken.
  Scenario: The rename is not offered when an `.lts` module of the name is already there
    Given the file "size.ts" without its indentation:
      """ts
      export function margin(big: boolean): number {
        return match (big) {
          when true: 12;
          when false: 4;
        }
      }
      """
    And the file "size.lts":
      """ts
      export const margin: number = 4
      """
    And the file "tsconfig.json":
      """json
      { "extends": "./.ljos/tsconfig.base.json" }
      """
    When the files are checked
    Then the check reports:
      """
      size.lts:1:1: error: `size.ts` and `size.lts` are one module name; keep one file [module-clash]
      size.ts:1:1: error: `size.ts` and `size.lts` are one module name; keep one file [module-clash]
      size.ts:2:10: error: `match` is Ljós syntax, which stock TypeScript tools cannot read in a `.ts` file; it would be renamed to `size.lts`, but `size.lts` is in the way [ts-lts-syntax]
      2 files, 3 errors, 0 warnings
      ljos: 3 errors
      """

  # S7: the declaration is TypeScript's own emit, so an export needs its type.
  Scenario: An `.lts` module's export without a declared type is an error
    Given the file "width.lts" without its indentation:
      """ts
      export function width(thick: boolean) {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      { "extends": "./.ljos/tsconfig.base.json" }
      """
    When the files are checked
    Then the check reports:
      """
      width.lts:1:17: error: `width` is exported from an `.lts` module, so its result needs a declared type for the module's declaration [lts-export-type]
      2 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: A project with an `.lts` module and no tsconfig.json is told to get one
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    When the files are checked
    Then the check reports:
      """
      width.lts:1:1: warning: no `tsconfig.json` extends `.ljos/tsconfig.base.json`, so stock TypeScript cannot resolve an import of `width.lts`; `ljos migrate` writes one [tsconfig-lts]
      2 files, 0 errors, 1 warnings
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: Migrating writes a tsconfig.json that extends the base
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    When the files are migrated and checked
    Then the check reports:
      """
      wrote tsconfig.json
      1 files, 0 lowered, 0 kept
      2 files, 0 errors, 0 warnings
      """
    And the file "tsconfig.json" is:
      """json
      {
        "extends": "./.ljos/tsconfig.base.json"
      }
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: A tsconfig.json that does not extend the base is a warning
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      {
        "compilerOptions": { "strict": true }
      }
      """
    When the files are checked
    Then the check reports:
      """
      tsconfig.json:1:1: warning: `tsconfig.json` does not extend `./.ljos/tsconfig.base.json`, so stock TypeScript cannot resolve an import of `width.lts` [tsconfig-lts]
      2 files, 0 errors, 1 warnings
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: Migrating adds the base as the tsconfig.json's first key
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      {
        // The project's own options.
        "compilerOptions": { "strict": true }
      }
      """
    When the files are migrated and checked
    Then the check reports:
      """
      extended tsconfig.json with ./.ljos/tsconfig.base.json
      1 files, 0 lowered, 0 kept
      2 files, 0 errors, 0 warnings
      """
    And the file "tsconfig.json" is:
      """json
      {
        "extends": "./.ljos/tsconfig.base.json",
        // The project's own options.
        "compilerOptions": { "strict": true }
      }
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: Migrating puts the base last after the tsconfig.json's own extends
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      {
        "extends": "@tsconfig/strictest/tsconfig.json"
      }
      """
    When the files are migrated and checked
    Then the check reports:
      """
      extended tsconfig.json with ./.ljos/tsconfig.base.json
      1 files, 0 lowered, 0 kept
      2 files, 0 errors, 0 warnings
      """
    And the file "tsconfig.json" is:
      """json
      {
        "extends": ["@tsconfig/strictest/tsconfig.json", "./.ljos/tsconfig.base.json"]
      }
      """

  # S7: the tsconfig that lets stock TypeScript resolve `.lts` imports.
  Scenario: Migrating appends the generated directory to the tsconfig.json's own rootDirs
    Given the file "width.lts":
      """ts
      export function width(thick: boolean): number {
        return match (thick) {
          when true: 3;
          when false: 1;
        }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.lts" as W
      Item {
        property real w: W.width(true)
      }
      """
    And the file "tsconfig.json":
      """json
      {
        "extends": "./.ljos/tsconfig.base.json",
        "compilerOptions": { "rootDirs": ["src", "gen"] }
      }
      """
    When the files are migrated and checked
    Then the check reports:
      """
      added "." and ".ljos/types" to the rootDirs of tsconfig.json
      1 files, 0 lowered, 0 kept
      2 files, 0 errors, 0 warnings
      """
    And the file "tsconfig.json" is:
      """json
      {
        "extends": "./.ljos/tsconfig.base.json",
        "compilerOptions": { "rootDirs": ["src", "gen", ".", ".ljos/types"] }
      }
      """

  # S7: no `.lts` module, no tsconfig to ask for.
  Scenario: A project with no `.lts` module is not told about its tsconfig.json
    Given the file "width.ts":
      """ts
      export function width(thick: boolean): number {
        return thick ? 3 : 1
      }
      """
    And the file "tsconfig.json":
      """json
      {
        "compilerOptions": { "strict": true }
      }
      """
    And the file "Line.qml":
      """qml
      import Ljos.Ui
      import "width.ts" as W
      Item {
        property real w: W.width(true)
      }
      """
    When the files are checked
    Then the check reports:
      """
      2 files, 0 errors, 0 warnings
      """
