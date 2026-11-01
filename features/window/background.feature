# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html#background-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#color-prop
# The colour of a window is what the window is cleared to, so nothing in it
# can lie under that colour: a background has `z` -1 and is still seen.
#
# The oracle's driver grabs a window of its own and puts the file under it, so
# a file's own window is not what it reads: those two scenarios run on Ljós only.
# toycustomizer's reference picture shows the same background on Qt 6.11.2.
Feature: What lies behind the content of a window

  @ljos-only @api-QtQuick.Controls.ApplicationWindow.background
  Scenario: The background of an application window covers the window's colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls

      ApplicationWindow {
        width: 100; height: 100; visible: true
        color: "#ff0000"
        background: Rectangle { color: "#2060a0" }
      }
      """
    Then the pixel at 50, 50 of the window is "#2060a0"

  @ljos-only @api-QtQuick.Window.color
  Scenario: An item below the content of a window is seen over its colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Window

      Window {
        width: 100; height: 100; visible: true
        color: "#ff0000"
        Rectangle { z: -1; anchors.fill: parent; color: "#2060a0" }
      }
      """
    Then the pixel at 50, 50 of the window is "#2060a0"

  @qt-parity
  Scenario: Under an ordinary rectangle an item below it stays hidden
    Given the QML:
      """qml
      import QtQuick

      Rectangle {
        width: 100; height: 100
        color: "#ff0000"
        Rectangle { z: -1; anchors.fill: parent; color: "#2060a0" }
      }
      """
    Then the pixel at 50, 50 of the window is "#ff0000"
