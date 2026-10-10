# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only (a QAccessible helper would be needed to verify them on Qt).

@ljos-only @a11y
Feature: Items left out of the accessibility tree
  An item that is ignored, or that cannot be seen, is not in the accessibility tree.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item {
          objectName: "tools"
          width: 200; height: 40
          Accessible.role: Accessible.ToolBar
          Accessible.name: "Tools"
          Rectangle {
            objectName: "save"
            width: 80; height: 30
            Accessible.role: Accessible.Button
            Accessible.name: "Save"
          }
        }
      }
      """
    And an assistive technology is listening

  Rule: Accessible.ignored takes the item out, not what is inside it

    Scenario: An ignored item is left out
      When the property "Accessible.ignored" of "save" is set to true
      Then the accessibility tree has no "button" named "Save"

    Scenario: An item that is no longer ignored comes back
      When the property "Accessible.ignored" of "save" is set to true
      And the property "Accessible.ignored" of "save" is set to false
      Then the "button" named "Save" is inside the "toolbar" named "Tools"

    Scenario: The items inside an ignored item stay
      When the property "Accessible.ignored" of "tools" is set to true
      Then the "button" named "Save" is inside the "window" named ""

    Scenario: An ignored item keeps its place for when it comes back
      When the property "Accessible.ignored" of "tools" is set to true
      And the property "Accessible.ignored" of "tools" is set to false
      Then the "button" named "Save" is inside the "toolbar" named "Tools"

  Rule: An invisible item takes everything inside it out too

    Scenario: An invisible item is left out
      When the property "visible" of "save" is set to false
      Then the accessibility tree has no "button" named "Save"

    Scenario: What is inside an invisible item is left out
      When the property "visible" of "tools" is set to false
      Then the accessibility tree has no "button" named "Save"

    Scenario: An item that is shown again comes back
      When the property "visible" of "tools" is set to false
      And the property "visible" of "tools" is set to true
      Then the "button" named "Save" is inside the "toolbar" named "Tools"

    Scenario: A disabled item stays in the tree
      When the property "enabled" of "save" is set to false
      Then the accessibility tree has a "button" named "Save"
