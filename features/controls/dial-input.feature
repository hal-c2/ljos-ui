# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: How a dial takes the pointer
  `inputMode` says whether the pointer turns a dial round its middle or
  along a line, `live` whether the value follows the pointer or waits for
  it to let go, and `snapMode` whether it goes in steps.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Dial { objectName: "nl"; live: false }
        Dial {
          objectName: "hz"; x: 200; width: 100; height: 150
          inputMode: Dial.Horizontal
          property int moves: 0; onMoved: moves++
        }
        Dial { objectName: "vt"; x: 300; width: 100; height: 150; inputMode: Dial.Vertical }
        Dial { objectName: "so"; y: 200; stepSize: 0.25; snapMode: Dial.SnapOnRelease }
      }
      """

  @qt-parity
  @api-QtQuick.Controls.Dial.live
  Rule: A dial that is not live

    Scenario: It takes the value when the pointer lets go
      When the user presses the pointer at 92, 10
      Then the property "value" of "nl" is 0
      And the property "position" of "nl" is 0.5
      And the property "angle" of "nl" is 0
      When the user releases the pointer at 92, 10
      Then the property "value" of "nl" is 0.5

  @qt-parity
  @api-QtQuick.Controls.Dial.inputMode
  Rule: Along a line

    Scenario: A horizontal dial turns as the pointer goes right, twice its width for the whole way
      When the user presses the pointer at 250, 75
      Then the property "value" of "hz" is 0
      And the property "moves" of "hz" is 0
      When the user moves the pointer to 270, 75
      Then the property "value" of "hz" is within 0.0001 of 0.1
      And the property "moves" of "hz" is 1
      When the user moves the pointer to 270, 40
      Then the property "value" of "hz" is within 0.0001 of 0.1
      When the user releases the pointer at 270, 40
      Then the property "moves" of "hz" is 1

    Scenario: A vertical dial turns as the pointer goes up, twice its height for the whole way
      When the user presses the pointer at 350, 75
      And the user moves the pointer to 350, 45
      Then the property "value" of "vt" is within 0.0001 of 0.1
      When the user moves the pointer to 380, 45
      Then the property "value" of "vt" is within 0.0001 of 0.1

  @qt-parity
  @api-QtQuick.Controls.Dial.snapMode
  Rule: Snapping on release

    Scenario: The value is free until the pointer lets go
      When the user presses the pointer at 92, 210
      Then the property "value" of "so" is 0.5
      When the user moves the pointer to 160, 240
      Then the property "value" of "so" is within 0.0001 of 0.6878
      And the property "angle" of "so" is within 0.01 of 52.59
      When the user releases the pointer at 160, 240
      Then the property "value" of "so" is 0.75
      And the property "position" of "so" is 0.75
