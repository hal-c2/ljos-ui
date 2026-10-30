# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-material.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: PrincipledMaterial and what every material has
  A PrincipledMaterial is lit physically: a base colour that a metalness
  between 0 (a dielectric) and 1 (a metal) interprets, a roughness that
  spreads its highlights, an emissive colour of its own, an opacity, and an
  alphaMode that says whether that opacity blends, cuts out below alphaCutoff
  or is ignored. Every material culls the faces its cullMode names.

  # A red square, 100 units across, faces the camera in front of a blue
  # backdrop. The camera is orthographic, so one scene unit is one pixel and
  # the square covers pixels 50 to 150. Both start unlit, so a pixel shows a
  # material's colour as it is.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 600 }
          DirectionalLight { objectName: "sun"; brightness: 0 }
          Model {
            source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "blue"
            }
          }
          Model {
            objectName: "front"; source: "#Rectangle"
            materials: PrincipledMaterial {
              objectName: "pbr"; baseColor: "red"
              lighting: PrincipledMaterial.NoLighting
            }
          }
        }
      }
      """

  Rule: The materials start from the values the documentation gives

    Scenario: A new PrincipledMaterial
      Then the property "metalness" of "pbr" is 0
      And the property "roughness" of "pbr" is 0
      And the property "alphaCutoff" of "pbr" is 0.5
      And the property "emissiveFactor.x" of "pbr" is 0
      And the property "emissiveFactor.y" of "pbr" is 0
      And the property "emissiveFactor.z" of "pbr" is 0

  Rule: An unlit material shows its colour as it is

    Scenario: The square shows its base colour over the backdrop
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
      And the pixel at 20, 20 of the window is within 2 of "#0000ff"

  # Whether the colours blend in linear light or in sRGB is not documented:
  # half red over blue is #bc00bc one way and #800080 the other. The
  # tolerance takes either and neither colour alone.
  Rule: The alpha mode says what opacity does

    Scenario: By default a material less than opaque blends
      When the property "opacity" of "pbr" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#9e009e"

    Scenario: An opaque material ignores its opacity
      When the property "alphaMode" of "pbr" is set to 2
      And the property "opacity" of "pbr" is set to 0.5
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: A masked material below the cutoff is not drawn
      When the property "alphaMode" of "pbr" is set to 3
      And the property "opacity" of "pbr" is set to 0.4
      Then the pixel at 100, 100 of the window is within 2 of "#0000ff"

    Scenario: A masked material at or above the cutoff is drawn opaque
      When the property "alphaMode" of "pbr" is set to 3
      And the property "opacity" of "pbr" is set to 0.6
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: The cutoff is the material's own
      When the property "alphaMode" of "pbr" is set to 3
      And the property "opacity" of "pbr" is set to 0.6
      And the property "alphaCutoff" of "pbr" is set to 0.7
      Then the pixel at 100, 100 of the window is within 2 of "#0000ff"

  Rule: A material culls the faces its cull mode names

    Scenario: A face turned away from the camera is culled by default
      When the property "eulerRotation.y" of "front" is set to 180
      Then the pixel at 100, 100 of the window is within 2 of "#0000ff"

    Scenario: With no culling a face turned away is drawn
      When the property "eulerRotation.y" of "front" is set to 180
      And the property "cullMode" of "pbr" is set to 2
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: Front face culling culls a face turned to the camera
      When the property "cullMode" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 2 of "#0000ff"

  Rule: A lit material's emissive colour is its own light

    Scenario: A lit material with no light on it is black
      When the property "lighting" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 2 of "#000000"

    Scenario: The emissive factor lights a material with no light on it
      When the property "lighting" of "pbr" is set to 1
      And the property "baseColor" of "pbr" is set to "black"
      And the property "emissiveFactor.x" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: An unlit material has no emissive light
      When the property "baseColor" of "pbr" is set to "black"
      And the property "emissiveFactor.x" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 2 of "#000000"

  # The square is white and lit by the directional light; the camera sees
  # it straight on. The expected values come from the textbook
  # metal-roughness model, not from Qt's pictures: the tolerances say how
  # far apart the outcomes are, not how close Qt is.
  Rule: Metalness and roughness say how a surface reflects a light

    Background:
      When the property "lighting" of "pbr" is set to 1
      And the property "baseColor" of "pbr" is set to "white"
      And the property "brightness" of "sun" is set to 1

    # The light meets the surface at 60°: cos 60° = 0.5, which is 188 in
    # sRGB, a little less what a dielectric reflects.
    Scenario: A dielectric lit from the side shows its colour by the angle
      When the property "eulerRotation.y" of "sun" is set to 60
      And the property "roughness" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 12 of "#bcbcbc"

    Scenario: A smooth metal lit from the side reflects the light away
      When the property "eulerRotation.y" of "sun" is set to 60
      And the property "metalness" of "pbr" is set to 1
      And the property "roughness" of "pbr" is set to 0.1
      Then the pixel at 100, 100 of the window is within 4 of "#000000"

    Scenario: A rough metal lit from the side spreads some light to the camera
      When the property "eulerRotation.y" of "sun" is set to 60
      And the property "metalness" of "pbr" is set to 1
      And the property "roughness" of "pbr" is set to 1
      Then the pixel at 100, 100 of the window is within 40 of "#727272"

    Scenario: A smooth metal lit head on reflects the light to the camera
      When the property "metalness" of "pbr" is set to 1
      And the property "roughness" of "pbr" is set to 0.1
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"
