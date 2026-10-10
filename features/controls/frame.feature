# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-frame.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Frame
  A frame is a pane with a line around it instead of a filled background.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Frame { objectName: "empty" }
        Frame {
          objectName: "f"
          y: 50
          Rectangle { objectName: "kid"; implicitWidth: 30; implicitHeight: 20 }
        }
      }
      """

  Scenario Outline: An empty frame is as large as its padding
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property                | value       |
      | implicitWidth           | 24          |
      | implicitHeight          | 24          |
      | padding                 | 12          |
      | background.color        | "#00000000" |
      | background.border.color | "#bdbdbd"   |
      | background.border.width | 1           |

  Scenario Outline: A frame with one item is as large as the item and the padding
    Then the property "<property>" of "f" is <value>

    Examples:
      | property          | value |
      | contentWidth      | 30    |
      | contentHeight     | 20    |
      | implicitWidth     | 54    |
      | implicitHeight    | 44    |
      | contentItem.x     | 12    |
      | contentItem.y     | 12    |
      | background.width  | 54    |
      | background.height | 44    |

  Scenario: What is put into a frame becomes a child of its content item
    Then the property "parent.parent.objectName" of "kid" is "f"

  Scenario: The line follows the palette of the frame
    When the property "palette.mid" of "f" is set to "#112233"
    Then the property "background.border.color" of "f" is "#112233"
