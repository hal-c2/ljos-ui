# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Dialog @api-QtQuick.Controls.Menu
@api-QtQuick.Controls.ToolTip
Feature: Fusion's menus and tool tips cast a shadow
  In the Fusion style a menu and a tool tip are drawn over a faint shadow
  in the palette's shadow colour, a pixel down and to the right of them;
  a tool tip's is darker. A dialog has one too, but a pixel smaller each
  way, so it stays hidden behind it. A plain popup has none.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """

  Scenario Outline: A <popup>'s shadow
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 240; height: 120
        <popup> {
          objectName: "popup"
          x: 10; y: 10; width: 100; height: 60; visible: true
          <content>
        }
      }
      """
    Then the property "background.children.0.x" of "popup" is 1
    And the property "background.children.0.y" of "popup" is 1
    And the property "background.children.0.width" of "popup" is <width>
    And the property "background.children.0.height" of "popup" is <height>
    And the property "background.children.0.z" of "popup" is -1
    And the property "background.children.0.color" of "popup" is "#767676"
    And the property "background.children.0.opacity" of "popup" is <opacity>

    Examples:
      | popup   | content                   | width | height | opacity |
      | Dialog  |                           | 98    | 58     | 0.2     |
      | Menu    | MenuItem { text: "Item" } | 100   | 60     | 0.2     |
      | ToolTip | text: "Tip"               | 100   | 60     | 0.5     |

  Scenario: A plain popup has none
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Popup { objectName: "popup"; width: 100; height: 60; visible: true }
      }
      """
    Then the property "background.children.length" of "popup" is 0
