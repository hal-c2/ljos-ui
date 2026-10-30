# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html#pick-method
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; the
# scoping is the same as for a 2D item's own methods
# (features/language/unqualified-native-methods.feature, checked on qml6).

Feature: A view picks from its own scope
  pick(x, y) is View3D's own method, so a function, handler or child inside
  the view calls it by name without the view's id, and it works as the
  view completes, before any frame is drawn.

  # A cube 100 units across at the origin, the camera 300 units away on +Z:
  # the middle of a 100 by 100 view is on the cube.

  Scenario: A view's own function picks by name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        objectName: "root"; width: 100; height: 100
        property string hit: ""
        PerspectiveCamera { z: 300 }
        Model { objectName: "cube"; source: "#Cube"; pickable: true; materials: DefaultMaterial {} }
        function probe() {
          const r = pick(50, 50)
          hit = r.objectHit ? r.objectHit.objectName : "nothing"
        }
      }
      """
    When the method "probe" of "root" is called
    Then the property "hit" of "root" is "cube"

  Scenario: A nested view's own function picks by name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property string hit: ""
        View3D {
          objectName: "view"; anchors.fill: parent
          PerspectiveCamera { z: 300 }
          Model { objectName: "cube"; source: "#Cube"; pickable: true; materials: DefaultMaterial {} }
          function probe() {
            const r = pick(50, 50)
            hit = r.objectHit ? r.objectHit.objectName : "nothing"
          }
        }
      }
      """
    When the method "probe" of "view" is called
    Then the property "hit" of "root" is "cube"

  Scenario: A handler of an item inside the view picks by name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        objectName: "root"; width: 100; height: 100
        property string hit: ""
        PerspectiveCamera { z: 300 }
        Model { objectName: "cube"; source: "#Cube"; pickable: true; materials: DefaultMaterial {} }
        MouseArea {
          anchors.fill: parent
          onClicked: (m) => {
            const r = pick(m.x, m.y)
            hit = r.objectHit ? r.objectHit.objectName : "nothing"
          }
        }
      }
      """
    When the user presses the pointer at 50, 50
    And the user releases the pointer at 50, 50
    Then the property "hit" of "root" is "cube"

  Scenario: An anchored view picks as it completes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        objectName: "root"; width: 100; height: 100
        property string hit: ""
        View3D {
          anchors.fill: parent
          PerspectiveCamera { z: 300 }
          Model { objectName: "cube"; source: "#Cube"; pickable: true; materials: DefaultMaterial {} }
          Component.onCompleted: {
            const r = pick(50, 50)
            hit = r.objectHit ? r.objectHit.objectName : "nothing"
          }
        }
      }
      """
    Then the property "hit" of "root" is "cube"
