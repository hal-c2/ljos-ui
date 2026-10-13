# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (pathItemCount, offset, currentIndex,
#   preferredHighlightBegin, highlightRangeMode, PathView.onPath)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.pathItemCount
Feature: A PathView shows only so many rows on its path
  With `pathItemCount` the path has that many places, shared evenly; only the rows at a place have a
  delegate. The current row keeps its delegate when it is off the path, at the path's end.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 700; height: 300
        model: 6
        pathItemCount: 3
        function resetCount() { pathItemCount = undefined }
        delegate: Item {
          objectName: "d" + index
          width: 20; height: 10
          property bool on: PathView.onPath
        }
        path: Path { startX: 0; startY: 50; PathLine { x: 600; y: 50 } }
      }
      """

  Scenario: The first rows take the places
    Then the property "count" of "pv" is 6
    And the property "x" of "d0" is -10
    And the property "x" of "d1" is 190
    And the property "x" of "d2" is 390
    And the object "d3" does not exist
    And the object "d5" does not exist

  Scenario: The offset brings other rows onto the path
    When the property "offset" of "pv" is set to 2.5
    Then the property "currentIndex" of "pv" is 4
    And the property "x" of "d4" is 90
    And the property "x" of "d5" is 290
    And the property "x" of "d0" is 490
    And the object "d1" does not exist
    And the object "d2" does not exist

  Scenario: The current row off the path waits at its end
    When the property "offset" of "pv" is set to 5.75
    Then the property "currentIndex" of "pv" is 0
    And the property "x" of "d0" is 590
    And the property "on" of "d0" is false
    And the property "x" of "d1" is 140
    And the property "on" of "d1" is true
    And the property "x" of "d2" is 340

  Scenario: The places are counted from where the highlight begins
    When the property "preferredHighlightBegin" of "pv" is set to 0.5
    And the property "preferredHighlightEnd" of "pv" is set to 0.5
    Then the property "x" of "d0" is 290
    And the property "x" of "d1" is within 0.001 of 490
    And the property "x" of "d5" is within 0.001 of 90

  Scenario: Every row is on the path again once the count is reset
    When the method "resetCount" of "pv" is called
    Then the property "pathItemCount" of "pv" is -1
    And the property "x" of "d3" is 290
    And the property "x" of "d5" is 490
