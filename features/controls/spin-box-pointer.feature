# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#up-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#valueModified-signal
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#wheelEnabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

Feature: Stepping a SpinBox with the pointer
  The button at one side steps the value up, the one at the other steps it
  down. A button held down steps again and again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        SpinBox {
          objectName: "s"
          x: 20; y: 20; value: 5
          property string log: ""
          onValueChanged: log += "v" + value + " "
          onValueModified: log += "m" + value + " "
        }
        SpinBox {
          objectName: "m"
          x: 20; y: 100; value: 5
          LayoutMirroring.enabled: true
        }
        TextInput { objectName: "t"; y: 200; width: 100; height: 20 }
      }
      """

  @qt-parity
  Rule: A click on a button is one step

    Scenario: The right button steps up when it is let go
      When the user presses the pointer at 140, 40
      Then the property "up.pressed" of "s" is true
      And the property "value" of "s" is 5
      And the property "up.indicator.color" of "s" is "#bdbdbd"
      When the user releases the pointer at 140, 40
      Then the property "up.pressed" of "s" is false
      And the property "value" of "s" is 6
      And the property "up.indicator.color" of "s" is "#e0e0e0"

    @api-QtQuick.Controls.SpinBox.valueModified
    Scenario: A step by the user is a changed value and then a modified one
      When the user touches 140, 40
      Then the property "log" of "s" is "v6 m6 "

    Scenario: The left button steps down
      When the user presses the pointer at 40, 40
      Then the property "down.pressed" of "s" is true
      When the user releases the pointer at 40, 40
      Then the property "value" of "s" is 4
      And the property "log" of "s" is "v4 m4 "

    Scenario: A click on a button leaves the keyboard where it is
      When the method "forceActiveFocus" of "t" is called
      And the user touches 140, 40
      Then the property "activeFocus" of "t" is true
      And the property "activeFocus" of "s" is false

    Scenario: A button with nowhere to go takes no press
      When the property "value" of "s" is set to 99
      And the user presses the pointer at 140, 40
      Then the property "up.pressed" of "s" is false
      When the user releases the pointer at 140, 40
      Then the property "value" of "s" is 99

    Scenario: A box that is not enabled takes no press
      When the property "enabled" of "s" is set to false
      And the user touches 140, 40
      Then the property "value" of "s" is 5
      And the property "log" of "s" is ""

    Scenario: The step is the box's own
      When the property "stepSize" of "s" is set to 10
      And the user touches 140, 40
      Then the property "value" of "s" is 15

    Scenario: With wrap a click at the end begins again
      When the property "wrap" of "s" is set to true
      And the property "value" of "s" is set to 99
      And the user touches 140, 40
      Then the property "value" of "s" is 0

    Scenario: In a mirrored box the buttons change sides
      Then the property "up.indicator.x" of "m" is 0
      And the property "down.indicator.x" of "m" is 100
      When the user touches 40, 120
      Then the property "value" of "m" is 6
      When the user touches 140, 120
      And the user touches 140, 120
      Then the property "value" of "m" is 4

  @qt-parity
  Rule: A press that leaves its button is no step

    Scenario: Away from the box the button is let go
      When the user presses the pointer at 140, 40
      And the user moves the pointer to 300, 250
      Then the property "up.pressed" of "s" is false
      When the user releases the pointer at 300, 250
      Then the property "value" of "s" is 5

    Scenario: Back on the button it is pressed again
      When the user presses the pointer at 140, 40
      And the user moves the pointer to 300, 250
      And the user moves the pointer to 140, 40
      Then the property "up.pressed" of "s" is true
      When the user releases the pointer at 140, 40
      Then the property "value" of "s" is 6

    Scenario: Moved to the other button it is that one that is pressed
      When the user presses the pointer at 140, 40
      And the user moves the pointer to 40, 40
      Then the property "up.pressed" of "s" is false
      And the property "down.pressed" of "s" is true
      When the user releases the pointer at 40, 40
      Then the property "value" of "s" is 4

  @qt-parity
  Rule: A button held down steps again and again

    Scenario: Nothing happens at first
      When the user presses the pointer at 140, 40
      And 150 milliseconds pass
      Then the property "value" of "s" is 5

    Scenario: After a while the value begins to run
      When the user presses the pointer at 140, 40
      And 1 second passes
      Then the property "value" of "s" is at least 9
      And the property "value" of "s" is at most 13
      And the property "up.pressed" of "s" is true

    Scenario: Letting go of a button that ran adds no step
      When the user presses the pointer at 140, 40
      And 1 second passes
      And the user releases the pointer at 140, 40
      And 500 milliseconds pass
      Then the property "value" of "s" is at most 13

    Scenario: The run stops at the end and the button stays pressed
      When the property "value" of "s" is set to 97
      And the user presses the pointer at 140, 40
      And 1 second passes
      Then the property "value" of "s" is 99
      And the property "up.pressed" of "s" is true
      And the property "up.indicator.enabled" of "s" is false

    Scenario: A button left while held stops running
      When the user presses the pointer at 140, 40
      And the user moves the pointer to 300, 250
      And 1 second passes
      Then the property "value" of "s" is 5

  # The oracle's clock is the wall's, so what it shows of the timing is a
  # band. These are the numbers Ljós uses: the first step 400 milliseconds
  # after the press, then one every 100.
  Rule: The pace of a held button

    Scenario Outline: After <time> milliseconds the value is <value>
      When the user presses the pointer at 140, 40
      And <time> milliseconds pass
      Then the property "value" of "s" is <value>

      Examples:
        | time | value |
        | 390  | 5     |
        | 410  | 6     |
        | 510  | 7     |
        | 1050 | 12    |

    Scenario: Each step of the run is a modified value
      When the user presses the pointer at 140, 40
      And 550 milliseconds pass
      And the user releases the pointer at 140, 40
      Then the property "log" of "s" is "v6 m6 v7 m7 "

  @qt-parity
  Rule: The box knows which button the pointer is over

    @api-QtQuick.Controls.SpinBox.up.hovered
    Scenario: Over the button that steps up
      When the user moves the pointer to 140, 40
      Then the property "up.hovered" of "s" is true
      And the property "down.hovered" of "s" is false
      And the property "hovered" of "s" is true

    @api-QtQuick.Controls.SpinBox.down.hovered
    Scenario: From one button to the other, and off
      When the user moves the pointer to 140, 40
      And the user moves the pointer to 40, 40
      Then the property "up.hovered" of "s" is false
      And the property "down.hovered" of "s" is true
      When the user moves the pointer to 300, 250
      Then the property "down.hovered" of "s" is false
      And the property "hovered" of "s" is false

    Scenario: Over the number neither button is hovered
      When the user moves the pointer to 90, 40
      Then the property "hovered" of "s" is true
      And the property "up.hovered" of "s" is false
      And the property "down.hovered" of "s" is false

    Scenario: A button with nowhere to go is not hovered
      When the property "value" of "s" is set to 99
      And the user moves the pointer to 140, 40
      Then the property "up.hovered" of "s" is false
      And the property "hovered" of "s" is true

    Scenario: A box that does not want to know has no hovered button
      When the property "hoverEnabled" of "s" is set to false
      And the user moves the pointer to 140, 40
      Then the property "up.hovered" of "s" is false
      And the property "hovered" of "s" is false

  # The oracle's input channel has no wheel.
  @ljos-only
  Rule: The wheel steps a box that asks for it

    @api-QtQuick.Controls.Control.wheelEnabled
    Scenario: A box does not listen to the wheel unless told to
      When the user turns the wheel 1 notch up at 90, 40
      Then the property "value" of "s" is 5

    Scenario: A notch up is a step up by the user
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 1 notch up at 90, 40
      Then the property "value" of "s" is 6
      And the property "log" of "s" is "v6 m6 "

    Scenario: Notches down are steps down
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 2 notches down at 90, 40
      Then the property "value" of "s" is 3

    Scenario: The wheel stops at the end of the range
      When the property "wheelEnabled" of "s" is set to true
      And the property "value" of "s" is set to 98
      And the user turns the wheel 3 notches up at 90, 40
      Then the property "value" of "s" is 99

    Scenario: A box that is not enabled leaves the wheel alone
      When the property "wheelEnabled" of "s" is set to true
      And the property "enabled" of "s" is set to false
      And the user turns the wheel 1 notch up at 90, 40
      Then the property "value" of "s" is 5
