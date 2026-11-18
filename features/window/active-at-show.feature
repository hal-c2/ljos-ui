# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#active-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#requestActivate-method
# Verified on Qt 6.12.0 (qml6 under XWayland in the desktop sandbox): a
# window shown at start reads active false, then hears one change, to
# true, when the system gives it the keyboard.

@qt-parity @desktop
Feature: A window that has just been shown has not lost the keyboard
  A window is not active until the system gives it the keyboard. One that
  has just been shown has never had the keyboard, so it hears no change to
  inactive: a handler that hides it when it loses the keyboard, as a
  dropdown terminal's does, would hide it as it appears.

  @api-QtQuick.Window.active
  Scenario: A window shown at start never hears that it is inactive
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "window"
        width: 200; height: 100; visible: true
        property int losses: 0
        onActiveChanged: if (!active) losses += 1
      }
      """
    When 1 second passes
    Then the property "losses" of "window" is 0
