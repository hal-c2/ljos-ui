# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-2d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
#   https://doc.qt.io/qt-6/qtquick3d-quickitems-example.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Items inside a 3D scene
  An Item declared inside a Node is drawn in the scene as a flat picture in
  the node's plane: one pixel of the item is one scene unit, its top-left
  corner sits at the node's origin and it reaches down the node's y axis as
  it does on the screen. The node's position, rotation and scale move it
  as they move a model; anchors.centerIn: parent centres it on the node's
  origin. The item is not lit: it shows its own colours in a scene without
  lights. A click on it reaches the item under the pointer.

  # The view shows one scene unit as one pixel, the scene's origin in its
  # middle. The card's top-left corner is at the origin, so it covers 100
  # to 180 across and 100 to 140 down. The badge's node is 60 units down;
  # the badge is centred on it, so it covers 80 to 120 across and 150 to
  # 170 down. There is no light.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 200; color: "black"
        property string clicked: ""
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          Node {
            objectName: "node"
            Rectangle {
              objectName: "card"; width: 80; height: 40; color: "#ff0000"
              MouseArea { anchors.fill: parent; onClicked: root.clicked = "card" }
            }
          }
          Node {
            y: -60
            Rectangle { anchors.centerIn: parent; width: 40; height: 20; color: "#0000ff" }
          }
        }
      }
      """

  Rule: The item's top-left corner is at the node's origin, one pixel a unit

    Scenario: The item reaches right and down from the origin
      Then the pixel at 140, 120 of the window is within 2 of "#ff0000"
      And the pixel at 175, 135 of the window is within 2 of "#ff0000"
      And the pixel at 60, 120 of the window is within 2 of "#000000"
      And the pixel at 140, 80 of the window is within 2 of "#000000"
      And the pixel at 140, 150 of the window is within 2 of "#000000"

    Scenario: anchors.centerIn: parent centres the item on the node's origin
      Then the pixel at 85, 155 of the window is within 2 of "#0000ff"
      And the pixel at 115, 165 of the window is within 2 of "#0000ff"
      And the pixel at 75, 160 of the window is within 2 of "#000000"
      And the pixel at 125, 160 of the window is within 2 of "#000000"

  Rule: The node moves the item as it moves a model

    Scenario: The node's position moves the item
      When the property "x" of "node" is set to 50
      Then the pixel at 170, 120 of the window is within 2 of "#ff0000"
      And the pixel at 120, 120 of the window is within 2 of "#000000"

    Scenario: The node's scale scales the item
      When the property "scale.x" of "node" is set to 2
      And the property "scale.y" of "node" is set to 2
      Then the pixel at 170, 170 of the window is within 2 of "#ff0000"

    Scenario: A hidden node hides the item
      When the property "visible" of "node" is set to false
      Then the pixel at 140, 120 of the window is within 2 of "#000000"

  Rule: The picture follows the item

    Scenario: A change to the item is drawn
      When the property "color" of "card" is set to "#00ff00"
      Then the pixel at 140, 120 of the window is within 2 of "#00ff00"

  # Qt's own event loop cannot be read, so only Ljós is held to this.
  @ljos-only
  Rule: An item that does not change costs nothing

    Scenario: Nothing is scheduled while the item stays as it is
      Then the application has nothing scheduled

  Rule: A click reaches the item under the pointer

    Scenario: A click on the item clicks its MouseArea
      When the user presses the pointer at 140, 120
      And the user releases the pointer at 140, 120
      Then the property "clicked" of "root" is "card"

    Scenario: A click beside the item reaches nothing in it
      When the user presses the pointer at 60, 60
      And the user releases the pointer at 60, 60
      Then the property "clicked" of "root" is ""

    Scenario: A click follows the item where the node moved it
      When the property "x" of "node" is set to 50
      And the user presses the pointer at 170, 120
      And the user releases the pointer at 170, 120
      Then the property "clicked" of "root" is "card"
