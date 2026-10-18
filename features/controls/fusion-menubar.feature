# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubaritem.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.MenuBar @api-QtQuick.Controls.MenuBarItem
Feature: The Fusion style's menu bar
  A Fusion menu bar is as tall as its items, in the window's colour, and its
  items name their menus in the text colour. An item is marked in the
  highlight colour only while it is highlighted.

  Background:
    Given the file "Bars.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Column {
        width: 200
        MenuBar {
          objectName: "bar"
          MenuBarItem { objectName: "item"; menu: Menu { title: "File" } }
        }
        MenuBar {
          objectName: "dark"
          leftPadding: 10
          topPadding: 10
          palette.window: "transparent"
          palette.text: "#ffffff"
          palette.highlight: "#112233"
          palette.highlightedText: "#00ff00"
          MenuBarItem { objectName: "plain"; menu: Menu { title: "File" } }
          MenuBarItem { objectName: "marked"; highlighted: true; menu: Menu { title: "Edit" } }
        }
      }
      """

  Scenario Outline: A menu bar and its items have the style's metrics and colours
    Given the QML:
      """qml
      import QtQuick
      Bars {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property              | value       |
      | bar    | implicitHeight        | 26          |
      | bar    | padding               | 0           |
      | bar    | spacing               | 0           |
      | item   | implicitHeight        | 26          |
      | item   | padding               | 6           |
      | item   | leftPadding           | 6           |
      | item   | background.visible    | false       |
      | dark   | implicitHeight        | 36          |
      | dark   | background.color      | "#00000000" |
      | plain  | contentItem.color     | "#ffffff"   |
      | plain  | background.visible    | false       |
      | marked | contentItem.color     | "#00ff00"   |
      | marked | background.visible    | true        |
      | marked | background.color      | "#112233"   |

  # Fusion draws in the platform's palette and not in Basic's; Ljós has one
  # palette for every style.
  @backlog
  Scenario: A Fusion bar that sets no colours is in the platform's palette
    Given the QML:
      """qml
      import QtQuick
      Bars {}
      """
    Then the property "background.color" of "bar" is "#efefef"
    And the property "contentItem.color" of "item" is "#000000"
