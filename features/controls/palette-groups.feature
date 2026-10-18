# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-colorgroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-palette.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
# Verified on Qt 6.11.2 (qml6, offscreen), in the Basic and Fusion styles.

@qt-parity @api-QtQuick.Palette @api-QtQuick.ColorGroup
Feature: A palette's colour groups
  A palette has three groups of the same colours: "active", "inactive"
  and "disabled". What "palette.button" reads is the colour of the group
  the control is in now: the disabled one when it is not enabled, the
  active one otherwise. A colour set on the palette itself is set in
  every group; one set in a group is set there alone. The items inside
  take each colour of each group from the nearest item above that set it.

  Scenario: A control that is not enabled reads its disabled colours
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        Button { objectName: "on" }
        Button { objectName: "off"; enabled: false }
      }
      """
    Then the property "palette.highlight" of "off" is "#f0f6ff"
    And the property "palette.disabled.highlight" of "off" is "#f0f6ff"
    And the property "palette.active.highlight" of "off" is "#0066ff"
    And the property "palette.inactive.highlight" of "off" is "#0066ff"
    And the property "palette.active.buttonText" of "off" is "#26282a"
    And the property "palette.highlight" of "on" is "#0066ff"
    And the property "palette.disabled.highlight" of "on" is "#f0f6ff"
    And the property "palette.disabled.buttonText" of "on" is "#4d26282a"

  Scenario: A colour set on the palette is set in every group
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Button { objectName: "b"; palette.button: "green"; enabled: false }
      """
    Then the property "palette.button" of "b" is "#008000"
    And the property "palette.active.button" of "b" is "#008000"
    And the property "palette.inactive.button" of "b" is "#008000"
    And the property "palette.disabled.button" of "b" is "#008000"

  Scenario Outline: A colour set in the <group> group is set there alone
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Button { objectName: "b"; palette.<group>.button: "red" }
      """
    Then the property "palette.button" of "b" is "<now>"
    And the property "palette.active.button" of "b" is "<active>"
    And the property "palette.inactive.button" of "b" is "<inactive>"
    And the property "palette.disabled.button" of "b" is "<disabled>"
    When the property "enabled" of "b" is set to false
    Then the property "palette.button" of "b" is "<off>"

    Examples:
      | group    | now     | active  | inactive | disabled | off     |
      | active   | #ff0000 | #ff0000 | #e0e0e0  | #e0e0e0  | #e0e0e0 |
      | inactive | #e0e0e0 | #e0e0e0 | #ff0000  | #e0e0e0  | #e0e0e0 |
      | disabled | #e0e0e0 | #e0e0e0 | #e0e0e0  | #ff0000  | #ff0000 |

  Scenario: The items inside take a group's colour from above
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Pane {
        palette.disabled.text: "red"
        Column {
          Label { objectName: "on"; text: "On" }
          Label { objectName: "off"; text: "Off"; enabled: false }
          Item { palette.disabled.base: "#123456"; TextField { objectName: "field"; enabled: false } }
        }
      }
      """
    Then the property "palette.text" of "on" is "#353637"
    And the property "palette.disabled.text" of "on" is "#ff0000"
    And the property "palette.text" of "off" is "#ff0000"
    And the property "palette.base" of "field" is "#123456"
    And the property "palette.text" of "field" is "#ff0000"

  Scenario: A group's colour set from a script reaches the control
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        Button { objectName: "b"; enabled: false }
      }
      """
    When the property "palette.disabled.button" of "b" is set to "#ff0000"
    Then the property "palette.button" of "b" is "#ff0000"
    And the property "palette.active.button" of "b" is "#e0e0e0"

  @api-QtQuick.Controls.Switch
  Scenario: A Fusion switch that is not enabled fills in the active highlight
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Rectangle {
        width: 100; height: 40; color: "white"
        Switch { objectName: "switch"; enabled: false; checked: true }
      }
      """
    Then the property "palette.highlight" of "switch" is "#919191"
    And the property "palette.active.highlight" of "switch" is "#308cc6"
    And the pixel at 16, 14 of the window is within 12 of "#359bdb"
