# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The screen of a window
  A window knows the screen it is on, and a window shown over the whole
  screen is as large as that screen.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 300; height: 200
        visible: true
        readonly property bool fillsScreen: width === screen.width && height === screen.height
        readonly property real pixelRatio: screen.devicePixelRatio
        readonly property int screenWidth: screen.width
        readonly property int screenHeight: screen.height
      }
      """

  @api-QtQuick.Window.screen
  Scenario Outline: The screen has a size and a pixel ratio
    Then the property "<property>" of "w" is at least 1

    Examples:
      | property     |
      | screenWidth  |
      | screenHeight |
      | pixelRatio   |

  @api-QtQuick.Window.screen
  Scenario: A windowed window does not fill its screen
    Then the property "fillsScreen" of "w" is false

  @api-QtQuick.Window.showFullScreen
  Scenario: A window over the whole screen has the screen's size
    When the method "showFullScreen" of "w" is called
    And 50 milliseconds pass
    Then the property "fillsScreen" of "w" is true
    And the property "x" of "w" is 0
    And the property "y" of "w" is 0
