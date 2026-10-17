# Sources:
#   docs/ARCHITECTURE.md section 19.3
# `ljos check` is Ljós's own program: Qt has nothing to compare it with.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Each thing `ljos check` finds names its kind
  Every line `ljos check` writes about a file ends with the kind of what
  it found, in brackets: a word that stays the same when the message is
  reworded, so a program reading the lines tells kinds apart by it.

  Scenario: What the compiler finds names its kind
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        Text { horizontalAlignment: "left" }
        height: "tall"
        property real r: nowhere
      }
      """
    When the files are checked
    Then the check reports:
      """
      Main.qml:3:37: error: Invalid property assignment: unknown enumeration [invalid-assignment]
      Main.qml:4:17: error: Invalid property assignment: number expected [invalid-assignment]
      Main.qml:5:26: warning: `nowhere` is not a name in sight: it is looked up when the program runs [unresolved-name]
      1 files, 2 errors, 1 warnings
      ljos: 2 errors
      """

  # How a type nobody has is worded depends on whether the build knows Qt's
  # types; its kind does not.
  @qt-table
  Scenario: A type nobody has names its kind
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { NoSuchThing { } }
      """
    When the files are checked
    Then the check reports:
      """
      Main.qml:2:14: error: `NoSuchThing` is not a type of anything the file imports [unknown-type]
      1 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  @no-qt-table
  Scenario: A type nobody has names its kind without Qt's table
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { NoSuchThing { } }
      """
    When the files are checked
    Then the check reports:
      """
      Main.qml:2:14: error: `NoSuchThing` is not a type of anything the file imports (this build knows Ljós's own types only: with a table of Qt's types it could tell whether Qt has it) [unknown-type]
      1 files, 1 errors, 0 warnings
      ljos: 1 errors
      """
