# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#hoverEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#containsMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#entered-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#exited-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#positionChanged-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Hover over a MouseArea
  A `hoverEnabled` MouseArea follows the pointer while no button is held.
  The log holds, for each signal, the position and `containsMouse` its
  handler read from the area.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string log: ""
        function rec(s) {
          log += s + " " + area.mouseX + "," + area.mouseY + " " + area.containsMouse + "|"
        }
        MouseArea {
          id: area
          x: 50; y: 50; width: 100; height: 100
          hoverEnabled: true
          onEntered: root.rec("entered")
          onExited: root.rec("exited")
          onPositionChanged: (mouse) => root.rec("moved b" + mouse.buttons)
          onPressed: root.rec("pressed")
          onReleased: root.rec("released")
        }
      }
      """

  Scenario: Moving in gives entered and then a position change
    When the user moves the pointer to 10, 10
    And the user moves the pointer to 60, 70
    Then the property "log" of "root" is "entered 10,20 true|moved b0 10,20 true|"

  Scenario: The first move of all gives the same when it is inside
    When the user moves the pointer to 60, 70
    Then the property "log" of "root" is "entered 10,20 true|moved b0 10,20 true|"

  Scenario: A move inside is a position change only
    When the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 65, 72
    Then the property "log" of "root" is "moved b0 15,22 true|"

  Scenario: Moving out gives exited only, at the last position inside
    When the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 10, 10
    Then the property "log" of "root" is "exited 10,20 false|"

  Scenario: A button nothing took does not stop the hover, and the hover carries no button
    When the user presses the pointer at 10, 10
    And the user moves the pointer to 60, 70
    And the user moves the pointer to 65, 70
    And the user moves the pointer to 10, 10
    Then the property "log" of "root" is "entered 10,20 true|moved b0 10,20 true|moved b0 15,20 true|exited 15,20 false|"

  Scenario: A button nothing took is released over the area
    When the user presses the pointer at 10, 10
    And the user moves the pointer to 60, 70
    And the property "log" of "root" is set to ""
    And the user releases the pointer at 60, 70
    And the user moves the pointer to 65, 70
    Then the property "log" of "root" is "moved b0 15,20 true|"

  Scenario: Dragging out of the pressed area gives exited before the position changes
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 10, 10
    Then the property "log" of "root" is "exited 10,20 false|moved b1 -40,-40 false|"

  Scenario: Dragging back in gives entered at the position outside
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 70, 70
    Then the property "log" of "root" is "entered -40,-40 true|moved b1 20,20 true|"

  Scenario: A release outside leaves the area without hover
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user releases the pointer at 10, 10
    And the user moves the pointer to 12, 10
    Then the property "log" of "root" is "released -40,-40 false|"

  Scenario: A click inside keeps the hover
    When the user moves the pointer to 60, 70
    And the user presses the pointer at 60, 70
    And the user releases the pointer at 60, 70
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 65, 70
    Then the property "log" of "root" is "moved b0 15,20 true|"
