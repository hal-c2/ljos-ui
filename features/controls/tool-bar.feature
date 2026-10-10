# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbar.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ToolBar
  A tool bar is a pane for the top or bottom of a window, as high as a tool
  button and without padding.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        ToolBar { objectName: "empty" }
        ToolBar {
          objectName: "bar"
          y: 60
          Row {
            objectName: "row"
            ToolButton { objectName: "back" }
            ToolButton { objectName: "more" }
          }
        }
      }
      """

  @api-QtQuick.Controls.ToolBar.position
  Scenario Outline: An empty tool bar has a height and no width
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property                 | value     |
      | implicitWidth            | 0         |
      | implicitHeight           | 40        |
      | implicitBackgroundHeight | 40        |
      | padding                  | 0         |
      | position                 | 0         |
      | background.color         | "#e0e0e0" |

  Scenario Outline: A tool bar is as wide as what is in it
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property                 | value |
      | bar    | contentWidth             | 80    |
      | bar    | contentHeight            | 40    |
      | bar    | implicitWidth            | 80    |
      | bar    | implicitHeight           | 40    |
      | bar    | contentItem.x            | 0     |
      | bar    | background.width         | 80    |
      | more   | x                        | 40    |
      | row    | parent.parent.objectName | "bar" |

  Scenario Outline: The positions have names
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "names"
        property int header: ToolBar.Header
        property int footer: ToolBar.Footer
      }
      """
    Then the property "<property>" of "names" is <value>

    Examples:
      | property | value |
      | header   | 0     |
      | footer   | 1     |

  Scenario: The background follows the palette of the tool bar
    When the property "palette.button" of "bar" is set to "#112233"
    Then the property "background.color" of "bar" is "#112233"
