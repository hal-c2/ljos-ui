# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-imports.html
#   https://doc.qt.io/qt-6/qtqml-javascript-resources.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Errors thrown in an imported JavaScript file
  An error a script file's function throws is told at its line in that
  file, not at the QML that called it.

  Background:
    Given the file "lib.js":
      """
      .pragma library

      function boom() {
        throw new Error("boom")
      }
      function call(o) { o.nope() }
      """
    And the file "own.js":
      """
      function boom() { throw new Error("own") }
      """

  Scenario Outline: <what> is told at its line in the file
    Given the QML:
      """qml
      import QtQuick
      import "lib.js" as Lib
      import "own.js" as Own
      Item {
        Component.onCompleted: <call>
      }
      """
    Then the log has a warning matching "^.*/<file>:<line>: <said>"

    Examples:
      | what                           | call          | file   | line | said         |
      | An error a library throws      | Lib.boom()    | lib.js | 5    | Error: boom  |
      | A call of what is no function  | Lib.call({})  | lib.js | 7    | TypeError: . |
      | An error a script throws       | Own.boom()    | own.js | 2    | Error: own   |
