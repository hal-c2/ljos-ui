# Sources:
#   https://wayland.app/protocols/wayland#wl_seat:enum:capability
#   https://wayland.app/protocols/wayland#wl_keyboard:event:leave
#   https://wayland.app/protocols/virtual-keyboard-unstable-v1
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#active-prop

@ljos-only @desktop
Feature: The desktop sandbox's seat has a keyboard
  A compositor gives the keyboard focus to a client, and tells it when
  the focus has gone, only on a seat that has a keyboard. The desktop
  sandbox (`mise run desktop:cua`) runs a headless compositor. Its seat
  had no keyboard, so a window there never became active and never
  stopped being active, and whatever an application does when it loses
  the keyboard could not be seen there. The sandbox now gives its seat a
  keyboard before any application starts, so the focus comes and goes
  as it does on a desktop.

  # On the desktop surface: `ljos run` on the sandbox's sway. Another
  # client's window takes the focus, and the compositor gives it back.
  Scenario: Another client takes the focus, and the window gets it back
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "window"
        width: 200; height: 100; visible: true
        property string heard: ""
        onActiveChanged: heard += (heard ? "," : "") + "active=" + active
      }
      """
    When the window loses focus
    Then the property "heard" of "window" is "active=true,active=false"
    When the window gains focus
    Then the property "heard" of "window" is "active=true,active=false,active=true"
