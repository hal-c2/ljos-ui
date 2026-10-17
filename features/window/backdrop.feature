# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#color-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#flags-prop
# Qt has no way for a QML window to ask the window system to blur what is
# behind it; `backdrop` is Ljós's own, so the scenarios run on Ljós only.
# What the window system does with the request cannot be seen without one:
# the scenarios say what the window system is asked for. How each platform
# answers is in docs/ARCHITECTURE.md (section 7.3, the backdrop).

@ljos-only
Feature: A window asks for a blurred backdrop
  A window's backdrop is what the window system draws behind it, seen
  through the parts of the window its colour leaves see-through. A window
  asks for a blurred one with `backdrop: Window.BlurBackdrop`; where the
  window system has none, nothing changes. `backdrop` reads what the window asked for. The
  backdrop is light or dark with the colour scheme in effect: the
  application's own when it set one, else the system's.

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

  Scenario: A blurred backdrop is dark while the system's scheme is
    Given the system setting "color scheme" is dark
    And the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        color: "transparent"
        backdrop: Window.BlurBackdrop
      }
      """
    Then the window system is asked for a dark blurred backdrop behind "w"
    When the system setting "color scheme" changes to light
    Then the window system is asked for a light blurred backdrop behind "w"

  Scenario: The application's own colour scheme picks the backdrop's
    Given the system setting "color scheme" is dark
    And the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        color: "transparent"
        backdrop: Window.BlurBackdrop
        function light() { Qt.styleHints.colorScheme = Qt.ColorScheme.Light }
        function system() { Qt.styleHints.colorScheme = Qt.ColorScheme.Unknown }
      }
      """
    When the method "light" of "w" is called
    Then the window system is asked for a light blurred backdrop behind "w"
    When the method "system" of "w" is called
    Then the window system is asked for a dark blurred backdrop behind "w"
