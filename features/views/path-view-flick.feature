# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (flickDeceleration, maximumFlickVelocity,
#   flicking, flickStarted, flickEnded, snapMode, highlightRangeMode)
# Verified on Qt 6.11.2 (qml6, vnc; and qmltestrunner frame by frame for the deceleration).
# Let go, the rows slow down evenly by flickDeceleration / 10 places a second
# each second; velocities are pixels along the path. How fast a pointer moved
# depends on the machine, so every scenario caps it with maximumFlickVelocity,
# which the drags below are always faster than: 500 pixels a second goes
# 500 * 500 / (2 * 10 * 100) = 125 pixels, 1.25 places of 100 pixels.
# Snapped, the rows rest on the row nearest a quarter of a place beyond where
# they would have stopped (measured: going up, 3.24 rests on 3 and 3.26 on 4).

@qt-parity @api-QtQuick.PathView.flickDeceleration @api-QtQuick.PathView.maximumFlickVelocity
Feature: Flicking a PathView
  The rows go on along the path after the pointer lets go, slow down evenly and come to rest. With a
  snap mode or a strict highlight range they come to rest on a whole row, a little beyond where they
  would have stopped; `SnapOneItem` goes no
  further than the row next to the one nearest where the pointer let go.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 1000; height: 50
        model: 10
        maximumFlickVelocity: 500
        highlightRangeMode: PathView.NoHighlightRange
        property string log: ""
        onDragStarted: log += "d"
        onDragEnded: log += "D"
        onMovementStarted: log += "m"
        onMovementEnded: log += "M"
        onFlickStarted: log += "f"
        onFlickEnded: log += "F"
        delegate: Rectangle { width: 60; height: 40; color: "red" }
        path: Path { startX: 0; startY: 25; PathLine { x: 1000; y: 25 } }
      }
      """

  @api-QtQuick.PathView.flicking
  Scenario: The rows go on after the release and slow down evenly
    When the user presses the pointer at 10, 25
    And the user moves the pointer to 50, 25
    And 16 milliseconds pass
    And the user moves the pointer to 90, 25
    And 16 milliseconds pass
    And the user moves the pointer to 130, 25
    And 16 milliseconds pass
    And the user moves the pointer to 170, 25
    And the user releases the pointer at 170, 25
    And 3 seconds pass
    Then the property "offset" of "pv" is within 0.02 of 2.45
    And the property "flicking" of "pv" is false
    And the property "moving" of "pv" is false
    And the property "log" of "pv" is "mdDfFM"

  Scenario: A harder deceleration stops the rows sooner
    When the property "flickDeceleration" of "pv" is set to 400
    And the user presses the pointer at 10, 25
    And the user moves the pointer to 50, 25
    And 16 milliseconds pass
    And the user moves the pointer to 90, 25
    And 16 milliseconds pass
    And the user moves the pointer to 130, 25
    And 16 milliseconds pass
    And the user moves the pointer to 170, 25
    And the user releases the pointer at 170, 25
    And 3 seconds pass
    Then the property "offset" of "pv" is within 0.02 of 1.5125

  @api-QtQuick.PathView.highlightRangeMode
  Scenario: With a strict highlight range a flick that would stop at 2.45 rests on row 3
    When the property "highlightRangeMode" of "pv" is set to 2
    And the user presses the pointer at 10, 25
    And the user moves the pointer to 50, 25
    And 16 milliseconds pass
    And the user moves the pointer to 90, 25
    And 16 milliseconds pass
    And the user moves the pointer to 130, 25
    And 16 milliseconds pass
    And the user moves the pointer to 170, 25
    And the user releases the pointer at 170, 25
    And 3 seconds pass
    Then the property "offset" of "pv" is within 0.001 of 3
    And the property "currentIndex" of "pv" is 7

  @api-QtQuick.PathView.snapMode
  Scenario: SnapToItem rests a flick that would stop at 4.4 on row 5
    When the property "snapMode" of "pv" is set to 1
    And the property "maximumFlickVelocity" of "pv" is set to 800
    And the user presses the pointer at 10, 25
    And the user moves the pointer to 50, 25
    And 16 milliseconds pass
    And the user moves the pointer to 90, 25
    And 16 milliseconds pass
    And the user moves the pointer to 130, 25
    And 16 milliseconds pass
    And the user moves the pointer to 170, 25
    And the user releases the pointer at 170, 25
    And 3 seconds pass
    Then the property "offset" of "pv" is within 0.001 of 5

  @api-QtQuick.PathView.snapMode
  Scenario: SnapOneItem goes no further than the next row
    When the property "snapMode" of "pv" is set to 2
    And the property "maximumFlickVelocity" of "pv" is set to 800
    And the user presses the pointer at 10, 25
    And the user moves the pointer to 50, 25
    And 16 milliseconds pass
    And the user moves the pointer to 90, 25
    And 16 milliseconds pass
    And the user moves the pointer to 130, 25
    And 16 milliseconds pass
    And the user moves the pointer to 170, 25
    And the user releases the pointer at 170, 25
    And 3 seconds pass
    Then the property "offset" of "pv" is within 0.001 of 2
