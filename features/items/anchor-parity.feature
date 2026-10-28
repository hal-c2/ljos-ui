# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.fill-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.centerIn-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Anchor parity

  Scenario: Clearing anchors.fill reveals a declared anchors.left and anchors.top
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property string log: ""
        Item { id: sib; x: 30; y: 20; width: 40; height: 15 }
        Item {
          id: box; objectName: "box"; width: 50; height: 25
          anchors.fill: parent
          anchors.left: sib.right
          anchors.top: sib.bottom
        }
        function clearFill() {
          log = box.x + "," + box.y + "," + box.width + "," + box.height
          box.anchors.fill = undefined
          log += ";" + box.x + "," + box.y + "," + box.width + "," + box.height
        }
      }
      """
    When the method "clearFill" of "root" is called
    Then the property "log" of "root" is "0,0,200,100;0,0,200,100"

  Scenario: Clearing anchors.centerIn reveals a declared anchors.left
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property string log: ""
        Item {
          id: box; objectName: "box"; width: 40; height: 20
          anchors.centerIn: parent
          anchors.left: parent.left
          anchors.leftMargin: 12
        }
        function clearCenter() {
          log = box.x + "," + box.y
          box.anchors.centerIn = undefined
          log += ";" + box.x + "," + box.y
        }
      }
      """
    When the method "clearCenter" of "root" is called
    Then the property "log" of "root" is "80,40;80,40"
