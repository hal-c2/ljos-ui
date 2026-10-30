# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-pickresult.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Picking a model under the pointer
  View3D.pick(x, y) shoots a ray from the camera through a point of the
  view and gives back what it hits first: the model (objectHit), how far
  from the camera, and where and which way the face it hit is. Only a
  model whose pickable is true can be hit, and only where it is drawn.

  # A cube 100 units across at the origin, the camera 300 units away on +Z:
  # the ray through the middle of the view meets the cube's front face at
  # z 50, 250 units from the camera. A click turns the cube blue.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        objectName: "root"; width: 100; height: 100; color: "black"
        property string hit: ""
        property var result
        View3D {
          id: view; anchors.fill: parent
          PerspectiveCamera { z: 300 }
          Model {
            objectName: "cube"; source: "#Cube"; pickable: true
            materials: DefaultMaterial {
              id: paint; lighting: DefaultMaterial.NoLighting; diffuseColor: "red"
            }
          }
        }
        MouseArea {
          anchors.fill: parent
          onClicked: (mouse) => {
            result = view.pick(mouse.x, mouse.y)
            hit = result.objectHit ? result.objectHit.objectName : "nothing"
            if (result.objectHit) paint.diffuseColor = "blue"
          }
        }
      }
      """

  Rule: A pick finds the pickable model the ray meets first

    Scenario: A click on a pickable model picks it
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "hit" of "root" is "cube"
      And the property "result.hitType" of "root" is 1
      And the property "result.distance" of "root" is within 0.01 of 250

    Scenario: The pick says where the face was hit and which way it faces
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "result.scenePosition.z" of "root" is within 0.01 of 50
      And the property "result.position.z" of "root" is within 0.01 of 50
      And the property "result.sceneNormal.z" of "root" is within 0.001 of 1
      And the property "result.normal.z" of "root" is within 0.001 of 1
      And the property "result.uvPosition.x" of "root" is within 0.01 of 0.5

    Scenario: A click beside every model picks nothing
      When the user presses the pointer at 5, 5
      And the user releases the pointer at 5, 5
      Then the property "hit" of "root" is "nothing"
      And the property "result.hitType" of "root" is 0

    Scenario: A picked model can change how it is drawn
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the pixel at 50, 50 of the window is within 2 of "#0000ff"

  Rule: Only what is pickable and drawn can be picked

    Scenario: A model that is not pickable is passed over
      When the property "pickable" of "cube" is set to false
      And the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "hit" of "root" is "nothing"

    Scenario: A hidden model is passed over
      When the property "visible" of "cube" is set to false
      And the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "hit" of "root" is "nothing"

    # Moved 100 units right, the cube spans x 50 to 150: the middle of the
    # view now misses it, and a point right of the middle meets it.
    Scenario: A moved model is picked where it is drawn
      When the property "x" of "cube" is set to 100
      And the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "hit" of "root" is "nothing"
      When the user presses the pointer at 90, 50
      And the user releases the pointer at 90, 50
      Then the property "hit" of "root" is "cube"
