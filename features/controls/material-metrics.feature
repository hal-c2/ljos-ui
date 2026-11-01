# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#padding-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Material controls have Material's numbers
  A Material button's text is in the middle of it, a disabled label is in the
  hint colour, and a slider's track is as wide as the handle travels, with what
  is left of it a faint accent.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        objectName: "root"
        width: 400; height: 400
        Material.theme: Material.Light
        Button { objectName: "plain"; id: plain; text: "Throw" }
        Button { objectName: "flat"; flat: true; y: 60; text: "Throw" }
        Label { objectName: "on"; id: on; y: 120; text: "L" }
        Label { objectName: "off"; id: off; y: 140; text: "L"; enabled: false }
        Slider { objectName: "slider"; id: slider; y: 160; width: 200; value: 0.5 }
        readonly property string onColor: String(on.color)
        readonly property string offColor: String(off.color)
        readonly property var track: slider.background.children[0]
        readonly property string trackColor: String(track.color)
        readonly property var filled: track.children[0]
        readonly property string filledColor: String(filled.color)
        readonly property string accentColor: String(slider.Material.accent)
      }
      """

  Scenario Outline: A button is padded alike on both sides, and well rounded
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value |
      | plain  | leftPadding       | 24    |
      | plain  | rightPadding      | 24    |
      | plain  | topPadding        | 14    |
      | plain  | bottomPadding     | 14    |
      | plain  | background.radius | 20    |
      | flat   | leftPadding       | 12    |
      | flat   | rightPadding      | 12    |

  Scenario: A disabled label is in the hint colour
    Then the property "onColor" of "root" is "#dd000000"
    And the property "offColor" of "root" is "#60000000"

  Scenario Outline: A slider's handle and track have Material's numbers
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property          | value |
      | slider | handle.width      | 13    |
      | slider | handle.height     | 13    |
      | slider | handle.x          | 93.5  |
      | slider | background.width  | 188   |
      | slider | background.height | 4     |
      | root   | track.x           | 6.5   |
      | root   | track.width       | 175   |
      | root   | track.height      | 4     |
      | root   | filled.width      | 87.5  |

  Scenario: What is left of a slider's track is a faint accent
    Then the property "trackColor" of "root" is "#54e91e63"
    And the property "filledColor" of "root" is "#e91e63"
