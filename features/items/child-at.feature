# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#childAt-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Finding the child under a point
  Item.childAt(x, y) gives the visible child of the item under a point in
  the item's own coordinates, through the child's transform, or null when
  there is none. The last of the children under it wins, whatever their z;
  a visible child that is fully transparent counts. QML that hit-tests walks down from a container this way.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string hit: ""
        function probe(x, y) {
          const c = root.childAt(x, y)
          hit = c === null ? "null" : c.objectName
        }
        Rectangle { objectName: "raised"; x: 60; y: 0; width: 30; height: 30; z: 1 }
        Rectangle { objectName: "under"; x: 0; y: 0; width: 100; height: 100 }
        Rectangle { objectName: "over"; x: 50; y: 50; width: 100; height: 100
          Rectangle { objectName: "grandchild"; width: 10; height: 10 }
        }
        Rectangle { objectName: "hidden"; x: 150; y: 0; width: 50; height: 50; visible: false }
        Rectangle { objectName: "clear"; x: 0; y: 150; width: 40; height: 40; opacity: 0 }
        Rectangle { objectName: "scaled"; x: 150; y: 150; width: 20; height: 20; scale: 2 }
      }
      """

  Scenario Outline: The child under a point
    When the method "probe" of "root" is called with <x>, <y>
    Then the property "hit" of "root" is "<child>"

    Examples:
      | x   | y   | child  |
      | 10  | 10  | under  |
      | 75  | 75  | over   |
      | 55  | 55  | over   |
      | 120 | 20  | null   |
      | 160 | 10  | null   |
      | 10  | 160 | clear  |
      | 160 | 160 | scaled |
      | 145 | 145 | scaled |
      | 185 | 185 | null   |
      | 175 | 175 | scaled |
      | 100 | 100 | over   |
      | 150 | 150 | scaled |
      | -1  | 10  | null   |
      | 70  | 10  | under  |
