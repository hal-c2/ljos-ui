# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-switch.html
# Verified on Qt 6.11.2 (qml6, offscreen): sizes read from the controls,
# colours from a picture of the window.

@qt-parity
Feature: The Material Switch
  A Material switch is a pill-shaped track 52 by 32 with a round handle.
  Off, the track is light with a grey outline and the handle a small grey
  dot at the start; on, the track is the accent colour and the handle a
  larger white disc at the end. Disabled, it is faint. The switch is as
  tall as its text and padding, so one without text is as tall as its
  track and a column of them is no taller than its tracks.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Rectangle {
        width: 200; height: 120; color: "white"
        Switch { objectName: "off" }
        Switch { objectName: "on"; y: 40; checked: true }
        Switch { objectName: "dis"; y: 80; checked: true; enabled: false }
      }
      """

  Scenario Outline: A Material switch is <value> in <property>
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property               | value     |
      | off    | implicitWidth          | 76        |
      | off    | implicitHeight         | 32        |
      | off    | indicator.x            | 12        |
      | off    | indicator.y            | 0         |
      | off    | indicator.width        | 52        |
      | off    | indicator.height       | 32        |
      | off    | indicator.color        | "#e7e0ec" |
      | off    | indicator.border.color | "#757575" |
      | on     | indicator.color        | "#e91e63" |
      | dis    | indicator.color        | "#1e1c1b1f" |

  Scenario Outline: The pixel at <x>, <y> is the <part>
    Then the pixel at <x>, <y> of the window is "<colour>"

    Examples:
      | x  | y  | part                          | colour  |
      | 28 | 16 | small handle of the off one   | #757575 |
      | 38 | 16 | off track beside its handle   | #e7e0ec |
      | 28 | 25 | off track below its handle    | #e7e0ec |
      | 48 | 56 | handle of the on one          | #ffffff |
      | 38 | 56 | large handle's edge           | #ffffff |
      | 16 | 56 | on track                      | #e91e63 |
      | 48 | 96 | disabled one's handle         | #fffbfe |
      | 20 | 96 | disabled one's track          | #e4e4e5 |
