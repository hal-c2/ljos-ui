# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-helpers-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-spheregeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-cuboidgeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-planegeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-gridgeometry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A grid geometry draws lines
  A GridGeometry is line segments in the XY plane, centred on the origin:
  horizontalLines lines parallel to X, horizontalStep apart, and
  verticalLines lines parallel to Y, verticalStep apart. Lines are one
  pixel wide and do not light.

  Background:
    Given the QML:
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
            objectName: "model"
            x: 0.5; y: -0.5
            geometry: GridGeometry { objectName: "shape"; horizontalLines: 3; horizontalStep: 40; verticalLines: 3; verticalStep: 40 }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
              cullMode: DefaultMaterial.NoCulling
            }
          }
        }
      }
      """

  # The model is half a unit off so that the lines run through the
  # centres of pixels, not between them.
  # Three lines each way, 40 apart: at -40, 0 and 40 from the centre, each
  # spanning 80 across.
  Scenario: The lines are drawn where the steps put them
    Then the pixel at 100, 100 of the window is "#ff0000"
    And the pixel at 60, 100 of the window is "#ff0000"
    And the pixel at 100, 60 of the window is "#ff0000"
    And the pixel at 100, 140 of the window is "#ff0000"

  Scenario: Between the lines nothing is drawn
    Then the pixel at 80, 80 of the window is "#202020"
    And the pixel at 120, 120 of the window is "#202020"

  Scenario: A line is not drawn past its end
    Then the pixel at 100, 30 of the window is "#202020"
    And the pixel at 30, 100 of the window is "#202020"

  Scenario: A wider step spreads the lines
    When the property "verticalStep" of "shape" is set to 60
    Then the pixel at 40, 100 of the window is "#ff0000"
    And the pixel at 160, 100 of the window is "#ff0000"
