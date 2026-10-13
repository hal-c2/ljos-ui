# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Moving a slider with the pointer
  A press on a slider puts the handle under the pointer and a drag takes it
  along. The handle's middle travels the track inside the padding, less the
  handle's own width: from x 20 to x 180 on a slider 200 wide.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 500
        Slider {
          objectName: "s"; property int moves: 0
          onMoved: moves++
        }
        Slider {
          objectName: "late"; y: 50; live: false
          property string log: ""
          onValueChanged: log += value + " "
        }
        Slider { objectName: "snap"; y: 100; stepSize: 0.25; snapMode: Slider.SnapAlways }
        Slider { objectName: "settle"; y: 150; stepSize: 0.25; snapMode: Slider.SnapOnRelease }
        Slider { objectName: "back"; y: 200; from: 10; to: 0 }
        Slider { objectName: "up"; x: 300; y: 100; orientation: Qt.Vertical }
        Slider {
          objectName: "rtl"; y: 250; value: 0.25
          LayoutMirroring.enabled: true
          property int moves: 0
          onMoved: moves++
        }
      }
      """

  @qt-parity
  Rule: Pressing and dragging

    Scenario: A press puts the handle under the pointer, and is a move
      When the user presses the pointer at 100, 20
      Then the property "value" of "s" is 0.5
      And the property "pressed" of "s" is true
      And the property "moves" of "s" is 1
      When the user moves the pointer to 150, 20
      Then the property "value" of "s" is 0.8125
      And the property "moves" of "s" is 2
      When the user releases the pointer at 150, 20
      Then the property "value" of "s" is 0.8125
      And the property "pressed" of "s" is false

    Scenario: The handle stops at the ends of the track
      When the user drags from 100, 20 to 390, 20
      Then the property "value" of "s" is 1
      When the user drags from 100, 20 to 2, 20
      Then the property "value" of "s" is 0

    @api-QtQuick.Controls.Slider.live
    Scenario: A slider that is not live takes the value when it is let go
      When the user presses the pointer at 100, 70
      Then the property "position" of "late" is 0.5
      And the property "value" of "late" is 0
      When the user moves the pointer to 150, 70
      Then the property "position" of "late" is 0.8125
      And the property "value" of "late" is 0
      When the user releases the pointer at 150, 70
      Then the property "value" of "late" is 0.8125
      And the property "log" of "late" is "0.8125 "

    @api-QtQuick.Controls.Slider.snapMode
    Scenario: A slider that always snaps goes from step to step
      When the user presses the pointer at 70, 120
      Then the property "value" of "snap" is 0.25
      And the property "position" of "snap" is 0.25
      When the user moves the pointer to 116, 120
      Then the property "value" of "snap" is 0.5

    @api-QtQuick.Controls.Slider.snapMode
    Scenario: A slider that snaps on release moves its handle freely until it is let go
      # The value is the nearest step all along; only the handle is free.
      When the user presses the pointer at 70, 170
      Then the property "position" of "settle" is 0.3125
      And the property "value" of "settle" is 0.25
      When the user moves the pointer to 80, 170
      Then the property "position" of "settle" is 0.375
      And the property "value" of "settle" is 0.5
      When the user releases the pointer at 80, 170
      Then the property "position" of "settle" is 0.5
      And the property "value" of "settle" is 0.5

    Scenario: The ends given the other way round are the other way round on the track
      When the user presses the pointer at 20, 220
      Then the property "value" of "back" is 10
      And the property "position" of "back" is 0

    Scenario: A vertical slider grows upwards
      When the user presses the pointer at 320, 160
      Then the property "value" of "up" is 0.75
      And the property "visualPosition" of "up" is 0.25

  @qt-parity
  Rule: A mirrored slider

    Scenario: The handle goes the other way
      Then the property "visualPosition" of "rtl" is 0.75
      And the property "handle.x" of "rtl" is 126

    Scenario: A press is read from the right
      When the user presses the pointer at 60, 270
      Then the property "value" of "rtl" is 0.75

    Scenario: The arrow keys go the other way
      When the method "forceActiveFocus" of "rtl" is called
      And the user presses "Left"
      Then the property "value" of "rtl" is within 0.0001 of 0.35
      And the property "moves" of "rtl" is 1
      When the user presses "Right"
      Then the property "value" of "rtl" is within 0.0001 of 0.25

  @ljos-only
  Rule: The wheel

    Scenario: The wheel steps a slider that takes it, and that is a move
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 1 notch up at 100, 20
      Then the property "value" of "s" is within 0.0001 of 0.1
      And the property "moves" of "s" is 1
      When the user turns the wheel 1 notch down at 100, 20
      Then the property "value" of "s" is 0

    Scenario: A slider that does not take the wheel leaves it alone
      When the user turns the wheel 1 notch up at 100, 20
      Then the property "value" of "s" is 0
