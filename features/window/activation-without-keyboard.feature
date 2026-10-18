# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#active-prop
#   https://wayland.app/protocols/xdg-shell#xdg_toplevel:enum:state
# Seen on Qt 6.11.2 (qml6 on a Wayland seat with a pointer and no
# keyboard): a TextField's cursor blinks after a press on it.

@ljos-only @desktop
Feature: A window is active on a seat without a keyboard
  Some systems have a pointer or a touch screen and no keyboard: a kiosk,
  a tablet, a headless session driven by a virtual pointer. No keyboard
  focus ever comes to a window there, yet the window the user presses is
  the one they work in, and its field must take the focus so that an
  on-screen keyboard has somewhere to type. Until a keyboard has been
  heard of, a press on a window that is not active makes it the active
  one; once one has, the system says which window is active.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      ApplicationWindow {
        objectName: "window"
        width: 200; height: 80; visible: true
        TextField { objectName: "field"; x: 10; y: 10; width: 180 }
      }
      """

  # Blocked: the runner gives every window a seat with a keyboard; this
  # waits for a step that starts it without one. The rule is covered by
  # ljos_host's unit tests of the winit host until then.
  @blocked @api-QtQuick.Window.active
  Scenario: A press makes the window active and gives the field the focus
    When the user presses the pointer at 50, 30
    And the user releases the pointer at 50, 30
    Then the property "active" of "window" is true
    And the property "activeFocus" of "field" is true
