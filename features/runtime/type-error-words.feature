# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html#objectName-prop
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TypeError says what the script tried to do
  Calling a method of null or undefined is not reading a property: the
  error logged says it could not call the method. Reading through nothing
  still says it could not read the property, and writing through it says
  that nothing is not an object. Calling a property that is not a function
  names the property and the object it was read from.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
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
        property var obj: ({ a: 1 })
        property Item kid: Item { objectName: "kid" }
        function callsMissing() { obj.spawn() }
        function callsNumber() { obj.a(1) }
        function callsItem() { kid.spawn() }
        function callsOwn() { width() }
        function callsDeep() { root.obj.a.b() }
        function callsSpaced() { kid . spawn ( 1 ) }
        function writes() { unset.x = 1 }
        function writesNull() { nothing.x = 1 }
      }
      """

  Rule: A call through nothing cannot call the method

    Scenario Outline: <function> is logged as a call
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function     | line | message                               |
        | calls        | 8    | Cannot call method 'foo' of null      |
        | callsUnset   | 9    | Cannot call method 'foo' of undefined |
        | callsLocal   | 10   | Cannot call method 'foo' of null      |
        | callsIndexed | 11   | Cannot call method 'foo' of null      |

  Rule: A read through nothing cannot read the property

    Scenario Outline: <function> is logged as a read
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function   | line | message                            |
        | reads      | 12   | Cannot read property 'foo' of null |
        | readsDeep  | 13   | Cannot read property 'a' of null   |
        | readsIndex | 14   | Cannot read property '0' of null   |

  Rule: A write through nothing says nothing is not an object

    Scenario Outline: <function> is logged as a write
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function   | line | message                                                     |
        | writes     | 23   | Value is undefined and could not be converted to an object |
        | writesNull | 24   | Value is null and could not be converted to an object      |

  Rule: A call of what is not a function names the property and its object

    Scenario Outline: <function> names the property and the object
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: Property '<property>' of object <object> is not a function"

      Examples:
        | function     | line | property | object                                          |
        | callsMissing | 17   | spawn    | .object Object.                                 |
        | callsNumber  | 18   | a        | .object Object.                                 |
        | callsItem    | 19   | spawn    | QQuickItem.0x[0-9a-f]+, .kid..                   |
        | callsOwn     | 20   | width    | Scenario_QMLTYPE_[0-9]+.0x[0-9a-f]+, .root..     |
        | callsDeep    | 21   | b        | 1                                               |
        | callsSpaced  | 22   | spawn    | QQuickItem.0x[0-9a-f]+, .kid..                   |
