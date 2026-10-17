# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Dial
  A dial is turned like a knob: its value goes from `from` to `to` as its
  `angle` goes from `startAngle` to `endAngle`, clockwise from twelve
  o'clock. The pointer turns it round its middle, or along a line when its
  `inputMode` says so; the keys step it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Dial { objectName: "d"; property int moves: 0; onMoved: moves++ }
        Dial { objectName: "z"; x: 200; from: 0; to: 10; value: 0.5; startAngle: 0; endAngle: 270 }
        Dial { objectName: "w"; x: 200; y: 200; wrap: true; property int moves: 0; onMoved: moves++ }
        Dial { objectName: "s"; x: 0; y: 200; stepSize: 0.25; snapMode: Dial.SnapAlways }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A dial starts at the beginning of 0 to 1, turned to the lower left
      Then the property "<property>" of "d" is <value>

      Examples:
        | property         | value |
        | from             | 0     |
        | to               | 1     |
        | value            | 0     |
        | position         | 0     |
        | angle            | -140  |
        | startAngle       | -140  |
        | endAngle         | 140   |
        | stepSize         | 0     |
        | snapMode         | 0     |
        | inputMode        | 0     |
        | wrap             | false |
        | live             | true  |
        | pressed          | false |
        | wheelEnabled     | false |
        | focusPolicy      | 1     |
        | implicitWidth    | 184   |
        | implicitHeight   | 184   |
        | padding          | 0     |
        | background.width | 184   |

  @qt-parity
  Rule: The value

    Scenario: The angle is the position's share of the turn
      Then the property "position" of "z" is 0.05
      And the property "angle" of "z" is 13.5

    Scenario: The value stays between the ends
      When the property "value" of "d" is set to 5
      Then the property "value" of "d" is 1
      And the property "angle" of "d" is 140

    @api-QtQuick.Controls.Dial.increase
    Scenario: increase() goes one step and does not snap
      When the property "value" of "s" is set to 0.6
      And the method "increase" of "s" is called
      Then the property "value" of "s" is within 0.0001 of 0.85
      When the method "decrease" of "s" is called
      Then the property "value" of "s" is within 0.0001 of 0.6

  @qt-parity
  Rule: The keyboard

    Scenario: The arrow keys step the dial and Home and End go to the ends
      When the method "forceActiveFocus" of "d" is called
      And the user presses "Right"
      Then the property "value" of "d" is within 0.0001 of 0.1
      When the user presses "Up"
      Then the property "value" of "d" is within 0.0001 of 0.2
      When the user presses "Left"
      And the user presses "Down"
      Then the property "value" of "d" is within 0.0001 of 0
      When the user presses "End"
      Then the property "value" of "d" is 1
      When the user presses "Home"
      Then the property "value" of "d" is 0
      And the property "moves" of "d" is 6

    Scenario: Page keys do not turn it, and a step past the end is no move
      When the method "forceActiveFocus" of "w" is called
      And the user presses "PgUp"
      And the user presses "Left"
      Then the property "value" of "w" is 0
      And the property "moves" of "w" is 0

  @qt-parity
  Rule: The pointer

    Scenario: The pointer turns the dial round its middle
      When the user presses the pointer at 92, 10
      Then the property "value" of "d" is 0.5
      And the property "pressed" of "d" is true
      And the property "moves" of "d" is 1
      When the user moves the pointer to 174, 92
      Then the property "value" of "d" is within 0.001 of 0.8214
      When the user moves the pointer to 92, 175
      Then the property "value" of "d" is 1
      And the property "moves" of "d" is 3
      # Past the end, letting go is one more move, though the value stays.
      When the user releases the pointer at 92, 175
      Then the property "pressed" of "d" is false
      And the property "value" of "d" is 1
      And the property "moves" of "d" is 4

    Scenario: A press that does not turn the dial is no move
      When the user presses the pointer at 92, 10
      And the user releases the pointer at 92, 10
      Then the property "moves" of "d" is 1

    Scenario: A dial that does not wrap does not jump across the gap
      When the user presses the pointer at 92, 10
      And the user moves the pointer to 174, 92
      And the user moves the pointer to 92, 175
      Then the property "moves" of "d" is 3
      When the user moves the pointer to 30, 160
      Then the property "value" of "d" is 1
      And the property "moves" of "d" is 3

    Scenario: A dial that does not wrap stays put when pressed across the gap
      When the user presses the pointer at 92, 175
      Then the property "value" of "d" is 0
      And the property "moves" of "d" is 0

    # Further than half a turn of the circle the short way is through the
    # gap: the dial goes to the end on that way. Less far, it stays put.
    Scenario Outline: A dial that does not wrap, at <from> and pressed at <x>, <y>, goes to <value>
      When the property "value" of "d" is set to <from>
      And the user presses the pointer at <x>, <y>
      Then the property "value" of "d" is <value>
      And the property "moves" of "d" is <moves>

      Examples:
        | from | x   | y  | value | moves |
        | 0.1  | 172 | 84 | 0     | 1     |
        | 0.25 | 172 | 84 | 0.25  | 0     |
        | 0.9  | 12  | 84 | 1     | 1     |
        | 0.75 | 12  | 84 | 0.75  | 0     |

    Scenario Outline: Pressed in the gap at <x>, <y>, a dial that wraps goes to the nearer end, <value>
      When the property "value" of "w" is set to 0.5
      And the user presses the pointer at <x>, <y>
      Then the property "value" of "w" is <value>

      Examples:
        | x   | y   | value |
        | 306 | 371 | 1     |
        | 292 | 375 | 1     |
        | 278 | 371 | 0     |

    Scenario: A press at the very middle turns the dial to twelve o'clock
      When the user presses the pointer at 92, 92
      Then the property "value" of "d" is 0.5
      And the property "moves" of "d" is 1

    @api-QtQuick.Controls.Dial.wrap
    Scenario: A dial that wraps goes across the gap
      When the user presses the pointer at 230, 360
      Then the property "value" of "w" is within 0.001 of 0.0084
      And the property "angle" of "w" is within 0.01 of -137.64
      When the user moves the pointer to 292, 375
      Then the property "value" of "w" is 1
      When the user moves the pointer to 350, 360
      Then the property "value" of "w" is within 0.001 of 0.9983
      When the user moves the pointer to 230, 360
      Then the property "value" of "w" is within 0.001 of 0.0084

    @api-QtQuick.Controls.Dial.snapMode
    Scenario: A dial that always snaps goes from step to step
      When the user presses the pointer at 92, 210
      Then the property "value" of "s" is 0.5
      When the user moves the pointer to 160, 240
      Then the property "value" of "s" is 0.75
