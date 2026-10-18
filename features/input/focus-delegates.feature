# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Focus asked for by what a Repeater makes
  A Repeater makes its items when it completes, after the items written
  around it have asked for focus. The items it makes sit before it among
  their siblings, but an item written after the Repeater that asked for
  focus keeps it.

  Scenario: An item after a Repeater that asks for focus keeps it from the delegates
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        Repeater { model: 1; Rectangle { objectName: "made"; focus: true } }
        Rectangle { objectName: "after"; focus: true }
      }
      """
    When 50 milliseconds pass
    Then the property "active" of "root" is "after"
    And the property "focus" of "made" is false

  Scenario: An item before a Repeater that asks for focus keeps it from the delegates
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        Rectangle { objectName: "before"; focus: true }
        Repeater { model: 1; Rectangle { objectName: "made"; focus: true } }
      }
      """
    When 50 milliseconds pass
    Then the property "active" of "root" is "before"
    And the property "focus" of "made" is false

  Scenario: Among the delegates the first that asks for focus has it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        Repeater {
          model: 2
          Rectangle { required property int index; objectName: "made" + index; focus: true }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "active" of "root" is "made0"
    And the property "focus" of "made1" is false
