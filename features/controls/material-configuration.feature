# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-configuration.html
#   https://doc.qt.io/qt-6/qtquickcontrols-environment.html
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
# Verified on Qt 6.11.2 (qml6, offscreen, QT_QUICK_CONTROLS_CONF set): a
# colour named in the file is the Material colour of that name, in shade
# 200 for the accent and the background of the dark theme and in shade 500
# otherwise; a variable comes before the file.

@qt-parity @api-QtQuick.Controls.Material.theme @api-QtQuick.Controls.Material.primary
@api-QtQuick.Controls.Material.accent @api-QtQuick.Controls.Material.foreground
@api-QtQuick.Controls.Material.background
Feature: The Material style's colours when the application starts
  The [Material] group of the controls configuration file, and the
  QT_QUICK_CONTROLS_MATERIAL_ variables, give the theme and the colours an
  application starts with. They are what an item has that neither it nor
  an item it is in sets.

  Background:
    Given the file "Items.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        Item {
          objectName: "plain"
          property int theme: Material.theme
          property color primary: Material.primary
          property color accent: Material.accent
          property color foreground: Material.foreground
          property color background: Material.background
        }
        Item {
          Material.accent: Material.Purple
          Item { objectName: "inside"; property color accent: Material.accent }
        }
      }
      """

  Scenario Outline: The file gives the <property>
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Material

      [Material]
      Theme=Dark
      Primary=Green
      Accent=#123456
      Foreground=Red
      Background=Yellow
      """
    And the QML:
      """qml
      import QtQuick
      Items {}
      """
    Then the property "<property>" of "plain" is <value>

    Examples:
      | property   | value     |
      | theme      | 1         |
      | primary    | "#4caf50" |
      | accent     | "#123456" |
      | foreground | "#f44336" |
      | background | "#fff59d" |

  Scenario: A colour named in the file is in the shade of the theme
    Given the application starts with the controls configuration:
      """
      [Material]
      Accent=Teal
      Background=Yellow
      """
    And the QML:
      """qml
      import QtQuick
      Items {}
      """
    Then the property "theme" of "plain" is 0
    And the property "accent" of "plain" is "#009688"
    And the property "background" of "plain" is "#ffeb3b"

  Scenario: An item that is in one that sets a colour has that colour
    Given the application starts with the controls configuration:
      """
      [Material]
      Accent=Teal
      """
    And the QML:
      """qml
      import QtQuick
      Items {}
      """
    Then the property "accent" of "inside" is "#9c27b0"

  Scenario: A variable comes before the file
    Given the application starts with the controls configuration:
      """
      [Material]
      Theme=Light
      Accent=#123456
      """
    And the application starts with "QT_QUICK_CONTROLS_MATERIAL_THEME" set to "Dark"
    And the application starts with "QT_QUICK_CONTROLS_MATERIAL_ACCENT" set to "Teal"
    And the QML:
      """qml
      import QtQuick
      Items {}
      """
    Then the property "theme" of "plain" is 1
    And the property "accent" of "plain" is "#80cbc4"
