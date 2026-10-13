# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (preferredHighlightBegin, preferredHighlightEnd,
#   highlightRangeMode, highlightMoveDuration, positionViewAtIndex, cacheItemCount, pathItemCount,
#   PathView.onPath)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.preferredHighlightEnd @api-QtQuick.PathView.cacheItemCount
Feature: A PathView keeps its current row in the highlight range and makes rows beyond its path
  `preferredHighlightBegin` and `preferredHighlightEnd` are the range the current row is kept in.
  With `StrictlyEnforceRange` it is brought to the beginning; with `ApplyRange` a row past the end
  is brought back to the end. A range that ends before it begins is no range. `cacheItemCount`
  makes delegates for rows just beyond the path, which wait off it at its end.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 1000; height: 50
        model: 10
        highlightMoveDuration: 100
        preferredHighlightBegin: 0.2
        preferredHighlightEnd: 0.6
        property var made: ({})
        property string rows: ""
        function list() { rows = Object.keys(made).map(Number).sort((a, b) => a - b).join(",") }
        delegate: Rectangle {
          objectName: "d" + index
          width: 20; height: 20
          property bool on: PathView.onPath
          Component.onCompleted: PathView.view.made[index] = true
          Component.onDestruction: delete PathView.view.made[index]
        }
        path: Path { startX: 0; startY: 25; PathLine { x: 1000; y: 25 } }
      }
      """

  Scenario: Strictly, the current row goes to where the range begins
    When the property "currentIndex" of "pv" is set to 3
    And 1 second passes
    Then the property "offset" of "pv" is 7
    And the property "x" of "d3" is 190
    When the property "offset" of "pv" is set to 7.3
    Then the property "currentIndex" of "pv" is 3
    And the property "x" of "d3" is 220

  Scenario: A range that ends before it begins is no range
    When the property "preferredHighlightEnd" of "pv" is set to 0.1
    Then the property "x" of "d0" is -10
    When the property "currentIndex" of "pv" is set to 2
    And 1 second passes
    Then the property "offset" of "pv" is 0
    When the property "offset" of "pv" is set to 1.4
    Then the property "currentIndex" of "pv" is 2

  Scenario: With ApplyRange a current row past the end of the range is brought back to it
    When the property "highlightRangeMode" of "pv" is set to 1
    And the property "currentIndex" of "pv" is set to 3
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 0
    And the property "x" of "d3" is within 0.1 of 490
    When the property "currentIndex" of "pv" is set to 5
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 9
    And the property "x" of "d5" is within 0.1 of 590
    When the property "currentIndex" of "pv" is set to 9
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 5
    And the property "x" of "d9" is within 0.1 of 590
    When the property "currentIndex" of "pv" is set to 7
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 5
    And the property "x" of "d7" is within 0.1 of 390
    When the property "currentIndex" of "pv" is set to 0
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 4
    When the method "decrementCurrentIndex" of "pv" is called
    And 1 second passes
    Then the property "currentIndex" of "pv" is 9
    And the property "offset" of "pv" is within 0.001 of 4
    And the property "x" of "d9" is within 0.1 of 490

  Scenario: With ApplyRange the range is a share of the places on the path
    When the property "highlightRangeMode" of "pv" is set to 1
    And the property "pathItemCount" of "pv" is set to 5
    And the property "currentIndex" of "pv" is set to 3
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 9
    And the property "x" of "d3" is within 0.1 of 590

  Scenario: Strictly, a row is shown where it is asked for and the row at the range is current
    When the method "positionViewAtIndex" of "pv" is called with 1, 0
    Then the property "offset" of "pv" is 7
    And the property "currentIndex" of "pv" is 3
    And the property "x" of "d1" is -10
    When the method "positionViewAtIndex" of "pv" is called with 4, 1
    Then the property "offset" of "pv" is 9
    And the property "currentIndex" of "pv" is 1
    When the method "positionViewAtIndex" of "pv" is called with 1, 2
    Then the property "offset" of "pv" is 6
    And the property "currentIndex" of "pv" is 4
    And the property "x" of "d1" is 890
    When the method "positionViewAtIndex" of "pv" is called with 7, 5
    Then the property "offset" of "pv" is 3
    And the property "currentIndex" of "pv" is 7
    When the method "positionViewAtIndex" of "pv" is called with 5, 4
    Then the property "offset" of "pv" is 3

  Scenario: With no cache only the rows on the path have a delegate
    When the property "preferredHighlightBegin" of "pv" is set to 0
    And the property "preferredHighlightEnd" of "pv" is set to 0
    And the property "pathItemCount" of "pv" is set to 3
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "cacheItemCount" of "pv" is 0
    And the property "rows" of "pv" is "0,1,2"

  Scenario: Cached rows are those within half the cache of either end of the path
    When the property "preferredHighlightBegin" of "pv" is set to 0
    And the property "preferredHighlightEnd" of "pv" is set to 0
    And the property "pathItemCount" of "pv" is set to 3
    And the property "cacheItemCount" of "pv" is set to 2
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "rows" of "pv" is "0,1,2,3"
    And the property "x" of "d3" is 990
    And the property "on" of "d3" is false
    And the property "on" of "d2" is true
    When the property "offset" of "pv" is set to 5.5
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "rows" of "pv" is "4,5,6,7,8"
    When the property "cacheItemCount" of "pv" is set to 4
    And the property "offset" of "pv" is set to 0
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "rows" of "pv" is "0,1,2,3,4,9"
    And the property "x" of "d9" is 990
    When the property "offset" of "pv" is set to 9
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "rows" of "pv" is "0,1,2,3,4,5"
    When the property "cacheItemCount" of "pv" is set to 1
    And the property "offset" of "pv" is set to 5.5
    And 100 milliseconds pass
    And the method "list" of "pv" is called
    Then the property "rows" of "pv" is "5,6,7"
