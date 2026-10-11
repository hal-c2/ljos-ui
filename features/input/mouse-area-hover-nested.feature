# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#hoverEnabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#entered-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#exited-signal
#   https://doc.qt.io/qt-6/qtquick-input-mouseevents.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Hover over several MouseAreas
  The topmost `hoverEnabled` MouseArea under the pointer and the ones it
  is inside are hovered together, the innermost first. `in1` holds `in2`,
  which holds `in3`; `top` lies over half of `under`; `plain` does not hover.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 200
        property string log: ""
        function rec(s) { log += s + "|" }
        component Area: MouseArea {
          property string n
          hoverEnabled: true
          onEntered: root.rec(n + " entered")
          onExited: root.rec(n + " exited")
          onPositionChanged: (mouse) => root.rec(n + " moved b" + mouse.buttons)
          onReleased: root.rec(n + " released")
        }
        Area { n: "in1"; width: 100; height: 50
          Area { n: "in2"; width: 80; height: 50
            Area { n: "in3"; width: 50; height: 50 } } }
        Area { n: "under"; x: 150; width: 100; height: 50 }
        Area { n: "top"; x: 200; width: 100; height: 50 }
        Area { n: "plain"; y: 100; width: 50; height: 50; hoverEnabled: false }
      }
      """

  Scenario: The innermost area enters first, each with its position change
    When the user moves the pointer to 10, 10
    Then the property "log" of "root" is "in3 entered|in3 moved b0|in2 entered|in2 moved b0|in1 entered|in1 moved b0|"

  Scenario: A move inside goes to the innermost area first
    When the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 12, 10
    Then the property "log" of "root" is "in3 moved b0|in2 moved b0|in1 moved b0|"

  Scenario: An area that is left exits after the others have moved
    When the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 60, 10
    Then the property "log" of "root" is "in2 moved b0|in1 moved b0|in3 exited|"

  Scenario: Leaving them all exits the innermost first
    When the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 390, 190
    Then the property "log" of "root" is "in3 exited|in2 exited|in1 exited|"

  Scenario: Areas exit in the order they were entered
    When the user moves the pointer to 60, 10
    And the user moves the pointer to 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 390, 190
    Then the property "log" of "root" is "in2 exited|in1 exited|in3 exited|"

  Scenario: Only the topmost of two areas that overlap is hovered
    When the user moves the pointer to 210, 10
    Then the property "log" of "root" is "top entered|top moved b0|"

  Scenario: Moving from one area to another enters before it exits
    When the user moves the pointer to 210, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 160, 10
    Then the property "log" of "root" is "under entered|under moved b0|top exited|"

  Scenario: Nothing else hovers while an area holds the press
    When the user moves the pointer to 390, 190
    And the user presses the pointer at 10, 110
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 160, 10
    And the user moves the pointer to 162, 10
    Then the property "log" of "root" is "plain exited|plain moved b1|plain moved b1|"

  Scenario: The hover follows at the first move after the release
    When the user presses the pointer at 10, 110
    And the user moves the pointer to 160, 10
    And the user releases the pointer at 160, 10
    And the method "rec" of "root" is called with "then"
    And the user moves the pointer to 162, 10
    Then the property "log" of "root" is "plain entered|plain exited|plain moved b1|plain released|then|under entered|under moved b0|"

  Scenario: The areas around the pressed one keep their hover until the release
    When the user moves the pointer to 10, 10
    And the user presses the pointer at 10, 10
    And the property "log" of "root" is set to ""
    And the user moves the pointer to 390, 190
    And the user releases the pointer at 390, 190
    And the method "rec" of "root" is called with "then"
    And the user moves the pointer to 392, 190
    Then the property "log" of "root" is "in3 exited|in3 moved b1|in3 released|then|in2 exited|in1 exited|"
