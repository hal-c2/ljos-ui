# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-effects-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-effects-hdrbloomtonemap.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-effects-colormaster.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: The ready-made effects of QtQuick3D.Effects
  An effect from the module goes in a SceneEnvironment's effects like any
  other. HDRBloomTonemap makes bright light glow and sets exposure, gamma and
  the white point. ColorMaster scales each channel and sets the saturation.

  # Stage is a view with an orthographic camera and a red square, 100 units
  # across, that covers pixels 50 to 150, on black. Flat squares are unlit, so
  # red is 1 in linear light.
  Background:
    Given the file "Flat.qml":
      """qml
      import QtQuick3D
      Model {
        id: flat
        property color color: "red"
        source: "#Rectangle"
        materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: flat.color }
      }
      """
    And the file "Stage.qml":
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        property alias environment: view.environment
        property alias squareColor: square.color
        View3D {
          id: view
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          Flat { id: square }
        }
      }
      """

  Rule: HDRBloomTonemap

    Background:
      Given the QML:
        """qml
        import QtQuick3D
        import QtQuick3D.Effects
        Stage {
          environment: SceneEnvironment {
            clearColor: "black"; backgroundMode: SceneEnvironment.Color
            effects: HDRBloomTonemap { objectName: "fx" }
          }
        }
        """

    Scenario: Under the threshold it changes nothing
      Then the pixel at 100, 100 of the window is within 3 of "#ff0000"
      And the pixel at 40, 100 of the window is within 2 of "#000000"

    # A stop less of light; half of red's is #bc0000.
    Scenario: Exposure is in stops
      When the property "exposure" of "fx" is set to -1
      Then the pixel at 100, 100 of the window is within 3 of "#bc0000"

    # Half of the light, gamma 2: 0.707 in linear light is #da0000.
    Scenario: Gamma lightens the mid tones
      When the property "exposure" of "fx" is set to -1
      And the property "gamma" of "fx" is set to 2
      Then the pixel at 100, 100 of the window is within 3 of "#da0000"

    # A quarter of the light with white at one half is one half.
    Scenario: channelThreshold is the white point
      When the property "exposure" of "fx" is set to -2
      And the property "channelThreshold" of "fx" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#bc0000"

    Scenario: Bright light glows past its edge
      When the property "bloomThreshold" of "fx" is set to 0
      Then the pixel at 46, 100 of the window is within 60 of "#600000"
      And the pixel at 15, 100 of the window is within 2 of "#000000"

    Scenario: Falloff takes from the glow
      When the property "bloomThreshold" of "fx" is set to 0
      And the property "blurFalloff" of "fx" is set to 10
      Then the pixel at 46, 100 of the window is within 25 of "#2c0000"

    Scenario: tonemappingLerp 0 is no glow
      When the property "bloomThreshold" of "fx" is set to 0
      And the property "tonemappingLerp" of "fx" is set to 0
      Then the pixel at 46, 100 of the window is within 2 of "#000000"

  Rule: ColorMaster

    Background:
      Given the QML:
        """qml
        import QtQuick3D
        import QtQuick3D.Effects
        Stage {
          objectName: "stage"
          environment: SceneEnvironment {
            clearColor: "black"; backgroundMode: SceneEnvironment.Color
            effects: ColorMaster { objectName: "fx" }
          }
        }
        """

    Scenario: Red and blue start as they are
      Then the pixel at 100, 100 of the window is within 3 of "#ff0000"

    # #00aa00 is 0.4 in linear light; half as much again is 0.6.
    Scenario: Green starts half as strong again
      When the property "squareColor" of "stage" is set to "#00aa00"
      Then the pixel at 100, 100 of the window is within 3 of "#00cc00"

    Scenario: A channel's strength scales it
      When the property "redStrength" of "fx" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#bc0000"

    # The luminance of red is 0.2126: #7f in the window.
    Scenario: Saturation -1 is grey
      When the property "saturation" of "fx" is set to -1
      Then the pixel at 100, 100 of the window is within 3 of "#7f7f7f"
