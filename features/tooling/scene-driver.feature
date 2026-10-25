# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#hovered-prop
# The scenario runner, the QtTest runner, the MCP inspector and an
# application's own tests drive a scene through one driver (crates/
# ljos_drive). These scenarios hold the scenario runner to its rules; the
# inspector's side of the same rules is in features/tooling/mcp/input.feature.

@ljos-only @a11y
Feature: Input aimed at a control goes where a hand could put it
  A step that clicks a control by its role and name puts the pointer on the
  control's middle first and clicks there, as a hand would. A control a
  person could not click is not clicked: one that is disabled, or inside
  something disabled, takes no input, and what is under it gets none
  either.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        id: root; objectName: "root"
        width: 300; height: 200
        property int under: 0
        property int saved: 0
        MouseArea { anchors.fill: parent; onClicked: root.under++ }
        Column {
          Button { objectName: "save"; text: "Save"; onClicked: root.saved++ }
          Button { objectName: "locked"; text: "Locked"; enabled: false; onClicked: root.saved++ }
          Item {
            width: 100; height: 40; enabled: false
            Button { text: "Inside"; onClicked: root.saved++ }
          }
        }
      }
      """

  Scenario: A click goes to the control's middle, with the pointer moved there
    When the user clicks the "button" "Save"
    Then the property "saved" of "root" is 1
    And the property "hovered" of "save" is true
    And the property "under" of "root" is 0

  Scenario: A disabled control is not clicked, nor what is under it
    When the user clicks the "button" "Locked"
    Then the property "saved" of "root" is 0
    And the property "under" of "root" is 0

  Scenario: A control inside something disabled is not clicked
    When the user clicks the "button" "Inside"
    Then the property "saved" of "root" is 0
    And the property "under" of "root" is 0
