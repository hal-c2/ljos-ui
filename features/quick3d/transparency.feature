# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-material.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Blended models are drawn back to front
  A model whose material blends is drawn after every opaque model, and
  blended models are drawn from the farthest from the camera to the nearest,
  whatever order they are declared in, so each is laid over what is behind
  it.

  # Two half-opaque squares overlap in front of a black background, the red
  # one declared first. The camera is orthographic and sees them straight on.
  # Whether colours blend in linear light or in sRGB is not documented, so
  # each expected colour takes either: red laid over blue is #800040 one way
  # and #bc0089 the other, and blue over red is #400080 or #8900bc.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          Model {
            objectName: "red"; source: "#Rectangle"; z: 50
            materials: PrincipledMaterial {
              lighting: PrincipledMaterial.NoLighting
              baseColor: "red"; opacity: 0.5
            }
          }
          Model {
            objectName: "blue"; source: "#Rectangle"; z: -50
            materials: PrincipledMaterial {
              lighting: PrincipledMaterial.NoLighting
              baseColor: "blue"; opacity: 0.5
            }
          }
        }
      }
      """

  Rule: The nearer blended model is laid over the farther one

    Scenario: A nearer model declared first is laid over the one behind it
      Then the pixel at 100, 100 of the window is within 40 of "#9e0066"

    Scenario: A model moved in front is laid over the other
      When the property "z" of "blue" is set to 100
      Then the pixel at 100, 100 of the window is within 40 of "#66009e"

    Scenario: Where they do not overlap each shows over the background
      When the property "x" of "blue" is set to 80
      Then the pixel at 60, 100 of the window is within 40 of "#9e0000"
      And the pixel at 170, 100 of the window is within 40 of "#00009e"
