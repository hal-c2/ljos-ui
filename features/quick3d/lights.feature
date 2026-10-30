# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-light.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-pointlight.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-spotlight.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-directionallight.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Point, spot and directional lights
  A light has a colour, a brightness that multiplies it and an ambient colour
  that reaches every surface. A point light shines from where it is in every
  direction and fades with distance as constantFade + linearFade × 0.01 × d +
  quadraticFade × 0.0001 × d². A spot light fades the same way and shines
  only inside its cone: coneAngle is the cone from edge to edge, and the light
  starts to fall off at innerConeAngle. A directional light does not fade.
  Many lights light one scene, and their light adds up.

  # A white surface faces the camera, which looks at it straight on through
  # an orthographic projection: one scene unit is one pixel and the view's
  # centre is the scene's origin. A light straight in front of that point
  # meets the surface head on there, so the pixel shows what the light gives
  # after its fade. Lights not under test have a brightness of 0. The
  # expected values take a surface facing a white light of brightness 1 to
  # show its own colour, as the directional light of view3d.feature does.
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
          PointLight { objectName: "lamp"; z: 10 }
          PointLight { objectName: "lamp2"; z: 10; brightness: 0; color: "#00ff00" }
          SpotLight { objectName: "spot"; z: 100; brightness: 0 }
          DirectionalLight { objectName: "sun"; brightness: 0 }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(4, 4, 1)
            materials: DefaultMaterial { diffuseColor: "white" }
          }
        }
      }
      """

  Rule: The lights start from the values the documentation gives

    Scenario: A new point light and spot light
      Then the property "constantFade" of "lamp" is 1
      And the property "linearFade" of "lamp" is 0
      And the property "quadraticFade" of "lamp" is 1
      And the property "constantFade" of "spot" is 1
      And the property "linearFade" of "spot" is 0
      And the property "quadraticFade" of "spot" is 1
      And the property "coneAngle" of "spot" is 40
      And the property "innerConeAngle" of "spot" is 30

  Rule: A point light fades with distance

    # 1 / (1 + 0.0001 × 10²) = 0.99
    Scenario: A point light close to a surface lights it fully
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"

    # 1 / (1 + 0.0001 × 1000²) = 0.0099, which is 25 in sRGB.
    Scenario: A point light far away hardly lights it
      When the property "z" of "lamp" is set to 1000
      Then the pixel at 100, 100 of the window is within 30 of "#000000"

    Scenario: Without a quadratic fade distance does not dim a point light
      When the property "quadraticFade" of "lamp" is set to 0
      And the property "z" of "lamp" is set to 1000
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"

    # 1 / (1 + 1 × 0.01 × 100) = 0.5, which is 188 in sRGB.
    Scenario: The linear fade is a hundredth per unit of distance
      When the property "quadraticFade" of "lamp" is set to 0
      And the property "linearFade" of "lamp" is set to 1
      And the property "z" of "lamp" is set to 100
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

    Scenario: The constant fade divides the light at any distance
      When the property "quadraticFade" of "lamp" is set to 0
      And the property "constantFade" of "lamp" is set to 2
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

  Rule: A light's colour and brightness are what it gives

    Scenario: A red light makes a white surface red
      When the property "color" of "lamp" is set to "red"
      Then the pixel at 100, 100 of the window is within 4 of "#ff0000"

    Scenario: A light of brightness 0 gives nothing
      When the property "brightness" of "lamp" is set to 0
      Then the pixel at 100, 100 of the window is within 1 of "#000000"

    Scenario: A directional light at half brightness gives half the light
      When the property "brightness" of "lamp" is set to 0
      And the property "brightness" of "sun" is set to 0.5
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"

    # The light shines away from the surface, which only its ambient colour
    # reaches.
    Scenario: A light's ambient colour reaches a surface it does not shine on
      When the property "brightness" of "lamp" is set to 0
      And the property "brightness" of "sun" is set to 1
      And the property "eulerRotation.y" of "sun" is set to 180
      And the property "ambientColor" of "sun" is set to "#808080"
      Then the pixel at 100, 100 of the window is within 2 of "#808080"

  Rule: The light of many lights adds up

    Scenario: A red and a green light make a white surface yellow
      When the property "color" of "lamp" is set to "red"
      And the property "brightness" of "lamp2" is set to 1
      Then the pixel at 100, 100 of the window is within 4 of "#ffff00"

  Rule: A spot light shines inside its cone

    # The spot is 100 units in front of the surface and shines at it. Its
    # cone of 40° reaches 36 units from the centre and starts to fall off
    # 27 units out, at 30°.
    Background:
      When the property "brightness" of "lamp" is set to 0
      And the property "brightness" of "spot" is set to 1
      And the property "quadraticFade" of "spot" is set to 0

    Scenario: The middle of the cone is lit
      Then the pixel at 100, 100 of the window is within 4 of "#ffffff"

    # 20 units out the light meets the surface at 11°: cos 11° = 0.98.
    Scenario: Inside the inner cone the spot lights like a point light
      Then the pixel at 120, 100 of the window is within 6 of "#fdfdfd"

    Scenario: Outside the cone the spot gives nothing
      Then the pixel at 160, 100 of the window is within 1 of "#000000"
      And the pixel at 100, 40 of the window is within 1 of "#000000"

    # 60 units out the light meets the surface at 31°: cos 31° = 0.86.
    Scenario: A wider cone reaches further
      When the property "coneAngle" of "spot" is set to 100
      And the property "innerConeAngle" of "spot" is set to 100
      Then the pixel at 160, 100 of the window is within 6 of "#eeeeee"

    # An inner cone wider than the cone is taken as the cone itself.
    Scenario: An inner cone wider than the cone is the cone
      When the property "innerConeAngle" of "spot" is set to 90
      Then the pixel at 160, 100 of the window is within 1 of "#000000"
      And the pixel at 120, 100 of the window is within 6 of "#fdfdfd"

    # 1 / (1 + 0.0001 × 100²) = 0.5
    Scenario: A spot light fades as a point light does
      When the property "quadraticFade" of "spot" is set to 1
      Then the pixel at 100, 100 of the window is within 4 of "#bcbcbc"
