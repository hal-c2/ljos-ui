# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssgiEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssgiIndirectLightEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssgiSampleRadius-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Screen-space global illumination of ExtendedSceneEnvironment
  An ExtendedSceneEnvironment can light surfaces with the light other
  surfaces in view throw back at them, and darken the places other
  surfaces hide from light all around. Only what the view shows gives
  light. With ssgiIndirectLightEnabled off only the darkening is left,
  like ambient occlusion.

  # A red box stands on a white floor, seen from the front and above, with
  # the light coming from behind the camera. The floor at the box's foot is
  # beside the box's lit front.
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
          environment: ExtendedSceneEnvironment {
            objectName: "env"
            clearColor: "black"; backgroundMode: SceneEnvironment.Color
          }
          PerspectiveCamera { y: 200; z: 300; eulerRotation.x: -33.7 }
          DirectionalLight { eulerRotation.x: -45 }
          Model {
            source: "#Rectangle"; eulerRotation.x: -90; scale: Qt.vector3d(8, 8, 1)
            materials: DefaultMaterial { diffuseColor: "white" }
          }
          Model {
            source: "#Cube"; y: 50
            materials: DefaultMaterial { diffuseColor: "red" }
          }
        }
      }
      """

  Rule: Global illumination starts off, with the values the documentation gives

    Scenario: A new ExtendedSceneEnvironment
      Then the property "ssgiEnabled" of "env" is false
      And the property "ssgiBufferSizeFactor" of "env" is 0.5
      And the property "ssgiHitThickness" of "env" is 0.5
      And the property "ssgiIndirectLightBoost" of "env" is 4
      And the property "ssgiIndirectLightEnabled" of "env" is true
      And the property "ssgiSampleCount" of "env" is 4
      And the property "ssgiSampleRadius" of "env" is 0.1
      And the property "ssgiSimulatedBounceEnabled" of "env" is false
      And the property "ssgiSimulatedBounceFactor" of "env" is 0.5
      And the property "ssgiSliceCount" of "env" is 4

    Scenario: Without it the floor is evenly lit
      Then the pixel at 100, 120 of the window is within 2 of "#dbdbdb"
      And the pixel at 100, 190 of the window is within 2 of "#dbdbdb"

  Rule: The box throws its red light onto the floor at its foot

    Scenario: With it the floor by the box turns red
      When the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 100, 120 of the window is within 8 of "#ffc2c2"
      And the pixel at 100, 190 of the window is within 3 of "#dbdbdb"

  Rule: Without the indirect light only the darkening is left

    Scenario: The floor by the box only darkens
      When the property "ssgiIndirectLightEnabled" of "env" is set to false
      And the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 100, 120 of the window is within 8 of "#c2c2c2"
      And the pixel at 100, 190 of the window is within 3 of "#dbdbdb"

  Rule: A still scene with global illumination does no work

    Scenario: Nothing is scheduled once it is drawn
      When the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 100, 120 of the window is within 8 of "#ffc2c2"
      And the application has nothing scheduled
