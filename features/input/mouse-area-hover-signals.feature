# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#containsMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#mouseX-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#entered-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#positionChanged-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The order of a MouseArea's hover signals
  `containsMouse` changes before `entered` and `exited`; `mouseX` and
  `mouseY` signal their change after them and before `positionChanged`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string log: ""
        function rec(s) { log += s + "|" }
        MouseArea {
          x: 50; y: 50; width: 100; height: 100
          hoverEnabled: true
          onContainsMouseChanged: root.rec("contains " + containsMouse + " at " + mouseX + "," + mouseY)
          onEntered: root.rec("entered")
          onExited: root.rec("exited")
          onMouseXChanged: root.rec("x " + mouseX)
          onMouseYChanged: root.rec("y " + mouseY)
          onPositionChanged: root.rec("moved")
          onPressed: root.rec("pressed")
        }
      }
      """

  # Ljós tells of the coordinates before `entered` ("contains true at
  # 10,20|x 10|y 20|entered|moved|"): the handlers of `containsMouse` and
  # `entered` must read the new position, and the engine cannot yet store a
  # value and tell of the change later.
  @backlog
  Scenario: Moving in
    When the user moves the pointer to 10, 10
    And the user moves the pointer to 60, 70
    Then the property "log" of "root" is "contains true at 10,20|entered|x 10|y 20|moved|"

  Scenario: Both coordinates signal a change when one of them moved
    When the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 65, 70
    Then the property "log" of "root" is "x 15|y 20|moved|"

  Scenario: Moving out changes neither coordinate
    When the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 10, 10
    Then the property "log" of "root" is "contains false at 10,20|exited|"

  # Waits for the same thing: Qt tells of both coordinates after `pressed`,
  # changed or not, and of `pressed` and `pressedButtons` after them.
  @backlog
  Scenario: A press signals the coordinates after pressed
    When the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user presses the pointer at 60, 70
    Then the property "log" of "root" is "pressed|x 10|y 20|"

  Scenario: Dragging out of the pressed area
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 10, 10
    Then the property "log" of "root" is "contains false at 10,20|exited|x -40|y -40|moved|"

  Scenario: Dragging back into the pressed area
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 70, 70
    Then the property "log" of "root" is "contains true at -40,-40|entered|x 20|y 20|moved|"
