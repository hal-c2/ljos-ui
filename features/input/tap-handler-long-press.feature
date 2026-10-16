# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#longPressed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#longPressThreshold-prop
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#timeHeld-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A TapHandler's long press
  A press held for `longPressThreshold` seconds, without moving beyond the drag threshold, is a
  long press: the handler says `longPressed` while it is still pressed, and letting go then is no
  tap. `timeHeld` is how long the press has been held, and -1 while nothing is pressed. A
  threshold of 0 turns long presses off.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        Item {
          width: 100; height: 100
          TapHandler {
            objectName: "tap"
            longPressThreshold: 0.3
            property string log: ""
            onLongPressed: log += "long " + timeHeld.toFixed(1) + " " + pressed + " "
            onTapped: log += "tapped "
          }
        }
      }
      """

  Rule: A press held long enough is a long press, not a tap

    Scenario: A press held past the threshold is a long press
      When the user presses the pointer at 10, 10
      And 500 milliseconds pass
      Then the property "log" of "tap" is "long 0.3 true "
      When the user releases the pointer at 10, 10
      Then the property "log" of "tap" is "long 0.3 true "

    Scenario: A press let go before the threshold is a tap
      When the user presses the pointer at 10, 10
      And 100 milliseconds pass
      And the user releases the pointer at 10, 10
      And 500 milliseconds pass
      Then the property "log" of "tap" is "tapped "

    Scenario: A press that moves beyond the drag threshold is no long press
      When the user presses the pointer at 10, 10
      And the user moves the pointer to 60, 10
      And 500 milliseconds pass
      Then the property "log" of "tap" is ""

    Scenario: A threshold of 0 turns long presses off
      When the property "longPressThreshold" of "tap" is set to 0
      And the user presses the pointer at 10, 10
      And 500 milliseconds pass
      Then the property "log" of "tap" is ""
      When the user releases the pointer at 10, 10
      Then the property "log" of "tap" is "tapped "

  Rule: timeHeld is -1 while nothing is pressed

    Scenario: timeHeld before a press and after it
      Then the property "timeHeld" of "tap" is -1
      When the user touches 10, 10
      Then the property "timeHeld" of "tap" is -1
