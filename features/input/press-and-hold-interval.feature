# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-stylehints.html#mousePressAndHoldInterval-prop
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#longPressThreshold-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#pressAndHold-signal
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: The press-and-hold interval
  `Qt.styleHints.mousePressAndHoldInterval` is how long a press is held before it is a press and
  hold: 800 milliseconds unless the platform says otherwise. A TapHandler's `longPressThreshold`
  starts from it, and a button's `pressAndHold` waits for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        objectName: "root"
        width: 300; height: 200
        property int interval: Qt.styleHints.mousePressAndHoldInterval
        property string log: ""
        Item {
          width: 100; height: 100
          TapHandler {
            objectName: "tap"
            onLongPressed: log += "long "
          }
        }
        Button {
          x: 150; width: 100; height: 100
          onPressAndHold: log += "held "
        }
      }
      """

  @api-QtQuick.StyleHints.mousePressAndHoldInterval @api-QtQuick.TapHandler.longPressThreshold
  Scenario: The interval and the threshold that starts from it
    Then the property "interval" of "root" is 800
    And the property "longPressThreshold" of "tap" is 0.8

  @api-QtQuick.TapHandler.longPressed
  Scenario: A TapHandler's long press waits for the interval
    When the user presses the pointer at 10, 10
    And 600 milliseconds pass
    Then the property "log" of "root" is ""
    When 300 milliseconds pass
    Then the property "log" of "root" is "long "

  @api-QtQuick.Controls.AbstractButton.pressAndHold
  Scenario: A button's press and hold waits for the interval
    When the user presses the pointer at 200, 50
    And 600 milliseconds pass
    Then the property "log" of "root" is ""
    When 300 milliseconds pass
    Then the property "log" of "root" is "held "
