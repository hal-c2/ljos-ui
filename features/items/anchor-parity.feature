# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.fill-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors.centerIn-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#x-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#width-prop
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

  Scenario: Changing width on a right-anchored item updates x before widthChanged finishes
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property string log: ""
        property bool rec: false
        Item {
          id: box; objectName: "box"; width: 50; height: 20
          anchors.right: parent.right
          onXChanged: if (root.rec) root.log += "x(" + x + "," + width + ");"
          onWidthChanged: if (root.rec) root.log += "w(" + x + "," + width + ");"
        }
        function resize() {
          rec = true
          box.width = 80
        }
      }
      """
    When the method "resize" of "root" is called
    Then the property "log" of "root" is "x(120,80);w(120,80);"

  Scenario: Moving the left anchor target of a left-and-right anchored item updates both x and width
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property string log: ""
        property bool rec: false
        Item { id: sib; x: 0; width: 40; height: 20 }
        Item {
          id: box; objectName: "box"; height: 20
          anchors.left: sib.right; anchors.right: parent.right
          onXChanged: if (root.rec) root.log += "x(" + x + "," + width + ");"
          onWidthChanged: if (root.rec) root.log += "w(" + x + "," + width + ");"
        }
        function moveSib() {
          rec = true
          sib.width = 70
        }
      }
      """
    When the method "moveSib" of "root" is called
    Then the property "log" of "root" is "w(40,130);x(70,130);"
