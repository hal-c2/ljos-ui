# Sources:
#   docs/ARCHITECTURE.md section 19.3
#   docs/DIALECT.md (§4.4: TypeScript modules; §4.21: compiled away; §5.1: ts-lts-syntax, lts-export-type)
#   https://github.com/tc39/proposal-pattern-matching
# The authoring tools and `.lts` modules are Ljós's own: Qt has nothing to compare them with.
# A file given here is written without the spaces its doc string is indented by, so the
# first column of a line is 1.

@ljos-only @headless
Feature: The authoring tools read `.ts` and `.lts` modules as `ljos check` does
  An agent that changes a script module asks `qml_check` about it as it
  asks about a QML file. The module is read with Ljós's syntax compiled
  away, and each finding is where it is in the file the agent wrote. A
  `.ts` module that uses Ljós's syntax is renamed to `.lts` by
  `module_rename_lts`, with the files that import it, as `ljos migrate`
  renames it.

  Background:
    Given the authoring tools are connected with the capabilities "fs"

  Rule: A module is checked where it is written

    Scenario: The new syntax in a `.ts` module is reported at its place in the file
      Given the file "size.ts":
        """ts
        export function margin(big: boolean): number {
          return match (big) {
            when true: 12;
            when false: 4;
          }
        }
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["size.ts"]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/file" is "size.ts"
      And the answer's "/diagnostics/0/line" is 2
      And the answer's "/diagnostics/0/column" is 10
      And the answer's "/diagnostics/0/severity" is "error"
      And the answer's "/diagnostics/0/code" is "ts-lts-syntax"

    Scenario: An `.lts` module's finding after a `match` is at its place in the file
      Given the file "width.lts":
        """ts
        export function width(thick: boolean): number {
          return match (thick) {
            when true: 3;
            when false: 1;
          }
        }
        export const gap = [1].length
        """
      When the agent calls the tool "qml_lint" with:
        """json
        {"paths": ["width.lts"]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/file" is "width.lts"
      And the answer's "/diagnostics/0/line" is 7
      And the answer's "/diagnostics/0/column" is 14
      And the answer's "/diagnostics/0/code" is "lts-export-type"

    Scenario: A directory's `.lts` modules are checked with its QML files
      Given the file "width.lts":
        """ts
        export const gap = [1].length
        """
      And the file "Line.qml":
        """qml
        import Ljos.Ui
        Item {}
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["."]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/file" is "width.lts"
      And the answer's "/diagnostics/0/code" is "lts-export-type"

    Scenario: A module named in a directory a walk of the project skips is still checked
      Given the file "build/size.lts":
        """ts
        export const gap = [1].length
        """
      When the agent calls the tool "qml_check" with:
        """json
        {"paths": ["build/size.lts"]}
        """
      Then the answer's "/diagnostics" has 1 entries
      And the answer's "/diagnostics/0/file" is "build/size.lts"
      And the answer's "/diagnostics/0/code" is "lts-export-type"

  Rule: The rename `ts-lts-syntax` names is the agent's to make

    Scenario: Renaming a `.ts` module renames the imports that name it
      Given the file "size.ts":
        """ts
        export function margin(big: boolean): number {
          return match (big) {
            when true: 12;
            when false: 4;
          }
        }
        """
      And the file "Pad.qml":
        """qml
        import Ljos.Ui
        import "size.ts" as Size
        Item {
            property real gap: Size.margin(true)
        }
        """
      When the agent calls the tool "module_rename_lts" with:
        """json
        {"path": "size.ts", "write": true}
        """
      Then the answer's "/to" is "size.lts"
      And the answer's "/importers" has 1 entries
      And the answer's "/importers/0" is "Pad.qml"
      And the answer's "/written" is true
      And the file "Pad.qml" is:
        """qml
        import Ljos.Ui
        import "size.lts" as Size
        Item {
            property real gap: Size.margin(true)
        }
        """

    Scenario: Without `write` the rename is only said
      Given the file "size.ts":
        """ts
        export function margin(big: boolean): number {
          return match (big) {
            when true: 12;
            when false: 4;
          }
        }
        """
      When the agent calls the tool "module_rename_lts" with:
        """json
        {"path": "size.ts"}
        """
      Then the answer's "/to" is "size.lts"
      And the answer's "/written" is false
      And the file "size.ts" is:
        """ts
        export function margin(big: boolean): number {
          return match (big) {
            when true: 12;
            when false: 4;
          }
        }
        """

    Scenario: A module without Ljós's syntax is not renamed
      Given the file "size.ts":
        """ts
        export const big: number = 12
        """
      When the agent calls the tool "module_rename_lts" with:
        """json
        {"path": "size.ts", "write": true}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "no Ljós syntax"
