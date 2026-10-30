# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-runtimeloader.html
#   https://doc.qt.io/qt-6/qtquick3d-runtimeloader-example.html
#   https://doc.qt.io/qt-6/qml-pickresult.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Picking a model a RuntimeLoader loaded
  A RuntimeLoader's content is created as its children, so each of the
  file's meshes is a Model a pick can hit, named after the file's node. A
  pick's objectHit is that Model. The documentation does not say whether
  loaded Models are pickable; here they are, as there is no other way to
  pick what a file holds.

  # pair.gltf, made by features/quick3d/models/gen.py: a square 40 units
  # across, node "left", 50 units left, and the same mesh at half the size,
  # node "right", 50 units right. An orthographic camera at magnification 1
  # shows one scene unit as one pixel, so the view's centre is the scene's
  # origin.
  Background:
    Given the model file "pair.gltf"
    And the model file "pair.bin"
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        objectName: "root"; width: 200; height: 200; color: "black"
        property string hit: ""
        property var result
        function pickAt(x, y) {
          result = view.pick(x, y)
          hit = result.objectHit ? result.objectHit.objectName : "nothing"
        }
        function pickRay(x) {
          result = view.rayPick(Qt.vector3d(x, 20, 300), Qt.vector3d(0, 0, -1))
          hit = result.objectHit ? result.objectHit.objectName : "nothing"
        }
        View3D {
          id: view; anchors.fill: parent
          OrthographicCamera { z: 300 }
          RuntimeLoader { objectName: "loader"; source: "pair.gltf"; y: 20 }
        }
      }
      """

  Rule: A pick hits the loaded mesh under the point

    Scenario: A pick names the file's node it hit
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 50, 80
      Then the property "hit" of "root" is "left"
      And the property "result.hitType" of "root" is 1
      And the property "result.distance" of "root" is within 0.01 of 300
      And the property "result.scenePosition.y" of "root" is within 0.01 of 20

    Scenario: A pick on the smaller square names it
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 150, 80
      Then the property "hit" of "root" is "right"

    Scenario: A ray from the scene hits a loaded mesh
      When 100 milliseconds pass
      And the method "pickRay" of "root" is called with 50
      Then the property "hit" of "root" is "right"

    Scenario: Beside the meshes nothing is picked
      When 100 milliseconds pass
      And the method "pickAt" of "root" is called with 100, 80
      Then the property "hit" of "root" is "nothing"

    Scenario: Moving the loader moves where its meshes are picked
      When 100 milliseconds pass
      And the property "y" of "loader" is set to -20
      And the method "pickAt" of "root" is called with 50, 120
      Then the property "hit" of "root" is "left"
