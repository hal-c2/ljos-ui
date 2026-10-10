# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Action
  An action is something the user can do. A button given an action shows its
  text, follows its state and triggers it when clicked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Action {
          id: save
          objectName: "save"
          text: "Save"
          property string log: ""
          onTriggered: log += "triggered "
          onToggled: log += "toggled "
        }
        Button {
          objectName: "b"
          action: save
          onClicked: save.log += "clicked "
        }
        Button { objectName: "plain"; y: 50; text: "Plain" }
      }
      """

  Scenario Outline: An action starts out enabled and not checkable
    Then the property "<property>" of "save" is <value>

    Examples:
      | property  | value  |
      | text      | "Save" |
      | enabled   | true   |
      | checkable | false  |
      | checked   | false  |

  @api-QtQuick.Controls.AbstractButton.action
  Scenario: The button shows the text of its action
    Then the property "text" of "b" is "Save"
    When the property "text" of "save" is set to "Save all"
    Then the property "text" of "b" is "Save all"

  Scenario: A text of its own wins over the text of the action
    When the property "text" of "b" is set to "Mine"
    And the property "text" of "save" is set to "Other"
    Then the property "text" of "b" is "Mine"

  Scenario: The button is disabled with its action
    When the property "enabled" of "save" is set to false
    Then the property "enabled" of "b" is false

  Scenario: The button is checkable with its action
    When the property "checkable" of "save" is set to true
    Then the property "checkable" of "b" is true

  Scenario: The button is checked with its action
    When the property "checkable" of "save" is set to true
    And the property "checked" of "save" is set to true
    Then the property "checked" of "b" is true

  @api-QtQuick.Controls.Action.triggered
  Scenario: A click triggers the action before the button says it was clicked
    When the user touches 50, 20
    Then the property "log" of "save" is "triggered clicked "

  @api-QtQuick.Controls.Action.toggled
  Scenario: A click checks a checkable action
    When the property "checkable" of "save" is set to true
    And the user touches 50, 20
    Then the property "checked" of "save" is true
    And the property "checked" of "b" is true
    And the property "log" of "save" is "toggled triggered clicked "

  Scenario: A disabled action is not triggered by a click
    When the property "enabled" of "save" is set to false
    And the user touches 50, 20
    Then the property "log" of "save" is ""

  @api-QtQuick.Controls.Action.trigger
  Scenario: trigger() triggers without a click, and the buttons of the action say they were clicked
    When the method "trigger" of "save" is called
    Then the property "log" of "save" is "triggered clicked "

  Scenario: trigger() does nothing on a disabled action
    When the property "enabled" of "save" is set to false
    And the method "trigger" of "save" is called
    Then the property "log" of "save" is ""

  @api-QtQuick.Controls.Action.toggle
  Scenario: toggle() checks a checkable action without triggering it
    When the property "checkable" of "save" is set to true
    And the method "toggle" of "save" is called
    Then the property "checked" of "save" is true
    And the property "log" of "save" is "toggled "

  Scenario: toggle() does nothing on an action that is not checkable
    When the method "toggle" of "save" is called
    Then the property "checked" of "save" is false

  Scenario: A button without an action has none
    Then the property "action" of "plain" is null
