# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html#opacity-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-lodmanager.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A node's opacity is drawn
  A node's opacity is inherited by its children, and a model below 1 lets
  what is behind it show.

  # The documentation says a node's opacity is inherited; here it multiplies
  # as an Item's does. Whether colours blend in linear light or in sRGB is
  # not documented, so a half-opaque white over black takes #808080 or
  # #bcbcbc, and each expected colour lies within reach of both.
  Background:
    Given the file "Square.qml":
      """qml
      import QtQuick3D
      Model {
        property color colour: "white"
        property int mode: PrincipledMaterial.Default
        source: "#Rectangle"
        materials: PrincipledMaterial {
          lighting: PrincipledMaterial.NoLighting
          baseColor: colour; alphaMode: mode
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "black"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          Square { objectName: "behind"; colour: "blue"; z: -50; visible: false }
          Node {
            objectName: "parentNode"
            Square { objectName: "square" }
          }
        }
      }
      """

  Rule: Below 1 a model blends

    Scenario: At 1 a model is drawn whole
      Then the pixel at 100, 100 of the window is "#ffffff"

    Scenario: Below 1 what is behind shows through
      When the property "opacity" of "square" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#9e9e9e"

    Scenario: A blue square shows through a half-opaque white one
      When the property "visible" of "behind" is set to true
      And the property "opacity" of "square" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#9e9eff"

    Scenario: Changing the opacity again draws it again
      When the property "opacity" of "square" is set to 0.5
      And the property "opacity" of "square" is set to 1
      Then the pixel at 100, 100 of the window is "#ffffff"

    Scenario: At 0 a model is not drawn
      When the property "opacity" of "square" is set to 0
      Then the pixel at 100, 100 of the window is "#000000"

    # 1 is PrincipledMaterial.Mask and 3 PrincipledMaterial.Opaque.
    Scenario Outline: A material that asks for no blending still fades
      When the property "mode" of "square" is set to <mode>
      And the property "opacity" of "square" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#9e9e9e"

      Examples:
        | mode |
        | 1    |
        | 3    |

  Rule: Children inherit it

    Scenario: A parent's opacity reaches its child
      When the property "opacity" of "parentNode" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#9e9e9e"

    Scenario: A parent's and a child's multiply
      When the property "opacity" of "parentNode" is set to 0.5
      And the property "opacity" of "square" is set to 0.5
      Then the pixel at 100, 100 of the window is within 40 of "#646464"

    Scenario: A parent at 0 hides a child at 1
      When the property "opacity" of "parentNode" is set to 0
      Then the pixel at 100, 100 of the window is "#000000"

    Scenario: A loader's opacity reaches the file's models
      Given the model file "square.gltf"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment {
              clearColor: "black"; backgroundMode: SceneEnvironment.Color
            }
            OrthographicCamera { z: 300 }
            RuntimeLoader { source: "square.gltf"; opacity: 0.5 }
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is within 40 of "#9e0000"

  Rule: The Helpers that set an opacity are seen to

    # At 425 the fade is a quarter through: the near level at 0.25, the middle
    # at 0.75, one laid over the other on the dark background.
    Scenario: Across a LodManager's distance both levels show, neither whole
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 100; height: 100; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment {
              clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
            }
            PerspectiveCamera { id: cam; z: 425 }
            LodManager {
              camera: cam; distances: [400, 800]; fadeDistance: 100
              Square { source: "#Cube"; colour: "red" }
              Square { source: "#Cube"; colour: "lime" }
              Square { source: "#Cube"; colour: "blue" }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 30 of "#2fd208"

    Scenario: An AxisHelper's gridOpacity dims its grid
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
              clearColor: "black"; backgroundMode: SceneEnvironment.Color
            }
            OrthographicCamera {
              x: -0.125; y: 0.125; z: 300
              horizontalMagnification: 4; verticalMagnification: 4
            }
            AxisHelper {
              enableAxisLines: false; enableXYGrid: true
              gridColor: "#00ff00"; gridOpacity: 0.5
            }
          }
        }
        """
      Then the pixel at 140, 120 of the window is within 40 of "#009e00"
