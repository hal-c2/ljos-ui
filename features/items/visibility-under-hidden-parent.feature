# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop

Feature: What an item's own visible says while its parent is hidden
  A hidden parent makes its children read not visible, but each child's own
  binding goes on: when the parent shows again, a child whose binding turned
  false meanwhile stays hidden, on screen as in its property.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root
        objectName: "root"
        width: 100; height: 100
        color: "black"
        property int count: 0
        Item {
          objectName: "page"
          anchors.fill: parent
          visible: false
          Rectangle {
            objectName: "empty"
            anchors.fill: parent
            color: "red"
            visible: root.count === 0
          }
        }
      }
      """

  # Found by `ljos observe`: its Logs page drew "No line matches" over the
  # lines it had, while the label's `visible` read false.
  Scenario: A child whose binding turned false while its parent was hidden is not drawn
    When the property "count" of "root" is set to 3
    And the property "visible" of "page" is set to true
    Then the property "visible" of "empty" is false
    And the pixel at 50, 50 of the window is "#000000"
