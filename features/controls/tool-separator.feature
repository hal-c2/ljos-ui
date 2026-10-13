# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolseparator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: ToolSeparator
  A tool separator is a thin line between groups of tools in a tool bar:
  upright by default, lying down when it is `Qt.Horizontal`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        ToolSeparator { objectName: "v" }
        ToolSeparator { objectName: "h"; y: 50; orientation: Qt.Horizontal }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: An upright separator is a short line with room around it
      Then the property "<property>" of "v" is <value>

      Examples:
        | property           | value     |
        | orientation        | 2         |
        | vertical           | true      |
        | horizontal         | false     |
        | implicitWidth      | 13        |
        | implicitHeight     | 34        |
        | leftPadding        | 6         |
        | topPadding         | 2         |
        | focusPolicy        | 0         |
        | activeFocusOnTab   | false     |
        | contentItem.x      | 6         |
        | contentItem.y      | 2         |
        | contentItem.width  | 1         |
        | contentItem.height | 30        |
        | contentItem.color  | "#bdbdbd" |

    Scenario Outline: A horizontal separator lies down
      Then the property "<property>" of "h" is <value>

      Examples:
        | property           | value |
        | horizontal         | true  |
        | vertical           | false |
        | implicitWidth      | 34    |
        | implicitHeight     | 13    |
        | contentItem.x      | 2     |
        | contentItem.y      | 6     |
        | contentItem.width  | 30    |
        | contentItem.height | 1     |
