# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.CheckBox @api-QtQuick.Controls.RadioButton
@api-QtQuick.Controls.Switch @api-QtQuick.Controls.Slider
@api-QtQuick.Controls.TextField @api-QtQuick.Controls.ComboBox
@api-QtQuick.Controls.SpinBox @api-QtQuick.Controls.RoundButton
Feature: How the Fusion style shows a control's state
  Pressed, a check box or radio button is shaded; one that is not enabled
  is filled with the window's colour and its mark is grey. A switch's or
  slider's filled part is the highlight, and grey when it is not enabled;
  a switch's fill shows only once it is checked, and stays in the
  highlight when it is not enabled. A round button is a gradient over
  white, flat in the pressed shade while it is down or checked.
  A slider's handle sits where its value is, across or down: a white knob
  in an outline of the window's colour, darker. A groove is shaded from
  top to bottom, a vertical one too. A field that
  has the keyboard is outlined in a darker highlight, and so is a spin
  box; a combo box keeps its outline. A spin box's step is shaded while it
  is pressed.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "States.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        CheckBox { objectName: "check"; text: "Ab" }
        CheckBox { objectName: "pressedCheck"; text: "Ab"; down: true }
        CheckBox { objectName: "offCheck"; text: "Ab"; enabled: false; checked: true }
        RadioButton { objectName: "pressedRadio"; text: "Ab"; down: true }
        RadioButton { objectName: "offRadio"; text: "Ab"; enabled: false }
        Switch { objectName: "switch"; text: "Ab" }
        Switch { objectName: "offSwitch"; text: "Ab"; enabled: false; checked: true }
        Switch { objectName: "onSwitch"; text: "Ab"; checked: true }
        RoundButton { objectName: "round"; text: "A" }
        RoundButton { objectName: "downRound"; text: "A"; down: true }
        RoundButton { objectName: "checkedRound"; text: "A"; checkable: true; checked: true }
        RoundButton { objectName: "litRound"; text: "A"; highlighted: true; down: true }
        Slider { objectName: "slider"; value: 0.5 }
        Slider { objectName: "pressedSlider"; value: 0.5; pressed: true }
        Slider { objectName: "offSlider"; value: 0.5; enabled: false }
        Slider { objectName: "upright"; value: 0.5; orientation: Qt.Vertical }
        TextField { objectName: "field"; text: "Ab" }
        TextField { objectName: "offField"; text: "Ab"; enabled: false }
        ComboBox { objectName: "combo"; model: ["Ab"] }
        ComboBox { objectName: "pressedCombo"; model: ["Ab"]; down: true }
        ComboBox { objectName: "offCombo"; model: ["Ab"]; enabled: false }
        SpinBox { objectName: "spin"; value: 5 }
        SpinBox { objectName: "upSpin"; value: 5; up.pressed: true }
        SpinBox { objectName: "downSpin"; value: 5; down.pressed: true }
        SpinBox { objectName: "offSpin"; enabled: false }
      }
      """

  Scenario Outline: The <property> of the <object>
    Given the QML:
      """qml
      import QtQuick
      States {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object        | property                 | value     |
      | check         | indicator.color          | "#ffffff" |
      | check         | indicator.border.color   | "#bcbcbc" |
      | check         | indicator.x              | 6         |
      | check         | indicator.y              | 6         |
      | pressedCheck  | indicator.color          | "#d8d8d8" |
      | offCheck      | indicator.color          | "#efefef" |
      | offCheck      | indicator.border.color   | "#bcbcbc" |
      | pressedRadio  | indicator.color          | "#d8d8d8" |
      | pressedRadio  | indicator.border.color   | "#9f9f9f" |
      | offRadio      | indicator.color          | "#ffffff" |
      | switch        | indicator.x              | 6         |
      | switch        | indicator.y              | 6         |
      | switch        | indicator.width          | 40        |
      | switch        | indicator.height         | 16        |
      | switch        | indicator.border.color   | "#ababab" |
      | offSwitch     | indicator.border.color   | "#ababab" |
      | slider        | background.y             | 4         |
      | slider        | background.width         | 160       |
      | slider        | background.height        | 5         |
      | slider        | background.border.color  | "#ababab" |
      | slider        | handle.x                 | 74        |
      | slider        | handle.y                 | 0         |
      | slider        | handle.width             | 13        |
      | slider        | handle.color             | "#ffffff" |
      | slider        | handle.border.color      | "#00000000" |
      | slider        | handle.children.0.border.color | "#ababab" |
      | pressedSlider | handle.x                 | 74        |
      | offSlider     | handle.x                 | 74        |
      | offSlider     | handle.color             | "#ffffff" |
      | offSlider     | handle.children.0.border.color | "#ababab" |
      | offSwitch     | indicator.children.1.color | "#ffffff" |
      | switch        | indicator.children.0.opacity | 0       |
      | switch        | indicator.children.0.visible | true    |
      | switch        | indicator.children.0.width | 20        |
      | onSwitch      | indicator.children.0.opacity | 1       |
      | onSwitch      | indicator.children.0.width | 40        |
      | onSwitch      | indicator.children.0.border.color | "#236690" |
      | onSwitch      | indicator.children.0.gradient.stops.0.color | "#308cc6" |
      | onSwitch      | indicator.children.0.gradient.stops.1.color | "#3aa8ee" |
      | offSwitch     | indicator.children.0.border.color | "#696969" |
      | offSwitch     | indicator.children.0.border.width | 0 |
      | offSwitch     | indicator.children.0.gradient.stops.0.color | "#308cc6" |
      | round         | background.color         | "#ffffff" |
      | round         | background.radius        | 16        |
      | round         | background.gradient.stops.0.color | "#ffffff" |
      | round         | background.gradient.stops.1.color | "#ececec" |
      | downRound     | background.color         | "#ffffff" |
      | downRound     | background.gradient.stops.0.color | "#d3d3d3" |
      | downRound     | background.gradient.stops.1.color | "#d3d3d3" |
      | checkedRound  | background.gradient.stops.0.color | "#d3d3d3" |
      | litRound      | background.color         | "#ffffff" |
      | litRound      | background.gradient.stops.0.color | "#c1c9ce" |
      | litRound      | background.border.color  | "#26709e" |
      | upright       | background.x             | 4         |
      | upright       | background.width         | 5         |
      | upright       | background.height        | 160       |
      | upright       | handle.x                 | 0         |
      | upright       | handle.y                 | 74        |
      | field         | background.color         | "#ffffff" |
      | field         | background.border.color  | "#ababab" |
      | offField      | background.color         | "#efefef" |
      | combo         | background.color         | "#e8e8e8" |
      | combo         | background.border.color  | "#ababab" |
      | combo         | indicator.x              | 100       |
      | combo         | indicator.y              | 9         |
      | combo         | indicator.width          | 20        |
      | pressedCombo  | background.color         | "#d3d3d3" |
      | offCombo      | background.border.color  | "#c4c4c4" |
      | spin          | background.color         | "#ffffff" |
      | spin          | up.indicator.color       | "#00000000" |
      | upSpin        | up.indicator.color       | "#dbdbdb" |
      | upSpin        | down.indicator.color     | "#00000000" |
      | downSpin      | down.indicator.color     | "#dbdbdb" |
      | offSpin       | background.color         | "#efefef" |
      | offSpin       | background.border.color  | "#ababab" |

  Scenario Outline: With the keyboard, the <object>'s outline is <value>
    Given the QML:
      """qml
      import QtQuick
      States {}
      """
    When the method "forceActiveFocus" of "<object>" is called
    Then the property "background.border.color" of "<object>" is <value>

    Examples:
      | object | value     |
      | field  | "#26709e" |
      | combo  | "#ababab" |

  Scenario: A vertical slider's groove is shaded from top to bottom
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 40; height: 170; color: "white"
        Slider { orientation: Qt.Vertical; value: 0.5 }
      }
      """
    Then the pixel at 6, 8 of the window is within 4 of "#bfbfbf"
    And the pixel at 6, 64 of the window is within 4 of "#cdcdcd"
    And the pixel at 6, 100 of the window is within 6 of "#3393d0"
    And the pixel at 6, 150 of the window is within 6 of "#39a5e9"
