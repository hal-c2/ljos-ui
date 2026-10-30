# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduralmesh.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduralmeshsubset.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A mesh made from lists
  A ProceduralMesh takes its vertices from the list positions, and joins
  them as the primitiveMode says, in the order of indexes (in the order
  they are listed when there are none).

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Rectangle {
        id: scene; objectName: "scene"
        property bool lines: false
        property bool pair: false
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            clearColor: "#202020"; backgroundMode: SceneEnvironment.Color
          }
          OrthographicCamera { z: 300 }
          Model {
            x: 0.5; y: -0.5
            geometry: ProceduralMesh {
              objectName: "mesh"
              primitiveMode: scene.lines ? ProceduralMesh.Lines : ProceduralMesh.Triangles
              indexes: scene.pair ? [0, 1] : []
              positions: [Qt.vector3d(-40, -40, 0), Qt.vector3d(40, -40, 0), Qt.vector3d(0, 40, 0)]
            }
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
              cullMode: DefaultMaterial.NoCulling
            }
          }
        }
      }
      """

  Scenario: Three positions are a triangle
    Then the pixel at 100, 110 of the window is "#ff0000"
    And the pixel at 100, 65 of the window is "#ff0000"
    And the pixel at 50, 60 of the window is "#202020"
    And the pixel at 100, 145 of the window is "#202020"

  Scenario: The primitive mode makes them lines
    When the property "lines" of "scene" is set to true
    And the property "pair" of "scene" is set to true
    Then the pixel at 100, 140 of the window is "#ff0000"
    And the pixel at 100, 110 of the window is "#202020"

  Scenario: Fewer positions draw less
    When the property "pair" of "scene" is set to true
    Then the pixel at 100, 110 of the window is "#202020"
