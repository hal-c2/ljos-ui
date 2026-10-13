# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (interactive, dragging, dragStarted, dragEnded)
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html (preventStealing, canceled)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathView.dragging @api-QtQuick.MouseArea.preventStealing
Feature: Pressing a MouseArea in a PathView's delegate
  A press on a MouseArea inside a delegate belongs to the MouseArea, so a click on a delegate is
  its click. Once the pointer has gone far enough along the path the view takes the press away to
  drag the rows, and the MouseArea is told its press was cancelled, unless it prevents stealing.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        id: pv
        objectName: "pv"
        width: 500; height: 300
        model: 4
        property string log: ""
        onDragStarted: log += "d"
        onDragEnded: log += "D"
        delegate: Rectangle {
          width: 40; height: 40; color: "red"
          MouseArea {
            objectName: "area" + index
            anchors.fill: parent
            onPressed: pv.log += "p" + index
            onReleased: pv.log += "r" + index
            onClicked: pv.log += "c" + index
            onCanceled: pv.log += "x" + index
          }
        }
        path: Path { startX: 0; startY: 50; PathLine { x: 400; y: 50 } }
      }
      """

  Scenario: A click on a delegate is the MouseArea's
    When the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    Then the property "log" of "pv" is "p1r1c1"
    And the property "offset" of "pv" is 0

  Scenario: A short move along the path is still a click
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 104, 50
    And the user moves the pointer to 108, 50
    And the user releases the pointer at 108, 50
    Then the property "log" of "pv" is "p1r1c1"
    And the property "offset" of "pv" is 0
    And the property "dragging" of "pv" is false

  Scenario: Dragging along the path takes the press away from the MouseArea
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 120, 50
    Then the property "log" of "pv" is "p1"
    And the property "pressed" of "area1" is true
    When the user moves the pointer to 140, 50
    Then the property "log" of "pv" is "p1dx1"
    And the property "offset" of "pv" is within 0.001 of 0.2
    When the user moves the pointer to 160, 50
    Then the property "log" of "pv" is "p1dx1"
    And the property "dragging" of "pv" is true
    And the property "offset" of "pv" is within 0.001 of 0.4
    And the property "pressed" of "area1" is false
    When the user releases the pointer at 160, 50
    Then the property "log" of "pv" is "p1dx1D"

  Scenario: A MouseArea that prevents stealing keeps the press
    When the property "preventStealing" of "area1" is set to true
    And the user presses the pointer at 100, 50
    And the user moves the pointer to 120, 50
    And the user moves the pointer to 140, 50
    And the user moves the pointer to 160, 50
    Then the property "dragging" of "pv" is false
    And the property "offset" of "pv" is 0
    And the property "pressed" of "area1" is true
    When the user releases the pointer at 160, 50
    Then the property "log" of "pv" is "p1r1"

  Scenario: A move across the path leaves the press with the MouseArea
    When the user presses the pointer at 100, 50
    And the user moves the pointer to 100, 60
    And the user moves the pointer to 100, 68
    And the user releases the pointer at 100, 68
    Then the property "log" of "pv" is "p1r1c1"
    And the property "dragging" of "pv" is false
