# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#children-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#data-prop
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html#deleting-objects-dynamically
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A destroyed object leaves the lists of its parent
  children, resources and data list what an item holds. An object that is destroyed is in
  none of them afterwards.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen: ""
        property list<QtObject> picked: [first, timer]
        property var made: null
        Item { id: first }
        Item { id: second }
        Timer { id: timer }
        Component { id: comp; Item {} }
        function look() { seen = children.length + " " + resources.length + " " + data.length }
        function lookPicked() { seen = picked.length + " " + (picked[0] === null) }
        function make() { made = comp.createObject(root) }
        function dropItem() { first.destroy() }
        function dropTimer() { timer.destroy() }
        function dropMade() { made.destroy() }
      }
      """

  Scenario: The lists hold what is declared
    When the method "look" of "root" is called
    Then the property "seen" of "root" is "2 2 4"

  Scenario: A destroyed item leaves children and data
    When the method "dropItem" of "root" is called
    And 10 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "1 2 3"

  Scenario: A destroyed object that is not an item leaves resources and data
    When the method "dropTimer" of "root" is called
    And 10 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "2 1 3"

  Scenario: An item made from a component joins children and data
    When the method "make" of "root" is called
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "3 2 5"

  Scenario: An item made from a component leaves them when destroyed
    When the method "make" of "root" is called
    And the method "dropMade" of "root" is called
    And 10 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "2 2 4"

  Scenario: A list property of the author's keeps its length and reads null
    When the method "dropItem" of "root" is called
    And 10 milliseconds pass
    And the method "lookPicked" of "root" is called
    Then the property "seen" of "root" is "2 true"
