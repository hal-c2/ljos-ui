# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#lensFlareEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#lensFlareBloomBias-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#lensFlareGhostDispersal-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: The lens flare of ExtendedSceneEnvironment
  An ExtendedSceneEnvironment can draw the flare a camera's lens makes of
  bright light: ghosts of the light on the far side of the view's middle,
  a halo around it, coloured fringes, and a dirt or starburst texture over
  them. Only light above the bloom bias flares, and the bloom scale says
  how much.

  # A small white unlit square, 10 units across, sits up and left of the
  # middle of an orthographic view, at pixels 65 to 75. Mirrored through
  # the middle it would be at 125 to 135; its ghosts lie on that line.
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
          Model {
            source: "#Rectangle"; x: -30; y: 30; scale: Qt.vector3d(0.1, 0.1, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "white"
            }
          }
        }
      }
      """

  Rule: The lens flare starts off, with the values the documentation gives

    Scenario: A new ExtendedSceneEnvironment
      Then the property "lensFlareEnabled" of "env" is false
      And the property "lensFlareBloomScale" of "env" is 10
      And the property "lensFlareBloomBias" of "env" is 0.95
      And the property "lensFlareGhostDispersal" of "env" is 0.5
      And the property "lensFlareGhostCount" of "env" is 4
      And the property "lensFlareHaloWidth" of "env" is 0.25
      And the property "lensFlareStretchToAspect" of "env" is 0.5
      And the property "lensFlareDistortion" of "env" is 5
      And the property "lensFlareBlurAmount" of "env" is 3
      And the property "lensFlareApplyDirtTexture" of "env" is false
      And the property "lensFlareApplyStarburstTexture" of "env" is false
      And the property "lensFlareCameraDirection" of "env" is {"x":0,"y":0,"z":-1}

    Scenario: Without the flare the far side of the view stays black
      Then the pixel at 160, 160 of the window is within 2 of "#000000"
      And the pixel at 70, 70 of the window is within 2 of "#ffffff"

  Rule: Bright light makes ghosts on the far side of the view's middle

    Scenario: With the flare a ghost of the square lights the far side
      When the property "lensFlareBloomBias" of "env" is set to 0
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 160, 160 of the window is not "#000000"
      And the pixel at 160, 40 of the window is within 2 of "#000000"

    Scenario: Light below the bloom bias does not flare
      When the property "lensFlareBloomBias" of "env" is set to 2
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 160, 160 of the window is within 2 of "#000000"

    Scenario: No ghosts and no halo leave no flare
      When the property "lensFlareBloomBias" of "env" is set to 0
      And the property "lensFlareGhostCount" of "env" is set to 0
      And the property "lensFlareHaloWidth" of "env" is set to 0
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 160, 160 of the window is within 2 of "#000000"

    Scenario: Without ghosts the halo still flares
      When the property "lensFlareBloomBias" of "env" is set to 0
      And the property "lensFlareGhostCount" of "env" is set to 0
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 94, 94 of the window is not "#000000"

    Scenario Outline: The flare still shows with the <texture> texture applied
      When the property "lensFlareBloomBias" of "env" is set to 0
      And the property "<property>" of "env" is set to true
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 160, 160 of the window is not "#000000"

      Examples:
        | texture   | property                       |
        | dirt      | lensFlareApplyDirtTexture      |
        | starburst | lensFlareApplyStarburstTexture |

  Rule: A still scene with a lens flare does no work

    Scenario: Nothing is scheduled once the flare is drawn
      When the property "lensFlareBloomBias" of "env" is set to 0
      And the property "lensFlareEnabled" of "env" is set to true
      Then the pixel at 160, 160 of the window is not "#000000"
      And the application has nothing scheduled
