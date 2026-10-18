# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html#stepSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html#stepSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: The steps a pointer gives a slider or a dial that does not snap
  A slider's or dial's `snapMode` says how its handle snaps. The value the
  pointer gives follows `stepSize` even when the handle does not snap: a
  slider's goes from step to step, and a dial's is a whole number when its
  ends and its step are whole.

  @qt-parity
  @api-QtQuick.Controls.Slider.stepSize
  Rule: A slider with steps that does not snap
    The value goes from step to step, and the handle stays where the
    pointer leaves it until the value changes again. The coffee demo's
    sliders show whole millilitres this way.

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 100
          Slider {
            objectName: "k"; width: 200
            from: 0; to: 30; stepSize: 2
            property int moves: 0
            onMoved: moves++
          }
        }
        """

    Scenario: The value is the step nearest the pointer
      When the user presses the pointer at 100, 20
      Then the property "value" of "k" is 16
      And the property "position" of "k" is 0.5
      When the user moves the pointer to 130, 20
      Then the property "value" of "k" is 20
      And the property "position" of "k" is 0.6875
      When the user moves the pointer to 131, 20
      And the user releases the pointer at 131, 20
      Then the property "value" of "k" is 20
      And the property "position" of "k" is 0.69375
      And the property "visualPosition" of "k" is 0.69375
      And the property "moves" of "k" is 3

    Scenario: The handle goes to the value's place once the value changes
      When the user presses the pointer at 130, 20
      And the user releases the pointer at 130, 20
      And the property "value" of "k" is set to 10
      Then the property "position" of "k" is within 0.0001 of 0.3333

  @qt-parity
  @api-QtQuick.Controls.Dial.stepSize
  Rule: Whole ends and a whole step
    When "from", "to" and "stepSize" are all whole numbers, the value the
    pointer gives is the nearest whole number, and the handle shows it.
    Otherwise the value is free. The thermostat demo shows its dial's
    value as it is, whole degrees.

    Scenario Outline: The pointer's value with <props>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Dial { objectName: "w"; <props> }
        }
        """
      When the user presses the pointer at 92, 10
      And the user moves the pointer to 160, 40
      Then the property "value" of "w" is within 0.0001 of <value>

      Examples:
        | props                             | value   |
        | from: 10; to: 30                  | 23.7568 |
        | from: 10; to: 30; stepSize: 1     | 24      |
        | from: 0; to: 30; stepSize: 2      | 21      |
        | from: 0; to: 100; stepSize: 7     | 69      |
        | from: 30; to: 0; stepSize: 1      | 9       |
        | from: 0; to: 30; stepSize: -1     | 21      |
        | from: 0.5; to: 20.5; stepSize: 1  | 14.2568 |
        | from: 0; to: 20.5; stepSize: 1    | 14.1007 |
        | from: 0; to: 30; stepSize: 1.5    | 20.6351 |

    Scenario: The handle shows the whole number
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Dial { objectName: "w"; from: 0; to: 30; stepSize: 1 }
        }
        """
      When the user presses the pointer at 92, 10
      And the user moves the pointer to 160, 40
      Then the property "value" of "w" is 21
      And the property "position" of "w" is 0.7
      And the property "angle" of "w" is 56
      When the user releases the pointer at 160, 40
      Then the property "value" of "w" is 21
      And the property "position" of "w" is 0.7
