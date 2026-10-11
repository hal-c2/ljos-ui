# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The active window
  One window has the keyboard at a time. Each window remembers its own
  focus item, and only the active window's focus item has active focus.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        Window {
          id: a
          objectName: "a"
          width: 200; height: 100
          visible: true
          transientParent: null
          readonly property bool keysGoToInput: activeFocusItem === inA
          readonly property bool noFocusItem: activeFocusItem === null
          TextInput { id: inA; objectName: "inA"; focus: true }
        }
        Window {
          objectName: "b"
          width: 200; height: 100
          visible: true
          transientParent: null
          readonly property bool noFocusItem: activeFocusItem === null
          TextInput { objectName: "inB"; focus: true }
        }
      }
      """
    When the method "requestActivate" of "a" is called
    And 50 milliseconds pass

  @api-QtQuick.Window.requestActivate @api-QtQuick.Window.active
  Scenario: The window that asked is the active one
    Then the property "active" of "a" is true
    And the property "active" of "b" is false

  @api-QtQuick.Item.activeFocus
  Scenario: Only the active window's focus item has active focus
    Then the property "activeFocus" of "inA" is true
    And the property "activeFocus" of "inB" is false

  @api-QtQuick.Item.focus
  Scenario: The other window keeps its focus item
    Then the property "focus" of "inB" is true

  @api-QtQuick.Window.activeFocusItem
  Scenario: The active window names its focus item and the other names none
    Then the property "keysGoToInput" of "a" is true
    And the property "noFocusItem" of "b" is true

  @api-QtQuick.Window.requestActivate
  Scenario: Activating the other window moves the keyboard to its focus item
    When the method "requestActivate" of "b" is called
    And 50 milliseconds pass
    Then the property "active" of "b" is true
    And the property "active" of "a" is false
    And the property "activeFocus" of "inB" is true
    And the property "activeFocus" of "inA" is false
    And the property "noFocusItem" of "a" is true
