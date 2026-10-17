# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#timeHeld-prop
# Verified on Qt 6.11.2 with qmltestrunner on the offscreen platform: about
# 30 changes in 300 ms. The oracle runs on the vnc platform, which draws no
# frames until a client looks, so there timeHeld changes once: the oracle
# cannot observe this.

@ljos-only
Feature: A TapHandler's timeHeld while the press is held
  `timeHeld` is brought up to date once for every frame drawn while the
  press is held. Something drawn from it changes the next frame, so it
  keeps moving; with nothing drawn, no frames are drawn for it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        TapHandler {
          id: tap
          objectName: "tap"
          longPressThreshold: 10
          property int changes: 0
          onTimeHeldChanged: changes++
        }
        Rectangle { width: tap.timeHeld * 100 + 1; height: 10 }
      }
      """

  Scenario: A bar drawn from timeHeld grows while the press is held
    When the user presses the pointer at 50, 50
    And 300 milliseconds pass
    Then the property "changes" of "tap" is at least 5
    And the property "timeHeld" of "tap" is at least 0.2
    And the property "timeHeld" of "tap" is at most 0.3
    When the user releases the pointer at 50, 50
    Then the property "timeHeld" of "tap" is -1
