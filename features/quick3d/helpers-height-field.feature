# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-heightfieldgeometry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A terrain from a grey-scale picture
  A HeightFieldGeometry has a vertex for each pixel of its source, as high
  as the pixel is light. It is as wide and as deep as extents.x and
  extents.z, centred on the origin, and as tall as extents.y.

  Background:
    Given a 8 by 8 image file "ridge.png" of quadrants "#ffffff, #ffffff, #000000, #000000"
    And a 8 by 8 image file "black.png" filled with "#000000"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          Model {
            geometry: HeightFieldGeometry {
              objectName: "field"
              extents: Qt.vector3d(100, 40, 100)
              source: "ridge.png"
            }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
              cullMode: DefaultMaterial.NoCulling
            }
          }
        }
      }
      """

  # The picture's upper half is white and its lower half black: the far
  # half of the field stands extents.y high and the near half lies flat,
  # so that seen from the front the field is a wall as tall as extents.y.
  Scenario: White is as tall as extents.y
    Then the pixel at 100, 85 of the window is "#ff0000"
    And the pixel at 55, 85 of the window is "#ff0000"
    And the pixel at 100, 55 of the window is "#202020"
    And the pixel at 160, 85 of the window is "#202020"
    And the property "status" of "field" is 2

  Scenario: A black picture is flat, and edge on it draws nothing
    When the property "source" of "field" is set to "black.png"
    Then the pixel at 100, 85 of the window is "#202020"
    And the property "status" of "field" is 2

  Scenario: A taller extent reaches higher
    When the property "extents.y" of "field" is set to 80
    Then the pixel at 100, 55 of the window is "#ff0000"
    And the pixel at 100, 15 of the window is "#202020"

  Scenario: A picture that is not there is an error
    When the property "source" of "field" is set to "missing.png"
    And 200 milliseconds pass
    Then the property "status" of "field" is 3
