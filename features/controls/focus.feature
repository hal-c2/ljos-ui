# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#focusPolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#visualFocus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Keyboard focus of the controls
  A button takes focus when it is clicked or tabbed to, shows that it has it
  when the keyboard gave it, and is pressed with Space.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Button {
          objectName: "first"
          property int clicks: 0
          onClicked: clicks++
        }
        CheckBox { objectName: "second"; y: 50 }
        Label { objectName: "text"; y: 100 }
      }
      """

  @api-QtQuick.Controls.Control.focusPolicy
  Scenario Outline: Buttons take focus by Tab and by click, labels do not
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property         | value |
      | first  | focusPolicy      | 11    |
      | second | focusPolicy      | 11    |
      | first  | activeFocusOnTab | true  |
      | first  | visualFocus      | false |
      | text   | activeFocusOnTab | false |

  Scenario: A click gives a button focus without showing it
    When the user touches 20, 20
    Then the property "activeFocus" of "first" is true
    And the property "visualFocus" of "first" is false

  @api-QtQuick.Controls.Control.visualFocus
  Scenario: Tab gives the next control focus and shows it
    When the user touches 20, 20
    And the user presses "Tab"
    Then the property "activeFocus" of "second" is true
    And the property "visualFocus" of "second" is true

  Scenario: Space clicks the button that has focus
    When the user presses "Tab"
    And the user presses "Space"
    Then the property "clicks" of "first" is 1

  Scenario: Space checks the check box that has focus
    When the user presses "Tab"
    And the user presses "Tab"
    And the user presses "Space"
    Then the property "checked" of "second" is true

  Scenario: The keyboard came for no reason a control knows until it comes
    Then the property "focusReason" of "first" is 7

  Scenario Outline: A control knows what brought the keyboard to it
    When the user <does>
    Then the property "activeFocus" of "<object>" is true
    And the property "focusReason" of "<object>" is <reason>

    Examples:
      | does                | object | reason |
      | touches 20, 20      | first  | 0      |
      | presses "Tab"       | first  | 1      |
      | presses "Shift+Tab" | second | 2      |

  Scenario Outline: <key> clicks the button that has focus
    When the user presses "Tab"
    And the user presses "<key>"
    Then the property "clicks" of "first" is <clicks>

    Examples:
      | key    | clicks |
      | Return | 1      |
      | Enter  | 1      |
      | A      | 0      |

  Scenario: A policy without Tab takes the control out of the Tab order
    When the property "focusPolicy" of "first" is set to 2
    Then the property "activeFocusOnTab" of "first" is false

  Scenario: Tab passes by a control whose policy has no Tab
    When the property "focusPolicy" of "first" is set to 2
    And the user presses "Tab"
    Then the property "activeFocus" of "second" is true

  Scenario: A click leaves the keyboard where it was when the policy has no click
    When the property "focusPolicy" of "first" is set to 1
    And the user touches 20, 20
    Then the property "activeFocus" of "first" is false
    And the property "clicks" of "first" is 1

