# Sources:
#   docs/ARCHITECTURE.md sections 5.4, 19.4 and 19.9
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Inspecting one item
  An agent asks about one item and is told what it is, where it was written
  and, for each of its properties, where the value it has now came from.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: page
        objectName: "page"
        width: 300; height: 200
        property int margin: 10
        Rectangle {
          id: card
          objectName: "card"
          x: page.margin
          width: parent.width / 2
          height: 40
          color: "red"
          Accessible.role: Accessible.Button
          Accessible.name: "Card"
          NumberAnimation on opacity { from: 0.5; to: 1; duration: 1000 }
        }
        Item { objectName: "field"; focus: true }
        Item { objectName: "other" }
      }
      """
    And the inspector is connected

  Scenario: The item's type, id, object name and place in the file are given
    When the agent inspects "role=button[name=\"Card\"]"
    Then the answer's "/type" is "Rectangle"
    And the answer's "/id" is "card"
    And the answer's "/object_name" is "card"
    And the answer's "/role" is "button"
    And the answer's "/source/file" matches "Scenario\.qml$"
    And the answer's "/source/line" is 7

  Scenario: The geometry is given in the item's parent and in the window
    When the agent inspects "#card"
    Then the answer's "/geometry/x" is 10
    And the answer's "/geometry/width" is 150
    And the answer's "/geometry/height" is 40
    And the answer's "/geometry/window" is [10, 0, 150, 40]

  Scenario Outline: A property says where its value came from
    When the agent inspects "#card"
    Then the inspected property "<property>" is "<provenance>"

    Examples:
      | property | provenance |
      | height   | assigned   |
      | color    | assigned   |
      | x        | bound      |
      | width    | bound      |
      | opacity  | animated   |

  # What the toolkit writes itself (focus, a window's size, a font that
  # follows the parent's) keeps a binding below it as an animation step
  # does, and is still not an animation.
  Scenario: A value the toolkit gives is said to be the host's
    When the property "focus" of "other" is set to true
    And the agent inspects every property of "#field"
    Then the inspected property "activeFocus" is "host"
    And the inspected property "activeFocus" has the value "false"

  Scenario: A bound property says where its binding was written
    When the agent inspects "#card"
    Then the inspected property "width" was written at line 11
    And the inspected property "x" was written at line 10

  Scenario: A binding that only reads another property runs without JavaScript
    When the agent inspects "#card"
    Then the inspected property "x" is bound at tier 0

  Scenario: A value assigned later replaces the binding
    When the property "width" of "card" is set to 20
    And the agent inspects "#card"
    Then the inspected property "width" is "assigned"
    And the inspected property "width" has the value "20"

  Scenario: Properties that were never set are left out unless asked for
    When the agent inspects "#card"
    Then the answer has no inspected property "rotation"

  Scenario: Every property can be asked for
    When the agent inspects every property of "#card"
    Then the inspected property "rotation" is "default"
    And the inspected property "rotation" has the value "0"
    And the inspected property "y" is "default"

  Scenario: An item without a role is found by its object name
    When the agent inspects "#page"
    Then the answer's "/type" is "Scenario"
    And the answer's "/types/1" is "Item"
    And the inspected property "margin" has the value "10"
    And the inspected property "margin" is "assigned"
    And the answer's "/role" is null

  Scenario: An item that is not shown says so
    When the property "visible" of "card" is set to false
    And the agent inspects "#card"
    Then the answer's "/state/visible" is false
    And the answer's "/state/lattice" is "hidden"

  Scenario: A locator that fits nothing is refused
    When the agent inspects "role=button[name=\"Nowhere\"]"
    Then the agent is refused with "not_found"
