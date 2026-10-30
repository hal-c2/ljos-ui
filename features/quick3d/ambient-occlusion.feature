# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#aoStrength-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html#aoEnabled-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Screen-space ambient occlusion
  A SceneEnvironment darkens the places where surfaces come close to each
  other, as light from all around reaches them less. aoStrength says how
  dark, up to full darkness at 100, and 0 turns it off; aoDistance says how
  far the shadow spreads. Turning aoEnabled on gives a strength or a
  distance that is 0 a value that makes the occlusion show.

  # A white wall stands 10 units behind a white box that faces the camera;
  # a light from the camera's side lights both fully.
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
            materials: DefaultMaterial { diffuseColor: "white" }
          }
        }
      }
      """

  Rule: Ambient occlusion starts off

    Scenario: A new SceneEnvironment
      Then the property "aoEnabled" of "env" is false
      And the property "aoStrength" of "env" is 0
      And the property "aoDistance" of "env" is 5
      And the property "aoSoftness" of "env" is 50
      And the property "aoSampleRate" of "env" is 2
      And the property "aoBias" of "env" is 0
      And the property "aoDither" of "env" is false

    Scenario: Without ambient occlusion the wall is evenly lit
      Then the pixel at 45, 100 of the window is within 2 of "#ffffff"

  Rule: Ambient occlusion darkens a surface near another one in front of it

    Scenario: Enabling it gives it a strength and darkens the wall by the box
      When the property "aoDistance" of "env" is set to 50
      And the property "aoEnabled" of "env" is set to true
      Then the property "aoStrength" of "env" is 100
      And the pixel at 45, 100 of the window is not "#ffffff"
      And the pixel at 5, 5 of the window is within 2 of "#ffffff"

    Scenario: A strength of 0 turns it off again
      When the property "aoDistance" of "env" is set to 50
      And the property "aoEnabled" of "env" is set to true
      And the property "aoStrength" of "env" is set to 0
      Then the pixel at 45, 100 of the window is within 2 of "#ffffff"
