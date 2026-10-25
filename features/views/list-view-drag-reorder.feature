# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#drag.target-prop
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Reordering a list by dragging its delegates
  Each delegate is dragged by a MouseArea and holds a DropArea. A delegate
  dragged down the list and let go is dropped on the one under it, not on
  its own area, and the drop can move it in the model.

  Scenario: A delegate dropped on the last one moves there
    Given the file "TaskRow.qml":
      """qml
      import QtQuick
      Rectangle {
        id: d
        required property int index
        required property string name
        signal dropped(int from)
        width: 300; height: 60
        Drag.active: area.pressed
        Drag.hotSpot.x: width / 2
        Drag.hotSpot.y: height / 2
        z: area.pressed ? 1 : 0
        MouseArea {
          id: area
          anchors.fill: parent
          drag.target: parent
          drag.axis: Drag.YAxis
          onReleased: parent.Drag.drop()
          DropArea {
            anchors.fill: parent
            onDropped: drag => d.dropped(drag.source.DelegateModel.itemsIndex)
          }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        ListModel { id: tasks; ListElement { name: "A" } ListElement { name: "B" } ListElement { name: "C" } }
        function order() { let s = ""; for (let i = 0; i < tasks.count; ++i) s += tasks.get(i).name; return s }
        ListView {
          width: 300; height: 300
          model: tasks
          interactive: false
          delegate: TaskRow {
            onDropped: from => {
              root.log += "dropped on " + name + "|"
              tasks.move(from, index, 1)
              root.log += root.order() + "|"
            }
          }
        }
      }
      """
    When the user presses the pointer at 100, 30
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 100
    And the user moves the pointer to 100, 150
    And the user releases the pointer at 100, 150
    Then the property "log" of "root" is "dropped on C|BCA|"
