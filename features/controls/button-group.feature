# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-buttongroup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ButtonGroup
  A button group keeps one of its buttons checked, wherever the buttons are.
  When it is not exclusive it tells whether none, some or all are checked.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        ButtonGroup {
          id: group
          objectName: "group"
          property string log: ""
          onClicked: button => log += button.objectName + " "
          function drop() { removeButton(c) }
          function take() { addButton(out) }
        }
        CheckBox { objectName: "a"; ButtonGroup.group: group }
        Item {
          y: 50; width: 100; height: 50
          CheckBox { objectName: "b"; ButtonGroup.group: group }
        }
        CheckBox { id: c; objectName: "c"; y: 100; ButtonGroup.group: group }
        CheckBox { id: out; objectName: "out"; y: 150 }
      }
      """

  @api-QtQuick.Controls.ButtonGroup.exclusive @api-QtQuick.Controls.ButtonGroup.checkedButton
  Scenario Outline: A group starts out exclusive with nothing checked
    Then the property "<property>" of "group" is <value>

    Examples:
      | property       | value |
      | exclusive      | true  |
      | checkedButton  | null  |
      | buttons.length | 3     |
      | checkState     | 0     |

  Scenario: A click checks one button
    When the user touches 20, 20
    Then the property "checked" of "a" is true
    And the property "checkedButton.objectName" of "group" is "a"

  Scenario: A click on another button of the group moves the check, across parents
    When the user touches 20, 20
    And 1 second passes
    And the user touches 20, 70
    Then the property "checked" of "b" is true
    And the property "checked" of "a" is false
    And the property "checkedButton.objectName" of "group" is "b"

  Scenario: A click on the checked button of an exclusive group leaves it checked
    When the user touches 20, 20
    And 1 second passes
    And the user touches 20, 20
    Then the property "checked" of "a" is true

  Scenario: A button outside the group is left alone
    When the property "checked" of "out" is set to true
    And the user touches 20, 20
    Then the property "checked" of "out" is true

  @api-QtQuick.Controls.ButtonGroup.clicked
  Scenario: The group says which of its buttons was clicked
    When the user touches 20, 20
    And 1 second passes
    And the user touches 20, 120
    Then the property "log" of "group" is "a c "

  Scenario: Checking a button from a script unchecks the other
    When the property "checked" of "a" is set to true
    And the property "checked" of "c" is set to true
    Then the property "checked" of "a" is false
    And the property "checkedButton.objectName" of "group" is "c"

  Scenario: A script can uncheck the checked button
    When the property "checked" of "a" is set to true
    And the property "checked" of "a" is set to false
    Then the property "checked" of "a" is false
    And the property "checkState" of "group" is 0

  Scenario: Clearing the checked button of the group unchecks it
    When the property "checked" of "a" is set to true
    And the property "checkedButton" of "group" is set to null
    Then the property "checked" of "a" is false

  @api-QtQuick.Controls.ButtonGroup.removeButton
  Scenario: A button taken out of the group is free again
    When the property "checked" of "a" is set to true
    And the method "drop" of "group" is called
    And the property "checked" of "c" is set to true
    Then the property "checked" of "a" is true
    And the property "buttons.length" of "group" is 2

  Scenario: The checked button taken out of the group is unchecked
    When the property "checked" of "c" is set to true
    And the method "drop" of "group" is called
    Then the property "checked" of "c" is false
    And the property "checkedButton" of "group" is null

  @api-QtQuick.Controls.ButtonGroup.addButton
  Scenario: A checked button put into the group becomes its checked button
    When the property "checked" of "out" is set to true
    And the method "take" of "group" is called
    Then the property "checkedButton.objectName" of "group" is "out"
    And the property "buttons.length" of "group" is 4

  @api-QtQuick.Controls.ButtonGroup.checkState
  Rule: A group that is not exclusive tells how many are checked

    Scenario: Several buttons can be checked
      When the property "exclusive" of "group" is set to false
      And the property "checked" of "a" is set to true
      And the property "checked" of "b" is set to true
      Then the property "checked" of "a" is true
      And the property "checkState" of "group" is 1

    Scenario: With all buttons checked the group is checked
      When the property "exclusive" of "group" is set to false
      And the property "checked" of "a" is set to true
      And the property "checked" of "b" is set to true
      And the property "checked" of "c" is set to true
      Then the property "checkState" of "group" is 2

    Scenario: Checking the group checks every button
      When the property "exclusive" of "group" is set to false
      And the property "checkState" of "group" is set to 2
      Then the property "checked" of "a" is true
      And the property "checked" of "b" is true
      And the property "checked" of "c" is true

    Scenario: Unchecking the group unchecks every button
      When the property "exclusive" of "group" is set to false
      And the property "checked" of "a" is set to true
      And the property "checkState" of "group" is set to 0
      Then the property "checked" of "a" is false

    Scenario: A click on a checked button unchecks it
      When the property "exclusive" of "group" is set to false
      And the user touches 20, 20
      And 1 second passes
      And the user touches 20, 20
      Then the property "checked" of "a" is false
      And the property "checkState" of "group" is 0
