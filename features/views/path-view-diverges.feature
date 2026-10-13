# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html#highlightRangeMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html#highlightMoveDuration-prop
#
# ApplyRange: the documentation says the view keeps the highlight within the range. Qt 6.11.2
# (qml6, offscreen) does so, bringing a current row past the end of the range back to the end over
# highlightMoveDuration (path-view-range.feature), with two exceptions:
# - With highlightMoveDuration 0 it does not: a current row at 0.7 of a 0.2..0.6 range stays there,
#   and a row at 1.1 leaves `offset` at 0.01.
# - After `offset` was written directly, the next move starts from an offset near 0 (each frame's
#   offset was logged: 0.01, 0.47, ...) and ends at whichever end of the range that start gives, so
#   a current row already in the range may move, and to its beginning.
# Ljós brings the row back to the end at once when there is no time to move, and measures from
# where the rows are after `offset` was written.

@qt-diverges
Feature: Where a PathView differs from Qt
  With ApplyRange the current row is kept in the range however the rows were moved, and at once when
  there is no time to move.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 1000; height: 50
        model: 10
        highlightRangeMode: PathView.ApplyRange
        preferredHighlightBegin: 0.2
        preferredHighlightEnd: 0.6
        delegate: Rectangle { objectName: "d" + index; width: 20; height: 20 }
        path: Path { startX: 0; startY: 25; PathLine { x: 1000; y: 25 } }
      }
      """

  Scenario: With no time to move the current row is brought back at once
    When the property "highlightMoveDuration" of "pv" is set to 0
    And the property "currentIndex" of "pv" is set to 5
    Then the property "offset" of "pv" is within 0.001 of 9
    And the property "x" of "d5" is within 0.1 of 590

  Scenario: A row in the range after the offset was written stays
    When the property "offset" of "pv" is set to 6.5
    And the property "currentIndex" of "pv" is set to 7
    And 1 second passes
    Then the property "offset" of "pv" is within 0.001 of 6.5
    And the property "x" of "d7" is within 0.1 of 540
