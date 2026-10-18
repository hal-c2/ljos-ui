# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
#
# On qml6 a plain item reads the platform's colour where nobody above it
# set one (#efefef for button under offscreen); Ljós gives it the style's
# (no scenario: the platform's differ from one system to another).

@qt-parity @api-QtQuick.Item.palette
Feature: The palette of a plain item
  Any item has a palette. A colour set on a plain item, a `Row` or an `Item`, reaches the controls
  inside it, as one set on a control does.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Row {
          objectName: "row"
          palette.highlight: "#2CDE85"
          palette.button: "#2CDE85"
          function back() { palette.highlight = undefined }
          Button { objectName: "b"; text: "B" }
        }
        Item {
          objectName: "outer"
          y: 50
          palette.button: "red"
          Item { objectName: "inner"; Button { objectName: "deep" } }
        }
        Item {
          objectName: "late"
          y: 100
          Item { Button { objectName: "lateb" } }
        }
        Button { objectName: "free"; y: 150 }
      }
      """

  Scenario Outline: A colour set on a plain item reaches the controls inside it
    Then the property "<property>" of "<object>" is "<colour>"

    Examples:
      | object | property           | colour  |
      | row    | palette.highlight  | #2cde85 |
      | b      | palette.highlight  | #2cde85 |
      | b      | palette.button     | #2cde85 |
      | b      | palette.buttonText | #26282a |
      | b      | background.color   | #2cde85 |
      | deep   | palette.button     | #ff0000 |
      | free   | palette.button     | #e0e0e0 |

  # A plain item's colours are found when a control inside it resolves its
  # own; reading them on the item itself would cost a check on every read.
  @backlog
  Scenario: A plain item reads the colours of the items it is in
    Then the property "palette.button" of "inner" is "#ff0000"
    When the property "palette.button" of "outer" is set to "#0000ff"
    Then the property "palette.button" of "inner" is "#0000ff"

  Scenario: A later change reaches the controls inside
    When the property "palette.button" of "outer" is set to "#0000ff"
    Then the property "palette.button" of "deep" is "#0000ff"
    And the property "background.color" of "deep" is "#0000ff"

  Scenario: A colour first set after the item completed reaches the controls inside
    When the property "palette.button" of "late" is set to "#0000ff"
    Then the property "palette.button" of "lateb" is "#0000ff"
    And the property "background.color" of "lateb" is "#0000ff"

  Scenario: A colour taken back is the style's again inside
    When the method "back" of "row" is called
    Then the property "palette.highlight" of "b" is "#0066ff"
    And the property "palette.button" of "b" is "#2cde85"
