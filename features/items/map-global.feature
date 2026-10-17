# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#mapToGlobal-method
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#mapFromGlobal-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Item.mapToGlobal @api-QtQuick.Item.mapFromGlobal
Feature: An item maps points to and from the desktop
  `mapToGlobal` gives a point in an item's coordinates on the desktop: where
  it is in the window, moved by where the window is. `mapFromGlobal` goes
  back. Both take `x, y` or one `point`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "win"
        x: 100; y: 50
        width: 200; height: 200
        visible: true
        property point out: Qt.point(-1, -1)
        property point back: Qt.point(-1, -1)
        function toGlobal() { out = box.mapToGlobal(5, 6) }
        function toGlobalPoint() { out = box.mapToGlobal(Qt.point(5, 6)) }
        function fromGlobal() { back = box.mapFromGlobal(Qt.point(135, 76)) }
        Item {
          x: 10; y: 10; width: 100; height: 100
          Item { id: box; x: 20; y: 10; width: 50; height: 50 }
        }
      }
      """

  Scenario: A point in an item is on the desktop where its window puts it
    When the method "toGlobal" of "win" is called
    Then the property "out.x" of "win" is 135
    And the property "out.y" of "win" is 76

  Scenario: The point may be given as one point
    When the method "toGlobalPoint" of "win" is called
    Then the property "out.x" of "win" is 135
    And the property "out.y" of "win" is 76

  Scenario: A point on the desktop maps back into the item
    When the method "fromGlobal" of "win" is called
    Then the property "back.x" of "win" is 5
    And the property "back.y" of "win" is 6
