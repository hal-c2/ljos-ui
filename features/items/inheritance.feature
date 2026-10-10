# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#opacity-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#childrenRect-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What children inherit from their parent
  visible and enabled pass down the tree; opacity applies to children but
  each item keeps its own value.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Item {
          objectName: "panel"
          Item { objectName: "child"; x: 10; y: 5; width: 20; height: 20 }
          Item { objectName: "own"; x: -4; y: 30; width: 8; height: 2; visible: false }
        }
      }
      """

  Scenario: Hiding a parent hides its children
    When the property "visible" of "panel" is set to false
    Then the property "visible" of "child" is false

  Scenario: Showing the parent again shows the children
    When the property "visible" of "panel" is set to false
    And the property "visible" of "panel" is set to true
    Then the property "visible" of "child" is true

  Scenario: A child hidden on its own stays hidden
    When the property "visible" of "panel" is set to false
    And the property "visible" of "panel" is set to true
    Then the property "visible" of "own" is false

  Scenario: Disabling a parent disables its children
    When the property "enabled" of "panel" is set to false
    Then the property "enabled" of "child" is false

  Scenario: A child keeps its own opacity value
    When the property "opacity" of "panel" is set to 0.5
    Then the property "opacity" of "child" is 1

  Scenario: childrenRect covers every child
    Then the property "childrenRect.x" of "panel" is -4
    And the property "childrenRect.y" of "panel" is 5
    And the property "childrenRect.width" of "panel" is 34
    And the property "childrenRect.height" of "panel" is 27
