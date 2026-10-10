# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquick-visualcanvas-visualparent.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only (a QAccessible helper would be needed to verify them on Qt).

@ljos-only @a11y
Feature: Nesting in the accessibility tree
  The accessibility tree follows the visual tree: a node is inside the nearest
  item above it that is itself a node.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        function moveSave() { save.parent = page }
        Item {
          objectName: "tools"
          width: 200; height: 40
          Accessible.role: Accessible.ToolBar; Accessible.name: "Tools"
          Item {
            objectName: "spacer"
            Rectangle {
              id: save
              width: 80; height: 30
              Accessible.role: Accessible.Button; Accessible.name: "Save"
            }
          }
        }
        Item {
          id: page
          y: 40; width: 200; height: 60
          Accessible.role: Accessible.Pane; Accessible.name: "Page"
        }
      }
      """
    And an assistive technology is listening

  Scenario: A node is inside the node of the item around it
    Then the "button" named "Save" is inside the "toolbar" named "Tools"

  Scenario: Items that are not nodes do not show between two nodes
    Then the accessibility tree has no "group" named ""

  Scenario: An item with no node above it is at the top
    Then the "toolbar" named "Tools" is inside the "window" named ""

  Scenario: An item moved to another parent moves in the tree
    When the method "moveSave" of "root" is called
    Then the "button" named "Save" is inside the "pane" named "Page"

  Scenario: An item in between that gets a name takes the nodes inside it
    When the property "Accessible.name" of "spacer" is set to "Files"
    Then the "button" named "Save" is inside the "group" named "Files"

  Scenario: An item in between that gets a name is inside the node around it
    When the property "Accessible.name" of "spacer" is set to "Files"
    Then the "group" named "Files" is inside the "toolbar" named "Tools"
