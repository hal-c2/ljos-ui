# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html#objectName-prop
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TypeError says what the script tried to do
  Calling a method of null or undefined is not reading a property: the
  error logged says it could not call the method. Reading through nothing
  still says it could not read the property.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var nothing: null
        property var unset
        property string run: ""
        onRunChanged: root[run]()
        function calls() { nothing.foo() }
        function callsUnset() { unset.foo() }
        function callsLocal() { var x = null; x.foo(1, 2) }
        function callsIndexed() { nothing["foo"]() }
        function reads() { return nothing.foo }
        function readsDeep() { return nothing.a.b }
        function readsIndex() { return nothing[0] }
      }
      """

  Rule: A call through nothing cannot call the method

    Scenario Outline: <function> is logged as a call
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function     | line | message                               |
        | calls        | 9    | Cannot call method 'foo' of null      |
        | callsUnset   | 10   | Cannot call method 'foo' of undefined |
        | callsLocal   | 11   | Cannot call method 'foo' of null      |
        | callsIndexed | 12   | Cannot call method 'foo' of null      |

  Rule: A read through nothing cannot read the property

    Scenario Outline: <function> is logged as a read
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function   | line | message                            |
        | reads      | 13   | Cannot read property 'foo' of null |
        | readsDeep  | 14   | Cannot read property 'a' of null   |
        | readsIndex | 15   | Cannot read property '0' of null   |
