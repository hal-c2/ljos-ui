# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#font-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.font
Feature: The font of a control
  A control takes its font from the control or window it is in. An attribute
  it sets itself stays; the others go on following.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Pane {
          objectName: "outer"
          font.pixelSize: 30
          Column {
            Button { objectName: "b"; text: "B"; font.bold: true }
            Label { objectName: "l" }
            Text { objectName: "plain" }
            Pane {
              objectName: "inner"
              font.italic: true
              Label { objectName: "deep" }
            }
          }
        }
        Button { objectName: "free"; y: 150 }
      }
      """

  Scenario Outline: Controls and labels take the font of the control they are in
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property       | value |
      | b      | font.pixelSize | 30    |
      | b      | font.bold      | true  |
      | b      | font.italic    | false |
      | l      | font.pixelSize | 30    |
      | l      | font.bold      | false |
      | inner  | font.pixelSize | 30    |
      | inner  | font.italic    | true  |
      | deep   | font.pixelSize | 30    |
      | deep   | font.italic    | true  |
      | outer  | font.italic    | false |
      | free   | font.bold      | false |
      | free   | font.italic    | false |

  Scenario: A control outside takes nothing
    Then the property "font.pixelSize" of "free" is not 30

  Scenario: A plain text does not take the font
    Then the property "font.pixelSize" of "plain" is not 30

  Scenario: The parts of a control are drawn with its font
    Then the property "contentItem.font.pixelSize" of "outer/b" is at least 30

  Scenario: A later change reaches everything inside
    When the property "font.pixelSize" of "outer" is set to 40
    Then the property "font.pixelSize" of "b" is 40
    And the property "font.pixelSize" of "l" is 40
    And the property "font.pixelSize" of "deep" is 40

  Scenario: An attribute a control set itself stays
    When the property "font.pixelSize" of "b" is set to 12
    And the property "font.pixelSize" of "outer" is set to 40
    Then the property "font.pixelSize" of "b" is 12
    And the property "font.bold" of "b" is true

  Scenario: The other attributes go on following
    When the property "font.pixelSize" of "b" is set to 12
    And the property "font.italic" of "outer" is set to true
    Then the property "font.italic" of "b" is true
    And the property "font.pixelSize" of "b" is 12

  Scenario: An attribute set in between reaches only what is inside
    When the property "font.underline" of "inner" is set to true
    Then the property "font.underline" of "deep" is true
    And the property "font.underline" of "b" is false

  Scenario: A larger font makes a button larger
    When the property "font.pixelSize" of "outer" is set to 60
    Then the property "implicitHeight" of "b" is at least 60
