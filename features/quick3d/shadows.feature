# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-light.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-directionallight.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Shadows
  A light whose castsShadow is true casts shadows: a model between it and a
  surface keeps its light from that surface. shadowFactor, 0 to 100, says how
  much of the light a shadow keeps away. A model's castsShadows says whether
  it casts shadows and its receivesShadows whether shadows fall on it; both
  are true by default. Directional, point and spot lights all cast shadows.

  # A white wall 400 units across faces the camera, which looks at it straight
  # on through an orthographic projection: one scene unit is one pixel and the
  # view's centre is the scene's origin. A cube 100 across stands 100 in front
  # of the wall's middle. The sun is turned 45° about y, so its light slants
  # towards -x and the cube's shadow falls on the wall from x -200 to 0, partly
  # behind the cube: pixel 25, 100 is in it and pixel 175, 100 is not. A
  # surface meeting a white light at 45° shows 0.71 of its colour, #dbdbdb;
  # a quarter of that, what a shadowFactor of 75 leaves, is #757575.
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
          DirectionalLight { objectName: "sun"; eulerRotation.y: 45 }
          SpotLight { objectName: "spot"; x: 300; z: 300; eulerRotation.y: 45; brightness: 0; quadraticFade: 0; castsShadow: true }
          PointLight { objectName: "lamp"; x: 300; z: 300; brightness: 0; quadraticFade: 0; castsShadow: true }
          Model {
            objectName: "wall"; source: "#Rectangle"; scale: Qt.vector3d(4, 4, 1)
            materials: DefaultMaterial { diffuseColor: "white" }
          }
          Model {
            objectName: "cube"; source: "#Cube"; z: 100
            materials: DefaultMaterial { diffuseColor: "white" }
          }
        }
      }
      """

  Rule: Shadows start from the values the documentation gives

    Scenario: A new light casts no shadow and a new model casts and receives them
      Then the property "castsShadow" of "sun" is false
      And the property "shadowFactor" of "sun" is 75
      And the property "shadowBias" of "sun" is 10
      And the property "shadowMapFar" of "sun" is 5000
      And the property "pcfFactor" of "sun" is 2
      And the property "castsShadows" of "cube" is true
      And the property "receivesShadows" of "wall" is true

    # ShadowMapQualityLow is 0 and PCF4 is 1.
    Scenario: A new light's shadow map is of low quality, softened by four samples
      Then the property "shadowMapQuality" of "sun" is 0
      And the property "softShadowQuality" of "sun" is 1

    Scenario: A new directional light does not split its shadow map
      Then the property "csmNumSplits" of "sun" is 0
      And the property "csmSplit1" of "sun" is 0.1
      And the property "csmSplit2" of "sun" is 0.25
      And the property "csmSplit3" of "sun" is 0.5
      And the property "csmBlendRatio" of "sun" is 0.05
      And the property "lockShadowmapTexels" of "sun" is false

  Rule: Only a light whose castsShadow is true casts shadows

    Scenario: A light that casts no shadow lights what is behind a model
      Then the pixel at 25, 100 of the window is within 4 of "#dbdbdb"

    Scenario: A light that casts shadows keeps its light from behind a model
      When the property "castsShadow" of "sun" is set to true
      Then the pixel at 25, 100 of the window is within 8 of "#757575"
      And the pixel at 175, 100 of the window is within 4 of "#dbdbdb"

    Scenario: A shadow follows the model that casts it
      When the property "castsShadow" of "sun" is set to true
      And the property "y" of "cube" is set to 200
      Then the pixel at 25, 100 of the window is within 4 of "#dbdbdb"

  Rule: shadowFactor says how dark a shadow is

    Scenario Outline: A shadowFactor of <factor> leaves <left> of the light
      When the property "castsShadow" of "sun" is set to true
      And the property "shadowFactor" of "sun" is set to <factor>
      Then the pixel at 25, 100 of the window is within 8 of "<color>"

      Examples:
        | factor | left       | color   |
        | 0      | all        | #dbdbdb |
        | 100    | none       | #000000 |

  Rule: A model's castsShadows and receivesShadows choose what takes part

    Scenario: A model whose castsShadows is false casts no shadow
      When the property "castsShadow" of "sun" is set to true
      And the property "castsShadows" of "cube" is set to false
      Then the pixel at 25, 100 of the window is within 4 of "#dbdbdb"

    Scenario: A model whose receivesShadows is false is lit through a shadow
      When the property "castsShadow" of "sun" is set to true
      And the property "receivesShadows" of "wall" is set to false
      Then the pixel at 25, 100 of the window is within 4 of "#dbdbdb"

  # The surface lit in the open is not darkened by its own shadow map: the
  # bias keeps a lit surface from shadowing itself.
  Rule: A surface the light reaches is not shadowed by itself

    Scenario: The wall beside the shadow is evenly lit
      When the property "castsShadow" of "sun" is set to true
      Then the pixel at 175, 100 of the window is within 4 of "#dbdbdb"
      And the pixel at 150, 30 of the window is within 4 of "#dbdbdb"
      And the pixel at 185, 180 of the window is within 4 of "#dbdbdb"

  # A spot light and a point light stand at x 300, z 300, dark until a
  # scenario lights them; the spot light is turned 45° about y, at the scene's
  # origin. Neither fades. Pixel 25, 100 of the wall meets their light at 51°,
  # which leaves 0.62 of the colour, #cfcfcf, and the cube stands in the way.
  Rule: Point and spot lights cast shadows

    Scenario Outline: A shadowFactor of <factor> on the "<light>" light leaves <left> of its light
      When the property "brightness" of "sun" is set to 0
      And the property "brightness" of "<light>" is set to 1
      And the property "shadowFactor" of "<light>" is set to <factor>
      Then the pixel at 25, 100 of the window is within 8 of "<color>"

      Examples:
        | light | factor | left | color   |
        | spot  | 0      | all  | #cfcfcf |
        | spot  | 100    | none | #000000 |
        | lamp  | 0      | all  | #cfcfcf |
        | lamp  | 100    | none | #000000 |

  # Qt's own event loop cannot be read, so only Ljós is held to this.
  @ljos-only
  Rule: A still scene with shadows costs nothing

    Scenario: Nothing is scheduled while a shadowed scene does not change
      When the property "castsShadow" of "sun" is set to true
      Then the application has nothing scheduled
