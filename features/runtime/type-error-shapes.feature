# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html#deleting-objects-dynamically
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TypeError names what a chain, a call and a deleted object were
  A method called on what a chain of properties ends in, when that is null
  or undefined, cannot be called. A property called that is not a function
  is named with the object it was read from, whether it holds a value,
  nothing or is not there. An object that was destroyed reads as null, so
  reading, calling or writing a member of it says so.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property var holder: ({ inner: null, deep: { inner: undefined } })
        property QtObject obj: null
        property var made
        property int count: 2
        Component { id: comp; Item { objectName: "gone"; property int v: 1; function f() { return 1 } } }
        function callsNullMember() { holder.inner.go() }
        function callsUndefinedMember() { holder.missing.go(1) }
        function callsDeepMember() { root.holder.deep.inner.go() }
        function callsNullObject() { root.obj.go() }
        function callsString() { root.objectName() }
        function callsInt() { count() }
        function callsMissing() { root.nothing() }
        function callsNullValue() { holder.inner() }
        function readsGone() { return made.v }
        function readsGoneName() { return made.objectName }
        function callsGone() { made.f() }
        function writesGone() { made.v = 3 }
        property string seen: ""; property string run: ""
        function looksAtGone() { seen = typeof made + " " + (made === null) }
        onRunChanged: root[run]()
        Component.onCompleted: { made = comp.createObject(root); made.destroy() }
      }
      """

  Rule: A method of what a chain ends in, when that is nothing, cannot be called

    Scenario Outline: <function> cannot call the method
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function             | line | message                               |
        | callsNullMember      | 9    | Cannot call method 'go' of null       |
        | callsUndefinedMember | 10   | Cannot call method 'go' of undefined  |
        | callsDeepMember      | 11   | Cannot call method 'go' of undefined  |
        | callsNullObject      | 12   | Cannot call method 'go' of null       |

  Rule: A property called that is not a function is named with its object

    Scenario Outline: <function> names the property and the object
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: Property '<property>' of object <object> is not a function"

      Examples:
        | function       | line | property   | object                                       |
        | callsString    | 13   | objectName | Scenario_QMLTYPE_[0-9]+.0x[0-9a-f]+, .root.. |
        | callsInt       | 14   | count      | Scenario_QMLTYPE_[0-9]+.0x[0-9a-f]+, .root.. |
        | callsMissing   | 15   | nothing    | Scenario_QMLTYPE_[0-9]+.0x[0-9a-f]+, .root.. |
        | callsNullValue | 16   | inner      | .object Object.                              |

  Rule: A destroyed object reads as null

    Scenario: A destroyed object is null to a script
      When the property "run" of "root" is set to "looksAtGone"
      Then the property "seen" of "root" is "object true"

    Scenario Outline: <function> is told the object is null
      When the property "run" of "root" is set to "<function>"
      Then the log has a warning matching "Scenario.qml:<line>: TypeError: <message>"

      Examples:
        | function      | line | message                                               |
        | readsGone     | 17   | Cannot read property 'v' of null                      |
        | readsGoneName | 18   | Cannot read property 'objectName' of null             |
        | callsGone     | 19   | Cannot call method 'f' of null                        |
        | writesGone    | 20   | Value is null and could not be converted to an object |
