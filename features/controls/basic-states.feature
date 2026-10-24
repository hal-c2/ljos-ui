# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-basic.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#visualFocus-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.RoundButton @api-QtQuick.Controls.DelayButton
@api-QtQuick.Controls.Switch @api-QtQuick.Controls.Slider
@api-QtQuick.Controls.RangeSlider
Feature: How the Basic style shows a control's state
  Held down, a round button and a delay button are half way from their
  colour to the palette's mid colour, like a button, and so is a delay
  button's fill. A switch's knob is outlined in the mid colour, or the
  midlight one when it is not enabled; with the keyboard the knob, not
  the track, is outlined in the highlight. A slider's knob keeps its
  outline's colour with the keyboard; the outline only grows thicker. The
  knob of a range slider that has the keyboard is outlined in the
  highlight.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Basic
      """

  Scenario Outline: The <property> of the <object>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        RoundButton { objectName: "round"; text: "A" }
        RoundButton { objectName: "downRound"; text: "A"; down: true }
        RoundButton { objectName: "downLitRound"; text: "A"; down: true; highlighted: true }
        RoundButton { objectName: "downCheckedRound"; text: "A"; down: true; checkable: true; checked: true }
        DelayButton { objectName: "delay"; text: "A" }
        DelayButton { objectName: "downDelay"; text: "A"; down: true }
        Switch { objectName: "switch"; text: "A" }
        Switch { objectName: "downSwitch"; text: "A"; down: true }
        Switch { objectName: "offSwitch"; text: "A"; enabled: false }
        Slider { objectName: "offSlider"; enabled: false }
      }
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object           | property                          | value     |
      | round            | background.color                  | "#e0e0e0" |
      | downRound        | background.color                  | "#cfcfcf" |
      | downLitRound     | background.color                  | "#797a7a" |
      | downCheckedRound | background.color                  | "#797a7a" |
      | delay            | background.color                  | "#e0e0e0" |
      | delay            | background.children.0.color       | "#353637" |
      | downDelay        | background.color                  | "#cfcfcf" |
      | downDelay        | background.children.0.color       | "#797a7a" |
      | switch           | indicator.children.0.border.color | "#bdbdbd" |
      | switch           | indicator.children.0.border.width | 1         |
      | downSwitch       | indicator.children.0.color        | "#f6f6f6" |
      | offSwitch        | indicator.children.0.border.color | "#e4e4e4" |
      | offSlider        | handle.border.color               | "#e4e4e4" |

  Scenario Outline: With the keyboard, the <object>'s <property> is <value>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 200
        <object> { objectName: "c"; <setup> }
      }
      """
    When the user presses "Tab"
    Then the property "<property>" of "c" is <value>

    Examples:
      | object      | setup                                | property                          | value     |
      | Switch      | text: "A"                            | visualFocus                       | true      |
      | Switch      | text: "A"                            | indicator.border.width            | 0         |
      | Switch      | text: "A"                            | indicator.children.0.border.color | "#0066ff" |
      | Switch      | text: "A"                            | indicator.children.0.border.width | 2         |
      | Slider      | value: 0.5                           | visualFocus                       | true      |
      | Slider      | value: 0.5                           | handle.border.color               | "#bdbdbd" |
      | Slider      | value: 0.5                           | handle.border.width               | 2         |
      | RangeSlider | first.value: 0.2; second.value: 0.8  | first.handle.border.color         | "#0066ff" |
      | RangeSlider | first.value: 0.2; second.value: 0.8  | first.handle.border.width         | 2         |
      | RoundButton | text: "A"                            | background.border.color           | "#0066ff" |

  Scenario: A round button held down is drawn half way to the mid colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 60; height: 60; color: "white"
        RoundButton { width: 60; height: 60; down: true }
      }
      """
    Then the pixel at 30, 30 of the window is within 2 of "#cfcfcf"
