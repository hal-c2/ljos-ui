# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (offset, flickStarted, flickDeceleration)
# Verified on Qt 6.11.2 with qmltestrunner, which stamps each mouse event at
# a fixed time, move by move.
# The drag begins at the first move more than 10 pixels along the path from
# the press (10 does not begin it, 11 does), there, without moving the rows.
# A release throws the mean speed of the last three moves after that one,
# less a fiftieth for each millisecond the pointer rested before it let go;
# under about 75 pixels a second it throws nothing. Rows of 40 pixels slow
# down by flickDeceleration / 10 rows a second each second, 400 pixels here,
# and rest on the row nearest a quarter of a row beyond where they stop. A
# rest halfway between two rows goes to the lower offset.

@api-QtQuick.PathView.offset @api-QtQuick.PathView.flickStarted
Feature: How far a PathView is thrown
  How far the rows go after the pointer lets go follows the pointer's last few moves, and how long
  it rested before letting go.

  Background:
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 200; height: 400
        model: 30; pathItemCount: 30
        property bool flicked: false
        onFlickStarted: flicked = true
        delegate: Rectangle { width: 200; height: 40; color: "gray" }
        path: Path { startX: 100; startY: -400; PathLine { x: 100; y: 800 } }
      }
      """

  @qt-parity
  Rule: The drag begins more than 10 pixels along the path

    Scenario: Moves of 8 pixels begin it at the second and move the rows from the third
      When the user presses the pointer at 100, 200
      And the user moves the pointer to 100, 192
      And the user moves the pointer to 100, 184
      Then the property "offset" of "pv" is 0
      When the user moves the pointer to 100, 176
      Then the property "offset" of "pv" is within 0.0001 of 29.8

    Scenario: A move of 11 pixels begins it and the next one moves the rows
      When the user presses the pointer at 100, 200
      And the user moves the pointer to 100, 189
      Then the property "offset" of "pv" is 0
      When the user moves the pointer to 100, 178
      Then the property "offset" of "pv" is within 0.0001 of 29.725

    Scenario: A move of 10 pixels does not begin it
      When the user presses the pointer at 100, 200
      And the user moves the pointer to 100, 190
      And the user moves the pointer to 100, 180
      Then the property "offset" of "pv" is 0
      When the user moves the pointer to 100, 170
      Then the property "offset" of "pv" is within 0.0001 of 29.75

  @qt-parity
  Rule: A pointer that rested or crept throws nothing

    Scenario Outline: Dragged <way> to rest halfway between two rows, still for 50 milliseconds, the rows go to the lower one
      When the user presses the pointer at 100, 200
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y1>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y2>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y3>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y4>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y5>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y6>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y7>
      And 16 milliseconds pass
      And the user moves the pointer to 100, <y8>
      Then the property "offset" of "pv" is within 0.0001 of <halfway>
      When 50 milliseconds pass
      And the user releases the pointer at 100, <y8>
      And 2 seconds pass
      Then the property "flicked" of "pv" is false
      And the property "offset" of "pv" is <rest>

      Examples:
        | way  | y1  | y2  | y3  | y4  | y5  | y6  | y7  | y8  | halfway | rest |
        | up   | 190 | 180 | 170 | 160 | 150 | 140 | 130 | 120 | 28.5    | 28   |
        | down | 210 | 220 | 230 | 240 | 250 | 260 | 270 | 280 | 1.5     | 1    |

    Scenario: A pointer that creeps 2 pixels every 50 milliseconds throws nothing
      When the user presses the pointer at 100, 200
      And 50 milliseconds pass
      And the user moves the pointer to 100, 198
      And 50 milliseconds pass
      And the user moves the pointer to 100, 196
      And 50 milliseconds pass
      And the user moves the pointer to 100, 194
      And 50 milliseconds pass
      And the user moves the pointer to 100, 192
      And 50 milliseconds pass
      And the user moves the pointer to 100, 190
      And 50 milliseconds pass
      And the user moves the pointer to 100, 188
      And 50 milliseconds pass
      And the user moves the pointer to 100, 186
      And 50 milliseconds pass
      And the user moves the pointer to 100, 184
      And 50 milliseconds pass
      And the user releases the pointer at 100, 184
      And 2 seconds pass
      Then the property "flicked" of "pv" is false

  # The speeds below need events at exact times, which only the Ljós runner
  # gives. The numbers are qmltestrunner's, which stamps each mouse event a
  # millisecond after the delay it was given: its 16 is 17 here.
  @ljos-only
  Rule: A release throws the speed of the last three moves

    Scenario Outline: Eight moves of 10 pixels 17 milliseconds apart, let go after <rest> milliseconds, go on to <offset>
      When the user presses the pointer at 100, 200
      And 17 milliseconds pass
      And the user moves the pointer to 100, 190
      And 17 milliseconds pass
      And the user moves the pointer to 100, 180
      And 17 milliseconds pass
      And the user moves the pointer to 100, 170
      And 17 milliseconds pass
      And the user moves the pointer to 100, 160
      And 17 milliseconds pass
      And the user moves the pointer to 100, 150
      And 17 milliseconds pass
      And the user moves the pointer to 100, 140
      And 17 milliseconds pass
      And the user moves the pointer to 100, 130
      And 17 milliseconds pass
      And the user moves the pointer to 100, 120
      And <rest> milliseconds pass
      And the user releases the pointer at 100, 120
      And 3 seconds pass
      Then the property "flicked" of "pv" is true
      And the property "offset" of "pv" is <offset>

      Examples:
        | rest | offset |
        | 17   | 24     |
        | 25   | 26     |
        | 41   | 28     |

    Scenario: Slow moves that end in a fast one go on as if a third as fast
      When the user presses the pointer at 100, 200
      And 17 milliseconds pass
      And the user moves the pointer to 100, 197
      And 17 milliseconds pass
      And the user moves the pointer to 100, 194
      And 17 milliseconds pass
      And the user moves the pointer to 100, 191
      And 17 milliseconds pass
      And the user moves the pointer to 100, 188
      And 17 milliseconds pass
      And the user moves the pointer to 100, 185
      And 17 milliseconds pass
      And the user moves the pointer to 100, 182
      And 17 milliseconds pass
      And the user moves the pointer to 100, 179
      And 17 milliseconds pass
      And the user moves the pointer to 100, 176
      And 17 milliseconds pass
      And the user moves the pointer to 100, 156
      And 17 milliseconds pass
      And the user releases the pointer at 100, 156
      And 3 seconds pass
      Then the property "flicked" of "pv" is true
      And the property "offset" of "pv" is 25
