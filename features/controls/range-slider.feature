# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-rangeslider.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: RangeSlider
  A range slider holds two values on one track, `first.value` and
  `second.value`, the first never past the second. Each has its own handle,
  and the handle that has the keyboard is the one the keys move.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        RangeSlider {
          objectName: "r"
          first.value: 0.25; second.value: 0.75
        }
        RangeSlider {
          objectName: "tens"
          y: 50; from: 0; to: 10; stepSize: 1
          first.value: 2; second.value: 8
          property real half: valueAt(0.5)
          property real third: valueAt(0.33)
        }
        RangeSlider { objectName: "up"; x: 300; y: 100; orientation: Qt.Vertical }
      }
      """

  @qt-parity
  Rule: The values

    Scenario: The values are shares of the track
      Then the property "first.value" of "r" is 0.25
      And the property "first.position" of "r" is 0.25
      And the property "first.visualPosition" of "r" is 0.25
      And the property "second.value" of "r" is 0.75
      And the property "second.position" of "r" is 0.75
      And the property "first.pressed" of "r" is false

    Scenario: The handles stand on the track at their positions
      Then the property "first.handle.x" of "r" is 46
      And the property "second.handle.x" of "r" is 126
      And the property "first.implicitHandleWidth" of "r" is 28

    Scenario: One handle does not pass the other
      When the property "first.value" of "r" is set to 0.9
      Then the property "first.value" of "r" is 0.75
      When the property "second.value" of "r" is set to 0.1
      Then the property "second.value" of "r" is 0.75

    @api-QtQuick.Controls.RangeSlider.setValues
    Scenario: Both values are set at once
      When the method "setValues" of "r" is called with 0.2, 0.6
      Then the property "first.value" of "r" is 0.2
      And the property "second.value" of "r" is 0.6

    Scenario: The values count from the ends given
      Then the property "first.position" of "tens" is 0.2
      And the property "half" of "tens" is 5
      And the property "third" of "tens" is 3

    Scenario: A vertical range slider grows upwards
      Then the property "first.value" of "up" is 0
      And the property "second.value" of "up" is 1
      And the property "first.visualPosition" of "up" is 1
      And the property "second.visualPosition" of "up" is 0
      And the property "first.handle.y" of "up" is 166
      And the property "second.handle.y" of "up" is 6

  @qt-parity
  Rule: The keyboard

    Scenario: The keyboard goes to the first handle, and Tab to the second
      When the method "forceActiveFocus" of "r" is called
      Then the property "first.handle.activeFocus" of "r" is true
      When the user presses "Right"
      Then the property "first.value" of "r" is within 0.0001 of 0.35
      When the user presses "Tab"
      Then the property "second.handle.activeFocus" of "r" is true
      When the user presses "Right"
      Then the property "second.value" of "r" is within 0.0001 of 0.85
      And the property "first.value" of "r" is within 0.0001 of 0.35

    Scenario: The steps are the step size
      When the method "forceActiveFocus" of "tens" is called
      And the user presses "Right"
      Then the property "first.value" of "tens" is 3
      When the user presses "PgUp"
      And the user presses "End"
      Then the property "first.value" of "tens" is 3

  @qt-parity
  Rule: The pointer

    Scenario: A press takes the nearer handle and puts it under the pointer
      When the method "setValues" of "r" is called with 0.2, 0.6
      And the user presses the pointer at 68, 20
      Then the property "first.value" of "r" is 0.3
      And the property "first.pressed" of "r" is true
      And the property "first.handle.activeFocus" of "r" is true
      When the user moves the pointer to 36, 20
      Then the property "first.value" of "r" is 0.1
      When the user releases the pointer at 36, 20
      Then the property "first.pressed" of "r" is false
      And the property "second.value" of "r" is 0.6

    Scenario: A dragged handle stops at the other one
      When the method "setValues" of "r" is called with 0.1, 0.5
      And the user presses the pointer at 164, 20
      Then the property "second.value" of "r" is 0.9
      And the property "second.pressed" of "r" is true
      When the user moves the pointer to 28, 20
      Then the property "second.value" of "r" is 0.1
      And the property "first.value" of "r" is 0.1
