# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-perspectivecamera.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-directionallight.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
#   https://doc.qt.io/qt-6/qtquick3d-index.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A 3D scene in a View3D
  A View3D is an item that shows a 3D scene: models, a camera and lights,
  placed by Nodes. It is drawn where the item is, in the order of the items
  around it, and only when something in its scene changed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          objectName: "view"; anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; clearColor: "#202020"
            backgroundMode: SceneEnvironment.Color
          }
          PerspectiveCamera { objectName: "camera"; z: 300 }
          DirectionalLight { objectName: "sun" }
          Model {
            objectName: "cube"; source: "#Cube"
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: The view draws its scene

    Scenario: The clear colour fills where no model is
      Then the pixel at 2, 2 of the window is "#202020"

    # Unlit, a DefaultMaterial shows its diffuse colour as it is.
    Scenario: A model in front of the camera is drawn where it is
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"

    Scenario: A model that is not visible is not drawn
      When the property "visible" of "cube" is set to false
      Then the pixel at 50, 50 of the window is "#202020"

    Scenario: A model moved out of the camera's view is no longer drawn
      When the property "x" of "cube" is set to 1000
      Then the pixel at 50, 50 of the window is "#202020"

    Scenario: A transparent background shows the items under the view
      When the property "backgroundMode" of "env" is set to 0
      Then the pixel at 2, 2 of the window is "#000000"
      And the pixel at 50, 50 of the window is within 2 of "#ff0000"

  Rule: The types start from the values the documentation gives

    Scenario: A new camera, light and node
      Then the property "fieldOfView" of "camera" is 60
      And the property "clipNear" of "camera" is 10
      And the property "clipFar" of "camera" is 10000
      And the property "brightness" of "sun" is 1
      And the property "scale.x" of "cube" is 1
      And the property "eulerRotation.y" of "cube" is 0
      And the property "visible" of "cube" is true

  # Qt's own event loop cannot be read, so only Ljós is held to this.
  @ljos-only
  Rule: A still scene costs nothing

    Scenario: Nothing is scheduled while nothing in the scene changes
      Then the application has nothing scheduled

    Scenario: A change to the scene is drawn and then nothing more is scheduled
      When the property "eulerRotation.y" of "cube" is set to 45
      Then the application has nothing scheduled
