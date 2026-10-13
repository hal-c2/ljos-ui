# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#platform-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.platform
  The operating system the application runs on, and the window system it
  shows its windows with. A test run has no window system: it draws off
  screen.

  Scenario: Qt.platform.os names the operating system
    Given the QML:
      """qml
      import QtQuick
      Item { objectName: "o"; property string v: Qt.platform.os }
      """
    Then the property "v" of "o" is "linux"

  Scenario: A run without a window system is off screen
    Given the QML:
      """qml
      import QtQuick
      Item { objectName: "o"; property string v: Qt.platform.pluginName }
      """
    Then the property "v" of "o" is "offscreen"
