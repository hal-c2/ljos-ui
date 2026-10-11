# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (snapMode)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.snapMode
Feature: A ListView that snaps to its rows
  With a snap mode the view comes to rest with a row at its start, not between two rows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 100
        readonly property bool snapped: Math.round(contentY) % 40 === 0
        delegate: Item { objectName: "r" + index; width: 200; height: 40 }
      }
      """

  Scenario: Without a snap mode a drag ends where the pointer let go
    When the user drags from 100, 250 to 100, 135
    And 2 seconds pass
    Then the property "contentY" of "list" is at least 85
    And the property "contentY" of "list" is at most 115
    And the property "snapped" of "list" is false
    And the property "moving" of "list" is false

  Scenario: Snapping to a row brings the nearest row to the start of the view
    When the property "snapMode" of "list" is set to 1
    And the user drags from 100, 250 to 100, 135
    And 2 seconds pass
    Then the property "contentY" of "list" is at least 80
    And the property "moving" of "list" is false
    And the property "snapped" of "list" is true

  Scenario: Putting a row at the snap position
    When the property "snapMode" of "list" is set to 1
    And the method "positionViewAtIndex" of "list" is called with 12, 5
    Then the property "contentY" of "list" is 480
