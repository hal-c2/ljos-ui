# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qquick3dgeometry-attribute.html
# Ljós only as written: the geometry is declared in QML, which Qt's QML does
# not do (geometry-in-qml.feature). COLOR itself is as Qt documents it.

@ljos-only
Feature: A custom material reads a vertex's colour
  In a CustomMaterial's vertex snippet COLOR is the colour of the vertex, and
  white for a mesh that has no colours.

  Background:
    Given the text file "paint.vert":
      """
      VARYING vec4 paint;
      void MAIN() { paint = COLOR; }
      """
    And the text file "paint.frag":
      """
      VARYING vec4 paint;
      void MAIN() { FRAGCOLOR = paint; }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; tonemapMode: SceneEnvironment.TonemapModeNone }
          OrthographicCamera { z: 300 }
          CustomMaterial { id: paint; shadingMode: CustomMaterial.Unshaded; vertexShader: "paint.vert"; fragmentShader: "paint.frag" }
          Model {
            materials: paint
            geometry: Geometry {
              // Position, then a colour of blue.
              Component.onCompleted: {
                setVertexData(new Float32Array([-50, 0, 0, 0, 0, 1, 1, -10, 0, 0, 0, 0, 1, 1, -30, 50, 0, 0, 0, 1, 1]))
                setStride(28)
                addAttribute(Geometry.PositionSemantic, 0, Geometry.F32Type)
                addAttribute(Geometry.ColorSemantic, 12, Geometry.F32Type)
                update()
              }
            }
          }
          Model {
            materials: paint
            geometry: Geometry {
              Component.onCompleted: {
                setVertexData(new Float32Array([10, 0, 0, 50, 0, 0, 30, 50, 0]))
                setStride(12)
                addAttribute(Geometry.PositionSemantic, 0, Geometry.F32Type)
                update()
              }
            }
          }
        }
      }
      """

  Scenario: A vertex's colour reaches the snippet
    Then the pixel at 70, 90 of the window is "#0000ff"

  Scenario: A mesh with no colours is white
    Then the pixel at 130, 90 of the window is "#ffffff"
