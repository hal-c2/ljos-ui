# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#status-prop
#   docs/ARCHITECTURE.md §6.3 (time-sliced creation)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Component.incubateObject
  `incubateObject(parent, properties, mode)` returns an incubator: an
  object with the `status` of the creation, the `object` once it is made
  and `forceCompletion()`, which finishes the creation before it returns.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var inc: null
        property int status: -1
        property int n: -1
        property bool mine: false
        property bool empty: false
        Component { id: comp; Item { objectName: "kid"; property int n: 1 } }
        Component { id: needy; Item { objectName: "needy"; required property int n } }
        function look() {
          status = inc.status; empty = inc.object === null
          if (inc.object) { n = inc.object.n; mine = inc.object.parent === root }
        }
        function start(mode) { inc = comp.incubateObject(root, { n: 5 }, mode) }
        function force() { start(Qt.Asynchronous); inc.forceCompletion(); look() }
        function now() { start(Qt.Synchronous); look() }
        function orphan() { inc = comp.incubateObject(null); inc.forceCompletion(); look() }
        function fail() { inc = needy.incubateObject(root); inc.forceCompletion(); look() }
        function twice() { force(); inc.forceCompletion(); look() }
      }
      """

  Scenario: Nothing is made until incubateObject is called
    Then the object "kid" does not exist

  Scenario: forceCompletion makes the object before it returns
    When the method "force" of "root" is called
    Then the property "status" of "root" is 1
    And the property "empty" of "root" is false
    And the object "root/kid" exists

  Scenario: The object has the given parent and properties
    When the method "force" of "root" is called
    Then the property "n" of "root" is 5
    And the property "mine" of "root" is true

  Scenario: A synchronous incubation is ready at once
    When the method "now" of "root" is called
    Then the property "status" of "root" is 1
    And the property "n" of "root" is 5

  Scenario: An incubation left alone becomes ready
    When the method "start" of "root" is called with 0
    And the method "look" of "root" is called
    Then the property "status" of "root" is 1
    And the property "n" of "root" is 5

  Scenario: An object can be made without a parent
    When the method "orphan" of "root" is called
    Then the property "status" of "root" is 1
    And the property "mine" of "root" is false

  Scenario: forceCompletion on a finished incubation changes nothing
    When the method "twice" of "root" is called
    Then the property "status" of "root" is 1
    And the property "n" of "root" is 5

  Scenario: A component that cannot be made is an error with no object
    When the method "fail" of "root" is called
    Then the property "status" of "root" is 3
    And the property "empty" of "root" is true
    And the object "needy" does not exist
