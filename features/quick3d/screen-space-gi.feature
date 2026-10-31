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

  # A white wall stands 10 units behind a red box that faces the camera;
  # a light from the camera's side lights both.
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
          OrthographicCamera { z: 600; clipNear: 1; clipFar: 1000 }
          DirectionalLight { brightness: 1 }
          Model {
            source: "#Rectangle"; z: -60; scale: Qt.vector3d(4, 4, 1)
            materials: DefaultMaterial { diffuseColor: "white" }
          }
          Model {
            source: "#Cube"
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

    Scenario: Without it the wall beside the box is evenly lit
      Then the pixel at 45, 100 of the window is within 2 of "#ffffff"
      And the pixel at 10, 100 of the window is within 2 of "#ffffff"

  Rule: The box throws its red light onto the wall beside it

    Scenario: With it the wall by the box turns red
      When the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 45, 100 of the window is not "#ffffff"
      And the pixel at 10, 100 of the window is within 8 of "#ffffff"

  Rule: Without the indirect light only the darkening is left

    Scenario: The wall by the box only darkens
      When the property "ssgiIndirectLightEnabled" of "env" is set to false
      And the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 45, 100 of the window is not "#ffffff"
      And the pixel at 10, 100 of the window is within 8 of "#ffffff"

  Rule: A still scene with global illumination does no work

    Scenario: Nothing is scheduled once it is drawn
      When the property "ssgiEnabled" of "env" is set to true
      Then the pixel at 45, 100 of the window is not "#ffffff"
      And the application has nothing scheduled
