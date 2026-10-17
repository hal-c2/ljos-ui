# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#color-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#flags-prop
# Qt has no way for a QML window to ask the window system to blur what is
# behind it; `backdrop` is Ljós's own, so the scenarios run on Ljós only.
# What the window system does with the request cannot be seen without one:
# the scenarios say what the window system is asked for.

@ljos-only
Feature: A window asks for a blurred backdrop
  A window's backdrop is what the window system draws behind it, seen
  through the parts of the window its colour leaves see-through. A window
  asks for a blurred one with `backdrop: Window.BlurBackdrop`; where the
  window system has none, nothing changes. `backdrop` reads what the
  window asked for.

  Scenario: A window asks for no backdrop unless it says so
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        property bool none: backdrop === Window.NoBackdrop
      }
      """
    Then the property "none" of "w" is true
    And the window system is asked for no backdrop behind "w"

  Scenario: A see-through window asks for a blurred backdrop
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        color: "#80000000"
        backdrop: Window.BlurBackdrop
      }
      """
    Then the window system is asked for a blurred backdrop behind "w"

  Scenario: A window that gives up its backdrop asks for none again
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        color: "transparent"
        backdrop: Window.BlurBackdrop
        function plain() { backdrop = Window.NoBackdrop }
      }
      """
    When the method "plain" of "w" is called
    Then the property "backdrop" of "w" is 0
    And the window system is asked for no backdrop behind "w"

  Scenario: A window that asks for a backdrop later is asked for it then
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        color: "transparent"
      }
      """
    When the property "backdrop" of "w" is set to 1
    Then the window system is asked for a blurred backdrop behind "w"
