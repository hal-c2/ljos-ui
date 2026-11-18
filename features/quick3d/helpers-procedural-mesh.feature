# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduralmesh.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-proceduralmeshsubset.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html
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
        property bool lines: false; property bool pair: false
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            x: 0.5; y: -0.5
            geometry: ProceduralMesh {
              objectName: "mesh"
              primitiveMode: scene.lines ? ProceduralMesh.Lines : ProceduralMesh.Triangles
              indexes: scene.pair ? [0, 1] : []
              positions: [Qt.vector3d(-40, -40, 0), Qt.vector3d(40, -40, 0), Qt.vector3d(0, 40, 0)]
            }
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red"; cullMode: DefaultMaterial.NoCulling }
          }
        }
      }
      """

  Scenario: Three positions are a triangle
    Then the pixel at 100, 110 of the window is "#ff0000"
    And the pixel at 100, 65 of the window is "#ff0000"
    And the pixel at 50, 60 of the window is "#1e1e1e"
    And the pixel at 100, 145 of the window is "#1e1e1e"

  Scenario: The primitive mode makes them lines
    When the property "lines" of "scene" is set to true
    And the property "pair" of "scene" is set to true
    Then the pixel at 100, 140 of the window is "#ff0000"
    And the pixel at 100, 110 of the window is "#1e1e1e"

  Scenario: Fewer positions draw less
    When the property "pair" of "scene" is set to true
    Then the pixel at 100, 110 of the window is "#1e1e1e"

  Rule: A subset is drawn with the material of its place

    Scenario: Two subsets, two materials
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            Model {
              geometry: ProceduralMesh {
                positions: [
                  Qt.vector3d(-80, -40, 0), Qt.vector3d(-10, -40, 0), Qt.vector3d(-45, 40, 0),
                  Qt.vector3d(10, -40, 0), Qt.vector3d(80, -40, 0), Qt.vector3d(45, 40, 0)
                ]
                subsets: [ProceduralMeshSubset { offset: 0; length: 3 }, ProceduralMeshSubset { offset: 3; length: 3 }]
              }
              materials: [
                DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red"; cullMode: DefaultMaterial.NoCulling },
                DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "blue"; cullMode: DefaultMaterial.NoCulling }
              ]
            }
          }
        }
        """
      Then the pixel at 55, 110 of the window is "#ff0000"
      And the pixel at 145, 110 of the window is "#0000ff"

  Rule: uv0s are the texture coordinates, uv1s are kept

    # The texture's left half is red and its right half green, so a mesh
    # whose every vertex is at u 0.75 is green wherever its v falls.
    Background:
      Given a 64 by 64 image file "halves.png" of quadrants "#ff0000, #00ff00, #ff0000, #00ff00"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          id: scene; objectName: "scene"; width: 200; height: 200; color: "black"
          property int second: mesh.uv1s.length
          View3D {
            anchors.fill: parent
            OrthographicCamera { z: 300 }
            Model {
              geometry: ProceduralMesh {
                id: mesh
                positions: [Qt.vector3d(-40, -40, 0), Qt.vector3d(40, -40, 0), Qt.vector3d(0, 40, 0)]
                uv0s: [Qt.vector2d(0.75, 0.25), Qt.vector2d(0.75, 0.5), Qt.vector2d(0.75, 0.75)]
                uv1s: [Qt.vector2d(0, 0), Qt.vector2d(1, 0), Qt.vector2d(0, 1)]
              }
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; cullMode: DefaultMaterial.NoCulling
                diffuseMap: Texture { source: "halves.png" } }
            }
          }
        }
        """

    Scenario: A mesh given uv0s is textured where they point
      Then the pixel at 100, 110 of the window is within 2 of "#00ff00"

    Scenario: The second texture coordinates are kept
      Then the property "second" of "scene" is 3
