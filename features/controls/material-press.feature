# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://m3.material.io/foundations/interaction/states/state-layers
# Verified on Qt 6.11.2 (qml6 on a compositor, by hand): colours from
# pictures of the window taken while the buttons were held.

# The oracle's vnc platform, the one that takes input, draws no Material
# button at all, so it cannot see the press.
@ljos-only
Feature: A Material button answers a press
  A pressed Material button is covered by a faint layer of the theme's text
  colour. A round button takes it at once and drops it at once on release;
  a button's layer comes in over a tenth of a second and fades out over
  less than half a second after release. The press shows in the next frame.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Rectangle {
        width: 300; height: 80; color: "white"
        RoundButton { objectName: "round"; x: 10; y: 10; text: "+" }
        Button { objectName: "button"; x: 80; y: 10; text: "Button" }
      }
      """

  Scenario: A round button darkens while it is held
    Then the pixel at 24, 36 of the window is within 3 of "#d6d7d7"
    When the user presses the pointer at 24, 36
    And 20 milliseconds pass
    Then the pixel at 24, 36 of the window is within 3 of "#c9caca"
    When the user releases the pointer at 24, 36
    And 20 milliseconds pass
    Then the pixel at 24, 36 of the window is within 3 of "#d6d7d7"

  Scenario: A button's layer comes in and fades out
    When the user presses the pointer at 100, 36
    And 300 milliseconds pass
    Then the pixel at 100, 24 of the window is within 3 of "#c9caca"
    When the user releases the pointer at 100, 36
    And 1000 milliseconds pass
    Then the pixel at 100, 24 of the window is within 3 of "#d6d7d7"
