# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-perspectivecamera.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-camera.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Cameras
  A PerspectiveCamera's fieldOfView is vertical unless its
  fieldOfViewOrientation says it is horizontal. A camera with
  frustumCullingEnabled leaves out what it cannot see, which changes nothing
  in the picture.

  # A red square, 300 units wide, in front of a camera 300 units away, in a
  # view twice as wide as it is tall. With a horizontal field of view of 90°
  # the view is 600 units across, so the square covers the middle half of
  # it: x from 50 to 150. With the same 90° taken vertically the view is
  # 1200 units across and the square covers only x from 75 to 125.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 100; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; clearColor: "#202020"
            backgroundMode: SceneEnvironment.Color
          }
          PerspectiveCamera {
            objectName: "camera"; z: 300; fieldOfView: 90
          }
          Model {
            objectName: "square"; source: "#Rectangle"
            scale: Qt.vector3d(3, 0.5, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: The field of view is vertical unless said otherwise

    Scenario: A new camera's field of view is vertical and culls nothing
      Then the property "fieldOfViewOrientation" of "camera" is 0
      And the property "frustumCullingEnabled" of "camera" is false
      And the pixel at 100, 50 of the window is within 2 of "#ff0000"
      And the pixel at 60, 50 of the window is "#202020"

    Scenario: A horizontal field of view spans the view's width
      When the property "fieldOfViewOrientation" of "camera" is set to 1
      Then the pixel at 60, 50 of the window is within 2 of "#ff0000"
      And the pixel at 140, 50 of the window is within 2 of "#ff0000"
      And the pixel at 40, 50 of the window is "#202020"

  Rule: Frustum culling changes nothing that can be seen

    Scenario: A model in view is drawn with culling on
      When the property "frustumCullingEnabled" of "camera" is set to true
      Then the pixel at 100, 50 of the window is within 2 of "#ff0000"

    Scenario: A model out of view is not drawn with culling on
      When the property "frustumCullingEnabled" of "camera" is set to true
      And the property "x" of "square" is set to 2000
      Then the pixel at 100, 50 of the window is "#202020"
