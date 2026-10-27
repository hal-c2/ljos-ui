# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-column.html#forceLayout-method
#   https://doc.qt.io/qt-6/qml-qtquick-row.html#forceLayout-method
#   https://doc.qt.io/qt-6/qml-qtquick-grid.html#forceLayout-method
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html#forceLayout-method
# Verified on Qt 6.11.2 (qml6).

@qt-parity
Feature: A positioner lays its children out at once when told to
  Row, Column, Grid and Flow place their children before the next frame.
  `forceLayout()` places them now, so the script that called it reads the
  new places and size on its next line.

  Scenario: A column laid out from its own completion has its size
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property real completedHeight: -1
        Column {
          id: column
          Repeater {
            model: 3
            Rectangle { width: 50; height: 20 }
          }
          Component.onCompleted: {
            column.forceLayout()
            root.completedHeight = column.height
          }
        }
      }
      """
    Then the property "completedHeight" of "root" is 60

  Scenario: A row told to lay out reads its new size and places at once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string seen: ""
        function grow() {
          first.width = 40
          first.height = 40
          positioner.forceLayout()
          seen = positioner.width + "x" + positioner.height + " " + positioner.children[1].x + "," + positioner.children[1].y
        }
        Row {
          id: positioner
          Rectangle { id: first; width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
        }
      }
      """
    When the method "grow" of "root" is called
    Then the property "seen" of "root" is "50x40 40,0"

  Scenario: A grid told to lay out reads its new size and places at once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string seen: ""
        function grow() {
          first.width = 40
          first.height = 40
          positioner.forceLayout()
          seen = positioner.width + "x" + positioner.height + " " + positioner.children[1].x + "," + positioner.children[1].y
        }
        Grid {
          id: positioner
          columns: 2
          Rectangle { id: first; width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
        }
      }
      """
    When the method "grow" of "root" is called
    Then the property "seen" of "root" is "50x40 40,0"

  Scenario: A flow told to lay out reads its new places at once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string seen: ""
        function grow() {
          first.width = 40
          first.height = 40
          positioner.forceLayout()
          seen = positioner.width + "x" + positioner.height + " " + positioner.children[1].x + "," + positioner.children[1].y
        }
        Flow {
          id: positioner
          width: 100
          Rectangle { id: first; width: 10; height: 10 }
          Rectangle { width: 10; height: 10 }
        }
      }
      """
    When the method "grow" of "root" is called
    Then the property "seen" of "root" is "100x40 40,0"
