# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extrudedtextgeometry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.
#
# The scenes lay out an "I" in the bundled sans font: its stem is about a
# tenth of an em wide, from 0.1 to 0.19 em, and 0.72 em tall from the
# baseline. One unit is one pixel under the orthographic camera, with the
# view's centre at 100, 100.

@ljos-only
Feature: Text extruded into a mesh
  An ExtrudedTextGeometry is the outlines of its text, in its font, filled
  front and back and joined by side walls along the z axis. An em of text
  is scale units whatever the font's size, which only makes the curves
  smoother; the extrusion is depth units deep whatever the scale. The mesh's
  origin is the rear left end of the text's baseline.

  Rule: Unlit, the mesh is the shape of the text

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          id: scene; objectName: "scene"
          property bool side: false
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera {
              x: scene.side ? 300 : 0; z: scene.side ? 0 : 300
              eulerRotation.y: scene.side ? 90 : 0
            }
            ExtrudedTextGeometry { objectName: "plain" }
            Model {
              x: -14; y: -36
              geometry: ExtrudedTextGeometry {
                objectName: "text"; text: "I"; scale: 100; depth: 50
                font.pointSize: 24
              }
              materials: DefaultMaterial { diffuseColor: "red"; lighting: DefaultMaterial.NoLighting }
            }
          }
        }
        """

    Scenario: A new ExtrudedTextGeometry
      Then the property "text" of "plain" is ""
      And the property "scale" of "plain" is 1
      And the property "depth" of "plain" is 1
      And the property "asynchronous" of "plain" is false

    Scenario: The text's outline is filled
      Then the property "status" of "text" is 2
      And the pixel at 100, 100 of the window is "#ff0000"
      And the pixel at 130, 100 of the window is "#202020"
      And the pixel at 100, 40 of the window is "#202020"

    Scenario: The scale makes the text larger
      When the property "scale" of "text" is set to 200
      Then the pixel at 114, 40 of the window is "#ff0000"

    Scenario: No text is no mesh
      When the property "text" of "text" is set to ""
      Then the pixel at 100, 100 of the window is "#202020"

    Scenario: The text is extruded from the baseline's rear towards the front
      When the property "side" of "scene" is set to true
      Then the pixel at 75, 100 of the window is "#ff0000"
      And the pixel at 55, 100 of the window is "#ff0000"
      And the pixel at 125, 100 of the window is "#202020"
      And the pixel at 45, 100 of the window is "#202020"

    Scenario: The depth is not scaled
      When the property "scale" of "text" is set to 200
      And the property "depth" of "text" is set to 20
      And the property "side" of "scene" is set to true
      Then the pixel at 90, 100 of the window is "#ff0000"
      And the pixel at 70, 100 of the window is "#202020"

  Rule: Lit, the front faces the camera

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
            environment: SceneEnvironment { clearColor: "#000000"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            DirectionalLight { }
            Model {
              x: -14; y: -36
              geometry: ExtrudedTextGeometry { text: "I"; scale: 100; depth: 50 }
              materials: DefaultMaterial { diffuseColor: "white" }
            }
          }
        }
        """

    Scenario: The front faces the light in front of it
      Then the pixel at 100, 100 of the window is within 40 of "#ffffff"
