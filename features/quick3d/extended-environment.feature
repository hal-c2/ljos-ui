# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: ExtendedSceneEnvironment's built-in effects and ambient occlusion
  ExtendedSceneEnvironment, from QtQuick3D.Helpers, is a SceneEnvironment
  with common post-processing built in, each effect turned on by its own
  property: exposure and white point for the tonemapper, glow and bloom,
  depth of field, colour adjustments, a vignette, a look-up table, FXAA and
  dithering. Ambient occlusion belongs to every SceneEnvironment and has a
  feature of its own.

  # A square, 100 units across, faces an orthographic camera, so it covers
  # pixels 50 to 150 on black. It is unlit, and its grey is 0.5 in linear
  # light, which the default linear tonemapping shows as #bcbcbc.
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
          environment: ExtendedSceneEnvironment {
            objectName: "env"
            clearColor: "black"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          Model {
            source: "#Rectangle"
            materials: DefaultMaterial {
              objectName: "grey"
              lighting: DefaultMaterial.NoLighting; diffuseColor: "#bcbcbc"
            }
          }
        }
      }
      """

  Rule: The effects start off, with the values the documentation gives

    Scenario: A new ExtendedSceneEnvironment
      Then the property "exposure" of "env" is 1
      And the property "whitePoint" of "env" is 1
      And the property "glowEnabled" of "env" is false
      And the property "glowIntensity" of "env" is 1
      And the property "glowHDRMinimumValue" of "env" is 1
      And the property "vignetteEnabled" of "env" is false
      And the property "vignetteRadius" of "env" is 0.35
      And the property "vignetteStrength" of "env" is 15
      And the property "colorAdjustmentsEnabled" of "env" is false
      And the property "adjustmentSaturation" of "env" is 1
      And the property "depthOfFieldEnabled" of "env" is false
      And the property "depthOfFieldFocusDistance" of "env" is 600
      And the property "depthOfFieldFocusRange" of "env" is 100
      And the property "fxaaEnabled" of "env" is false
      And the property "ditheringEnabled" of "env" is false
      And the property "lutEnabled" of "env" is false
      And the property "lutSize" of "env" is 16

    Scenario: With no effect on, the scene looks as a SceneEnvironment draws it
      Then the pixel at 100, 100 of the window is within 3 of "#bcbcbc"
      And the pixel at 20, 20 of the window is within 2 of "#000000"

  Rule: Exposure scales the light before it is tonemapped

    Scenario: Twice the exposure makes half the light full
      When the property "exposure" of "env" is set to 2
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"

    Scenario: Half the exposure halves the light
      When the property "exposure" of "env" is set to 0.5
      Then the pixel at 100, 100 of the window is within 3 of "#898989"

  Rule: Colour adjustments change brightness, contrast and saturation

    Scenario: Adjustments do nothing until they are enabled
      When the property "adjustmentBrightness" of "env" is set to 4
      Then the pixel at 100, 100 of the window is within 3 of "#bcbcbc"

    Scenario: More brightness brightens
      When the property "colorAdjustmentsEnabled" of "env" is set to true
      And the property "adjustmentBrightness" of "env" is set to 4
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"

    # How much each primary weighs in a grey is not documented; red's grey
    # is #4c4c4c weighed in sRGB and #7f7f7f weighed in linear light.
    Scenario: No saturation leaves a grey
      When the property "diffuseColor" of "grey" is set to "red"
      And the property "colorAdjustmentsEnabled" of "env" is set to true
      And the property "adjustmentSaturation" of "env" is set to 0
      Then the pixel at 100, 100 of the window is within 30 of "#666666"

  Rule: A vignette darkens the edges of the view toward its colour

    Scenario: The corners take the vignette's colour and the middle does not
      When the property "diffuseColor" of "grey" is set to "white"
      And the property "vignetteEnabled" of "env" is set to true
      And the property "vignetteColor" of "env" is set to "red"
      Then the pixel at 100, 100 of the window is within 3 of "#ffffff"
      And the pixel at 1, 1 of the window is within 40 of "#ff0000"

  Rule: Glow spreads bright light past the edges it came from

    Scenario: Without glow the black around the square stays black
      When the property "diffuseColor" of "grey" is set to "white"
      Then the pixel at 46, 100 of the window is within 2 of "#000000"

    Scenario: With glow the black around the square is lit
      When the property "diffuseColor" of "grey" is set to "white"
      And the property "glowHDRMinimumValue" of "env" is set to 0
      And the property "glowBloom" of "env" is set to 1
      And the property "glowBlendMode" of "env" is set to 0
      And the property "glowEnabled" of "env" is set to true
      Then the pixel at 46, 100 of the window is not "#000000"

  Rule: Depth of field blurs what is out of focus

    # The square is 600 units from the camera, where the focus is by default.
    # The empty background behind it is out of focus and may blur over its
    # edge, so only the inside of the edge is checked.
    Scenario: The square in focus keeps a sharp edge
      When the property "diffuseColor" of "grey" is set to "white"
      And the property "depthOfFieldEnabled" of "env" is set to true
      Then the pixel at 53, 100 of the window is within 2 of "#ffffff"

    Scenario: The square out of focus is blurred across its edge
      When the property "diffuseColor" of "grey" is set to "white"
      And the property "depthOfFieldFocusDistance" of "env" is set to 100
      And the property "depthOfFieldFocusRange" of "env" is set to 10
      And the property "depthOfFieldBlurAmount" of "env" is set to 10
      And the property "depthOfFieldEnabled" of "env" is set to true
      Then the pixel at 47, 100 of the window is not "#000000"
      And the pixel at 53, 100 of the window is not "#ffffff"

  Rule: The other built-in effects keep a flat colour as it is

    Scenario Outline: <effect> leaves the middle of the square alone
      When the property "<effect>" of "env" is set to true
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

      Examples:
        | effect           |
        | fxaaEnabled      |
        | ditheringEnabled |
        | lutEnabled       |
