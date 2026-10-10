# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Component.createObject
  A Component makes new objects on request, with initial property values.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int base: 5
        property int made: children.length
        property int ready: comp.status === Component.Ready ? 1 : 0
        Component {
          id: comp
          Item { property int n: 0; width: n + root.base }
        }
        function make(name, n) { comp.createObject(root, { objectName: name, n: n }) }
      }
      """

  Scenario: A Component is ready once loaded
    Then the property "ready" of "root" is 1

  Scenario: createObject makes an object with the given properties
    When the method "make" of "root" is called with "kid", 3
    Then the object "root/kid" exists
    And the property "n" of "root/kid" is 3
    And the property "made" of "root" is 1

  Scenario: The new object's bindings read the context it was declared in
    When the method "make" of "root" is called with "kid", 3
    And the property "base" of "root" is set to 10
    Then the property "width" of "root/kid" is 13

  Scenario: Nothing is made until createObject is called
    Then the object "root/kid" does not exist
    And the property "made" of "root" is 0
