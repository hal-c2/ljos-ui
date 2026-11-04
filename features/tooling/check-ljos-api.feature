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
  is not held to them.

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
