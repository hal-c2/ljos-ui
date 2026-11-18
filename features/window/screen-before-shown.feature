# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#screen-prop
# Verified on Qt 6.11.2 (qml6, wayland and xcb, on the desktop sandbox's sway with one 1600 by 1000 output).

@qt-parity
Feature: The screen a window is made on
  A window's screen is a display of the system's from the moment the
  window is made, before it is on the screen: what `Component.onCompleted`
  reads of `Screen` to size and place the window is the display it will be
  shown on, not a stand-in. The desktop sandbox has one display, 1600 by
  1000.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: w
        objectName: "root"
        visible: false
        property int madeWidth: -1
        property int madeHeight: -1
        property int madeScreenWidth: -1
        property int madeScreens: -1
        Component.onCompleted: {
          madeWidth = Screen.width
          madeHeight = Screen.height
          madeScreenWidth = w.screen.width
          madeScreens = Qt.application.screens.length
          width = Math.round(Screen.width / 2)
          height = Math.round(Screen.height * 0.4)
          visible = true
        }
      }
      """

  @desktop @api-QtQuick.Screen.width @api-QtQuick.Screen.height @api-QtQuick.Window.screen
  Scenario Outline: What a window reads as it is made is the display it is shown on
    Then the property "<property>" of "root" is <value>

    Examples:
      | property        | value |
      | madeWidth       | 1600  |
      | madeHeight      | 1000  |
      | madeScreenWidth | 1600  |
      | madeScreens     | 1     |
      | width           | 800   |
      | height          | 400   |
