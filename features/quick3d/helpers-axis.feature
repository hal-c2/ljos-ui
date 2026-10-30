# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-axishelper.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: Axes and grids to read a scene against
  An AxisHelper draws the X axis in red, the Y axis in green and the Z
  axis in blue, and a grid in the XZ plane. The XY and YZ grids are there
  to be switched on. Each part has its property to switch it off.

  # An orthographic camera at magnification 4 shows a scene unit as four
  # pixels. It is half a pixel off the origin so that a line through the
  # origin covers pixels and does not lie between them.
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
          OrthographicCamera {
            x: -0.125; y: 0.125; z: 300
            horizontalMagnification: 4; verticalMagnification: 4
          }
          AxisHelper { objectName: "axes" }
        }
      }
      """

  Rule: The axes are coloured bars

    # Seen along Z the XZ grid is edge on, its lines lie along the X axis
    # and would cover it: it is switched off to see the bars.
    Scenario: The X axis is red and the Y axis green
      When the property "enableXZGrid" of "axes" is set to false
      Then the pixel at 150, 100 of the window is "#ff0000"
      And the pixel at 100, 150 of the window is "#008000"

    Scenario: enableAxisLines switches them off
      When the property "enableXZGrid" of "axes" is set to false
      And the property "enableAxisLines" of "axes" is set to false
      Then the pixel at 150, 100 of the window is "#202020"
      And the pixel at 100, 150 of the window is "#202020"

  Rule: The grids are in the planes

    # Seen along Z, the XZ grid is edge on and draws nothing, and the XY
    # grid is what faces the camera.
    Scenario: The XY grid is off at first
      When the property "enableAxisLines" of "axes" is set to false
      Then the pixel at 140, 120 of the window is "#202020"

    Scenario: Switching on the XY grid draws its lines
      When the property "enableAxisLines" of "axes" is set to false
      And the property "enableXYGrid" of "axes" is set to true
      Then the pixel at 140, 120 of the window is not "#202020"
      And the pixel at 120, 120 of the window is "#202020"

    Scenario: The grid takes the colour it is given
      When the property "enableAxisLines" of "axes" is set to false
      And the property "enableXYGrid" of "axes" is set to true
      And the property "gridOpacity" of "axes" is set to 1
      And the property "gridColor" of "axes" is set to "#00ff00"
      Then the pixel at 140, 120 of the window is "#00ff00"
