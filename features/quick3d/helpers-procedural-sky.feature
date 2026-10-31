# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduralskytexturedata.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#lightProbe-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation, and
# the defaults, which it does not give, are Ljós's own.

@ljos-only
Feature: A sky made from colours
  A ProceduralSkyTextureData is a light probe painted from a few colours:
  the sky fades from skyHorizonColor at the horizon to skyTopColor
  straight up, the ground from groundHorizonColor to groundBottomColor
  straight down, and the sun is a disc of sunColor at sunLatitude and
  sunLongitude. As a SceneEnvironment's lightProbe it lights the models
  and, with the SkyBox background, is drawn behind them.

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
            objectName: "env"; backgroundMode: SceneEnvironment.SkyBox
            lightProbe: Texture {
              textureData: ProceduralSkyTextureData {
                objectName: "sky"; textureQuality: ProceduralSkyTextureData.SkyTextureQualityLow
                skyTopColor: "#4080c0"; skyHorizonColor: "#c0d0e0"
                groundBottomColor: "#302010"; groundHorizonColor: "#605040"
                sunColor: "#ffffff"; sunLatitude: 0; sunLongitude: 0; sunAngleMin: 5; sunAngleMax: 6; sunEnergy: 1
              }
            }
          }
          PerspectiveCamera { objectName: "camera"; fieldOfView: 10 }
          Model {
            objectName: "ball"; source: "#Sphere"; z: -2000; visible: false
            materials: PrincipledMaterial { baseColor: "white"; roughness: 1 }
          }
        }
      }
      """

  Scenario: A new sky's quality
    Then the property "textureQuality" of "sky" is 512

  Scenario: Straight up is the top of the sky
    When the property "eulerRotation.x" of "camera" is set to 90
    Then the pixel at 100, 100 of the window is within 4 of "#4080c0"

  Scenario: Straight down is the bottom of the ground
    When the property "eulerRotation.x" of "camera" is set to -90
    Then the pixel at 100, 100 of the window is within 4 of "#302010"

  Scenario: The sun is where its latitude and longitude say
    Then the pixel at 100, 100 of the window is within 4 of "#ffffff"
    When the property "sunLongitude" of "sky" is set to 90
    Then the pixel at 100, 100 of the window is not "#ffffff"

  Scenario: The sky lights a model
    When the property "backgroundMode" of "env" is set to 0
    And the property "visible" of "ball" is set to true
    Then the pixel at 100, 100 of the window is not "#000000"
    And the pixel at 5, 5 of the window is "#000000"
