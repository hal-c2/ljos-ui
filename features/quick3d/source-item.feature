# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
#   https://doc.qt.io/qt-6/qtquick3d-2d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: An item as a texture
  A Texture whose sourceItem is set shows that item and its children, drawn
  off the screen, instead of an image file: source is then ignored. The
  picture follows the item: when the item or a child changes, the texture
  shows the change. Input over a pickable model reaches the item at the
  point of the item the pointer is over, so a MouseArea in it is clicked.

  # A square 200 units across fills the view, which shows one scene unit as
  # one pixel. The texture's item is 64 by 64 and split into quadrants, red
  # top left, green top right, blue bottom left and white bottom right, so
  # a pixel tells which part of the item is drawn there. The red and green
  # quadrants are clickable and say so in "clicked".
  Background:
    Given a 64 by 64 image file "quadrants.png" of quadrants "#ffff00, #ffff00, #ffff00, #ffff00"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 200; color: "black"
        property string clicked: ""
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          Model {
            objectName: "screen"; pickable: true; source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting
              diffuseMap: Texture { objectName: "tex"; sourceItem: Rectangle {
                width: 64; height: 64; color: "white"
                Rectangle { y: 32; width: 32; height: 32; color: "#0000ff" }
                Rectangle { objectName: "red"; width: 32; height: 32; color: "#ff0000"
                  MouseArea { anchors.fill: parent; onClicked: root.clicked = "red" } }
                Rectangle { x: 32; width: 32; height: 32; color: "#00ff00"
                  MouseArea { anchors.fill: parent; onClicked: root.clicked = "green" } }
              } }
            }
          }
        }
      }
      """

  Rule: The item is drawn into the texture

    Scenario: The item covers the mesh upright
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 150, 50 of the window is within 2 of "#00ff00"
      And the pixel at 50, 150 of the window is within 2 of "#0000ff"
      And the pixel at 150, 150 of the window is within 2 of "#ffffff"

    Scenario: A source is ignored while there is a sourceItem
      When the property "source" of "tex" is set to "quadrants.png"
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 150, 150 of the window is within 2 of "#ffffff"

  Rule: The texture follows the item

    Scenario: A child's change is drawn
      When the property "color" of "red" is set to "#ffff00"
      Then the pixel at 50, 50 of the window is within 2 of "#ffff00"
      And the pixel at 150, 50 of the window is within 2 of "#00ff00"

    Scenario: A child that moves is drawn where it went
      When the property "y" of "red" is set to 32
      Then the pixel at 50, 50 of the window is within 2 of "#ffffff"
      And the pixel at 50, 150 of the window is within 2 of "#ff0000"

  # Qt's own event loop cannot be read, so only Ljós is held to this.
  @ljos-only
  Rule: An item that does not change costs nothing

    Scenario: Nothing is scheduled while the item stays as it is
      Then the application has nothing scheduled

    Scenario: A change to the item is drawn and then nothing more is scheduled
      When the property "color" of "red" is set to "#ffff00"
      Then the application has nothing scheduled

  Rule: A click on a pickable model reaches the item under it

    Scenario Outline: A click reaches the MouseArea at the point of the item it is over
      When the user presses the pointer at <x>, <y>
      And the user releases the pointer at <x>, <y>
      Then the property "clicked" of "root" is "<clicked>"

      Examples:
        | x   | y   | clicked |
        | 50  | 50  | red     |
        | 150 | 50  | green   |
        | 50  | 150 |         |

    Scenario: A model that is not pickable takes no clicks for its item
      When the property "pickable" of "screen" is set to false
      And the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "clicked" of "root" is ""
