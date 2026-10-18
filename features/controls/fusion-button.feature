# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-button.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Button
Feature: The Fusion style's button colours
  A Fusion button is a shade darker than the palette's button colour, with
  a darker outline; it is darker still while it is pressed or checked. A
  highlighted button has an outline in the highlight colour and keeps its
  text colour, and one that is not enabled has a paler outline.

  Scenario Outline: A button's <property> when it is <object>
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        Button { objectName: "plain"; text: "Ab" }
        Button { objectName: "down"; text: "Ab"; down: true }
        Button { objectName: "checked"; text: "Ab"; checkable: true; checked: true }
        Button { objectName: "highlighted"; text: "Ab"; highlighted: true }
        Button { objectName: "off"; text: "Ab"; enabled: false }
      }
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object      | property                | value     |
      | plain       | background.color        | "#e8e8e8" |
      | plain       | background.border.color | "#ababab" |
      | plain       | contentItem.color       | "#000000" |
      | down        | background.color        | "#d3d3d3" |
      | down        | background.border.color | "#ababab" |
      | checked     | background.color        | "#d3d3d3" |
      | highlighted | background.border.color | "#26709e" |
      | highlighted | contentItem.color       | "#000000" |
      | off         | background.color        | "#e8e8e8" |
      | off         | background.border.color | "#c4c4c4" |
      | off         | contentItem.color       | "#bebebe" |
