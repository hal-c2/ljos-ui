# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#focusPolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#visualFocus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#focusReason-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: When a click brings the keyboard, and which controls show that they have it
  A control whose policy has Qt.ClickFocus takes the keyboard as the
  pointer goes down on it, before the click is over. Only the keyboard
  makes a control show where it is.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Column {
        width: 300; height: 400
        Button { objectName: "button"; height: 40 }
        CheckBox { objectName: "check"; height: 40 }
        ComboBox { objectName: "combo"; height: 40; model: ["One", "Two"] }
        SpinBox { objectName: "spin"; height: 40 }
        TextField { objectName: "field"; height: 40 }
        Switch { objectName: "switch"; height: 40 }
        RadioButton { objectName: "radio"; height: 40 }
      }
      """

  Scenario Outline: The <control> has the keyboard while the pointer is still down on it
    When the user presses the pointer at <x>, <y>
    Then the property "activeFocus" of "<control>" is true

    Examples:
      | control | x  | y   |
      | button  | 20 | 20  |
      | check   | 20 | 60  |
      | combo   | 20 | 100 |
      | spin    | 70 | 140 |
      | switch  | 20 | 220 |
      | radio   | 20 | 260 |

  Scenario: A text field waits for the pointer to come up again
    When the user presses the pointer at 20, 180
    Then the property "activeFocus" of "field" is false

  Scenario: A click on a text field brings the keyboard
    When the user touches 20, 180
    Then the property "activeFocus" of "field" is true

  Scenario: A press on a button of a spin box leaves the keyboard where it is
    When the user touches 10, 140
    Then the property "activeFocus" of "spin" is false
    And the property "focusReason" of "spin" is 7

  @api-QtQuick.Controls.Control.focusReason
  Scenario Outline: A click on the <control> is why it has the keyboard and it does not show it
    When the user touches <x>, <y>
    Then the property "activeFocus" of "<control>" is true
    And the property "visualFocus" of "<control>" is false
    And the property "focusReason" of "<control>" is 0

    Examples:
      | control | x  | y   |
      | combo   | 20 | 100 |
      | spin    | 70 | 140 |
      | switch  | 20 | 220 |
      | radio   | 20 | 260 |

  @api-QtQuick.Controls.Control.visualFocus
  Scenario: Tab shows where the keyboard is on a combo box
    When the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    Then the property "activeFocus" of "combo" is true
    And the property "visualFocus" of "combo" is true
    And the property "focusReason" of "combo" is 1

  @api-QtQuick.Controls.Control.visualFocus
  Scenario: Tab shows where the keyboard is on a spin box, whose editor has it
    When the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    Then the property "activeFocus" of "spin" is true
    And the property "visualFocus" of "spin" is true
    And the property "focusReason" of "spin" is 1

  Scenario: The keyboard going on takes the showing with it
    When the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Tab"
    Then the property "visualFocus" of "spin" is false
    And the property "activeFocus" of "field" is true
    And the property "focusReason" of "field" is 1
