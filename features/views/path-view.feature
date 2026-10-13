# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (model, delegate, path, count, offset,
#   currentIndex, currentItem, highlightMoveDuration, preferredHighlightBegin, highlightRangeMode,
#   incrementCurrentIndex, decrementCurrentIndex, positionViewAtIndex, indexAt, itemAt,
#   PathView.isCurrentItem, PathView.onPath, PathView.view)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.offset @api-QtQuick.PathView.currentIndex
Feature: A PathView lays its rows out along a path
  Each row has a delegate, centred on a point of the path: the rows share the path evenly, and
  `offset` moves them all along it. By default the current row is the one at the start of the path,
  and making another row current moves the rows until it is there.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        id: pv
        objectName: "pv"
        width: 500; height: 300
        model: 4
        property int hit: -2
        property string hitName: "none"
        function probe(x, y) {
          hit = indexAt(x, y);
          var it = itemAt(x, y);
          hitName = it ? it.objectName : "";
        }
        delegate: Item {
          objectName: "d" + index
          width: 20; height: 10
          property bool cur: PathView.isCurrentItem
          property bool on: PathView.onPath
          property var owner: PathView.view
        }
        path: Path { startX: 0; startY: 50; PathLine { x: 400; y: 50 } }
      }
      """

  Scenario: The rows share the path evenly
    Then the property "count" of "pv" is 4
    And the property "x" of "d0" is -10
    And the property "y" of "d0" is 45
    And the property "x" of "d1" is 90
    And the property "x" of "d2" is 190
    And the property "x" of "d3" is 290

  Scenario: What a PathView is when nothing is said
    Then the property "pathItemCount" of "pv" is -1
    And the property "preferredHighlightBegin" of "pv" is 0
    And the property "highlightRangeMode" of "pv" is 2
    And the property "snapMode" of "pv" is 0
    And the property "movementDirection" of "pv" is 0
    And the property "highlightMoveDuration" of "pv" is 300
    And the property "flickDeceleration" of "pv" is 100
    And the property "maximumFlickVelocity" of "pv" is 2500
    And the property "dragMargin" of "pv" is 0
    And the property "interactive" of "pv" is true
    And the property "moving" of "pv" is false
    And the property "dragging" of "pv" is false
    And the property "flicking" of "pv" is false

  Scenario: The offset moves every row along the path
    When the property "offset" of "pv" is set to 0.5
    Then the property "x" of "d0" is 40
    And the property "x" of "d3" is 340
    And the property "currentIndex" of "pv" is 0
    When the property "offset" of "pv" is set to 1
    Then the property "x" of "d3" is -10
    And the property "x" of "d0" is 90
    And the property "currentIndex" of "pv" is 3
    And the property "cur" of "d3" is true

  Scenario: A row half way between two places is current when the next one is
    When the property "offset" of "pv" is set to 3.5
    Then the property "currentIndex" of "pv" is 1

  Scenario: Another current row is moved to the start of the path over a while
    When the property "currentIndex" of "pv" is set to 1
    Then the property "currentItem.objectName" of "pv" is "d1"
    And the property "cur" of "d1" is true
    And the property "cur" of "d0" is false
    When 100 milliseconds pass
    Then the property "offset" of "pv" is at least 3.5
    And the property "offset" of "pv" is at most 3.95
    When 1 second passes
    Then the property "offset" of "pv" is 3
    And the property "x" of "d1" is -10

  Scenario: With no time to move, the rows are there at once
    When the property "highlightMoveDuration" of "pv" is set to 0
    And the property "currentIndex" of "pv" is set to 2
    Then the property "offset" of "pv" is 2
    And the property "x" of "d2" is -10

  Scenario: The current row steps round the rows
    When the property "highlightMoveDuration" of "pv" is set to 0
    And the method "incrementCurrentIndex" of "pv" is called
    Then the property "currentIndex" of "pv" is 1
    When the method "decrementCurrentIndex" of "pv" is called
    And the method "decrementCurrentIndex" of "pv" is called
    Then the property "currentIndex" of "pv" is 3
    When the method "incrementCurrentIndex" of "pv" is called
    Then the property "currentIndex" of "pv" is 0

  Scenario: A current index beyond the rows comes round again
    When the property "model" of "pv" is set to 3
    And the property "highlightMoveDuration" of "pv" is set to 0
    And the property "currentIndex" of "pv" is set to 7
    Then the property "currentIndex" of "pv" is 1
    When the property "currentIndex" of "pv" is set to -1
    Then the property "currentIndex" of "pv" is 2

  Scenario: Another model starts again at the first row
    When the property "offset" of "pv" is set to 1
    And the property "model" of "pv" is set to 6
    Then the property "offset" of "pv" is 0
    And the property "currentIndex" of "pv" is 0
    And the property "count" of "pv" is 6

  Scenario: The current row is kept where the highlight is to begin
    When the property "highlightMoveDuration" of "pv" is set to 0
    And the property "preferredHighlightBegin" of "pv" is set to 0.5
    And the property "preferredHighlightEnd" of "pv" is set to 0.5
    Then the property "x" of "d0" is 190
    When the property "currentIndex" of "pv" is set to 1
    Then the property "x" of "d1" is 190
    And the property "offset" of "pv" is 3

  Scenario: With no highlight range the offset and the current row go their own ways
    When the property "highlightRangeMode" of "pv" is set to 0
    And the property "currentIndex" of "pv" is set to 2
    And 1 second passes
    Then the property "offset" of "pv" is 0
    And the property "x" of "d0" is -10
    When the property "offset" of "pv" is set to 1
    Then the property "currentIndex" of "pv" is 2

  Scenario: A row is shown at the start of the path and made current
    When the method "positionViewAtIndex" of "pv" is called with 3, 0
    Then the property "currentIndex" of "pv" is 3
    And the property "offset" of "pv" is 1
    And the property "x" of "d3" is -10

  Scenario: With no highlight range a row is shown where it is asked for
    When the property "highlightRangeMode" of "pv" is set to 0
    And the method "positionViewAtIndex" of "pv" is called with 1, 0
    Then the property "offset" of "pv" is within 0.001 of 3
    And the property "currentIndex" of "pv" is 0
    When the method "positionViewAtIndex" of "pv" is called with 1, 1
    Then the property "offset" of "pv" is within 0.001 of 1
    And the property "x" of "d1" is within 0.1 of 190

  Scenario: The row at a point is the one whose delegate covers it
    When the method "probe" of "pv" is called with 90, 45
    Then the property "hit" of "pv" is 1
    And the property "hitName" of "pv" is "d1"
    When the method "probe" of "pv" is called with 109, 54
    Then the property "hit" of "pv" is 1
    When the method "probe" of "pv" is called with 110, 50
    Then the property "hit" of "pv" is -1
    And the property "hitName" of "pv" is ""
    When the method "probe" of "pv" is called with 290, 50
    Then the property "hit" of "pv" is 3

  Scenario: A delegate knows its view and whether it is on the path
    Then the property "owner.objectName" of "d2" is "pv"
    And the property "on" of "d2" is true
    And the property "cur" of "d0" is true
