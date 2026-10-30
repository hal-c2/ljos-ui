# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-perspectivecamera.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-orthographiccamera.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Picking through each kind of camera
  A pick goes through the point of the view the camera draws there, so a
  model is picked exactly where it is drawn, whatever the camera.

  # A cube 40 units across at the origin, in a view twice as wide as it is
  # tall. The perspective camera is 300 units away with a 30° field of
  # view: taken vertically, the cube's front face is 13 pixels either side
  # of the middle; taken horizontally, 27. The orthographic camera draws
  # one unit a pixel, so the cube is 20 pixels either side of the middle.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        id: view; objectName: "view"; width: 200; height: 100
        property bool ortho: false
        property string hit: ""
        function pickAt(x, y) {
          const r = view.pick(x, y)
          hit = r.objectHit ? r.objectHit.objectName : "nothing"
        }
        camera: ortho ? flat : deep
        PerspectiveCamera { id: deep; objectName: "deep"; z: 300; fieldOfView: 30 }
        OrthographicCamera { id: flat; z: 300 }
        Model {
          objectName: "cube"; source: "#Cube"; pickable: true
          scale: Qt.vector3d(0.4, 0.4, 0.4)
        }
      }
      """

  Rule: A pick follows the camera's projection

    Scenario: A vertical field of view
      When the method "pickAt" of "view" is called with 85, 50
      Then the property "hit" of "view" is "nothing"
      When the method "pickAt" of "view" is called with 100, 50
      Then the property "hit" of "view" is "cube"

    Scenario: A horizontal field of view
      When the property "fieldOfViewOrientation" of "deep" is set to 1
      And the method "pickAt" of "view" is called with 85, 50
      Then the property "hit" of "view" is "cube"

    Scenario: An orthographic camera
      When the property "ortho" of "view" is set to true
      And the method "pickAt" of "view" is called with 85, 50
      Then the property "hit" of "view" is "cube"
      When the method "pickAt" of "view" is called with 75, 50
      Then the property "hit" of "view" is "nothing"
