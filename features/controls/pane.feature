# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Pane
  A pane is a background in the colour of the window with padding around
  what is put into it. It is as large as its content needs.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Pane { objectName: "empty" }
        Pane {
          objectName: "p"
          y: 50
          Rectangle { objectName: "kid"; implicitWidth: 30; implicitHeight: 20 }
        }
        Pane {
          objectName: "two"
          y: 120
          Rectangle { implicitWidth: 30; implicitHeight: 20 }
          Rectangle { x: 40; implicitWidth: 50; implicitHeight: 10 }
        }
      }
      """

  Scenario Outline: An empty pane is as large as its padding
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property              | value     |
      | implicitWidth         | 24        |
      | implicitHeight        | 24        |
      | padding               | 12        |
      | contentWidth          | 0         |
      | contentHeight         | 0         |
      | background.color      | "#ffffff" |
      | background.width      | 24        |
      | contentChildren.length | 0        |

  @api-QtQuick.Controls.Pane.contentWidth @api-QtQuick.Controls.Pane.contentHeight
  Scenario Outline: A pane with one item is as large as the item and the padding
    Then the property "<property>" of "p" is <value>

    Examples:
      | property               | value |
      | contentWidth           | 30    |
      | contentHeight          | 20    |
      | implicitContentWidth   | 30    |
      | implicitContentHeight  | 20    |
      | implicitWidth          | 54    |
      | implicitHeight         | 44    |
      | width                  | 54    |
      | contentItem.x          | 12    |
      | contentItem.y          | 12    |
      | contentItem.width      | 30    |
      | contentItem.height     | 20    |
      | contentChildren.length | 1     |

  @api-QtQuick.Controls.Pane.contentChildren
  Scenario: What is put into a pane becomes a child of its content item
    Then the property "parent.parent.objectName" of "kid" is "p"
    And the property "x" of "kid" is 0

  Scenario: The pane follows the size of its item
    When the property "implicitWidth" of "kid" is set to 100
    Then the property "contentWidth" of "p" is 100
    And the property "implicitWidth" of "p" is 124

  Scenario: With several items the pane cannot tell the size of its content
    Then the property "contentWidth" of "two" is 0
    And the property "implicitWidth" of "two" is 24
    And the property "contentChildren.length" of "two" is 2

  Scenario: The application can say how large the content is
    When the property "contentWidth" of "two" is set to 90
    And the property "contentHeight" of "two" is set to 20
    Then the property "implicitWidth" of "two" is 114
    And the property "implicitHeight" of "two" is 44

  Scenario: A larger pane gives the room to its content item
    When the property "width" of "p" is set to 200
    And the property "height" of "p" is set to 100
    Then the property "contentItem.width" of "p" is 176
    And the property "contentItem.height" of "p" is 76
    And the property "background.width" of "p" is 200
    And the property "availableWidth" of "p" is 176

  Scenario: The background follows the palette of the pane
    When the property "palette.window" of "p" is set to "#112233"
    Then the property "background.color" of "p" is "#112233"
