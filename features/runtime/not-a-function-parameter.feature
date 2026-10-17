# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#method-attributes
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Calling a parameter that is not a function says what it holds
  A method that calls one of its parameters, given something that is not a
  function, throws a TypeError that shows the value as a string: `7 is not
  a function`, `null is not a function`. A parameter the method gives a new
  value before the call is not told by what it was given.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string run: ""
        onRunChanged: root[run]()
        function call(h) { h(1) }
        function second(a, h) { return h() }
        function changed(h) { h = 5; h() }
        function number() { call(7) }
        function nothing() { call(null) }
        function unset() { call() }
        function object() { call({ a: 1 }) }
        function text() { call("s") }
        function list() { call([1, 2]) }
        function last() { second(1, 2.5) }
        function changes() { changed(7) }
      }
      """

  Scenario Outline: A parameter given <given> is told as it reads
    When the property "run" of "root" is set to "<function>"
    Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

    Examples:
      | function | given    | line | message                            |
      | number   | 7        | 6    | 7 is not a function                |
      | nothing  | null     | 6    | null is not a function             |
      | unset    | nothing  | 6    | undefined is not a function        |
      | object   | object   | 6    | .object Object. is not a function   |
      | text     | a string | 6    | s is not a function                |
      | list     | an array | 6    | 1,2 is not a function              |
      | last     | 2.5      | 7    | 2.5 is not a function              |

  Scenario: A parameter given a new value is not told by the old one
    When the property "run" of "root" is set to "changes"
    Then the log has a warning matching "Scenario.qml:8: TypeError: "
    And the log has no warning matching "7 is not a function"
