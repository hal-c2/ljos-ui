# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#mapToItem-method
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#mapFromItem-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Coordinate mapping parity

  Scenario: Point and rect mapping between cousins and with a null target
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string log: ""
        Item {
          id: leftBranch; x: 10; y: 20; width: 100; height: 100
          Item { id: a; x: 5; y: 6; width: 30; height: 40 }
        }
        Item {
          id: rightBranch; x: 100; y: 50; width: 100; height: 100
          Item { id: b; x: 15; y: 25; width: 30; height: 40 }
        }
        Component.onCompleted: {
          var p1 = a.mapToItem(b, 3, 4)
          var p2 = b.mapFromItem(a, 3, 4)
          var r = a.mapToItem(b, 3, 4, 20, 30)
          var scene = a.mapToItem(null, 3, 4)
          var fromScene = a.mapFromItem(null, 18, 30)
          log = p1.x + "," + p1.y + ";" + p2.x + "," + p2.y + ";" + r.x + "," + r.y + "," + r.width + "," + r.height + ";" + scene.x + "," + scene.y + ";" + fromScene.x + "," + fromScene.y
        }
      }
      """
    Then the property "log" of "root" is "-97,-45;-97,-45;-97,-45,20,30;18,30;3,4"

  Scenario: Calling mapToItem on the statement immediately after reparenting and re-anchoring
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 300
        property string log: ""
        Item { id: boxA; x: 10; y: 15; width: 100; height: 100 }
        Item { id: boxB; x: 120; y: 80; width: 100; height: 100 }
        Item { id: child; parent: boxA; x: 5; y: 7; width: 20; height: 10 }
        function moveAndMap() {
          var before = child.mapToItem(root, 0, 0)
          child.parent = boxB
          child.anchors.right = boxB.right
          var after = child.mapToItem(root, 0, 0)
          log = before.x + "," + before.y + ";" + after.x + "," + after.y
        }
      }
      """
    When the method "moveAndMap" of "root" is called
    Then the property "log" of "root" is "15,22;200,87"
