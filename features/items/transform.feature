# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transform-prop
#   https://doc.qt.io/qt-6/qml-qtquick-rotation.html
#   https://doc.qt.io/qt-6/qml-qtquick-scale.html
#   https://doc.qt.io/qt-6/qml-qtquick-translate.html
#   https://doc.qt.io/qt-6/qml-qtquick-matrix4x4.html
#   https://doc.qt.io/qt-6/qml-qtquick-shear.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Item transforms
  An item's `transform` list moves, turns, scales and shears it after its own `rotation` and `scale`
  are applied: the first transform in the list is applied first.

  Rule: Points are mapped through the list

    Scenario: Each transform of the list is applied after the one before it
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property point at: Qt.point(-1, -1)
          function probe() { at = box.mapToItem(null, 0, 0) }
          Item {
            id: box; x: 10; y: 20; width: 100; height: 50
            transform: [Translate { x: 5 }, Scale { xScale: 2 }]
          }
        }
        """
      When the method "probe" of "root" is called
      Then the property "at.x" of "root" is 20
      And the property "at.y" of "root" is 20

    Scenario: The list is applied after the item's own rotation
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property point at: Qt.point(-1, -1)
          property point back: Qt.point(-1, -1)
          function probe() {
            at = box.mapToItem(null, 10, 0)
            back = box.mapFromItem(null, 50, 80)
          }
          Item {
            id: box; x: 10; y: 20; width: 100; height: 50
            rotation: 90
            transform: [Translate { x: 5 }, Scale { xScale: 2 }]
          }
        }
        """
      When the method "probe" of "root" is called
      Then the property "at.x" of "root" is within 0.001 of 170
      And the property "at.y" of "root" is within 0.001 of 5
      And the property "back.x" of "root" is within 0.001 of 85
      And the property "back.y" of "root" is within 0.001 of 60

    Scenario Outline: One transform of each kind
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property point at: Qt.point(-1, -1)
          function probe() { at = box.mapToItem(null, 10, 20) }
          Item { id: box; width: 100; height: 50; transform: <transform> }
        }
        """
      When the method "probe" of "root" is called
      Then the property "at.x" of "root" is within 0.001 of <x>
      And the property "at.y" of "root" is within 0.001 of <y>

      Examples:
        | transform                                                                | x   | y   |
        | Translate { x: 3; y: -4 }                                                | 13  | 16  |
        | Scale { xScale: 2; yScale: 3 }                                           | 20  | 60  |
        | Scale { origin.x: 10; origin.y: 10; xScale: 2; yScale: 3 }               | 10  | 40  |
        | Rotation { angle: 90 }                                                   | -20 | 10  |
        | Rotation { angle: 90; origin.x: 10; origin.y: 10 }                       | 0   | 10  |
        | Shear { xFactor: 0.5 }                                                   | 20  | 20  |
        | Shear { xAngle: 45; origin.x: 10; origin.y: 10 }                         | 20  | 20  |
        | Shear { yFactor: 1 }                                                     | 10  | 30  |
        | Matrix4x4 { matrix: Qt.matrix4x4(1, 0, 0, 7, 0, 2, 0, 9, 0, 0, 1, 0, 0, 0, 0, 1) } | 17  | 49  |

    Scenario Outline: A turn about the x or y axis is seen in perspective
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property point at: Qt.point(-1, -1)
          function probe() { at = box.mapToItem(null, <px>, <py>) }
          Item {
            id: box; width: 100; height: 100
            transform: Rotation { angle: 60; axis { x: <ax>; y: <ay>; z: 0 } origin.x: 50; origin.y: 50 }
          }
        }
        """
      When the method "probe" of "root" is called
      Then the property "at.x" of "root" is within 0.01 of <x>
      And the property "at.y" of "root" is within 0.01 of <y>

      Examples:
        | ax | ay | px  | py  | x      | y      |
        | 0  | 1  | 0   | 0   | 23.896 | -2.208 |
        | 0  | 1  | 100 | 0   | 73.986 | 2.029  |
        | 0  | 1  | 50  | 50  | 50     | 50     |
        | 1  | 0  | 0   | 100 | -2.208 | 76.104 |

    Scenario: A transform that changes moves the item
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 200; height: 200
          property point at: Qt.point(-1, -1)
          property alias moveX: move.x
          function probe() { at = box.mapToItem(null, 0, 0) }
          Item {
            id: box; width: 10; height: 10
            transform: Translate { id: move; x: 5 }
          }
        }
        """
      When the property "moveX" of "root" is set to 40
      And the method "probe" of "root" is called
      Then the property "at.x" of "root" is 40

    Scenario: A transformed child does not change its parent's childrenRect
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item {
            x: 10; y: 10; width: 20; height: 20
            transform: Scale { xScale: 3; yScale: 2 }
          }
        }
        """
      Then the property "childrenRect.x" of "root" is 10
      And the property "childrenRect.width" of "root" is 20
      And the property "childrenRect.height" of "root" is 20

  Rule: The properties of the transforms

    Scenario: The transforms start as no change at all
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var r: Rotation {}
          property var s: Scale {}
          property var t: Translate {}
          property var h: Shear {}
          property real axisZ: r.axis.z
          property real scaleX: s.xScale
          property real scaleZ: s.zScale
          property real shearX: h.xFactor
          property real shearAngle: h.yAngle
          property real moveX: t.x
          property real angle: r.angle
        }
        """
      Then the property "axisZ" of "root" is 1
      And the property "scaleX" of "root" is 1
      And the property "scaleZ" of "root" is 1
      And the property "shearX" of "root" is 0
      And the property "shearAngle" of "root" is 0
      And the property "moveX" of "root" is 0
      And the property "angle" of "root" is 0

    Scenario: An item lists its transforms
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: box.transform.length
          Item { id: box; transform: [Rotation {}, Scale {}, Translate {}] }
        }
        """
      Then the property "count" of "root" is 3

  Rule: The picture follows the transforms

    Scenario: A translated rectangle is painted where it was moved to
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100; color: "black"
          Rectangle {
            width: 20; height: 20; color: "#ff0000"
            transform: Translate { x: 50; y: 50 }
          }
        }
        """
      Then the pixel at 60, 60 of the window is "#ff0000"
      And the pixel at 10, 10 of the window is "#000000"

    Scenario: A scaled rectangle covers more of the window
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100; color: "black"
          Rectangle {
            width: 20; height: 20; color: "#00ff00"
            transform: Scale { xScale: 4; yScale: 2 }
          }
        }
        """
      Then the pixel at 70, 30 of the window is "#00ff00"
      And the pixel at 70, 50 of the window is "#000000"

  Rule: The pointer finds a transformed item where it is painted

    Scenario: A click lands on a moved item
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 200; height: 200
          MouseArea {
            objectName: "area"
            width: 20; height: 20
            property int clicks: 0
            onClicked: clicks++
            transform: Translate { x: 100; y: 100 }
          }
        }
        """
      When the user presses the pointer at 110, 110
      And the user releases the pointer at 110, 110
      Then the property "clicks" of "area" is 1
