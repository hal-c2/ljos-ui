# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-reflectionprobe.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A reflection probe
  A ReflectionProbe draws what is round it through each side of a cube. A
  model inside its box that says receivesReflections mirrors that in place
  of the environment's light probe; a model that says castsReflections is
  seen in it.

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
            backgroundMode: SceneEnvironment.Color; clearColor: "black"
          }
          PerspectiveCamera { z: 300 }
          ReflectionProbe {
            objectName: "probe"; boxSize: Qt.vector3d(2000, 2000, 2000)
            clearColor: "#ff0000"
          }
          Model {
            objectName: "mirror"; source: "#Rectangle"; scale: Qt.vector3d(2, 2, 1)
            receivesReflections: true
            materials: PrincipledMaterial {
              baseColor: "white"; metalness: 1; roughness: 0
              lighting: PrincipledMaterial.FragmentLighting
            }
          }
        }
      }
      """

  Rule: A model mirrors what is round a probe

    Scenario: A model that receives reflections shows the probe's clear colour
      Then the pixel at 100, 100 of the window is within 40 of "#ff0000"

    Scenario: A model that does not receive them does not
      When the property "receivesReflections" of "mirror" is set to false
      Then the pixel at 100, 100 of the window is within 3 of "#000000"
