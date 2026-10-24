# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#hoverEnabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.hovered
@api-QtQuick.Controls.Button @api-QtQuick.Controls.ToolButton
@api-QtQuick.Controls.RoundButton @api-QtQuick.Controls.ComboBox
@api-QtQuick.Controls.DelayButton @api-QtQuick.Controls.Slider
@api-QtQuick.Controls.Switch @api-QtQuick.Controls.TabButton
@api-QtQuick.Controls.CheckBox
Feature: How the Fusion style shows the pointer over a control
  Under the pointer, Fusion lightens a button's face and its gradient, and
  the knob of a slider or switch, by Fusion's hovered shade; pressed too,
  the pressed shade is lightened the same way. A knob held by the pointer
  is darker. A control that is not enabled, a tab button, a check box and
  a spin box keep their shades. Off-screen there is no hover unless a
  control asks for it, so each one here does.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """

  Scenario Outline: The <property> of a hovered <type>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 200
        <type> { objectName: "c"; hoverEnabled: true; <setup> }
      }
      """
    When the user moves the pointer to 10, 10
    Then the property "<property>" of "c" is <value>

    Examples:
      | type        | setup                         | property                                    | value     |
      | Button      | text: "Ab"                    | hovered                                     | true      |
      | Button      | text: "Ab"                    | background.color                            | "#f1f1f1" |
      | Button      | text: "Ab"                    | background.gradient.stops.0.color           | "#ffffff" |
      | Button      | text: "Ab"                    | background.gradient.stops.1.color           | "#f6f6f6" |
      | Button      | text: "Ab"                    | background.border.color                     | "#ababab" |
      | Button      | text: "Ab"; highlighted: true | background.color                            | "#dde6ec" |
      | Button      | text: "Ab"; highlighted: true | background.gradient.stops.1.color           | "#e1ebf1" |
      | Button      | text: "Ab"; checkable: true; checked: true | background.color               | "#dbdbdb" |
      | Button      | text: "Ab"; palette.button: "#808080" | background.color                    | "#8a8a8a" |
      | Button      | text: "Ab"; palette.button: "#808080" | background.gradient.stops.1.color   | "#8d8d8d" |
      | Button      | text: "Ab"; enabled: false    | background.color                            | "#e8e8e8" |
      | ToolButton  | text: "Ab"                    | background.visible                          | true      |
      | ToolButton  | text: "Ab"                    | background.color                            | "#f1f1f1" |
      | RoundButton | text: "A"                     | background.gradient.stops.0.color           | "#ffffff" |
      | RoundButton | text: "A"                     | background.gradient.stops.1.color           | "#f6f6f6" |
      | ComboBox    | model: ["Ab"]                 | background.color                            | "#f1f1f1" |
      | ComboBox    | model: ["Ab"]                 | background.gradient.stops.1.color           | "#f6f6f6" |
      | DelayButton | text: "Ab"                    | background.color                            | "#f1f1f1" |
      | Switch      | text: "Ab"                    | indicator.children.1.gradient.stops.0.color | "#ffffff" |
      | Switch      | text: "Ab"                    | indicator.children.1.gradient.stops.1.color | "#f6f6f6" |
      | Slider      | value: 0                      | handle.gradient.stops.0.color               | "#ffffff" |
      | Slider      | value: 0                      | handle.gradient.stops.1.color               | "#f6f6f6" |
      | Slider      | value: 0; palette.button: "#808080" | handle.gradient.stops.0.color         | "#ababab" |
      | Slider      | value: 0; palette.button: "#808080" | handle.gradient.stops.1.color         | "#8d8d8d" |
      | TabButton   | text: "Ab"                    | background.gradient.stops.0.color           | "#dfdfdf" |
      | TabButton   | text: "Ab"                    | background.gradient.stops.2.color           | "#d0d0d0" |
      | CheckBox    | text: "Ab"                    | indicator.color                             | "#ffffff" |

  Scenario Outline: The <property> of a hovered <type> that is pressed
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 200
        <type> { objectName: "c"; hoverEnabled: true; <setup> }
      }
      """
    When the user moves the pointer to 10, 10
    And the user presses the pointer at 10, 10
    Then the property "<property>" of "c" is <value>

    Examples:
      | type        | setup                         | property                                    | value     |
      | Button      | text: "Ab"                    | background.color                            | "#dbdbdb" |
      | Button      | text: "Ab"; highlighted: true | background.color                            | "#c9d1d7" |
      | ToolButton  | text: "Ab"                    | background.color                            | "#dbdbdb" |
      | RoundButton | text: "A"                     | background.gradient.stops.0.color           | "#dbdbdb" |
      | RoundButton | text: "A"                     | background.gradient.stops.1.color           | "#dbdbdb" |
      | ComboBox    | model: ["Ab"]                 | background.color                            | "#dbdbdb" |
      | DelayButton | text: "Ab"                    | background.color                            | "#dbdbdb" |
      | Switch      | text: "Ab"                    | indicator.children.1.gradient.stops.0.color | "#ffffff" |
      | Switch      | text: "Ab"                    | indicator.children.1.gradient.stops.1.color | "#dfdfdf" |
      | Slider      | value: 0                      | handle.gradient.stops.1.color               | "#dfdfdf" |
      | Slider      | value: 0; palette.button: "#808080" | handle.gradient.stops.0.color         | "#9c9c9c" |
      | Slider      | value: 0; palette.button: "#808080" | handle.gradient.stops.1.color         | "#808080" |
      | CheckBox    | text: "Ab"                    | indicator.color                             | "#d8d8d8" |

  Scenario Outline: A <type>'s knob held without hover is darker
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 200
        <type> { objectName: "c"; <setup> }
      }
      """
    When the user presses the pointer at 10, 10
    Then the property "<property>" of "c" is <value>

    Examples:
      | type   | setup      | property                                    | value     |
      | Slider | value: 0   | handle.gradient.stops.0.color               | "#ffffff" |
      | Slider | value: 0   | handle.gradient.stops.1.color               | "#d7d7d7" |
      | Switch | text: "Ab" | indicator.children.1.gradient.stops.1.color | "#d7d7d7" |

  Scenario: A switch set down, not held, keeps its knob's shade
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Switch { objectName: "c"; down: true }
      """
    Then the property "indicator.children.1.gradient.stops.1.color" of "c" is "#ececec"
