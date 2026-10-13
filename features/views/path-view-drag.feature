# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (interactive, dragging, moving, dragMargin,
#   dragStarted, dragEnded, movementStarted, movementEnded, highlightRangeMode)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.dragging @api-QtQuick.PathView.interactive
Feature: Dragging a PathView
  The user drags the delegates along the path. The rows follow once the pointer has gone a little
  way along the path, and after the release they come to rest with a row at the start of the path.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 500; height: 300
        model: 4
        property string log: ""
        onDragStarted: log += "d"
        onDragEnded: log += "D"
        onMovementStarted: log += "m"
        onMovementEnded: log += "M"
        property bool whole: Math.abs(offset - Math.round(offset)) < 0.000001
        property bool done: log.startsWith("md") && log.indexOf("D") > 0 && log.endsWith("M")
        delegate: Rectangle { objectName: "d" + index; width: 40; height: 40; color: "red" }
        path: Path { startX: 0; startY: 50; PathLine { x: 400; y: 50 } }
      }
      """

  Scenario: The rows follow the pointer along the path
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 120, 50
    And the user moves the pointer to 140, 50
    And the user moves the pointer to 160, 50
    Then the property "offset" of "pv" is within 0.001 of 0.4
    And the property "dragging" of "pv" is true
    And the property "moving" of "pv" is true
    And the property "log" of "pv" is "md"

  Scenario: A short move does not drag
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 104, 50
    And the user moves the pointer to 108, 50
    Then the property "offset" of "pv" is 0
    And the property "dragging" of "pv" is false

  Scenario: The rows come to rest on a row after the release
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 120, 50
    And the user moves the pointer to 140, 50
    And the user moves the pointer to 160, 50
    And the user releases the pointer at 160, 50
    And 3 seconds pass
    Then the property "dragging" of "pv" is false
    And the property "moving" of "pv" is false
    And the property "whole" of "pv" is true
    And the property "done" of "pv" is true

  Scenario: A view that is not interactive does not move
    When the property "interactive" of "pv" is set to false
    And the user presses the pointer at 100, 50
    And the user moves the pointer to 120, 50
    And the user moves the pointer to 140, 50
    And the user releases the pointer at 140, 50
    Then the property "offset" of "pv" is 0
    And the property "log" of "pv" is ""

  Scenario: A press away from the delegates does not move the rows
    When the user presses the pointer at 150, 50
    And the user moves the pointer to 170, 50
    And the user moves the pointer to 190, 50
    And the user releases the pointer at 190, 50
    Then the property "offset" of "pv" is 0
    And the property "log" of "pv" is ""

  Scenario: A move across the path does not move the rows
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 70
    Then the property "offset" of "pv" is 0
    And the property "dragging" of "pv" is false
