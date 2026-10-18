# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TypeError through a local says what the local holds
  A function that keeps an object in a local and calls a member of it
  that is not a function is told as a call through the path the local
  was given: the error names the property and the object. A local called
  that is not a function says what it holds.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property var obj: ({ a: 1 })
        property Item kid: Item { objectName: "kid" }
        property string run: ""
        onRunChanged: root[run]()
        function callsThroughLocal() { var k = kid; k.spawn() }
        function callsLocalValue() { var f = obj.a; f() }
        function callsThroughLocalPath() { const o = root.obj; o.a() }
      }
      """

  Scenario Outline: <function> names the property and the object a local holds
    When the property "run" of "root" is set to "<function>"
    Then the log has a warning matching "Scenario.qml:<line>: TypeError: Property '<property>' of object <object> is not a function"

    Examples:
      | function              | line | property | object                          |
      | callsThroughLocal     | 8    | spawn    | QQuickItem.0x[0-9a-f]+, .kid..   |
      | callsThroughLocalPath | 10   | a        | .object Object.                 |

  Scenario: A local called that is not a function says what it holds
    When the property "run" of "root" is set to "callsLocalValue"
    Then the log has a warning matching "Scenario.qml:9: TypeError: 1 is not a function"
