# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A light probe, the sky box and tone mapping
  A SceneEnvironment's lightProbe is an image of everything around the
  scene, a Radiance HDR file or an ordinary picture, laid around it as a
  sphere. It lights every PrincipledMaterial whatever the background is,
  and with backgroundMode SkyBox the view shows it behind the models.
  probeExposure brightens it, probeOrientation turns it, probeHorizon
  darkens its lower half in what it lights and skyboxBlurAmount blurs the
  sky box. tonemapMode says how the light a pixel gathers becomes a colour
  on screen: TonemapModeLinear, the default, gamma corrects it into sRGB;
  TonemapModeNone writes it as it is.

  # A perspective camera looks at a small square in front of a probe whose
  # radiance is 0.5 everywhere; the halves probe is 0.5 above the horizon
  # and 0 below it. The square starts hidden. Linear light of 0.5 is #bcbcbc
  # in sRGB.
  Background:
    Given a 64 by 32 Radiance image file "grey.hdr" filled with 0.5, 0.5, 0.5
    And a 64 by 32 Radiance image file "halves.hdr" with an upper half of 0.5, 0.5, 0.5 and a lower half of 0, 0, 0
    And a 64 by 32 image file "grey.png" filled with "#808080"
    And a 4 by 4 image file "black.png" filled with "#000000"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; backgroundMode: SceneEnvironment.SkyBox
            lightProbe: Texture { objectName: "probe"; source: "grey.hdr" }
          }
          PerspectiveCamera { z: 300 }
          Model {
            objectName: "square"; source: "#Rectangle"; visible: false
            materials: PrincipledMaterial {
              objectName: "pbr"; roughness: 1
              occlusionMap: Texture { objectName: "ao" }
            }
          }
        }
      }
      """

  Rule: The environment starts from the values the documentation gives

    Scenario: A new SceneEnvironment's probe and tone mapping
      Then the property "tonemapMode" of "env" is 1
      And the property "probeExposure" of "env" is 1
      And the property "probeHorizon" of "env" is 0
      And the property "skyboxBlurAmount" of "env" is 0
      And the property "probeOrientation.x" of "env" is 0

  Rule: A sky box shows the light probe behind the models

    Scenario: The sky box shows the probe's radiance
      Then the pixel at 10, 10 of the window is within 3 of "#bcbcbc"

    Scenario: A transparent background shows no sky box
      When the property "backgroundMode" of "env" is set to 0
      Then the pixel at 10, 10 of the window is "#000000"

    Scenario: An ordinary picture is a light probe too
      When the property "source" of "probe" is set to "grey.png"
      Then the pixel at 10, 10 of the window is within 3 of "#808080"

    Scenario: Up in the probe is up in the scene
      When the property "source" of "probe" is set to "halves.hdr"
      Then the pixel at 100, 20 of the window is within 3 of "#bcbcbc"
      And the pixel at 100, 180 of the window is within 3 of "#000000"

    Scenario: probeOrientation turns the probe
      When the property "source" of "probe" is set to "halves.hdr"
      And the property "probeOrientation.x" of "env" is set to 180
      Then the pixel at 100, 20 of the window is within 3 of "#000000"
      And the pixel at 100, 180 of the window is within 3 of "#bcbcbc"

    # Linear light of 0.25, half way between the halves, is #898989.
    Scenario: A blurred sky box mixes the colours across the horizon
      When the property "source" of "probe" is set to "halves.hdr"
      And the property "skyboxBlurAmount" of "env" is set to 1
      Then the pixel at 100, 100 of the window is within 40 of "#898989"

    Scenario: probeExposure scales the probe's light
      When the property "probeExposure" of "env" is set to 0.5
      Then the pixel at 10, 10 of the window is within 3 of "#898989"

  # Linear, ACES, Hejl-Dawson and filmic are the published curves of those
  # names (Narkowicz's fit of ACES, Hable's filmic curve). The documentation
  # names them but does not give them, so these values are Ljós's own.
  Rule: The tone mapping mode says how light becomes a colour

    Scenario Outline: A sky of 0.5 under each tone mapping mode
      When the property "tonemapMode" of "env" is set to <mode>
      Then the pixel at 10, 10 of the window is within 3 of "<colour>"

      Examples:
        | mode | tonemapping | colour  |
        | 0    | None        | #808080 |
        | 1    | Linear      | #bcbcbc |
        | 2    | Aces        | #cecece |
        | 3    | HejlDawson  | #bababa |
        | 4    | Filmic      | #969696 |

    Scenario: Without tone mapping probeExposure does nothing
      When the property "tonemapMode" of "env" is set to 0
      And the property "probeExposure" of "env" is set to 0.5
      Then the pixel at 10, 10 of the window is within 3 of "#808080"

  # A rough white surface facing a probe of the same radiance all round
  # gathers that radiance; with half of it dark it gathers half.
  Rule: The light probe lights a PrincipledMaterial

    Scenario: A rough white square shows the probe's radiance
      When the property "visible" of "square" is set to true
      Then the pixel at 100, 100 of the window is within 12 of "#bcbcbc"

    Scenario: probeHorizon darkens the light from below
      When the property "visible" of "square" is set to true
      And the property "probeHorizon" of "env" is set to 1
      Then the pixel at 100, 100 of the window is within 12 of "#898989"

    Scenario: A smooth metal mirrors the probe
      When the property "visible" of "square" is set to true
      And the property "metalness" of "pbr" is set to 1
      And the property "roughness" of "pbr" is set to 0
      Then the pixel at 100, 100 of the window is within 12 of "#bcbcbc"

    Scenario: An occlusion map's red channel shades the probe's light
      When the property "visible" of "square" is set to true
      And the property "source" of "ao" is set to "black.png"
      Then the pixel at 100, 100 of the window is within 4 of "#000000"
