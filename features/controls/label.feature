# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-label.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Label
  A label is a text that takes its font and colour from the control or
  window it is in, and may have a background.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Label { objectName: "plain" }
        Label {
          objectName: "l"
          y: 40; width: 120; height: 30
          text: "Name"
          background: Rectangle { objectName: "bg"; implicitWidth: 50; implicitHeight: 10; color: "#ffee88" }
        }
        Pane {
          y: 100
          font.pixelSize: 30; font.bold: true
          palette.windowText: "#aa0000"
          Label { objectName: "inner"; text: "In a pane" }
        }
      }
      """

  Scenario Outline: A label is a text
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property      | value     |
      | plain  | text          | ""        |
      | plain  | implicitWidth | 0         |
      | plain  | background    | null      |
      | plain  | color         | "#26282a" |
      | plain  | wrapMode      | 0         |
      | l      | text          | "Name"    |
      | l      | font.bold     | false     |

  @api-QtQuick.Controls.Label.background
  Scenario Outline: The background fills the label and lies behind the text
    Then the property "<property>" of "bg" is <value>

    Examples:
      | property | value |
      | x        | 0     |
      | y        | 0     |
      | width    | 120   |
      | height   | 30    |
      | z        | -1    |

  Scenario: The background is a child of the label
    Then the property "parent.objectName" of "bg" is "l"
    And the property "background.objectName" of "l" is "bg"

  @api-QtQuick.Controls.Label.implicitBackgroundWidth
  Scenario: The label tells the implicit size of its background, which does not size the label
    Then the property "implicitBackgroundWidth" of "l" is 50
    And the property "implicitBackgroundHeight" of "l" is 10
    And the property "implicitBackgroundWidth" of "plain" is 0

  @api-QtQuick.Controls.Label.topInset
  Scenario: Insets move the edges of the background inwards
    When the property "topInset" of "l" is set to 4
    And the property "leftInset" of "l" is set to 10
    And the property "rightInset" of "l" is set to -5
    Then the property "x" of "bg" is 10
    And the property "y" of "bg" is 4
    And the property "width" of "bg" is 115
    And the property "height" of "bg" is 26

  Scenario: The background follows the size of the label
    When the property "width" of "l" is set to 200
    Then the property "width" of "bg" is 200

  Scenario: A label takes the font of the control it is in
    Then the property "font.pixelSize" of "inner" is 30
    And the property "font.bold" of "inner" is true

  Scenario: A font attribute of its own wins over the one it would take
    When the property "font.pixelSize" of "inner" is set to 12
    Then the property "font.pixelSize" of "inner" is 12
    And the property "font.bold" of "inner" is true

  Scenario: A label takes the text colour of the control it is in
    Then the property "color" of "inner" is "#aa0000"

  Scenario: A colour of its own wins
    When the property "color" of "inner" is set to "#0000ff"
    Then the property "color" of "inner" is "#0000ff"

  @api-QtQuick.Controls.Label.palette
  Scenario: The text colour comes from the palette of the label
    When the property "palette.windowText" of "plain" is set to "#008800"
    Then the property "color" of "plain" is "#008800"
