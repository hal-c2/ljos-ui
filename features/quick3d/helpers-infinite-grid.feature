# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-infinitegrid.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A grid on the floor of a scene
  An InfiniteGrid declared in a SceneEnvironment draws lines on the floor
  (the XZ plane) gridInterval apart, and with gridAxes the X axis in red
  and the Z axis in blue.

  # The camera looks straight down from above, with +X to the right and
  # +Z down the window. One scene unit is a pixel, and the camera is half
  # a pixel off so that a line covers pixels and does not lie between them.
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
            InfiniteGrid { objectName: "grid"; gridInterval: 20 }
          }
          OrthographicCamera {
            x: -0.125; y: 300; z: -0.125
            eulerRotation.x: -90
          }
        }
      }
      """

  Scenario: The axes are red and blue
    Then the pixel at 150, 100 of the window is "#ff0000"
    And the pixel at 100, 150 of the window is "#0000ff"

  Scenario: The lines are an interval apart
    Then the pixel at 120, 130 of the window is not "#202020"
    And the pixel at 130, 130 of the window is "#202020"

  Scenario: The axes are drawn unless switched off
    When the property "gridAxes" of "grid" is set to false
    Then the pixel at 150, 100 of the window is not "#ff0000"
    And the pixel at 150, 100 of the window is not "#202020"

  Scenario: A grid that is not visible draws nothing
    When the property "visible" of "grid" is set to false
    Then the pixel at 120, 130 of the window is "#202020"
    And the pixel at 150, 100 of the window is "#202020"
