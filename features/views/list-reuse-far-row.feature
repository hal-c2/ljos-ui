# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (positionViewAtIndex, reuseItems)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A list of reused rows of two heights goes to a row it never laid out
  positionViewAtIndex measures the row it goes to where rows of the
  average height put it. Rows that leave the view for the pool on the way
  do not take the view back to where it was.

  Scenario: A far row of reused delegates of two heights is reached
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: win
        objectName: "win"; width: 900; height: 375
        function toRow() { view.positionViewAtIndex(65, ListView.Beginning) }
        ListView {
          id: view
          objectName: "list"; anchors.fill: parent
          clip: true; model: 116; reuseItems: true
          delegate: Rectangle {
            required property int index
            width: ListView.view.width
            height: index % 65 == 0 ? 36 : 18
          }
        }
      }
      """
    When 100 milliseconds pass
    And the method "toRow" of "win" is called
    And 100 milliseconds pass
    Then the property "contentY" of "list" is 1188
