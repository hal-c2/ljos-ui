# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only (a QAccessible helper would be needed to verify them on Qt).

@ljos-only @a11y
Feature: Accessible descriptions
  Accessible.description says what an item does, in more words than its name.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Rectangle {
          objectName: "save"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Save"
          Accessible.description: "Writes the file"
        }
        Rectangle {
          objectName: "open"
          y: 40; width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "Open"
        }
        Item {
          objectName: "hint"
          Accessible.description: "Only a hint"
        }
      }
      """
    And an assistive technology is listening

  Scenario: A description is exposed with its node
    Then the "button" named "Save" is described as "Writes the file"

  Scenario: An item that sets no description has an empty one
    Then the "button" named "Open" is described as ""

  Scenario: Changing the description updates the node
    When the property "Accessible.description" of "save" is set to "Writes to disk"
    Then the "button" named "Save" is described as "Writes to disk"

  Scenario: Clearing the description empties it
    When the property "Accessible.description" of "save" is set to ""
    Then the "button" named "Save" is described as ""

  # Ours: an item needs a role or a name to be a node, so that the tree
  # holds only what a user can be told about.
  Scenario: A description alone does not expose an item
    Then the accessibility tree has no "group" named ""
