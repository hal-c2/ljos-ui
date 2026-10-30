# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Multisample antialiasing
  A SceneEnvironment with antialiasingMode MSAA smooths the edges of models:
  a pixel an edge crosses is part model and part what is behind it. The
  antialiasingQuality says how many samples a pixel has: Medium 2, High 4
  (the default) and VeryHigh 8.

  # A square, turned 45° and shrunk to 80 units corner to corner, is a red
  # diamond seen through an orthographic camera, one unit to a pixel. Its
  # lower right edge runs through the middle of the pixel at 70, 69, which
  # two or four samples see half covered. Whether the samples are averaged
  # in linear light or in sRGB is not documented: half red is #800000 one
  # way and #bc0000 the other, and the tolerance takes either.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; clearColor: "black"
            backgroundMode: SceneEnvironment.Color
            antialiasingMode: SceneEnvironment.MSAA
          }
          OrthographicCamera { z: 300 }
          Model {
            source: "#Rectangle"; eulerRotation.z: 45
            scale: Qt.vector3d(0.565685, 0.565685, 1)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: An edge pixel is part model and part background

    Scenario: Four samples to a pixel by default
      Then the property "antialiasingQuality" of "env" is 1
      And the pixel at 70, 69 of the window is within 70 of "#9e0000"
      And the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 90, 90 of the window is "#000000"

    Scenario: Medium quality smooths the edge too
      When the property "antialiasingQuality" of "env" is set to 0
      Then the pixel at 70, 69 of the window is within 70 of "#9e0000"
      And the pixel at 50, 50 of the window is within 2 of "#ff0000"
