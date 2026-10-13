# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-actiongroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ActionGroup
  Actions that belong together. In an exclusive group, which is what a
  group is unless told otherwise, checking one action unchecks the one
  that was checked; the group says which action of it was triggered, and
  disabling the group disables its actions.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property string log: ""
        property string current: g.checkedAction ? g.checkedAction.text : "none"
        property int count: g.actions.length
        property bool zeroIn: g.actions.indexOf(a0) === 0
        property bool fourIn: a4.ActionGroup.group === g; property bool fiveIn: a5.ActionGroup.group === g
        function addFive() { g.addAction(a5) }
        function removeFive() { g.removeAction(a5) }
        function chooseTwo() { g.checkedAction = a2 }
        function chooseNone() { g.checkedAction = null }
        function detachFour() { a4.ActionGroup.group = null }
        ActionGroup {
          id: g; objectName: "g"
          onTriggered: (action) => root.log += "g:" + action.text + " "
          Action { id: a0; text: "a0"; checkable: true }
        }
        Action { id: a1; objectName: "a1"; text: "a1"; checkable: true; ActionGroup.group: g; onTriggered: root.log += "a1 "; onToggled: root.log += "toggled " }
        Action { id: a2; objectName: "a2"; text: "a2"; checkable: true; ActionGroup.group: g }
        Action { id: a3; objectName: "a3"; text: "a3"; ActionGroup.group: g }
        Action { id: a4; objectName: "a4"; text: "a4"; checkable: true; ActionGroup.group: g }
        Action { id: a5; objectName: "a5"; text: "a5"; checkable: true }
      }
      """

  Scenario: A group is exclusive, enabled, and has the actions in it and those attached to it
    Then the property "exclusive" of "g" is true
    And the property "enabled" of "g" is true
    And the property "current" of "root" is "none"
    And the property "count" of "root" is 5
    And the property "zeroIn" of "root" is true
    And the property "fourIn" of "root" is true

  @api-QtQuick.Controls.ActionGroup.checkedAction
  Scenario: Triggering a checkable action checks it, and the group says it was triggered
    When the method "trigger" of "a1" is called
    Then the property "checked" of "a1" is true
    And the property "current" of "root" is "a1"
    And the property "log" of "root" is "toggled a1 g:a1 "

  Scenario: Checking another action unchecks the one before
    When the method "trigger" of "a1" is called
    And the method "trigger" of "a2" is called
    Then the property "checked" of "a1" is false
    And the property "checked" of "a2" is true
    And the property "current" of "root" is "a2"
    When the property "checked" of "a4" is set to true
    Then the property "checked" of "a2" is false
    And the property "current" of "root" is "a4"

  Scenario: Triggering the checked action again leaves it checked
    When the method "trigger" of "a1" is called
    And the property "log" of "root" is set to ""
    And the method "trigger" of "a1" is called
    Then the property "checked" of "a1" is true
    And the property "log" of "root" is "a1 g:a1 "

  Scenario: Unchecking the checked action does not change which action the group has
    When the method "trigger" of "a1" is called
    And the property "checked" of "a1" is set to false
    Then the property "checked" of "a1" is false
    And the property "current" of "root" is "a1"

  Scenario: An action that is not checkable is triggered through the group and stays unchecked
    When the method "trigger" of "a3" is called
    Then the property "checked" of "a3" is false
    And the property "current" of "root" is "none"
    And the property "log" of "root" is "g:a3 "

  @api-QtQuick.Controls.ActionGroup.checkedAction
  Scenario: Setting checkedAction checks that action and unchecks the others
    When the method "trigger" of "a1" is called
    And the method "chooseTwo" of "root" is called
    Then the property "checked" of "a2" is true
    And the property "checked" of "a1" is false
    When the method "chooseNone" of "root" is called
    Then the property "checked" of "a2" is false
    And the property "current" of "root" is "none"

  @api-QtQuick.Controls.ActionGroup.enabled
  Scenario: Disabling the group disables its actions
    When the property "enabled" of "g" is set to false
    Then the property "enabled" of "a1" is false
    And the property "enabled" of "a5" is true
    When the property "enabled" of "g" is set to true
    Then the property "enabled" of "a1" is true

  Scenario: An action disabled on its own stays disabled when the group is enabled again
    When the property "enabled" of "a1" is set to false
    And the property "enabled" of "g" is set to false
    And the property "enabled" of "g" is set to true
    Then the property "enabled" of "a1" is false
    And the property "enabled" of "a2" is true

  # An action's `enabled` reads as its own value and its group's: what is
  # assigned while the group is disabled is the action's own, and shows
  # once the group is enabled again.
  @api-QtQuick.Controls.ActionGroup.enabled
  Scenario: An action enabled while its group is disabled reads disabled until the group is enabled
    When the property "enabled" of "a1" is set to false
    And the property "enabled" of "g" is set to false
    And the property "enabled" of "a1" is set to true
    Then the property "enabled" of "a1" is false
    When the property "enabled" of "g" is set to true
    Then the property "enabled" of "a1" is true

  @api-QtQuick.Controls.ActionGroup.enabled
  Scenario: An action disabled while its group is disabled stays disabled
    When the property "enabled" of "g" is set to false
    And the property "enabled" of "a1" is set to false
    And the property "enabled" of "g" is set to true
    Then the property "enabled" of "a1" is false
    And the property "enabled" of "a2" is true

  @api-QtQuick.Controls.ActionGroup.enabled
  Scenario: An action that leaves a disabled group is enabled, and one that joins it is not
    When the property "enabled" of "g" is set to false
    And the method "detachFour" of "root" is called
    Then the property "enabled" of "a4" is true
    When the method "addFive" of "root" is called
    Then the property "enabled" of "a5" is false

  # Only the action's own `enabled` keeps a script from triggering it.
  @api-QtQuick.Controls.Action.trigger
  Scenario: A script can trigger an action of a disabled group
    When the property "enabled" of "g" is set to false
    And the method "trigger" of "a1" is called
    Then the property "checked" of "a1" is true
    And the property "current" of "root" is "a1"
    And the property "log" of "root" is "toggled a1 g:a1 "
    When the method "toggle" of "a1" is called
    Then the property "checked" of "a1" is false

  @api-QtQuick.Controls.Action.trigger
  Scenario: An action disabled on its own is not triggered, in a disabled group or not
    When the property "enabled" of "a1" is set to false
    And the method "trigger" of "a1" is called
    Then the property "checked" of "a1" is false
    And the property "log" of "root" is ""
    When the property "enabled" of "g" is set to false
    And the method "trigger" of "a1" is called
    And the method "toggle" of "a1" is called
    Then the property "checked" of "a1" is false
    And the property "log" of "root" is ""

  @api-QtQuick.Controls.ActionGroup.addAction
  Scenario: An action added to the group joins it, and one removed leaves it unchecked
    When the method "addFive" of "root" is called
    Then the property "count" of "root" is 6
    And the property "fiveIn" of "root" is false
    When the method "trigger" of "a1" is called
    And the method "trigger" of "a5" is called
    Then the property "checked" of "a5" is true
    And the property "checked" of "a1" is false
    And the property "current" of "root" is "a5"
    When the method "removeFive" of "root" is called
    Then the property "count" of "root" is 5
    And the property "checked" of "a5" is false
    And the property "current" of "root" is "none"

  @api-QtQuick.Controls.ActionGroup.group
  Scenario: An action leaves the group when its attached group is cleared
    When the method "detachFour" of "root" is called
    Then the property "count" of "root" is 4
    And the property "fourIn" of "root" is false
