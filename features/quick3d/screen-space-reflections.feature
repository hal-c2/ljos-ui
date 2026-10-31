# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssrEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssrMaxSteps-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-extendedsceneenvironment.html#ssrThickness-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Screen-space reflections of ExtendedSceneEnvironment
  An ExtendedSceneEnvironment can let shiny surfaces reflect what the view
  already shows. Reflections are found by following each reflected ray
  across the view's picture, so only what is in view is reflected.
  ssrMaxSteps bounds how far a ray is followed and ssrThickness how thick
  the surfaces it passes behind are taken to be.

  # A camera looks down at a mirror-like metal floor. A red box floats
  # above it; no light falls on the floor, so it is black but for what it
  # reflects. The box shows above the view's middle, its reflection below.
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
          PerspectiveCamera { y: 100; z: 300; eulerRotation.x: -18.43 }
          Model {
            source: "#Rectangle"; eulerRotation.x: -90; scale: Qt.vector3d(6, 6, 1)
            materials: PrincipledMaterial { objectName: "floor"; baseColor: "white"; metalness: 1; roughness: 0 }
          }
          Model {
            source: "#Cube"; y: 50; scale: Qt.vector3d(0.5, 0.5, 0.5)
            materials: DefaultMaterial {
              lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
      }
      """

  Rule: Reflections start off, with the values the documentation gives

    Scenario: A new ExtendedSceneEnvironment
      Then the property "ssrEnabled" of "env" is false
      And the property "ssrMaxSteps" of "env" is 256
      And the property "ssrThickness" of "env" is 20

    Scenario: Without reflections the floor below the box is black
      Then the pixel at 100, 125 of the window is within 2 of "#000000"
      And the pixel at 100, 73 of the window is within 2 of "#ff0000"

  Rule: A shiny floor reflects what is above it

    Scenario: With reflections the box shows in the floor below it
      When the property "ssrEnabled" of "env" is set to true
      Then the pixel at 100, 125 of the window is not "#000000"
      And the pixel at 30, 125 of the window is within 2 of "#000000"
      And the pixel at 100, 73 of the window is within 2 of "#ff0000"

    Scenario: A floor that is not shiny reflects nothing
      When the property "metalness" of "floor" is set to 0
      And the property "baseColor" of "floor" is set to "black"
      And the property "roughness" of "floor" is set to 1
      And the property "ssrEnabled" of "env" is set to true
      Then the pixel at 100, 125 of the window is within 2 of "#000000"

  Rule: A still scene with reflections does no work

    Scenario: Nothing is scheduled once the reflections are drawn
      When the property "ssrEnabled" of "env" is set to true
      Then the pixel at 100, 125 of the window is not "#000000"
      And the application has nothing scheduled
